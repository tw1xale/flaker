/// Expands a leading tilde (~) in a path string to the current user home directory.
pub fn expand_tilde(path_str: &str) -> PathBuf {
    if let Some(stripped) = path_str.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(stripped);
        }
    } else if path_str == "~"
        && let Ok(home) = std::env::var("HOME")
    {
        return PathBuf::from(home);
    }
    PathBuf::from(path_str)
}

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

use crate::config::Config;

/// Context and configuration of the active NixOS Flake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlakeContext {
    pub flake_dir: PathBuf,
    pub flake_target: String,
    pub is_git: bool,
    pub needs_sudo: bool,
}

mod nix_syntax;
pub use nix_syntax::attribute_names as parse_nix_attribute_names;

/// Strips comments (# and //) and quoted strings (" ... ") to prevent brace tracking errors.
pub fn strip_nix_comments_and_strings(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    let mut in_string = false;

    while let Some(c) = chars.next() {
        if in_string {
            if c == '\\' {
                let _ = chars.next();
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '#' || (c == '/' && chars.peek() == Some(&'/')) {
            break;
        } else if c == '"' {
            in_string = true;
        } else {
            out.push(c);
        }
    }
    out
}

pub fn fallback_parse_flake_configs(content: &str) -> Vec<String> {
    let mut configs = Vec::new();
    let mut in_configs_block = false;
    let mut in_pending_brace = false;
    let mut brace_depth: usize = 0;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.starts_with("//") || trimmed.starts_with("/*") {
            continue;
        }

        if let Some(pos) = trimmed.find("nixosConfigurations.") {
            let rest = &trimmed[pos + "nixosConfigurations.".len()..];
            let name: String = if let Some(unquoted) = rest.strip_prefix('"') {
                unquoted.chars().take_while(|c| *c != '"').collect()
            } else {
                rest.chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                    .collect()
            };
            if !name.is_empty() && !configs.contains(&name) {
                configs.push(name);
            }
        }

        if !in_configs_block {
            if trimmed.starts_with("nixosConfigurations")
                && (trimmed.contains('=') || trimmed.contains('{'))
                && !trimmed.starts_with("nixosConfigurations.")
            {
                let clean = strip_nix_comments_and_strings(trimmed);
                let opens = clean.chars().filter(|&c| c == '{').count();
                let closes = clean.chars().filter(|&c| c == '}').count();
                if opens > 0 {
                    in_configs_block = true;
                    brace_depth = opens.saturating_sub(closes);
                } else {
                    in_pending_brace = true;
                }
            } else if in_pending_brace {
                let clean = strip_nix_comments_and_strings(trimmed);
                let opens = clean.chars().filter(|&c| c == '{').count();
                let closes = clean.chars().filter(|&c| c == '}').count();
                if opens > 0 {
                    in_configs_block = true;
                    in_pending_brace = false;
                    brace_depth = opens.saturating_sub(closes);
                }
            }
        } else {
            let clean = strip_nix_comments_and_strings(trimmed);
            let opens = clean.chars().filter(|&c| c == '{').count();
            let closes = clean.chars().filter(|&c| c == '}').count();

            if brace_depth == 1
                && let Some((name_part, _)) = trimmed.split_once('=')
            {
                let clean_name = name_part.trim().trim_matches('"');
                let valid_name: String = clean_name
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                    .collect();
                if !valid_name.is_empty() && !configs.contains(&valid_name) {
                    configs.push(valid_name);
                }
            }

            brace_depth = brace_depth.saturating_add(opens).saturating_sub(closes);
            if brace_depth == 0 {
                in_configs_block = false;
            }
        }
    }
    configs
}

/// Best-effort host discovery: AST parser first, resilient heuristic fallback second.
pub fn parse_flake_configs(content: &str) -> Vec<String> {
    match parse_nix_attribute_names(content, "nixosConfigurations") {
        Ok(names) if !names.is_empty() => names,
        _ => fallback_parse_flake_configs(content),
    }
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
        // SAFETY: POSIX getuid() takes no arguments, cannot fail, and has no memory side effects.
        unsafe extern "C" {
            fn getuid() -> u32;
        }
        // SAFETY: Calling POSIX getuid() has no preconditions and is always safe.
        unsafe { getuid() }
    }
    #[cfg(not(unix))]
    {
        1000
    }
}

