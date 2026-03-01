use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::text::Line;
use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, enable_raw_mode};
use ratatui::DefaultTerminal;
use std::io::stdout;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;

use crate::ralph::runner::RunnerEvent;
use crate::ralph::store::Store;
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
    /// Capped at 1000 lines; oldest lines are dropped when the cap is exceeded.
    pub log_lines: Vec<Line<'static>>,
    pub state: RunnerTabState,
    pub runner_rx: Option<UnboundedReceiver<RunnerEvent>>,
    pub runner_kill_tx: Option<oneshot::Sender<()>>,
    /// Sender used to deliver stdin lines to the claude subprocess (set in TASK-002).
    pub stdin_tx: Option<UnboundedSender<String>>,
    pub input_buffer: String,
    /// Scroll offset for the log view (0 = auto-scroll to bottom).
    pub log_scroll: usize,
}

impl RunnerTab {
    fn push_log(&mut self, line: Line<'static>) {
        self.log_lines.push(line);
        if self.log_lines.len() > 1000 {
            self.log_lines.remove(0);
        }
    }
}

/// Converts a raw string (possibly containing ANSI escape sequences) into a
/// `Line<'static>` with ratatui style spans. Falls back to a plain unstyled
/// line if the ANSI parser fails.
fn parse_ansi_line(s: String) -> Line<'static> {
    use ansi_to_tui::IntoText;
    let bytes = s.as_bytes().to_vec();
    match bytes.into_text() {
        Ok(mut text) if !text.lines.is_empty() => text.lines.remove(0),
        _ => Line::from(s),
    }
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
    mut stdin_rx: UnboundedReceiver<String>,
    size: (u16, u16),
) {
    use portable_pty::{CommandBuilder, PtySize, native_pty_system};

    eprintln!("[runner] spawning claude in {}", repo_root.display());
    eprintln!("[runner] RALPH_PLAN_DIR={}", plan_dir.display());

    let (cols, rows) = size;
    let pty_system = native_pty_system();
    let pair = match pty_system.openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[runner] PTY open failed: {e}");
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
            eprintln!("[runner] spawn failed: {friendly}");
            let _ = tx.send(RunnerEvent::SpawnError(friendly));
            return;
        }
    };

    eprintln!("[runner] spawned claude pid={:?}", child.process_id());

    // Clone the killer handle so we can signal the child from the select arm
    // without holding the child borrow (which is blocked in wait()).
    let mut killer = child.clone_killer();

    // Drop the slave end in the parent so EOF propagates to the master when
    // the child process exits and closes its inherited slave fd.
    drop(pair.slave);

    let reader = match pair.master.try_clone_reader() {
        Ok(r) => r,
        Err(e) => {
            let _ = tx.send(RunnerEvent::SpawnError(format!("PTY reader: {e}")));
            return;
        }
    };
    let mut writer = match pair.master.take_writer() {
        Ok(w) => w,
        Err(e) => {
            let _ = tx.send(RunnerEvent::SpawnError(format!("PTY writer: {e}")));
            return;
        }
    };

    // Read PTY output in a blocking thread: split on newlines, send as Line events.
    let tx_read = tx.clone();
    let read_handle = tokio::task::spawn_blocking(move || {
        use std::io::Read;
        let mut buf = [0u8; 4096];
        let mut remainder = String::new();
        let mut reader = reader;
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let chunk = String::from_utf8_lossy(&buf[..n]);
                    remainder.push_str(&chunk);
                    while let Some(pos) = remainder.find('\n') {
                        let line = remainder[..pos].trim_end_matches('\r').to_string();
                        if line.contains("<promise>COMPLETE</promise>") {
                            let _ = tx_read.send(RunnerEvent::Complete);
                        }
                        if tx_read.send(RunnerEvent::Line(line)).is_err() {
                            return;
                        }
                        remainder = remainder[pos + 1..].to_string();
                    }
                }
            }
        }
        // Flush any incomplete final line (no trailing newline).
        if !remainder.is_empty() {
            let line = remainder.trim_end_matches('\r').to_string();
            if line.contains("<promise>COMPLETE</promise>") {
                let _ = tx_read.send(RunnerEvent::Complete);
            }
            let _ = tx_read.send(RunnerEvent::Line(line));
        }
    });

    // Forward stdin_rx lines to PTY stdin. Writes are small and infrequent
    // (user keyboard input), so a brief sync write in the async task is acceptable.
    drop(tokio::spawn(async move {
        use std::io::Write;
        while let Some(line) = stdin_rx.recv().await {
            let mut data = line;
            data.push('\n');
            if writer.write_all(data.as_bytes()).is_err() {
                break;
            }
        }
    }));

    // Wait for the child to exit in a blocking task; signal completion via oneshot.
    let (done_tx, done_rx) = tokio::sync::oneshot::channel::<()>();
    tokio::task::spawn_blocking(move || {
        let _ = child.wait();
        let _ = done_tx.send(());
    });

    let was_killed = tokio::select! {
        _ = done_rx => {
            eprintln!("[runner] claude exited naturally");
            false
        }
        _ = kill_rx => true,
    };

    if was_killed {
        eprintln!("[runner] killing claude");
        let _ = killer.kill();
        eprintln!("[runner] claude killed");
    }

    // Wait for the reader to drain all remaining PTY output before sending Exited.
    let _ = read_handle.await;
    let _ = tx.send(RunnerEvent::Exited);
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
}

