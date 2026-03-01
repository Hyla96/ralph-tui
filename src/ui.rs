use crate::app::{App, Dialog, RunnerTab, RunnerTabState};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};

pub fn draw(frame: &mut Frame, app: &App) {
    // Top-level split: tab bar (1 line) | content area (rest)
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(frame.area());

    draw_tab_bar(frame, app, outer[0]);

    // Content area below tab bar
    let content = outer[1];

    // Outer vertical split: top panels (~75%) | log (~20%) | status bar (1 line)
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(75),
            Constraint::Percentage(20),
            Constraint::Length(1),
        ])
        .split(content);

    // Top row: Workflows (~25%) | Tasks (~75%)
    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25), Constraint::Percentage(75)])
        .split(vertical[0]);

    // Workflows panel
    let plans_title = format!("Workflows ({})", app.workflows.len());
    let plans_block = Block::default().borders(Borders::ALL).title(plans_title);

    if app.workflows.is_empty() {
        let empty_msg =
            Paragraph::new("No workflows. Press [n] to create one.").block(plans_block);
        frame.render_widget(empty_msg, top[0]);
    } else {
        let items: Vec<ListItem> =
            app.workflows.iter().map(|name| ListItem::new(name.as_str())).collect();
        let list = List::new(items)
            .block(plans_block)
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        let mut list_state = ListState::default().with_selected(app.selected_workflow);
        frame.render_stateful_widget(list, top[0], &mut list_state);
    }

    // Tasks panel
    match &app.current_workflow {
        None => {
            let block = Block::default().borders(Borders::ALL).title("Tasks");
            frame.render_widget(Paragraph::new("Select a workflow").block(block), top[1]);
        }
        Some(workflow) => {
            let title = format!("Tasks ({}/{})", workflow.done_count(), workflow.total_count());
            let block = Block::default().borders(Borders::ALL).title(title);
            let items: Vec<ListItem> = workflow
                .prd
                .tasks
                .iter()
                .map(|task| {
                    let check = if task.passes { "✓" } else { "○" };
                    let text =
                        format!("{} [{}] {}: {}", check, task.priority, task.id, task.title);
                    let style = if task.passes {
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

    // Log panel: show log lines from the active runner tab; empty when on Workflows tab.
    let active_runner: Option<&RunnerTab> = if app.active_tab > 0 {
        app.runner_tabs.get(app.active_tab - 1)
    } else {
        None
    };

    let log_block = Block::default().borders(Borders::ALL).title("Log");
    match active_runner {
        None => {
            frame.render_widget(log_block, vertical[1]);
        }
        Some(tab) if tab.log_lines.is_empty() => {
            frame.render_widget(log_block, vertical[1]);
        }
        Some(tab) => {
            let items: Vec<ListItem> =
                tab.log_lines.iter().map(|l| ListItem::new(l.as_str())).collect();
            let list = List::new(items).block(log_block);
            let selected = if matches!(tab.state, RunnerTabState::Running { .. }) {
                Some(tab.log_lines.len().saturating_sub(1))
            } else {
                None
            };
            let mut log_state = ListState::default().with_selected(selected);
            frame.render_stateful_widget(list, vertical[1], &mut log_state);
        }
    }

    // Status bar: no border, content depends on active tab and runner state.
    let status_text = if let Some(msg) = &app.status_message {
        Line::from(Span::styled(msg.as_str(), Style::default().fg(Color::Red)))
    } else {
        let hint = match active_runner {
            None => "[r]un  [n]ew  [e]dit  [d]elete  [?]help  [q]uit".to_string(),
            Some(tab) => match &tab.state {
                RunnerTabState::Running { iteration } => {
                    format!("[s]top  [q]uit  Running iteration {}/10\u{2026}", iteration)
                }
                RunnerTabState::Done => {
                    "Done  [n]ew  [e]dit  [d]elete  [?]help  [q]uit".to_string()
                }
                RunnerTabState::Error(msg) => {
                    format!("Error: {}  [q]uit", msg)
                }
            },
        };
        Line::from(hint)
    };
    frame.render_widget(Paragraph::new(status_text), vertical[2]);

    // Render dialog overlays on top of everything else
    match &app.dialog {
        Some(Dialog::NewWorkflow { input, error }) => {
            draw_new_workflow_dialog(frame, frame.area(), input, error);
        }
        Some(Dialog::DeleteWorkflow { name }) => {
            draw_delete_workflow_dialog(frame, frame.area(), name);
        }
        Some(Dialog::ContinuePrompt { next_id, next_title }) => {
            draw_continue_prompt_dialog(frame, frame.area(), next_id, next_title);
        }
        Some(Dialog::Help) => {
            draw_help_dialog(frame, frame.area());
        }
        None => {}
    }
}

/// Renders the single-line tab bar at the top of the screen.
///
/// Tab 0 is always `[Workflows]`. Runner tabs show `[name]`, `[name ✓]`, or `[name !]`.
/// The active tab is highlighted with REVERSED style. Tabs that don't fit are cropped.
fn draw_tab_bar(frame: &mut Frame, app: &App, area: Rect) {
    let mut spans: Vec<Span> = Vec::new();
    let mut used_width: u16 = 0;

    // Tab 0: Workflows
    let label = "[Workflows]";
    let label_w = label.chars().count() as u16;
    if used_width + label_w <= area.width {
        let style = if app.active_tab == 0 {
            Style::default().add_modifier(Modifier::REVERSED).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        spans.push(Span::styled(label, style));
        used_width += label_w;
    }

    // Runner tabs (1-indexed in active_tab)
    for (i, tab) in app.runner_tabs.iter().enumerate() {
        let tab_idx = i + 1;
        let suffix = match &tab.state {
            RunnerTabState::Running { .. } => String::new(),
            RunnerTabState::Done => " \u{2713}".to_string(),  // ✓
            RunnerTabState::Error(_) => " !".to_string(),
        };
        let label = format!("[{}{}]", tab.workflow_name, suffix);
        let label_w = label.chars().count() as u16;
        if used_width + label_w > area.width {
            break;
        }
        let style = if app.active_tab == tab_idx {
            Style::default().add_modifier(Modifier::REVERSED).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        spans.push(Span::styled(label, style));
        used_width += label_w;
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Returns a centered `Rect` of the given dimensions inside `area`.
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    let actual_width = width.min(area.width);
    let actual_height = height.min(area.height);
    Rect::new(x, y, actual_width, actual_height)
}

fn draw_delete_workflow_dialog(frame: &mut Frame, area: Rect, name: &str) {
    // 52 chars wide (2 border + content), 3 rows tall (2 border + 1 content line)
    let dialog_rect = centered_rect(52, 3, area);
    frame.render_widget(Clear, dialog_rect);

    let text = format!("Delete workflow '{name}'? [y/N]");
    let block = Block::default().borders(Borders::ALL).title("Delete Workflow");
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
        Line::from("Task done. Continue? [Y/n]"),
        Line::from(format!("Next: {next_id}: {next_title}")),
    ];
    let block = Block::default().borders(Borders::ALL).title("Continue?");
    frame.render_widget(Paragraph::new(lines).block(block), dialog_rect);
}

fn draw_new_workflow_dialog(frame: &mut Frame, area: Rect, input: &str, error: &Option<String>) {
    // 72 chars wide (2 border + 70 content), 4 rows tall (2 border + 2 content)
    let dialog_rect = centered_rect(72, 4, area);
    frame.render_widget(Clear, dialog_rect);

    let prompt = format!("New workflow name: {}_", input);
    let lines: Vec<Line> = match error {
        Some(err) => vec![
            Line::from(prompt),
            Line::from(Span::styled(err.as_str(), Style::default().fg(Color::Red))),
        ],
        None => vec![Line::from(prompt), Line::from("")],
    };

    let block = Block::default().borders(Borders::ALL).title("New Workflow");
    frame.render_widget(Paragraph::new(lines).block(block), dialog_rect);
}

fn draw_help_dialog(frame: &mut Frame, area: Rect) {
    // 46 wide (2 border + 44 content), 10 tall (2 border + 8 keybinding rows)
    let dialog_rect = centered_rect(46, 10, area);
    frame.render_widget(Clear, dialog_rect);

    let lines = vec![
        Line::from("  j/k/\u{2191}\u{2193}   navigate workflows"),
        Line::from("  r         run ralph loop"),
        Line::from("  s         stop loop"),
        Line::from("  n         new workflow"),
        Line::from("  e         edit prd.json"),
        Line::from("  d         delete workflow"),
        Line::from("  ?         help"),
        Line::from("  q         quit"),
    ];
    let block = Block::default().borders(Borders::ALL).title("Help");
    frame.render_widget(Paragraph::new(lines).block(block), dialog_rect);
}
