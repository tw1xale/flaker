use crate::actions::{make_cmd, run_silent, run_visible};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
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

/// Type of difference in closure packages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffKind {
    Added,
    Removed,
    Updated,
    Rebuilt,
}

/// Represents an individual package change between two Nix closures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosureDiffItem {
    pub package: String,
    pub before_version: Option<String>,
    pub after_version: Option<String>,
    pub size_delta: Option<String>,
    pub kind: DiffKind,
    pub raw: String,
}

/// Parses the output of `nix store diff-closures`.
pub fn parse_diff_closures_output(output: &str) -> Vec<ClosureDiffItem> {
    let mut items = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with("warning:")
            || trimmed.starts_with("error:")
            || trimmed.starts_with("this derivation")
            || trimmed.starts_with("building")
        {
            continue;
        }

        let Some((pkg, rest)) = trimmed.split_once(':') else {
            continue;
        };
        let package = pkg.trim().to_string();
        let rest = rest.trim();

        if let Some((left, right)) = rest.split_once('→') {
            let left = left.trim();
            let right = right.trim();

            let (new_ver, size_delta) = if let Some((ver_part, size_part)) = right.rsplit_once(',')
            {
                let sp = size_part.trim();
                if sp.contains('B') || sp.starts_with('+') || sp.starts_with('-') {
                    (ver_part.trim(), Some(sp.to_string()))
                } else {
                    (right, None)
                }
            } else {
                (right, None)
            };

            let kind = if left == "∅" {
                DiffKind::Added
            } else if new_ver == "∅" {
                DiffKind::Removed
            } else {
                DiffKind::Updated
            };

            let before_version = if left == "∅" {
                None
            } else {
                Some(left.to_string())
            };
            let after_version = if new_ver == "∅" {
                None
            } else {
                Some(new_ver.to_string())
            };

            items.push(ClosureDiffItem {
                package,
                before_version,
                after_version,
                size_delta,
                kind,
                raw: trimmed.to_string(),
            });
        } else {
            items.push(ClosureDiffItem {
                package,
                before_version: None,
                after_version: None,
                size_delta: if !rest.is_empty() {
                    Some(rest.to_string())
                } else {
                    None
                },
                kind: DiffKind::Rebuilt,
                raw: trimmed.to_string(),
            });
        }
    }
    items
}

/// Runs a build of the NixOS configuration, returning the canonical path of the built system closure.
pub fn nixos_rebuild_build_closure(flake_target: &str, dir: &Path) -> Result<PathBuf> {
    let mut cmd = Command::new("sudo");
    cmd.args(["nixos-rebuild", "build", "--flake", flake_target])
        .current_dir(dir);

    let status = run_visible(
        "BUILDING CONFIGURATION FOR PREVIEW",
        &format!("sudo nixos-rebuild build --flake {flake_target}"),
        &mut cmd,
    )
    .context("Failed to execute nixos-rebuild build")?;

    if !status.success() {
        anyhow::bail!("nixos-rebuild build exited with status: {status}");
    }

    let result_link = dir.join("result");
    if result_link.exists() {
        let canonical = fs::canonicalize(&result_link)
            .with_context(|| format!("Failed to canonicalize {}", result_link.display()))?;
        Ok(canonical)
    } else {
        anyhow::bail!("Build succeeded but 'result' symlink was not found in {}", dir.display());
    }
}

/// Runs `nix store diff-closures` between two store paths (read-only, no sudo).
pub fn diff_closures(before: &Path, after: &Path) -> Result<Vec<ClosureDiffItem>> {
    let mut cmd = Command::new("nix");
    cmd.args([
        "store",
        "diff-closures",
        &before.to_string_lossy(),
        &after.to_string_lossy(),
    ]);

    let output = run_silent(&mut cmd).context("Failed to execute nix store diff-closures")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("nix store diff-closures exited with error: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_diff_closures_output(&stdout))
}

/// Computes closure diff against the active system (/run/current-system).
pub fn get_system_closure_diff(new_closure: &Path) -> Result<Vec<ClosureDiffItem>> {
    let current_system = Path::new("/run/current-system");
    if current_system.exists() {
        diff_closures(current_system, new_closure)
    } else {
        Ok(Vec::new())
    }
}

/// Runs a test build of the NixOS configuration (builds without activating).
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

/// Represents an individual input defined in flake.lock or flake.nix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlakeInput {
    pub name: String,
    pub details: String,
}

