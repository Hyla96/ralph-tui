use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, enable_raw_mode};
use ratatui::DefaultTerminal;
use std::io::stdout;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{Receiver, UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;
use vt100::Parser as VtParser;

use crate::ralph::runner::RunnerEvent;
use crate::ralph::store::Store;
use crate::ralph::watcher::{Watcher, WatcherEvent};
use crate::ralph::workflow::Workflow;

// Maximum number of ralph loop iterations before the loop stops automatically.
// TODO: make configurable
const MAX_ITERATIONS: u32 = 10;

/// Per-runner tab state.
pub enum RunnerTabState {
    Running { iteration: u32 },
    Done,
    Error(String),
}

/// Holds all state for a single runner tab.
pub struct RunnerTab {
    pub workflow_name: String,
    /// VT100 terminal emulator that processes raw PTY bytes.
    /// Scrollback is set to 1000 rows. Dimensions kept in sync with the terminal.
    pub parser: VtParser,
    pub state: RunnerTabState,
    pub runner_rx: Option<UnboundedReceiver<RunnerEvent>>,
    pub runner_kill_tx: Option<oneshot::Sender<()>>,
    /// Sender used to deliver raw bytes to the PTY stdin.
    pub stdin_tx: Option<UnboundedSender<Vec<u8>>>,
    /// Scroll offset for the log view (0 = auto-scroll to bottom).
    pub log_scroll: usize,
    /// When true the runner automatically spawns the next iteration on completion
    /// without showing the ContinuePrompt dialog.
    pub auto_continue: bool,
    /// ID of the task currently being executed (populated from `next_task()` before spawn).
    pub current_task_id: Option<String>,
    /// Title of the task currently being executed.
    pub current_task_title: Option<String>,
    /// Number of iterations used for this runner tab (starts at 1, incremented by spawn_next_iteration).
    pub iterations_used: u32,
}

pub enum Dialog {
    NewWorkflow { input: String, error: Option<String> },
    DeleteWorkflow { name: String },
    ContinuePrompt { next_id: String, next_title: String },
    Help,
}

/// Spawns `claude --agent ralph` inside a PTY and streams output lines back via `tx`.
/// Listens on `kill_rx` for an early termination signal.
/// Lines received on `stdin_rx` are forwarded (with a trailing `\n`) to the PTY stdin.
async fn runner_task(
    plan_dir: PathBuf,
    repo_root: PathBuf,
    tx: UnboundedSender<RunnerEvent>,
    kill_rx: oneshot::Receiver<()>,
    mut stdin_rx: UnboundedReceiver<Vec<u8>>,
    size: (u16, u16),
    resize_rx: UnboundedReceiver<(u16, u16)>,
) {
    use portable_pty::{CommandBuilder, PtySize, native_pty_system};

    // Send startup info through the channel so it appears in the vt100 screen, not stderr.
    let _ = tx.send(RunnerEvent::Bytes(
        format!(
            "[runner] spawning claude in {}\r\n[runner] RALPH_PLAN_DIR={}\r\n",
            repo_root.display(),
            plan_dir.display()
        )
        .into_bytes(),
    ));

    let (cols, rows) = size;
    let pty_system = native_pty_system();
    let pair = match pty_system.openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }) {
        Ok(p) => p,
        Err(e) => {
            let _ = tx.send(RunnerEvent::SpawnError(format!("PTY open failed: {e}")));
            return;
        }
    };

    let mut cmd = CommandBuilder::new("claude");
    cmd.args(["--agent", "ralph", "Implement the next task."]);
    cmd.cwd(&repo_root);
    cmd.env("RALPH_PLAN_DIR", &plan_dir);

    let mut child = match pair.slave.spawn_command(cmd) {
        Ok(c) => c,
        Err(e) => {
            let msg = e.to_string();
            let friendly = if msg.contains("No such file") || msg.contains("not found") {
                "claude not found on PATH".to_string()
            } else {
                msg
            };
            let _ = tx.send(RunnerEvent::SpawnError(friendly));
            return;
        }
    };

    let _ = tx.send(RunnerEvent::Bytes(
        format!("[runner] spawned claude pid={:?}\r\n", child.process_id()).into_bytes(),
    ));

    // Clone the killer handle so we can signal the child from the select arm
    // without holding the child borrow (which is blocked in wait()).
    let mut killer = child.clone_killer();

    // Drop the slave end in the parent so EOF propagates to the master when
    // the child process exits and closes its inherited slave fd.
    drop(pair.slave);

    // Extract master so it can be moved into the resize task after reader/writer are taken.
    let master = pair.master;

    let reader = match master.try_clone_reader() {
        Ok(r) => r,
        Err(e) => {
            let _ = tx.send(RunnerEvent::SpawnError(format!("PTY reader: {e}")));
            return;
        }
    };
    let mut writer = match master.take_writer() {
        Ok(w) => w,
        Err(e) => {
            let _ = tx.send(RunnerEvent::SpawnError(format!("PTY writer: {e}")));
            return;
        }
    };

    // Read PTY output in a blocking thread: send raw 4096-byte chunks as Bytes events.
    // <promise>COMPLETE</promise> is detected by scanning the chunk as lossy UTF-8.
    let tx_read = tx.clone();
    let read_handle = tokio::task::spawn_blocking(move || {
        use std::io::Read;
        let mut buf = [0u8; 4096];
        let mut reader = reader;
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let chunk = &buf[..n];
                    if String::from_utf8_lossy(chunk).contains("<promise>COMPLETE</promise>") {
                        let _ = tx_read.send(RunnerEvent::Complete);
                    }
                    if tx_read.send(RunnerEvent::Bytes(chunk.to_vec())).is_err() {
                        break;
                    }
                }
            }
        }
    });

    // Forward stdin_rx bytes to PTY stdin. Writes are small and infrequent
    // (user keyboard input), so a brief sync write in the async task is acceptable.
    drop(tokio::spawn(async move {
        use std::io::Write;
        while let Some(bytes) = stdin_rx.recv().await {
            if writer.write_all(&bytes).is_err() {
                break;
            }
        }
    }));

    // Forward resize events to the PTY master. master is moved here after
    // reader/writer are already extracted.
    drop(tokio::spawn(async move {
        use portable_pty::PtySize;
        let mut resize_rx = resize_rx;
        while let Some((cols, rows)) = resize_rx.recv().await {
            let _ = master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 });
        }
    }));

    // Wait for the child to exit in a blocking task; send the exit code via oneshot.
    let (done_tx, done_rx) = tokio::sync::oneshot::channel::<u32>();
    tokio::task::spawn_blocking(move || {
        let code = child.wait().map(|s| if s.success() { 0u32 } else { 1u32 }).unwrap_or(1u32);
        let _ = done_tx.send(code);
    });

    let (was_killed, exit_code) = tokio::select! {
        result = done_rx => (false, Some(result.unwrap_or(1))),
        _ = kill_rx => (true, None),
    };

    if was_killed {
        let _ = killer.kill();
    }

    // Wait for the reader to drain all remaining PTY output before sending Exited.
    let _ = read_handle.await;
    // None = killed, Some(n) = natural exit with code n.
    let _ = tx.send(RunnerEvent::Exited(exit_code));
}

