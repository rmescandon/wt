use std::{
    fmt, io,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Output, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::editor::shell_quote;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TerminalKind {
    Ghostty,
    GnomeTerminal,
    Konsole,
    WezTerm,
    Kitty,
    Iterm2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TerminalChoice {
    pub kind: TerminalKind,
}

impl TerminalKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Ghostty => "Ghostty",
            Self::GnomeTerminal => "GNOME Terminal",
            Self::Konsole => "Konsole",
            Self::WezTerm => "WezTerm",
            Self::Kitty => "Kitty",
            Self::Iterm2 => "iTerm2",
        }
    }

    pub fn executable(self) -> &'static str {
        match self {
            Self::Ghostty => "ghostty",
            Self::GnomeTerminal => "gnome-terminal",
            Self::Konsole => "konsole",
            Self::WezTerm => "wezterm",
            Self::Kitty => "kitten",
            Self::Iterm2 => "osascript",
        }
    }

    fn app_bundle(self) -> Option<&'static str> {
        match self {
            Self::Ghostty => Some("Ghostty.app"),
            Self::Iterm2 => Some("iTerm.app"),
            Self::Kitty => Some("kitty.app"),
            Self::WezTerm => Some("WezTerm.app"),
            Self::Konsole => Some("Konsole.app"),
            Self::GnomeTerminal => None,
        }
    }
}

impl fmt::Display for TerminalKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

pub fn platform_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

pub fn supported_terminals(platform: &str) -> Vec<TerminalKind> {
    let kinds: &[TerminalKind] = match platform {
        "macos" => &[
            TerminalKind::Ghostty,
            TerminalKind::Konsole,
            TerminalKind::WezTerm,
            TerminalKind::Kitty,
            TerminalKind::Iterm2,
        ],
        _ => &[
            TerminalKind::Ghostty,
            TerminalKind::GnomeTerminal,
            TerminalKind::Konsole,
            TerminalKind::WezTerm,
            TerminalKind::Kitty,
        ],
    };
    kinds.to_vec()
}

