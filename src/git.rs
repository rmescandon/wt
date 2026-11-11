use std::{
    fs,
    path::{Component, Path, PathBuf},
    process::{Command, Output},
};

use anyhow::{Context, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeEntry {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub detached: bool,
    pub prunable: bool,
}

pub fn map_worktree_path(
    current: &Path,
    roots: &[PathBuf],
    worktree_root: &Path,
    branch: &str,
) -> PathBuf {
    let matching_root = roots
        .iter()
        .filter_map(|root| {
            let normalized_root = normalize(root);
            let normalized_project = normalize(current);
            normalized_project
                .strip_prefix(&normalized_root)
                .ok()
                .map(|suffix| (normalized_root.components().count(), suffix.to_path_buf()))
        })
        .max_by_key(|(length, _)| *length);
    if let Some((_, suffix)) = matching_root {
        worktree_root.join(suffix).join(branch)
    } else {
        current
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(branch)
    }
}

fn normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}

pub fn parse_worktrees(output: &str) -> Vec<WorktreeEntry> {
    if output.starts_with("worktree\0") {
        return output
            .split("\0\0")
            .filter(|record| !record.is_empty())
            .filter_map(|record| {
                let mut fields = record.split('\0');
                let mut path = None;
                let mut branch = None;
                let mut detached = false;
                let mut prunable = false;
                while let Some(key) = fields.next() {
                    match key {
                        "worktree" => path = fields.next().map(PathBuf::from),
                        "branch" => {
                            branch = fields
                                .next()
                                .and_then(|reference| reference.strip_prefix("refs/heads/"))
                                .map(str::to_owned);
                        }
                        "detached" => detached = true,
                        "prunable" => {
                            prunable = true;
                            fields.next();
                        }
                        _ => {
                            fields.next();
                        }
                    }
                }
                path.map(|path| WorktreeEntry {
                    path,
                    branch,
                    detached,
                    prunable,
                })
            })
            .collect();
    }

    let mut entries = Vec::new();
    let mut current: Option<WorktreeEntry> = None;
    for line in output.lines().chain(std::iter::once("")) {
        if line.is_empty() {
            if let Some(entry) = current.take() {
                entries.push(entry);
            }
            continue;
        }
        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(entry) = current.take() {
                entries.push(entry);
            }
            current = Some(WorktreeEntry {
                path: PathBuf::from(path),
                branch: None,
                detached: false,
                prunable: false,
            });
        } else if let Some(branch) = line.strip_prefix("branch refs/heads/") {
            if let Some(entry) = current.as_mut() {
                entry.branch = Some(branch.to_string());
            }
        } else if line == "detached"
            && let Some(entry) = current.as_mut()
        {
            entry.detached = true;
        } else if line.starts_with("prunable")
            && let Some(entry) = current.as_mut()
        {
            entry.prunable = true;
        }
    }
    entries
}

pub fn git<I, S>(directory: &Path, args: I) -> Result<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    Command::new("git")
        .args(args)
        .current_dir(directory)
        .output()
        .with_context(|| format!("run git in {}", directory.display()))
}

