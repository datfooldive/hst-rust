use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState, Paragraph},
};

use crate::app::App;

fn match_style() -> Style {
    Style::default()
        .fg(Color::Yellow)
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

pub fn render(frame: &mut Frame<'_>, app: &App) {
    let chunks = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(frame.area());

    let prompt = Paragraph::new(format!("> {}", app.query));
    frame.render_widget(prompt, chunks[0]);

    let available_height = chunks[1].height as usize;
    app.set_viewport_height(available_height);

    let visible_indices = app.visible_indices();
    let visible_len = visible_indices.len();
    let end = (app.scroll_offset + available_height).min(visible_len);
    let items: Vec<ListItem> = visible_indices
        .get(app.scroll_offset..end)
        .unwrap_or(&[])
        .iter()
        .map(|index| ListItem::new(command_line(&app.entries[*index].command, &app.query)))
        .collect();

    let list = List::new(items)
        .highlight_symbol("> ")
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    let mut list_state = ListState::default();
    if visible_len > 0 && app.selected < visible_len {
        list_state.select(Some(app.selected.saturating_sub(app.scroll_offset)));
    }
    frame.render_stateful_widget(list, chunks[1], &mut list_state);
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
    fn render_highlights_matches_in_visible_commands() {
        let mut app = app_with(&["git status", "cargo test"]);
        app.query = "git".to_string();
        let backend = TestBackend::new(30, 3);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| render(frame, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        for x in 2..=4 {
            let style = buffer[(x, 1)].style();
            assert_eq!(style.fg, Some(Color::Yellow));
            assert!(style.add_modifier.contains(Modifier::BOLD));
        }
        assert_ne!(buffer[(5, 1)].style().fg, Some(Color::Yellow));
    }

    #[test]
    fn renders_compact_prompt_layout() {
        let mut app = app_with(&["git status", "git test", "git fmt", "git clippy"]);
        app.query = "git".to_string();
        let backend = TestBackend::new(50, 5);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| render(frame, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        assert!(line(buffer, 0).starts_with("> git"));
        assert!(line(buffer, 1).contains("git status"));
        assert!(line(buffer, 4).contains("git clippy"));
        assert!(!line(buffer, 4).contains("Enter accept"));
        assert!(!line(buffer, 4).contains("cancel"));
    }
}
