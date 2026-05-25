use crate::history::{HistoryEntry, HistorySource};

pub struct App {
    pub source: HistorySource,
    pub entries: Vec<HistoryEntry>,
    pub query: String,
    pub selected: usize,
    pub should_quit: bool,
    pub status: Option<String>,
    pub accepted_command: Option<String>,
}

impl App {
    pub fn new(source: HistorySource, entries: Vec<HistoryEntry>) -> Self {
        Self {
            source,
            entries,
            query: String::new(),
            selected: 0,
            should_quit: false,
            status: None,
            accepted_command: None,
        }
    }

    pub fn visible_indices(&self) -> Vec<usize> {
        let query = self.query.to_lowercase();

        self.entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                if query.is_empty() || entry.command.to_lowercase().contains(&query) {
                    Some(index)
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn selected_entry_index(&self) -> Option<usize> {
        self.visible_indices().get(self.selected).copied()
    }

    pub fn move_up(&mut self) {
        self.status = None;
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn move_down(&mut self) {
        self.status = None;
        let visible_len = self.visible_indices().len();
        if visible_len > 0 {
            self.selected = (self.selected + 1).min(visible_len - 1);
        }
    }

    pub fn push_search_char(&mut self, value: char) {
        self.query.push(value);
        self.selected = 0;
        self.status = None;
    }

    pub fn pop_search_char(&mut self) {
        self.query.pop();
        self.selected = 0;
        self.status = None;
    }

    pub fn accept_selected(&mut self) {
        self.accepted_command = self
            .selected_entry_index()
            .map(|index| self.entries[index].command.clone());
        self.should_quit = true;
    }

    pub fn cancel(&mut self) {
        self.accepted_command = None;
        self.should_quit = true;
    }

    pub fn delete_selected(&mut self) -> Option<(usize, HistoryEntry)> {
        let entry_index = self.selected_entry_index()?;
        let removed = self.entries.remove(entry_index);
        self.clamp_selection();
        self.status = Some("deleted selected command".to_string());
        Some((entry_index, removed))
    }

    pub fn restore_entry(&mut self, index: usize, entry: HistoryEntry) {
        let index = index.min(self.entries.len());
        self.entries.insert(index, entry);
        self.clamp_selection();
    }

    pub fn set_error(&mut self, error: impl std::fmt::Display) {
        self.status = Some(format!("error: {error}"));
    }

    fn clamp_selection(&mut self) {
        let visible_len = self.visible_indices().len();
        if visible_len == 0 {
            self.selected = 0;
        } else {
            self.selected = self.selected.min(visible_len - 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::ShellKind;
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

    #[test]
    fn filters_commands_case_insensitively() {
        let mut app = app_with(&["ls", "Cargo Test", "git status"]);

        app.query = "cargo".to_string();

        assert_eq!(app.visible_indices(), vec![1]);
    }

    #[test]
    fn movement_is_clamped_to_visible_entries() {
        let mut app = app_with(&["one", "two"]);

        app.move_down();
        app.move_down();
        app.move_up();

        assert_eq!(app.selected, 0);
    }

    #[test]
    fn delete_selected_removes_visible_entry() {
        let mut app = app_with(&["one", "two", "three"]);
        app.query = "two".to_string();

        let removed = app.delete_selected().unwrap();

        assert_eq!(removed.0, 1);
        assert_eq!(removed.1.command, "two");
        assert_eq!(
            app.entries
                .iter()
                .map(|entry| entry.command.as_str())
                .collect::<Vec<_>>(),
            vec!["one", "three"]
        );
    }

    #[test]
    fn accept_selected_stores_command_and_quits() {
        let mut app = app_with(&["one", "two"]);
        app.move_down();

        app.accept_selected();

        assert_eq!(app.accepted_command, Some("two".to_string()));
        assert!(app.should_quit);
    }

    #[test]
    fn cancel_quits_without_command() {
        let mut app = app_with(&["one"]);

        app.cancel();

        assert_eq!(app.accepted_command, None);
        assert!(app.should_quit);
    }
}