/// Discovers top-level inputs from `flake.lock` (or fallback `flake.nix`).
pub fn get_flake_inputs(dir: &Path) -> Result<Vec<FlakeInput>> {
    let lock_path = dir.join("flake.lock");
    if lock_path.exists() {
        let content = fs::read_to_string(&lock_path)
            .with_context(|| format!("Failed to read {}", lock_path.display()))?;
        let v: serde_json::Value =
            serde_json::from_str(&content).with_context(|| "Failed to parse flake.lock as JSON")?;

        if let Some(root_inputs) = v
            .get("nodes")
            .and_then(|n| n.get("root"))
            .and_then(|r| r.get("inputs"))
            .and_then(|i| i.as_object())
        {
            let mut inputs = Vec::new();
            for (name, node_target) in root_inputs {
                let mut details = String::new();
                if let Some(target_key) = node_target.as_str()
                    && let Some(node) = v.get("nodes").and_then(|n| n.get(target_key))
                {
                    if let Some(locked) = node.get("locked") {
                        let typ = locked.get("type").and_then(|t| t.as_str()).unwrap_or("");
                        let owner = locked.get("owner").and_then(|o| o.as_str());
                        let repo = locked.get("repo").and_then(|r| r.as_str());
                        let rev = locked.get("rev").and_then(|r| r.as_str());

                        if let (Some(o), Some(r)) = (owner, repo) {
                            if let Some(rev) = rev {
                                let short_rev: String = rev.chars().take(7).collect();
                                details = format!("{o}/{r}@{short_rev}");
                            } else {
                                details = format!("{o}/{r}");
                            }
                        } else if let Some(url) = locked.get("url").and_then(|u| u.as_str()) {
                            details = url.to_string();
                        } else if let Some(path) = locked.get("path").and_then(|p| p.as_str()) {
                            details = format!("path:{path}");
                        } else if !typ.is_empty() {
                            details = typ.to_string();
                        }
                    } else if let Some(original) = node.get("original") {
                        if let (Some(o), Some(r)) = (
                            original.get("owner").and_then(|o| o.as_str()),
                            original.get("repo").and_then(|r| r.as_str()),
                        ) {
                            details = format!("{o}/{r}");
                        } else if let Some(url) = original.get("url").and_then(|u| u.as_str()) {
                            details = url.to_string();
                        } else if let Some(id) = original.get("id").and_then(|i| i.as_str()) {
                            details = id.to_string();
                        } else if let Some(path) = original.get("path").and_then(|p| p.as_str()) {
                            details = format!("path:{path}");
                        }
                    }
                } else if let Some(arr) = node_target.as_array() {
                    let path = arr
                        .iter()
                        .filter_map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join("/");
                    details = format!("follows {path}");
                }

                inputs.push(FlakeInput {
                    name: name.clone(),
                    details,
                });
            }
            inputs.sort_by(|a, b| a.name.cmp(&b.name));
            if !inputs.is_empty() {
                return Ok(inputs);
            }
        }
    }

    // Fallback: parse flake.nix if flake.lock is missing or empty
    let flake_nix_path = dir.join("flake.nix");
    if flake_nix_path.exists() {
        let content = fs::read_to_string(&flake_nix_path).unwrap_or_default();
        let mut names = Vec::new();
        let mut in_inputs_block = false;

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("inputs") && trimmed.contains('{') {
                in_inputs_block = true;
                continue;
            }

            if in_inputs_block {
                if trimmed.starts_with('}') {
                    break;
                }
                if let Some((name_part, _)) = trimmed.split_once('.') {
                    let clean = name_part.trim().trim_matches('"');
                    if !clean.is_empty() && !names.contains(&clean.to_string()) {
                        names.push(clean.to_string());
                    }
                } else if let Some((name_part, _)) = trimmed.split_once('=') {
                    let clean = name_part.trim().trim_matches('"');
                    if !clean.is_empty() && !names.contains(&clean.to_string()) {
                        names.push(clean.to_string());
                    }
                }
            }
        }

        if !names.is_empty() {
            names.sort();
            return Ok(names
                .into_iter()
                .map(|name| FlakeInput {
                    name,
                    details: "from flake.nix".to_string(),
                })
                .collect());
        }
    }

    anyhow::bail!("No inputs found in flake.lock or flake.nix")
}

/// Updates the flake lockfile (all inputs).
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

