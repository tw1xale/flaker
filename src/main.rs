/// RAII guard ensuring terminal raw mode and alternate screen are cleanly restored upon exit or panic.
struct TerminalGuard {
    active: bool,
}

impl TerminalGuard {
    fn new() -> Result<Self> {
        enable_raw_mode()?;
        let mut out = stdout();
        if let Err(err) = execute!(out, EnterAlternateScreen, Hide, EnableMouseCapture) {
            restore_terminal().context("Terminal setup failed and cleanup also failed")?;
            return Err(err.into());
        }
        Ok(Self { active: true })
    }
}

fn restore_terminal() -> Result<()> {
    // Attempt both cleanup operations even if one fails.
    let screen = execute!(stdout(), LeaveAlternateScreen, Show, DisableMouseCapture);
    let raw = disable_raw_mode();
    screen.context("Failed to restore the terminal screen")?;
    raw.context("Failed to restore cooked input")?;
    Ok(())
}

impl TerminalGuard {
    fn restore(&mut self) -> Result<()> {
        if self.active {
            restore_terminal()?;
            self.active = false;
        }
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // Destructors cannot return errors; the normal exit restores explicitly.
        if let Err(error) = self.restore() {
            eprintln!("Terminal cleanup failed: {error:#}");
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
use anyhow::{Context, Result};
use crossterm::{
    cursor::{Hide, Show},
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::{self, stdout};

#[derive(Debug, PartialEq, Eq)]
enum CliAction {
    Run,
    Help,
    Version,
}

fn cli_action(args: &[String]) -> Result<CliAction> {
    match args {
        [] => Ok(CliAction::Run),
        [flag] if flag == "-h" || flag == "--help" => Ok(CliAction::Help),
        [flag] if flag == "-V" || flag == "--version" => Ok(CliAction::Version),
        _ => anyhow::bail!(
            "Unsupported arguments: {}. Use --help for usage.",
            args.join(" ")
        ),
    }
}

fn main() -> Result<()> {
    // Validate every argument before loading config or touching terminal state.
    let args: Vec<String> = std::env::args().skip(1).collect();
    match cli_action(&args)? {
        CliAction::Help => {
            println!(
                "flaker {}\nInteractive TUI manager for NixOS Flakes\n",
                env!("CARGO_PKG_VERSION")
            );
            println!("USAGE:\n    flaker [OPTIONS]\n");
            println!(
                "OPTIONS:\n    -h, --help       Print help information\n    -V, --version    Print version information\n"
            );
            println!(
                "Set FLAKER_DIR to select a local flake directory, or FLAKER_TARGET for a flake target."
            );
            return Ok(());
        }
        CliAction::Version => {
            println!("flaker {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        CliAction::Run => {}
    }

    // Ensure terminal is restored on panic
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Err(error) = restore_terminal() {
            eprintln!("Terminal cleanup after panic failed: {error:#}");
        }
        default_hook(info);
    }));

    // Initialize application state before entering raw mode so startup warnings are visible
    let mut app = App::new()?;

    // Setup terminal with RAII cleanup guard
    let mut guard = TerminalGuard::new()?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    let res = run_app(&mut terminal, &mut app);

    // Restore terminal before reporting any application error to stderr
    let cleanup = guard.restore();
    res?;
    cleanup
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    loop {
        if app.should_quit {
            return Ok(());
        }

        // Check if there is an external task to execute in terminal mode
        if !matches!(app.pending_external_task, ExternalTask::None) {
            let task = std::mem::replace(&mut app.pending_external_task, ExternalTask::None);
            let return_screen = task.return_screen();
            let mut suspender = actions::TerminalSuspender::suspend()?;
            if let Err(error) = execute_external_task(app, task) {
                app.screen = Screen::Result(ResultState {
                    is_success: false,
                    title: "ACTION FAILED".to_string(),
                    message: format!("{error:#}"),
                    return_screen: Box::new(Screen::SubMenu(return_screen)),
                });
            }
            suspender.resume()?;
            terminal.clear()?;
            continue;
        }

        terminal.draw(|frame| {
            app.render(frame);
        })?;

        // There are no animations or background jobs: redraw only after an event.
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                app.handle_key(key);
            }
            Event::Mouse(mouse) => {
                let term_width = terminal.size()?.width;
                app.handle_mouse(mouse, term_width);
            }
            _ => {}
        }
    }
}

fn execute_external_task(app: &mut App, task: ExternalTask) -> Result<()> {
    let flake_target = app.flake_target.clone();
    let flake_dir = app.flake_dir.clone();
    let is_git = app.is_git;
    let needs_sudo = app.needs_sudo;

    if actions::is_remote_target(&flake_target)
        && !matches!(
            task,
            ExternalTask::None
                | ExternalTask::RebuildSwitchOnly
                | ExternalTask::TestBuild
                | ExternalTask::CleanStore
        )
    {
        anyhow::bail!(
            "This action changes a local flake. Select a checkout with FLAKER_DIR; the remote target was not modified."
        );
    }

    if is_git
        && matches!(
            task,
            ExternalTask::UpdateFlakeOnly
                | ExternalTask::UpdateFlakeAndPush(_)
                | ExternalTask::FullCycleSwitchOnly
                | ExternalTask::FullCycleCommitAndSwitch(_)
                | ExternalTask::SelectiveUpdateFlakeOnly(_)
                | ExternalTask::SelectiveFullCycleSwitchOnly(_)
                | ExternalTask::SelectiveFullCycleCommitAndSwitch(_, _)
        )
    {
        git::ensure_path_clean(&flake_dir, "flake.lock")?;
    }

    /// Try to push; returns an optional warning suffix for the result message.
    fn try_push(dir: &std::path::Path, needs_sudo: bool, force: bool) -> String {
        match git::git_push(dir, needs_sudo, force) {
            Ok(()) => String::new(),
            Err(err) => format!("\n⚠ Git push failed: {err}"),
        }
    }

    fn show_post_activation_closure_diff(
        app: &mut App,
        before_system: std::result::Result<&std::path::Path, &std::io::Error>,
        title_suffix: &str,
        return_screen: SubMenuKind,
        success_title: &str,
        success_message: &str,
    ) {
        let diff_result = (|| -> Result<nix::SystemClosureDiff> {
            let before = before_system
                .map_err(|error| anyhow::anyhow!("Previous system profile: {error}"))?;
            let after = std::fs::canonicalize("/run/current-system")
                .context("Cannot resolve the active system profile")?;
            nix::get_closure_diff_between(before, &after)
        })();

        match diff_result {
            Ok(diff) => {
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
            }
            Err(error) => {
                app.screen = Screen::Result(ResultState {
                    is_success: true,
                    title: success_title.to_string(),
                    message: format!(
                        "{success_message}\nClosure comparison unavailable: {error:#}"
                    ),
                    return_screen: Box::new(Screen::SubMenu(return_screen)),
                });
            }
        }
    }

    match task {
        ExternalTask::None => {}

        // --- System Rebuild and Activation Actions ---
        ExternalTask::RebuildSwitchOnly => {
            let before_system = std::fs::canonicalize("/run/current-system");
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
                git::commit_changes(&flake_dir, needs_sudo, &msg, ".")?
            } else {
                false
            };

            let before_system = std::fs::canonicalize("/run/current-system");
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
                let before_system = std::fs::canonicalize("/run/current-system");
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
                        git::commit_changes(&flake_dir, needs_sudo, &msg, ".")?
                    } else {
                        false
                    };

                    let before_system = std::fs::canonicalize("/run/current-system");
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
                    let before_system = std::fs::canonicalize("/run/current-system");
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
                    let committed =
                        is_git && git::commit_changes(&flake_dir, needs_sudo, &msg, "flake.lock")?;

                    let before_system = std::fs::canonicalize("/run/current-system");
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
            git::git_checkout_files(&flake_dir, needs_sudo, &hash)?;
            let has_staged = git::commit_changes(&flake_dir, needs_sudo, &msg, ".")?;
            if !has_staged {
                app.screen = Screen::Result(ResultState {
                    is_success: true,
                    title: "NO CHANGES TO REVERT".to_string(),
                    message: format!("Files are already identical to commit {hash}."),
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                });
                return Ok(());
            }

            let before_system = std::fs::canonicalize("/run/current-system");
            match nix::nixos_rebuild_switch(&flake_target, &flake_dir) {
                Ok(()) => {
                    let push_warn = if is_git && has_staged {
                        try_push(&flake_dir, needs_sudo, false)
                    } else {
                        String::new()
                    };

                    app.generation = nix::get_system_generation();
                    let action_desc = format!(
                        "Working tree reverted to {hash}, committed, and system activated{push_warn}"
                    );

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

        ExternalTask::RestoreCommitAndSwitch(hash, paths, msg) => {
            let file = paths.join(", ");
            if !is_git || paths.is_empty() || !git::has_uncommitted_changes(&flake_dir)? {
                app.screen = Screen::Result(ResultState {
                    is_success: true,
                    title: "NOTHING TO COMMIT".to_string(),
                    message: "No restored changes to commit.".to_string(),
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                });
                return Ok(());
            }
            git::git_commit_paths(&flake_dir, needs_sudo, &msg, &paths)?;
            let has_staged = true;

            let before_system = std::fs::canonicalize("/run/current-system");
            match nix::nixos_rebuild_switch(&flake_target, &flake_dir) {
                Ok(()) => {
                    let push_warn = if is_git && has_staged {
                        try_push(&flake_dir, needs_sudo, false)
                    } else {
                        String::new()
                    };

                    app.generation = nix::get_system_generation();
                    let action_desc = format!(
                        "Restored {file} from {hash}, committed, and system activated{push_warn}"
                    );

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

            let before_system = std::fs::canonicalize("/run/current-system");
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
                    let committed =
                        is_git && git::commit_changes(&flake_dir, needs_sudo, &msg, "flake.lock")?;
                    if is_git && !committed {
                        app.screen = Screen::Result(ResultState {
                            is_success: true,
                            title: "FLAKE LOCKFILE UP TO DATE".to_string(),
                            message: "No lockfile changes to commit or push.".to_string(),
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
                            title: "FLAKE LOCKFILE UPDATED".to_string(),
                            message: format!("flake.lock updated and committed{push_warn}"),
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

        ExternalTask::TrimHistoryCommitAndPush(hash, msg) => {
            let r1 = git::trim_history(&flake_dir, needs_sudo, &hash, &msg);
            let r2 = if r1.is_ok() {
                git::git_push(&flake_dir, needs_sudo, true)
            } else {
                r1
            };

            match r2 {
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
                    let has_changes = git::path_has_changes(&flake_dir, &file)?;
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
                                hash,
                                vec![file],
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
                    let paths = git::changed_paths(&flake_dir)?;
                    if !paths.is_empty() {
                        let target_name = file_opt.as_deref().unwrap_or("all files");
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
                                hash, paths,
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

#[cfg(test)]
mod codex_audit {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        let root = std::env::temp_dir()
            .join(format!("flaker_fixture_{}", std::process::id()))
            .join(name);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn git_status_failure_must_not_mean_clean_tree() {
        let dir = fixture("invalid-git-status");
        std::fs::write(dir.join(".git"), "gitdir: missing-git-dir\n").unwrap();
        let result = git::has_uncommitted_changes(&dir);
        assert!(result.is_err(), "expected Git error, received {result:?}");
    }

    #[test]
    fn git_diff_failure_must_not_mean_staged_changes() {
        let dir = fixture("invalid-git-diff");
        std::fs::write(dir.join(".git"), "gitdir: missing-git-dir\n").unwrap();
        let result = git::has_staged_changes(&dir);
        assert!(result.is_err(), "expected Git error, received {result:?}");
    }

    #[test]
    fn failed_closure_comparison_must_propagate() {
        let temp_dir = std::env::temp_dir().join(format!("flaker_audit_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let before = temp_dir.join("non-store-before");
        let after = temp_dir.join("non-store-after");
        let _ = std::fs::create_dir_all(&before);
        let _ = std::fs::create_dir_all(&after);
        assert!(nix::diff_closures(&before, &after).is_err());
        let result = nix::get_closure_diff_between(&before, &after);
        assert!(
            result.is_err(),
            "expected comparison error, received {result:?}"
        );
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn lockfile_root_identifier_must_be_respected() {
        let dir = fixture("renamed-lock-root");
        std::fs::write(
            dir.join("flake.lock"),
            r#"{"version":7,"root":"project","nodes":{"project":{"inputs":{"nixpkgs":"nixpkgs"}},"nixpkgs":{"locked":{"type":"github","owner":"NixOS","repo":"nixpkgs","rev":"abcdef"}}}}"#,
        )
        .unwrap();
        let result = nix::get_flake_inputs(&dir);
        assert!(
            result.is_ok(),
            "valid renamed root was rejected: {result:?}"
        );
        assert_eq!(result.unwrap().first().unwrap().name, "nixpkgs");
    }

    #[test]
    fn braces_in_nix_strings_must_not_end_configuration_block() {
        let configs = actions::parse_flake_configs(
            r#"{
  nixosConfigurations = {
    first = build { message = "}"; };
    second = build {};
  };
}"#,
        );
        assert_eq!(configs, vec!["first", "second"]);
    }

    #[test]
    fn tilde_target_must_be_expanded_before_command_execution() {
        let mut config = config::Config::default();
        config.general.flake_target = "~/#host".to_string();
        let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
        if !home.exists() {
            assert!(actions::detect_flake_context(&config).is_err());
            return;
        }
        let context = actions::detect_flake_context(&config).unwrap();
        assert!(
            !context.flake_target.starts_with('~'),
            "Command receives literal unexpanded target: {}",
            context.flake_target
        );
    }

    #[test]
    fn unicode_input_must_remain_visible_after_horizontal_scrolling() {
        let backend = ratatui::backend::TestBackend::new(30, 15);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let input = tui_input::Input::new("界".repeat(40));
        terminal
            .draw(|frame| {
                ui::input_modal::render_input_modal(
                    frame,
                    frame.area(),
                    "test",
                    &input,
                    "",
                    "",
                    &theme::Theme::default(),
                );
            })
            .unwrap();
        assert!(
            terminal
                .backend()
                .buffer()
                .content
                .iter()
                .any(|cell| cell.symbol().contains('界')),
            "Input vanished: display-cell scroll offset was used as a character index"
        );
    }

    #[test]
    fn long_closure_search_must_not_panic() {
        let result = std::panic::catch_unwind(|| {
            let backend = ratatui::backend::TestBackend::new(80, 30);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            let state = app::ClosureDiffState {
                items: Vec::new(),
                filtered_indices: Vec::new(),
                search_input: tui_input::Input::new("x".repeat(usize::from(u16::MAX))),
                scroll_offset: 0,
                is_identical_closure: false,
                on_confirm_task: None,
                return_screen: Box::new(app::Screen::TopMenu),
                title_suffix: String::new(),
            };
            terminal
                .draw(|frame| {
                    ui::closure_diff::render_closure_diff(
                        frame,
                        frame.area(),
                        &state,
                        &theme::Theme::default(),
                    );
                })
                .unwrap();
        });
        assert!(
            result.is_ok(),
            "Long user input overflowed cursor coordinates"
        );
    }
}
