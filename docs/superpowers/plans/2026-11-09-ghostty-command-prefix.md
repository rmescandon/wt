# Ghostty AppleScript Worktree Launch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep Ghostty macOS worktree tabs open and usable at the worktree path.

**Architecture:** Ghostty’s macOS AppleScript surface runs the configured command through `/usr/bin/login -flp $USER /bin/bash --noprofile --norc -c "exec -l <command>"`. Any command that itself starts with `exec` collides with that wrapper. Pass shell text without a `shell:` prefix (Task 1) and without a redundant `exec` (Task 2).

**Tech Stack:** Rust, Cargo tests, Ghostty AppleScript.

**Spec:** Conversation-approved scope: fix Ghostty macOS worktree tab launch; preserve all other terminal behavior.

## Global Constraints

- Keep Ghostty Linux CLI arguments unchanged.
- Keep iTerm2 and other terminal launch paths unchanged.
- Keep Ghostty macOS working-directory assignment unchanged.

## Review Focus

- Ghostty macOS must receive the exact shell command, without a `shell:` prefix; assert generated AppleScript command.
- Plain-shell handoff command must survive Ghostty’s `exec -l` wrapper; assert it runs under `bash -c "exec -l <command>"`.
- Worktree paths with spaces must remain quoted in the command; use existing `/tmp/project folder` regression fixture.
- Ghostty macOS working directory must remain set; retain assertion in regression test.
- Ghostty Linux CLI and iTerm2 contracts must remain unchanged; retain existing assertions.

---

### Task 1: Pass raw shell command to Ghostty AppleScript

**Files:**
- Modify: `src/terminal.rs:246-251`
- Test: `src/lib.rs:306-334`

**Interfaces:**
- Consumes: `terminal_argv(TerminalKind, &str, &Path, &str)`.
- Produces: Ghostty macOS AppleScript assigning `terminal_command` directly to `command of cfg`.

- [x] **Step 1: Strengthen existing regression test**

Assert AppleScript contains:
```rust
assert!(args[1].contains("set command of cfg to \"nvim '/tmp/project folder'\""));
```

- [x] **Step 2: Run focused test and confirm failure**

Run: `cargo test terminal_argv_matches_supported_cli_contracts`
Result: FAIL because generated AppleScript included `shell:`.

- [x] **Step 3: Remove config prefix from AppleScript command**

`ghostty_applescript` passes `terminal_command` directly for `command of cfg`; Ghostty Linux and iTerm2 unchanged.

- [x] **Step 4: Verify behavior and formatting**

Run: `cargo test` → all pass. Run: `cargo fmt --check` → clean.

---

### Task 2: Pass shell handoff without redundant `exec`

**Files:**
- Modify: `src/main.rs:74-85`
- Test: `src/main.rs:96-105`

**Interfaces:**
- Produces: `shell_handoff_command() -> String` returning shell text valid under Ghostty’s `exec -l` wrapper and under `/bin/sh -lc`.

- [x] **Step 1: Write failing test**

```rust
#[test]
fn shell_handoff_command_survives_ghostty_login_exec_wrapper() {
    let script = format!("exec -l {}", super::shell_handoff_command());
    let status = std::process::Command::new("/bin/bash")
        .args(["--noprofile", "--norc", "-c", &script])
        .env("SHELL", "/usr/bin/true")
        .status()
        .expect("run bash wrapper");
    assert!(status.success(), "handoff command failed under exec -l wrapper: {status}");
}
```

- [x] **Step 2: Run test and confirm failure**

Run: `cargo test shell_handoff_command_survives_ghostty_login_exec_wrapper`
Result: FAIL with `bash: line 0: exec: exec: not found` (exit 127).

- [x] **Step 3: Implement shell handoff command**

Add to `src/main.rs`:
```rust
fn shell_handoff_command() -> String {
    "\"${SHELL:-/bin/sh}\"".to_string()
}
```
Use it for the non-terminal-editor branch of `open_worktree`.

- [x] **Step 4: Verify**

Run: `cargo test` → all pass. Run: `cargo fmt --check` → clean.