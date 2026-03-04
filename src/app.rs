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
use crate::ralph::usage::{TaskUsage, UsageFile};
use crate::ralph::watcher::{Watcher, WatcherEvent};
use crate::ralph::workflow::{Task, Workflow};

// Maximum number of ralph loop iterations before the loop stops automatically.
// TODO: make configurable
const MAX_ITERATIONS: u32 = 10;

// Rows consumed by UI chrome around the PTY viewport:
// 1 tab bar + 1 task bar + 1 top border + 1 bottom border + 1 buttons bar.
const PTY_ROW_OVERHEAD: u16 = 5;

/// Per-runner tab state.
pub enum RunnerTabState {
    Running { iteration: u32 },
    Done,
    /// Runner was manually stopped by the user (via [s]top).
    Stopped,
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
    /// without waiting for the user to press [c].
    pub auto_continue: bool,
    /// ID of the task currently being executed (populated from `next_task()` before spawn).
    pub current_task_id: Option<String>,
    /// Title of the task currently being executed.
    pub current_task_title: Option<String>,
    /// Number of iterations used for this runner tab (starts at 1, incremented by spawn_next_iteration).
    pub iterations_used: u32,
    /// Accumulated input tokens for the current story.
    pub current_story_input_tokens: u64,
    /// Accumulated output tokens for the current story.
    pub current_story_output_tokens: u64,
    /// Accumulated cache read tokens for the current story.
    pub current_story_cache_read_tokens: u64,
    /// Accumulated cache write tokens for the current story.
    pub current_story_cache_write_tokens: u64,
    /// Estimated USD cost for the current story.
    pub current_story_cost_usd: f64,
    /// When true, raw key input is forwarded to the PTY (Insert mode).
    /// When false, app shortcuts are active (Normal mode).
    pub insert_mode: bool,
    /// Set to true when a `RunnerEvent::Complete` sentinel is received during an iteration.
    /// Cleared at the start of each new iteration and consumed in the `done` block to
    /// determine whether the task should be treated as a success.
    pub saw_complete: bool,
}

pub enum Dialog {
    NewWorkflow {
        input: String,
        error: Option<String>,
    },
    DeleteWorkflow {
        name: String,
    },
    ContinuePrompt {
        next_id: String,
        next_title: String,
    },
    Help,
    RunnerHelp,
    ImportPrd {
        workflow_name: String,
        input: String,
        error: Option<String>,
        /// When true, prd-source.md already exists and the user is being asked to confirm overwrite.
        confirm_overwrite: bool,
    },
    /// Shown when the user presses [q]; y/Y confirms quit, any other key cancels.
    QuitConfirm,
    /// Shown when the user presses [s] and the workflow is not complete; y/Y stops, any other key cancels.
    StopConfirm,
}

/// Which field of the metadata form currently has focus.
#[derive(Debug, Clone, PartialEq)]
pub enum PrdEditorField {
    Project,
    Branch,
    Description,
    ValidationCommands,
}

/// Which field of the story detail form currently has focus.
#[derive(Debug, Clone, PartialEq)]
pub enum StoryDetailField {
    Id,
    Title,
    Description,
    Priority,
    Criteria,
}

/// Which top-level section of the PRD editor is active.
#[derive(Debug, Clone, PartialEq)]
pub enum PrdEditorMode {
    /// Focus is on the metadata fields (Project, Branch, Description).
    Metadata,
    /// Focus is on the story list panel.
    StoryList,
    /// Focus is on the story detail form (fully implemented in US-003).
    StoryDetail,
}

/// In-memory state for the full-screen plan-metadata editor.
pub struct PrdEditorState {
    /// Name of the workflow being edited (used to resolve the directory).
    pub workflow_name: String,
    pub project: String,
    pub branch: String,
    pub description: String,
    /// Which metadata field has focus when mode == Metadata.
    pub focused_field: PrdEditorField,
    /// Which top-level section of the editor is active.
    pub mode: PrdEditorMode,
    /// In-memory copy of all tasks; mutated by story list add/delete.
    pub stories: Vec<Task>,
    /// Index of the currently selected story in the story list.
    /// When mode == StoryDetail, this is the index of the story being edited
    /// (or None when is_new_story == true).
    pub selected_story: Option<usize>,
    /// True when StoryDetail was opened via [a] (new story) rather than Enter.
    pub is_new_story: bool,
    /// Some(idx) = delete confirmation prompt is shown for the story at that index.
    pub confirm_delete: Option<usize>,
    /// Transient error/status shown in the hint line; cleared on next keystroke.
    pub status: Option<String>,
    /// Validation commands list (one per line); populated from prd.json.
    pub validation_commands: Vec<String>,
    /// Index of the currently active validation command line (within validation_commands).
    pub validation_commands_cursor: usize,
    // Story detail editing fields (valid when mode == StoryDetail; populated on entry).
    pub story_id: String,
    pub story_title: String,
    pub story_description: String,
    pub story_priority: String,
    /// One string per criterion; may be empty when criteria list is empty.
    pub story_criteria: Vec<String>,
    /// Index of the currently active criterion line (within story_criteria).
    pub story_criteria_cursor: usize,
    /// Which field of the story detail form has focus.
    pub story_focused_field: StoryDetailField,
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
    let pair = match pty_system.openpty(PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    }) {
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
    // RALPH_SENTINEL_COMPLETE is detected by scanning the ANSI-stripped combined buffer
    // (tail + current chunk) so ANSI escape codes around the sentinel don't break detection,
    // and sentinels split across two 4096-byte reads are still caught.
    let tx_read = tx.clone();
    let debug_pty = std::env::var("RALPH_DEBUG_PTY").is_ok();
    let debug_log_path = repo_root.join(".ralph").join("pty-debug.log");
    let read_handle = tokio::task::spawn_blocking(move || {
        use std::io::Read;
        let mut buf = [0u8; 4096];
        let mut reader = reader;
        // Tail buffer: last ~512 bytes of the previous chunk, prepended to the current
        // chunk before scanning for token lines. Prevents missing lines split across chunks.
        let mut tail: Vec<u8> = Vec::new();
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let chunk = &buf[..n];
                    let chunk_str = String::from_utf8_lossy(chunk);

                    // When RALPH_DEBUG_PTY=1, append ANSI-stripped text to .ralph/pty-debug.log
                    // so the actual Claude CLI output format can be inspected.
                    // The raw bytes are forwarded unchanged; only the stripped copy is logged.
                    if debug_pty {
                        use std::io::Write;
                        let stripped = strip_ansi(&chunk_str);
                        if let Ok(mut f) = std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(&debug_log_path)
                        {
                            let _ = write!(f, "{stripped}");
                            let _ = writeln!(f, "\n---");
                        }
                    }

                    // Build combined string (tail + chunk) to handle lines split across
                    // consecutive 4096-byte PTY chunks. Strip ANSI before parsing.
                    let mut combined = Vec::with_capacity(tail.len() + n);
                    combined.extend_from_slice(&tail);
                    combined.extend_from_slice(chunk);
                    let combined_lossy = String::from_utf8_lossy(&combined);
                    let stripped_combined = strip_ansi(&combined_lossy);

                    // Detect the completion sentinel on the ANSI-stripped combined buffer so
                    // that ANSI escape codes injected by the terminal don't prevent matching.
                    // Uses a plain-text sentinel because Claude Code CLI strips XML-like tags
                    // (e.g. <promise>) from rendered output before they reach the PTY.
                    if stripped_combined.contains("RALPH_SENTINEL_COMPLETE") {
                        let _ = tx_read.send(RunnerEvent::Complete);
                    }

                    if let Some(usage) = parse_token_line(&stripped_combined) {
                        let _ = tx_read.send(RunnerEvent::TokenUsage {
                            input_tokens: usage.0,
                            output_tokens: usage.1,
                            cache_read_tokens: usage.2,
                            cache_write_tokens: usage.3,
                            cost_usd: usage.4,
                        });
                    }

                    // Update tail to last 512 bytes of the current chunk.
                    tail = chunk[chunk.len().saturating_sub(512)..].to_vec();

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
            let _ = master.resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            });
        }
    }));

    // Wait for the child to exit in a blocking task; send the exit code via oneshot.
    let (done_tx, done_rx) = tokio::sync::oneshot::channel::<u32>();
    tokio::task::spawn_blocking(move || {
        let code = child
            .wait()
            .map(|s| if s.success() { 0u32 } else { 1u32 })
            .unwrap_or(1u32);
        let _ = done_tx.send(code);
    });

    let (was_killed, exit_code) = tokio::select! {
        result = done_rx => (false, Some(result.unwrap_or(1))),
        _ = kill_rx => (true, None),
    };

    if was_killed {
        let _ = killer.kill();
    }

    // Drain remaining PTY output, but with a 500 ms timeout.
    // Claude may spawn subprocesses (git, test runners, etc.) that inherit the PTY
    // slave fd.  Those processes keep the slave open after the main claude process
    // exits, which would cause reader.read() to block forever — preventing the
    // Exited event from ever being sent and leaving the tab stuck in Running state.
    let _ = tokio::time::timeout(Duration::from_millis(500), read_handle).await;

    // None = killed, Some(n) = natural exit with code n.
    let _ = tx.send(RunnerEvent::Exited(exit_code));
}

