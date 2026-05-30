# hst

A small shell history picker for Bash and Zsh.

Inspired by [hstr](https://github.com/dvorka-oss/hstr), but intended to be simpler.

## Installation

Build from source:

```sh
cargo build --release
```

Then make sure the binary is available on your `PATH`:

```sh
cp target/release/hst ~/.local/bin/
```

## Shell setup

`hst` can print a shell hook that binds `Ctrl-R` to the history picker.

For Bash, add this to your shell config:

```sh
eval "$(hst --shell bash)"
```

For Zsh, add this to your shell config:

```sh
eval "$(hst --shell zsh)"
```

Restart your shell, or source your shell config, then press `Ctrl-R`.

## Keybindings

| Key | Action |
| --- | --- |
| Type | Filter history |
| Up / Ctrl-P | Move selection up |
| Down / Ctrl-N | Move selection down |
| Enter | Accept selected command |
| Esc / Ctrl-C | Cancel |
| Delete / Ctrl-D | Delete selected history entry |
