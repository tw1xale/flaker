use crate::actions::{make_cmd, run_silent, run_visible};
use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::{Command, Output};

fn read_command(dir: &Path) -> Command {
    make_cmd("git", dir, false)
}

fn checked_output(dir: &Path, args: &[&str]) -> Result<Output> {
    let output = run_silent(read_command(dir).args(args))?;
    check_output(&output, args.first().copied().unwrap_or("command"))?;
    Ok(output)
}

fn check_output(output: &Output, operation: &str) -> Result<()> {
    if !output.status.success() {
        bail!(
            "git {operation} failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

fn diff_status(output: &Output) -> Result<bool> {
    match output.status.code() {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        _ => {
            check_output(output, "diff")?;
            bail!("git diff returned an unexpected status")
        }
    }
}

fn mutation(dir: &Path, needs_sudo: bool, args: &[&str], title: &str) -> Result<()> {
    let mut command = make_cmd("git", dir, needs_sudo);
    command.args(args);
    let status = run_visible(title, &format!("git {}", args.join(" ")), &mut command)?;
    if !status.success() {
        bail!("git {} exited with status {status}", args.join(" "));
    }
    Ok(())
}

/// Stage exactly the supplied literal path, including a tracked deletion.
pub fn git_add(dir: &Path, needs_sudo: bool, path: &str) -> Result<()> {
    mutation(
        dir,
        needs_sudo,
        &["--literal-pathspecs", "add", "-A", "--", path],
        "STAGING CHANGES",
    )
}

/// Only diff exit 1 indicates a change; other failures must stop the workflow.
pub fn has_staged_changes(dir: &Path) -> Result<bool> {
    let output = run_silent(read_command(dir).args(["diff", "--cached", "--quiet"]))?;
    diff_status(&output)
}

/// Include staged, unstaged and untracked changes, propagating repository errors.
pub fn has_uncommitted_changes(dir: &Path) -> Result<bool> {
    let output = checked_output(
        dir,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    Ok(!output.stdout.is_empty())
}

pub fn ensure_clean(dir: &Path) -> Result<()> {
    if has_uncommitted_changes(dir)? {
        bail!(
            "Commit or save your local changes before restoring or rewriting history. No files were changed."
        );
    }
    Ok(())
}

/// Protect existing edits to a path while allowing unrelated staged work.
pub fn ensure_path_clean(dir: &Path, path: &str) -> Result<()> {
    let output = checked_output(
        dir,
        &[
            "--literal-pathspecs",
            "status",
            "--porcelain=v1",
            "-z",
            "--",
            path,
        ],
    )?;
    if !output.stdout.is_empty() {
        bail!("Save the existing changes to '{path}' before updating it. No files were changed.");
    }
    Ok(())
}

/// Commit all staged changes only when the user explicitly selected that scope.
pub fn git_commit(dir: &Path, needs_sudo: bool, message: &str) -> Result<()> {
    mutation(
        dir,
        needs_sudo,
        &["commit", "-m", message],
        "RECORDING GIT COMMIT",
    )
}

/// Preserve unrelated staged and unstaged changes during a scoped commit.
pub fn git_commit_paths(
    dir: &Path,
    needs_sudo: bool,
    message: &str,
    paths: &[String],
) -> Result<()> {
    if paths.is_empty() {
        bail!("No paths were selected for the commit");
    }
    let mut args = vec![
        "--literal-pathspecs",
        "commit",
        "--only",
        "-m",
        message,
        "--",
    ];
    args.extend(paths.iter().map(String::as_str));
    mutation(dir, needs_sudo, &args, "COMMITTING SELECTED FILES")
}

pub fn has_remote(dir: &Path) -> Result<bool> {
    let output = checked_output(dir, &["remote"])?;
    Ok(!output.stdout.is_empty())
}

/// Respect tracking, pushRemote and refspec settings instead of assuming origin.
pub fn git_push(dir: &Path, needs_sudo: bool, force: bool) -> Result<()> {
    if !has_remote(dir)? {
        return Ok(());
    }
    let args = if force {
        vec!["push", "--force-with-lease"]
    } else {
        vec!["push"]
    };
    mutation(dir, needs_sudo, &args, "PUSHING TO REMOTE REPOSITORY")
}

/// Bound synchronous history loading to keep filtering responsive.
pub fn get_recent_commits(dir: &Path) -> Result<Vec<String>> {
    let output = checked_output(
        dir,
        &[
            "log",
            "-n",
            "200",
            "--color=never",
            "--format=%h%x1f%cr%x1f%s",
        ],
    )?;
    let stdout = String::from_utf8(output.stdout).context("Commit history is not UTF-8")?;
    Ok(stdout
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\x1f');
            let hash = parts.next()?;
            let date = parts.next()?;
            let message = parts.next()?;
            Some(format!("{hash} │ {date:<14} │ {message}"))
        })
        .collect())
}

pub fn get_diff(dir: &Path) -> Result<String> {
    let unstaged = checked_output(dir, &["diff", "--color=never"])?;
    let staged = checked_output(dir, &["diff", "--cached", "--color=never"])?;
    let mut combined = String::from_utf8(unstaged.stdout).context("Working diff is not UTF-8")?;
    if !combined.is_empty() && !staged.stdout.is_empty() {
        combined.push('\n');
    }
    combined.push_str(&String::from_utf8(staged.stdout).context("Staged diff is not UTF-8")?);
    Ok(combined)
}

pub fn git_reset_hard(dir: &Path, needs_sudo: bool, hash: &str) -> Result<()> {
    mutation(
        dir,
        needs_sudo,
        &["reset", "--hard", hash],
        "HARD RESETTING REPOSITORY",
    )
}

/// Restore the exact tracked snapshot, including removal of newer paths.
pub fn git_checkout_files(dir: &Path, needs_sudo: bool, hash: &str) -> Result<()> {
    ensure_clean(dir)?;
    mutation(
        dir,
        needs_sudo,
        &[
            "restore",
            "--source",
            hash,
            "--staged",
            "--worktree",
            "--",
            ":/",
        ],
        "RESTORING REPOSITORY SNAPSHOT",
    )
}

pub fn get_commit_diff(dir: &Path, hash: &str) -> Result<String> {
    if hash.is_empty() {
        bail!("A commit must be selected");
    }
    let output = checked_output(
        dir,
        &["show", "--stat", "--patch", "--color=never", hash, "--"],
    )?;
    String::from_utf8(output.stdout).context("Commit diff is not UTF-8")
}

/// Preserve exact paths rather than parsing Git's C-quoted display format.
pub fn get_commit_files(dir: &Path, hash: &str) -> Result<Vec<String>> {
    if hash.is_empty() {
        bail!("A commit must be selected");
    }
    let output = checked_output(dir, &["show", "--pretty=", "--name-only", "-z", hash, "--"])?;
    nul_paths(&output.stdout)
}

fn nul_paths(bytes: &[u8]) -> Result<Vec<String>> {
    bytes
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| {
            String::from_utf8(path.to_vec()).context(
                "This repository contains a non-UTF-8 filename; refusing to use a modified path",
            )
        })
        .collect()
}

pub fn head(dir: &Path) -> Result<String> {
    let output = checked_output(dir, &["rev-parse", "--verify", "HEAD"])?;
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

pub fn is_head_commit(dir: &Path, hash: &str) -> Result<bool> {
    let output = checked_output(
        dir,
        &["rev-parse", "--verify", &format!("{hash}^{{commit}}")],
    )?;
    Ok(String::from_utf8(output.stdout)?.trim() == head(dir)?)
}

pub fn has_parent_commit(dir: &Path, hash: &str) -> Result<bool> {
    let output = checked_output(dir, &["rev-list", "--parents", "-n", "1", hash, "--"])?;
    Ok(String::from_utf8(output.stdout)?.split_whitespace().count() > 1)
}

pub fn is_file_identical_to_head(dir: &Path, hash: &str, file: &str) -> Result<bool> {
    let output = run_silent(read_command(dir).args([
        "--literal-pathspecs",
        "diff",
        "--quiet",
        "HEAD",
        hash,
        "--",
        file,
    ]))?;
    Ok(!diff_status(&output)?)
}

pub fn is_commit_identical_to_head(dir: &Path, hash: &str) -> Result<bool> {
    let output = run_silent(read_command(dir).args(["diff", "--quiet", "HEAD", hash, "--"]))?;
    Ok(!diff_status(&output)?)
}

pub fn get_file_diff_from_commit(dir: &Path, hash: &str, file: &str) -> Result<String> {
    let target = if is_head_commit(dir, hash)? && has_parent_commit(dir, hash)? {
        format!("{hash}~1")
    } else {
        hash.to_string()
    };
    let output = checked_output(
        dir,
        &[
            "--literal-pathspecs",
            "diff",
            "--color=never",
            "HEAD",
            &target,
            "--",
            file,
        ],
    )?;
    String::from_utf8(output.stdout).context("File diff is not UTF-8")
}

/// Restore an old file or its absence if the selected commit predates its addition.
pub fn git_restore_file(dir: &Path, needs_sudo: bool, target: &str, file: &str) -> Result<()> {
    ensure_path_clean(dir, file)?;
    mutation(
        dir,
        needs_sudo,
        &[
            "--literal-pathspecs",
            "restore",
            "--source",
            target,
            "--staged",
            "--worktree",
            "--",
            file,
        ],
        "RESTORING FILE FROM COMMIT",
    )
}

pub fn git_restore_patch(
    dir: &Path,
    needs_sudo: bool,
    target: &str,
    file: Option<&str>,
) -> Result<()> {
    ensure_clean(dir)?;
    let mut args = vec![
        "--literal-pathspecs",
        "restore",
        "--source",
        target,
        "--patch",
        "--staged",
        "--worktree",
        "--",
    ];
    args.push(file.unwrap_or("."));
    mutation(dir, needs_sudo, &args, "INTERACTIVE PATCH RESTORATION")
}

/// Paths changed by a restore relative to HEAD and the selected flake directory.
pub fn changed_paths(dir: &Path) -> Result<Vec<String>> {
    let output = checked_output(
        dir,
        &["diff", "HEAD", "--name-only", "-z", "--relative", "--"],
    )?;
    nul_paths(&output.stdout)
}

/// Create the replacement before changing HEAD, including when trees are equal.
/// A compare-and-swap rejects concurrent history updates without losing work.
pub fn trim_history(dir: &Path, needs_sudo: bool, target: &str, message: &str) -> Result<()> {
    ensure_clean(dir)?;
    let old_head = head(dir)?;
    let target_output = checked_output(
        dir,
        &["rev-parse", "--verify", &format!("{target}^{{commit}}")],
    )?;
    let target = String::from_utf8(target_output.stdout)?.trim().to_string();
    if target == old_head {
        bail!("Select a commit before HEAD to trim history");
    }
    let ancestor =
        run_silent(read_command(dir).args(["merge-base", "--is-ancestor", &target, &old_head]))?;
    if !ancestor.status.success() {
        bail!("The selected commit must be an ancestor of HEAD");
    }
    let tree_output = checked_output(dir, &["rev-parse", "HEAD^{tree}"])?;
    let tree = String::from_utf8(tree_output.stdout)?.trim().to_string();
    let mut command = make_cmd("git", dir, needs_sudo);
    command.args(["commit-tree", &tree, "-p", &target, "-m", message]);
    let signing =
        run_silent(read_command(dir).args(["config", "--bool", "--get", "commit.gpgsign"]))?;
    match signing.status.code() {
        Some(0) if signing.stdout == b"true\n" => {
            command.arg("-S");
        }
        Some(0 | 1) => {}
        _ => {
            check_output(&signing, "config")?;
        }
    }
    let output = run_silent(&mut command)?;
    check_output(&output, "commit-tree")?;
    let new_head = String::from_utf8(output.stdout)?.trim().to_string();
    mutation(
        dir,
        needs_sudo,
        &[
            "update-ref",
            "-m",
            "flaker: compact history",
            "HEAD",
            &new_head,
            &old_head,
        ],
        "COMPACTING COMMIT HISTORY",
    )
}
