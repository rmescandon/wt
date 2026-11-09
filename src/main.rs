use std::{
    env,
    io::{self, Write},
};

use anyhow::Result;
use wt_worktree::{
    config::{Config, config_path},
    git::{self, create_worktree, current_directory, map_worktree_path, validate_branch},
    setup, terminal,
};

fn main() {
    if let Err(error) = run() {
        eprintln!("wt: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if help_requested(&args) {
        println!(
            "Usage: wt [--help] [branch]\n\n  wt <branch>  Create a confirmed worktree from origin/main\n  wt           Confirm removal of the current linked worktree"
        );
        return Ok(());
    }
    if !cfg!(target_os = "macos") && !cfg!(target_os = "linux") {
        anyhow::bail!("wt supports macOS and Linux only");
    }
    if args.len() > 1 {
        anyhow::bail!("usage: wt [branch]");
    }
    let path = config_path()?;
    let first_run = !path.exists();
    if first_run {
        setup::run_wizard(&path)?;
    }
    let config = Config::load(&path)?;
    if first_run && args.is_empty() {
        println!("Setup complete. No worktree was removed.");
        return Ok(());
    }
    match args.as_slice() {
        [] => remove_flow(),
        [branch] => create_flow(&config, branch),
        _ => anyhow::bail!("usage: wt [branch]"),
    }
}

fn create_flow(config: &Config, branch: &str) -> Result<()> {
    print!("Create worktree for branch '{branch}'? [Y/n] ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    if !matches!(answer.trim(), "" | "y" | "Y" | "yes" | "YES") {
        println!("Cancelled.");
        return Ok(());
    }
    validate_branch(branch)?;
    let current = current_directory()?;
    let project = git::repository_root(&current)?;
    let destination = map_worktree_path(
        &current,
        &config.project_roots,
        &config.worktree_root,
        branch,
    );
    create_worktree(&project, &destination, branch)?;
    println!("Created worktree at {}", destination.display());
    open_worktree(config, &destination)
}

fn open_worktree(config: &Config, destination: &std::path::Path) -> Result<()> {
    let terminal_editor = config.editor.is_terminal();
    if !terminal_editor {
        config.editor.launch(destination)?;
    }
    let command = if terminal_editor {
        config.editor.terminal_command(destination)
    } else {
        shell_handoff_command()
    };
    terminal::open_tab(config.terminal, destination, &command)
}

fn shell_handoff_command() -> String {
    "\"${SHELL:-/bin/sh}\"".to_string()
}

fn remove_flow() -> Result<()> {
    git::remove_current_worktree(&current_directory()?)
}

fn help_requested(args: &[String]) -> bool {
    args.iter()
        .any(|argument| argument == "--help" || argument == "-h")
}

#[cfg(test)]
mod tests {
    #[test]
    fn help_flag_is_recognized_without_positional_arguments() {
        assert!(super::help_requested(&["--help".into()]));
        assert!(super::help_requested(&["-h".into()]));
        assert!(!super::help_requested(&[]));
        assert!(!super::help_requested(&["feature".into()]));
    }

    #[test]
    fn shell_handoff_command_survives_ghostty_login_exec_wrapper() {
        let script = format!("exec -l {}", super::shell_handoff_command());
        let status = std::process::Command::new("/bin/bash")
            .args(["--noprofile", "--norc", "-c", &script])
            .env("SHELL", "/usr/bin/true")
            .status()
            .expect("run bash wrapper");
        assert!(
            status.success(),
            "handoff command failed under exec -l wrapper: {status}"
        );
    }
}