pub fn terminal_specs(platform: &str) -> Vec<TerminalChoice> {
    let app_bundles = if platform == "macos" {
        app_bundle_locations()
            .into_iter()
            .filter(|path| path.is_dir())
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let mut executables = supported_terminals(platform)
        .into_iter()
        .filter_map(|kind| which::which(kind.executable()).ok())
        .filter_map(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .collect::<Vec<_>>();
    if platform == "macos" && which::which("osascript").is_ok() {
        executables.push("osascript".into());
    }
    terminal_specs_for_installed(platform, &app_bundles, &executables)
}

fn app_bundle_locations() -> Vec<PathBuf> {
    let mut locations = vec![PathBuf::from("/Applications")];
    if let Some(home) = std::env::var_os("HOME") {
        locations.push(Path::new(&home).join("Applications"));
        locations.push(Path::new(&home).join("Applications/Utilities"));
    }
    [
        "Ghostty.app",
        "iTerm.app",
        "iTerm2.app",
        "kitty.app",
        "WezTerm.app",
        "Konsole.app",
    ]
    .into_iter()
    .flat_map(|bundle| {
        locations
            .iter()
            .map(move |directory| directory.join(bundle))
    })
    .collect()
}

pub fn terminal_specs_for_installed(
    platform: &str,
    app_bundles: &[PathBuf],
    executables: &[String],
) -> Vec<TerminalChoice> {
    supported_terminals(platform)
        .into_iter()
        .filter(|kind| {
            if platform != "macos" {
                return executables.iter().any(|name| name == kind.executable());
            }
            let Some(bundle) = kind.app_bundle() else {
                return false;
            };
            let bundle_installed = app_bundles.iter().any(|path| {
                path.file_name().is_some_and(|name| {
                    name == bundle || (*kind == TerminalKind::Iterm2 && name == "iTerm2.app")
                })
            });
            let needs_cli = !matches!(kind, TerminalKind::Ghostty | TerminalKind::Iterm2);
            let executable_available = executables.iter().any(|name| name == kind.executable());
            let cli_available = !needs_cli || executable_available;
            let applescript_available =
                matches!(kind, TerminalKind::Ghostty | TerminalKind::Iterm2)
                    .then(|| executables.iter().any(|name| name == "osascript"))
                    .unwrap_or(true);
            bundle_installed && cli_available && applescript_available
        })
        .map(|kind| TerminalChoice { kind })
        .collect()
}

pub fn terminal_argv(
    kind: TerminalKind,
    platform: &str,
    path: &Path,
    terminal_command: &str,
) -> (&'static str, Vec<String>) {
    let path = path.to_string_lossy().into_owned();
    let shell = || vec!["sh".into(), "-lc".into(), terminal_command.into()];
    match kind {
        TerminalKind::Ghostty if platform == "macos" => (
            "osascript",
            vec!["-e".into(), ghostty_applescript(&path, terminal_command)],
        ),
        TerminalKind::Ghostty => {
            let mut args = vec![
                "+new-tab".into(),
                "--working-directory".into(),
                path,
                "-e".into(),
            ];
            args.extend(shell());
            ("ghostty", args)
        }
        TerminalKind::GnomeTerminal => {
            let mut args = vec![
                "--tab".into(),
                "--working-directory".into(),
                path,
                "--".into(),
            ];
            args.extend(shell());
            ("gnome-terminal", args)
        }
        TerminalKind::Konsole => {
            let mut args = vec!["--new-tab".into(), "--workdir".into(), path, "-e".into()];
            args.extend(shell());
            ("konsole", args)
        }
        TerminalKind::WezTerm => {
            let mut args = vec![
                "cli".into(),
                "spawn".into(),
                "--cwd".into(),
                path,
                "--".into(),
            ];
            args.extend(shell());
            ("wezterm", args)
        }
        TerminalKind::Kitty => {
            let mut args = vec![
                "@".into(),
                "launch".into(),
                "--type=tab".into(),
                "--cwd".into(),
                path,
            ];
            args.extend(shell());
            ("kitten", args)
        }
        TerminalKind::Iterm2 => (
            "osascript",
            vec!["-e".into(), iterm_applescript(&path, terminal_command)],
        ),
    }
}

fn ghostty_applescript(path: &str, terminal_command: &str) -> String {
    format!(
        "tell application \"Ghostty\"\n activate\n set cfg to new surface configuration\n set initial working directory of cfg to {}\n set command of cfg to {}\n set win to front window\n set newTab to new tab in win with configuration cfg\nend tell",
        applescript_string(path),
        applescript_string(terminal_command)
    )
}

fn iterm_applescript(path: &str, terminal_command: &str) -> String {
    let command_line = format!("cd {} && {}", shell_quote(path), terminal_command);
    format!(
        "tell application \"iTerm2\"\n activate\n tell current window\n create tab with default profile\n tell current session\n write text {}\n end tell\n end tell\nend tell",
        applescript_string(&command_line)
    )
}

// Allow quick CLI errors to surface without waiting on terminals that keep shells alive.
const CLI_DISPATCH_GRACE: Duration = Duration::from_millis(300);
const CLI_OUTPUT_DRAIN_GRACE: Duration = Duration::from_millis(100);
const CLI_DISPATCH_POLL_INTERVAL: Duration = Duration::from_millis(10);
const CLI_OUTPUT_CAPTURE_LIMIT: usize = 64 * 1024;

pub fn open_tab(choice: TerminalChoice, path: &Path, terminal_command: &str) -> Result<()> {
    let platform = platform_name();
    if !supported_terminals(platform).contains(&choice.kind) {
        anyhow::bail!("{} is not supported on {platform}", choice.kind.name());
    }
    let (executable, args) = terminal_argv(choice.kind, platform, path, terminal_command);
    let mut command = Command::new(executable);
    command.args(&args);
    if matches!(choice.kind, TerminalKind::Ghostty | TerminalKind::Iterm2) && platform == "macos" {
        let output = command
            .output()
            .context("run AppleScript terminal operation")?;
        check_applescript_output(output)?;
    } else {
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        let mut child = command.spawn().context("run terminal CLI operation")?;
        let stdout = child.stdout.take().context("capture terminal stdout")?;
        let stderr = child.stderr.take().context("capture terminal stderr")?;
        let stdout_capture = Arc::new(Mutex::new(CapturedStream::default()));
        let stderr_capture = Arc::new(Mutex::new(CapturedStream::default()));
        let stdout_reader = spawn_output_reader(stdout, Arc::clone(&stdout_capture));
        let stderr_reader = spawn_output_reader(stderr, Arc::clone(&stderr_capture));
        if let Some(status) = wait_for_cli_dispatch(&mut child, CLI_DISPATCH_GRACE)? {
            let output = collect_cli_output(
                status,
                stdout_reader,
                stderr_reader,
                stdout_capture,
                stderr_capture,
            );
            check_cli_output(output)?;
        }
    }
    Ok(())
}

pub fn check_cli_output(output: Output) -> Result<()> {
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let details = match (stderr.is_empty(), stdout.is_empty()) {
            (false, false) => format!("stderr: {stderr}; stdout: {stdout}"),
            (false, true) => format!("stderr: {stderr}"),
            (true, false) => format!("stdout: {stdout}"),
            (true, true) => "no output".to_string(),
        };
        anyhow::bail!(
            "terminal CLI operation failed ({}): {details}",
            output.status
        );
    }
    Ok(())
}

#[derive(Clone, Default)]
struct CapturedStream {
    bytes: Vec<u8>,
    truncated: bool,
}

fn spawn_output_reader<R>(
    mut reader: R,
    captured: Arc<Mutex<CapturedStream>>,
) -> thread::JoinHandle<()>
where
    R: io::Read + Send + 'static,
{
    thread::spawn(move || read_capped_output(&mut reader, &captured))
}

fn read_capped_output(reader: &mut impl io::Read, captured: &Mutex<CapturedStream>) {
    let mut buffer = [0; 4096];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                let mut captured = captured.lock().expect("terminal output capture poisoned");
                let remaining = CLI_OUTPUT_CAPTURE_LIMIT.saturating_sub(captured.bytes.len());
                let retained = read.min(remaining);
                captured.bytes.extend_from_slice(&buffer[..retained]);
                captured.truncated |= retained < read;
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
}

fn collect_cli_output(
    status: ExitStatus,
    stdout_reader: thread::JoinHandle<()>,
    stderr_reader: thread::JoinHandle<()>,
    stdout_capture: Arc<Mutex<CapturedStream>>,
    stderr_capture: Arc<Mutex<CapturedStream>>,
) -> Output {
    let deadline = Instant::now() + CLI_OUTPUT_DRAIN_GRACE;
    while (!stdout_reader.is_finished() || !stderr_reader.is_finished())
        && Instant::now() < deadline
    {
        thread::sleep(
            CLI_DISPATCH_POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())),
        );
    }
    if stdout_reader.is_finished() {
        let _ = stdout_reader.join();
    }
    if stderr_reader.is_finished() {
        let _ = stderr_reader.join();
    }
    let stdout = stdout_capture
        .lock()
        .expect("stdout capture poisoned")
        .clone();
    let stderr = stderr_capture
        .lock()
        .expect("stderr capture poisoned")
        .clone();
    let mut stdout_bytes = stdout.bytes;
    let mut stderr_bytes = stderr.bytes;
    if stdout.truncated {
        stdout_bytes.extend_from_slice(b"\n[stdout truncated]");
    }
    if stderr.truncated {
        stderr_bytes.extend_from_slice(b"\n[stderr truncated]");
    }
    Output {
        status,
        stdout: stdout_bytes,
        stderr: stderr_bytes,
    }
}