/// Maps a crossterm `KeyEvent` to the raw bytes that should be sent to the PTY.
///
/// Returns `None` for keys that have no meaningful PTY representation (function
/// keys, media keys, etc.).  The caller is responsible for intercepting keys
/// that should NOT be forwarded (t, q, s, x, k/Up, j/Down, G/End, Ctrl+C).
fn key_to_pty_bytes(key: KeyEvent) -> Option<Vec<u8>> {
    // Ctrl+letter → control byte (Ctrl+A = 1 … Ctrl+Z = 26).
    if key.modifiers.contains(KeyModifiers::CONTROL)
        && let KeyCode::Char(c) = key.code
    {
        let lower = c.to_ascii_lowercase();
        if lower.is_ascii_alphabetic() {
            return Some(vec![lower as u8 - b'a' + 1]);
        }
    }

    match key.code {
        KeyCode::Char(c) => {
            let mut buf = [0u8; 4];
            Some(c.encode_utf8(&mut buf).as_bytes().to_vec())
        }
        KeyCode::Enter => Some(vec![b'\r']),
        KeyCode::Backspace => Some(vec![b'\x7f']),
        KeyCode::Tab => Some(vec![b'\t']),
        KeyCode::Esc => Some(vec![b'\x1b']),
        KeyCode::Up => Some(b"\x1b[A".to_vec()),
        KeyCode::Down => Some(b"\x1b[B".to_vec()),
        KeyCode::Right => Some(b"\x1b[C".to_vec()),
        KeyCode::Left => Some(b"\x1b[D".to_vec()),
        _ => None,
    }
}

pub struct App {
    pub running: bool,
    pub store: Store,
    pub workflows: Vec<String>,
    pub selected_workflow: Option<usize>,
    pub current_workflow: Option<Workflow>,
    /// All open runner tabs (tab 0 is the Workflows tab, not stored here).
    pub runner_tabs: Vec<RunnerTab>,
    /// 0 = Workflows tab; 1..=runner_tabs.len() = runner tab at index active_tab-1.
    pub active_tab: usize,
    /// When true the next keypress is interpreted as a tab navigation chord.
    pub tab_nav_pending: bool,
    pub dialog: Option<Dialog>,
    pub status_message: Option<String>,
    pub status_message_expires: Option<Instant>,
    /// Terminal size at startup; used as initial PTY size for runner tasks.
    pub initial_size: (u16, u16),
    /// One sender per active runner task; used to propagate terminal resize events.
    /// Dead senders (task exited) are pruned lazily when the next resize event arrives.
    pub resize_txs: Vec<UnboundedSender<(u16, u16)>>,
    /// Receives file-change notifications from the OS-native watcher.
    /// `None` when the watcher failed to start.
    pub watcher_rx: Option<Receiver<WatcherEvent>>,
    /// Keeps the OS watcher alive. Dropping this stops watching.
    pub _watcher: Option<Watcher>,
    /// Transient notification set after a watcher-triggered reload.
    /// Tuple is (message_text, time_set). Cleared after 3 seconds.
    pub notification: Option<(String, Instant)>,
}