/// Spawns `claude --dangerously-skip-permissions /prd-synth` inside a PTY in the
/// workflow directory and streams output back via `tx`.
/// Listens on `kill_rx` for an early termination signal.
async fn synth_task(
    workflow_dir: PathBuf,
    tx: UnboundedSender<RunnerEvent>,
    kill_rx: oneshot::Receiver<()>,
    size: (u16, u16),
    resize_rx: UnboundedReceiver<(u16, u16)>,
) {
    use portable_pty::{CommandBuilder, PtySize, native_pty_system};

    let _ = tx.send(RunnerEvent::Bytes(
        format!(
            "[synth] spawning claude prd-synth in {}\r\n",
            workflow_dir.display()
        )
        .into_bytes(),
    ));

    let (cols, rows) = size;
    let pty_system = native_pty_system();
    let pair = match pty_system.openpty(PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    }) {
        Ok(p) => p,
        Err(e) => {
            let _ = tx.send(RunnerEvent::SpawnError(format!("PTY open failed: {e}")));
            return;
        }
    };

    let mut cmd = CommandBuilder::new("claude");
    cmd.args(["--dangerously-skip-permissions", "/prd-synth"]);
    cmd.cwd(&workflow_dir);

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
        format!("[synth] spawned claude pid={:?}\r\n", child.process_id()).into_bytes(),
    ));

    let mut killer = child.clone_killer();

    // Drop the slave end in the parent so EOF propagates when the child exits.
    drop(pair.slave);

    let master = pair.master;

    let reader = match master.try_clone_reader() {
        Ok(r) => r,
        Err(e) => {
            let _ = tx.send(RunnerEvent::SpawnError(format!("PTY reader: {e}")));
            return;
        }
    };

    // Read PTY output in a blocking thread.
    let tx_read = tx.clone();
    let read_handle = tokio::task::spawn_blocking(move || {
        use std::io::Read;
        let mut buf = [0u8; 4096];
        let mut reader = reader;
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if tx_read.send(RunnerEvent::Bytes(buf[..n].to_vec())).is_err() {
                        break;
                    }
                }
            }
        }
    });

    // Forward resize events to the PTY master (master is moved here).
    drop(tokio::spawn(async move {
        use portable_pty::PtySize;
        let mut resize_rx = resize_rx;
        while let Some((cols, rows)) = resize_rx.recv().await {
            let _ = master.resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            });
        }
    }));

    // Wait for the child to exit in a blocking task.
    let (done_tx, done_rx) = tokio::sync::oneshot::channel::<u32>();
    tokio::task::spawn_blocking(move || {
        let code = child
            .wait()
            .map(|s| if s.success() { 0u32 } else { 1u32 })
            .unwrap_or(1u32);
        let _ = done_tx.send(code);
    });

    let (was_killed, exit_code) = tokio::select! {
        result = done_rx => (false, Some(result.unwrap_or(1))),
        _ = kill_rx => (true, None),
    };

    if was_killed {
        let _ = killer.kill();
    }

    // Drain remaining PTY output with a short timeout (same as runner_task).
    let _ = tokio::time::timeout(Duration::from_millis(500), read_handle).await;

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

/// Parse a Claude CLI cost/token line and extract token counts and cost.
///
/// Observed format (run with RALPH_DEBUG_PTY=1 to capture and verify from .ralph/pty-debug.log):
///   `Cost: $<amount> (<n> input, <n> output[, <n> cache read, <n> cache write] tokens)`
///
/// Examples:
///   `Cost: $0.0123 (1,234 input, 567 output tokens)`
///   `Cost: $0.0456 (10,000 input, 2,500 output, 500 cache read, 100 cache write tokens)`
///
/// Numbers may use comma thousands-separators (e.g. `1,234`).
/// The input string must already have ANSI escape sequences stripped.
///
/// Returns `Some((input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, cost_usd))`
/// or `None` if the line cannot be parsed.
fn parse_token_line(s: &str) -> Option<(u64, u64, u64, u64, f64)> {
    const COST_PREFIX: &str = "Cost: $";

    let cost_start = s.find(COST_PREFIX)?;
    let rest = &s[cost_start + COST_PREFIX.len()..];

    // Find the opening parenthesis to separate the cost amount from token counts.
    let paren_idx = rest.find('(')?;
    let amount_str = rest[..paren_idx].trim();
    let cost_usd: f64 = amount_str.parse().ok()?;

    // Extract content between parentheses.
    let tokens_part = &rest[paren_idx + 1..];
    let closing_paren = tokens_part.find(')')?;
    let tokens_str = &tokens_part[..closing_paren];

    // Extract each labeled count by searching for known label substrings.
    // This avoids splitting on commas, which also appear inside thousands-separated numbers.
    let input_tokens = extract_labeled_count(tokens_str, " input")?;
    let output_tokens = extract_labeled_count(tokens_str, " output")?;
    let cache_read_tokens = extract_labeled_count(tokens_str, " cache read").unwrap_or(0);
    let cache_write_tokens = extract_labeled_count(tokens_str, " cache write").unwrap_or(0);

    Some((input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, cost_usd))
}

/// Extract the numeric token count immediately preceding `label` in `s`.
///
/// For example, `extract_labeled_count("1,234 input, 567 output tokens", " input")` → `Some(1234)`.
/// Walks backwards from the label position collecting digits and commas, then parses the result.
fn extract_labeled_count(s: &str, label: &str) -> Option<u64> {
    let label_pos = s.find(label)?;
    let prefix = &s[..label_pos];
    // Walk backwards collecting digit and comma characters (thousands separators).
    let num_str: String = prefix
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit() || *c == ',')
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    if num_str.is_empty() {
        return None;
    }
    num_str.replace(',', "").parse::<u64>().ok()
}

/// Strip ANSI/VT100 escape sequences from a string, returning plain text.
///
/// Removes:
/// - CSI sequences: `ESC [` … final byte (0x40–0x7E)
/// - OSC sequences: `ESC ]` … BEL (0x07) or ST (`ESC \`)
/// - Bare ESC bytes for any other sequence type (only the ESC byte is dropped)
///
/// The input must be valid UTF-8. Multi-byte characters are preserved unchanged.
/// All escape-sequence bytes are ASCII so iteration stays on char boundaries.
fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            i += 1;
            if i >= bytes.len() {
                break;
            }
            match bytes[i] {
                b'[' => {
                    // CSI: consume until a final byte in range 0x40–0x7E.
                    i += 1;
                    while i < bytes.len() {
                        let b = bytes[i];
                        i += 1;
                        if (0x40..=0x7e).contains(&b) {
                            break;
                        }
                    }
                }
                b']' => {
                    // OSC: consume until BEL (0x07) or ST (ESC \).
                    i += 1;
                    while i < bytes.len() {
                        if bytes[i] == 0x07 {
                            i += 1;
                            break;
                        }
                        if bytes[i] == 0x1b {
                            i += 1;
                            if i < bytes.len() && bytes[i] == b'\\' {
                                i += 1;
                            }
                            break;
                        }
                        i += 1;
                    }
                }
                _ => {
                    // Unknown escape type: drop only the ESC byte; reprocess the next byte.
                }
            }
        } else {
            // Push the next Unicode scalar value and advance by its byte length.
            // SAFETY: `i` is always on a char boundary (all consumed escape bytes are ASCII).
            if let Some(c) = s[i..].chars().next() {
                result.push(c);
                i += c.len_utf8();
            } else {
                i += 1;
            }
        }
    }
    result
}

/// Which pane of the PRDs tab has keyboard focus.
#[derive(Debug, Clone, PartialEq)]
pub enum PrdsFocus {
    List,
    Content,
}

/// State for the PRDs read-only tab.
pub struct PrdsTab {
    /// Filenames (not full paths) of `.md` files found in `tasks/`, sorted alphabetically.
    pub files: Vec<String>,
    /// Index of the currently selected file, or `None` when the list is empty.
    pub selected: Option<usize>,
    /// Full text content of the selected file.
    pub content: String,
    /// Scroll offset (lines from top) for the content pane.
    pub scroll: u16,
    /// Which pane currently has keyboard focus.
    pub focus: PrdsFocus,
}

pub struct App {
    pub running: bool,
    pub store: Store,
    pub workflows: Vec<String>,
    pub selected_workflow: Option<usize>,
    pub current_workflow: Option<Workflow>,
    /// All open runner tabs (tab 0 is the PRDs tab, tab 1 is the Workflows tab).
    pub runner_tabs: Vec<RunnerTab>,
    /// 0 = PRDs tab; 1 = Workflows tab; 2..=1+runner_tabs.len() = runner tab at index active_tab-2.
    pub active_tab: usize,
    /// State for the PRDs tab.
    pub prds_tab: PrdsTab,
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
    /// When `Some`, the full-screen PRD metadata editor is active.
    /// All key input is routed to the editor; normal TUI is hidden.
    pub prd_editor: Option<PrdEditorState>,
    /// VT100 parser for synthesis subprocess output.
    /// `None` until the first synthesis has been started.
    pub synth_parser: Option<VtParser>,
    /// Receiver for synthesis subprocess events; `Some` only while synthesis is running.
    pub synth_rx: Option<UnboundedReceiver<RunnerEvent>>,
    /// Kill signal sender for the synthesis subprocess.
    pub synth_kill_tx: Option<oneshot::Sender<()>>,
    /// Name of the workflow currently being (or last) synthesized.
    pub synth_workflow_name: Option<String>,
}

