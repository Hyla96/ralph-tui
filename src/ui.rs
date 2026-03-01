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
    // Log panel: render buffered lines; auto-scroll to last line when running.
    let log_block = Block::default().borders(Borders::ALL).title("Log");
    if app.log_lines.is_empty() {
        frame.render_widget(log_block, vertical[1]);
    } else {
        let items: Vec<ListItem> =
            app.log_lines.iter().map(|l| ListItem::new(l.as_str())).collect();
        let list = List::new(items).block(log_block);
        let selected = if matches!(app.app_state, AppState::Running { .. }) {
            Some(app.log_lines.len().saturating_sub(1))
        } else {
            None
        };
        let mut log_state = ListState::default().with_selected(selected);
        frame.render_stateful_widget(list, vertical[1], &mut log_state);
    }

    // Status bar: no border, content depends on AppState (or a status message).
    let status_text = if let Some(msg) = &app.status_message {
        Line::from(Span::styled(msg.as_str(), Style::default().fg(Color::Red)))
    } else {
        let hint = match &app.app_state {
            AppState::Idle => "[r]un  [n]ew  [e]dit  [d]elete  [?]help  [q]uit".to_string(),
            AppState::Running { iteration } => {
                format!("[s]top  [q]uit  Running iteration {}/10\u{2026}", iteration)
            }
            AppState::Complete => {
                "COMPLETE  [n]ew  [e]dit  [d]elete  [?]help  [q]uit".to_string()
            }
        };
        Line::from(hint)
    };
    frame.render_widget(Paragraph::new(status_text), vertical[2]);

    // Render dialog overlays on top of everything else
    match &app.dialog {
        Some(Dialog::NewPlan { input, error }) => {
            draw_new_plan_dialog(frame, frame.area(), input, error);
        }
        Some(Dialog::DeletePlan { name }) => {
            draw_delete_plan_dialog(frame, frame.area(), name);
        }
        Some(Dialog::ContinuePrompt { next_id, next_title }) => {
            draw_continue_prompt_dialog(frame, frame.area(), next_id, next_title);
        }
        None => {}
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

fn draw_delete_plan_dialog(frame: &mut Frame, area: Rect, name: &str) {
    // 52 chars wide (2 border + content), 3 rows tall (2 border + 1 content line)
    let dialog_rect = centered_rect(52, 3, area);
    frame.render_widget(Clear, dialog_rect);

    let text = format!("Delete plan '{name}'? [y/N]");
    let block = Block::default().borders(Borders::ALL).title("Delete Plan");
    frame.render_widget(Paragraph::new(text).block(block), dialog_rect);
}

fn draw_continue_prompt_dialog(
    frame: &mut Frame,
    area: Rect,
    next_id: &str,
    next_title: &str,
) {
    // 70 wide (2 border + 68 content), 4 tall (2 border + 2 content lines)
    let dialog_rect = centered_rect(70, 4, area);
    frame.render_widget(Clear, dialog_rect);

    let lines = vec![
        Line::from("Story done. Continue? [Y/n]"),
        Line::from(format!("Next: {next_id}: {next_title}")),
    ];
    let block = Block::default().borders(Borders::ALL).title("Continue?");
    frame.render_widget(Paragraph::new(lines).block(block), dialog_rect);
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
