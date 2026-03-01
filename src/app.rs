use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::DefaultTerminal;
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
}

pub struct App {
    pub running: bool,
    pub store: Store,
    pub plans: Vec<String>,
    pub selected_plan: Option<usize>,
    pub current_plan: Option<Plan>,
    pub app_state: AppState,
    pub dialog: Option<Dialog>,
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
        };
        app.load_current_plan();
        app
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while self.running {
            terminal.draw(|frame| crate::ui::draw(frame, self))?;
            self.handle_events()?;
        }
        Ok(())
    }

    fn handle_events(&mut self) -> Result<()> {
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
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn handle_dialog_key(&mut self, code: KeyCode) {
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
