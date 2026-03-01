use crate::app::{App, AppState, Dialog};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
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

    // Stories panel
    match &app.current_plan {
        None => {
            let block = Block::default().borders(Borders::ALL).title("Stories");
            frame.render_widget(Paragraph::new("Select a plan").block(block), top[1]);
        }
        Some(plan) => {
            let title = format!("Stories ({}/{})", plan.done_count(), plan.total_count());
            let block = Block::default().borders(Borders::ALL).title(title);
            let items: Vec<ListItem> = plan
                .prd
                .user_stories
                .iter()
                .map(|story| {
                    let check = if story.passes { "✓" } else { "○" };
                    let text =
                        format!("{} [{}] {}: {}", check, story.priority, story.id, story.title);
                    let style = if story.passes {
                        Style::default().fg(Color::DarkGray)
                    } else {
                        Style::default()
                    };
                    ListItem::new(text).style(style)
                })
                .collect();
            let list = List::new(items).block(block);
            frame.render_widget(list, top[1]);
        }
    }
    frame.render_widget(Block::default().borders(Borders::ALL).title("Log"), vertical[1]);

    // Status bar: no border, content depends on AppState
    let status_text = match &app.app_state {
        AppState::Idle => "[r]un  [n]ew  [e]dit  [d]elete  [?]help  [q]uit".to_string(),
        AppState::Running { iteration } => {
            format!("[s]top  [q]uit  Running iteration {}/10\u{2026}", iteration)
        }
        AppState::Complete => {
            "COMPLETE  [n]ew  [e]dit  [d]elete  [?]help  [q]uit".to_string()
        }
    };
    frame.render_widget(Paragraph::new(status_text), vertical[2]);

    // Render new-plan dialog overlay on top of everything else
    if let Some(Dialog::NewPlan { input, error }) = &app.dialog {
        draw_new_plan_dialog(frame, frame.area(), input, error);
    }
}

/// Returns a centered `Rect` of the given dimensions inside `area`.
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    let actual_width = width.min(area.width);
    let actual_height = height.min(area.height);
    Rect::new(x, y, actual_width, actual_height)
}

fn draw_new_plan_dialog(frame: &mut Frame, area: Rect, input: &str, error: &Option<String>) {
    // 72 chars wide (2 border + 70 content), 4 rows tall (2 border + 2 content)
    let dialog_rect = centered_rect(72, 4, area);
    frame.render_widget(Clear, dialog_rect);

    let prompt = format!("New plan name: {}_", input);
    let lines: Vec<Line> = match error {
        Some(err) => vec![
            Line::from(prompt),
            Line::from(Span::styled(err.as_str(), Style::default().fg(Color::Red))),
        ],
        None => vec![Line::from(prompt), Line::from("")],
    };

    let block = Block::default().borders(Borders::ALL).title("New Plan");
    frame.render_widget(Paragraph::new(lines).block(block), dialog_rect);
}
