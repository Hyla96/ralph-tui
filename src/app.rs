use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::DefaultTerminal;
use std::time::Duration;

use crate::ralph::store::Store;

pub struct App {
    pub running: bool,
    pub store: Store,
    pub plans: Vec<String>,
    pub selected_plan: Option<usize>,
}

impl App {
    pub fn new(store: Store) -> Self {
        let plans = store.list_plans();
        let selected_plan = if plans.is_empty() { None } else { Some(0) };
        App { running: true, store, plans, selected_plan }
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

    fn move_up(&mut self) {
        if let Some(i) = self.selected_plan
            && i > 0
        {
            self.selected_plan = Some(i - 1);
        }
    }

    fn move_down(&mut self) {
        if let Some(i) = self.selected_plan
            && i + 1 < self.plans.len()
        {
            self.selected_plan = Some(i + 1);
        }
    }
}
