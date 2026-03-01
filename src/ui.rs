use crate::app::App;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

pub fn draw(frame: &mut Frame, app: &App) {
    // Outer vertical split: top panels (~75%) | log (~20%) | status bar (1 line)
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(75),
            Constraint::Percentage(20),
            Constraint::Length(1),
        ])
        .split(frame.area());

    // Top row: Plans (~25%) | Stories (~75%)
    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25), Constraint::Percentage(75)])
        .split(vertical[0]);

    // Plans panel
    let plans_title = format!("Plans ({})", app.plans.len());
    let plans_block = Block::default().borders(Borders::ALL).title(plans_title);

    if app.plans.is_empty() {
        let empty_msg =
            Paragraph::new("No plans. Press [n] to create one.").block(plans_block);
        frame.render_widget(empty_msg, top[0]);
    } else {
        let items: Vec<ListItem> =
            app.plans.iter().map(|name| ListItem::new(name.as_str())).collect();
        let list = List::new(items)
            .block(plans_block)
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        let mut list_state = ListState::default().with_selected(app.selected_plan);
        frame.render_stateful_widget(list, top[0], &mut list_state);
    }

    frame.render_widget(Block::default().borders(Borders::ALL).title("Stories"), top[1]);
    frame.render_widget(Block::default().borders(Borders::ALL).title("Log"), vertical[1]);

    // Status bar: no border
    frame.render_widget(Paragraph::new("[q]uit"), vertical[2]);
}
