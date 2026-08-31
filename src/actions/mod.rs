pub mod git;
pub mod nix;

use anyhow::{Context, Result};
use crossterm::{
    cursor::{Hide, Show},
    event, execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use std::io::stdout;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output, Stdio};
use std::time::Duration;

/// Context and configuration of the active NixOS Flake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlakeContext {
    pub flake_dir: PathBuf,
    pub flake_target: String,
    pub is_git: bool,
    pub needs_sudo: bool,
}

/// Extracts all `nixosConfigurations.<name>` identifiers from flake content.
pub fn parse_flake_configs(content: &str) -> Vec<String> {
    let mut configs = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(pos) = trimmed.find("nixosConfigurations.") {
            let rest = &trimmed[pos + "nixosConfigurations.".len()..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                .collect();
            if !name.is_empty() && !configs.contains(&name) {
                configs.push(name);
            }
        }
    }
    configs
}

/// Checks if a given directory is a Git repository.
pub fn is_git_repo(dir: &Path) -> bool {
    if dir.join(".git").exists() {
        return true;
    }
    Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Checks if file/git operations in this directory require `sudo` privileges.
pub fn check_needs_sudo(dir: &Path) -> bool {
    // If owned by root and current process is not root, we need sudo
    if let Ok(meta) = std::fs::metadata(dir) {
        let is_root_owned = meta.uid() == 0;
        let current_uid = libc_getuid();
        if is_root_owned && current_uid != 0 {
            return true;
        }
    }

    if dir.starts_with("/etc") {
        return libc_getuid() != 0;
    }

    false
}

fn libc_getuid() -> u32 {
    #[cfg(unix)]
    {
        unsafe extern "C" {
            fn getuid() -> u32;
        }
        unsafe { getuid() }
    }
    #[cfg(not(unix))]
    {
        1000
    }
}

/// Discovers the most appropriate NixOS flake directory.
pub fn discover_flake_dir() -> PathBuf {
    // 1. Explicit FLAKER_DIR / FLAKE_DIR env
    if let Ok(val) = std::env::var("FLAKER_DIR").or_else(|_| std::env::var("FLAKE_DIR")) {
        let p = PathBuf::from(val.trim());
        if p.join("flake.nix").exists() || p.exists() {
            return p;
        }
    }

    let mut candidates = Vec::new();

    // 2. Current working directory
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd);
    }

    // 3. Common user home directories
    if let Ok(home) = std::env::var("HOME") {
        let home_path = PathBuf::from(home);
        candidates.push(home_path.join(".config").join("nixos"));
        candidates.push(home_path.join("dotfiles").join("nixos"));
        candidates.push(home_path.join("dotfiles"));
        candidates.push(home_path.join("nixos-config"));
        candidates.push(home_path.join("nixos"));
    }

    // 4. System default
    candidates.push(PathBuf::from("/etc/nixos"));

    for candidate in candidates {
        if candidate.join("flake.nix").exists() {
            return candidate;
        }
    }

    PathBuf::from("/etc/nixos")
}

/// Resolves the flake target string (e.g. "/etc/nixos#hostname" or "~/dotfiles#user") for a directory.
pub fn resolve_target_for_dir(dir: &Path) -> String {
    let host = whoami::fallible::hostname().unwrap_or_default();
    let user = whoami::fallible::username().unwrap_or_default();
    let dir_str = dir.display().to_string();

    let flake_file = dir.join("flake.nix");
    if let Ok(content) = std::fs::read_to_string(&flake_file) {
        let configs = parse_flake_configs(&content);

        // Priority 1: Configuration matching system hostname
        if !host.is_empty() && configs.iter().any(|c| c == &host) {
            return format!("{dir_str}#{host}");
        }

        // Priority 2: Configuration matching current username
        if !user.is_empty() && configs.iter().any(|c| c == &user) {
            return format!("{dir_str}#{user}");
        }

        // Priority 3: Explicit "default" configuration
        if configs.iter().any(|c| c == "default") {
            return format!("{dir_str}#default");
        }

        // Priority 4: If only one configuration is defined, use it
        if configs.len() == 1 {
            return format!("{dir_str}#{}", configs[0]);
        }
    }

    // Default fallback to hostname
    let fallback = if host.is_empty() { "nixos" } else { &host };
    format!("{dir_str}#{fallback}")
}

