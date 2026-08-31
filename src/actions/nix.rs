use crate::actions::{make_cmd, run_silent, run_visible};
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use std::process::Command;

/// Reads the current active NixOS system generation number (read-only, no sudo).
pub fn get_system_generation() -> String {
    if let Ok(target) = fs::read_link("/nix/var/nix/profiles/system")
        && let Some(name) = target.file_name().and_then(|n| n.to_str())
        && let Some(num) = name.split('-').nth(1)
        && !num.is_empty()
        && num.chars().all(|c| c.is_ascii_digit())
    {
        return num.to_string();
    }
    "N/A".to_string()
}

/// Rebuilds and activates the NixOS configuration.
pub fn nixos_rebuild_switch(flake_target: &str, dir: &Path) -> Result<()> {
    let mut cmd = Command::new("sudo");
    cmd.args(["nixos-rebuild", "switch", "--flake", flake_target])
        .current_dir(dir);

    let status = run_visible(
        "REBUILDING & ACTIVATING SYSTEM",
        &format!("sudo nixos-rebuild switch --flake {flake_target}"),
        &mut cmd,
    )
    .context("Failed to execute nixos-rebuild switch")?;

    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("nixos-rebuild switch exited with status: {status}");
    }
}

/// Runs a dry-run test build of the NixOS configuration.
pub fn nixos_rebuild_build(flake_target: &str, dir: &Path) -> Result<()> {
    let mut cmd = Command::new("sudo");
    cmd.args(["nixos-rebuild", "build", "--flake", flake_target])
        .current_dir(dir);

    let status = run_visible(
        "TEST BUILDING CONFIGURATION",
        &format!("sudo nixos-rebuild build --flake {flake_target}"),
        &mut cmd,
    )
    .context("Failed to execute nixos-rebuild build")?;

    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("nixos-rebuild build exited with status: {status}");
    }
}

/// Fetches the output of nixos-rebuild list-generations (read-only, no sudo).
pub fn nixos_list_generations(dir: &Path) -> Result<String> {
    let mut cmd = Command::new("nixos-rebuild");
    cmd.args(["list-generations"]).current_dir(dir);

    let output =
        run_silent(&mut cmd).context("Failed to execute nixos-rebuild list-generations")?;

    if !output.status.success() {
        anyhow::bail!(
            "nixos-rebuild list-generations exited with status: {}",
            output.status
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Updates the flake lockfile.
pub fn nix_flake_update(dir: &Path, needs_sudo: bool) -> Result<()> {
    let mut cmd = make_cmd("nix", dir, needs_sudo);
    cmd.args(["flake", "update"]);

    let prefix = if needs_sudo { "sudo " } else { "" };
    let status = run_visible(
        "UPDATING FLAKE LOCKFILE",
        &format!("{prefix}nix flake update"),
        &mut cmd,
    )
    .context("Failed to execute nix flake update")?;

    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("nix flake update exited with status: {status}");
    }
}

/// Executes full store garbage collection and deduplication.
pub fn cleanup_nix_store() -> Result<()> {
    let mut cmd1 = Command::new("sudo");
    cmd1.args(["nix-collect-garbage", "-d"]);
    let s1 = run_visible(
        "COLLECTING OLD GENERATIONS (1/3)",
        "sudo nix-collect-garbage -d",
        &mut cmd1,
    )
    .context("Failed to run nix-collect-garbage")?;

    if !s1.success() {
        anyhow::bail!("nix-collect-garbage exited with status: {s1}");
    }

    let mut cmd2 = Command::new("sudo");
    cmd2.args(["nix-store", "--gc"]);
    let s2 = run_visible(
        "COLLECTING STORE GARBAGE (2/3)",
        "sudo nix-store --gc",
        &mut cmd2,
    )
    .context("Failed to run nix-store --gc")?;

    if !s2.success() {
        anyhow::bail!("nix-store --gc exited with status: {s2}");
    }

    let mut cmd3 = Command::new("sudo");
    cmd3.args(["nix-store", "--optimise"]);
    let s3 = run_visible(
        "OPTIMIZING NIX STORE (3/3)",
        "sudo nix-store --optimise",
        &mut cmd3,
    )
    .context("Failed to run nix-store --optimise")?;

    if !s3.success() {
        anyhow::bail!("nix-store --optimise exited with status: {s3}");
    }

    Ok(())
}
