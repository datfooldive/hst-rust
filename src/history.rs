use anyhow::{Context, Result, anyhow};
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShellKind {
    Bash,
    Zsh,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistorySource {
    pub shell: ShellKind,
    pub path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryEntry {
    pub command: String,
    pub raw_line: String,
}

pub fn detect_history_source() -> Result<HistorySource> {
    if let Some(histfile) = env::var_os("HISTFILE").filter(|value| !value.is_empty()) {
        let path = PathBuf::from(histfile);
        let shell = infer_shell_for_path(&path);
        return Ok(HistorySource { shell, path });
    }

    let home = dirs::home_dir().ok_or_else(|| anyhow!("could not find home directory"))?;
    let shell_env = env::var("SHELL").unwrap_or_default();

    if shell_env.contains("zsh") {
        return Ok(HistorySource {
            shell: ShellKind::Zsh,
            path: home.join(".zsh_history"),
        });
    }

    if shell_env.contains("bash") {
        return Ok(HistorySource {
            shell: ShellKind::Bash,
            path: home.join(".bash_history"),
        });
    }

    let zsh_path = home.join(".zsh_history");
    if zsh_path.exists() {
        return Ok(HistorySource {
            shell: ShellKind::Zsh,
            path: zsh_path,
        });
    }

    let bash_path = home.join(".bash_history");
    if bash_path.exists() {
        return Ok(HistorySource {
            shell: ShellKind::Bash,
            path: bash_path,
        });
    }

    Err(anyhow!(
        "could not detect Bash or Zsh history file; set HISTFILE to a history path"
    ))
}

pub fn source_from_path(path: PathBuf, shell_hint: Option<ShellKind>) -> HistorySource {
    let shell = shell_hint.unwrap_or_else(|| infer_shell_for_path(&path));
    HistorySource { shell, path }
}

pub fn load_entries(source: &HistorySource) -> Result<Vec<HistoryEntry>> {
    let contents = fs::read_to_string(&source.path)
        .with_context(|| format!("failed to read {}", source.path.display()))?;
    Ok(parse_history(source.shell.clone(), &contents))
}

pub fn parse_history(shell: ShellKind, contents: &str) -> Vec<HistoryEntry> {
    contents
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| parse_line(&shell, line))
        .collect()
}

pub fn rewrite_entries(source: &HistorySource, entries: &[HistoryEntry]) -> Result<()> {
    let parent = source
        .path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    let mut temp = NamedTempFile::new_in(parent)
        .with_context(|| format!("failed to create temporary file in {}", parent.display()))?;

    for entry in entries {
        writeln!(temp, "{}", entry.raw_line).with_context(|| {
            format!(
                "failed to write temporary history for {}",
                source.path.display()
            )
        })?;
    }

    temp.flush().with_context(|| {
        format!(
            "failed to flush temporary history for {}",
            source.path.display()
        )
    })?;
    temp.persist(&source.path)
        .map_err(|error| error.error)
        .with_context(|| format!("failed to replace {}", source.path.display()))?;

    Ok(())
}

fn parse_line(shell: &ShellKind, line: &str) -> HistoryEntry {
    let command = match shell {
        ShellKind::Bash => line.to_string(),
        ShellKind::Zsh => parse_zsh_command(line),
    };

    HistoryEntry {
        command,
        raw_line: line.to_string(),
    }
}

fn parse_zsh_command(line: &str) -> String {
    if line.starts_with(": ") {
        if let Some((_, command)) = line.split_once(';') {
            return command.to_string();
        }
    }

    line.to_string()
}

fn infer_shell_for_path(path: &Path) -> ShellKind {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if file_name.contains("zsh") || file_name.contains("zhistory") {
        ShellKind::Zsh
    } else if file_name.contains("bash") {
        ShellKind::Bash
    } else if env::var("SHELL").unwrap_or_default().contains("zsh") {
        ShellKind::Zsh
    } else {
        ShellKind::Bash
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parses_bash_history_lines() {
        let entries = parse_history(ShellKind::Bash, "ls -la\n\ncargo test\n");

        assert_eq!(
            entries,
            vec![
                HistoryEntry {
                    command: "ls -la".to_string(),
                    raw_line: "ls -la".to_string(),
                },
                HistoryEntry {
                    command: "cargo test".to_string(),
                    raw_line: "cargo test".to_string(),
                },
            ]
        );
    }

    #[test]
    fn parses_zsh_extended_history_lines() {
        let entries = parse_history(ShellKind::Zsh, ": 1716650000:0;cargo test\nplain command\n");

        assert_eq!(entries[0].command, "cargo test");
        assert_eq!(entries[0].raw_line, ": 1716650000:0;cargo test");
        assert_eq!(entries[1].command, "plain command");
        assert_eq!(entries[1].raw_line, "plain command");
    }

    #[test]
    fn source_from_zhistory_path_infers_zsh() {
        let source = source_from_path(PathBuf::from("/tmp/.zhistory"), None);

        assert_eq!(source.shell, ShellKind::Zsh);
        assert_eq!(source.path, PathBuf::from("/tmp/.zhistory"));
    }

    #[test]
    fn source_from_bash_history_path_infers_bash() {
        let source = source_from_path(PathBuf::from("/tmp/.bash_history"), None);

        assert_eq!(source.shell, ShellKind::Bash);
        assert_eq!(source.path, PathBuf::from("/tmp/.bash_history"));
    }

    #[test]
    fn explicit_shell_hint_overrides_ambiguous_path() {
        let source = source_from_path(PathBuf::from("/tmp/history"), Some(ShellKind::Zsh));

        assert_eq!(source.shell, ShellKind::Zsh);
        assert_eq!(source.path, PathBuf::from("/tmp/history"));
    }

    #[test]
    fn rewrites_remaining_raw_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history");
        fs::write(&path, "old\n").unwrap();
        let source = HistorySource {
            shell: ShellKind::Bash,
            path: path.clone(),
        };
        let entries = vec![
            HistoryEntry {
                command: "echo one".to_string(),
                raw_line: "echo one".to_string(),
            },
            HistoryEntry {
                command: "cargo test".to_string(),
                raw_line: "cargo test".to_string(),
            },
        ];

        rewrite_entries(&source, &entries).unwrap();

        assert_eq!(fs::read_to_string(path).unwrap(), "echo one\ncargo test\n");
    }

    #[test]
    fn clears_history_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history");
        fs::write(&path, "old\n").unwrap();
        let source = HistorySource {
            shell: ShellKind::Zsh,
            path: path.clone(),
        };

        rewrite_entries(&source, &[]).unwrap();

        assert_eq!(fs::read_to_string(path).unwrap(), "");
    }
}