impl App {
    pub fn new(store: Store, initial_size: (u16, u16)) -> Self {
        let workflows = store.list_workflows();
        let selected_workflow = if workflows.is_empty() { None } else { Some(0) };

        // Capture the root path before `store` is moved into the App struct.
        let root = store.root().to_path_buf();

        // Start OS-native file watcher. Gracefully degrade if it fails.
        let (watcher_tx, watcher_rx) = tokio::sync::mpsc::channel::<WatcherEvent>(64);
        let (watcher_opt, watcher_rx_opt, watcher_warning) =
            match Watcher::start(&root, watcher_tx) {
                Ok(w) => (Some(w), Some(watcher_rx), None),
                Err(e) => (None, None, Some(format!("file watcher unavailable: {e}"))),
            };

        let mut app = App {
            running: true,
            store,
            workflows,
            selected_workflow,
            current_workflow: None,
            runner_tabs: Vec::new(),
            active_tab: 0,
            tab_nav_pending: false,
            dialog: None,
            status_message: watcher_warning,
            status_message_expires: None,
            initial_size,
            resize_txs: Vec::new(),
            watcher_rx: watcher_rx_opt,
            _watcher: watcher_opt,
            notification: None,
        };
        app.load_current_workflow();
        app
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while self.running {
            self.check_status_timeout();
            self.drain_runner_channels();
            let watcher_events = self.drain_watcher_channel();
            if !watcher_events.is_empty() {
                let first_path = watcher_events.first().map(|e| e.path.clone());
                self.reload_all();
                if let Some(path) = first_path {
                    let root = self.store.root().to_path_buf();
                    let rel =
                        path.strip_prefix(&root).map(|p| p.to_path_buf()).unwrap_or(path);
                    self.notification = Some((
                        format!("↻ {} reloaded", rel.display()),
                        Instant::now(),
                    ));
                }
            }
            if let Err(e) = terminal.draw(|frame| crate::ui::draw(frame, self)) {
                self.display_error(e.to_string());
            }
            if let Err(e) = self.handle_events(terminal) {
                self.display_error(e.to_string());
            }
        }
        Ok(())
    }

    /// Truncates `msg` to 80 chars and shows it in the status bar.
    fn display_error(&mut self, msg: String) {
        let truncated: String = msg.chars().take(80).collect();
        self.status_message = Some(truncated);
    }

