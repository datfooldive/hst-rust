mod app;
mod cli;
mod history;
mod input;
mod ui;

use std::{
    io::{self, Stderr},
    time::Duration,
};

use anyhow::Result;
use app::App;
use clap::Parser;
use cli::{Cli, render_hook};
use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode},
};
use history::{HistorySource, detect_history_source, load_entries, source_from_path};
use ratatui::{
    Terminal, TerminalOptions, Viewport,
    backend::{Backend, ClearType, CrosstermBackend},
};

#[derive(Debug, Eq, PartialEq)]
enum StartupAction {
    PrintHook(String),
    RunTui(Option<HistorySource>),
}

fn main() -> Result<()> {
    match startup_action(Cli::parse()) {
        StartupAction::PrintHook(hook) => {
            print!("{hook}");
            Ok(())
        }
        StartupAction::RunTui(source) => {
            let source = source.map_or_else(detect_history_source, Ok)?;
            let entries = load_entries(&source)?;
            let mut app = App::new(source, entries);

            run(&mut app)?;
            if let Some(command) = accepted_command_output(&app) {
                println!("{command}");
            }
            Ok(())
        }
    }
}

fn startup_action(cli: Cli) -> StartupAction {
    if let Some(shell) = cli.shell {
        return StartupAction::PrintHook(render_hook(shell, cli.history_file));
    }

    StartupAction::RunTui(cli.history_file.map(|path| source_from_path(path, None)))
}

const PROMPT_HEIGHT: u16 = 10;

fn accepted_command_output(app: &App) -> Option<&str> {
    app.accepted_command.as_deref()
}

fn prompt_terminal_options() -> TerminalOptions {
    TerminalOptions {
        viewport: Viewport::Inline(PROMPT_HEIGHT),
    }
}

fn clear_prompt<B: Backend>(terminal: &mut Terminal<B>) -> std::result::Result<(), B::Error> {
    terminal.clear()?;
    terminal.backend_mut().clear_region(ClearType::CurrentLine)?;
    terminal.backend_mut().flush()
}

fn run(app: &mut App) -> Result<()> {
    {
        let _guard = TerminalGuard::activate()?;
        let backend = CrosstermBackend::new(io::stderr());
        let mut terminal = Terminal::with_options(backend, prompt_terminal_options())?;

        loop {
            terminal.draw(|frame| ui::render(frame, app))?;

            if app.should_quit {
                break;
            }

            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    input::handle_key(app, key)?;
                }
            }
        }

        clear_prompt(&mut terminal)?;
    }

    Ok(())
}

struct TerminalGuard;

impl TerminalGuard {
    fn activate() -> Result<Self> {
        enable_raw_mode()?;
        execute!(io::stderr(), Hide)?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let mut stderr: Stderr = io::stderr();
        let _ = execute!(stderr, Show);
        let _ = disable_raw_mode();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cli::{Cli, Shell},
        history::{HistorySource, ShellKind},
    };
    use ratatui::{Terminal, Viewport, backend::TestBackend, buffer::Buffer};
    use std::path::PathBuf;

    fn app_with(commands: &[&str]) -> App {
        let source = HistorySource {
            shell: ShellKind::Bash,
            path: PathBuf::from("/tmp/history"),
        };
        let entries = commands
            .iter()
            .map(|command| crate::history::HistoryEntry {
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
    fn shell_arg_prints_hook_without_tui() {
        let action = startup_action(Cli {
            shell: Some(Shell::Bash),
            history_file: None,
        });

        match action {
            StartupAction::PrintHook(hook) => assert!(hook.contains("bind -x")),
            StartupAction::RunTui(_) => panic!("expected hook output"),
        }
    }

    #[test]
    fn history_file_arg_launches_tui_with_custom_source() {
        let path = PathBuf::from("/tmp/.zhistory");
        let action = startup_action(Cli {
            shell: None,
            history_file: Some(path.clone()),
        });

        assert_eq!(
            action,
            StartupAction::RunTui(Some(HistorySource {
                shell: ShellKind::Zsh,
                path,
            }))
        );
    }

    #[test]
    fn no_args_launches_tui_with_auto_detected_source() {
        let action = startup_action(Cli {
            shell: None,
            history_file: None,
        });

        assert_eq!(action, StartupAction::RunTui(None));
    }

    #[test]
    fn accepted_command_output_returns_selected_command() {
        let source = HistorySource {
            shell: ShellKind::Bash,
            path: PathBuf::from("/tmp/history"),
        };
        let mut app = App::new(source, Vec::new());
        app.accepted_command = Some("git status".to_string());

        assert_eq!(accepted_command_output(&app), Some("git status"));
    }

    #[test]
    fn prompt_terminal_uses_inline_viewport() {
        assert_eq!(
            prompt_terminal_options().viewport,
            Viewport::Inline(PROMPT_HEIGHT)
        );
    }

    #[test]
    fn clear_prompt_removes_rendered_prompt_lines() {
        let app = app_with(&["git status", "cargo test"]);
        let backend = TestBackend::new(50, PROMPT_HEIGHT);
        let mut terminal = Terminal::with_options(backend, prompt_terminal_options()).unwrap();

        terminal.draw(|frame| ui::render(frame, &app)).unwrap();
        assert!(line(terminal.backend().buffer(), 0).contains(">"));

        clear_prompt(&mut terminal).unwrap();

        let buffer = terminal.backend().buffer();
        for y in 0..buffer.area.height {
            assert!(line(buffer, y).trim().is_empty(), "line {y} was not cleared");
        }
    }
}
