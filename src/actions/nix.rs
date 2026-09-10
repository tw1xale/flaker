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

/// Strips ANSI escape sequences from strings safely without dropping valid text.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if let Some(&next) = chars.peek() {
                if next == '[' {
                    chars.next();
                    for c2 in chars.by_ref() {
                        if (0x40..=0x7E).contains(&(c2 as u32)) {
                            break;
                        }
                    }
                } else if next == ']' {
                    chars.next();
                    for c2 in chars.by_ref() {
                        if c2 == '\x07' || c2 == '\x1b' {
                            if c2 == '\x1b' && chars.peek() == Some(&'\\') {
                                chars.next();
                            }
                            break;
                        }
                    }
                } else if next == '(' || next == ')' {
                    chars.next();
                    chars.next();
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Parses the output of `nix store diff-closures`.
pub fn parse_diff_closures_output(output: &str) -> Vec<ClosureDiffItem> {
    let mut items = Vec::new();
    for line in output.lines() {
        let cleaned = strip_ansi(line);
        let trimmed = cleaned.trim();
        if trimmed.is_empty()
            || trimmed.starts_with("warning:")
            || trimmed.starts_with("error:")
            || trimmed.starts_with("trace:")
            || trimmed.starts_with("note:")
            || trimmed.starts_with("evaluating ")
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

        let arrow_split = rest.split_once('→').or_else(|| rest.split_once("->"));
        if let Some((left, right)) = arrow_split {
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

            let kind = if left == "∅" || left.is_empty() {
                DiffKind::Added
            } else if new_ver == "∅" || new_ver.is_empty() {
                DiffKind::Removed
            } else {
                DiffKind::Updated
            };

            let before_version = if left == "∅" || left.is_empty() {
                None
            } else {
                Some(left.to_string())
            };
            let after_version = if new_ver == "∅" || new_ver.is_empty() {
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

/// Safely removes the `./result` symlink in `dir`, using sudo if unprivileged removal fails.
pub fn clean_result_link(dir: &Path, needs_sudo: bool) {
    let result_link = dir.join("result");
    if let Ok(meta) = result_link.symlink_metadata() {
        let removed = if meta.is_dir() {
            fs::remove_dir_all(&result_link).is_ok()
        } else {
            fs::remove_file(&result_link).is_ok()
        };
        if !removed && needs_sudo {
            // Use -n (non-interactive) and silence stdio so sudo never blocks or corrupts the TUI
            let mut cmd = Command::new("sudo");
            cmd.args(["-n", "rm", "-rf", &result_link.to_string_lossy()])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            let _ = cmd.status();
        }
    }
}

/// Runs a build of the NixOS configuration, returning the canonical path of the built system closure.
pub fn nixos_rebuild_build_closure(
    flake_target: &str,
    dir: &Path,
    needs_sudo: bool,
) -> Result<PathBuf> {
    let mut cmd = make_cmd("nixos-rebuild", dir, needs_sudo);
    cmd.args(["build", "--flake", flake_target]);

    let prefix = if needs_sudo { "sudo " } else { "" };
    let status = run_visible(
        "BUILDING CONFIGURATION FOR PREVIEW",
        &format!("{prefix}nixos-rebuild build --flake {flake_target}"),
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
        anyhow::bail!(
            "Build succeeded but 'result' symlink was not found in {}",
            dir.display()
        );
    }
}

/// Runs `nix store diff-closures` between two store paths (read-only, no sudo).
pub fn diff_closures(before: &Path, after: &Path) -> Result<Vec<ClosureDiffItem>> {
    let mut cmd = Command::new("nix");
    cmd.args([
        "--extra-experimental-features",
        "nix-command flakes",
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemClosureDiff {
    pub items: Vec<ClosureDiffItem>,
    pub is_identical_closure: bool,
}

fn format_size_delta(bytes_diff: i64) -> Option<String> {
    if bytes_diff == 0 {
        return None;
    }
    let abs_d = bytes_diff.unsigned_abs();
    let sign = if bytes_diff > 0 { '+' } else { '-' };
    if abs_d < 1024 {
        Some(format!("{sign}{abs_d} B"))
    } else if abs_d < 1024 * 1024 {
        Some(format!("{sign}{:.1} KiB", abs_d as f64 / 1024.0))
    } else {
        Some(format!("{sign}{:.1} MiB", abs_d as f64 / (1024.0 * 1024.0)))
    }
}

pub fn parse_pkg_name_and_version(pkg_str: &str) -> (String, Option<String>) {
    if let Some((name, ver)) = pkg_str.rsplit_once('-')
        && ver.chars().next().is_some_and(|c| c.is_ascii_digit())
    {
        return (name.to_string(), Some(ver.to_string()));
    }
    (pkg_str.to_string(), None)
}

/// Discovers packages rebuilt in system-path (`sw/bin`, `sw/sbin`) that `nix store diff-closures`
/// may have omitted because their version did not change and size delta was under 8 KiB.
pub fn find_rebuilt_system_packages(
    before: &Path,
    after: &Path,
    existing_items: &[ClosureDiffItem],
) -> Vec<ClosureDiffItem> {
    let mut rebuilts = Vec::new();
    let mut seen_packages: std::collections::HashSet<String> = existing_items
        .iter()
        .map(|item| item.package.to_lowercase())
        .collect();

    for sub in &["bin", "sbin"] {
        let dir_before = before.join("sw").join(sub);
        let dir_after = after.join("sw").join(sub);

        let Ok(entries) = fs::read_dir(&dir_after) else {
            continue;
        };

        for entry in entries.flatten() {
            let path_after = entry.path();
            let file_name = entry.file_name();
            let path_before = dir_before.join(&file_name);

            if !path_before.is_symlink() || !path_after.is_symlink() {
                continue;
            }

            let Ok(target_before) = fs::read_link(&path_before) else {
                continue;
            };
            let Ok(target_after) = fs::read_link(&path_after) else {
                continue;
            };

            if target_before != target_after {
                let canon_before = fs::canonicalize(&path_before).unwrap_or(target_before);
                let canon_after = fs::canonicalize(&path_after).unwrap_or(target_after);

                if canon_before == canon_after {
                    continue;
                }

                let store_dir_after = canon_after.components().find_map(|c| {
                    let s = c.as_os_str().to_string_lossy();
                    if s.len() > 33 && s.chars().nth(32) == Some('-') {
                        Some(s.to_string())
                    } else {
                        None
                    }
                });

                let Some(store_dir) = store_dir_after else {
                    continue;
                };

                let pkg_name_ver = &store_dir[33..];
                let (pkg_name, version) = parse_pkg_name_and_version(pkg_name_ver);

                if seen_packages.insert(pkg_name.to_lowercase()) {
                    let size_delta = match (fs::metadata(&canon_before), fs::metadata(&canon_after))
                    {
                        (Ok(m1), Ok(m2)) => {
                            let diff = (m2.len() as i64) - (m1.len() as i64);
                            format_size_delta(diff)
                        }
                        _ => None,
                    };

                    let ver_str = version.as_deref().unwrap_or("rebuilt").to_string();
                    rebuilts.push(ClosureDiffItem {
                        package: pkg_name.clone(),
                        before_version: version.clone(),
                        after_version: version,
                        size_delta,
                        kind: DiffKind::Rebuilt,
                        raw: format!("{pkg_name}: {ver_str}"),
                    });
                }
            }
        }
    }

    rebuilts
}

/// Computes closure diff against the active system (/run/current-system).
pub fn get_system_closure_diff(new_closure: &Path) -> Result<SystemClosureDiff> {
    let current_system = Path::new("/run/current-system");
    if !current_system.exists() {
        return Ok(SystemClosureDiff {
            items: Vec::new(),
            is_identical_closure: false,
        });
    }

    let is_identical_closure = fs::canonicalize(current_system)
        .and_then(|c| fs::canonicalize(new_closure).map(|n| c == n))
        .unwrap_or(false);

    if is_identical_closure {
        return Ok(SystemClosureDiff {
            items: Vec::new(),
            is_identical_closure: true,
        });
    }

    let mut items = diff_closures(current_system, new_closure).unwrap_or_default();
    let rebuilts = find_rebuilt_system_packages(current_system, new_closure, &items);
    items.extend(rebuilts);

    Ok(SystemClosureDiff {
        items,
        is_identical_closure: false,
    })
}

/// Runs a test build of the NixOS configuration (builds without activating).
pub fn nixos_rebuild_build(flake_target: &str, dir: &Path, needs_sudo: bool) -> Result<()> {
    let mut cmd = make_cmd("nixos-rebuild", dir, needs_sudo);
    cmd.args(["build", "--flake", flake_target]);

    let prefix = if needs_sudo { "sudo " } else { "" };
    let status = run_visible(
        "TEST BUILDING CONFIGURATION",
        &format!("{prefix}nixos-rebuild build --flake {flake_target}"),
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

        let mut brace_depth: usize = 0;
        for line in content.lines() {
            let trimmed = line.trim();
            if !in_inputs_block {
                if (trimmed.starts_with("inputs") || trimmed.contains("inputs ="))
                    && trimmed.contains('{')
                {
                    in_inputs_block = true;
                    brace_depth = 1;
                    let opens = trimmed.chars().filter(|&c| c == '{').count();
                    let closes = trimmed.chars().filter(|&c| c == '}').count();
                    if opens > 1 {
                        brace_depth += opens - 1;
                    }
                    brace_depth = brace_depth.saturating_sub(closes);
                    if brace_depth == 0 {
                        break;
                    }
                }
            } else {
                let opens = trimmed.chars().filter(|&c| c == '{').count();
                let closes = trimmed.chars().filter(|&c| c == '}').count();

                if brace_depth == 1 {
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

                brace_depth = brace_depth.saturating_add(opens).saturating_sub(closes);
                if brace_depth == 0 {
                    break;
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
        "path": "/etc/nixos/local"
      }
    }
  },
  "root": "root",
  "version": 7
}"#;

        std::fs::write(temp_dir.join("flake.lock"), lock_content).unwrap();

        let inputs = get_flake_inputs(&temp_dir).unwrap();
        assert_eq!(inputs.len(), 4);

        let names: Vec<String> = inputs.into_iter().map(|i| i.name).collect();
        assert_eq!(
            names,
            vec!["home-manager", "local-flake", "nixpkgs", "subinput"]
        );

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

        let flake_content = r#"
{
  description = "Test Flake";
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    "rust-overlay".url = "github:oxalica/rust-overlay";
    disko = {
      url = "github:nix-community/disko";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };
  outputs = { self, ... }: {};
}
"#;
        std::fs::write(temp_dir.join("flake.nix"), flake_content).unwrap();

        let inputs = get_flake_inputs(&temp_dir).unwrap();
        assert_eq!(inputs.len(), 3);

        let names: Vec<String> = inputs.into_iter().map(|i| i.name).collect();
        assert_eq!(names, vec!["disko", "nixpkgs", "rust-overlay"]);

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
        assert_eq!(
            items[5].before_version.as_deref(),
            Some("4.9.2, 4.9.2_fish")
        );
        assert_eq!(items[5].after_version.as_deref(), Some("4.9.3, 4.9.3_fish"));
    }

    #[test]
    fn test_parse_diff_closures_output_ansi_and_traces() {
        let sample = "\x1b[32mfirefox\x1b[0m: 134.0 → 135.0, +12.4 MiB\ntrace: something evaluating\nnote: nix note\n";
        let items = parse_diff_closures_output(sample);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].package, "firefox");
        assert_eq!(items[0].before_version.as_deref(), Some("134.0"));
        assert_eq!(items[0].after_version.as_deref(), Some("135.0"));
        assert_eq!(items[0].size_delta.as_deref(), Some("+12.4 MiB"));
    }

    #[test]
    fn test_parse_diff_closures_output_ascii_arrow() {
        let sample = "bash: 5.2p26 -> 5.2p32, +4.2 KiB\n";
        let items = parse_diff_closures_output(sample);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].package, "bash");
        assert_eq!(items[0].before_version.as_deref(), Some("5.2p26"));
        assert_eq!(items[0].after_version.as_deref(), Some("5.2p32"));
        assert_eq!(items[0].size_delta.as_deref(), Some("+4.2 KiB"));
        assert_eq!(items[0].kind, DiffKind::Updated);

        let sample_added = "zsh: -> 5.9, +1.2 MiB\n";
        let items_added = parse_diff_closures_output(sample_added);
        assert_eq!(items_added.len(), 1);
        assert_eq!(items_added[0].package, "zsh");
        assert_eq!(items_added[0].kind, DiffKind::Added);
        assert_eq!(items_added[0].before_version, None);
        assert_eq!(items_added[0].after_version.as_deref(), Some("5.9"));
    }

    #[test]
    fn test_strip_ansi_comprehensive() {
        assert_eq!(strip_ansi("\x1b[32mhello\x1b[0m world"), "hello world");
        assert_eq!(
            strip_ansi("\x1b]0;ignored title\x07actual text"),
            "actual text"
        );
        assert_eq!(
            strip_ansi("\x1b]0;ignored title\x1b\\actual text"),
            "actual text"
        );
        assert_eq!(strip_ansi("plain text"), "plain text");
        assert_eq!(strip_ansi(""), "");
    }

    #[test]
    fn test_parse_pkg_name_and_version() {
        assert_eq!(
            parse_pkg_name_and_version("flaker-0.2.0"),
            ("flaker".to_string(), Some("0.2.0".to_string()))
        );
        assert_eq!(
            parse_pkg_name_and_version("libunistring-1.4.2"),
            ("libunistring".to_string(), Some("1.4.2".to_string()))
        );
        assert_eq!(
            parse_pkg_name_and_version("system-path"),
            ("system-path".to_string(), None)
        );
    }

    #[test]
    fn test_format_size_delta() {
        assert_eq!(format_size_delta(0), None);
        assert_eq!(format_size_delta(200), Some("+200 B".to_string()));
        assert_eq!(format_size_delta(-907), Some("-907 B".to_string()));
        assert_eq!(format_size_delta(15360), Some("+15.0 KiB".to_string()));
    }

    #[test]
    fn test_find_rebuilt_system_packages_mock() {
        let base = std::env::temp_dir().join(format!("flaker_test_rebuilt_{}", std::process::id()));
        let _ = fs::create_dir_all(&base);
        let sys1 = base.join("sys1");
        let sys2 = base.join("sys2");

        let store_flaker1 =
            base.join("store/11111111111111111111111111111111-flaker-0.2.0/bin/flaker");
        let store_flaker2 =
            base.join("store/22222222222222222222222222222222-flaker-0.2.0/bin/flaker");
        let _ = fs::create_dir_all(store_flaker1.parent().unwrap());
        let _ = fs::create_dir_all(store_flaker2.parent().unwrap());
        fs::write(&store_flaker1, b"v1").unwrap();
        fs::write(&store_flaker2, b"v2-longer").unwrap();

        let sw1_bin = sys1.join("sw/bin");
        let sw2_bin = sys2.join("sw/bin");
        let _ = fs::create_dir_all(&sw1_bin);
        let _ = fs::create_dir_all(&sw2_bin);

        let _ = std::os::unix::fs::symlink(&store_flaker1, sw1_bin.join("flaker"));
        let _ = std::os::unix::fs::symlink(&store_flaker2, sw2_bin.join("flaker"));

        let rebuilts = find_rebuilt_system_packages(&sys1, &sys2, &[]);
        let _ = fs::remove_dir_all(&base);

        assert_eq!(rebuilts.len(), 1);
        assert_eq!(rebuilts[0].package, "flaker");
        assert_eq!(rebuilts[0].after_version, Some("0.2.0".to_string()));
        assert_eq!(rebuilts[0].kind, DiffKind::Rebuilt);
        assert_eq!(rebuilts[0].size_delta, Some("+7 B".to_string()));
    }

    #[test]
    fn test_clean_result_link() {
        let temp_dir =
            std::env::temp_dir().join(format!("flaker_test_result_link_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let link_target = temp_dir.join("target");
        let _ = std::fs::write(&link_target, "test");
        let link_path = temp_dir.join("result");
        #[cfg(unix)]
        let _ = std::os::unix::fs::symlink(&link_target, &link_path);

        assert!(link_path.symlink_metadata().is_ok());
        clean_result_link(&temp_dir, false);
        assert!(link_path.symlink_metadata().is_err());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
