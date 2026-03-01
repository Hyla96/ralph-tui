use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, enable_raw_mode};
use ratatui::DefaultTerminal;
use std::io::stdout;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::ralph::plan::Plan;
use crate::ralph::runner::RunnerEvent;
use crate::ralph::store::Store;

pub enum AppState {
    Idle,
    Running { iteration: u32 },
    Complete,
}

pub enum Dialog {
    NewPlan { input: String, error: Option<String> },
    DeletePlan { name: String },
}

pub struct App {
    pub running: bool,
    pub store: Store,
    pub plans: Vec<String>,
    pub selected_plan: Option<usize>,
    pub current_plan: Option<Plan>,
    pub app_state: AppState,
    pub dialog: Option<Dialog>,
    pub status_message: Option<String>,
    pub status_message_expires: Option<Instant>,
    pub log_lines: Vec<String>,
    pub runner_rx: Option<UnboundedReceiver<RunnerEvent>>,
}

impl App {
    pub fn new(store: Store) -> Self {
        let plans = store.list_plans();
        let selected_plan = if plans.is_empty() { None } else { Some(0) };
        let mut app = App {
            running: true,
            store,
            plans,
            selected_plan,
            current_plan: None,
            app_state: AppState::Idle,
            dialog: None,
            status_message: None,
            status_message_expires: None,
            log_lines: Vec::new(),
            runner_rx: None,
        };
        app.load_current_plan();
        app
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while self.running {
            self.check_status_timeout();
            self.drain_runner_channel();
            terminal.draw(|frame| crate::ui::draw(frame, self))?;
            self.handle_events(terminal)?;
        }
        Ok(())
    }

