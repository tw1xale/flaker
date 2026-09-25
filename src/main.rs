/// RAII guard ensuring terminal raw mode and alternate screen are cleanly restored upon exit or panic.
struct TerminalGuard {
    active: bool,
}

impl TerminalGuard {
    fn new() -> Result<Self> {
        enable_raw_mode()?;
        let mut out = stdout();
        if let Err(err) = execute!(out, EnterAlternateScreen, Hide, EnableMouseCapture) {
            let _ = disable_raw_mode();
            return Err(err.into());
        }
        Ok(Self { active: true })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if self.active {
            self.active = false;
            let mut out = stdout();
            let _ = execute!(out, LeaveAlternateScreen, Show, DisableMouseCapture);
            let _ = disable_raw_mode();
        }
    }
}

mod actions;
mod app;
mod config;
mod theme;
mod ui;

use crate::actions::{git, nix};
use crate::app::{App, ExternalTask, ResultState, Screen, SubMenuKind};
use anyhow::Result;
use crossterm::{
    cursor::{Hide, Show},
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::{self, stdout};
use std::time::Duration;

fn main() -> Result<()> {
    // Handle simple CLI flags before entering raw mode
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        match args[1].as_str() {
            "-h" | "--help" => {
                println!("flaker {}", env!("CARGO_PKG_VERSION"));
                println!("Interactive TUI manager for NixOS Flakes\n");
                println!("USAGE:");
                println!("    flaker [OPTIONS]\n");
                println!("OPTIONS:");
                println!("    -h, --help       Print help information");
                println!("    -V, --version    Print version information");
                return Ok(());
            }
            "-V" | "--version" => {
                println!("flaker {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            _ => {}
        }
    }

    // Ensure terminal is restored on panic
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen, Show, DisableMouseCapture);
        default_hook(info);
    }));

    // Initialize application state before entering raw mode so startup warnings are visible
    let mut app = App::new();

    // Setup terminal with RAII cleanup guard
    let guard = TerminalGuard::new()?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    let res = run_app(&mut terminal, &mut app);

    // Restore terminal before reporting any application error to stderr
    drop(guard);

    if let Err(err) = res {
        eprintln!("Application error: {err}");
    }

    Ok(())
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    loop {
        if app.should_quit {
            return Ok(());
        }

        // Check if there is an external task to execute in terminal mode
        if !matches!(app.pending_external_task, ExternalTask::None) {
            let task = std::mem::replace(&mut app.pending_external_task, ExternalTask::None);
            let _ = execute_external_task(terminal, app, task);
            terminal.clear()?;
            continue;
        }

        terminal.draw(|frame| {
            app.render(frame);
        })?;

        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    app.handle_key(key);
                }
                Event::Mouse(mouse) => {
                    let term_width = terminal.size().map(|s| s.width).unwrap_or(80);
                    app.handle_mouse(mouse, term_width);
                }
                _ => {}
            }
        }
    }
}

