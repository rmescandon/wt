use std::{env, path::PathBuf};

use anyhow::{Context, Result};
use dialoguer::{Confirm, Input, Select, theme::ColorfulTheme};

use crate::{
    config::{Config, expand_tilde},
    editor::editors,
    terminal::{platform_name, terminal_specs},
};

pub fn run_wizard(config_path: &std::path::Path) -> Result<Config> {
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .context("HOME is not set")?;
    let available_editors = editors();
    let editor_labels = available_editors
        .iter()
        .map(|editor| editor.name())
        .collect::<Vec<_>>();
    let editor_index = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Choose editor")
        .items(&editor_labels)
        .default(0)
        .interact()?;
    let editor = available_editors[editor_index];

    let mut roots = Vec::new();
    loop {
        let value: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt("Project root directory")
            .interact_text()?;
        let root = expand_tilde(&value, &home);
        if !root.is_absolute() {
            anyhow::bail!("project root must be absolute");
        }
        roots.push(root);
        if !Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("Add another project root?")
            .default(false)
            .interact()?
        {
            break;
        }
    }
    let worktree_default = home.join("worktrees").to_string_lossy().to_string();
    let worktree_root: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Worktree root directory")
        .default(worktree_default)
        .interact_text()?;
    let worktree_root = expand_tilde(&worktree_root, &home);
    if !worktree_root.is_absolute() {
        anyhow::bail!("worktree root must be absolute");
    }

    let available_terminals = terminal_specs(platform_name());
    if available_terminals.is_empty() {
        anyhow::bail!(
            "no supported terminal found for {}. Install Ghostty, GNOME Terminal, Konsole, WezTerm, Kitty, or iTerm2.",
            platform_name()
        );
    }
    let terminal_labels = available_terminals
        .iter()
        .map(|terminal| terminal.kind.name())
        .collect::<Vec<_>>();
    let terminal_index = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Choose terminal")
        .items(&terminal_labels)
        .default(0)
        .interact()?;
    let config = Config {
        editor,
        project_roots: roots,
        worktree_root,
        terminal: available_terminals[terminal_index],
    };
    config.save(config_path)?;
    println!("Saved config to {}", config_path.display());
    Ok(config)
}
