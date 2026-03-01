#![allow(dead_code)]

mod app;
mod ralph;
mod ui;

use anyhow::Result;
use ralph::store::Store;

#[tokio::main]
async fn main() -> Result<()> {
    eprintln!("Starting Ralph CLI");
    let cwd = std::env::current_dir()?;
    let store = match Store::find(&cwd) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    };

    // Restore terminal before printing any panic message.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        ratatui::restore();
        default_hook(info);
    }));

    // Read terminal size before ratatui::init() switches to alternate screen.
    let terminal_size = crossterm::terminal::size().unwrap_or((80, 24));

    let mut terminal = ratatui::init();
    let mut app = app::App::new(store, terminal_size);
    let result = app.run(&mut terminal);
    ratatui::restore();
    result
}
