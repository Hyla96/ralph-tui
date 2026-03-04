use crate::app::{
    App, Dialog, PrdEditorField, PrdEditorMode, PrdEditorState, PrdsFocus, RunnerTab,
    RunnerTabState, StoryDetailField,
};
use crate::ralph::usage::UsageFile;
use crate::ralph::workflow::Workflow;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};

pub fn draw(frame: &mut Frame, app: &App) {
    // Full-screen PRD metadata editor takes over the entire frame.
    if let Some(editor) = &app.prd_editor {
        draw_prd_editor(frame, editor, frame.area());
        return;
    }

    // Top-level split: tab bar (1 line) | content area (rest)
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(frame.area());

    draw_tab_bar(frame, app, outer[0]);

    let content = outer[1];

    if app.active_tab == 0 {
        draw_prds_tab(frame, app, content);
    } else if app.active_tab == 1 {
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
        Some(Dialog::ContinuePrompt {
            next_id,
            next_title,
        }) => {
            draw_continue_prompt_dialog(frame, frame.area(), next_id, next_title);
        }
        Some(Dialog::Help) => {
            draw_help_dialog(frame, frame.area());
        }
        Some(Dialog::RunnerHelp) => {
            draw_runner_help_dialog(frame, frame.area());
        }
        Some(Dialog::ImportPrd {
            input,
            error,
            confirm_overwrite,
            ..
        }) => {
            draw_import_prd_dialog(frame, frame.area(), input, error, *confirm_overwrite);
        }
        Some(Dialog::QuitConfirm) => {
            draw_quit_confirm_dialog(frame, frame.area());
        }
        Some(Dialog::StopConfirm) => {
            draw_stop_confirm_dialog(frame, frame.area());
        }
        None => {}
    }
}

