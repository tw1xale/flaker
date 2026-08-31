use crate::actions::{make_cmd, run_silent, run_visible};
use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

/// Adds files to the git staging area.
pub fn git_add(dir: &Path, needs_sudo: bool, path: &str) -> Result<()> {
    let mut cmd = make_cmd("git", dir, needs_sudo);
    cmd.args(["add", path]);

    let prefix = if needs_sudo { "sudo " } else { "" };
    let status = run_visible(
        "STAGING CHANGES",
        &format!("{prefix}git add {path}"),
        &mut cmd,
    )
    .context("Failed to execute git add")?;

    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("git add exited with status: {status}");
    }
}

/// Checks if there are staged changes ready to be committed (read-only, no sudo).
pub fn has_staged_changes(dir: &Path) -> Result<bool> {
    let mut cmd = Command::new("git");
    cmd.args(["diff", "--cached", "--quiet"]).current_dir(dir);

    let output = run_silent(&mut cmd).context("Failed to check staged git diff")?;

    // exit code 0 means no diff, exit code 1 means differences exist
    Ok(!output.status.success())
}

/// Checks if there are any uncommitted changes in the repository (read-only, no sudo).
pub fn has_uncommitted_changes(dir: &Path) -> Result<bool> {
    let mut cmd = Command::new("git");
    cmd.args(["status", "--porcelain"]).current_dir(dir);

    let output = run_silent(&mut cmd).context("Failed to check git status")?;
    let out_str = String::from_utf8_lossy(&output.stdout);
    Ok(!out_str.trim().is_empty())
}

/// Commits staged changes with the provided message.
pub fn git_commit(dir: &Path, needs_sudo: bool, message: &str) -> Result<()> {
    let mut cmd = make_cmd("git", dir, needs_sudo);
    cmd.args(["commit", "-m", message]);

    let prefix = if needs_sudo { "sudo " } else { "" };
    let status = run_visible(
        "RECORDING GIT COMMIT",
        &format!("{prefix}git commit -m \"{message}\""),
        &mut cmd,
    )
    .context("Failed to execute git commit")?;

    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("git commit exited with status: {status}");
    }
}

/// Retrieves the current git branch name, falling back to "master" (read-only, no sudo).
pub fn get_current_branch(dir: &Path) -> String {
    let mut cmd = Command::new("git");
    cmd.args(["branch", "--show-current"]).current_dir(dir);

    if let Ok(out) = run_silent(&mut cmd) {
        let branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if out.status.success() && !branch.is_empty() {
            return branch;
        }
    }

    "master".to_string()
}

/// Pushes commits to the remote repository.
pub fn git_push(dir: &Path, needs_sudo: bool, force: bool) -> Result<()> {
    let branch = get_current_branch(dir);

    let mut args = vec!["push"];
    if force {
        args.push("--force");
    }
    args.extend(["origin", &branch]);

    let mut cmd = make_cmd("git", dir, needs_sudo);
    cmd.args(&args);

    let prefix = if needs_sudo { "sudo " } else { "" };
    let title = if force {
        "FORCE PUSHING TO REMOTE"
    } else {
        "PUSHING TO REMOTE REPOSITORY"
    };
    let desc = format!(
        "{prefix}git push {} origin {branch}",
        if force { "--force" } else { "" }
    );

    let status = run_visible(title, desc.trim(), &mut cmd);

    if matches!(status, Ok(ref s) if s.success()) {
        return Ok(());
    }

    // Fallback without explicit remote/branch
    let mut fallback_args = vec!["push"];
    if force {
        fallback_args.push("--force");
    }

    let mut fallback_cmd = make_cmd("git", dir, needs_sudo);
    fallback_cmd.args(&fallback_args);

    let fallback_desc = format!("{prefix}git push {}", if force { "--force" } else { "" });

    let fallback_status = run_visible(title, fallback_desc.trim(), &mut fallback_cmd)
        .context("Failed to execute git push fallback")?;

    if fallback_status.success() {
        Ok(())
    } else {
        anyhow::bail!("git push failed with status: {fallback_status}");
    }
}

