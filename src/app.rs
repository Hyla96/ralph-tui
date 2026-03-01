use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::DefaultTerminal;
use std::time::Duration;

use crate::ralph::plan::Plan;
use crate::ralph::store::Store;

pub struct App {
    pub running: bool,
    pub store: Store,
    pub plans: Vec<String>,
    pub selected_plan: Option<usize>,
    pub current_plan: Option<Plan>,
}

impl App {
    pub fn new(store: Store) -> Self {
        let plans = store.list_plans();
        let selected_plan = if plans.is_empty() { None } else { Some(0) };
        let mut app = App { running: true, store, plans, selected_plan, current_plan: None };
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
            match key.code {
                KeyCode::Char('q') => self.running = false,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.running = false;
                }
                KeyCode::Up | KeyCode::Char('k') => self.move_up(),
                KeyCode::Down | KeyCode::Char('j') => self.move_down(),
                _ => {}
            }
        }
        Ok(())
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