/// Renders the PRDs tab: file list (left 30%) | content preview (right 70%) | status bar.
fn draw_prds_tab(frame: &mut Frame, app: &App, area: Rect) {
    // Vertical split: main content (flexible) | status bar (1 line)
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    // Horizontal split: file list (30%) | content preview (70%)
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(vertical[0]);

    let focused_style = Style::default().fg(Color::Yellow);

    // Left pane: file list
    let list_focused = app.prds_tab.focus == PrdsFocus::List;
    let list_border_style = if list_focused {
        focused_style
    } else {
        Style::default()
    };
    let list_title = format!("PRDs ({})", app.prds_tab.files.len());
    let list_block = Block::default()
        .borders(Borders::ALL)
        .title(list_title)
        .border_style(list_border_style);

    if app.prds_tab.files.is_empty() {
        let empty_msg = Paragraph::new("No PRDs found in tasks/").block(list_block);
        frame.render_widget(empty_msg, panes[0]);
    } else {
        let items: Vec<ListItem> = app
            .prds_tab
            .files
            .iter()
            .map(|name| ListItem::new(name.as_str()))
            .collect();
        let list = List::new(items)
            .block(list_block)
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        let mut list_state = ListState::default().with_selected(app.prds_tab.selected);
        frame.render_stateful_widget(list, panes[0], &mut list_state);
    }

    // Right pane: content preview
    let content_focused = app.prds_tab.focus == PrdsFocus::Content;
    let content_border_style = if content_focused {
        focused_style
    } else {
        Style::default()
    };
    let preview_title = app
        .prds_tab
        .selected
        .and_then(|i| app.prds_tab.files.get(i))
        .map(|s| s.as_str())
        .unwrap_or("Preview");
    let content_block = Block::default()
        .borders(Borders::ALL)
        .title(preview_title)
        .border_style(content_border_style);
    let content_para = Paragraph::new(app.prds_tab.content.as_str())
        .block(content_block)
        .scroll((app.prds_tab.scroll, 0));
    frame.render_widget(content_para, panes[1]);

    // Status bar
    let status_line = if app.prds_tab.focus == PrdsFocus::List {
        "[j/↓] down  [k/↑] up  [Enter] preview  [Tab] switch tab  [q]uit"
    } else {
        "[j/↓] scroll down  [k/↑] scroll up  [Esc] back to list  [Tab] switch tab  [q]uit"
    };
    frame.render_widget(Paragraph::new(status_line), vertical[1]);
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
        let empty_msg = Paragraph::new("No workflows. Press [n] to create one.").block(plans_block);
        frame.render_widget(empty_msg, top[0]);
    } else {
        let items: Vec<ListItem> = app
            .workflows
            .iter()
            .map(|name| {
                let has_src = app
                    .store
                    .workflow_dir(name)
                    .join("prd-source.md")
                    .exists();
                let label = if has_src {
                    format!("{name} [src]")
                } else {
                    name.clone()
                };
                ListItem::new(label)
            })
            .collect();
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
            // Load usage file once for per-task token display and title aggregate.
            let usage_file = app
                .selected_workflow
                .and_then(|i| app.workflows.get(i))
                .map(|name| {
                    let dir = app.store.workflow_dir(name);
                    UsageFile::load(&dir).unwrap_or_default()
                })
                .unwrap_or_default();

            let total_tokens = usage_file.total.input_tokens + usage_file.total.output_tokens;
            let title = if total_tokens > 0 {
                format!(
                    "Tasks ({}/{})  {}",
                    workflow.done_count(),
                    workflow.total_count(),
                    format_tokens(total_tokens)
                )
            } else {
                format!(
                    "Tasks ({}/{})",
                    workflow.done_count(),
                    workflow.total_count()
                )
            };
            let block = Block::default().borders(Borders::ALL).title(title);

            let items: Vec<ListItem> = workflow
                .prd
                .tasks
                .iter()
                .map(|task| {
                    let check = if task.passes { "✓" } else { "○" };
                    let tok_suffix = if task.passes {
                        usage_file
                            .tasks
                            .get(&task.id)
                            .map(|entry| {
                                let total = entry.input_tokens + entry.output_tokens;
                                format!("  {}", format_tokens(total))
                            })
                            .unwrap_or_default()
                    } else {
                        String::new()
                    };
                    let text = format!(
                        "{} [{}] {}: {}{}",
                        check, task.priority, task.id, task.title, tok_suffix
                    );
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

    // Log panel: shows synthesis PTY output when active, empty otherwise.
    if let Some(parser) = &app.synth_parser {
        let synth_name = app.synth_workflow_name.as_deref().unwrap_or("unknown");
        let synthesizing = app.is_synthesizing();
        let title_text = if synthesizing {
            format!("Synthesis: {} \u{2014} running", synth_name)
        } else {
            format!("Synthesis: {}", synth_name)
        };
        let log_block = Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(title_text, Style::default().fg(CLAUDE_ORANGE)));
        use tui_term::widget::PseudoTerminal;
        let pseudo_term = PseudoTerminal::new(parser.screen()).block(log_block);
        frame.render_widget(pseudo_term, vertical[1]);
    } else {
        let log_block = Block::default().borders(Borders::ALL).title("Log");
        frame.render_widget(log_block, vertical[1]);
    }

    // Status bar: workflow management hints + optional right-aligned notification.
    let notif = app.notification.as_ref().map(|(s, _)| s.as_str());
    let bar_width = vertical[2].width;
    let status_text = if let Some(msg) = &app.status_message {
        let left = msg.as_str();
        let left_len = left.chars().count();
        let mut spans = vec![Span::styled(
            left.to_string(),
            Style::default().fg(Color::Red),
        )];
        if let Some(n) = notif {
            spans.extend(notification_right_spans(left_len, n, bar_width));
        }
        Line::from(spans)
    } else if app.is_synthesizing() {
        let left = "[s]top  Synthesizing\u{2026}";
        let left_len = left.chars().count();
        let mut spans = vec![Span::styled(
            left.to_string(),
            Style::default().fg(CLAUDE_ORANGE),
        )];
        if let Some(n) = notif {
            spans.extend(notification_right_spans(left_len, n, bar_width));
        }
        Line::from(spans)
    } else {
        let left = "[r]un  [S]ynth  [n]ew  [i]mport  [e]dit  [d]elete  [?]help  [q]uit";
        let left_len = left.chars().count();
        let mut spans = vec![Span::raw(left)];
        if let Some(n) = notif {
            spans.extend(notification_right_spans(left_len, n, bar_width));
        }
        Line::from(spans)
    };
    frame.render_widget(Paragraph::new(status_text), vertical[2]);
}

/// Renders an active runner tab: log panel (top) | status line (bottom).
///
/// Layout (from top to bottom):
///   task bar    — 1 row: task title left-aligned, counts right-aligned
///   PTY viewport — flexible height, scrollable via vt100 scrollback
///   buttons bar — 1 row: action keybindings only (no task context)
///
/// Keyboard input is forwarded directly to the PTY as raw bytes (no input buffer row).
fn draw_runner_tab(frame: &mut Frame, app: &App, area: Rect) {
    let tab = match app.runner_tabs.get(app.active_tab - 2) {
        Some(t) => t,
        None => return,
    };

    // 3-section layout: task bar (1 row) | PTY viewport (flexible) | buttons bar (1 row)
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    // Task bar (layout[0]): task title left-aligned, counts (tasks/iter/tokens) right-aligned.
    // For Error state: show "Runner: {workflow_name}" dimmed. Insert mode uses same as Running/Done.
    let task_bar_width = layout[0].width;
    let task_bar_line = match &tab.state {
        RunnerTabState::Error(_) => Line::from(Span::styled(
            format!("Runner: {}", tab.workflow_name),
            Style::default().fg(Color::DarkGray),
        )),
        state => {
            let iter_n = match state {
                RunnerTabState::Running { iteration } => *iteration,
                RunnerTabState::Done | RunnerTabState::Stopped => tab.iterations_used,
                RunnerTabState::Error(_) => unreachable!(),
            };
            // Left side: task title truncated to 40 visible chars.
            let task_title = match &tab.current_task_title {
                Some(t) => {
                    let chars: Vec<char> = t.chars().collect();
                    if chars.len() > 40 {
                        let truncated: String = chars.iter().take(39).collect();
                        format!("{truncated}…")
                    } else {
                        t.clone()
                    }
                }
                None => "unknown".to_string(),
            };
            // Right side: "{done}/{total} tasks  iter {n}  {token_str}"
            let workflow_dir = app.store.workflow_dir(&tab.workflow_name);
            let (done, total) = Workflow::load(&workflow_dir)
                .map(|w| (w.done_count(), w.total_count()))
                .unwrap_or((0, 0));
            let token_str = match state {
                RunnerTabState::Running { .. } => {
                    let task_tokens =
                        tab.current_story_input_tokens + tab.current_story_output_tokens;
                    match UsageFile::load(&workflow_dir) {
                        Ok(usage) => {
                            let session_tokens =
                                usage.total.input_tokens + usage.total.output_tokens + task_tokens;
                            format!(
                                "task: {}  session: {}",
                                format_tokens(task_tokens),
                                format_tokens(session_tokens)
                            )
                        }
                        Err(_) => format!("task: {}", format_tokens(task_tokens)),
                    }
                }
                RunnerTabState::Done | RunnerTabState::Stopped => {
                    match UsageFile::load(&workflow_dir) {
                        Ok(usage) => {
                            let session_tokens =
                                usage.total.input_tokens + usage.total.output_tokens;
                            format!("session: {}", format_tokens(session_tokens))
                        }
                        Err(_) => "session: ? tok".to_string(),
                    }
                }
                RunnerTabState::Error(_) => unreachable!(),
            };
            let right_str = format!("{done}/{total} tasks  iter {iter_n}  {token_str}");
            let left_len = task_title.chars().count();
            let mut spans = vec![Span::raw(task_title)];
            spans.extend(notification_right_spans(
                left_len,
                &right_str,
                task_bar_width,
            ));
            Line::from(spans)
        }
    };
    frame.render_widget(Paragraph::new(task_bar_line), layout[0]);

    // PTY viewport (layout[1]): border title is now "Runner: {workflow_name}" only.
    // The vt100 scrollback position (set_scrollback) is updated by key handlers so that
    // PseudoTerminal renders the correct view without needing a mutable App reference.
    let log_title_text = format!("Runner: {}", tab.workflow_name);
    let log_block = Block::default().borders(Borders::ALL).title(Span::styled(
        log_title_text,
        Style::default().fg(CLAUDE_ORANGE),
    ));
    {
        use tui_term::widget::PseudoTerminal;
        let pseudo_term = PseudoTerminal::new(tab.parser.screen()).block(log_block);
        frame.render_widget(pseudo_term, layout[1]);
    }

    // Buttons bar (layout[2]): action keybindings only; no task context on the right side.
    // Insert mode: INSERT indicator (green). Status message: red text. State-based: keybindings.
    let insert_mode = tab.insert_mode;
    let buttons_line = if insert_mode {
        Line::from(Span::styled(
            "-- INSERT --  [Esc] normal mode",
            Style::default().fg(Color::Green),
        ))
    } else if let Some(msg) = &app.status_message {
        Line::from(Span::styled(
            msg.to_string(),
            Style::default().fg(Color::Red),
        ))
    } else {
        match &tab.state {
            RunnerTabState::Running { .. } => {
                let auto_label = if tab.auto_continue {
                    "[a]uto:ON"
                } else {
                    "[a]uto:OFF"
                };
                // [c]continue is never shown in Running state regardless of auto mode.
                Line::from(Span::raw(format!(
                    "[i]nsert  [s]stop  {auto_label}  [?]help  [q]uit"
                )))
            }
            RunnerTabState::Done => {
                // [c]continue only when auto OFF and workflow not complete.
                let workflow_complete = app.is_workflow_complete(&tab.workflow_name);
                let show_continue = !tab.auto_continue && !workflow_complete;
                if show_continue {
                    Line::from(vec![
                        Span::raw("[c]continue  "),
                        Span::raw("[x]close  [?]help"),
                    ])
                } else {
                    Line::from(Span::raw("[x]close  [?]help"))
                }
            }
            RunnerTabState::Stopped => Line::from(Span::raw("[x]close  [r]estart  [?]help")),
            RunnerTabState::Error(_) => Line::from(Span::styled(
                "[x]close  [q]quit  [?]help",
                Style::default().fg(Color::Red),
            )),
        }
    };
    frame.render_widget(Paragraph::new(buttons_line), layout[2]);
}

// Claude brand orange (#DA7756).
const CLAUDE_ORANGE: Color = Color::Rgb(218, 119, 86);

/// Formats a token count with comma thousands-separators and appends " tok".
/// Example: format_tokens(12345) → "12,345 tok"
fn format_tokens(n: u64) -> String {
    let s = n.to_string();
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    let mut result = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(c);
    }
    format!("{result} tok")
}