impl App {
    pub fn new(store: Store, initial_size: (u16, u16)) -> Self {
        let workflows = store.list_workflows();
        let selected_workflow = if workflows.is_empty() { None } else { Some(0) };

        // Capture the root path before `store` is moved into the App struct.
        let root = store.root().to_path_buf();

        // Start OS-native file watcher. Gracefully degrade if it fails.
        let (watcher_tx, watcher_rx) = tokio::sync::mpsc::channel::<WatcherEvent>(64);
        let (watcher_opt, watcher_rx_opt, watcher_warning) = match Watcher::start(&root, watcher_tx)
        {
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
            prds_tab: PrdsTab {
                files: Vec::new(),
                selected: None,
                content: String::new(),
                scroll: 0,
                focus: PrdsFocus::List,
            },
            tab_nav_pending: false,
            dialog: None,
            status_message: watcher_warning,
            status_message_expires: None,
            initial_size,
            resize_txs: Vec::new(),
            watcher_rx: watcher_rx_opt,
            _watcher: watcher_opt,
            notification: None,
            prd_editor: None,
            synth_parser: None,
            synth_rx: None,
            synth_kill_tx: None,
            synth_workflow_name: None,
        };
        app.load_current_workflow();
        app.load_prds_files();
        app
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while self.running {
            self.check_status_timeout();
            self.drain_runner_channels();
            self.drain_synth_channel();
            let watcher_events = self.drain_watcher_channel();
            if !watcher_events.is_empty() {
                let first_path = watcher_events.first().map(|e| e.path.clone());
                self.reload_all();
                if let Some(path) = first_path {
                    let root = self.store.root().to_path_buf();
                    let rel = path
                        .strip_prefix(&root)
                        .map(|p| p.to_path_buf())
                        .unwrap_or(path);
                    self.notification =
                        Some((format!("↻ {} reloaded", rel.display()), Instant::now()));
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
                let pty_rows = rows.saturating_sub(PTY_ROW_OVERHEAD);
                // Broadcast new size to all active PTY runners; prune dead senders.
                self.resize_txs.retain(|tx| tx.send((cols, pty_rows)).is_ok());
                // Recreate each RunnerTab's vt100::Parser with the new dimensions.
                // vt100::Parser has no resize() method, so a new parser is created.
                // Known limitation: the screen state is cleared on resize — scrollback is not replayed.
                for tab in &mut self.runner_tabs {
                    tab.parser = VtParser::new(pty_rows, cols, 1000);
                    tab.log_scroll = 0;
                }
                // Recreate synthesis parser with new dimensions (same limitation as runner tabs).
                if self.synth_parser.is_some() {
                    self.synth_parser = Some(VtParser::new(pty_rows, cols, 1000));
                }
                self.initial_size = (cols, rows);
            }
            Event::Key(key) => {
                #[allow(clippy::collapsible_else_if)]
                if self.prd_editor.is_some() {
                    self.handle_prd_editor_key(key);
                } else if self.dialog.is_some() {
                    self.handle_dialog_key(key.code);
                } else if self.tab_nav_pending {
                    // Consume the chord: always clear the flag, then act.
                    self.tab_nav_pending = false;
                    self.handle_tab_nav_key(key.code);
                } else if self.active_tab == 0 {
                    // PRDs tab keybindings.
                    match key.code {
                        KeyCode::Char('t') => self.tab_nav_pending = true,
                        KeyCode::Tab => {
                            let total_tabs = 2 + self.runner_tabs.len();
                            self.active_tab = (self.active_tab + 1) % total_tabs;
                        }
                        KeyCode::BackTab => {
                            let total_tabs = 2 + self.runner_tabs.len();
                            self.active_tab = if self.active_tab == 0 {
                                total_tabs - 1
                            } else {
                                self.active_tab - 1
                            };
                        }
                        KeyCode::Char('q') => self.dialog = Some(Dialog::QuitConfirm),
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.running = false;
                        }
                        _ => match self.prds_tab.focus {
                            PrdsFocus::List => match key.code {
                                KeyCode::Down | KeyCode::Char('j') => {
                                    if !self.prds_tab.files.is_empty() {
                                        let next = match self.prds_tab.selected {
                                            None => 0,
                                            Some(i) => (i + 1) % self.prds_tab.files.len(),
                                        };
                                        self.select_prds_file(next);
                                    }
                                }
                                KeyCode::Up | KeyCode::Char('k') => {
                                    if !self.prds_tab.files.is_empty() {
                                        let prev = match self.prds_tab.selected {
                                            None => 0,
                                            Some(0) => self.prds_tab.files.len() - 1,
                                            Some(i) => i - 1,
                                        };
                                        self.select_prds_file(prev);
                                    }
                                }
                                KeyCode::Enter => {
                                    self.prds_tab.focus = PrdsFocus::Content;
                                }
                                _ => {}
                            },
                            PrdsFocus::Content => match key.code {
                                KeyCode::Down | KeyCode::Char('j') => {
                                    let line_count =
                                        self.prds_tab.content.lines().count();
                                    let (_, rows) = self.initial_size;
                                    // Layout: 1 tab bar + flexible content + 1 status bar
                                    // + 2 content border = 4 fixed lines consumed.
                                    let visible_lines =
                                        (rows as usize).saturating_sub(4);
                                    let max_scroll =
                                        line_count.saturating_sub(visible_lines) as u16;
                                    self.prds_tab.scroll =
                                        (self.prds_tab.scroll + 1).min(max_scroll);
                                }
                                KeyCode::Up | KeyCode::Char('k') => {
                                    self.prds_tab.scroll =
                                        self.prds_tab.scroll.saturating_sub(1);
                                }
                                KeyCode::Esc => {
                                    self.prds_tab.focus = PrdsFocus::List;
                                }
                                _ => {}
                            },
                        },
                    }
                } else if self.active_tab == 1 {
                    // Workflows tab keybindings.
                    match key.code {
                        KeyCode::Char('t') => self.tab_nav_pending = true,
                        KeyCode::Tab => {
                            let total_tabs = 2 + self.runner_tabs.len();
                            self.active_tab = (self.active_tab + 1) % total_tabs;
                            if self.active_tab == 0 {
                                self.load_prds_files();
                            }
                        }
                        KeyCode::BackTab => {
                            let total_tabs = 2 + self.runner_tabs.len();
                            self.active_tab = if self.active_tab == 0 {
                                total_tabs - 1
                            } else {
                                self.active_tab - 1
                            };
                            if self.active_tab == 0 {
                                self.load_prds_files();
                            }
                        }
                        KeyCode::Char('q') => self.dialog = Some(Dialog::QuitConfirm),
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.running = false;
                        }
                        KeyCode::Up | KeyCode::Char('k') => self.move_up(),
                        KeyCode::Down | KeyCode::Char('j') => self.move_down(),
                        // [r]un is disabled while synthesis is in progress.
                        KeyCode::Char('r') => {
                            if !self.is_synthesizing() {
                                self.start_runner();
                            }
                        }
                        // [s]top: stops synthesis if running, otherwise stops the active runner.
                        KeyCode::Char('s') => {
                            if self.is_synthesizing() {
                                self.stop_synthesizing();
                            } else {
                                self.stop_runner();
                            }
                        }
                        // Shift+S: trigger prd-synth synthesis for the selected workflow.
                        KeyCode::Char('S') => self.start_synthesizing(),
                        KeyCode::Char('n') => self.open_new_workflow_dialog(),
                        KeyCode::Char('i') => self.open_import_prd_dialog(),
                        KeyCode::Char('e') => self.edit_current_plan(terminal)?,
                        KeyCode::Char('E') => self.open_prd_editor(),
                        KeyCode::Char('d') => self.open_delete_workflow_dialog(),
                        KeyCode::Char('?') => self.open_help_dialog(),
                        _ => {}
                    }
                } else {
                    // Runner tab keybindings.
                    // Read insert_mode via copy before any mutable borrow to avoid borrow conflicts.
                    let tab_idx = self.active_tab - 2;
                    let insert_mode = self
                        .runner_tabs
                        .get(tab_idx)
                        .map(|t| t.insert_mode)
                        .unwrap_or(false);

                    if insert_mode {
                        // Insert mode: Esc exits, Ctrl+C sends interrupt to PTY, all other keys forwarded.
                        match key.code {
                            KeyCode::Esc => {
                                if let Some(tab) = self.runner_tabs.get_mut(tab_idx) {
                                    tab.insert_mode = false;
                                }
                            }
                            KeyCode::Char('c')
                                if key.modifiers.contains(KeyModifiers::CONTROL) =>
                            {
                                if let Some(tab) = self.runner_tabs.get(tab_idx)
                                    && let Some(tx) = &tab.stdin_tx
                                {
                                    let _ = tx.send(vec![0x03]);
                                }
                            }
                            _ => {
                                if let Some(bytes) = key_to_pty_bytes(key)
                                    && let Some(tab) = self.runner_tabs.get(tab_idx)
                                    && let Some(tx) = &tab.stdin_tx
                                {
                                    let _ = tx.send(bytes);
                                }
                            }
                        }
                    } else {
                        // Normal mode keybindings.
                        // Keys NOT forwarded to PTY: i, t, q, Ctrl+C, s, x, a, ?, k/Up, j/Down, G/End.
                        // All other keys are forwarded as raw bytes via key_to_pty_bytes.
                        match key.code {
                            KeyCode::Char('i') => {
                                if let Some(tab) = self.runner_tabs.get_mut(tab_idx) {
                                    tab.insert_mode = true;
                                }
                            }
                            KeyCode::Char('t') => self.tab_nav_pending = true,
                            KeyCode::Char('q') => self.dialog = Some(Dialog::QuitConfirm),
                            KeyCode::Char('?') => self.dialog = Some(Dialog::RunnerHelp),
                            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                self.running = false;
                            }
                            KeyCode::Char('s') => {
                                let (is_running, workflow_name) = self
                                    .runner_tabs
                                    .get(tab_idx)
                                    .map(|t| {
                                        (
                                            matches!(t.state, RunnerTabState::Running { .. }),
                                            t.workflow_name.clone(),
                                        )
                                    })
                                    .unwrap_or((false, String::new()));
                                if is_running {
                                    if self.is_workflow_complete(&workflow_name) {
                                        self.stop_runner();
                                    } else {
                                        self.dialog = Some(Dialog::StopConfirm);
                                    }
                                }
                            }
                            // [r]estart: re-spawn the runner from current plan state when Stopped.
                            // No-op in Running or Done states (button not shown in those states).
                            KeyCode::Char('r') => {
                                let is_stopped = self
                                    .runner_tabs
                                    .get(tab_idx)
                                    .map(|t| matches!(t.state, RunnerTabState::Stopped))
                                    .unwrap_or(false);
                                if is_stopped {
                                    self.restart_runner_at(tab_idx);
                                }
                            }
                            // [c]ontinue: advance to the next task when Done and auto_continue=false.
                            // No-op when Running or when auto_continue=true.
                            KeyCode::Char('c') => {
                                let can_continue = self
                                    .runner_tabs
                                    .get(tab_idx)
                                    .map(|t| {
                                        matches!(
                                            t.state,
                                            RunnerTabState::Done | RunnerTabState::Stopped
                                        ) && !t.auto_continue
                                    })
                                    .unwrap_or(false);
                                if can_continue {
                                    self.spawn_next_iteration_at(tab_idx);
                                }
                            }
                            KeyCode::Char('a') => {
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
                                    // Move to the previous tab; saturating_sub(1) gives 1 (Workflows)
                                    // when active_tab was 2 (the only runner tab).
                                    self.active_tab = self.active_tab.saturating_sub(1);
                                }
                            }
                            // Log scroll: Up/k scroll up (into scrollback), Down/j scroll down.
                            // log_scroll == 0 means auto-scroll (live vt100 screen).
                            // log_scroll == N means N rows of scrollback are shown above the screen.
                            // The scrollback position is kept in sync on the vt100 parser's screen so
                            // that PseudoTerminal renders the correct view without needing &mut in draw.
                            KeyCode::Up | KeyCode::Char('k') => {
                                if let Some(tab) = self.runner_tabs.get_mut(tab_idx) {
                                    // Cap at the configured scrollback size (1000 rows).
                                    tab.log_scroll = (tab.log_scroll + 1).min(1000);
                                    tab.parser.screen_mut().set_scrollback(tab.log_scroll);
                                }
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                if let Some(tab) = self.runner_tabs.get_mut(tab_idx) {
                                    tab.log_scroll = tab.log_scroll.saturating_sub(1);
                                    tab.parser.screen_mut().set_scrollback(tab.log_scroll);
                                }
                            }
                            // End or G re-enables auto-scroll (live screen, scrollback = 0).
                            KeyCode::End | KeyCode::Char('G') => {
                                if let Some(tab) = self.runner_tabs.get_mut(tab_idx) {
                                    tab.log_scroll = 0;
                                    tab.parser.screen_mut().set_scrollback(0);
                                }
                            }
                            KeyCode::Tab => {
                                let total_tabs = 2 + self.runner_tabs.len();
                                self.active_tab = (self.active_tab + 1) % total_tabs;
                                if self.active_tab == 0 {
                                    self.load_prds_files();
                                }
                            }
                            KeyCode::BackTab => {
                                let total_tabs = 2 + self.runner_tabs.len();
                                self.active_tab = if self.active_tab == 0 {
                                    total_tabs - 1
                                } else {
                                    self.active_tab - 1
                                };
                                // Runner BackTab can never reach 0 (runner index >= 2),
                                // so no load_prds_files call needed here.
                            }
                            // Normal mode: unrecognized keys are ignored (use Insert mode to type freely).
                            _ => {}
                        }
                    }
                } // closes else { block
            } // closes Event::Key(key) => { arm body
            _ => {} // other events (mouse, focus, paste, …) are ignored
        } // closes match event::read()?
        Ok(())
    }

    /// Handles the second key of a `t`-prefix tab navigation chord.
    ///
    /// Digits `1`–`9` jump to the tab at index `digit − 1` (0 = PRDs, 1 = Workflows, 2+ = runners).
    /// Any other key is silently ignored (flag was already cleared by the caller).
    fn handle_tab_nav_key(&mut self, code: KeyCode) {
        let total_tabs = 2 + self.runner_tabs.len(); // PRDs tab + Workflows tab + runner tabs
        match code {
            KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
                let idx = (c as usize) - ('1' as usize); // digit '1' → 0, '9' → 8
                if idx < total_tabs {
                    self.active_tab = idx;
                    if idx == 0 {
                        self.load_prds_files();
                    }
                }
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
        // QuitConfirm: y/Y exits the application, any other key cancels.
        if matches!(self.dialog, Some(Dialog::QuitConfirm)) {
            self.dialog = None;
            if code == KeyCode::Char('y') || code == KeyCode::Char('Y') {
                self.running = false;
            }
            return;
        }

        // StopConfirm: y/Y stops the runner (-> Stopped state), any other key cancels.
        if matches!(self.dialog, Some(Dialog::StopConfirm)) {
            self.dialog = None;
            if code == KeyCode::Char('y') || code == KeyCode::Char('Y') {
                self.stop_runner();
            }
            return;
        }

        // Help overlays: any key closes them.
        if matches!(self.dialog, Some(Dialog::Help) | Some(Dialog::RunnerHelp)) {
            self.dialog = None;
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

        // ImportPrd dialog.
        if matches!(self.dialog, Some(Dialog::ImportPrd { .. })) {
            let (workflow_name, input, confirm_overwrite) = match &self.dialog {
                Some(Dialog::ImportPrd {
                    workflow_name,
                    input,
                    confirm_overwrite,
                    ..
                }) => (workflow_name.clone(), input.clone(), *confirm_overwrite),
                _ => return,
            };

            if confirm_overwrite {
                // Overwrite confirmation: y proceeds, anything else cancels.
                self.dialog = None;
                if matches!(code, KeyCode::Char('y') | KeyCode::Char('Y')) {
                    self.do_import_prd_copy(&workflow_name, &input);
                }
                return;
            }

            // Path input phase.
            match code {
                KeyCode::Esc => {
                    self.dialog = None;
                }
                KeyCode::Backspace => {
                    if let Some(Dialog::ImportPrd { input, error, .. }) = &mut self.dialog {
                        input.pop();
                        *error = None;
                    }
                }
                KeyCode::Char(c) => {
                    if let Some(Dialog::ImportPrd { input, error, .. }) = &mut self.dialog {
                        input.push(c);
                        *error = None;
                    }
                }
                KeyCode::Enter => {
                    self.handle_import_prd_submit(&workflow_name, &input);
                }
                _ => {}
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

    /// Opens the full-screen PRD metadata editor for the currently selected workflow.
    /// Pre-populates all fields from the on-disk prd.json.
    fn open_prd_editor(&mut self) {
        let Some(idx) = self.selected_workflow else {
            return;
        };
        let Some(name) = self.workflows.get(idx).cloned() else {
            return;
        };

        let dir = self.store.workflow_dir(&name);
        let workflow = match Workflow::load(&dir) {
            Ok(w) => w,
            Err(e) => {
                self.status_message = Some(format!("Failed to load workflow: {e}"));
                return;
            }
        };

        let selected_story = if workflow.prd.tasks.is_empty() {
            None
        } else {
            Some(0)
        };
        self.prd_editor = Some(PrdEditorState {
            workflow_name: name,
            project: workflow.prd.project.clone(),
            branch: workflow.prd.branch_name.clone(),
            description: workflow.prd.description.clone(),
            focused_field: PrdEditorField::Project,
            mode: PrdEditorMode::Metadata,
            stories: workflow.prd.tasks.clone(),
            selected_story,
            is_new_story: false,
            confirm_delete: None,
            status: None,
            validation_commands: workflow.prd.validation_commands.clone(),
            validation_commands_cursor: 0,
            // Story detail fields — populated when entering StoryDetail mode.
            story_id: String::new(),
            story_title: String::new(),
            story_description: String::new(),
            story_priority: String::new(),
            story_criteria: Vec::new(),
            story_criteria_cursor: 0,
            story_focused_field: StoryDetailField::Id,
        });
    }

    /// Writes the current editor state back to prd.json.
    /// Saves project, branch, description, and the full stories list.
    /// On success closes the editor; on error shows the message in the status line.
    fn save_prd_editor(&mut self) {
        // Clone the values we need before releasing the immutable borrow.
        let (name, project, branch, description, stories, validation_commands) = match &self.prd_editor {
            Some(e) => (
                e.workflow_name.clone(),
                e.project.clone(),
                e.branch.clone(),
                e.description.clone(),
                e.stories.clone(),
                e.validation_commands.clone(),
            ),
            None => return,
        };

        let dir = self.store.workflow_dir(&name);
        match Workflow::load(&dir) {
            Ok(mut workflow) => {
                workflow.prd.project = project;
                workflow.prd.branch_name = branch;
                workflow.prd.description = description;
                workflow.prd.tasks = stories;
                workflow.prd.validation_commands = validation_commands;
                match workflow.save(&dir) {
                    Ok(()) => {
                        // Verify the saved file is valid JSON and can be deserialized.
                        match Workflow::load(&dir) {
                            Ok(_) => {
                                self.prd_editor = None;
                                self.load_current_workflow();
                            }
                            Err(e) => {
                                if let Some(editor) = &mut self.prd_editor {
                                    editor.status = Some(format!("Save verification failed: {e}"));
                                }
                            }
                        }
                    }
                    Err(e) => {
                        if let Some(editor) = &mut self.prd_editor {
                            editor.status = Some(format!("Save failed: {e}"));
                        }
                    }
                }
            }
            Err(e) => {
                if let Some(editor) = &mut self.prd_editor {
                    editor.status = Some(format!("Load failed: {e}"));
                }
            }
        }
    }

    /// Handles a key event while the PRD editor is open.
    ///
    /// Dispatch order:
    ///   1. Delete confirmation overlay (if active) — consumes all keys.
    ///   2. Global bindings: Esc (close / go back), Ctrl+S (save).
    ///   3. Mode-specific handlers: Metadata, StoryList, StoryDetail.
    fn handle_prd_editor_key(&mut self, key: KeyEvent) {
        // Extract mode and confirm_delete without holding a borrow.
        let (mode, confirm_delete) = match &self.prd_editor {
            Some(e) => (e.mode.clone(), e.confirm_delete),
            None => return,
        };

        // Delete confirmation overlay: y confirms, anything else cancels.
        if let Some(del_idx) = confirm_delete {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    if let Some(editor) = &mut self.prd_editor {
                        editor.stories.remove(del_idx);
                        editor.selected_story = if editor.stories.is_empty() {
                            None
                        } else {
                            Some(del_idx.min(editor.stories.len() - 1))
                        };
                        editor.confirm_delete = None;
                        editor.status = None;
                    }
                }
                _ => {
                    if let Some(editor) = &mut self.prd_editor {
                        editor.confirm_delete = None;
                    }
                }
            }
            return;
        }

        // Global bindings.
        match key.code {
            KeyCode::Esc => {
                match mode {
                    // US-003: Esc from story detail returns to story list without saving.
                    PrdEditorMode::StoryDetail => {
                        if let Some(editor) = &mut self.prd_editor {
                            editor.mode = PrdEditorMode::StoryList;
                        }
                    }
                    // Esc from metadata or story list closes the editor.
                    PrdEditorMode::Metadata | PrdEditorMode::StoryList => {
                        self.prd_editor = None;
                    }
                }
                return;
            }
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if mode == PrdEditorMode::StoryDetail {
                    self.save_story_detail();
                } else {
                    self.save_prd_editor();
                }
                return;
            }
            _ => {}
        }

        // Mode-specific key handling.
        match mode {
            PrdEditorMode::Metadata => self.handle_prd_editor_metadata_key(key),
            PrdEditorMode::StoryList => self.handle_prd_story_list_key(key),
            PrdEditorMode::StoryDetail => self.handle_prd_story_detail_key(key),
        }
    }

    /// Handles key events when the metadata section (Project / Branch / Description) is active.
    fn handle_prd_editor_metadata_key(&mut self, key: KeyEvent) {
        // Handle 'x' delete in ValidationCommands first (before general Char branch)
        if let KeyCode::Char('x') = key.code
            && let Some(editor) = &mut self.prd_editor
            && editor.focused_field == PrdEditorField::ValidationCommands
            && editor.validation_commands_cursor < editor.validation_commands.len()
        {
            editor.validation_commands.remove(editor.validation_commands_cursor);
            // Adjust cursor if we removed the last item
            if editor.validation_commands_cursor >= editor.validation_commands.len()
                && editor.validation_commands_cursor > 0
            {
                editor.validation_commands_cursor -= 1;
            }
            editor.status = None;
            return;
        }

        match key.code {
            KeyCode::Tab => {
                if let Some(editor) = &mut self.prd_editor {
                    match editor.focused_field {
                        PrdEditorField::Project => editor.focused_field = PrdEditorField::Branch,
                        PrdEditorField::Branch => {
                            editor.focused_field = PrdEditorField::Description;
                        }
                        PrdEditorField::Description => {
                            editor.focused_field = PrdEditorField::ValidationCommands;
                        }
                        PrdEditorField::ValidationCommands => {
                            // Advance past the last metadata field into the story list.
                            editor.mode = PrdEditorMode::StoryList;
                        }
                    }
                }
            }
            KeyCode::BackTab => {
                if let Some(editor) = &mut self.prd_editor {
                    match editor.focused_field {
                        PrdEditorField::Project => {
                            // Wrap backwards into the story list.
                            editor.mode = PrdEditorMode::StoryList;
                        }
                        PrdEditorField::Branch => editor.focused_field = PrdEditorField::Project,
                        PrdEditorField::Description => {
                            editor.focused_field = PrdEditorField::Branch;
                        }
                        PrdEditorField::ValidationCommands => {
                            editor.focused_field = PrdEditorField::Description;
                        }
                    }
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(editor) = &mut self.prd_editor
                    && editor.focused_field == PrdEditorField::ValidationCommands
                    && editor.validation_commands_cursor > 0
                {
                    editor.validation_commands_cursor -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(editor) = &mut self.prd_editor
                    && editor.focused_field == PrdEditorField::ValidationCommands
                {
                    let len = editor.validation_commands.len();
                    if editor.validation_commands_cursor + 1 < len {
                        editor.validation_commands_cursor += 1;
                    }
                }
            }
            KeyCode::Enter => {
                if let Some(editor) = &mut self.prd_editor
                    && editor.focused_field == PrdEditorField::ValidationCommands
                {
                    if editor.validation_commands.is_empty() {
                        editor.validation_commands.push(String::new());
                    } else {
                        let insert_pos = editor.validation_commands_cursor + 1;
                        editor.validation_commands.insert(insert_pos, String::new());
                        editor.validation_commands_cursor = insert_pos;
                    }
                }
            }
            KeyCode::Backspace => {
                if let Some(editor) = &mut self.prd_editor {
                    match editor.focused_field {
                        PrdEditorField::Project => {
                            editor.project.pop();
                        }
                        PrdEditorField::Branch => {
                            editor.branch.pop();
                        }
                        PrdEditorField::Description => {
                            editor.description.pop();
                        }
                        PrdEditorField::ValidationCommands => {
                            if editor.validation_commands_cursor < editor.validation_commands.len() {
                                editor.validation_commands[editor.validation_commands_cursor].pop();
                            }
                        }
                    }
                    editor.status = None;
                }
            }
            KeyCode::Char(c) => {
                if let Some(editor) = &mut self.prd_editor {
                    match editor.focused_field {
                        PrdEditorField::Project => editor.project.push(c),
                        PrdEditorField::Branch => editor.branch.push(c),
                        PrdEditorField::Description => editor.description.push(c),
                        PrdEditorField::ValidationCommands => {
                            if editor.validation_commands_cursor < editor.validation_commands.len() {
                                editor.validation_commands[editor.validation_commands_cursor].push(c);
                            }
                        }
                    }
                    editor.status = None;
                }
            }
            _ => {}
        }
    }

    /// Handles key events when the story list panel is active.
    fn handle_prd_story_list_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(editor) = &mut self.prd_editor
                    && let Some(sel) = editor.selected_story
                    && sel > 0
                {
                    editor.selected_story = Some(sel - 1);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(editor) = &mut self.prd_editor {
                    let len = editor.stories.len();
                    match editor.selected_story {
                        Some(sel) if sel + 1 < len => {
                            editor.selected_story = Some(sel + 1);
                        }
                        None if len > 0 => {
                            editor.selected_story = Some(0);
                        }
                        _ => {}
                    }
                }
            }
            KeyCode::Enter => {
                // Populate story detail fields from the selected story and enter StoryDetail mode.
                if let Some(editor) = &mut self.prd_editor
                    && let Some(sel) = editor.selected_story
                    && let Some(story) = editor.stories.get(sel).cloned()
                {
                    editor.story_id = story.id;
                    editor.story_title = story.title;
                    editor.story_description = story.description;
                    editor.story_priority = story.priority.to_string();
                    editor.story_criteria = if story.acceptance_criteria.is_empty() {
                        vec![String::new()]
                    } else {
                        story.acceptance_criteria
                    };
                    editor.story_criteria_cursor = 0;
                    editor.story_focused_field = StoryDetailField::Id;
                    editor.is_new_story = false;
                    editor.mode = PrdEditorMode::StoryDetail;
                    editor.status = None;
                }
            }
            KeyCode::Char('a') => {
                // Open an empty story detail form for a new story.
                if let Some(editor) = &mut self.prd_editor {
                    let next_num = editor.stories.len() + 1;
                    editor.story_id = format!("US-{next_num:03}");
                    editor.story_title = String::new();
                    editor.story_description = String::new();
                    editor.story_priority = next_num.to_string();
                    editor.story_criteria = vec![String::new()];
                    editor.story_criteria_cursor = 0;
                    editor.story_focused_field = StoryDetailField::Id;
                    editor.is_new_story = true;
                    editor.mode = PrdEditorMode::StoryDetail;
                    editor.status = None;
                }
            }
            KeyCode::Char('x') => {
                // Show delete confirmation for the currently selected story.
                if let Some(editor) = &mut self.prd_editor
                    && editor.selected_story.is_some()
                {
                    editor.confirm_delete = editor.selected_story;
                }
            }
            KeyCode::Tab => {
                // Move focus back to the metadata section (wrap to Project).
                if let Some(editor) = &mut self.prd_editor {
                    editor.mode = PrdEditorMode::Metadata;
                    editor.focused_field = PrdEditorField::Project;
                }
            }
            KeyCode::BackTab => {
                // Move focus back to the metadata section (Description).
                if let Some(editor) = &mut self.prd_editor {
                    editor.mode = PrdEditorMode::Metadata;
                    editor.focused_field = PrdEditorField::Description;
                }
            }
            _ => {}
        }
    }

    /// Handles key events when the story detail form is active.
    ///
    /// Field order (Tab): Id → Title → Description → Priority → Criteria → Id (wrap).
    /// BackTab reverses the order. Within the Criteria list, Up/Down move between lines,
    /// Enter inserts a new line below the cursor, and x deletes the focused line.
    fn handle_prd_story_detail_key(&mut self, key: KeyEvent) {
        // Clone focused field to avoid holding the borrow during the match.
        let focused = match &self.prd_editor {
            Some(e) => e.story_focused_field.clone(),
            None => return,
        };

        match key.code {
            KeyCode::Tab => {
                if let Some(editor) = &mut self.prd_editor {
                    editor.story_focused_field = match editor.story_focused_field {
                        StoryDetailField::Id => StoryDetailField::Title,
                        StoryDetailField::Title => StoryDetailField::Description,
                        StoryDetailField::Description => StoryDetailField::Priority,
                        StoryDetailField::Priority => StoryDetailField::Criteria,
                        StoryDetailField::Criteria => StoryDetailField::Id,
                    };
                }
            }
            KeyCode::BackTab => {
                if let Some(editor) = &mut self.prd_editor {
                    editor.story_focused_field = match editor.story_focused_field {
                        StoryDetailField::Id => StoryDetailField::Criteria,
                        StoryDetailField::Title => StoryDetailField::Id,
                        StoryDetailField::Description => StoryDetailField::Title,
                        StoryDetailField::Priority => StoryDetailField::Description,
                        StoryDetailField::Criteria => StoryDetailField::Priority,
                    };
                }
            }
            KeyCode::Up if focused == StoryDetailField::Criteria => {
                if let Some(editor) = &mut self.prd_editor
                    && editor.story_criteria_cursor > 0
                {
                    editor.story_criteria_cursor -= 1;
                }
            }
            KeyCode::Down if focused == StoryDetailField::Criteria => {
                if let Some(editor) = &mut self.prd_editor {
                    let max = editor.story_criteria.len().saturating_sub(1);
                    if editor.story_criteria_cursor < max {
                        editor.story_criteria_cursor += 1;
                    }
                }
            }
            KeyCode::Enter if focused == StoryDetailField::Criteria => {
                if let Some(editor) = &mut self.prd_editor {
                    if editor.story_criteria.is_empty() {
                        editor.story_criteria.push(String::new());
                        editor.story_criteria_cursor = 0;
                    } else {
                        let cursor = editor.story_criteria_cursor;
                        editor.story_criteria.insert(cursor + 1, String::new());
                        editor.story_criteria_cursor = cursor + 1;
                    }
                }
            }
            KeyCode::Backspace => {
                if let Some(editor) = &mut self.prd_editor {
                    match editor.story_focused_field {
                        StoryDetailField::Id => {
                            editor.story_id.pop();
                        }
                        StoryDetailField::Title => {
                            editor.story_title.pop();
                        }
                        StoryDetailField::Description => {
                            editor.story_description.pop();
                        }
                        StoryDetailField::Priority => {
                            editor.story_priority.pop();
                        }
                        StoryDetailField::Criteria => {
                            let cursor = editor.story_criteria_cursor;
                            if let Some(line) = editor.story_criteria.get_mut(cursor) {
                                line.pop();
                            }
                        }
                    }
                    editor.status = None;
                }
            }
            KeyCode::Char(c) => {
                if let Some(editor) = &mut self.prd_editor {
                    // x in the Criteria field deletes the focused criterion line.
                    if c == 'x'
                        && editor.story_focused_field == StoryDetailField::Criteria
                        && !editor.story_criteria.is_empty()
                    {
                        let cursor = editor.story_criteria_cursor;
                        editor.story_criteria.remove(cursor);
                        editor.story_criteria_cursor = if editor.story_criteria.is_empty() {
                            0
                        } else {
                            cursor.min(editor.story_criteria.len() - 1)
                        };
                    } else {
                        match editor.story_focused_field {
                            StoryDetailField::Id => editor.story_id.push(c),
                            StoryDetailField::Title => editor.story_title.push(c),
                            StoryDetailField::Description => editor.story_description.push(c),
                            StoryDetailField::Priority => editor.story_priority.push(c),
                            StoryDetailField::Criteria => {
                                if editor.story_criteria.is_empty() {
                                    editor.story_criteria.push(c.to_string());
                                } else {
                                    let cursor = editor.story_criteria_cursor;
                                    if let Some(line) = editor.story_criteria.get_mut(cursor) {
                                        line.push(c);
                                    }
                                }
                            }
                        }
                    }
                    editor.status = None;
                }
            }
            _ => {}
        }
    }

    /// Saves the story detail form back into the in-memory story list, then persists
    /// the full plan (metadata + all stories) to prd.json.  Returns to StoryList mode
    /// on success; shows an error in the hint line on failure.
    fn save_story_detail(&mut self) {
        // Build the Task from story detail fields (clone to release the borrow).
        let (workflow_name, task, project, branch, description, is_new, selected_idx) = {
            let editor = match &self.prd_editor {
                Some(e) => e,
                None => return,
            };
            let priority = editor.story_priority.parse::<u32>().unwrap_or(1);
            // Drop empty criterion lines before saving.
            let criteria: Vec<String> = editor
                .story_criteria
                .iter()
                .filter(|s| !s.is_empty())
                .cloned()
                .collect();
            // Preserve passes / notes from the original story when editing.
            let (passes, notes) = if editor.is_new_story {
                (false, String::new())
            } else {
                editor
                    .selected_story
                    .and_then(|i| editor.stories.get(i))
                    .map(|s| (s.passes, s.notes.clone()))
                    .unwrap_or((false, String::new()))
            };
            let task = Task {
                id: editor.story_id.clone(),
                title: editor.story_title.clone(),
                description: editor.story_description.clone(),
                acceptance_criteria: criteria,
                priority,
                passes,
                notes,
            };
            (
                editor.workflow_name.clone(),
                task,
                editor.project.clone(),
                editor.branch.clone(),
                editor.description.clone(),
                editor.is_new_story,
                editor.selected_story,
            )
        };

        // Update the in-memory story list and switch back to StoryList mode.
        if let Some(editor) = &mut self.prd_editor {
            if is_new {
                editor.stories.push(task.clone());
                let new_idx = editor.stories.len() - 1;
                editor.selected_story = Some(new_idx);
            } else if let Some(idx) = selected_idx
                && let Some(existing) = editor.stories.get_mut(idx)
            {
                *existing = task.clone();
            }
            editor.mode = PrdEditorMode::StoryList;
            editor.status = None;
        }

        // Persist updated metadata + stories to disk.
        let updated_stories = match &self.prd_editor {
            Some(e) => e.stories.clone(),
            None => return,
        };

        let dir = self.store.workflow_dir(&workflow_name);
        match Workflow::load(&dir) {
            Ok(mut workflow) => {
                workflow.prd.project = project;
                workflow.prd.branch_name = branch;
                workflow.prd.description = description;
                workflow.prd.tasks = updated_stories;
                match workflow.save(&dir) {
                    Ok(()) => {
                        // Verify the saved file is valid JSON and can be deserialized.
                        match Workflow::load(&dir) {
                            Ok(_) => {
                                self.load_current_workflow();
                            }
                            Err(e) => {
                                if let Some(editor) = &mut self.prd_editor {
                                    editor.status = Some(format!("Save verification failed: {e}"));
                                }
                            }
                        }
                    }
                    Err(e) => {
                        if let Some(editor) = &mut self.prd_editor {
                            editor.status = Some(format!("Save failed: {e}"));
                        }
                    }
                }
            }
            Err(e) => {
                if let Some(editor) = &mut self.prd_editor {
                    editor.status = Some(format!("Load failed: {e}"));
                }
            }
        }
    }

    fn open_new_workflow_dialog(&mut self) {
        self.dialog = Some(Dialog::NewWorkflow {
            input: String::new(),
            error: None,
        });
    }

    /// Opens the ImportPrd dialog for the currently selected workflow.
    fn open_import_prd_dialog(&mut self) {
        let Some(idx) = self.selected_workflow else {
            return;
        };
        let Some(name) = self.workflows.get(idx).cloned() else {
            return;
        };
        self.dialog = Some(Dialog::ImportPrd {
            workflow_name: name,
            input: String::new(),
            error: None,
            confirm_overwrite: false,
        });
    }

    /// Validates the path entered in the ImportPrd dialog and either copies the file,
    /// asks for overwrite confirmation, or shows an inline error.
    fn handle_import_prd_submit(&mut self, workflow_name: &str, input: &str) {
        // Resolve relative paths from the repo root.
        let path = if std::path::Path::new(input).is_absolute() {
            std::path::PathBuf::from(input)
        } else {
            self.store.root().join(input)
        };

        // Validate that the path exists.
        if !path.exists() {
            if let Some(Dialog::ImportPrd { error, .. }) = &mut self.dialog {
                *error = Some("File not found".to_string());
            }
            return;
        }

        // Validate .md extension.
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            if let Some(Dialog::ImportPrd { error, .. }) = &mut self.dialog {
                *error = Some("Not a .md file".to_string());
            }
            return;
        }

        // If prd-source.md already exists, prompt for overwrite confirmation.
        let dest = self.store.workflow_dir(workflow_name).join("prd-source.md");
        if dest.exists() {
            if let Some(Dialog::ImportPrd {
                confirm_overwrite,
                error,
                ..
            }) = &mut self.dialog
            {
                *confirm_overwrite = true;
                *error = None;
            }
            return;
        }

        // No conflict — copy immediately.
        self.do_import_prd_copy(workflow_name, input);
    }

    /// Copies the source markdown file to `<workflow_dir>/prd-source.md`.
    /// Closes the dialog and sets a notification on success, or sets a timed
    /// status message on failure.
    fn do_import_prd_copy(&mut self, workflow_name: &str, input: &str) {
        let path = if std::path::Path::new(input).is_absolute() {
            std::path::PathBuf::from(input)
        } else {
            self.store.root().join(input)
        };

        let dest = self.store.workflow_dir(workflow_name).join("prd-source.md");
        match std::fs::copy(&path, &dest) {
            Ok(_) => {
                self.dialog = None;
                self.notification = Some(("PRD imported".to_string(), Instant::now()));
            }
            Err(e) => {
                self.dialog = None;
                self.status_message = Some(format!("Import failed: {e}"));
                self.status_message_expires =
                    Some(Instant::now() + Duration::from_secs(3));
            }
        }
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
            Some(
                old_idx
                    .map(|i| i.min(self.workflows.len() - 1))
                    .unwrap_or(0),
            )
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

    /// Scans `<repo_root>/tasks/` for `.md` files and populates `prds_tab`.
    ///
    /// - Files are sorted alphabetically by filename.
    /// - If the directory does not exist or contains no `.md` files, `files` is
    ///   empty and `selected` is `None`.
    /// - If files are present, `selected` defaults to `Some(0)` and `content` is
    ///   set to the full text of the first file; `scroll` is reset to 0.
    pub fn load_prds_files(&mut self) {
        let tasks_dir = self.store.root().join("tasks");
        let mut files: Vec<String> = match std::fs::read_dir(&tasks_dir) {
            Ok(entries) => entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.path().extension().and_then(|s| s.to_str()) == Some("md")
                })
                .filter_map(|e| e.file_name().to_str().map(|s| s.to_owned()))
                .collect(),
            Err(_) => Vec::new(),
        };
        files.sort();

        if files.is_empty() {
            self.prds_tab.files = files;
            self.prds_tab.selected = None;
            self.prds_tab.content = String::new();
        } else {
            let first_path = tasks_dir.join(&files[0]);
            let content = std::fs::read_to_string(&first_path).unwrap_or_default();
            self.prds_tab.selected = Some(0);
            self.prds_tab.content = content;
            self.prds_tab.scroll = 0;
            self.prds_tab.files = files;
        }
    }

    /// Selects the file at `idx` in `prds_tab.files`, loads its content from disk,
    /// and resets the scroll offset to 0.
    ///
    /// # Panics
    /// Panics if `idx` is out of bounds for `prds_tab.files`.
    fn select_prds_file(&mut self, idx: usize) {
        let tasks_dir = self.store.root().join("tasks");
        let path = tasks_dir.join(&self.prds_tab.files[idx]);
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        self.prds_tab.selected = Some(idx);
        self.prds_tab.content = content;
        self.prds_tab.scroll = 0;
    }

    /// Returns `true` if all tasks in the named workflow have `passes == true`.
    /// Returns `false` if the workflow cannot be loaded from disk.
    pub fn is_workflow_complete(&self, workflow_name: &str) -> bool {
        let workflow_dir = self.store.workflow_dir(workflow_name);
        Workflow::load(&workflow_dir)
            .map(|w| w.is_complete())
            .unwrap_or(false)
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
    /// and reloads the displayed workflow.
    /// Does not interrupt active runner subprocesses.
    pub fn reload_all(&mut self) {
        // Remember the currently selected workflow name to restore after the list refresh.
        let old_name = self
            .selected_workflow
            .and_then(|i| self.workflows.get(i).cloned());

        // Refresh the workflow list from disk.
        self.workflows = self.store.list_workflows();

        // Restore selection: prefer the same workflow by name; fall back to first, or None.
        self.selected_workflow = match &old_name {
            Some(name) => {
                self.workflows
                    .iter()
                    .position(|p| p == name)
                    .or(if self.workflows.is_empty() {
                        None
                    } else {
                        Some(0)
                    })
            }
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
            let tab_workflow_name = (self.active_tab > 1)
                .then(|| self.runner_tabs.get(self.active_tab - 2))
                .flatten()
                .map(|t| t.workflow_name.clone());

            let task_still_pending = tab_workflow_name
                .as_ref()
                .map(|name| {
                    let dir = self.store.workflow_dir(name);
                    Workflow::load(&dir)
                        .ok()
                        .map(|w| {
                            w.prd
                                .tasks
                                .iter()
                                .any(|t| t.id == next_id_clone && !t.passes)
                        })
                        .unwrap_or(false)
                })
                .unwrap_or(false);

            if !task_still_pending {
                self.dialog = None;
                if self.active_tab > 1
                    && let Some(tab) = self.runner_tabs.get_mut(self.active_tab - 2)
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
        let mut token_usages: Vec<(u64, u64, u64, u64, f64)> = Vec::new();
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
                    Ok(RunnerEvent::TokenUsage {
                        input_tokens,
                        output_tokens,
                        cache_read_tokens,
                        cache_write_tokens,
                        cost_usd,
                    }) => {
                        token_usages.push((input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, cost_usd));
                    }
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

        // Accumulate token and cost totals for the current story.
        for (input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, cost_usd) in token_usages {
            self.runner_tabs[tab_idx].current_story_input_tokens =
                self.runner_tabs[tab_idx].current_story_input_tokens.saturating_add(input_tokens);
            self.runner_tabs[tab_idx].current_story_output_tokens =
                self.runner_tabs[tab_idx].current_story_output_tokens.saturating_add(output_tokens);
            self.runner_tabs[tab_idx].current_story_cache_read_tokens =
                self.runner_tabs[tab_idx].current_story_cache_read_tokens.saturating_add(cache_read_tokens);
            self.runner_tabs[tab_idx].current_story_cache_write_tokens =
                self.runner_tabs[tab_idx].current_story_cache_write_tokens.saturating_add(cache_write_tokens);
            self.runner_tabs[tab_idx].current_story_cost_usd += cost_usd;
        }

        // Feed raw bytes into the vt100 parser.
        for chunk in byte_chunks {
            self.runner_tabs[tab_idx].parser.process(&chunk);
        }

        // Track the Complete sentinel on the tab so the done block can use it
        // even after the local `complete` variable goes out of scope.
        if complete {
            self.runner_tabs[tab_idx].saw_complete = true;
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
                let is_complete = self.is_workflow_complete(&workflow_name);
                if is_complete {
                    self.runner_tabs[tab_idx].state = RunnerTabState::Done;
                    self.runner_tabs[tab_idx].insert_mode = false;
                } else {
                    // Kill the old process before spawning next. It sent the sentinel
                    // but has not exited yet. Mirrors the stop_runner() pattern.
                    if let Some(kill_tx) = self.runner_tabs[tab_idx].runner_kill_tx.take() {
                        let _ = kill_tx.send(());
                    }
                    self.spawn_next_iteration_at(tab_idx);
                }
            } else if !is_auto {
                // Original behavior: mark Done right away.
                self.runner_tabs[tab_idx].state = RunnerTabState::Done;
                self.runner_tabs[tab_idx].insert_mode = false;
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
                self.runner_tabs[tab_idx].insert_mode = false;
                self.status_message = Some(msg);
                self.status_message_expires = None; // persist until dismissed
            } else {
                // Reload plan from disk — ralph may have updated passes: true.
                self.load_current_workflow();

                // Persist token usage to usage.json.
                {
                    let tab = &self.runner_tabs[tab_idx];
                    let workflow_name = tab.workflow_name.clone();
                    let task_id = tab.current_task_id.clone();
                    let workflow_dir = self.store.workflow_dir(&workflow_name);
                    let task_usage = TaskUsage {
                        input_tokens: tab.current_story_input_tokens,
                        output_tokens: tab.current_story_output_tokens,
                        cache_read_tokens: tab.current_story_cache_read_tokens,
                        cache_write_tokens: tab.current_story_cache_write_tokens,
                        estimated_cost_usd: tab.current_story_cost_usd,
                    };
                    if let Some(task_id) = task_id {
                        match UsageFile::load(&workflow_dir) {
                            Ok(mut usage_file) => {
                                usage_file.record_story(&task_id, task_usage);
                                let _ = usage_file.save(&workflow_dir);
                            }
                            Err(_) => {
                                // Silently swallow load errors; don't crash the app
                            }
                        }
                    }
                }

                // Determine whether to auto-loop or transition to Done.
                // Only act if still in Running state (not already Done from Complete signal or stop).
                // Read and clear saw_complete before any spawning so the next iteration starts clean.
                let saw_complete = self.runner_tabs[tab_idx].saw_complete;
                self.runner_tabs[tab_idx].saw_complete = false;

                let iteration_opt = match self.runner_tabs[tab_idx].state {
                    RunnerTabState::Running { iteration } => Some(iteration),
                    _ => None,
                };

                if let Some(iteration) = iteration_opt {
                    // Load the specific workflow for this runner tab (may differ from selected).
                    let workflow_name = self.runner_tabs[tab_idx].workflow_name.clone();
                    let is_complete = self.is_workflow_complete(&workflow_name);
                    let workflow_dir = self.store.workflow_dir(&workflow_name);
                    let tab_workflow = Workflow::load(&workflow_dir).ok();

                    let auto_continue = self.runner_tabs[tab_idx].auto_continue;

                    if auto_continue {
                        // Sentinel (saw_complete) takes precedence: treat as success regardless of
                        // exit code. Without sentinel, exit code 0 is success.
                        let is_success =
                            saw_complete || matches!(exited_code, Some(Some(0)));

                        if is_complete {
                            self.runner_tabs[tab_idx].state = RunnerTabState::Done;
                            self.runner_tabs[tab_idx].insert_mode = false;
                        } else if is_success {
                            // Success: spawn next iteration immediately.
                            self.spawn_next_iteration_at(tab_idx);
                        } else if iteration >= MAX_ITERATIONS {
                            let msg = format!(
                                "\r\n[runner] Max iterations ({MAX_ITERATIONS}) reached. Stopping.\r\n"
                            );
                            self.runner_tabs[tab_idx].parser.process(msg.as_bytes());
                            self.runner_tabs[tab_idx].state = RunnerTabState::Done;
                            self.runner_tabs[tab_idx].insert_mode = false;
                        } else {
                            // Failure (no sentinel, non-zero exit): prompt the user instead
                            // of auto-retrying so they can inspect output before deciding.
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
                    } else {
                        // auto_continue=false: transition to Done; user presses [c] to continue.
                        if is_complete {
                            self.runner_tabs[tab_idx].state = RunnerTabState::Done;
                            self.runner_tabs[tab_idx].insert_mode = false;
                        } else if iteration >= MAX_ITERATIONS {
                            let msg = format!(
                                "\r\nMax iterations ({MAX_ITERATIONS}) reached. Stopping.\r\n"
                            );
                            self.runner_tabs[tab_idx].parser.process(msg.as_bytes());
                            self.runner_tabs[tab_idx].state = RunnerTabState::Done;
                            self.runner_tabs[tab_idx].insert_mode = false;
                        } else {
                            // Natural exit within limit — transition to Done.
                            // User can press [c] to continue to the next task.
                            self.runner_tabs[tab_idx].state = RunnerTabState::Done;
                            self.runner_tabs[tab_idx].insert_mode = false;
                        }
                    }
                }
            }
        }
    }

    /// Returns `true` while the synthesis subprocess is running.
    pub fn is_synthesizing(&self) -> bool {
        self.synth_rx.is_some()
    }

    /// Starts prd-synth synthesis for the currently selected workflow.
    ///
    /// Checks that prd-source.md exists in the workflow directory; shows a status
    /// error if not. Spawns `synth_task` via a PTY and wires up the event channel.
    fn start_synthesizing(&mut self) {
        let Some(idx) = self.selected_workflow else {
            return;
        };
        let Some(name) = self.workflows.get(idx).cloned() else {
            return;
        };

        // Refuse to start a second synthesis while one is already running.
        if self.is_synthesizing() {
            self.status_message = Some("Synthesis already in progress".to_string());
            self.status_message_expires = Some(Instant::now() + Duration::from_secs(2));
            return;
        }

        let workflow_dir = self.store.workflow_dir(&name);
        let prd_source = workflow_dir.join("prd-source.md");
        if !prd_source.exists() {
            self.status_message =
                Some("No prd-source.md \u{2014} press [i] to import".to_string());
            self.status_message_expires = Some(Instant::now() + Duration::from_secs(4));
            return;
        }

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<RunnerEvent>();
        let (kill_tx, kill_rx) = oneshot::channel::<()>();
        let (resize_tx, resize_rx) = tokio::sync::mpsc::unbounded_channel::<(u16, u16)>();

        let (cols, rows) = self.initial_size;
        let pty_rows = rows.saturating_sub(PTY_ROW_OVERHEAD);

        self.synth_parser = Some(VtParser::new(pty_rows, cols, 1000));
        self.synth_rx = Some(rx);
        self.synth_kill_tx = Some(kill_tx);
        self.synth_workflow_name = Some(name.clone());
        self.resize_txs.push(resize_tx);

        drop(tokio::spawn(synth_task(
            workflow_dir,
            tx,
            kill_rx,
            (cols, pty_rows),
            resize_rx,
        )));
    }

    /// Kills the running synthesis subprocess immediately.
    /// Writes a "stopped" message to the synthesis log parser.
    fn stop_synthesizing(&mut self) {
        if let Some(kill_tx) = self.synth_kill_tx.take() {
            let _ = kill_tx.send(());
        }
        // Clear the channel so drain_synth_channel no longer processes events.
        self.synth_rx = None;
        if let Some(parser) = &mut self.synth_parser {
            parser.process(b"\r\n--- Synthesis stopped ---\r\n");
        }
    }

    /// Drains pending events from the synthesis subprocess channel.
    /// Feeds raw bytes into the synthesis VT100 parser and handles process exit.
    fn drain_synth_channel(&mut self) {
        if self.synth_rx.is_none() {
            return;
        }

        let mut byte_chunks: Vec<Vec<u8>> = Vec::new();
        let mut done = false;
        let mut spawn_error: Option<String> = None;
        let mut exited_code: Option<Option<u32>> = None;

        {
            let rx = match self.synth_rx.as_mut() {
                Some(r) => r,
                None => return,
            };
            loop {
                use tokio::sync::mpsc::error::TryRecvError;
                match rx.try_recv() {
                    Ok(RunnerEvent::Bytes(bytes)) => byte_chunks.push(bytes),
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
                    // Synthesis doesn't emit Complete or TokenUsage events; ignore them.
                    Ok(_) => {}
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        done = true;
                        break;
                    }
                }
            }
        } // rx borrow released

        for chunk in byte_chunks {
            if let Some(parser) = &mut self.synth_parser {
                parser.process(&chunk);
            }
        }

        if done {
            self.synth_rx = None;
            self.synth_kill_tx = None;

            if let Some(msg) = spawn_error {
                let err_msg = format!("\r\nSpawnError: {msg}\r\n");
                if let Some(parser) = &mut self.synth_parser {
                    parser.process(err_msg.as_bytes());
                }
                self.status_message = Some(msg);
                self.status_message_expires = None; // persist until dismissed
            } else {
                let summary = match exited_code {
                    Some(None) => "\r\n--- Synthesis stopped ---\r\n".to_string(),
                    Some(Some(code)) => {
                        format!("\r\n--- Synthesis exited (code: {code}) ---\r\n")
                    }
                    None => "\r\n--- Synthesis exited ---\r\n".to_string(),
                };
                if let Some(parser) = &mut self.synth_parser {
                    parser.process(summary.as_bytes());
                }

                // Post-synthesis outcome handling.
                match exited_code {
                    Some(Some(0)) => {
                        // Exit code 0: attempt to load the synthesized prd.json.
                        let workflow_name = self.synth_workflow_name.clone();
                        if let Some(name) = workflow_name {
                            let dir = self.store.workflow_dir(&name);
                            match Workflow::load(&dir) {
                                Ok(_) => {
                                    // prd.json is valid — reload stories panel and show success.
                                    self.load_current_workflow();
                                    self.notification = Some((
                                        "Synthesis complete".to_string(),
                                        Instant::now(),
                                    ));
                                }
                                Err(_) => {
                                    // prd.json missing or unparseable.
                                    self.status_message = Some(
                                        "prd.json invalid after synthesis \u{2014} check log"
                                            .to_string(),
                                    );
                                    self.status_message_expires = None; // persist until dismissed
                                }
                            }
                        }
                    }
                    Some(Some(code)) => {
                        // Non-zero exit: synthesis failed; do not touch prd.json.
                        self.status_message = Some(format!(
                            "Synthesis failed (exit {code}) \u{2014} check log"
                        ));
                        self.status_message_expires = None; // persist until dismissed
                    }
                    _ => {
                        // Killed (Some(None)) or channel disconnected (None): no special action.
                    }
                }
            }
        }
    }

    fn stop_runner(&mut self) {
        if self.active_tab <= 1 {
            return;
        }
        let tab_idx = self.active_tab - 2;
        let Some(tab) = self.runner_tabs.get_mut(tab_idx) else {
            return;
        };
        if !matches!(tab.state, RunnerTabState::Running { .. }) {
            return;
        }
        if let Some(kill_tx) = tab.runner_kill_tx.take() {
            let _ = kill_tx.send(());
        }
        // Mark Stopped immediately so drain_tab_channel skips re-processing on Exited.
        tab.state = RunnerTabState::Stopped;
        tab.insert_mode = false;
    }

    fn start_runner(&mut self) {
        let Some(idx) = self.selected_workflow else {
            return;
        };
        let Some(name) = self.workflows.get(idx).cloned() else {
            return;
        };

        // Prevent starting a second runner for the same workflow while one is active.
        if self
            .runner_tabs
            .iter()
            .any(|t| t.workflow_name == name && matches!(t.state, RunnerTabState::Running { .. }))
        {
            self.status_message = Some("Already running".to_string());
            self.status_message_expires = Some(Instant::now() + Duration::from_secs(2));
            return;
        }

        let plan_dir = self.store.workflow_dir(&name);
        let repo_root = self.store.root().to_path_buf();

        // Load workflow to populate current task info before spawning.
        let (current_task_id, current_task_title) = {
            let workflow_dir = self.store.workflow_dir(&name);
            match Workflow::load(&workflow_dir)
                .ok()
                .and_then(|w| w.next_task().map(|t| (t.id.clone(), t.title.clone())))
            {
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
            t.workflow_name == name && !matches!(t.state, RunnerTabState::Running { .. })
        });

        let (cols, rows) = self.initial_size;
        let pty_rows = rows.saturating_sub(PTY_ROW_OVERHEAD);
        if let Some(reuse) = reuse_idx {
            let tab = &mut self.runner_tabs[reuse];
            // Reset parser with current terminal dimensions; scrollback capacity = 1000.
            tab.parser = VtParser::new(pty_rows, cols, 1000);
            tab.log_scroll = 0;
            tab.state = RunnerTabState::Running { iteration: 1 };
            tab.runner_rx = Some(rx);
            tab.runner_kill_tx = Some(kill_tx);
            tab.stdin_tx = Some(stdin_tx);
            tab.auto_continue = false;
            tab.current_task_id = current_task_id;
            tab.current_task_title = current_task_title;
            tab.iterations_used = 1;
            tab.current_story_input_tokens = 0;
            tab.current_story_output_tokens = 0;
            tab.current_story_cache_read_tokens = 0;
            tab.current_story_cache_write_tokens = 0;
            tab.current_story_cost_usd = 0.0;
            tab.insert_mode = false;
            tab.saw_complete = false;
            self.active_tab = reuse + 2; // active_tab is 2-indexed for runner tabs (0=PRDs, 1=Workflows)
        } else {
            let tab = RunnerTab {
                workflow_name: name,
                parser: VtParser::new(pty_rows, cols, 1000),
                state: RunnerTabState::Running { iteration: 1 },
                runner_rx: Some(rx),
                runner_kill_tx: Some(kill_tx),
                stdin_tx: Some(stdin_tx),
                log_scroll: 0,
                auto_continue: false,
                current_task_id,
                current_task_title,
                iterations_used: 1,
                current_story_input_tokens: 0,
                current_story_output_tokens: 0,
                current_story_cache_read_tokens: 0,
                current_story_cache_write_tokens: 0,
                current_story_cost_usd: 0.0,
                insert_mode: false,
                saw_complete: false,
            };
            self.runner_tabs.push(tab);
            self.active_tab = 1 + self.runner_tabs.len(); // runner tabs are 2-indexed in active_tab (0=PRDs, 1=Workflows)
        }

        self.resize_txs.push(resize_tx);
        drop(tokio::spawn(runner_task(
            plan_dir,
            repo_root,
            tx,
            kill_rx,
            stdin_rx,
            (cols, pty_rows),
            resize_rx,
        )));
    }

    /// Spawns the next claude iteration on the active runner tab.
    /// Increments the current iteration counter and starts a new subprocess.
    fn spawn_next_iteration(&mut self) {
        if self.active_tab <= 1 {
            return;
        }
        self.spawn_next_iteration_at(self.active_tab - 2);
    }

    /// Restarts the runner for a tab currently in `Stopped` state.
    /// Resets the tab to a fresh `Running { iteration: 1 }` with a new subprocess,
    /// preserving the same workflow. Returns early if the tab is not in `Stopped` state.
    fn restart_runner_at(&mut self, tab_idx: usize) {
        // Extract workflow_name; confirm the tab is Stopped.
        let name = {
            let Some(tab) = self.runner_tabs.get(tab_idx) else {
                return;
            };
            if !matches!(tab.state, RunnerTabState::Stopped) {
                return;
            }
            tab.workflow_name.clone()
        };

        let plan_dir = self.store.workflow_dir(&name);
        let repo_root = self.store.root().to_path_buf();

        // Load workflow to populate current task info before spawning.
        let (current_task_id, current_task_title) = {
            let workflow_dir = self.store.workflow_dir(&name);
            match Workflow::load(&workflow_dir)
                .ok()
                .and_then(|w| w.next_task().map(|t| (t.id.clone(), t.title.clone())))
            {
                Some((id, title)) => (Some(id), Some(title)),
                None => (None, None),
            }
        };

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<RunnerEvent>();
        let (kill_tx, kill_rx) = oneshot::channel::<()>();
        let (stdin_tx, stdin_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        let (resize_tx, resize_rx) = tokio::sync::mpsc::unbounded_channel::<(u16, u16)>();

        let (cols, rows) = self.initial_size;
        let pty_rows = rows.saturating_sub(PTY_ROW_OVERHEAD);

        if let Some(tab) = self.runner_tabs.get_mut(tab_idx) {
            tab.parser = VtParser::new(pty_rows, cols, 1000);
            tab.log_scroll = 0;
            tab.state = RunnerTabState::Running { iteration: 1 };
            tab.runner_rx = Some(rx);
            tab.runner_kill_tx = Some(kill_tx);
            tab.stdin_tx = Some(stdin_tx);
            tab.auto_continue = false;
            tab.current_task_id = current_task_id;
            tab.current_task_title = current_task_title;
            tab.iterations_used = 1;
            tab.current_story_input_tokens = 0;
            tab.current_story_output_tokens = 0;
            tab.current_story_cache_read_tokens = 0;
            tab.current_story_cache_write_tokens = 0;
            tab.current_story_cost_usd = 0.0;
            tab.insert_mode = false;
            tab.saw_complete = false;
        }

        self.resize_txs.push(resize_tx);
        drop(tokio::spawn(runner_task(
            plan_dir,
            repo_root,
            tx,
            kill_rx,
            stdin_rx,
            (cols, pty_rows),
            resize_rx,
        )));
    }

    /// Spawns the next claude iteration on the runner tab at `tab_idx`.
    /// Increments the iteration counter and replaces the subprocess channels.
    /// Requires the tab to be in `Running { iteration }` or `Done` state; returns early otherwise.
    /// When called from `Done` state, uses `iterations_used` as the iteration count.
    fn spawn_next_iteration_at(&mut self, tab_idx: usize) {
        // Extract workflow_name and iteration without holding a borrow.
        let (name, iteration) = {
            let Some(tab) = self.runner_tabs.get(tab_idx) else {
                return;
            };
            let iteration = match tab.state {
                RunnerTabState::Running { iteration } => iteration,
                RunnerTabState::Done | RunnerTabState::Stopped => tab.iterations_used,
                RunnerTabState::Error(_) => return,
            };
            (tab.workflow_name.clone(), iteration)
        };

        let plan_dir = self.store.workflow_dir(&name);
        let repo_root = self.store.root().to_path_buf();

        // Load workflow to update current task info before spawning.
        let (current_task_id, current_task_title) = {
            let workflow_dir = self.store.workflow_dir(&name);
            match Workflow::load(&workflow_dir)
                .ok()
                .and_then(|w| w.next_task().map(|t| (t.id.clone(), t.title.clone())))
            {
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
            // Reset token and cost fields at the start of a new iteration.
            tab.current_story_input_tokens = 0;
            tab.current_story_output_tokens = 0;
            tab.current_story_cache_read_tokens = 0;
            tab.current_story_cache_write_tokens = 0;
            tab.current_story_cost_usd = 0.0;
            tab.saw_complete = false;
            tab.runner_rx = Some(rx);
            tab.runner_kill_tx = Some(kill_tx);
            tab.stdin_tx = Some(stdin_tx);
            tab.state = RunnerTabState::Running {
                iteration: new_iteration,
            };
            tab.current_task_id = current_task_id;
            tab.current_task_title = current_task_title;
            tab.iterations_used = new_iteration;
        }

        self.resize_txs.push(resize_tx);
        let (cols, rows) = self.initial_size;
        drop(tokio::spawn(runner_task(
            plan_dir,
            repo_root,
            tx,
            kill_rx,
            stdin_rx,
            (cols, rows.saturating_sub(PTY_ROW_OVERHEAD)),
            resize_rx,
        )));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_token_line_parses_actual_format() {
        // 1. Exact observed format with typical values (all fields present).
        let line = "Cost: $0.0456 (10,000 input, 2,500 output, 500 cache read, 100 cache write tokens)";
        let result = parse_token_line(line).expect("should parse full format");
        assert_eq!(result.0, 10_000, "input_tokens");
        assert_eq!(result.1, 2_500, "output_tokens");
        assert_eq!(result.2, 500, "cache_read_tokens");
        assert_eq!(result.3, 100, "cache_write_tokens");
        assert!((result.4 - 0.0456).abs() < 1e-9, "cost_usd");

        // 2. Cache tokens absent — should default to 0.
        let line_no_cache = "Cost: $0.0123 (1,234 input, 567 output tokens)";
        let result2 = parse_token_line(line_no_cache).expect("should parse without cache");
        assert_eq!(result2.0, 1_234, "input_tokens");
        assert_eq!(result2.1, 567, "output_tokens");
        assert_eq!(result2.2, 0, "cache_read_tokens defaults to 0");
        assert_eq!(result2.3, 0, "cache_write_tokens defaults to 0");
        assert!((result2.4 - 0.0123).abs() < 1e-9, "cost_usd");

        // 3. Numbers with comma thousands-separators (larger values).
        let line_large = "Cost: $1.2345 (12,345 input, 6,789 output, 1,000 cache read, 500 cache write tokens)";
        let result3 = parse_token_line(line_large).expect("should parse thousands-separated");
        assert_eq!(result3.0, 12_345, "input_tokens");
        assert_eq!(result3.1, 6_789, "output_tokens");
        assert_eq!(result3.2, 1_000, "cache_read_tokens");
        assert_eq!(result3.3, 500, "cache_write_tokens");
        assert!((result3.4 - 1.2345).abs() < 1e-9, "cost_usd");
    }
}
