use std::cell::{Cell, RefCell};

use crate::history::{HistoryEntry, HistorySource};

pub struct App {
    pub source: HistorySource,
    pub entries: Vec<HistoryEntry>,
    pub query: String,
    pub selected: usize,
    pub should_quit: bool,
    pub status: Option<String>,
    pub accepted_command: Option<String>,
    pub scroll_offset: usize,
    viewport_height: Cell<usize>,
    cached_visible: RefCell<Vec<usize>>,
    cached_query: RefCell<Option<String>>,
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
            scroll_offset: 0,
            viewport_height: Cell::new(10),
            cached_visible: RefCell::new(Vec::new()),
            cached_query: RefCell::new(None),
        }
    }

    pub fn visible_indices(&self) -> std::cell::Ref<'_, Vec<usize>> {
        let mut cached = self.cached_query.borrow_mut();
        if cached.as_deref() != Some(self.query.as_str()) {
            *self.cached_visible.borrow_mut() = self.compute_visible_indices();
            *cached = Some(self.query.clone());
        }
        drop(cached);
        self.cached_visible.borrow()
    }

    fn compute_visible_indices(&self) -> Vec<usize> {
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

    pub fn set_viewport_height(&self, height: usize) {
        if height > 0 {
            self.viewport_height.set(height);
        }
    }

    pub fn selected_entry_index(&self) -> Option<usize> {
        self.visible_indices().get(self.selected).copied()
    }

    pub fn move_up(&mut self) {
        self.status = None;
        self.selected = self.selected.saturating_sub(1);
        self.adjust_scroll_up();
    }

    pub fn move_down(&mut self) {
        self.status = None;
        let visible_len = self.visible_indices().len();
        if visible_len > 0 {
            self.selected = (self.selected + 1).min(visible_len - 1);
        }
        self.adjust_scroll_down(visible_len);
    }

    fn adjust_scroll_up(&mut self) {
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        }
    }

    fn adjust_scroll_down(&mut self, visible_len: usize) {
        let height = self.viewport_height.get();
        if visible_len > 0 && self.selected >= self.scroll_offset + height {
            self.scroll_offset = self.selected - height + 1;
        }
    }

    pub fn push_search_char(&mut self, value: char) {
        self.query.push(value);
        self.selected = 0;
        self.scroll_offset = 0;
        self.status = None;
    }

    pub fn pop_search_char(&mut self) {
        self.query.pop();
        self.selected = 0;
        self.scroll_offset = 0;
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

    fn invalidate_cache(&mut self) {
        *self.cached_query.borrow_mut() = None;
    }

    pub fn delete_selected(&mut self) -> Option<(usize, HistoryEntry)> {
        let entry_index;
        {
            let visible = self.visible_indices();
            entry_index = *visible.get(self.selected)?;
        }
        let removed = self.entries.remove(entry_index);
        self.invalidate_cache();
        self.clamp_selection();
        self.status = Some("deleted selected command".to_string());
        Some((entry_index, removed))
    }

    pub fn restore_entry(&mut self, index: usize, entry: HistoryEntry) {
        let index = index.min(self.entries.len());
        self.entries.insert(index, entry);
        self.invalidate_cache();
        self.clamp_selection();
    }

    pub fn set_error(&mut self, error: impl std::fmt::Display) {
        self.status = Some(format!("error: {error}"));
    }

    fn clamp_selection(&mut self) {
        let visible_len = self.visible_indices().len();
        if visible_len == 0 {
            self.selected = 0;
            self.scroll_offset = 0;
        } else {
            self.selected = self.selected.min(visible_len - 1);
            self.adjust_scroll_up();
            self.adjust_scroll_down(visible_len);
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

        let indices = app.visible_indices();
        assert_eq!(*indices, vec![1]);
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

    #[test]
    fn scroll_offset_advances_when_selection_exceeds_viewport() {
        let mut app = app_with(&["a", "b", "c", "d", "e"]);
        app.set_viewport_height(3);

        for _ in 0..4 {
            app.move_down();
        }

        assert_eq!(app.selected, 4);
        assert_eq!(app.scroll_offset, 2);
    }

    #[test]
    fn scroll_offset_retreats_when_selection_moves_above() {
        let mut app = app_with(&["a", "b", "c", "d", "e"]);
        app.set_viewport_height(3);

        for _ in 0..4 {
            app.move_down();
        }
        for _ in 0..3 {
            app.move_up();
        }

        assert_eq!(app.selected, 1);
        assert_eq!(app.scroll_offset, 1);
    }

    #[test]
    fn scroll_offset_stays_zero_when_selection_visible() {
        let mut app = app_with(&["a", "b", "c", "d", "e"]);
        app.set_viewport_height(3);

        app.move_down();

        assert_eq!(app.selected, 1);
        assert_eq!(app.scroll_offset, 0);
    }

    #[test]
    fn push_search_char_resets_scroll_offset() {
        let mut app = app_with(&["a", "b", "c", "d", "e"]);
        app.set_viewport_height(2);

        for _ in 0..4 {
            app.move_down();
        }
        assert_eq!(app.scroll_offset, 3);

        app.push_search_char('a');

        assert_eq!(app.selected, 0);
        assert_eq!(app.scroll_offset, 0);
    }

    #[test]
    fn pop_search_char_resets_scroll_offset() {
        let mut app = app_with(&["ax", "bx", "cx", "dx", "ex"]);
        app.set_viewport_height(2);
        app.query = "x".to_string();

        for _ in 0..4 {
            app.move_down();
        }
        assert_eq!(app.scroll_offset, 3);

        app.pop_search_char();

        assert_eq!(app.selected, 0);
        assert_eq!(app.scroll_offset, 0);
    }

    #[test]
    fn delete_resets_scroll_offset_when_empty() {
        let mut app = app_with(&["only"]);
        app.set_viewport_height(3);

        app.delete_selected();

        assert_eq!(app.selected, 0);
        assert_eq!(app.scroll_offset, 0);
    }
}
