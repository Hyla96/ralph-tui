use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, enable_raw_mode};
use ratatui::DefaultTerminal;
use std::io::stdout;
use std::time::Duration;

use crate::ralph::plan::Plan;
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
        };
        app.load_current_plan();
        app
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while self.running {
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
}
