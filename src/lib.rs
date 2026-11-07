pub mod config;
pub mod editor;
pub mod git;
pub mod setup;
pub mod terminal;

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::{
        config::Config,
        config::{config_path_for, expand_tilde},
        editor::{editors, shell_quote},
        git::{map_worktree_path, parse_worktrees, removal_prompt},
        terminal::{
            TerminalKind, check_cli_output, supported_terminals, terminal_specs_for_installed,
        },
    };

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock after Unix epoch")
                .as_nanos();
            let path = std::env::temp_dir()
                .join(format!("wt-worktree-test-{}-{unique}", std::process::id()));
            fs::create_dir(&path).expect("create test directory");
            fs::write(path.join("gitconfig"), "[commit]\n\tgpgsign = false\n")
                .expect("write isolated git config");
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn run_git(directory: &Path, args: &[&str]) -> String {
        let global_config = directory
            .ancestors()
            .map(|ancestor| ancestor.join("gitconfig"))
            .find(|path| path.is_file());
        let mut command = Command::new("git");
        command.args(args).current_dir(directory);
        if let Some(global_config) = global_config {
            command.env("GIT_CONFIG_GLOBAL", global_config);
        }
        let output = command.output().expect("run git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    #[test]
    fn fetch_origin_main_updates_tracking_ref_with_custom_fetch_refspec() {
        let temporary = TestDirectory::new();
        let origin = temporary.0.join("origin.git");
        let seed = temporary.0.join("seed");
        let local = temporary.0.join("local");
        let origin_arg = origin.to_string_lossy();
        let local_arg = local.to_string_lossy();

        run_git(
            &temporary.0,
            &["init", "--bare", "--initial-branch=main", &origin_arg],
        );
        fs::create_dir(&seed).expect("create seed repository");
        run_git(&seed, &["init", "--initial-branch=main"]);
        run_git(&seed, &["config", "user.name", "Test"]);
        run_git(&seed, &["config", "user.email", "test@example.invalid"]);
        run_git(&seed, &["config", "commit.gpgsign", "false"]);
        fs::write(seed.join("README"), "initial\n").expect("write initial commit");
        run_git(&seed, &["add", "README"]);
        run_git(&seed, &["commit", "-m", "initial"]);
        run_git(&seed, &["remote", "add", "origin", &origin_arg]);
        run_git(&seed, &["push", "-u", "origin", "main"]);

        run_git(
            &temporary.0,
            &[
                "clone",
                "--single-branch",
                "--branch",
                "main",
                &origin_arg,
                &local_arg,
            ],
        );
        run_git(&local, &["config", "commit.gpgsign", "false"]);
        run_git(
            &local,
            &[
                "config",
                "remote.origin.fetch",
                "+refs/heads/topic:refs/remotes/origin/topic",
            ],
        );

        fs::write(seed.join("README"), "advanced\n").expect("advance remote main");
        run_git(&seed, &["add", "README"]);
        run_git(&seed, &["commit", "-m", "advance main"]);
        run_git(&seed, &["push", "origin", "main"]);

        let remote_main = run_git(&origin, &["rev-parse", "refs/heads/main"]);
        let stale_tracking_ref = run_git(&local, &["rev-parse", "refs/remotes/origin/main"]);
        assert_ne!(
            stale_tracking_ref, remote_main,
            "test setup must leave tracking ref stale"
        );

        crate::git::fetch_origin_main(&local).expect("fetch origin main");

        let updated_tracking_ref = run_git(&local, &["rev-parse", "refs/remotes/origin/main"]);
        assert_eq!(updated_tracking_ref, remote_main);
    }

    #[test]
    fn editor_menu_matches_approved_names_order_and_executables() {
        let actual = editors()
            .into_iter()
            .map(|editor| (editor.name(), editor.command()))
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            vec![
                ("Visual Studio Code", "code"),
                ("VS Code Insiders", "code-insiders"),
                ("Cursor", "cursor"),
                ("Windsurf", "windsurf"),
                ("Sublime Text", "subl"),
                ("Zed", "zed"),
                ("VSCodium", "codium"),
                ("Vim", "vim"),
                ("Neovim", "nvim"),
                ("Emacs", "emacs"),
                ("Helix", "hx"),
                ("IntelliJ IDEA", "idea"),
                ("PyCharm", "pycharm"),
                ("WebStorm", "webstorm"),
                ("PhpStorm", "phpstorm"),
                ("GoLand", "goland"),
                ("RubyMine", "rubymine"),
                ("CLion", "clion"),
                ("Android Studio", "studio"),
            ]
        );
    }

    #[test]
    fn emacs_terminal_command_uses_no_window_mode_and_shell_quotes_path() {
        assert_eq!(
            crate::editor::EditorChoice::Emacs
                .terminal_command(Path::new("/tmp/worktree folder/$branch'")),
            "emacs -nw '/tmp/worktree folder/$branch'\\'''"
        );
    }

    #[test]
    fn config_round_trips_through_toml() {
        let config = Config {
            editor: crate::editor::EditorChoice::Neovim,
            project_roots: vec![PathBuf::from("/home/u/code")],
            worktree_root: PathBuf::from("/home/u/worktrees"),
            terminal: crate::terminal::TerminalChoice {
                kind: TerminalKind::Ghostty,
            },
        };
        let toml = toml::to_string(&config).expect("serialize config");
        let decoded: Config = toml::from_str(&toml).expect("parse config");
        assert_eq!(decoded.editor, config.editor);
        assert_eq!(decoded.project_roots, config.project_roots);
        assert_eq!(decoded.worktree_root, config.worktree_root);
        assert_eq!(decoded.terminal, config.terminal);
    }

    #[test]
    fn shell_quote_preserves_spaces_and_shell_metacharacters() {
        assert_eq!(shell_quote("/tmp/a b/$c'file"), "'/tmp/a b/$c'\\''file'");
        assert_eq!(shell_quote("plain/path"), "'plain/path'");
        assert_eq!(
            shell_quote("/tmp/a; touch /tmp/unwanted"),
            "'/tmp/a; touch /tmp/unwanted'"
        );
    }

    #[test]
    fn config_path_uses_xdg_then_home_fallback() {
        assert_eq!(
            config_path_for(Some(Path::new("/xdg")), Some(Path::new("/home/u"))),
            PathBuf::from("/xdg/wt/config.toml")
        );
        assert_eq!(
            config_path_for(None, Some(Path::new("/home/u"))),
            PathBuf::from("/home/u/.config/wt/config.toml")
        );
    }

    #[test]
    fn expand_tilde_expands_home_prefix_only() {
        assert_eq!(
            expand_tilde("~/Projects", Path::new("/home/u")),
            PathBuf::from("/home/u/Projects")
        );
        assert_eq!(
            expand_tilde("~other/Projects", Path::new("/home/u")),
            PathBuf::from("~other/Projects")
        );
    }

    #[test]
    fn worktree_path_uses_longest_matching_project_root() {
        let roots = vec![
            PathBuf::from("/home/u/code"),
            PathBuf::from("/home/u/code/team"),
        ];
        assert_eq!(
            map_worktree_path(
                Path::new("/home/u/code/team/app"),
                &roots,
                Path::new("/home/u/worktrees"),
                "feature/a",
            ),
            PathBuf::from("/home/u/worktrees/app/feature/a")
        );
    }

    #[test]
    fn worktree_path_preserves_nested_cwd_suffix_below_configured_root() {
        let repository_root = Path::new("/home/u/code/team/app");
        let current = repository_root.join("packages/tool");
        assert_eq!(
            map_worktree_path(
                &current,
                &[PathBuf::from("/home/u/code")],
                Path::new("/home/u/worktrees"),
                "feature/a",
            ),
            PathBuf::from("/home/u/worktrees/team/app/packages/tool/feature/a")
        );
    }

    #[test]
    fn worktree_path_fallback_is_sibling_to_actual_cwd() {
        let repository_root = Path::new("/home/u/random/app");
        let current = repository_root.join("src/nested");
        assert_eq!(
            map_worktree_path(
                &current,
                &[PathBuf::from("/home/u/code")],
                Path::new("/home/u/worktrees"),
                "topic",
            ),
            PathBuf::from("/home/u/random/app/src/topic")
        );
    }

    #[test]
    fn parse_git_worktree_porcelain_records_branch_detached_and_prunable_state() {
        let entries = parse_worktrees(
            "worktree /repo\nHEAD abc\nbranch refs/heads/main\n\nworktree /tmp/wt\nHEAD def\ndetached\n\n",
        );
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].branch.as_deref(), Some("main"));
        assert!(!entries[0].detached);
        assert!(!entries[0].prunable);
        assert_eq!(entries[1].branch, None);
        assert!(entries[1].detached);
        let nul_entries = parse_worktrees(
            "worktree\0/repo with spaces\0HEAD\0abc\0branch\0refs/heads/main\0\0worktree\0/tmp/wt\0detached\0",
        );
        assert_eq!(nul_entries[0].path, PathBuf::from("/repo with spaces"));
        assert_eq!(nul_entries[1].path, PathBuf::from("/tmp/wt"));
        assert!(nul_entries[1].detached);
        let entries = parse_worktrees(
            "worktree /removed\nHEAD abc\nbranch refs/heads/main\nprunable: missing\n\nworktree /existing\nHEAD def\nbranch refs/heads/topic\n\n",
        );
        assert!(entries[0].prunable);
        assert!(!entries[1].prunable);
    }

    #[test]
    fn primary_checkout_uses_first_non_prunable_entry_even_if_detached() {
        let entries = parse_worktrees(
            "worktree /removed\nHEAD 000\nbranch refs/heads/old\nprunable: gone\n\nworktree /primary\nHEAD abc\ndetached\n\nworktree /linked\nHEAD def\nbranch refs/heads/topic\n\n",
        );
        assert_eq!(
            crate::git::primary_checkout_from_entries(&entries),
            Some(PathBuf::from("/primary"))
        );
    }

    #[test]
    fn terminal_argv_matches_supported_cli_contracts() {
        let path = Path::new("/tmp/project folder");
        let command = "nvim '/tmp/project folder'";
        assert_eq!(
            crate::terminal::terminal_argv(TerminalKind::Ghostty, "linux", path, command),
            (
                "ghostty",
                [
                    "+new-tab",
                    "--working-directory",
                    "/tmp/project folder",
                    "-e",
                    "sh",
                    "-lc",
                    "nvim '/tmp/project folder'",
                ]
                .into_iter()
                .map(str::to_owned)
                .collect()
            )
        );
        let (executable, args) =
            crate::terminal::terminal_argv(TerminalKind::Ghostty, "macos", path, command);
        assert_eq!(executable, "osascript");
        assert!(args[1].contains("tell application \"Ghostty\""));
        assert!(args[1].contains("initial working directory of cfg"));
        assert!(args[1].contains("command of cfg"));

        assert_eq!(
            crate::terminal::terminal_argv(TerminalKind::Kitty, "linux", path, command),
            (
                "kitten",
                [
                    "@",
                    "launch",
                    "--type=tab",
                    "--cwd",
                    "/tmp/project folder",
                    "sh",
                    "-lc",
                    "nvim '/tmp/project folder'",
                ]
                .into_iter()
                .map(str::to_owned)
                .collect()
            )
        );
        let (executable, args) =
            crate::terminal::terminal_argv(TerminalKind::Iterm2, "macos", path, command);
        assert_eq!(executable, "osascript");
        assert!(args[1].contains("tell application \"iTerm2\""));
        assert!(args[1].contains("create tab with default profile"));

        assert_eq!(
            crate::terminal::terminal_argv(TerminalKind::WezTerm, "macos", path, command),
            (
                "wezterm",
                [
                    "cli",
                    "spawn",
                    "--cwd",
                    "/tmp/project folder",
                    "--",
                    "sh",
                    "-lc",
                    "nvim '/tmp/project folder'",
                ]
                .into_iter()
                .map(str::to_owned)
                .collect()
            )
        );
        assert_eq!(
            crate::terminal::terminal_argv(TerminalKind::Konsole, "macos", path, command),
            (
                "konsole",
                [
                    "--new-tab",
                    "--workdir",
                    "/tmp/project folder",
                    "-e",
                    "sh",
                    "-lc",
                    "nvim '/tmp/project folder'",
                ]
                .into_iter()
                .map(str::to_owned)
                .collect()
            )
        );
        assert_eq!(
            crate::terminal::terminal_argv(TerminalKind::GnomeTerminal, "linux", path, command),
            (
                "gnome-terminal",
                [
                    "--tab",
                    "--working-directory",
                    "/tmp/project folder",
                    "--",
                    "sh",
                    "-lc",
                    "nvim '/tmp/project folder'",
                ]
                .into_iter()
                .map(str::to_owned)
                .collect()
            )
        );
    }

    #[test]
    fn terminal_detection_requires_supported_app_bundles_and_cli() {
        let kinds = [
            TerminalKind::Ghostty,
            TerminalKind::Konsole,
            TerminalKind::WezTerm,
            TerminalKind::Kitty,
            TerminalKind::Iterm2,
        ];
        let bundles = [
            "Ghostty.app",
            "iTerm.app",
            "kitty.app",
            "WezTerm.app",
            "Konsole.app",
        ]
        .into_iter()
        .map(|name| PathBuf::from("/Applications").join(name))
        .collect::<Vec<_>>();
        let executables = ["osascript", "kitten", "wezterm", "konsole"]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let available = terminal_specs_for_installed("macos", &bundles, &executables);
        assert_eq!(
            available
                .into_iter()
                .map(|terminal| terminal.kind)
                .collect::<Vec<_>>(),
            kinds
        );
        assert!(terminal_specs_for_installed("macos", &bundles, &[]).is_empty());
        let linux =
            terminal_specs_for_installed("linux", &[], &["ghostty", "kitten"].map(str::to_owned));
        assert_eq!(
            linux
                .into_iter()
                .map(|terminal| terminal.kind)
                .collect::<Vec<_>>(),
            [TerminalKind::Ghostty, TerminalKind::Kitty]
        );
        assert!(terminal_specs_for_installed("linux", &[], &[]).is_empty());
    }

    #[test]
    fn terminal_platform_support_is_explicit() {
        assert!(supported_terminals("macos").contains(&TerminalKind::Iterm2));
        assert!(!supported_terminals("linux").contains(&TerminalKind::Iterm2));
        assert!(supported_terminals("linux").contains(&TerminalKind::GnomeTerminal));
    }

    #[test]
    fn cli_terminal_failure_propagates_stderr_and_exit_code() {
        let output = std::process::Command::new("sh")
            .args(["-c", "printf 'no running mux' >&2; exit 7"])
            .output()
            .expect("run test command");
        let error = check_cli_output(output).expect_err("failed CLI should be reported");
        assert!(error.to_string().contains("no running mux"));
        assert!(error.to_string().contains("7"));
    }

    #[test]
    fn cli_terminal_success_is_accepted() {
        let output = std::process::Command::new("true")
            .output()
            .expect("run test command");
        check_cli_output(output).expect("successful CLI dispatch");
    }

    #[test]
    fn removal_prompt_warns_ignored_files_and_names_absolute_path() {
        let prompt = removal_prompt(Path::new("/worktrees/project/topic"));
        assert!(prompt.contains("/worktrees/project/topic"));
        assert!(prompt.contains("ignored files"));
        assert!(prompt.contains("delete"));
    }

    #[test]
    fn applescript_terminal_failure_propagates_stderr() {
        let output = std::process::Command::new("sh")
            .args(["-c", "printf 'automation denied' >&2; exit 1"])
            .output()
            .expect("run test command");
        let error = crate::terminal::check_applescript_output(output)
            .expect_err("failed AppleScript should be reported");
        assert!(error.to_string().contains("automation denied"));
    }

    #[test]
    fn applescript_escaping_protects_paths_with_quotes_and_backslashes() {
        assert_eq!(
            crate::terminal::applescript_string("/tmp/a\\b\"c"),
            "\"/tmp/a\\\\b\\\"c\""
        );
        assert_eq!(
            crate::terminal::applescript_string("line\nnext\r"),
            "\"line\\nnext\\r\""
        );
    }
}
