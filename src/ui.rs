use crate::app::{App, Dialog, RunnerTabState};
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

    let content = outer[1];

    if app.active_tab == 0 {
        draw_workflows_tab(frame, app, content);
    } else {
        draw_runner_tab(frame, app, content);
    }

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

/// Renders the Workflows tab: workflow list (left) | tasks (right) | log panel | status bar.
fn draw_workflows_tab(frame: &mut Frame, app: &App, area: Rect) {
    // Outer vertical split: top panels (~75%) | log (~20%) | status bar (1 line)
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(75),
            Constraint::Percentage(20),
            Constraint::Length(1),
        ])
        .split(area);

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

    // Log panel: empty on the Workflows tab (runner logs live on runner tabs).
    let log_block = Block::default().borders(Borders::ALL).title("Log");
    frame.render_widget(log_block, vertical[1]);

    // Status bar: workflow management hints.
    let status_text = if let Some(msg) = &app.status_message {
        Line::from(Span::styled(msg.as_str(), Style::default().fg(Color::Red)))
    } else {
        Line::from("[r]un  [n]ew  [e]dit  [d]elete  [?]help  [q]uit")
    };
    frame.render_widget(Paragraph::new(status_text), vertical[2]);
}

/// Renders an active runner tab: log panel (top) | status line (bottom).
///
/// Layout (from top to bottom):
///   log view  — flexible height, scrollable; log_scroll==0 auto-scrolls to newest line
///   status line — 1 line: shows Running/Done/Error state or a transient status message
///
/// Keyboard input is forwarded directly to the PTY as raw bytes (no input buffer row).
fn draw_runner_tab(frame: &mut Frame, app: &App, area: Rect) {
    let tab = match app.runner_tabs.get(app.active_tab - 1) {
        Some(t) => t,
        None => return,
    };

    // Split: log panel (flexible) | status line (1 line)
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    // Log panel — rendered by tui_term::widget::PseudoTerminal from the vt100 screen.
    // The vt100 screen's scrollback position (set_scrollback) is updated by the key handlers
    // so that PseudoTerminal renders the correct view (scrollback or live) without requiring
    // a mutable App reference in draw. When log_scroll == 0 the live screen is shown;
    // when log_scroll > 0 the screen is offset into the scrollback buffer.
    let log_title = format!("Runner: {} — Up/k scroll up  End/G bottom", tab.workflow_name);
    let log_block = Block::default().borders(Borders::ALL).title(log_title);
    {
        use tui_term::widget::PseudoTerminal;
        let pseudo_term = PseudoTerminal::new(tab.parser.screen()).block(log_block);
        frame.render_widget(pseudo_term, layout[0]);
    }

    // Status line: transient messages take priority; otherwise show runner state.
    let status_text = if let Some(msg) = &app.status_message {
        Line::from(Span::styled(msg.as_str(), Style::default().fg(Color::Red)))
    } else {
        match &tab.state {
            RunnerTabState::Running { iteration } => {
                Line::from(format!("[s]top  Running \u{2014} iteration {}/10", iteration))
            }
            RunnerTabState::Done => Line::from("[x]close  Done"),
            RunnerTabState::Error(msg) => {
                let prefix = "Error: ";
                let suffix = "  [x]close  [q]uit";
                let avail = layout[1].width as usize;
                let max_msg = avail.saturating_sub(prefix.len() + suffix.len());
                let truncated: String = msg.chars().take(max_msg).collect();
                Line::from(Span::styled(
                    format!("{prefix}{truncated}{suffix}"),
                    Style::default().fg(Color::Red),
                ))
            }
        }
    };
    frame.render_widget(Paragraph::new(status_text), layout[1]);

}

/// Renders the single-line tab bar at the top of the screen.
///
/// Tabs are space-padded and separated by `│`. The active tab is REVERSED+BOLD;
/// inactive tabs are dimmed. Any remaining width is filled to extend the bar.
fn draw_tab_bar(frame: &mut Frame, app: &App, area: Rect) {
    // Build the full list of (label, is_active) entries up front.
    let mut entries: Vec<(String, bool)> = Vec::new();

    entries.push((" Workflows ".to_string(), app.active_tab == 0));

    for (i, tab) in app.runner_tabs.iter().enumerate() {
        let suffix = match &tab.state {
            RunnerTabState::Running { .. } => "",
            RunnerTabState::Done => " \u{2713}",  // ✓
            RunnerTabState::Error(_) => " !",
        };
        entries.push((
            format!(" {}{} ", tab.workflow_name, suffix),
            app.active_tab == i + 1,
        ));
    }

    let sep_style = Style::default().fg(Color::DarkGray);
    let inactive_style = Style::default().fg(Color::DarkGray);
    let active_style =
        Style::default().add_modifier(Modifier::REVERSED).add_modifier(Modifier::BOLD);

    let mut spans: Vec<Span> = Vec::new();
    let mut used_width: u16 = 0;

    for (idx, (label, is_active)) in entries.iter().enumerate() {
        // Separator between tabs (not before the first one).
        if idx > 0 {
            if used_width + 1 > area.width {
                break;
            }
            spans.push(Span::styled("\u{2502}", sep_style)); // │
            used_width += 1;
        }

        let label_w = label.chars().count() as u16;
        if used_width + label_w > area.width {
            break;
        }

        let style = if *is_active { active_style } else { inactive_style };
        spans.push(Span::styled(label.as_str(), style));
        used_width += label_w;
    }

    // Fill the rest of the bar so it reads as a continuous strip.
    if used_width < area.width {
        let pad = " ".repeat((area.width - used_width) as usize);
        spans.push(Span::raw(pad));
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
