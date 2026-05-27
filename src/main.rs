mod app;
mod cli;
mod history;
mod input;
mod ui;

use std::{
    fs::File,
    io::{self, Read, Stderr, Write},
    os::unix::io::AsRawFd,
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
use ratatui::layout::Position;
use history::{HistorySource, detect_history_source, load_entries, source_from_path};
use ratatui::{
    Terminal, TerminalOptions, Viewport,
    backend::{Backend, ClearType, CrosstermBackend},
    layout::Rect,
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

fn accepted_command_output(app: &App) -> Option<&str> {
    app.accepted_command.as_deref()
}

const VIEWPORT_HEIGHT: u16 = 10;

fn cursor_position() -> Option<(u16, u16)> {
    let mut tty = File::open("/dev/tty").ok()?;
    let fd = tty.as_raw_fd();

    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
    }

    io::stderr().write_all(b"\x1b[6n").ok()?;
    io::stderr().flush().ok()?;

    let mut buf = [0u8; 32];
    std::thread::sleep(Duration::from_millis(50));
    let n = tty.read(&mut buf).unwrap_or(0);

    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        libc::fcntl(fd, libc::F_SETFL, flags & !libc::O_NONBLOCK);
    }

    let resp = std::str::from_utf8(&buf[..n]).ok()?;
    let resp = resp.strip_prefix('\x1b')?.strip_prefix('[')?;
    let resp = resp.strip_suffix('R')?;
    let mut parts = resp.split(';');
    let row: u16 = parts.next()?.parse().ok()?;
    Some((0, row))
}

fn clear_prompt<B: Backend>(terminal: &mut Terminal<B>, start_row: u16) -> std::result::Result<(), B::Error> {
    let backend = terminal.backend_mut();
    for y in 0..VIEWPORT_HEIGHT {
        backend.set_cursor_position(Position::new(0, start_row + y))?;
        backend.clear_region(ClearType::CurrentLine)?;
    }
    backend.set_cursor_position(Position::new(0, start_row))?;
    backend.flush()
}

fn run(app: &mut App) -> Result<()> {
    {
        let _guard = TerminalGuard::activate()?;
        let (col, row) = cursor_position().unwrap_or((0, 0));
        let cols = crossterm::terminal::size().map(|(w, _)| w).unwrap_or(80);
        let area = Rect::new(col, row, cols.saturating_sub(col), VIEWPORT_HEIGHT);
        let mut terminal = Terminal::with_options(
            CrosstermBackend::new(io::stderr()),
            TerminalOptions { viewport: Viewport::Fixed(area) },
        )?;

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

        clear_prompt(&mut terminal, row)?;
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
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};
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
    fn viewport_height_is_10() {
        assert_eq!(VIEWPORT_HEIGHT, 10);
    }

    #[test]
    fn clear_prompt_removes_rendered_prompt_lines() {
        let app = app_with(&["git status", "cargo test"]);
        let backend = TestBackend::new(50, VIEWPORT_HEIGHT);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| ui::render(frame, &app)).unwrap();
        assert!(line(terminal.backend().buffer(), 0).contains(">"));

        clear_prompt(&mut terminal, 0).unwrap();

        let buffer = terminal.backend().buffer();
        for y in 0..buffer.area.height {
            assert!(line(buffer, y).trim().is_empty(), "line {y} was not cleared");
        }
    }
}