/// Renders the single-line tab bar at the top of the screen.
///
/// Tabs are space-padded and separated by `│`. The active tab is REVERSED+BOLD;
/// inactive tabs are dimmed. Runner tabs use the Claude brand orange.
fn draw_tab_bar(frame: &mut Frame, app: &App, area: Rect) {
    // Build the full list of (label, is_active, is_runner, is_done) entries up front.
    let mut entries: Vec<(String, bool, bool, bool)> = Vec::new();

    entries.push((" [1] PRDs ".to_string(), app.active_tab == 0, false, false));
    entries.push((" [2] Workflows ".to_string(), app.active_tab == 1, false, false));

    for (i, tab) in app.runner_tabs.iter().enumerate() {
        let suffix = match &tab.state {
            RunnerTabState::Running { .. } => "",
            RunnerTabState::Done => " \u{2713}",    // ✓
            RunnerTabState::Stopped => " \u{25a0}", // ■
            RunnerTabState::Error(_) => " !",
        };
        let is_done = matches!(tab.state, RunnerTabState::Done);
        entries.push((
            format!(" [{}] {}{} ", i + 3, tab.workflow_name, suffix),
            app.active_tab == i + 2,
            true,
            is_done,
        ));
    }

    let sep_style = Style::default().fg(Color::DarkGray);
    let inactive_style = Style::default().fg(Color::DarkGray);
    let active_style = Style::default()
        .add_modifier(Modifier::REVERSED)
        .add_modifier(Modifier::BOLD);
    let runner_inactive_style = Style::default()
        .fg(CLAUDE_ORANGE)
        .add_modifier(Modifier::DIM);
    let runner_active_style = Style::default()
        .fg(CLAUDE_ORANGE)
        .add_modifier(Modifier::REVERSED)
        .add_modifier(Modifier::BOLD);
    let done_inactive_style = Style::default()
        .fg(Color::Green)
        .add_modifier(Modifier::DIM);
    let done_active_style = Style::default()
        .fg(Color::Green)
        .add_modifier(Modifier::REVERSED)
        .add_modifier(Modifier::BOLD);

    let mut spans: Vec<Span> = Vec::new();
    let mut used_width: u16 = 0;

    for (idx, (label, is_active, is_runner, is_done)) in entries.iter().enumerate() {
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

        let style = match (is_active, is_runner, is_done) {
            (true, true, true) => done_active_style,
            (false, true, true) => done_inactive_style,
            (true, true, false) => runner_active_style,
            (false, true, false) => runner_inactive_style,
            (true, false, _) => active_style,
            (false, false, _) => inactive_style,
        };
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

/// Builds the right-aligned task context string for a runner tab status bar.
///
/// Returns `None` for Error state (no task context shown) or when the workflow
/// cannot be loaded (should not normally happen). Format:
///   `"{task_title}  {done}/{total} tasks  iter {n}"`
/// where `task_title` is truncated to 30 visible chars with a `…` suffix if needed.
fn runner_tab_context(app: &App, tab: &RunnerTab) -> Option<String> {
    let iter_n = match &tab.state {
        RunnerTabState::Running { iteration } => *iteration,
        RunnerTabState::Done | RunnerTabState::Stopped => tab.iterations_used,
        RunnerTabState::Error(_) => return None,
    };

    let task_title = match &tab.current_task_title {
        Some(t) => {
            let chars: Vec<char> = t.chars().collect();
            if chars.len() > 30 {
                let truncated: String = chars.iter().take(29).collect();
                format!("{truncated}…")
            } else {
                t.clone()
            }
        }
        None => "unknown".to_string(),
    };

    let workflow_dir = app.store.workflow_dir(&tab.workflow_name);
    let (done, total) = Workflow::load(&workflow_dir)
        .map(|w| (w.done_count(), w.total_count()))
        .unwrap_or((0, 0));

    let token_str = match &tab.state {
        RunnerTabState::Running { .. } => {
            let task_tokens = tab.current_story_input_tokens + tab.current_story_output_tokens;
            match UsageFile::load(&workflow_dir) {
                Ok(usage) => {
                    let session_tokens =
                        usage.total.input_tokens + usage.total.output_tokens + task_tokens;
                    format!(
                        "task: {}  session: {}",
                        format_tokens(task_tokens),
                        format_tokens(session_tokens)
                    )
                }
                Err(_) => format!("task: {}", format_tokens(task_tokens)),
            }
        }
        RunnerTabState::Done | RunnerTabState::Stopped => match UsageFile::load(&workflow_dir) {
            Ok(usage) => {
                let session_tokens = usage.total.input_tokens + usage.total.output_tokens;
                format!("session: {}", format_tokens(session_tokens))
            }
            Err(_) => "session: ? tok".to_string(),
        },
        RunnerTabState::Error(_) => unreachable!(),
    };

    Some(format!(
        "{task_title}  {done}/{total} tasks  iter {iter_n}  {token_str}"
    ))
}

/// Builds right-aligned notification spans to append to a status bar line.
///
/// Returns padding + styled notification text as `Span<'static>` values, or an empty
/// `Vec` if there is no room (left content fills the bar).
fn notification_right_spans(
    left_visible_width: usize,
    notif: &str,
    bar_width: u16,
) -> Vec<Span<'static>> {
    let total = bar_width as usize;
    let available = total.saturating_sub(left_visible_width);
    // Need at least 2 chars: 1 space separator + 1 content character.
    if available < 2 {
        return Vec::new();
    }
    let max_display = available - 1; // 1 char reserved for the space separator
    let notif_chars: Vec<char> = notif.chars().collect();
    let notif_len = notif_chars.len();
    let style = Style::default().fg(Color::Cyan);
    if notif_len <= max_display {
        // Fits without truncation: pad to right-align.
        let padding = available - notif_len;
        vec![
            Span::raw(" ".repeat(padding)),
            Span::styled(notif.to_string(), style),
        ]
    } else if max_display >= 2 {
        // Truncate with ellipsis; reserve 1 char for `…`.
        let text_len = max_display - 1;
        let truncated: String = notif_chars.iter().take(text_len).collect();
        vec![
            Span::raw(" ".to_string()),
            Span::styled(format!("{}…", truncated), style),
        ]
    } else {
        Vec::new()
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

fn draw_delete_workflow_dialog(frame: &mut Frame, area: Rect, name: &str) {
    // 52 chars wide (2 border + content), 3 rows tall (2 border + 1 content line)
    let dialog_rect = centered_rect(52, 3, area);
    frame.render_widget(Clear, dialog_rect);

    let text = format!("Delete workflow '{name}'? [y/N]");
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Delete Workflow");
    frame.render_widget(Paragraph::new(text).block(block), dialog_rect);
}

fn draw_quit_confirm_dialog(frame: &mut Frame, area: Rect) {
    // 38 chars wide (2 border + 36 content), 3 rows tall (2 border + 1 content line)
    let dialog_rect = centered_rect(38, 3, area);
    frame.render_widget(Clear, dialog_rect);

    let block = Block::default().borders(Borders::ALL).title("Quit");
    frame.render_widget(
        Paragraph::new("Quit ralph-tui? [y/N]").block(block),
        dialog_rect,
    );
}

fn draw_stop_confirm_dialog(frame: &mut Frame, area: Rect) {
    // 46 chars wide (2 border + 44 content), 3 rows tall (2 border + 1 content line)
    let dialog_rect = centered_rect(46, 3, area);
    frame.render_widget(Clear, dialog_rect);

    let block = Block::default().borders(Borders::ALL).title("Stop");
    frame.render_widget(
        Paragraph::new("Workflow not complete. Stop? [y/N]").block(block),
        dialog_rect,
    );
}

fn draw_continue_prompt_dialog(frame: &mut Frame, area: Rect, next_id: &str, next_title: &str) {
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

fn draw_import_prd_dialog(
    frame: &mut Frame,
    area: Rect,
    input: &str,
    error: &Option<String>,
    confirm_overwrite: bool,
) {
    // 72 chars wide (2 border + 70 content), 4 rows tall (2 border + 2 content lines)
    let dialog_rect = centered_rect(72, 4, area);
    frame.render_widget(Clear, dialog_rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .title("Import PRD File");

    let lines: Vec<Line> = if confirm_overwrite {
        vec![
            Line::from("Overwrite existing prd-source.md? [y/N]"),
            Line::from(""),
        ]
    } else {
        let prompt = format!("Import PRD file: {}_", input);
        match error {
            Some(err) => vec![
                Line::from(prompt),
                Line::from(Span::styled(err.as_str(), Style::default().fg(Color::Red))),
            ],
            None => vec![Line::from(prompt), Line::from("")],
        }
    };

    frame.render_widget(Paragraph::new(lines).block(block), dialog_rect);
}

fn draw_help_dialog(frame: &mut Frame, area: Rect) {
    // 52 wide (2 border + 50 content), 13 tall (2 border + 11 keybinding rows)
    let dialog_rect = centered_rect(52, 13, area);
    frame.render_widget(Clear, dialog_rect);

    let lines = vec![
        Line::from("  j/k/\u{2191}\u{2193}   navigate workflows"),
        Line::from("  r         run ralph loop"),
        Line::from("  s         stop loop / synthesis"),
        Line::from("  S         synthesize prd.json from prd-source.md"),
        Line::from("  n         new workflow"),
        Line::from("  i         import PRD file"),
        Line::from("  e         edit prd.json"),
        Line::from("  E         open form editor"),
        Line::from("  d         delete workflow"),
        Line::from("  ?         help"),
        Line::from("  q         quit"),
    ];
    let block = Block::default().borders(Borders::ALL).title("Help");
    frame.render_widget(Paragraph::new(lines).block(block), dialog_rect);
}

fn draw_runner_help_dialog(frame: &mut Frame, area: Rect) {
    // 52 wide (2 border + 50 content), 19 tall (2 border + 17 content rows)
    let dialog_rect = centered_rect(52, 19, area);
    frame.render_widget(Clear, dialog_rect);

    let header_style = Style::default().add_modifier(Modifier::BOLD);
    let lines = vec![
        Line::from(Span::styled("  -- Normal mode --", header_style)),
        Line::from("  i           enter insert mode"),
        Line::from("  s           stop loop"),
        Line::from("  a           toggle auto-continue"),
        Line::from("  \u{2191}/k         scroll up"),
        Line::from("  \u{2193}/j         scroll down"),
        Line::from("  End/G       jump to bottom"),
        Line::from("  Tab         next tab"),
        Line::from("  Shift+Tab   prev tab"),
        Line::from("  t+1..9      switch tab by number"),
        Line::from("  x           close tab"),
        Line::from("  ?           this help"),
        Line::from("  q           quit"),
        Line::from(""),
        Line::from(Span::styled("  -- Insert mode --", header_style)),
        Line::from("  Esc         back to normal mode"),
        Line::from("  Ctrl+C      send interrupt to PTY"),
    ];
    let block = Block::default().borders(Borders::ALL).title("Runner Help");
    frame.render_widget(Paragraph::new(lines).block(block), dialog_rect);
}

/// Renders the full-screen PRD editor.
///
/// Dispatches to the appropriate sub-renderer based on the active mode:
///   - StoryDetail: placeholder panel (to be fleshed out in US-003)
///   - Metadata / StoryList: three metadata fields + story list below
fn draw_prd_editor(frame: &mut Frame, editor: &PrdEditorState, area: Rect) {
    let title = format!(" PRD Editor: {} ", editor.workflow_name);
    let outer_block = Block::default().borders(Borders::ALL).title(title);
    let inner_area = outer_block.inner(area);
    frame.render_widget(outer_block, area);

    match editor.mode {
        PrdEditorMode::StoryDetail => draw_prd_story_detail(frame, editor, inner_area),
        PrdEditorMode::Metadata | PrdEditorMode::StoryList => {
            draw_prd_metadata_and_stories(frame, editor, inner_area);
        }
    }
}

/// Renders the metadata fields (Project / Branch / Description) and the story list below.
///
/// Layout (inside the outer border):
///   Project field   — 3 rows (bordered)
///   Branch field    — 3 rows (bordered)
///   Description field — 3 rows (bordered)
///   Stories list    — flexible (bordered, scrollable)
///   hint / status   — 1 row
///
/// Active section border is highlighted yellow; focused metadata field shows `_` cursor.
fn draw_prd_metadata_and_stories(frame: &mut Frame, editor: &PrdEditorState, area: Rect) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Project
            Constraint::Length(3), // Branch
            Constraint::Length(3), // Description
            Constraint::Length(5), // Validation Commands
            Constraint::Min(0),    // Stories list
            Constraint::Length(1), // hint / status
        ])
        .split(area);

    let active_style = Style::default().fg(Color::Yellow);
    let is_metadata = editor.mode == PrdEditorMode::Metadata;

    // Project field
    let focused = is_metadata && editor.focused_field == PrdEditorField::Project;
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Project")
        .border_style(if focused {
            active_style
        } else {
            Style::default()
        });
    let text = if focused {
        format!("{}_", editor.project)
    } else {
        editor.project.clone()
    };
    frame.render_widget(Paragraph::new(text).block(block), layout[0]);

    // Branch field
    let focused = is_metadata && editor.focused_field == PrdEditorField::Branch;
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Branch")
        .border_style(if focused {
            active_style
        } else {
            Style::default()
        });
    let text = if focused {
        format!("{}_", editor.branch)
    } else {
        editor.branch.clone()
    };
    frame.render_widget(Paragraph::new(text).block(block), layout[1]);

    // Description field
    let focused = is_metadata && editor.focused_field == PrdEditorField::Description;
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Description")
        .border_style(if focused {
            active_style
        } else {
            Style::default()
        });
    let text = if focused {
        format!("{}_", editor.description)
    } else {
        editor.description.clone()
    };
    frame.render_widget(Paragraph::new(text).block(block), layout[2]);

    // Validation Commands field (multi-line list)
    let val_cmd_focused = is_metadata && editor.focused_field == PrdEditorField::ValidationCommands;
    let val_cmd_title = format!("Validation Commands ({})", editor.validation_commands.len());
    let val_cmd_block = Block::default()
        .borders(Borders::ALL)
        .title(val_cmd_title)
        .border_style(if val_cmd_focused {
            active_style
        } else {
            Style::default()
        });

    if editor.validation_commands.is_empty() {
        let msg = Paragraph::new("No validation commands. Press [Enter] to add one.")
            .block(val_cmd_block);
        frame.render_widget(msg, layout[3]);
    } else {
        let items: Vec<ListItem> = editor
            .validation_commands
            .iter()
            .enumerate()
            .map(|(i, cmd)| {
                let text = if val_cmd_focused && i == editor.validation_commands_cursor {
                    format!("{}_", cmd)
                } else {
                    cmd.clone()
                };
                ListItem::new(text)
            })
            .collect();
        let list = List::new(items)
            .block(val_cmd_block)
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        let mut list_state = ListState::default().with_selected(if val_cmd_focused {
            Some(editor.validation_commands_cursor)
        } else {
            None
        });
        frame.render_stateful_widget(list, layout[3], &mut list_state);
    }

    // Stories list panel
    let stories_focused = editor.mode == PrdEditorMode::StoryList;
    let stories_title = format!("Stories ({})", editor.stories.len());
    let stories_block = Block::default()
        .borders(Borders::ALL)
        .title(stories_title)
        .border_style(if stories_focused {
            active_style
        } else {
            Style::default()
        });

    if editor.stories.is_empty() {
        let msg = Paragraph::new("No stories. Press [a] to add one.").block(stories_block);
        frame.render_widget(msg, layout[4]);
    } else {
        let items: Vec<ListItem> = editor
            .stories
            .iter()
            .enumerate()
            .map(|(i, story)| {
                let text = format!("[{}] {}: {}", i + 1, story.id, story.title);
                ListItem::new(text)
            })
            .collect();
        let list = List::new(items)
            .block(stories_block)
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        let mut list_state = ListState::default().with_selected(editor.selected_story);
        frame.render_stateful_widget(list, layout[4], &mut list_state);
    }

    // Hint / status line
    let hint = if let Some(del_idx) = editor.confirm_delete {
        let story_id = editor
            .stories
            .get(del_idx)
            .map(|s| s.id.as_str())
            .unwrap_or("?");
        Line::from(Span::styled(
            format!("Delete story {story_id}? [y/N]"),
            Style::default().fg(Color::Yellow),
        ))
    } else if let Some(err) = &editor.status {
        Line::from(Span::styled(err.as_str(), Style::default().fg(Color::Red)))
    } else if stories_focused {
        Line::from(
            "[↑↓/j/k] navigate  [Enter] edit  [a] add  [x] delete  [Tab] fields  [Ctrl+S] save  [Esc] cancel",
        )
    } else {
        Line::from("[Tab] next field  [Shift+Tab] prev  [Ctrl+S] save  [Esc] cancel")
    };
    frame.render_widget(Paragraph::new(hint), layout[5]);
}

/// Renders the story detail form (US-003).
///
/// Layout (inside the outer border from draw_prd_editor):
///   ID (60%) + Priority (40%)   — 3 rows, side-by-side
///   Title                       — 3 rows
///   Description                 — 3 rows
///   Acceptance Criteria list    — flexible height
///   hint / status               — 1 row
///
/// Active field border is highlighted yellow. Focused text fields append `_` cursor.
/// In the Criteria list, the active line is shown with REVERSED highlight and `_` cursor.
fn draw_prd_story_detail(frame: &mut Frame, editor: &PrdEditorState, area: Rect) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // ID + Priority row
            Constraint::Length(3), // Title
            Constraint::Length(3), // Description
            Constraint::Min(0),    // Acceptance Criteria list
            Constraint::Length(1), // hint / status
        ])
        .split(area);

    let active_style = Style::default().fg(Color::Yellow);

    // --- First row: ID (left 60%) + Priority (right 40%) ---
    let top_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(layout[0]);

    // ID field
    let id_focused = editor.story_focused_field == StoryDetailField::Id;
    let id_block = Block::default()
        .borders(Borders::ALL)
        .title("ID")
        .border_style(if id_focused {
            active_style
        } else {
            Style::default()
        });
    let id_text = if id_focused {
        format!("{}_", editor.story_id)
    } else {
        editor.story_id.clone()
    };
    frame.render_widget(Paragraph::new(id_text).block(id_block), top_row[0]);

    // Priority field
    let prio_focused = editor.story_focused_field == StoryDetailField::Priority;
    let prio_block = Block::default()
        .borders(Borders::ALL)
        .title("Priority")
        .border_style(if prio_focused {
            active_style
        } else {
            Style::default()
        });
    let prio_text = if prio_focused {
        format!("{}_", editor.story_priority)
    } else {
        editor.story_priority.clone()
    };
    frame.render_widget(Paragraph::new(prio_text).block(prio_block), top_row[1]);

    // --- Title field ---
    let title_focused = editor.story_focused_field == StoryDetailField::Title;
    let title_block = Block::default()
        .borders(Borders::ALL)
        .title("Title")
        .border_style(if title_focused {
            active_style
        } else {
            Style::default()
        });
    let title_text = if title_focused {
        format!("{}_", editor.story_title)
    } else {
        editor.story_title.clone()
    };
    frame.render_widget(Paragraph::new(title_text).block(title_block), layout[1]);

    // --- Description field ---
    let desc_focused = editor.story_focused_field == StoryDetailField::Description;
    let desc_block = Block::default()
        .borders(Borders::ALL)
        .title("Description")
        .border_style(if desc_focused {
            active_style
        } else {
            Style::default()
        });
    let desc_text = if desc_focused {
        format!("{}_", editor.story_description)
    } else {
        editor.story_description.clone()
    };
    frame.render_widget(Paragraph::new(desc_text).block(desc_block), layout[2]);

    // --- Acceptance Criteria list ---
    let crit_focused = editor.story_focused_field == StoryDetailField::Criteria;
    let crit_block = Block::default()
        .borders(Borders::ALL)
        .title("Acceptance Criteria  [Enter] add  [x] delete  [↑↓] navigate")
        .border_style(if crit_focused {
            active_style
        } else {
            Style::default()
        });

    if editor.story_criteria.is_empty() {
        let msg = if crit_focused {
            "  (empty — press Enter to add a criterion)"
        } else {
            "  (no criteria)"
        };
        frame.render_widget(Paragraph::new(msg).block(crit_block), layout[3]);
    } else {
        let cursor = editor.story_criteria_cursor;
        let items: Vec<ListItem> = editor
            .story_criteria
            .iter()
            .enumerate()
            .map(|(i, crit)| {
                if crit_focused && i == cursor {
                    ListItem::new(format!("{crit}_"))
                } else {
                    ListItem::new(crit.as_str())
                }
            })
            .collect();
        let list = List::new(items)
            .block(crit_block)
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        let mut list_state =
            ListState::default().with_selected(if crit_focused { Some(cursor) } else { None });
        frame.render_stateful_widget(list, layout[3], &mut list_state);
    }

    // --- Hint / status line ---
    let hint = if let Some(err) = &editor.status {
        Line::from(Span::styled(err.as_str(), Style::default().fg(Color::Red)))
    } else {
        Line::from("[Tab] next field  [Shift+Tab] prev  [Ctrl+S] save  [Esc] back")
    };
    frame.render_widget(Paragraph::new(hint), layout[4]);
}