fn wait_for_cli_dispatch(child: &mut Child, grace: Duration) -> Result<Option<ExitStatus>> {
    let deadline = Instant::now() + grace;
    loop {
        if let Some(status) = child.try_wait().context("check terminal CLI operation")? {
            return Ok(Some(status));
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(None);
        }
        thread::sleep(CLI_DISPATCH_POLL_INTERVAL.min(remaining));
    }
}

pub fn check_applescript_output(output: Output) -> Result<()> {
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let details = if stderr.is_empty() { stdout } else { stderr };
        anyhow::bail!("AppleScript terminal operation failed: {details}");
    }
    Ok(())
}

pub fn applescript_string(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
    )
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        process::{Command, Stdio},
        thread,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };

    use super::wait_for_cli_dispatch;

    #[test]
    fn cli_dispatch_reports_failure_that_exits_during_grace() {
        let mut child = Command::new("sh")
            .args(["-c", "exit 7"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn failing terminal command");

        let status = wait_for_cli_dispatch(&mut child, Duration::from_millis(300))
            .expect("check terminal dispatch")
            .expect("child exited during dispatch grace");

        assert!(!status.success());
        assert_eq!(status.code(), Some(7));
    }

    #[test]
    fn cli_dispatch_does_not_wait_for_persistent_command_or_kill_it() {
        let marker = std::env::temp_dir().join(format!(
            "wt-terminal-dispatch-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time after epoch")
                .as_nanos()
        ));
        let mut child = Command::new("sh")
            .args([
                "-c",
                "sleep 0.6; touch \"$1\"",
                "wt-test",
                marker.to_str().expect("temporary marker path is UTF-8"),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn persistent terminal command");
        let started = Instant::now();

        wait_for_cli_dispatch(&mut child, Duration::from_millis(100))
            .expect("still-running command should count as dispatched");
        let elapsed = started.elapsed();
        drop(child);

        assert!(
            elapsed < Duration::from_millis(500),
            "dispatch waited too long: {elapsed:?}"
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        while !marker.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(marker.exists(), "dropping child killed dispatched command");
        fs::remove_file(marker).expect("remove temporary marker");
    }
}