    fn handle_events(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        if !event::poll(Duration::from_millis(100))? {
            return Ok(());
        }
        match event::read()? {
            Event::Resize(cols, rows) => {
                // Broadcast new size to all active PTY runners; prune dead senders.
                self.resize_txs.retain(|tx| tx.send((cols, rows)).is_ok());
                // Recreate each RunnerTab's vt100::Parser with the new dimensions.
                // vt100::Parser has no resize() method, so a new parser is created.
                // Known limitation: the screen state is cleared on resize — scrollback is not replayed.
                for tab in &mut self.runner_tabs {
                    tab.parser = VtParser::new(rows, cols, 1000);
                    tab.log_scroll = 0;
                }
                self.initial_size = (cols, rows);
            }
            Event::Key(key) => {
                #[allow(clippy::collapsible_else_if)]
                if self.dialog.is_some() {
                    self.handle_dialog_key(key.code);
                } else if self.tab_nav_pending {
                    // Consume the chord: always clear the flag, then act.
                    self.tab_nav_pending = false;
                    self.handle_tab_nav_key(key.code);
                } else if self.active_tab == 0 {
                // Workflows tab keybindings.
                match key.code {
                    KeyCode::Char('t') => self.tab_nav_pending = true,
                    KeyCode::Char('q') => self.running = false,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.running = false;
                    }
                    KeyCode::Up | KeyCode::Char('k') => self.move_up(),
                    KeyCode::Down | KeyCode::Char('j') => self.move_down(),
                    KeyCode::Char('r') => self.start_runner(),
                    KeyCode::Char('s') => self.stop_runner(),
                    KeyCode::Char('n') => self.open_new_workflow_dialog(),
                    KeyCode::Char('e') => self.edit_current_plan(terminal)?,
                    KeyCode::Char('d') => self.open_delete_workflow_dialog(),
                    KeyCode::Char('?') => self.open_help_dialog(),
                    _ => {}
                }
            } else {
                // Runner tab keybindings.
                // Keys NOT forwarded to PTY: t, q, Ctrl+C, s, x, a, k/Up, j/Down, G/End.
                // All other keys are forwarded as raw bytes via key_to_pty_bytes.
                match key.code {
                    KeyCode::Char('t') => self.tab_nav_pending = true,
                    KeyCode::Char('q') => self.running = false,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.running = false;
                    }
                    KeyCode::Char('s') => self.stop_runner(),
                    KeyCode::Char('a') => {
                        let tab_idx = self.active_tab - 1;
                        if let Some(tab) = self.runner_tabs.get_mut(tab_idx) {
                            tab.auto_continue = !tab.auto_continue;
                            let msg = if tab.auto_continue {
                                "Auto-continue ON".to_string()
                            } else {
                                "Auto-continue OFF".to_string()
                            };
                            self.status_message = Some(msg);
                            self.status_message_expires =
                                Some(Instant::now() + Duration::from_secs(2));
                        }
                    }
                    // Close a Done/Error runner tab; refuse if still Running.
                    KeyCode::Char('x') => {
                        let tab_idx = self.active_tab - 1;
                        let is_running = self
                            .runner_tabs
                            .get(tab_idx)
                            .map(|t| matches!(t.state, RunnerTabState::Running { .. }))
                            .unwrap_or(false);
                        if is_running {
                            self.status_message =
                                Some("Stop the runner first [s]".to_string());
                            self.status_message_expires =
                                Some(Instant::now() + Duration::from_secs(2));
                        } else if self.runner_tabs.get(tab_idx).is_some() {
                            self.runner_tabs.remove(tab_idx);
                            // Move to the previous tab; saturating_sub(1) gives 0 (Workflows)
                            // when active_tab was 1 (the only runner tab).
                            self.active_tab = self.active_tab.saturating_sub(1);
                        }
                    }
                    // Log scroll: Up/k scroll up (into scrollback), Down/j scroll down.
                    // log_scroll == 0 means auto-scroll (live vt100 screen).
                    // log_scroll == N means N rows of scrollback are shown above the screen.
                    // The scrollback position is kept in sync on the vt100 parser's screen so
                    // that PseudoTerminal renders the correct view without needing &mut in draw.
                    KeyCode::Up | KeyCode::Char('k') => {
                        let tab_idx = self.active_tab - 1;
                        if let Some(tab) = self.runner_tabs.get_mut(tab_idx) {
                            // Cap at the configured scrollback size (1000 rows).
                            tab.log_scroll = (tab.log_scroll + 1).min(1000);
                            tab.parser.screen_mut().set_scrollback(tab.log_scroll);
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        let tab_idx = self.active_tab - 1;
                        if let Some(tab) = self.runner_tabs.get_mut(tab_idx) {
                            tab.log_scroll = tab.log_scroll.saturating_sub(1);
                            tab.parser.screen_mut().set_scrollback(tab.log_scroll);
                        }
                    }
                    // End or G re-enables auto-scroll (live screen, scrollback = 0).
                    KeyCode::End | KeyCode::Char('G') => {
                        let tab_idx = self.active_tab - 1;
                        if let Some(tab) = self.runner_tabs.get_mut(tab_idx) {
                            tab.log_scroll = 0;
                            tab.parser.screen_mut().set_scrollback(0);
                        }
                    }
                    // All other keys are forwarded directly to the PTY as raw bytes.
                    _ => {
                        if let Some(bytes) = key_to_pty_bytes(key) {
                            let tab_idx = self.active_tab - 1;
                            if let Some(tab) = self.runner_tabs.get(tab_idx)
                                && let Some(tx) = &tab.stdin_tx
                            {
                                let _ = tx.send(bytes);
                            }
                        }
                    }
                }
            }   // closes else { block
            }   // closes Event::Key(key) => { arm body
            _ => {} // other events (mouse, focus, paste, …) are ignored
        }       // closes match event::read()?
        Ok(())
    }

    /// Handles the second key of a `t`-prefix tab navigation chord.
    ///
    /// Digits `1`–`9` jump to the tab at index `digit − 1` (0 = Workflows).
    /// `Left`/`Right` cycle through all tabs with wrapping.
    /// Any other key is silently ignored (flag was already cleared by the caller).
    fn handle_tab_nav_key(&mut self, code: KeyCode) {
        let total_tabs = 1 + self.runner_tabs.len(); // Workflows tab + runner tabs
        match code {
            KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
                let idx = (c as usize) - ('1' as usize); // digit '1' → 0, '9' → 8
                if idx < total_tabs {
                    self.active_tab = idx;
                }
            }
            KeyCode::Left => {
                self.active_tab = if self.active_tab == 0 {
                    total_tabs - 1
                } else {
                    self.active_tab - 1
                };
            }
            KeyCode::Right => {
                self.active_tab = (self.active_tab + 1) % total_tabs;
            }
            _ => {} // any other key: flag already cleared, no tab change
        }
    }

    fn edit_current_plan(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let Some(idx) = self.selected_workflow else {
            return Ok(());
        };
        let Some(name) = self.workflows.get(idx).cloned() else {
            return Ok(());
        };

        let prd_path = self.store.workflow_dir(&name).join("prd.json");
        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

        // Suspend TUI: disable raw mode and leave alternate screen.
        ratatui::restore();

        let spawn_result = std::process::Command::new(&editor).arg(&prd_path).status();

        // Re-enable raw mode and enter alternate screen.
        enable_raw_mode()?;
        execute!(stdout(), EnterAlternateScreen)?;
        terminal.clear()?;

        match spawn_result {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                self.status_message = Some(format!("editor not found: {editor}"));
            }
            Err(e) => {
                self.status_message = Some(e.to_string());
            }
            Ok(_) => {
                self.status_message = None;
            }
        }

        // Reload workflow from disk so updated tasks are immediately visible.
        self.load_current_workflow();

        Ok(())
    }

    fn handle_dialog_key(&mut self, code: KeyCode) {
        // Help overlay: any key closes it.
        if matches!(self.dialog, Some(Dialog::Help)) {
            self.dialog = None;
            return;
        }

        // ContinuePrompt: Y/Enter continues loop, any other key cancels to Done.
        if let Some(Dialog::ContinuePrompt { .. }) = &self.dialog {
            self.dialog = None;
            match code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    self.spawn_next_iteration();
                }
                _ => {
                    // Mark the active runner tab as Done (runner already exited).
                    if self.active_tab > 0
                        && let Some(tab) = self.runner_tabs.get_mut(self.active_tab - 1)
                    {
                        tab.state = RunnerTabState::Done;
                    }
                }
            }
            return;
        }

        // DeleteWorkflow confirmation: y confirms, any other key cancels.
        if let Some(Dialog::DeleteWorkflow { name }) = &self.dialog {
            let name = name.clone();
            let old_idx = self.selected_workflow;
            self.dialog = None;
            if code == KeyCode::Char('y') || code == KeyCode::Char('Y') {
                let dir = self.store.workflow_dir(&name);
                let _ = std::fs::remove_dir_all(dir);
                self.refresh_workflows_after_delete(old_idx);
            }
            return;
        }

        match code {
            KeyCode::Esc => {
                self.dialog = None;
            }
            KeyCode::Backspace => {
                if let Some(Dialog::NewWorkflow { input, error }) = &mut self.dialog {
                    input.pop();
                    *error = None;
                }
            }
            KeyCode::Char(c) if c.is_ascii_alphanumeric() || c == '-' => {
                if let Some(Dialog::NewWorkflow { input, error }) = &mut self.dialog {
                    input.push(c);
                    *error = None;
                }
            }
            KeyCode::Enter => {
                // Clone input before releasing the borrow so we can call store methods.
                let input = match &self.dialog {
                    Some(Dialog::NewWorkflow { input, .. }) => input.clone(),
                    _ => return,
                };
                if !Store::is_valid_name(&input) {
                    if let Some(Dialog::NewWorkflow { error, .. }) = &mut self.dialog {
                        *error = Some(
                            "Invalid name — use lowercase letters, digits, hyphens (3–64 chars)"
                                .to_string(),
                        );
                    }
                    return;
                }
                match self.store.create_workflow(&input) {
                    Ok(()) => {
                        self.dialog = None;
                        self.refresh_workflows_and_focus(&input);
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        if let Some(Dialog::NewWorkflow { error, .. }) = &mut self.dialog {
                            *error = if msg.contains("already exists") {
                                Some("Workflow already exists".to_string())
                            } else {
                                Some(msg)
                            };
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn open_help_dialog(&mut self) {
        self.dialog = Some(Dialog::Help);
    }

    fn open_new_workflow_dialog(&mut self) {
        self.dialog = Some(Dialog::NewWorkflow {
            input: String::new(),
            error: None,
        });
    }

    fn open_delete_workflow_dialog(&mut self) {
        let Some(idx) = self.selected_workflow else {
            return;
        };
        let Some(name) = self.workflows.get(idx).cloned() else {
            return;
        };
        self.dialog = Some(Dialog::DeleteWorkflow { name });
    }

    fn refresh_workflows_after_delete(&mut self, old_idx: Option<usize>) {
        self.workflows = self.store.list_workflows();
        self.selected_workflow = if self.workflows.is_empty() {
            None
        } else {
            Some(old_idx.map(|i| i.min(self.workflows.len() - 1)).unwrap_or(0))
        };
        self.load_current_workflow();
    }

    fn refresh_workflows_and_focus(&mut self, name: &str) {
        self.workflows = self.store.list_workflows();
        self.selected_workflow = self.workflows.iter().position(|p| p == name);
        if self.selected_workflow.is_none() && !self.workflows.is_empty() {
            self.selected_workflow = Some(0);
        }
        self.load_current_workflow();
    }

    fn load_current_workflow(&mut self) {
        self.current_workflow = self.selected_workflow.and_then(|i| {
            let name = self.workflows.get(i)?;
            let dir = self.store.workflow_dir(name);
            Workflow::load(&dir).ok()
        });
    }

    fn move_up(&mut self) {
        if let Some(i) = self.selected_workflow
            && i > 0
        {
            self.selected_workflow = Some(i - 1);
        }
        self.load_current_workflow();
    }

    fn move_down(&mut self) {
        if let Some(i) = self.selected_workflow
            && i + 1 < self.workflows.len()
        {
            self.selected_workflow = Some(i + 1);
        }
        self.load_current_workflow();
    }

    fn check_status_timeout(&mut self) {
        if let Some(expires) = self.status_message_expires
            && Instant::now() >= expires
        {
            self.status_message = None;
            self.status_message_expires = None;
        }
        // Clear notification after 3 seconds.
        if self
            .notification
            .as_ref()
            .is_some_and(|(_, set_at)| set_at.elapsed() >= Duration::from_secs(3))
        {
            self.notification = None;
        }
    }

    /// Drains all pending watcher events into a local Vec (non-blocking try_recv loop).
    /// Returns the collected events; an empty Vec means no file changes this tick.
    fn drain_watcher_channel(&mut self) -> Vec<WatcherEvent> {
        let Some(rx) = self.watcher_rx.as_mut() else {
            return Vec::new();
        };
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        events
    }

    /// Reloads all plan data from disk in response to a file-watcher event.
    ///
    /// Refreshes the workflow list, restores (or adjusts) the current selection,
    /// reloads the displayed workflow, and clears any stale `ContinuePrompt` dialog.
    /// Does not interrupt active runner subprocesses.
    pub fn reload_all(&mut self) {
        // Remember the currently selected workflow name to restore after the list refresh.
        let old_name = self.selected_workflow.and_then(|i| self.workflows.get(i).cloned());

        // Refresh the workflow list from disk.
        self.workflows = self.store.list_workflows();

        // Restore selection: prefer the same workflow by name; fall back to first, or None.
        self.selected_workflow = match &old_name {
            Some(name) => self
                .workflows
                .iter()
                .position(|p| p == name)
                .or(if self.workflows.is_empty() { None } else { Some(0) }),
            None => {
                if self.workflows.is_empty() {
                    None
                } else {
                    Some(0)
                }
            }
        };

        // Reload the currently selected workflow from disk.
        self.load_current_workflow();

        // Clear a stale ContinuePrompt if the referenced task no longer needs to run.
        if let Some(Dialog::ContinuePrompt { next_id, .. }) = &self.dialog {
            let next_id_clone = next_id.clone();

            // Find the workflow name for the active runner tab.
            let tab_workflow_name = (self.active_tab > 0)
                .then(|| self.runner_tabs.get(self.active_tab - 1))
                .flatten()
                .map(|t| t.workflow_name.clone());

            let task_still_pending = tab_workflow_name
                .as_ref()
                .map(|name| {
                    let dir = self.store.workflow_dir(name);
                    Workflow::load(&dir)
                        .ok()
                        .map(|w| w.prd.tasks.iter().any(|t| t.id == next_id_clone && !t.passes))
                        .unwrap_or(false)
                })
                .unwrap_or(false);

            if !task_still_pending {
                self.dialog = None;
                if self.active_tab > 0
                    && let Some(tab) = self.runner_tabs.get_mut(self.active_tab - 1)
                    && matches!(tab.state, RunnerTabState::Running { .. })
                {
                    tab.state = RunnerTabState::Done;
                }
            }
        }
    }

    /// Drains runner channels for all active runner tabs.
    fn drain_runner_channels(&mut self) {
        for tab_idx in 0..self.runner_tabs.len() {
            if self.runner_tabs[tab_idx].runner_rx.is_none() {
                continue;
            }
            self.drain_tab_channel(tab_idx);
        }
    }

    /// Drains events from the channel of runner tab at `tab_idx` and processes them.
    fn drain_tab_channel(&mut self, tab_idx: usize) {
        // Collect events into local vecs to avoid simultaneous mutable borrows.
        let mut byte_chunks: Vec<Vec<u8>> = Vec::new();
        let mut done = false;
        let mut complete = false;
        let mut spawn_error: Option<String> = None;
        // None = killed/unknown, Some(n) = natural exit code.
        let mut exited_code: Option<Option<u32>> = None;

        {
            let rx = match self.runner_tabs[tab_idx].runner_rx.as_mut() {
                Some(r) => r,
                None => return,
            };
            loop {
                use tokio::sync::mpsc::error::TryRecvError;
                match rx.try_recv() {
                    Ok(RunnerEvent::Bytes(bytes)) => byte_chunks.push(bytes),
                    Ok(RunnerEvent::Complete) => complete = true,
                    Ok(RunnerEvent::Resize(_, _)) => {} // resize acks via separate channel; ignore
                    Ok(RunnerEvent::Exited(code_opt)) => {
                        exited_code = Some(code_opt);
                        done = true;
                        break;
                    }
                    Ok(RunnerEvent::SpawnError(msg)) => {
                        spawn_error = Some(msg);
                        done = true;
                        break;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        done = true;
                        break;
                    }
                }
            }
        } // rx borrow released

        // Feed raw bytes into the vt100 parser.
        for chunk in byte_chunks {
            self.runner_tabs[tab_idx].parser.process(&chunk);
        }

        // Complete signal handling.
        //
        // Three sub-cases based on (auto_continue, done):
        //   auto_continue=false (any done): mark Done immediately — original behavior.
        //   auto_continue=true, done=false: sentinel arrived, process still running;
        //     decide now — spawn next or mark Done.
        //   auto_continue=true, done=true: defer all state changes to the done block below
        //     so the done block can read the Running iteration and run the auto-loop.
        if complete {
            let is_auto = self.runner_tabs[tab_idx].auto_continue;
            if is_auto && !done {
                // Sentinel received; process still running. Decide now.
                self.load_current_workflow();
                let workflow_name = self.runner_tabs[tab_idx].workflow_name.clone();
                let workflow_dir = self.store.workflow_dir(&workflow_name);
                let tab_workflow = Workflow::load(&workflow_dir).ok();
                let is_complete =
                    tab_workflow.as_ref().map(|w| w.is_complete()).unwrap_or(false);
                if is_complete {
                    self.runner_tabs[tab_idx].state = RunnerTabState::Done;
                } else {
                    // Spawn next immediately; old process will exit on its own.
                    // Its Exited event goes on the old (now-replaced) channel and is discarded.
                    self.spawn_next_iteration_at(tab_idx);
                }
            } else if !is_auto {
                // Original behavior: mark Done right away.
                self.runner_tabs[tab_idx].state = RunnerTabState::Done;
                self.load_current_workflow();
            }
            // When is_auto && done: fall through; the done block below handles everything.
        }

        if done {
            self.runner_tabs[tab_idx].runner_rx = None;
            self.runner_tabs[tab_idx].runner_kill_tx = None;
            self.runner_tabs[tab_idx].stdin_tx = None;

            // Write exit/stopped summary into the parser; skip for spawn errors (process never ran).
            if spawn_error.is_none() {
                let msg = match exited_code {
                    Some(None) => "\r\n--- Runner stopped ---\r\n".to_string(),
                    Some(Some(code)) => {
                        format!("\r\n--- Runner exited (code: {code}) ---\r\n")
                    }
                    None => "\r\n--- Runner exited ---\r\n".to_string(),
                };
                self.runner_tabs[tab_idx].parser.process(msg.as_bytes());
            }

            if let Some(msg) = spawn_error {
                // Write spawn error into the parser so it appears in the terminal output.
                let err_msg = format!("\r\nSpawnError: {msg}\r\n");
                self.runner_tabs[tab_idx].parser.process(err_msg.as_bytes());
                self.runner_tabs[tab_idx].state = RunnerTabState::Error(msg.clone());
                self.status_message = Some(msg);
                self.status_message_expires = None; // persist until dismissed
            } else {
                // Reload plan from disk — ralph may have updated passes: true.
                self.load_current_workflow();

                // Determine whether to auto-loop, show ContinuePrompt, or transition to Done.
                // Only act if still in Running state (not already Done from Complete signal or stop).
                let iteration_opt = match self.runner_tabs[tab_idx].state {
                    RunnerTabState::Running { iteration } => Some(iteration),
                    _ => None,
                };

                if let Some(iteration) = iteration_opt {
                    // Load the specific workflow for this runner tab (may differ from selected).
                    let workflow_name = self.runner_tabs[tab_idx].workflow_name.clone();
                    let workflow_dir = self.store.workflow_dir(&workflow_name);
                    let tab_workflow = Workflow::load(&workflow_dir).ok();

                    let is_complete =
                        tab_workflow.as_ref().map(|w| w.is_complete()).unwrap_or(false);

                    let auto_continue = self.runner_tabs[tab_idx].auto_continue;

                    if auto_continue {
                        // Sentinel (complete) takes precedence: treat as success regardless of
                        // exit code. Without sentinel, exit code 0 is success.
                        let is_success =
                            complete || matches!(exited_code, Some(Some(0)));

                        if is_complete {
                            self.runner_tabs[tab_idx].state = RunnerTabState::Done;
                        } else if is_success {
                            // Success: spawn next iteration immediately.
                            self.spawn_next_iteration_at(tab_idx);
                        } else if iteration >= MAX_ITERATIONS {
                            let msg = format!(
                                "\r\n[runner] Max iterations ({MAX_ITERATIONS}) reached. Stopping.\r\n"
                            );
                            self.runner_tabs[tab_idx].parser.process(msg.as_bytes());
                            self.runner_tabs[tab_idx].state = RunnerTabState::Done;
                        } else {
                            // Failure within limit: write retry log and spawn next.
                            let exit_code =
                                match exited_code { Some(Some(c)) => c, _ => 1u32 };
                            let msg = format!(
                                "\r\n[runner] Task failed (exit {exit_code}), retrying\u{2026} ({iteration}/{MAX_ITERATIONS})\r\n"
                            );
                            self.runner_tabs[tab_idx].parser.process(msg.as_bytes());
                            self.spawn_next_iteration_at(tab_idx);
                        }
                    } else {
                        // auto_continue=false: original ContinuePrompt behavior.
                        if is_complete {
                            self.runner_tabs[tab_idx].state = RunnerTabState::Done;
                        } else if iteration >= MAX_ITERATIONS {
                            let msg = format!(
                                "\r\nMax iterations ({MAX_ITERATIONS}) reached. Stopping.\r\n"
                            );
                            self.runner_tabs[tab_idx].parser.process(msg.as_bytes());
                            self.runner_tabs[tab_idx].state = RunnerTabState::Done;
                        } else {
                            // Natural exit within limit — ask user whether to continue.
                            let next = tab_workflow
                                .as_ref()
                                .and_then(|w| w.next_task())
                                .map(|t| (t.id.clone(), t.title.clone()))
                                .unwrap_or_else(|| ("?".to_string(), "unknown".to_string()));
                            self.dialog = Some(Dialog::ContinuePrompt {
                                next_id: next.0,
                                next_title: next.1,
                            });
                            // Keep Running { iteration } while awaiting the user's decision.
                        }
                    }
                }
            }
        }
    }

    fn stop_runner(&mut self) {
        if self.active_tab == 0 {
            return;
        }
        let tab_idx = self.active_tab - 1;
        let Some(tab) = self.runner_tabs.get_mut(tab_idx) else {
            return;
        };
        if !matches!(tab.state, RunnerTabState::Running { .. }) {
            return;
        }
        if let Some(kill_tx) = tab.runner_kill_tx.take() {
            let _ = kill_tx.send(());
        }
        // Mark Done immediately so drain_tab_channel skips the ContinuePrompt when Exited arrives.
        tab.state = RunnerTabState::Done;
    }

    fn start_runner(&mut self) {
        let Some(idx) = self.selected_workflow else {
            return;
        };
        let Some(name) = self.workflows.get(idx).cloned() else {
            return;
        };

        // Prevent starting a second runner for the same workflow while one is active.
        if self.runner_tabs.iter().any(|t| {
            t.workflow_name == name && matches!(t.state, RunnerTabState::Running { .. })
        }) {
            self.status_message = Some("Already running".to_string());
            self.status_message_expires = Some(Instant::now() + Duration::from_secs(2));
            return;
        }

        let plan_dir = self.store.workflow_dir(&name);
        let repo_root = self.store.root().to_path_buf();

        // Load workflow to populate current task info before spawning.
        let (current_task_id, current_task_title) = {
            let workflow_dir = self.store.workflow_dir(&name);
            match Workflow::load(&workflow_dir).ok().and_then(|w| {
                w.next_task().map(|t| (t.id.clone(), t.title.clone()))
            }) {
                Some((id, title)) => (Some(id), Some(title)),
                None => (None, None),
            }
        };

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<RunnerEvent>();
        let (kill_tx, kill_rx) = oneshot::channel::<()>();
        let (stdin_tx, stdin_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        let (resize_tx, resize_rx) = tokio::sync::mpsc::unbounded_channel::<(u16, u16)>();

        // Reuse an existing Done/Error tab for this workflow rather than accumulating tabs.
        let reuse_idx = self.runner_tabs.iter().position(|t| {
            t.workflow_name == name
                && !matches!(t.state, RunnerTabState::Running { .. })
        });

        let (cols, rows) = self.initial_size;
        if let Some(reuse) = reuse_idx {
            let tab = &mut self.runner_tabs[reuse];
            // Reset parser with current terminal dimensions; scrollback capacity = 1000.
            tab.parser = VtParser::new(rows, cols, 1000);
            tab.log_scroll = 0;
            tab.state = RunnerTabState::Running { iteration: 1 };
            tab.runner_rx = Some(rx);
            tab.runner_kill_tx = Some(kill_tx);
            tab.stdin_tx = Some(stdin_tx);
            tab.auto_continue = false;
            tab.current_task_id = current_task_id;
            tab.current_task_title = current_task_title;
            tab.iterations_used = 1;
            self.active_tab = reuse + 1; // active_tab is 1-indexed for runner tabs
        } else {
            let tab = RunnerTab {
                workflow_name: name,
                parser: VtParser::new(rows, cols, 1000),
                state: RunnerTabState::Running { iteration: 1 },
                runner_rx: Some(rx),
                runner_kill_tx: Some(kill_tx),
                stdin_tx: Some(stdin_tx),
                log_scroll: 0,
                auto_continue: false,
                current_task_id,
                current_task_title,
                iterations_used: 1,
            };
            self.runner_tabs.push(tab);
            self.active_tab = self.runner_tabs.len(); // runner tabs are 1-indexed in active_tab
        }

        self.resize_txs.push(resize_tx);
        drop(tokio::spawn(runner_task(
            plan_dir, repo_root, tx, kill_rx, stdin_rx, self.initial_size, resize_rx,
        )));
    }

    /// Spawns the next claude iteration after the user confirms via the ContinuePrompt dialog.
    /// Increments the current iteration counter and starts a new subprocess on the active runner tab.
    fn spawn_next_iteration(&mut self) {
        if self.active_tab == 0 {
            return;
        }
        self.spawn_next_iteration_at(self.active_tab - 1);
    }

    /// Spawns the next claude iteration on the runner tab at `tab_idx`.
    /// Increments the iteration counter and replaces the subprocess channels.
    /// Requires the tab to be in `Running { iteration }` state; returns early otherwise.
    fn spawn_next_iteration_at(&mut self, tab_idx: usize) {
        // Extract workflow_name and iteration without holding a borrow.
        let (name, iteration) = {
            let Some(tab) = self.runner_tabs.get(tab_idx) else {
                return;
            };
            let iteration = match tab.state {
                RunnerTabState::Running { iteration } => iteration,
                _ => return,
            };
            (tab.workflow_name.clone(), iteration)
        };

        let plan_dir = self.store.workflow_dir(&name);
        let repo_root = self.store.root().to_path_buf();

        // Load workflow to update current task info before spawning.
        let (current_task_id, current_task_title) = {
            let workflow_dir = self.store.workflow_dir(&name);
            match Workflow::load(&workflow_dir).ok().and_then(|w| {
                w.next_task().map(|t| (t.id.clone(), t.title.clone()))
            }) {
                Some((id, title)) => (Some(id), Some(title)),
                None => (None, None),
            }
        };

        let new_iteration = iteration + 1;

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<RunnerEvent>();
        let (kill_tx, kill_rx) = oneshot::channel::<()>();
        let (stdin_tx, stdin_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        let (resize_tx, resize_rx) = tokio::sync::mpsc::unbounded_channel::<(u16, u16)>();

        if let Some(tab) = self.runner_tabs.get_mut(tab_idx) {
            tab.runner_rx = Some(rx);
            tab.runner_kill_tx = Some(kill_tx);
            tab.stdin_tx = Some(stdin_tx);
            tab.state = RunnerTabState::Running { iteration: new_iteration };
            tab.current_task_id = current_task_id;
            tab.current_task_title = current_task_title;
            tab.iterations_used = new_iteration;
        }

        self.resize_txs.push(resize_tx);
        drop(tokio::spawn(runner_task(
            plan_dir, repo_root, tx, kill_rx, stdin_rx, self.initial_size, resize_rx,
        )));
    }
}