fn execute_external_task(
    _terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    task: ExternalTask,
) -> Result<()> {
    let flake_target = app.flake_target.clone();
    let flake_dir = app.flake_dir.clone();
    let is_git = app.is_git;
    let needs_sudo = app.needs_sudo;

    /// Try to push; returns an optional warning suffix for the result message.
    fn try_push(dir: &std::path::Path, needs_sudo: bool, force: bool) -> String {
        match git::git_push(dir, needs_sudo, force) {
            Ok(()) => String::new(),
            Err(err) => format!("\n⚠ Git push failed: {err}"),
        }
    }

    fn show_post_activation_closure_diff(
        app: &mut App,
        before_system: Option<&std::path::Path>,
        title_suffix: &str,
        return_screen: SubMenuKind,
        success_title: &str,
        success_message: &str,
    ) {
        let after_system = std::fs::canonicalize("/run/current-system").ok();
        let diff_result = match (before_system, after_system.as_deref()) {
            (Some(before), Some(after)) => nix::get_closure_diff_between(before, after).ok(),
            _ => None,
        };

        if let Some(diff) = diff_result {
            let filtered_indices = (0..diff.items.len()).collect();
            app.screen = Screen::ClosureDiff(crate::app::ClosureDiffState {
                items: diff.items,
                filtered_indices,
                search_input: tui_input::Input::default(),
                scroll_offset: 0,
                is_identical_closure: diff.is_identical_closure,
                on_confirm_task: None,
                return_screen: Box::new(Screen::SubMenu(return_screen)),
                title_suffix: title_suffix.to_string(),
            });
        } else {
            app.screen = Screen::Result(ResultState {
                is_success: true,
                title: success_title.to_string(),
                message: success_message.to_string(),
                return_screen: Box::new(Screen::SubMenu(return_screen)),
            });
        }
    }

    match task {
        ExternalTask::None => {}

        // --- System Rebuild and Activation Actions ---
        ExternalTask::RebuildSwitchOnly => {
            let before_system = std::fs::canonicalize("/run/current-system").ok();
            match nix::nixos_rebuild_switch(&flake_target, &flake_dir) {
                Ok(()) => {
                    app.generation = nix::get_system_generation();
                    show_post_activation_closure_diff(
                        app,
                        before_system.as_deref(),
                        "Rebuild & Switch",
                        SubMenuKind::Updates,
                        "SYSTEM REBUILT SUCCESSFULLY",
                        "System successfully rebuilt and activated",
                    );
                }
                Err(err) => {
                    app.screen = Screen::Result(ResultState {
                        is_success: false,
                        title: "REBUILD FAILED".to_string(),
                        message: err.to_string(),
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                    });
                }
            }
        }

        ExternalTask::RebuildCommitAndSwitch(msg) => {
            let has_staged = if is_git {
                git::git_add(&flake_dir, needs_sudo, ".").is_ok()
                    && git::has_staged_changes(&flake_dir).unwrap_or(false)
            } else {
                false
            };

            if has_staged && git::git_commit(&flake_dir, needs_sudo, &msg).is_err() {
                app.screen = Screen::Result(ResultState {
                    is_success: false,
                    title: "GIT COMMIT FAILED".to_string(),
                    message: "Failed to record git commit before activating".to_string(),
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                });
                return Ok(());
            }

            let before_system = std::fs::canonicalize("/run/current-system").ok();
            match nix::nixos_rebuild_switch(&flake_target, &flake_dir) {
                Ok(()) => {
                    let push_warn = if is_git && has_staged {
                        try_push(&flake_dir, needs_sudo, false)
                    } else {
                        String::new()
                    };

                    app.generation = nix::get_system_generation();
                    let action_desc = if is_git && has_staged {
                        format!("System successfully rebuilt, committed, and activated{push_warn}")
                    } else {
                        "System successfully rebuilt and activated (working tree had no changes to commit)".to_string()
                    };

                    if !push_warn.is_empty() {
                        app.screen = Screen::Result(ResultState {
                            is_success: true,
                            title: "SYSTEM REBUILT SUCCESSFULLY".to_string(),
                            message: action_desc,
                            return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                        });
                    } else {
                        show_post_activation_closure_diff(
                            app,
                            before_system.as_deref(),
                            "Rebuild & Commit",
                            SubMenuKind::Updates,
                            "SYSTEM REBUILT SUCCESSFULLY",
                            &action_desc,
                        );
                    }
                }
                Err(err) => {
                    app.screen = Screen::Result(ResultState {
                        is_success: false,
                        title: "REBUILD FAILED".to_string(),
                        message: err.to_string(),
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                    });
                }
            }
        }

        ExternalTask::FullCycleSwitchOnly => match nix::nix_flake_update(&flake_dir, needs_sudo) {
            Err(err) => {
                app.screen = Screen::Result(ResultState {
                    is_success: false,
                    title: "FLAKE UPDATE FAILED".to_string(),
                    message: err.to_string(),
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                });
            }
            Ok(()) => {
                let before_system = std::fs::canonicalize("/run/current-system").ok();
                match nix::nixos_rebuild_switch(&flake_target, &flake_dir) {
                    Ok(()) => {
                        app.generation = nix::get_system_generation();
                        show_post_activation_closure_diff(
                            app,
                            before_system.as_deref(),
                            "Full Cycle Update",
                            SubMenuKind::Updates,
                            "FULL UPDATE CYCLE COMPLETED",
                            "System successfully updated to latest package versions and activated",
                        );
                    }
                    Err(err) => {
                        app.screen = Screen::Result(ResultState {
                            is_success: false,
                            title: "ACTIVATION FAILED".to_string(),
                            message: err.to_string(),
                            return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                        });
                    }
                }
            }
        },

        ExternalTask::FullCycleCommitAndSwitch(msg) => {
            match nix::nix_flake_update(&flake_dir, needs_sudo) {
                Err(err) => {
                    app.screen = Screen::Result(ResultState {
                        is_success: false,
                        title: "FLAKE UPDATE FAILED".to_string(),
                        message: err.to_string(),
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                    });
                }
                Ok(()) => {
                    let has_staged = if is_git {
                        git::git_add(&flake_dir, needs_sudo, ".").is_ok()
                            && git::has_staged_changes(&flake_dir).unwrap_or(false)
                    } else {
                        false
                    };

                    if has_staged && git::git_commit(&flake_dir, needs_sudo, &msg).is_err() {
                        app.screen = Screen::Result(ResultState {
                            is_success: false,
                            title: "GIT COMMIT FAILED".to_string(),
                            message: "Failed to record git commit before activating".to_string(),
                            return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                        });
                        return Ok(());
                    }

                    let before_system = std::fs::canonicalize("/run/current-system").ok();
                    match nix::nixos_rebuild_switch(&flake_target, &flake_dir) {
                        Ok(()) => {
                            let push_warn = if is_git && has_staged {
                                try_push(&flake_dir, needs_sudo, false)
                            } else {
                                String::new()
                            };

                            app.generation = nix::get_system_generation();
                            let action_desc = if is_git && has_staged {
                                format!(
                                    "System successfully updated to latest package versions, committed, and activated{push_warn}"
                                )
                            } else {
                                "System successfully updated to latest package versions and activated (working tree had no changes to commit)".to_string()
                            };

                            if !push_warn.is_empty() {
                                app.screen = Screen::Result(ResultState {
                                    is_success: true,
                                    title: "FULL UPDATE CYCLE COMPLETED".to_string(),
                                    message: action_desc,
                                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                                });
                            } else {
                                show_post_activation_closure_diff(
                                    app,
                                    before_system.as_deref(),
                                    "Full Cycle Update",
                                    SubMenuKind::Updates,
                                    "FULL UPDATE CYCLE COMPLETED",
                                    &action_desc,
                                );
                            }
                        }
                        Err(err) => {
                            app.screen = Screen::Result(ResultState {
                                is_success: false,
                                title: "ACTIVATION FAILED".to_string(),
                                message: err.to_string(),
                                return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                            });
                        }
                    }
                }
            }
        }

        ExternalTask::SelectiveFullCycleSwitchOnly(inputs) => {
            match nix::nix_flake_update_inputs(&flake_dir, needs_sudo, &inputs) {
                Err(err) => {
                    app.screen = Screen::Result(ResultState {
                        is_success: false,
                        title: "SELECTIVE FLAKE UPDATE FAILED".to_string(),
                        message: err.to_string(),
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::SelectiveUpdate)),
                    });
                }
                Ok(()) => {
                    let before_system = std::fs::canonicalize("/run/current-system").ok();
                    match nix::nixos_rebuild_switch(&flake_target, &flake_dir) {
                        Ok(()) => {
                            app.generation = nix::get_system_generation();
                            show_post_activation_closure_diff(
                                app,
                                before_system.as_deref(),
                                "Selective Update",
                                SubMenuKind::SelectiveUpdate,
                                "SELECTIVE FULL CYCLE COMPLETED",
                                &format!(
                                    "Selected input(s) ({}) updated, system activated",
                                    inputs.join(", ")
                                ),
                            );
                        }
                        Err(err) => {
                            app.screen = Screen::Result(ResultState {
                                is_success: false,
                                title: "ACTIVATION FAILED".to_string(),
                                message: err.to_string(),
                                return_screen: Box::new(Screen::SubMenu(
                                    SubMenuKind::SelectiveUpdate,
                                )),
                            });
                        }
                    }
                }
            }
        }

        ExternalTask::SelectiveFullCycleCommitAndSwitch(inputs, msg) => {
            match nix::nix_flake_update_inputs(&flake_dir, needs_sudo, &inputs) {
                Err(err) => {
                    app.screen = Screen::Result(ResultState {
                        is_success: false,
                        title: "SELECTIVE FLAKE UPDATE FAILED".to_string(),
                        message: err.to_string(),
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::SelectiveUpdate)),
                    });
                }
                Ok(()) => {
                    let mut committed = false;
                    if is_git {
                        let _ = git::git_add(&flake_dir, needs_sudo, "flake.lock");
                        if git::has_staged_changes(&flake_dir).unwrap_or(false) {
                            if let Err(err) = git::git_commit(&flake_dir, needs_sudo, &msg) {
                                app.screen = Screen::Result(ResultState {
                                    is_success: false,
                                    title: "GIT COMMIT FAILED".to_string(),
                                    message: err.to_string(),
                                    return_screen: Box::new(Screen::SubMenu(
                                        SubMenuKind::SelectiveUpdate,
                                    )),
                                });
                                return Ok(());
                            }
                            committed = true;
                        }
                    }

                    let before_system = std::fs::canonicalize("/run/current-system").ok();
                    match nix::nixos_rebuild_switch(&flake_target, &flake_dir) {
                        Ok(()) => {
                            let push_warn = if is_git && committed {
                                try_push(&flake_dir, needs_sudo, false)
                            } else {
                                String::new()
                            };

                            app.generation = nix::get_system_generation();
                            let action_desc = format!(
                                "Selected input(s) ({}) updated, system activated{push_warn}",
                                inputs.join(", ")
                            );

                            if !push_warn.is_empty() {
                                app.screen = Screen::Result(ResultState {
                                    is_success: true,
                                    title: "SELECTIVE FULL CYCLE COMPLETED".to_string(),
                                    message: action_desc,
                                    return_screen: Box::new(Screen::SubMenu(
                                        SubMenuKind::SelectiveUpdate,
                                    )),
                                });
                            } else {
                                show_post_activation_closure_diff(
                                    app,
                                    before_system.as_deref(),
                                    "Selective Update",
                                    SubMenuKind::SelectiveUpdate,
                                    "SELECTIVE FULL CYCLE COMPLETED",
                                    &action_desc,
                                );
                            }
                        }
                        Err(err) => {
                            app.screen = Screen::Result(ResultState {
                                is_success: false,
                                title: "ACTIVATION FAILED".to_string(),
                                message: err.to_string(),
                                return_screen: Box::new(Screen::SubMenu(
                                    SubMenuKind::SelectiveUpdate,
                                )),
                            });
                        }
                    }
                }
            }
        }

        ExternalTask::SoftRevertCommitAndSwitch(hash, msg) => {
            let r1 = git::git_checkout_files(&flake_dir, needs_sudo, &hash);
            let r2 = if r1.is_ok() {
                git::git_add(&flake_dir, needs_sudo, ".")
            } else {
                r1
            };
            if r2.is_ok() && !git::has_staged_changes(&flake_dir).unwrap_or(false) {
                app.screen = Screen::Result(ResultState {
                    is_success: true,
                    title: "NO CHANGES TO REVERT".to_string(),
                    message: format!(
                        "Working tree files are already identical to commit {hash}. No revert commit needed."
                    ),
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                });
                return Ok(());
            }
            if let Err(err) = r2 {
                app.screen = Screen::Result(ResultState {
                    is_success: false,
                    title: "SOFT REVERT FAILED".to_string(),
                    message: err.to_string(),
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                });
                return Ok(());
            }

            let has_staged = if is_git {
                git::has_staged_changes(&flake_dir).unwrap_or(false)
            } else {
                false
            };

            if has_staged && git::git_commit(&flake_dir, needs_sudo, &msg).is_err() {
                app.screen = Screen::Result(ResultState {
                    is_success: false,
                    title: "GIT COMMIT FAILED".to_string(),
                    message: "Failed to record soft revert commit".to_string(),
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                });
                return Ok(());
            }

            let before_system = std::fs::canonicalize("/run/current-system").ok();
            match nix::nixos_rebuild_switch(&flake_target, &flake_dir) {
                Ok(()) => {
                    let push_warn = if is_git && has_staged {
                        try_push(&flake_dir, needs_sudo, false)
                    } else {
                        String::new()
                    };

                    app.generation = nix::get_system_generation();
                    let action_desc = if is_git && has_staged {
                        format!(
                            "Working tree reverted to {hash}, committed, and system activated{push_warn}"
                        )
                    } else {
                        format!("Working tree was already at {hash}, system activated")
                    };

                    if !push_warn.is_empty() {
                        app.screen = Screen::Result(ResultState {
                            is_success: true,
                            title: "SOFT REVERT SUCCESSFUL".to_string(),
                            message: action_desc,
                            return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                        });
                    } else {
                        show_post_activation_closure_diff(
                            app,
                            before_system.as_deref(),
                            &format!("Soft Revert to {hash}"),
                            SubMenuKind::GitHistory,
                            "SOFT REVERT SUCCESSFUL",
                            &action_desc,
                        );
                    }
                }
                Err(err) => {
                    app.screen = Screen::Result(ResultState {
                        is_success: false,
                        title: "REBUILD AFTER SOFT REVERT FAILED".to_string(),
                        message: err.to_string(),
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                    });
                }
            }
        }

        ExternalTask::RestoreCommitAndSwitch(hash, file, msg) => {
            let has_changes = if is_git {
                git::has_uncommitted_changes(&flake_dir).unwrap_or(false)
            } else {
                false
            };

            if !has_changes {
                app.screen = Screen::Result(ResultState {
                    is_success: true,
                    title: "NOTHING TO COMMIT".to_string(),
                    message: "Working tree has no changes. No commit was created.".to_string(),
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                });
                return Ok(());
            }

            let has_staged = if is_git {
                git::git_add(&flake_dir, needs_sudo, ".").is_ok()
                    && git::has_staged_changes(&flake_dir).unwrap_or(false)
            } else {
                false
            };

            if has_staged && git::git_commit(&flake_dir, needs_sudo, &msg).is_err() {
                app.screen = Screen::Result(ResultState {
                    is_success: false,
                    title: "GIT COMMIT FAILED".to_string(),
                    message: format!("Failed to record commit restoring {file} from {hash}"),
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                });
                return Ok(());
            }

            let before_system = std::fs::canonicalize("/run/current-system").ok();
            match nix::nixos_rebuild_switch(&flake_target, &flake_dir) {
                Ok(()) => {
                    let push_warn = if is_git && has_staged {
                        try_push(&flake_dir, needs_sudo, false)
                    } else {
                        String::new()
                    };

                    app.generation = nix::get_system_generation();
                    let action_desc = if is_git && has_staged {
                        format!(
                            "Restored {file} from {hash}, committed, and system activated{push_warn}"
                        )
                    } else {
                        format!("File {file} was already identical to {hash}, system activated")
                    };

                    if !push_warn.is_empty() {
                        app.screen = Screen::Result(ResultState {
                            is_success: true,
                            title: "RESTORE & REBUILD SUCCESSFUL".to_string(),
                            message: action_desc,
                            return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                        });
                    } else {
                        show_post_activation_closure_diff(
                            app,
                            before_system.as_deref(),
                            &format!("Restore {file} from {hash}"),
                            SubMenuKind::GitHistory,
                            "RESTORE & REBUILD SUCCESSFUL",
                            &action_desc,
                        );
                    }
                }
                Err(err) => {
                    app.screen = Screen::Result(ResultState {
                        is_success: false,
                        title: "REBUILD FAILED".to_string(),
                        message: err.to_string(),
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                    });
                }
            }
        }

        ExternalTask::HardReset(hash) => {
            let r1 = git::git_reset_hard(&flake_dir, needs_sudo, &hash);
            if let Err(err) = r1 {
                app.screen = Screen::Result(ResultState {
                    is_success: false,
                    title: format!("HARD ROLLBACK TO {hash} FAILED"),
                    message: err.to_string(),
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                });
                return Ok(());
            }

            let before_system = std::fs::canonicalize("/run/current-system").ok();
            match nix::nixos_rebuild_switch(&flake_target, &flake_dir) {
                Ok(()) => {
                    let push_warn = if is_git {
                        try_push(&flake_dir, needs_sudo, true)
                    } else {
                        String::new()
                    };

                    app.generation = nix::get_system_generation();
                    let action_desc =
                        format!("Hard reset to {hash} and system activated{push_warn}");

                    if !push_warn.is_empty() {
                        app.screen = Screen::Result(ResultState {
                            is_success: true,
                            title: "HARD ROLLBACK SUCCESSFUL".to_string(),
                            message: action_desc,
                            return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                        });
                    } else {
                        show_post_activation_closure_diff(
                            app,
                            before_system.as_deref(),
                            &format!("Hard Reset to {hash}"),
                            SubMenuKind::GitHistory,
                            "HARD ROLLBACK SUCCESSFUL",
                            &action_desc,
                        );
                    }
                }
                Err(err) => {
                    app.screen = Screen::Result(ResultState {
                        is_success: false,
                        title: format!("HARD ROLLBACK TO {hash} FAILED"),
                        message: err.to_string(),
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                    });
                }
            }
        }

        // --- Direct Non-Switching Actions ---
        ExternalTask::UpdateFlakeOnly => match nix::nix_flake_update(&flake_dir, needs_sudo) {
            Err(err) => {
                app.screen = Screen::Result(ResultState {
                    is_success: false,
                    title: "FLAKE UPDATE FAILED".to_string(),
                    message: err.to_string(),
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                });
            }
            Ok(()) => {
                app.screen = Screen::Result(ResultState {
                    is_success: true,
                    title: "FLAKE LOCKFILE UPDATED".to_string(),
                    message: "flake.lock has been successfully updated".to_string(),
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                });
            }
        },

        ExternalTask::UpdateFlakeAndPush(msg) => {
            match nix::nix_flake_update(&flake_dir, needs_sudo) {
                Err(err) => {
                    app.screen = Screen::Result(ResultState {
                        is_success: false,
                        title: "FLAKE UPDATE FAILED".to_string(),
                        message: err.to_string(),
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                    });
                }
                Ok(()) => {
                    let has_staged = if is_git {
                        git::git_add(&flake_dir, needs_sudo, "flake.lock").is_ok()
                            && git::has_staged_changes(&flake_dir).unwrap_or(false)
                    } else {
                        false
                    };

                    if is_git && !has_staged {
                        app.screen = Screen::Result(ResultState {
                            is_success: true,
                            title: "FLAKE LOCKFILE UP TO DATE".to_string(),
                            message:
                                "flake.lock is already up to date. No changes to commit or push."
                                    .to_string(),
                            return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                        });
                    } else if is_git && git::git_commit(&flake_dir, needs_sudo, &msg).is_err() {
                        app.screen = Screen::Result(ResultState {
                            is_success: false,
                            title: "GIT COMMIT FAILED".to_string(),
                            message: "Failed to record git commit after flake update".to_string(),
                            return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                        });
                    } else {
                        let push_warn = if is_git {
                            try_push(&flake_dir, needs_sudo, false)
                        } else {
                            String::new()
                        };

                        app.screen = Screen::Result(ResultState {
                            is_success: true,
                            title: "FLAKE UPDATED AND PUSHED".to_string(),
                            message: format!(
                                "flake.lock committed and pushed successfully{push_warn}"
                            ),
                            return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                        });
                    }
                }
            }
        }

        ExternalTask::SelectiveUpdateFlakeOnly(inputs) => {
            match nix::nix_flake_update_inputs(&flake_dir, needs_sudo, &inputs) {
                Err(err) => {
                    app.screen = Screen::Result(ResultState {
                        is_success: false,
                        title: "SELECTIVE FLAKE UPDATE FAILED".to_string(),
                        message: err.to_string(),
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::SelectiveUpdate)),
                    });
                }
                Ok(()) => {
                    app.screen = Screen::Result(ResultState {
                        is_success: true,
                        title: "FLAKE LOCKFILE UPDATED".to_string(),
                        message: format!(
                            "Selected input(s) ({}) have been successfully updated in flake.lock",
                            inputs.join(", ")
                        ),
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::SelectiveUpdate)),
                    });
                }
            }
        }

        ExternalTask::TestBuild => {
            match nix::nixos_rebuild_build(&flake_target, &flake_dir, needs_sudo) {
                Err(err) => {
                    app.screen = Screen::Result(ResultState {
                        is_success: false,
                        title: "TEST BUILD FAILED".to_string(),
                        message: err.to_string(),
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                    });
                }
                Ok(()) => {
                    nix::clean_result_link(&flake_dir, needs_sudo);
                    app.screen = Screen::Result(ResultState {
                        is_success: true,
                        title: "TEST BUILD SUCCESSFUL".to_string(),
                        message: "The NixOS configuration built without errors".to_string(),
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                    });
                }
            }
        }

        ExternalTask::CleanStore => match nix::cleanup_nix_store() {
            Err(err) => {
                app.screen = Screen::Result(ResultState {
                    is_success: false,
                    title: "CLEANUP FAILED".to_string(),
                    message: err.to_string(),
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::Maintenance)),
                });
            }
            Ok(()) => {
                app.screen = Screen::Result(ResultState {
                    is_success: true,
                    title: "NIX STORE CLEANED & OPTIMIZED".to_string(),
                    message: "Nix Store successfully cleaned and optimized".to_string(),
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::Maintenance)),
                });
            }
        },

        ExternalTask::TrimHistoryCommitAndPush(msg, hash) => {
            let r1 = git::git_reset_soft(&flake_dir, needs_sudo, &hash);
            let r2 = if r1.is_ok() {
                git::git_commit(&flake_dir, needs_sudo, &msg)
            } else {
                r1
            };
            let r3 = if r2.is_ok() {
                git::git_push(&flake_dir, needs_sudo, true)
            } else {
                r2
            };

            match r3 {
                Err(err) => {
                    app.screen = Screen::Result(ResultState {
                        is_success: false,
                        title: "HISTORY COMPACT FAILED".to_string(),
                        message: err.to_string(),
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                    });
                }
                Ok(()) => {
                    app.screen = Screen::Result(ResultState {
                        is_success: true,
                        title: "HISTORY COMPACT SUCCESSFUL".to_string(),
                        message: format!("Commits up to {hash} squashed and force-pushed"),
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                    });
                }
            }
        }

        ExternalTask::RestoreFile(hash, file) => {
            match git::git_restore_file(&flake_dir, needs_sudo, &hash, &file) {
                Err(err) => {
                    app.screen = Screen::Result(ResultState {
                        is_success: false,
                        title: "RESTORE FILE FAILED".to_string(),
                        message: err.to_string(),
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                    });
                }
                Ok(()) => {
                    let has_changes = git::has_uncommitted_changes(&flake_dir).unwrap_or(false);
                    if has_changes {
                        let cancel_screen = Box::new(Screen::Result(ResultState {
                            is_success: true,
                            title: "FILE RESTORED TO WORKING TREE".to_string(),
                            message: format!(
                                "Restored '{file}' into working tree from {hash}.\nChanges remain uncommitted for your review."
                            ),
                            return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                        }));

                        app.screen = Screen::Confirm(crate::app::ConfirmState {
                            title: "RESTORE COMPLETED".to_string(),
                            lines: vec![
                                format!("Restored '{file}' from {hash} into working tree."),
                                "Would you like to commit and switch system now?".to_string(),
                            ],
                            affirmative_label: "Commit & Switch".to_string(),
                            negative_label: "Keep in Working Tree".to_string(),
                            selected_button: 1,
                            is_danger: false,
                            on_confirm: crate::app::PendingAction::RestoreCommitAndSwitch(
                                hash, file,
                            ),
                            on_secondary: None,
                            return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                            on_cancel_screen: Some(cancel_screen),
                        });
                    } else {
                        app.screen = Screen::Result(ResultState {
                            is_success: true,
                            title: "NO CHANGES DETECTED".to_string(),
                            message: format!(
                                "File '{file}' at {hash} is already identical to the current working tree. No changes applied."
                            ),
                            return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                        });
                    }
                }
            }
        }

        ExternalTask::RestorePatch(hash, file_opt) => {
            let res = git::git_restore_patch(&flake_dir, needs_sudo, &hash, file_opt.as_deref());
            match res {
                Err(err) => {
                    app.screen = Screen::Result(ResultState {
                        is_success: false,
                        title: "PATCH RESTORE FAILED".to_string(),
                        message: err.to_string(),
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                    });
                }
                Ok(()) => {
                    let has_changes = git::has_uncommitted_changes(&flake_dir).unwrap_or(false);
                    if has_changes {
                        let target_name = file_opt.unwrap_or_else(|| "all files".to_string());
                        let cancel_screen = Box::new(Screen::Result(ResultState {
                            is_success: true,
                            title: "PATCH KEPT IN WORKING TREE".to_string(),
                            message: format!(
                                "Applied interactive changes from {hash} ({target_name}).\nChanges remain uncommitted for your review."
                            ),
                            return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                        }));

                        app.screen = Screen::Confirm(crate::app::ConfirmState {
                            title: "PATCH RESTORE COMPLETED".to_string(),
                            lines: vec![
                                format!("Applied interactive changes from {hash} ({target_name})."),
                                "Would you like to commit and switch system now?".to_string(),
                            ],
                            affirmative_label: "Commit & Switch".to_string(),
                            negative_label: "Keep in Working Tree".to_string(),
                            selected_button: 1,
                            is_danger: false,
                            on_confirm: crate::app::PendingAction::RestoreCommitAndSwitch(
                                hash,
                                target_name,
                            ),
                            on_secondary: None,
                            return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                            on_cancel_screen: Some(cancel_screen),
                        });
                    } else {
                        app.screen = Screen::Result(ResultState {
                            is_success: true,
                            title: "NO CHANGES APPLIED".to_string(),
                            message: "Interactive patch finished without modifications."
                                .to_string(),
                            return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                        });
                    }
                }
            }
        }
    }

    app.refresh_metadata();

    Ok(())
}