/// Updates specific inputs of the flake lockfile.
pub fn nix_flake_update_inputs(dir: &Path, needs_sudo: bool, inputs: &[String]) -> Result<()> {
    if inputs.is_empty() {
        return nix_flake_update(dir, needs_sudo);
    }

    let mut cmd = make_cmd("nix", dir, needs_sudo);
    cmd.args(["flake", "update"]);
    for input in inputs {
        cmd.arg(input);
    }

    let prefix = if needs_sudo { "sudo " } else { "" };
    let inputs_str = inputs.join(" ");
    let status = run_visible(
        "UPDATING FLAKE INPUTS",
        &format!("{prefix}nix flake update {inputs_str}"),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_flake_inputs_from_lock() {
        let temp_dir = std::env::temp_dir().join(format!(
            "flaker_test_nix_inputs_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let _ = std::fs::create_dir_all(&temp_dir);

        let lock_content = r#"{
  "nodes": {
    "root": {
      "inputs": {
        "nixpkgs": "nixpkgs_node",
        "home-manager": "hm_node",
        "local-flake": "local_node",
        "subinput": ["nixpkgs"]
      }
    },
    "nixpkgs_node": {
      "locked": {
        "type": "github",
        "owner": "NixOS",
        "repo": "nixpkgs",
        "rev": "abcdef1234567890"
      }
    },
    "hm_node": {
      "locked": {
        "type": "github",
        "owner": "nix-community",
        "repo": "home-manager",
        "rev": "1234567abcdef"
      }
    },
    "local_node": {
      "locked": {
        "type": "path",
        "path": "/etc/dotfiles/flake"
      }
    }
  },
  "version": 7
}"#;
        let lock_path = temp_dir.join("flake.lock");
        std::fs::write(&lock_path, lock_content).unwrap();

        let inputs = get_flake_inputs(&temp_dir).expect("parsing lock should succeed");
        assert_eq!(inputs.len(), 4);

        let nixpkgs = inputs.iter().find(|i| i.name == "nixpkgs").unwrap();
        assert_eq!(nixpkgs.details, "NixOS/nixpkgs@abcdef1");

        let hm = inputs.iter().find(|i| i.name == "home-manager").unwrap();
        assert_eq!(hm.details, "nix-community/home-manager@1234567");

        let local = inputs.iter().find(|i| i.name == "local-flake").unwrap();
        assert_eq!(local.details, "path:/etc/dotfiles/flake");

        let sub = inputs.iter().find(|i| i.name == "subinput").unwrap();
        assert_eq!(sub.details, "follows nixpkgs");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_get_flake_inputs_fallback_to_nix() {
        let temp_dir = std::env::temp_dir().join(format!(
            "flaker_test_nix_fallback_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let _ = std::fs::create_dir_all(&temp_dir);

        let flake_nix = r#"
{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    disko.url = "github:nix-community/disko";
  };
  outputs = { self, nixpkgs, disko }: { };
}
"#;
        std::fs::write(temp_dir.join("flake.nix"), flake_nix).unwrap();

        let inputs = get_flake_inputs(&temp_dir).expect("parsing flake.nix should succeed");
        assert_eq!(inputs.len(), 2);
        let names: Vec<_> = inputs.iter().map(|i| i.name.as_str()).collect();
        assert!(names.contains(&"nixpkgs"));
        assert!(names.contains(&"disko"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_parse_diff_closures_output() {
        let sample = r#"
antigravity-ide: 2.5.5 → ∅, -720.7 MiB
electron: 43.4.1 → 43.5.1
electron-unwrapped: 43.4.1 → 43.5.1, 2.4 MiB
kdeconnect-kde: ∅ → 20.08.2, +6599.7 KiB
flaker: 138.4 KiB
fish: 4.9.2, 4.9.2_fish → 4.9.3, 4.9.3_fish
warning: some warning
"#;
        let items = parse_diff_closures_output(sample);
        assert_eq!(items.len(), 6);

        // 1. antigravity-ide (Removed)
        assert_eq!(items[0].package, "antigravity-ide");
        assert_eq!(items[0].kind, DiffKind::Removed);
        assert_eq!(items[0].before_version.as_deref(), Some("2.5.5"));
        assert_eq!(items[0].after_version, None);
        assert_eq!(items[0].size_delta.as_deref(), Some("-720.7 MiB"));

        // 2. electron (Updated, no size)
        assert_eq!(items[1].package, "electron");
        assert_eq!(items[1].kind, DiffKind::Updated);
        assert_eq!(items[1].before_version.as_deref(), Some("43.4.1"));
        assert_eq!(items[1].after_version.as_deref(), Some("43.5.1"));
        assert_eq!(items[1].size_delta, None);

        // 3. electron-unwrapped (Updated, with size)
        assert_eq!(items[2].package, "electron-unwrapped");
        assert_eq!(items[2].kind, DiffKind::Updated);
        assert_eq!(items[2].size_delta.as_deref(), Some("2.4 MiB"));

        // 4. kdeconnect-kde (Added)
        assert_eq!(items[3].package, "kdeconnect-kde");
        assert_eq!(items[3].kind, DiffKind::Added);
        assert_eq!(items[3].before_version, None);
        assert_eq!(items[3].after_version.as_deref(), Some("20.08.2"));
        assert_eq!(items[3].size_delta.as_deref(), Some("+6599.7 KiB"));

        // 5. flaker (Rebuilt)
        assert_eq!(items[4].package, "flaker");
        assert_eq!(items[4].kind, DiffKind::Rebuilt);
        assert_eq!(items[4].size_delta.as_deref(), Some("138.4 KiB"));

        // 6. fish (Multiple versions in tag)
        assert_eq!(items[5].package, "fish");
        assert_eq!(items[5].kind, DiffKind::Updated);
        assert_eq!(items[5].before_version.as_deref(), Some("4.9.2, 4.9.2_fish"));
        assert_eq!(items[5].after_version.as_deref(), Some("4.9.3, 4.9.3_fish"));
    }

}