/// Select the first actual flake, retaining dynamically defined user configurations.
pub fn discover_flake_dir() -> Result<PathBuf> {
    let mut candidates = vec![std::env::current_dir()?];
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        candidates.extend([
            home.join(".config/nixos"),
            home.join("dotfiles/nixos"),
            home.join("dotfiles"),
            home.join("nixos-config"),
            home.join("nixos"),
        ]);
    }
    candidates.push(PathBuf::from("/etc/nixos"));
    candidates
        .into_iter()
        .find(|dir| dir.join("flake.nix").is_file())
        .ok_or_else(|| {
            anyhow::anyhow!("No flake.nix found. Pass --dir or --flake to select a configuration.")
        })
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
            return format!(
                "{dir_str}#{}",
                configs.first().map(String::as_str).unwrap_or("")
            );
        }
    }

    // Default fallback to hostname
    let fallback = if host.is_empty() { "nixos" } else { &host };
    format!("{dir_str}#{fallback}")
}

/// Remote targets have no local worktree to update, commit or reset.
pub fn is_remote_target(target: &str) -> bool {
    let reference = target
        .split_once('#')
        .map_or(target, |(reference, _)| reference);
    reference.contains(':') && !reference.starts_with("path:")
}

fn absolute_flake_dir(path: &str) -> Result<PathBuf> {
    let expanded = expand_tilde(path);
    let absolute = if expanded.is_absolute() {
        expanded
    } else {
        std::env::current_dir()?.join(expanded)
    };
    Ok(absolute.canonicalize().unwrap_or(absolute))
}

pub fn detect_flake_context(config: &Config) -> Result<FlakeContext> {
    let target_override = std::env::var("FLAKER_TARGET")
        .or_else(|_| std::env::var("FLAKE_TARGET"))
        .ok()
        .filter(|target| !target.trim().is_empty())
        .or_else(|| {
            (!config.general.flake_target.trim().is_empty())
                .then(|| config.general.flake_target.trim().to_string())
        });
    if let Some(target) = target_override {
        if is_remote_target(&target) {
            return Ok(FlakeContext {
                flake_dir: std::env::current_dir()?,
                flake_target: target,
                is_git: false,
                needs_sudo: false,
            });
        }
        let (path, selector) = target
            .split_once('#')
            .map_or((target.as_str(), None), |(path, selector)| {
                (path, Some(selector))
            });
        let path = path.strip_prefix("path:").unwrap_or(path);
        let dir = absolute_flake_dir(if path.is_empty() { "." } else { path })?;
        let flake_target = match selector {
            Some(selector) if !selector.is_empty() => format!("{}#{selector}", dir.display()),
            _ => resolve_target_for_dir(&dir),
        };
        return Ok(FlakeContext {
            is_git: is_git_repo(&dir),
            needs_sudo: check_needs_sudo(&dir),
            flake_dir: dir,
            flake_target,
        });
    }
    let directory = std::env::var("FLAKER_DIR")
        .or_else(|_| std::env::var("FLAKE_DIR"))
        .ok()
        .filter(|path| !path.trim().is_empty())
        .or_else(|| {
            (!config.general.flake_dir.trim().is_empty())
                .then(|| config.general.flake_dir.trim().to_string())
        });
    let dir = match directory {
        Some(path) => absolute_flake_dir(&path)?,
        None => discover_flake_dir()?,
    };
    Ok(FlakeContext {
        is_git: is_git_repo(&dir),
        needs_sudo: check_needs_sudo(&dir),
        flake_target: resolve_target_for_dir(&dir),
        flake_dir: dir,
    })
}