impl App {
    pub fn new(store: Store, initial_size: (u16, u16)) -> Self {
        let workflows = store.list_workflows();
        let selected_workflow = if workflows.is_empty() { None } else { Some(0) };
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
            status_message: None,
            status_message_expires: None,
            initial_size,
        };
        app.load_current_workflow();
        app
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while self.running {
            self.check_status_timeout();
            self.drain_runner_channels();
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
        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
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
                match key.code {
                    KeyCode::Char('t') => self.tab_nav_pending = true,
                    KeyCode::Char('q') => self.running = false,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.running = false;
                    }
                    KeyCode::Char('s') => self.stop_runner(),
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
                    // Log scroll: Up/k scroll up (toward older lines), Down/j scroll down.
                    // log_scroll == 0 means auto-scroll (always show newest line).
                    // log_scroll == N means show the line N positions from the bottom.
                    KeyCode::Up | KeyCode::Char('k') => {
                        let tab_idx = self.active_tab - 1;
                        if let Some(tab) = self.runner_tabs.get_mut(tab_idx)
                            && !tab.log_lines.is_empty()
                        {
                            let last = tab.log_lines.len() - 1;
                            tab.log_scroll = (tab.log_scroll + 1).min(last);
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        let tab_idx = self.active_tab - 1;
                        if let Some(tab) = self.runner_tabs.get_mut(tab_idx) {
                            tab.log_scroll = tab.log_scroll.saturating_sub(1);
                        }
                    }
                    // End or G re-enables auto-scroll.
                    KeyCode::End | KeyCode::Char('G') => {
                        let tab_idx = self.active_tab - 1;
                        if let Some(tab) = self.runner_tabs.get_mut(tab_idx) {
                            tab.log_scroll = 0;
                        }
                    }
                    // Input buffer: Backspace removes last char.
                    KeyCode::Backspace => {
                        let tab_idx = self.active_tab - 1;
                        if let Some(tab) = self.runner_tabs.get_mut(tab_idx) {
                            tab.input_buffer.pop();
                        }
                    }
                    // Input buffer: Enter sends trimmed buffer via stdin_tx (if active).
                    KeyCode::Enter => {
                        let tab_idx = self.active_tab - 1;
                        // Extract result inside the borrow scope so self.status_message
                        // can be set after the borrow on runner_tabs is released.
                        let error_msg = self.runner_tabs.get_mut(tab_idx).and_then(|tab| {
                            let trimmed = tab.input_buffer.trim().to_string();
                            tab.input_buffer.clear();
                            if let Some(tx) = &tab.stdin_tx {
                                let _ = tx.send(trimmed);
                                None
                            } else {
                                Some("Runner is not active".to_string())
                            }
                        });
                        if let Some(msg) = error_msg {
                            self.status_message = Some(msg);
                            self.status_message_expires =
                                Some(Instant::now() + Duration::from_secs(2));
                        }
                    }
                    // Input buffer: Esc clears without sending.
                    KeyCode::Esc => {
                        let tab_idx = self.active_tab - 1;
                        if let Some(tab) = self.runner_tabs.get_mut(tab_idx) {
                            tab.input_buffer.clear();
                        }
                    }
                    // Input buffer: append any remaining printable char.
                    // Specific chars (t/q/s/k/j/G and Ctrl+C) are caught by earlier arms
                    // so they never reach here.
                    KeyCode::Char(c) if !c.is_control() => {
                        let tab_idx = self.active_tab - 1;
                        if let Some(tab) = self.runner_tabs.get_mut(tab_idx) {
                            tab.input_buffer.push(c);
                        }
                    }
                    _ => {}
                }
            }
        }
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
        let mut lines: Vec<String> = Vec::new();
        let mut done = false;
        let mut complete = false;
        let mut spawn_error: Option<String> = None;

        {
            let rx = match self.runner_tabs[tab_idx].runner_rx.as_mut() {
                Some(r) => r,
                None => return,
            };
            loop {
                use tokio::sync::mpsc::error::TryRecvError;
                match rx.try_recv() {
                    Ok(RunnerEvent::Line(line)) => lines.push(line),
                    Ok(RunnerEvent::Complete) => complete = true,
                    Ok(RunnerEvent::Exited) => {
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

        for line in lines {
            self.runner_tabs[tab_idx].push_log(parse_ansi_line(line));
        }

        // Complete signal: transition to Done and refresh display.
        if complete {
            self.runner_tabs[tab_idx].state = RunnerTabState::Done;
            self.load_current_workflow();
        }

        if done {
            self.runner_tabs[tab_idx].runner_rx = None;
            self.runner_tabs[tab_idx].runner_kill_tx = None;
            self.runner_tabs[tab_idx].stdin_tx = None;
            self.runner_tabs[tab_idx].push_log(Line::from("--- Runner exited ---"));

            if let Some(msg) = spawn_error {
                let error_msg = msg.clone();
                self.runner_tabs[tab_idx].push_log(Line::from(format!("ERROR: {error_msg}")));
                self.runner_tabs[tab_idx].state = RunnerTabState::Error(msg);
                self.status_message = Some(error_msg);
                self.status_message_expires = None; // persist until dismissed
            } else {
                // Reload plan from disk — ralph may have updated passes: true.
                self.load_current_workflow();

                // Determine whether to show ContinuePrompt or transition to Done.
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

                    if is_complete {
                        self.runner_tabs[tab_idx].state = RunnerTabState::Done;
                    } else if iteration >= MAX_ITERATIONS {
                        let msg =
                            format!("Max iterations ({MAX_ITERATIONS}) reached. Stopping.");
                        self.runner_tabs[tab_idx].push_log(Line::from(msg));
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

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<RunnerEvent>();
        let (kill_tx, kill_rx) = oneshot::channel::<()>();
        let (stdin_tx, stdin_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

        // Reuse an existing Done/Error tab for this workflow rather than accumulating tabs.
        let reuse_idx = self.runner_tabs.iter().position(|t| {
            t.workflow_name == name
                && !matches!(t.state, RunnerTabState::Running { .. })
        });

        if let Some(reuse) = reuse_idx {
            let tab = &mut self.runner_tabs[reuse];
            tab.log_lines.clear();
            tab.log_scroll = 0;
            tab.input_buffer.clear();
            tab.state = RunnerTabState::Running { iteration: 1 };
            tab.runner_rx = Some(rx);
            tab.runner_kill_tx = Some(kill_tx);
            tab.stdin_tx = Some(stdin_tx);
            self.active_tab = reuse + 1; // active_tab is 1-indexed for runner tabs
        } else {
            let tab = RunnerTab {
                workflow_name: name,
                log_lines: Vec::new(),
                state: RunnerTabState::Running { iteration: 1 },
                runner_rx: Some(rx),
                runner_kill_tx: Some(kill_tx),
                stdin_tx: Some(stdin_tx),
                input_buffer: String::new(),
                log_scroll: 0,
            };
            self.runner_tabs.push(tab);
            self.active_tab = self.runner_tabs.len(); // runner tabs are 1-indexed in active_tab
        }

        drop(tokio::spawn(runner_task(
            plan_dir, repo_root, tx, kill_rx, stdin_rx, self.initial_size,
        )));
    }

    /// Spawns the next claude iteration after the user confirms via the ContinuePrompt dialog.
    /// Increments the current iteration counter and starts a new subprocess on the active runner tab.
    fn spawn_next_iteration(&mut self) {
        if self.active_tab == 0 {
            return;
        }
        let tab_idx = self.active_tab - 1;

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

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<RunnerEvent>();
        let (kill_tx, kill_rx) = oneshot::channel::<()>();
        let (stdin_tx, stdin_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

        if let Some(tab) = self.runner_tabs.get_mut(tab_idx) {
            tab.runner_rx = Some(rx);
            tab.runner_kill_tx = Some(kill_tx);
            tab.stdin_tx = Some(stdin_tx);
            tab.state = RunnerTabState::Running { iteration: iteration + 1 };
        }

        drop(tokio::spawn(runner_task(
            plan_dir, repo_root, tx, kill_rx, stdin_rx, self.initial_size,
        )));
    }
}