    fn handle_events(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            if self.dialog.is_some() {
                self.handle_dialog_key(key.code);
            } else {
                match key.code {
                    KeyCode::Char('q') => self.running = false,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.running = false;
                    }
                    KeyCode::Up | KeyCode::Char('k') => self.move_up(),
                    KeyCode::Down | KeyCode::Char('j') => self.move_down(),
                    KeyCode::Char('r') => self.start_runner(),
                    KeyCode::Char('n') => self.open_new_plan_dialog(),
                    KeyCode::Char('e') => self.edit_current_plan(terminal)?,
                    KeyCode::Char('d') => self.open_delete_plan_dialog(),
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn edit_current_plan(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let Some(idx) = self.selected_plan else {
            return Ok(());
        };
        let Some(name) = self.plans.get(idx).cloned() else {
            return Ok(());
        };

        let prd_path = self.store.plan_dir(&name).join("prd.json");
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

        // Reload plan from disk so updated stories are immediately visible.
        self.load_current_plan();

        Ok(())
    }

    fn handle_dialog_key(&mut self, code: KeyCode) {
        // DeletePlan confirmation: y confirms, any other key cancels.
        if let Some(Dialog::DeletePlan { name }) = &self.dialog {
            let name = name.clone();
            let old_idx = self.selected_plan;
            self.dialog = None;
            if code == KeyCode::Char('y') || code == KeyCode::Char('Y') {
                let dir = self.store.plan_dir(&name);
                let _ = std::fs::remove_dir_all(dir);
                self.refresh_plans_after_delete(old_idx);
            }
            return;
        }

        match code {
            KeyCode::Esc => {
                self.dialog = None;
            }
            KeyCode::Backspace => {
                if let Some(Dialog::NewPlan { input, error }) = &mut self.dialog {
                    input.pop();
                    *error = None;
                }
            }
            KeyCode::Char(c) if c.is_ascii_alphanumeric() || c == '-' => {
                if let Some(Dialog::NewPlan { input, error }) = &mut self.dialog {
                    input.push(c);
                    *error = None;
                }
            }
            KeyCode::Enter => {
                // Clone input before releasing the borrow so we can call store methods.
                let input = match &self.dialog {
                    Some(Dialog::NewPlan { input, .. }) => input.clone(),
                    _ => return,
                };
                if !Store::is_valid_name(&input) {
                    if let Some(Dialog::NewPlan { error, .. }) = &mut self.dialog {
                        *error = Some(
                            "Invalid name — use lowercase letters, digits, hyphens (3–64 chars)"
                                .to_string(),
                        );
                    }
                    return;
                }
                match self.store.create_plan(&input) {
                    Ok(()) => {
                        self.dialog = None;
                        self.refresh_plans_and_focus(&input);
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        if let Some(Dialog::NewPlan { error, .. }) = &mut self.dialog {
                            *error = if msg.contains("already exists") {
                                Some("Plan already exists".to_string())
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

    fn open_new_plan_dialog(&mut self) {
        self.dialog = Some(Dialog::NewPlan {
            input: String::new(),
            error: None,
        });
    }

    fn open_delete_plan_dialog(&mut self) {
        let Some(idx) = self.selected_plan else {
            return;
        };
        let Some(name) = self.plans.get(idx).cloned() else {
            return;
        };
        self.dialog = Some(Dialog::DeletePlan { name });
    }

    fn refresh_plans_after_delete(&mut self, old_idx: Option<usize>) {
        self.plans = self.store.list_plans();
        self.selected_plan = if self.plans.is_empty() {
            None
        } else {
            Some(old_idx.map(|i| i.min(self.plans.len() - 1)).unwrap_or(0))
        };
        self.load_current_plan();
    }

    fn refresh_plans_and_focus(&mut self, name: &str) {
        self.plans = self.store.list_plans();
        self.selected_plan = self.plans.iter().position(|p| p == name);
        if self.selected_plan.is_none() && !self.plans.is_empty() {
            self.selected_plan = Some(0);
        }
        self.load_current_plan();
    }

    fn load_current_plan(&mut self) {
        self.current_plan = self.selected_plan.and_then(|i| {
            let name = self.plans.get(i)?;
            let dir = self.store.plan_dir(name);
            Plan::load(&dir).ok()
        });
    }

    fn move_up(&mut self) {
        if let Some(i) = self.selected_plan
            && i > 0
        {
            self.selected_plan = Some(i - 1);
        }
        self.load_current_plan();
    }

    fn move_down(&mut self) {
        if let Some(i) = self.selected_plan
            && i + 1 < self.plans.len()
        {
            self.selected_plan = Some(i + 1);
        }
        self.load_current_plan();
    }

    fn check_status_timeout(&mut self) {
        if let Some(expires) = self.status_message_expires
            && Instant::now() >= expires
        {
            self.status_message = None;
            self.status_message_expires = None;
        }
    }

    fn drain_runner_channel(&mut self) {
        if self.runner_rx.is_none() {
            return;
        }
        let mut lines = Vec::new();
        let mut done = false;
        let mut spawn_error: Option<String> = None;

        if let Some(rx) = &mut self.runner_rx {
            loop {
                use tokio::sync::mpsc::error::TryRecvError;
                match rx.try_recv() {
                    Ok(RunnerEvent::Line(line)) => lines.push(line),
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
        }

        for line in lines {
            self.log_lines.push(line);
            if self.log_lines.len() > 1000 {
                self.log_lines.remove(0);
            }
        }

        if done {
            self.runner_rx = None;
            if matches!(self.app_state, AppState::Running { .. }) {
                self.app_state = AppState::Idle;
            }
            if let Some(msg) = spawn_error {
                self.status_message = Some(msg);
            }
        }
    }

    fn start_runner(&mut self) {
        let Some(idx) = self.selected_plan else {
            return;
        };
        let Some(name) = self.plans.get(idx).cloned() else {
            return;
        };

        if matches!(self.app_state, AppState::Running { .. }) {
            self.status_message = Some("Already running".to_string());
            self.status_message_expires =
                Some(Instant::now() + Duration::from_secs(2));
            return;
        }

        let plan_dir = self.store.plan_dir(&name);
        let repo_root = self.store.root().to_path_buf();

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<RunnerEvent>();
        self.runner_rx = Some(rx);
        self.app_state = AppState::Running { iteration: 1 };

        drop(tokio::spawn(async move {
            use std::process::Stdio;
            use tokio::io::AsyncBufReadExt;

            let mut child = match tokio::process::Command::new("claude")
                .args(["--agent", "ralph", "Implement the next user story."])
                .current_dir(&repo_root)
                .env("RALPH_PLAN_DIR", &plan_dir)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
            {
                Ok(c) => c,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    let _ = tx
                        .send(RunnerEvent::SpawnError("claude not found on PATH".to_string()));
                    return;
                }
                Err(e) => {
                    let _ = tx.send(RunnerEvent::SpawnError(e.to_string()));
                    return;
                }
            };

            let stdout = child.stdout.take().expect("stdout piped");
            let stderr = child.stderr.take().expect("stderr piped");

            let tx_stdout = tx.clone();
            let stdout_task = tokio::spawn(async move {
                let mut reader = tokio::io::BufReader::new(stdout).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    if tx_stdout.send(RunnerEvent::Line(line)).is_err() {
                        break;
                    }
                }
            });

            let tx_stderr = tx.clone();
            let stderr_task = tokio::spawn(async move {
                let mut reader = tokio::io::BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    if tx_stderr.send(RunnerEvent::Line(line)).is_err() {
                        break;
                    }
                }
            });

            let _ = child.wait().await;
            let _ = stdout_task.await;
            let _ = stderr_task.await;
            let _ = tx.send(RunnerEvent::Exited);
        }));
    }
}