/// Trust only the repository explicitly selected by the user, for this command.
/// Child Git processes inside Nix receive the same scoped setting, including sudo.
pub fn make_cmd(program: &str, dir: &Path, needs_sudo: bool) -> Command {
    let repository = dir.canonicalize().ok().and_then(|path| {
        path.ancestors()
            .find(|ancestor| ancestor.join(".git").exists())
            .map(Path::to_path_buf)
    });
    let mut command = if needs_sudo {
        let mut command = Command::new("sudo");
        if let Some(root) = &repository {
            command.args([
                "env",
                "GIT_CONFIG_COUNT=2",
                "GIT_CONFIG_KEY_0=safe.directory",
            ]);
            command.arg(format!("GIT_CONFIG_VALUE_0={}", root.display()));
            command.args([
                "GIT_CONFIG_KEY_1=core.hooksPath",
                "GIT_CONFIG_VALUE_1=/dev/null",
            ]);
        }
        command.arg(program);
        command
    } else {
        let mut command = Command::new(program);
        if let Some(root) = &repository {
            if program == "git" {
                command
                    .arg("-c")
                    .arg(format!("safe.directory={}", root.display()));
            } else if program == "nix" || program == "nixos-rebuild" {
                command
                    .env("GIT_CONFIG_COUNT", "1")
                    .env("GIT_CONFIG_KEY_0", "safe.directory")
                    .env("GIT_CONFIG_VALUE_0", root);
            }
        }
        command
    };
    command.current_dir(dir);
    command
}

/// Suspend only when a TUI is active; nested command batches stay suspended.
pub struct TerminalSuspender {
    active: bool,
}

impl TerminalSuspender {
    pub fn suspend() -> Result<Self> {
        if !crossterm::terminal::is_raw_mode_enabled()? {
            return Ok(Self { active: false });
        }
        disable_raw_mode().context("Failed to disable terminal raw mode")?;
        if let Err(error) = execute!(
            stdout(),
            LeaveAlternateScreen,
            Show,
            crossterm::event::DisableMouseCapture
        ) {
            enable_raw_mode().context("Failed to restore raw mode after suspension failed")?;
            return Err(error.into());
        }
        Ok(Self { active: true })
    }

    pub fn resume(&mut self) -> Result<()> {
        if self.active {
            execute!(
                stdout(),
                EnterAlternateScreen,
                Hide,
                crossterm::event::EnableMouseCapture
            )
            .context("Failed to restore alternate screen")?;
            enable_raw_mode().context("Failed to restore raw mode")?;
            flush_stdin_events()?;
            self.active = false;
        }
        Ok(())
    }
}

impl Drop for TerminalSuspender {
    fn drop(&mut self) {
        // Destructors cannot return errors; the normal path calls resume explicitly.
        if let Err(error) = self.resume() {
            eprintln!("Terminal cleanup failed: {error:#}");
        }
    }
}

pub fn flush_stdin_events() -> Result<()> {
    while event::poll(Duration::from_millis(0))? {
        event::read()?;
    }
    Ok(())
}

/// Prints a styled action header in terminal mode before command execution, using the configured palette.
pub fn print_action_header_cli(title: &str, command_desc: &str) {
    let config = crate::config::load_config();
    let theme = crate::theme::Theme::from_config(&config);
    print_action_header_cli_with_theme(title, command_desc, &theme);
}

