#![allow(dead_code)]

mod app;
mod ralph;
mod ui;

use anyhow::Result;
use ralph::store::Store;

fn main() -> Result<()> {
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

    let mut terminal = ratatui::init();
    let result = app::App::new(store).run(&mut terminal);
    ratatui::restore();
    result
}
