use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use crate::app::App;

fn match_style() -> Style {
    Style::default()
        .fg(Color::White)
        .add_modifier(Modifier::BOLD)
}

fn command_line<'a>(command: &'a str, query: &str) -> Line<'a> {
    if query.is_empty() {
        return Line::from(command);
    }

    let (command_lower, start_map, end_map) = lowercase_with_original_offsets(command);
    let query_lower = query.to_lowercase();
    let mut spans = Vec::new();
    let mut folded_cursor = 0;
    let mut original_cursor = 0;

    while let Some(relative_start) = command_lower[folded_cursor..].find(&query_lower) {
        let folded_start = folded_cursor + relative_start;
        let folded_end = folded_start + query_lower.len();
        let start = start_map[folded_start];
        let end = end_map[folded_end - 1];

        if start > original_cursor {
            spans.push(Span::raw(&command[original_cursor..start]));
        }
        spans.push(Span::styled(&command[start..end], match_style()));
        folded_cursor = folded_end;
        original_cursor = end;
    }

    if original_cursor < command.len() {
        spans.push(Span::raw(&command[original_cursor..]));
    }

    Line::from(spans)
}

fn lowercase_with_original_offsets(value: &str) -> (String, Vec<usize>, Vec<usize>) {
    let mut lowered = String::new();
    let mut start_map = Vec::new();
    let mut end_map = Vec::new();

    for (start, character) in value.char_indices() {
        let end = start + character.len_utf8();
        for lowered_character in character.to_lowercase() {
            let mut buffer = [0; 4];
            let lowered_bytes = lowered_character.encode_utf8(&mut buffer).len();
            lowered.push(lowered_character);
            start_map.extend(std::iter::repeat_n(start, lowered_bytes));
            end_map.extend(std::iter::repeat_n(end, lowered_bytes));
        }
    }

    (lowered, start_map, end_map)
}

const SEARCH_BOX_HEIGHT: u16 = 3;

