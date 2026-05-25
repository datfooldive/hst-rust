use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{app::App, history::rewrite_entries};

pub fn handle_key(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Enter => app.accept_selected(),
        KeyCode::Esc => app.cancel(),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => app.cancel(),
        KeyCode::Up => app.move_up(),
        KeyCode::Down => app.move_down(),
        KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => app.move_up(),
        KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => app.move_down(),
        KeyCode::Backspace => app.pop_search_char(),
        KeyCode::Delete => delete_selected(app)?,
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            delete_selected(app)?
        }
        KeyCode::Char(value) => app.push_search_char(value),
        _ => {}
    }

    Ok(())
}

fn delete_selected(app: &mut App) -> Result<()> {
    let Some((index, removed)) = app.delete_selected() else {
        app.status = Some("no command selected".to_string());
        return Ok(());
    };

    if let Err(error) = rewrite_entries(&app.source, &app.entries) {
        app.restore_entry(index, removed);
        app.set_error(error);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::{HistoryEntry, HistorySource, ShellKind};
    use crossterm::event::{KeyCode, KeyModifiers};
    use std::path::PathBuf;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

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

    #[test]
    fn printable_q_updates_query_instead_of_quitting() {
        let mut app = app_with(&["query command"]);

        handle_key(&mut app, key(KeyCode::Char('q'))).unwrap();

        assert_eq!(app.query, "q");
        assert!(!app.should_quit);
    }

    #[test]
    fn enter_accepts_selected_command() {
        let mut app = app_with(&["one", "two"]);
        handle_key(&mut app, key(KeyCode::Down)).unwrap();

        handle_key(&mut app, key(KeyCode::Enter)).unwrap();

        assert_eq!(app.accepted_command, Some("two".to_string()));
        assert!(app.should_quit);
    }

    #[test]
    fn escape_cancels_without_command() {
        let mut app = app_with(&["one"]);

        handle_key(&mut app, key(KeyCode::Esc)).unwrap();

        assert_eq!(app.accepted_command, None);
        assert!(app.should_quit);
    }
}
