use clap::{Parser, ValueEnum};
use std::path::{Path, PathBuf};

#[derive(Debug, Parser, Eq, PartialEq)]
#[command(author, version, about)]
pub struct Cli {
    #[arg(long)]
    pub history_file: Option<PathBuf>,

    #[arg(long, value_enum)]
    pub shell: Option<Shell>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum Shell {
    Bash,
    Zsh,
}

pub fn render_hook(shell: Shell, history_file: Option<PathBuf>) -> String {
    let command = render_command(shell, history_file.as_deref());

    match shell {
        Shell::Bash => format!(
            "__hst_rust_ctrl_r() {{\n  local selected\n  selected=\"$({command})\"\n  if [[ -n \"$selected\" ]]; then\n    READLINE_LINE=\"$selected\"\n    READLINE_POINT=\"${{#READLINE_LINE}}\"\n  fi\n}}\nbind -x '\"\\C-r\": __hst_rust_ctrl_r'\n"
        ),
        Shell::Zsh => format!(
            "__hst_rust_ctrl_r() {{\n  local selected\n  selected=\"$({command})\"\n  if [[ -n \"$selected\" ]]; then\n    LBUFFER=\"$selected\"\n  fi\n  zle reset-prompt\n}}\nzle -N __hst_rust_ctrl_r\nbindkey '^R' __hst_rust_ctrl_r\n"
        ),
    }
}

fn render_command(shell: Shell, history_file: Option<&Path>) -> String {
    let history_file = match (shell, history_file) {
        (_, Some(path)) => quote_path(path),
        (Shell::Bash, None) => String::from("\"${HISTFILE:-$HOME/.bash_history}\""),
        (Shell::Zsh, None) => String::from("\"${HISTFILE:-$HOME/.zsh_history}\""),
    };

    format!("hst --history-file {history_file}")
}

fn quote_path(path: &Path) -> String {
    let value = path.to_string_lossy();

    if value == "~" {
        return String::from("$HOME");
    }

    if let Some(rest) = value.strip_prefix("~/") {
        return format!("\"$HOME/{}\"", escape_double_quoted(rest));
    }

    shell_single_quote(&value)
}

fn escape_double_quoted(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$")
        .replace('`', "\\`")
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use std::path::PathBuf;

    #[test]
    fn no_args_parse_as_tui_mode() {
        let cli = Cli::try_parse_from(["hst"]).unwrap();

        assert_eq!(cli.history_file, None);
        assert_eq!(cli.shell, None);
    }

    #[test]
    fn history_file_parses_path() {
        let cli = Cli::try_parse_from(["hst", "--history-file", "~/.zhistory"]).unwrap();

        assert_eq!(cli.history_file, Some(PathBuf::from("~/.zhistory")));
        assert_eq!(cli.shell, None);
    }

    #[test]
    fn shell_bash_parses_hook_mode() {
        let cli = Cli::try_parse_from(["hst", "--shell", "bash"]).unwrap();

        assert_eq!(cli.shell, Some(Shell::Bash));
    }

    #[test]
    fn shell_zsh_with_history_file_parses_hook_mode_with_path() {
        let cli = Cli::try_parse_from([
            "hst",
            "--shell",
            "zsh",
            "--history-file",
            "~/.zhistory",
        ])
        .unwrap();

        assert_eq!(cli.shell, Some(Shell::Zsh));
        assert_eq!(cli.history_file, Some(PathBuf::from("~/.zhistory")));
    }

    #[test]
    fn bash_hook_inserts_selected_command_into_readline() {
        let hook = render_hook(Shell::Bash, Some(PathBuf::from("~/.bash_history")));

        assert!(hook.contains("selected=\"$(hst --history-file"));
        assert!(hook.contains("READLINE_LINE=\"$selected\""));
        assert!(hook.contains("READLINE_POINT=\"${#READLINE_LINE}\""));
    }

    #[test]
    fn zsh_hook_inserts_selected_command_into_lbuffer() {
        let hook = render_hook(Shell::Zsh, Some(PathBuf::from("~/.zhistory")));

        assert!(hook.contains("selected=\"$(hst --history-file"));
        assert!(hook.contains("LBUFFER=\"$selected\""));
        assert!(hook.contains("zle reset-prompt"));
    }
}
