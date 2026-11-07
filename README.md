# wt

`wt` is an interactive Git worktree manager for macOS and Linux. It creates branches from `origin/main`, opens worktrees in selected editors, and can safely remove the current linked worktree.

## Requirements

- macOS or Linux
- Rust toolchain (for source installation)
- Git on `PATH`
- At least one supported terminal installed: Ghostty, GNOME Terminal (Linux), Konsole, WezTerm, Kitty, or iTerm2 (macOS)
- Chosen editor installed and available on `PATH`

Editor menu contains exactly 19 options, in this order: Visual Studio Code (`code`), VS Code Insiders (`code-insiders`), Cursor (`cursor`), Windsurf (`windsurf`), Sublime Text (`subl`), Zed (`zed`), VSCodium (`codium`), Vim (`vim`), Neovim (`nvim`), Emacs (`emacs`), Helix (`hx`), IntelliJ IDEA (`idea`), PyCharm (`pycharm`), WebStorm (`webstorm`), PhpStorm (`phpstorm`), GoLand (`goland`), RubyMine (`rubymine`), CLion (`clion`), and Android Studio (`studio`). Nano is not included.

## Install

Install directly from a source checkout:

```sh
cargo install --path .
```

Cargo installs executable as `wt` (ensure Cargo's bin directory is on `PATH`). For a published release, install package with `cargo install wt-worktree`.

## Configure

Run `wt` once to open setup wizard. It asks for one editor from exact 19-option menu, one or more project roots, worktree root, and terminal installed and supported by current OS. Config saves to `$XDG_CONFIG_HOME/wt/config.toml`, or `~/.config/wt/config.toml` when XDG variable is unset.

Example:

```toml
editor = "visual-studio-code"
project_roots = ["/Users/alex/Projects"]
worktree_root = "/Users/alex/worktrees"
terminal = { kind = "ghostty" }
```

Use `~` or `~/...` in paths entered during setup; wizard expands home directory.

## Use

From a Git project checkout:

```sh
wt feature/my-change
```

`wt` asks for confirmation, validates branch name, fetches `origin main` from repository root, then creates new branch and worktree from `origin/main`. Current working directory maps under configured worktree root using longest matching project-root prefix, preserving its path suffix. When cwd falls outside configured roots, destination is sibling to cwd at `../<branch>`.

Choose configured editors to launch. GUI editors open separately; terminal editors launch inside a new terminal tab at worktree path.

Run `wt` with no arguments inside a linked worktree to see its path and branch and confirm removal. Removal refuses primary checkout, detached HEAD, or dirty worktrees. It runs `git worktree remove` on absolute worktree path, then `git branch -d` from primary checkout. Remote branches are never deleted; failed safe branch deletion leaves branch intact.

No-argument first run only performs setup and exits. It never removes a checkout.

## Development

```sh
cargo test
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```
