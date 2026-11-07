use std::{
    env, fs,
    path::Path,
    process::{Command, Output},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use wt_worktree::terminal::{TerminalChoice, TerminalKind, open_tab};

#[test]
fn help_does_not_read_config_or_start_setup() {
    let output = Command::new(env!("CARGO_BIN_EXE_wt"))
        .arg("--help")
        .env_remove("HOME")
        .env_remove("XDG_CONFIG_HOME")
        .output()
        .expect("run wt --help");

    assert!(
        output.status.success(),
        "wt --help failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Usage: wt"));
    assert!(output.stderr.is_empty());
}

#[test]
fn terminal_dispatch_returns_promptly_without_holding_caller_pipes() {
    let marker = temporary_path("dispatch-marker");
    let started = Instant::now();
    let output = run_dispatch_child("persistent", &marker);
    let elapsed = started.elapsed();

    assert!(
        output.status.success(),
        "dispatcher child failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        elapsed < Duration::from_millis(900),
        "caller capture waited for terminal child: {elapsed:?}"
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    while !marker.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(marker.exists(), "persistent terminal child was killed");
    fs::remove_file(marker).expect("remove terminal child marker");
}

#[test]
fn terminal_dispatch_includes_both_streams_in_early_failure() {
    let output = run_dispatch_child("failure", Path::new("unused"));

    assert!(
        output.status.success(),
        "dispatcher did not report complete failure diagnostics: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn terminal_dispatch_child() {
    let Ok(scenario) = env::var("WT_TEST_TERMINAL_SCENARIO") else {
        return;
    };
    let directory = temporary_path("fake-terminal-dir");
    fs::create_dir(&directory).expect("create fake terminal directory");
    let executable = directory.join("konsole");
    let script = match scenario.as_str() {
        "persistent" => {
            "#!/bin/sh\nprintf 'early stdout\\n'\nprintf 'early stderr\\n' >&2\nsleep 2\ntouch \"$WT_TEST_TERMINAL_MARKER\"\n"
        }
        "failure" => "#!/bin/sh\nprintf 'child stdout\\n'\nprintf 'child stderr\\n' >&2\nexit 7\n",
        _ => panic!("unknown terminal test scenario: {scenario}"),
    };
    fs::write(&executable, script).expect("write fake terminal executable");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
            .expect("make fake terminal executable");
    }
    let mut paths = vec![directory.clone()];
    paths.extend(env::split_paths(&env::var_os("PATH").unwrap_or_default()));
    // This child process runs one test only, so changing PATH cannot affect other tests.
    unsafe { env::set_var("PATH", env::join_paths(paths).expect("join PATH entries")) };

    let result = open_tab(
        TerminalChoice {
            kind: TerminalKind::Konsole,
        },
        Path::new("."),
        "true",
    );
    match scenario.as_str() {
        "persistent" => result.expect("persistent terminal should count as dispatched"),
        "failure" => {
            let error = result
                .expect_err("failed terminal CLI should be reported")
                .to_string();
            assert!(
                error.contains("exit status: 7"),
                "missing exit status: {error}"
            );
            assert!(error.contains("child stdout"), "missing stdout: {error}");
            assert!(error.contains("child stderr"), "missing stderr: {error}");
        }
        _ => unreachable!(),
    }
    fs::remove_dir_all(directory).expect("remove fake terminal directory");
}

fn run_dispatch_child(scenario: &str, marker: &Path) -> Output {
    let mut command = Command::new(env::current_exe().expect("get integration test executable"));
    command
        .args(["--exact", "terminal_dispatch_child", "--nocapture"])
        .env("WT_TEST_TERMINAL_SCENARIO", scenario);
    if scenario == "persistent" {
        command.env("WT_TEST_TERMINAL_MARKER", marker);
    }
    command.output().expect("run production dispatcher child")
}

fn temporary_path(label: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time after epoch")
        .as_nanos();
    env::temp_dir().join(format!("wt-{label}-{}-{unique}", std::process::id()))
}