pub fn render(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    let chunks = Layout::vertical([
        Constraint::Length(SEARCH_BOX_HEIGHT),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area);

    let list_height = chunks[1].height as usize;
    app.set_viewport_height(list_height);

    let visible_indices = app.visible_indices();
    let visible_len = visible_indices.len();

    render_search_box(frame, chunks[0], app);
    render_list(frame, chunks[1], app, &visible_indices, visible_len);
    drop(visible_indices);
    render_bottom_bar(frame, chunks[2], visible_len, app);
}

fn render_search_box(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let cursor = if app.query.is_empty() {
        Span::styled("_", Style::default().fg(Color::DarkGray))
    } else {
        Span::raw("")
    };

    let prompt = Line::from(vec![
        Span::styled("> ", Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
        Span::styled(&app.query, Style::default().add_modifier(Modifier::BOLD)),
        cursor,
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Filter ")
        .title_style(Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD));
    frame.render_widget(Paragraph::new(prompt).block(block), area);
}

fn render_list(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    visible_indices: &[usize],
    visible_len: usize,
) {
    if visible_len == 0 && !app.query.is_empty() {
        let empty_msg = Paragraph::new(Line::from(Span::styled(
            "(no matching commands)",
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(empty_msg, area);
        return;
    }

    let available_height = area.height as usize;

    let end = (app.scroll_offset + available_height).min(visible_len);
    let items: Vec<ListItem> = visible_indices
        .get(app.scroll_offset..end)
        .unwrap_or(&[])
        .iter()
        .map(|index| ListItem::new(command_line(&app.entries[*index].command, &app.query)))
        .collect();

    let list = List::new(items).highlight_style(
        Style::default()
            .fg(Color::White)
            .bg(Color::Rgb(32, 32, 38))
            .add_modifier(Modifier::BOLD),
    );

    let mut list_state = ListState::default();
    if visible_len > 0 && app.selected < visible_len {
        list_state.select(Some(app.selected.saturating_sub(app.scroll_offset)));
    }
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn render_bottom_bar(frame: &mut Frame<'_>, area: Rect, visible_len: usize, app: &App) {
    let chunks = Layout::horizontal([
        Constraint::Percentage(50),
        Constraint::Percentage(50),
    ])
    .split(area);

    let legend = Line::from(Span::styled(
        " Up/Down nav | Enter select | Del delete | Esc cancel ",
        Style::default().fg(Color::DarkGray),
    ));
    frame.render_widget(Paragraph::new(legend).alignment(Alignment::Left), chunks[0]);

    let total = app.entries.len();
    let status = if let Some(message) = &app.status {
        message.clone()
    } else if app.query.is_empty() {
        let shell = match app.source.shell {
            crate::history::ShellKind::Bash => "bash",
            crate::history::ShellKind::Zsh => "zsh",
        };
        format!("{shell} | {total} {}", if total == 1 { "result" } else { "results" })
    } else {
        let pct = if total > 0 { (visible_len * 100) / total } else { 0 };
        let label = if visible_len == 1 { "result" } else { "results" };
        format!("{visible_len} {label}  ({visible_len}/{total} {pct}%)")
    };

    let status_line = Line::from(Span::styled(status, Style::default().fg(Color::DarkGray)));
    frame.render_widget(Paragraph::new(status_line).alignment(Alignment::Right), chunks[1]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::{HistoryEntry, HistorySource, ShellKind};
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};
    use std::path::PathBuf;

    fn app_with(commands: &[&str]) -> App {
        let source = HistorySource {
            shell: ShellKind::Bash,
            path: PathBuf::from("/tmp/history"),
        };
        let entries = commands
            .iter()
            .map(|command| HistoryEntry {
                command: command.to_string(),
                raw_line: command.to_string(),
            })
            .collect();
        App::new(source, entries)
    }

    fn line(buffer: &Buffer, y: u16) -> String {
        (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect::<String>()
    }

    #[test]
    fn command_line_highlights_every_case_insensitive_match() {
        let line = command_line("Git status && git log", "git");

        assert_eq!(line.spans.len(), 4);
        assert_eq!(line.spans[0].content.as_ref(), "Git");
        assert_eq!(line.spans[0].style, match_style());
        assert_eq!(line.spans[1].content.as_ref(), " status && ");
        assert_eq!(line.spans[2].content.as_ref(), "git");
        assert_eq!(line.spans[2].style, match_style());
        assert_eq!(line.spans[3].content.as_ref(), " log");
    }

    #[test]
    fn command_line_handles_unicode_before_match() {
        let line = command_line("İ git", "git");

        assert_eq!(line.spans.len(), 2);
        assert_eq!(line.spans[0].content.as_ref(), "İ ");
        assert_eq!(line.spans[1].content.as_ref(), "git");
        assert_eq!(line.spans[1].style, match_style());
    }

    #[test]
    fn command_line_leaves_empty_query_unhighlighted() {
        let line = command_line("git status", "");

        assert_eq!(line.spans.len(), 1);
        assert_eq!(line.spans[0].content.as_ref(), "git status");
        assert_eq!(line.spans[0].style, Style::default());
    }

    #[test]
    fn render_shows_prompt() {
        let mut app = app_with(&["git status"]);
        app.query = "git".to_string();
        let backend = TestBackend::new(40, 6);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| render(frame, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        assert!(line(buffer, 1).contains("> git"));
    }

    #[test]
    fn render_applies_highlight_style_to_matching_cells() {
        let backend = TestBackend::new(40, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        let line = command_line("git status", "git");
        let item = ListItem::new(line);
        let list = List::new(vec![item]).highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Gray)
                .add_modifier(Modifier::BOLD),
        );
        let mut state = ListState::default();
        state.select(None); // no selection

        terminal
            .draw(|frame| {
                frame.render_stateful_widget(list, frame.area(), &mut state);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let expected = match_style();
        for x in 0..3 {
            let style = buffer[(x, 0)].style();
            assert_eq!(style.fg, expected.fg, "cell ({x},0) fg mismatch");
            assert!(
                style.add_modifier.contains(Modifier::BOLD),
                "cell ({x},0) not bold"
            );
        }
        let style = buffer[(3, 0)].style();
        assert_ne!(style.fg, expected.fg);
    }

    #[test]
    fn render_shows_status_bar() {
        let mut app = app_with(&["git status", "cargo test"]);
        app.query = "git".to_string();
        let backend = TestBackend::new(40, 6);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| render(frame, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        let last_row = buffer.area.height - 1;
        assert!(line(buffer, last_row).contains("1 result"));
    }

    #[test]
    fn renders_compact_prompt_layout() {
        let mut app = app_with(&["git status", "git test", "git fmt", "git clippy"]);
        app.query = "git".to_string();
        let backend = TestBackend::new(50, 8);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| render(frame, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        assert!(line(buffer, 1).contains("> git"));
        assert!(line(buffer, 3).contains("git status"));
    }
}