/// Prints a styled action header in terminal mode before command execution with a specific Theme.
pub fn print_action_header_cli_with_theme(
    title: &str,
    command_desc: &str,
    theme: &crate::theme::Theme,
) {
    println!(); // Preserve previous command output within a suspended batch.

    let term_width = crossterm::terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or(80);

    let title_w = unicode_width::UnicodeWidthStr::width(title);
    let desc_w = unicode_width::UnicodeWidthStr::width(command_desc);
    let content_len = title_w.max(desc_w).max(58);
    let inner_width = content_len.min(term_width.saturating_sub(4)).max(40);

    let border_line = "─".repeat(inner_width + 2);

    let format_line = |text: &str| -> String {
        let text_w = unicode_width::UnicodeWidthStr::width(text);
        if text_w > inner_width {
            let mut acc = 0;
            let mut truncated = String::new();
            let target = inner_width.saturating_sub(3);
            for ch in text.chars() {
                let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
                if acc + cw > target {
                    break;
                }
                truncated.push(ch);
                acc += cw;
            }
            truncated.push_str("...");
            let remaining = inner_width.saturating_sub(acc + 3);
            format!("{truncated}{}", " ".repeat(remaining))
        } else {
            let padding = inner_width.saturating_sub(text_w);
            format!("{text}{}", " ".repeat(padding))
        }
    };

    let border_fg = crate::theme::to_ansi_fg(theme.border);
    let title_fg = crate::theme::to_ansi_fg(theme.header_title);
    let desc_fg = crate::theme::to_ansi_fg(theme.muted_text);

    println!("{border_fg}╭{border_line}╮\x1b[0m");
    println!(
        "{border_fg}│\x1b[0m \x1b[1m{title_fg}{}\x1b[0m {border_fg}│\x1b[0m",
        format_line(title)
    );
    println!(
        "{border_fg}│\x1b[0m {desc_fg}{}\x1b[0m {border_fg}│\x1b[0m",
        format_line(command_desc)
    );
    println!("{border_fg}╰{border_line}╯\x1b[0m\n");
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

    if !status.success() {
        eprintln!("\n\x1b[33mPress Enter to return to Flaker...\x1b[0m");
        let mut buf = String::new();
        if std::io::IsTerminal::is_terminal(&std::io::stdin()) {
            std::io::stdin().read_line(&mut buf)?;
        }
    }

    suspender.resume()?;
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
    fn test_parse_flake_configs_comments_and_quotes() {
        let flake = r#"
            outputs = { self, nixpkgs, ... }: {
                # nixosConfigurations.commented-out = nixpkgs.lib.nixosSystem { ... };
                // nixosConfigurations.also-commented = nixpkgs.lib.nixosSystem { ... };
                nixosConfigurations."quoted-host" = nixpkgs.lib.nixosSystem { ... };
                nixosConfigurations.normal-host = nixpkgs.lib.nixosSystem { ... };
            };
        "#;
        let configs = parse_flake_configs(flake);
        assert_eq!(configs, vec!["quoted-host", "normal-host"]);
    }

    #[test]
    fn test_parse_flake_configs_block_syntax() {
        let flake = r#"
            outputs = { self, nixpkgs, ... }: {
                nixosConfigurations = {
                    workstation = nixpkgs.lib.nixosSystem { ... };
                    "media-center" = nixpkgs.lib.nixosSystem { ... };
                };
            };
        "#;
        let configs = parse_flake_configs(flake);
        assert_eq!(configs, vec!["workstation", "media-center"]);
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

    #[test]
    fn test_git_get_commit_files_and_diff() {
        let repo_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        if is_git_repo(repo_dir) {
            let _ = git::get_commit_files(repo_dir, "HEAD");
            let _ = git::get_file_diff_from_commit(repo_dir, "HEAD", "Cargo.toml");
        }
    }

    #[test]
    fn test_is_head_commit_and_parent() {
        let repo_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        if is_git_repo(repo_dir) {
            let _ = git::is_head_commit(repo_dir, "HEAD");
            let _ = git::is_head_commit(repo_dir, "head");
            assert!(!git::is_head_commit(repo_dir, "nonexistent123456789").unwrap_or(false));

            let _ = git::has_parent_commit(repo_dir, "HEAD");
            let _ = git::is_file_identical_to_head(repo_dir, "HEAD", "Cargo.toml");
            let _ = git::is_commit_identical_to_head(repo_dir, "HEAD");
        }
    }
}