fn checked_git<I, S>(directory: &Path, args: I) -> Result<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let output = git(directory, args)?;
    if !output.status.success() {
        anyhow::bail!(
            "git failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output)
}

pub fn current_directory() -> Result<PathBuf> {
    std::env::current_dir().context("read current directory")
}

pub fn repository_root(directory: &Path) -> Result<PathBuf> {
    let output = checked_git(directory, ["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(
        String::from_utf8_lossy(&output.stdout).trim(),
    ))
}

pub fn primary_checkout(directory: &Path) -> Result<PathBuf> {
    let root = repository_root(directory)?;
    let output = checked_git(&root, ["worktree", "list", "--porcelain"])?;
    let worktrees = parse_worktrees(&String::from_utf8_lossy(&output.stdout));
    primary_checkout_from_entries(&worktrees)
        .context("git worktree list returned no primary checkout")
}

pub fn primary_checkout_from_entries(worktrees: &[WorktreeEntry]) -> Option<PathBuf> {
    worktrees
        .iter()
        .find(|entry| !entry.prunable)
        .map(|entry| entry.path.clone())
}

pub fn is_dirty(directory: &Path) -> Result<bool> {
    let output = checked_git(directory, ["status", "--porcelain"])?;
    Ok(!String::from_utf8_lossy(&output.stdout).trim().is_empty())
}

pub fn branch_for_head(directory: &Path) -> Result<Option<String>> {
    let output = checked_git(directory, ["symbolic-ref", "--quiet", "--short", "HEAD"])?;
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok((!branch.is_empty()).then_some(branch))
}

pub fn validate_branch(branch: &str) -> Result<()> {
    if branch.is_empty() || branch.starts_with('-') || branch.contains(' ') || branch.contains('\\')
    {
        anyhow::bail!("invalid branch name: {branch}");
    }
    let reference = format!("refs/heads/{branch}");
    let output = Command::new("git")
        .args(["check-ref-format", reference.as_str()])
        .output()
        .context("validate branch name")?;
    if !output.status.success() {
        anyhow::bail!("invalid branch name: {branch}");
    }
    Ok(())
}

pub fn fetch_origin_main(project: &Path) -> Result<()> {
    checked_git(
        project,
        [
            "fetch",
            "origin",
            "+refs/heads/main:refs/remotes/origin/main",
        ],
    )?;
    Ok(())
}

pub fn create_worktree(project: &Path, destination: &Path, branch: &str) -> Result<()> {
    validate_branch(branch)?;
    let worktrees = checked_git(project, ["worktree", "list", "--porcelain"])?;
    if let Some(existing) = parse_worktrees(&String::from_utf8_lossy(&worktrees.stdout))
        .into_iter()
        .find(|entry| !entry.prunable && entry.branch.as_deref() == Some(branch))
    {
        anyhow::bail!(
            "branch '{branch}' is already checked out at {}",
            existing.path.display()
        );
    }

    let local_branch = git(
        project,
        [
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
    )?;
    if !local_branch.status.success() && local_branch.status.code() != Some(1) {
        anyhow::bail!(
            "check local branch '{branch}' failed: {}",
            String::from_utf8_lossy(&local_branch.stderr).trim()
        );
    }
    let destination = destination
        .to_str()
        .context("worktree path is not valid Unicode")?;
    if local_branch.status.success() {
        if let Some(parent) = Path::new(destination).parent() {
            fs::create_dir_all(parent)?;
        }
        checked_git(project, ["worktree", "add", destination, branch])?;
        return Ok(());
    }

    match origin_branch_exists(project, branch)? {
        true => {
            fetch_origin_branch(project, branch)?;
            if let Some(parent) = Path::new(destination).parent() {
                fs::create_dir_all(parent)?;
            }
            checked_git(
                project,
                [
                    "worktree",
                    "add",
                    "--track",
                    "-b",
                    branch,
                    destination,
                    &format!("origin/{branch}"),
                ],
            )?;
        }
        false => {
            fetch_origin_main(project)?;
            if let Some(parent) = Path::new(destination).parent() {
                fs::create_dir_all(parent)?;
            }
            checked_git(
                project,
                ["worktree", "add", "-b", branch, destination, "origin/main"],
            )?;
        }
    }
    Ok(())
}

fn origin_branch_exists(project: &Path, branch: &str) -> Result<bool> {
    let reference = format!("refs/heads/{branch}");
    let output = git(
        project,
        ["ls-remote", "--exit-code", "--heads", "origin", &reference],
    )?;
    if output.status.success() {
        return Ok(true);
    }
    if output.status.code() == Some(2) {
        return Ok(false);
    }
    anyhow::bail!(
        "look up branch '{branch}' on origin failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
}

fn fetch_origin_branch(project: &Path, branch: &str) -> Result<()> {
    let reference = format!("+refs/heads/{branch}:refs/remotes/origin/{branch}");
    checked_git(project, ["fetch", "origin", reference.as_str()])
        .with_context(|| format!("fetch origin branch {branch}"))?;
    checked_git(
        project,
        ["config", "--add", "remote.origin.fetch", &reference],
    )
    .with_context(|| format!("configure origin tracking for branch {branch}"))?;
    Ok(())
}

pub fn removal_prompt(absolute_path: &Path) -> String {
    format!(
        "Remove worktree at {} and delete local branch if safely merged? This deletes the folder and all contents, including ignored files. [y/N] ",
        absolute_path.display()
    )
}

pub fn remove_current_worktree(directory: &Path) -> Result<()> {
    let root = repository_root(directory)?;
    let primary = primary_checkout(&root)?;
    if normalize(&root) == normalize(&primary) {
        anyhow::bail!("refusing to remove primary checkout");
    }
    let branch = branch_for_head(&root)?.context("refusing to remove detached worktree")?;
    let absolute = fs::canonicalize(&root)
        .with_context(|| format!("resolve worktree path {}", root.display()))?;
    println!("Worktree: {}", absolute.display());
    println!("Branch: {branch}");
    if is_dirty(&root)? {
        anyhow::bail!("refusing to remove dirty worktree");
    }
    eprint!("{}", removal_prompt(&absolute));
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    if !matches!(answer.trim(), "y" | "Y") {
        println!("Cancelled.");
        return Ok(());
    }
    checked_git(
        &primary,
        [
            "worktree",
            "remove",
            absolute
                .to_str()
                .context("worktree path is not valid Unicode")?,
        ],
    )?;
    let delete = git(&primary, ["branch", "-d", branch.as_str()])?;
    if !delete.status.success() {
        anyhow::bail!(
            "worktree removed, branch retained: {}",
            String::from_utf8_lossy(&delete.stderr).trim()
        );
    }
    Ok(())
}