/// Fetches all commits from history for selection, with aligned columns (read-only, no sudo).
pub fn get_recent_commits(dir: &Path) -> Result<Vec<String>> {
    let mut cmd = Command::new("git");
    cmd.args(["log", "--format=%h%x1f%cr%x1f%s"])
        .current_dir(dir);

    let output = run_silent(&mut cmd).context("Failed to execute git log")?;

    if !output.status.success() {
        anyhow::bail!("git log exited with status: {}", output.status);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Vec<(&str, &str, &str)> = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            let mut parts = line.split('\x1f');
            let hash = parts.next().unwrap_or("");
            let date = parts.next().unwrap_or("");
            let msg = parts.next().unwrap_or("");
            (hash, date, msg)
        })
        .collect();

    let max_date_len = parsed
        .iter()
        .map(|(_, date, _)| date.chars().count())
        .max()
        .unwrap_or(14)
        .max(14);

    let commits: Vec<String> = parsed
        .into_iter()
        .map(|(hash, date, msg)| {
            if !hash.is_empty() {
                format!("{hash} │ {date:<width$} │ {msg}", width = max_date_len)
            } else {
                msg.to_string()
            }
        })
        .collect();

    Ok(commits)
}

/// Retrieves working directory and staged diffs (read-only, no sudo).
pub fn get_diff(dir: &Path) -> Result<String> {
    let mut unstaged_cmd = Command::new("git");
    unstaged_cmd.args(["diff"]).current_dir(dir);
    let unstaged = run_silent(&mut unstaged_cmd).context("Failed to execute git diff")?;

    let mut staged_cmd = Command::new("git");
    staged_cmd.args(["diff", "--cached"]).current_dir(dir);
    let staged = run_silent(&mut staged_cmd).context("Failed to execute git diff --cached")?;

    let mut combined = String::new();
    let unstaged_str = String::from_utf8_lossy(&unstaged.stdout);
    let staged_str = String::from_utf8_lossy(&staged.stdout);

    if !unstaged_str.trim().is_empty() {
        combined.push_str(&unstaged_str);
    }
    if !staged_str.trim().is_empty() {
        if !combined.is_empty() {
            combined.push('\n');
        }
        combined.push_str(&staged_str);
    }

    Ok(combined)
}

/// Resets repository to the target commit (hard reset).
pub fn git_reset_hard(dir: &Path, needs_sudo: bool, hash: &str) -> Result<()> {
    let mut cmd = make_cmd("git", dir, needs_sudo);
    cmd.args(["reset", "--hard", hash]);

    let prefix = if needs_sudo { "sudo " } else { "" };
    let status = run_visible(
        "HARD RESETTING REPOSITORY",
        &format!("{prefix}git reset --hard {hash}"),
        &mut cmd,
    )
    .context("Failed to execute git reset --hard")?;

    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("git reset --hard exited with status: {status}");
    }
}

/// Resets repository to the target commit (soft reset).
pub fn git_reset_soft(dir: &Path, needs_sudo: bool, hash: &str) -> Result<()> {
    let mut cmd = make_cmd("git", dir, needs_sudo);
    cmd.args(["reset", "--soft", hash]);

    let prefix = if needs_sudo { "sudo " } else { "" };
    let status = run_visible(
        "TRIMMING COMMIT HISTORY (SOFT RESET)",
        &format!("{prefix}git reset --soft {hash}"),
        &mut cmd,
    )
    .context("Failed to execute git reset --soft")?;

    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("git reset --soft exited with status: {status}");
    }
}

/// Checks out files from a specific commit into the current tree.
pub fn git_checkout_files(dir: &Path, needs_sudo: bool, hash: &str) -> Result<()> {
    let mut cmd = make_cmd("git", dir, needs_sudo);
    cmd.args(["checkout", hash, "--", "."]);

    let prefix = if needs_sudo { "sudo " } else { "" };
    let status = run_visible(
        "REVERTING WORKING TREE FILES",
        &format!("{prefix}git checkout {hash} -- ."),
        &mut cmd,
    )
    .context("Failed to execute git checkout")?;

    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("git checkout exited with status: {status}");
    }
}