/// Auto-detects the active NixOS flake target and environment context.
pub fn detect_flake_context() -> FlakeContext {
    // 1. Explicit override via environment variable
    if let Ok(val) = std::env::var("FLAKER_TARGET").or_else(|_| std::env::var("FLAKE_TARGET")) {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            let (dir_part, _target_name) = if let Some(idx) = trimmed.find('#') {
                (&trimmed[..idx], &trimmed[idx + 1..])
            } else {
                (trimmed, "")
            };

            let dir = if dir_part.is_empty() || dir_part == "." {
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
            } else {
                PathBuf::from(dir_part)
            };

            let is_git = is_git_repo(&dir);
            let needs_sudo = check_needs_sudo(&dir);

            return FlakeContext {
                flake_dir: dir,
                flake_target: trimmed.to_string(),
                is_git,
                needs_sudo,
            };
        }
    }

    let dir = discover_flake_dir();
    let is_git = is_git_repo(&dir);
    let needs_sudo = check_needs_sudo(&dir);
    let flake_target = resolve_target_for_dir(&dir);

    FlakeContext {
        flake_dir: dir,
        flake_target,
        is_git,
        needs_sudo,
    }
}

/// Helper to construct a command with or without sudo in the specified directory.
pub fn make_cmd(program: &str, dir: &Path, needs_sudo: bool) -> Command {
    let mut cmd = if needs_sudo {
        let mut c = Command::new("sudo");
        c.arg(program);
        c
    } else {
        Command::new(program)
    };
    cmd.current_dir(dir);
    cmd
}

/// RAII Scope Guard that ensures terminal raw mode and alternate screen are ALWAYS restored upon drop.
pub struct TerminalSuspender {
    active: bool,
}

impl TerminalSuspender {
    pub fn suspend() -> Result<Self> {
        disable_raw_mode().context("Failed to disable raw mode before running command")?;
        let mut out = stdout();
        let _ = execute!(out, LeaveAlternateScreen, Show);
        Ok(Self { active: true })
    }

    pub fn resume(&mut self) {
        if self.active {
            self.active = false;
            let mut out = stdout();
            let _ = execute!(out, EnterAlternateScreen, Hide);
            let _ = enable_raw_mode();
            flush_stdin_events();
        }
    }
}

impl Drop for TerminalSuspender {
    fn drop(&mut self) {
        self.resume();
    }
}

/// Flushes any accumulated stdin events (e.g. extra Enter key from password prompt).
pub fn flush_stdin_events() {
    while event::poll(Duration::from_millis(0)).unwrap_or(false) {
        let _ = event::read();
    }
}

/// Prints a styled action header in terminal mode before command execution.
pub fn print_action_header_cli(title: &str, command_desc: &str) {
    println!("\x1b[2J\x1b[H"); // Clear screen
    println!(
        "\x1b[38;2;137;180;250m╭──────────────────────────────────────────────────────────────╮\x1b[0m"
    );
    println!(
        "\x1b[38;2;137;180;250m│\x1b[0m \x1b[1;38;2;137;220;235m{:<60}\x1b[0m \x1b[38;2;137;180;250m│\x1b[0m",
        title
    );
    println!(
        "\x1b[38;2;137;180;250m│\x1b[0m \x1b[38;2;166;173;200m{:<60}\x1b[0m \x1b[38;2;137;180;250m│\x1b[0m",
        command_desc
    );
    println!(
        "\x1b[38;2;137;180;250m╰──────────────────────────────────────────────────────────────╯\x1b[0m\n"
    );
}

/// Runs a command visibly on the terminal by temporarily suspending TUI raw mode and alternate screen.
/// Always prints the action header before running.
pub fn run_visible(title: &str, command_desc: &str, cmd: &mut Command) -> Result<ExitStatus> {
    let mut suspender = TerminalSuspender::suspend()?;

    print_action_header_cli(title, command_desc);

    cmd.stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let status = cmd
        .status()
        .context("Failed to wait for command execution")?;

    suspender.resume();
    Ok(status)
}

/// Runs a command silently without suspending the TUI or printing headers.
/// Suitable for non-interactive read-only operations where sudo credentials are already cached.
pub fn run_silent(cmd: &mut Command) -> Result<Output> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("Failed to execute command")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_flake_configs_single() {
        let flake = r#"
            outputs = { self, nixpkgs, ... }: {
                nixosConfigurations.my-desktop = nixpkgs.lib.nixosSystem { ... };
            };
        "#;
        let configs = parse_flake_configs(flake);
        assert_eq!(configs, vec!["my-desktop"]);
    }

    #[test]
    fn test_parse_flake_configs_multiple() {
        let flake = r#"
            outputs = { self, nixpkgs, ... }: {
                nixosConfigurations.laptop = nixpkgs.lib.nixosSystem { ... };
                nixosConfigurations.server = nixpkgs.lib.nixosSystem { ... };
                nixosConfigurations.home = nixpkgs.lib.nixosSystem { ... };
            };
        "#;
        let configs = parse_flake_configs(flake);
        assert_eq!(configs, vec!["laptop", "server", "home"]);
    }

    #[test]
    fn test_is_git_repo_detection() {
        let temp_dir = std::env::temp_dir().join(format!("flaker_test_git_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        assert!(!is_git_repo(&temp_dir));

        let git_dir = temp_dir.join(".git");
        let _ = std::fs::create_dir_all(&git_dir);
        assert!(is_git_repo(&temp_dir));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
