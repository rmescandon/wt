# Existing-Branch Worktrees Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `wt <branch>` reuse matching local or `origin` branches, and create from `origin/main` only when neither exists.

**Architecture:** Keep CLI confirmation and worktree path mapping unchanged. Move branch-source selection into `git::create_worktree`: detect local branches first, query `origin` only when needed, fetch a matching remote branch before creating a tracking local branch, and preserve the existing `origin/main` fallback. Reject branches already checked out in another worktree with that worktree's path.

**Tech Stack:** Rust 2024, `std::process::Command`, existing `anyhow` errors, Git CLI, Cargo tests.

**Spec:** Approved inline design, “Existing-branch worktrees” (conversation, 2026-11-11; no separate design file).

## Global Constraints

- Preserve supported platforms: macOS and Linux.
- Preserve current CLI confirmation and worktree path mapping.
- Local branch takes precedence when same-named local and origin branches both exist.
- Remote lookup/network errors must be reported; do not silently fall back to `origin/main` after lookup failure.
- Keep dependencies unchanged.

## Review Focus

- Same-named local and remote branches: local branch wins; test local reuse when a remote branch of same name also exists.
- Missing local and remote branch: create from fetched `origin/main`; test resulting branch and commit.
- Remote lookup/network failure: report lookup error and do not attempt fallback; test an unreachable `origin` and lookup-specific diagnostic.
- Branch already checked out elsewhere: report existing worktree path; test another linked worktree holding target branch.
- Remote branch with nested name: safely fetch and track exact ref; use a name such as `feature/from-remote` in remote-branch test.

---

### Task 1: Resolve local, origin, and fallback branch sources

**Files:**
- Modify: `src/git.rs`
- Modify: `src/lib.rs` (Git integration tests)
- Modify: `src/main.rs` (help text)
- Modify: `README.md` (usage behavior)

**Interfaces:**
- Preserve public signature `create_worktree(project: &Path, destination: &Path, branch: &str) -> Result<()>`.
- Add private helper `fetch_origin_branch(project: &Path, branch: &str) -> Result<()>` to fetch exact branch into `refs/remotes/origin/<branch>`.
- Reuse `parse_worktrees` to detect active worktrees for the target branch.

- [ ] **Step 1: Add failing Git integration tests**

In `src/lib.rs` tests, add reusable local-origin setup if needed and tests named:
- `create_worktree_reuses_existing_local_branch`: existing local branch is checked out in destination; when an origin branch with same name also exists, local branch commit is used.
- `create_worktree_tracks_matching_origin_branch`: remote-only nested branch is fetched, checked out as a local branch, and has upstream `origin/<branch>`.
- `create_worktree_falls_back_to_origin_main_for_missing_branch`: absent branch is created at current fetched `origin/main`.
- `create_worktree_reports_origin_lookup_failure_without_fallback`: unreachable origin produces lookup-specific error; destination and local branch are not created.
- `create_worktree_reports_branch_checked_out_in_another_worktree`: error includes path of worktree already holding target branch.

Use temporary bare origin/seed/local repositories and existing `TestDirectory` and `run_git` helpers. Keep all tests offline; use local bare repositories except the unreachable-origin case.

- [ ] **Step 2: Run new tests and verify expected failures**

Run: `cargo test create_worktree_ -- --nocapture`
Expected: tests fail because current implementation always runs `git worktree add -b <branch> ... origin/main` and does not resolve existing branches or check worktree occupancy.

- [ ] **Step 3: Implement branch-source resolution in `src/git.rs`**

Keep `create_worktree` signature. Validate branch first. Reject an active matching branch found in `git worktree list --porcelain`, including its path in the error. If `refs/heads/<branch>` exists, use `git worktree add <destination> <branch>` without fetching. Otherwise query `git ls-remote --exit-code --heads origin refs/heads/<branch>`: status 0 means fetch the exact branch and create a local tracking branch with `git worktree add --track -b <branch> <destination> origin/<branch>`; status 2 means branch is absent and retains current `fetch_origin_main` plus `git worktree add -b <branch> <destination> origin/main`; any other status returns a lookup error, never fallback. Fetch exact remote ref with refspec `+refs/heads/<branch>:refs/remotes/origin/<branch>`.

- [ ] **Step 4: Run new tests and verify they pass**

Run: `cargo test create_worktree_ -- --nocapture`
Expected: all five branch-resolution and collision tests pass.

- [ ] **Step 5: Update CLI help and README**

Update `src/main.rs` help text and `README.md` usage section to document local-first resolution, matching `origin/<branch>` tracking, and `origin/main` fallback only when branch is absent on origin. State that lookup failures are reported and an already-checked-out branch reports its worktree path.

- [ ] **Step 6: Run full verification**

Run: `cargo test`
Expected: all unit and integration tests pass.

Run: `cargo fmt --check`
Expected: no formatting changes required.

Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: no warnings or errors.

---
