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

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, Hide, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();

    let res = run_app(&mut terminal, &mut app);

    // Teardown terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        Show,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

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

    /// Builds the configuration, generates closure diff against active system, and shows ClosureDiff preview screen.
    fn preview_closure_diff_and_wait(
        app: &mut App,
        flake_target: &str,
        flake_dir: &std::path::Path,
        needs_sudo: bool,
        title_suffix: &str,
        return_screen: Screen,
        on_confirm_task: ExternalTask,
    ) {
        nix::clean_result_link(flake_dir, needs_sudo);
        match nix::nixos_rebuild_build_closure(flake_target, flake_dir, needs_sudo) {
            Err(err) => {
                app.screen = Screen::Result(ResultState {
                    is_success: false,
                    title: "BUILD FAILED".to_string(),
                    message: err.to_string(),
                    return_screen: Box::new(return_screen),
                });
            }
            Ok(closure_path) => {
                let diff_result = match nix::get_system_closure_diff(&closure_path) {
                    Ok(diff) => diff,
                    Err(err) => {
                        nix::clean_result_link(flake_dir, needs_sudo);
                        app.screen = Screen::Result(ResultState {
                            is_success: false,
                            title: "CLOSURE DIFF FAILED".to_string(),
                            message: format!(
                                "Configuration built successfully, but comparing closures failed: {err}"
                            ),
                            return_screen: Box::new(return_screen),
                        });
                        return;
                    }
                };
                let filtered_indices = (0..diff_result.items.len()).collect();
                let task = on_confirm_task.with_closure_path(closure_path);
                app.screen = Screen::ClosureDiff(crate::app::ClosureDiffState {
                    items: diff_result.items,
                    filtered_indices,
                    search_input: tui_input::Input::default(),
                    scroll_offset: 0,
                    is_identical_closure: diff_result.is_identical_closure,
                    on_confirm_task: Some(Box::new(task)),
                    return_screen: Box::new(return_screen),
                    title_suffix: title_suffix.to_string(),
                });
            }
        }
    }

    match task {
        ExternalTask::None => {}

        // --- Build & Preview Actions (Pre-activation Closure Diff) ---
        ExternalTask::RebuildSwitchOnly => {
            preview_closure_diff_and_wait(
                app,
                &flake_target,
                &flake_dir,
                needs_sudo,
                "Rebuild & Switch",
                Screen::SubMenu(SubMenuKind::Updates),
                ExternalTask::ApplySwitchedSystem {
                    closure_path: None,
                    return_screen: SubMenuKind::Updates,
                    success_title: "SYSTEM REBUILT SUCCESSFULLY".to_string(),
                    success_message: "System successfully rebuilt and activated".to_string(),
                },
            );
        }

        ExternalTask::RebuildCommitAndSwitch(msg) => {
            preview_closure_diff_and_wait(
                app,
                &flake_target,
                &flake_dir,
                needs_sudo,
                "Rebuild & Commit",
                Screen::SubMenu(SubMenuKind::Updates),
                ExternalTask::ApplyCommitAndSwitch {
                    closure_path: None,
                    msg,
                    return_screen: SubMenuKind::Updates,
                    success_title: "SYSTEM REBUILT SUCCESSFULLY".to_string(),
                    success_message: "System successfully rebuilt, committed, and activated"
                        .to_string(),
                },
            );
        }

        ExternalTask::FullCycleSwitchOnly => {
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
                    preview_closure_diff_and_wait(
                        app,
                        &flake_target,
                        &flake_dir,
                        needs_sudo,
                        "Full Cycle Update",
                        Screen::SubMenu(SubMenuKind::Updates),
                        ExternalTask::ApplySwitchedSystem {
                            closure_path: None,
                            return_screen: SubMenuKind::Updates,
                            success_title: "FULL UPDATE CYCLE COMPLETED".to_string(),
                            success_message: "System successfully updated to latest package versions and activated".to_string(),
                        },
                    );
                }
            }
        }

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
                    preview_closure_diff_and_wait(
                        app,
                        &flake_target,
                        &flake_dir,
                        needs_sudo,
                        "Full Cycle Update",
                        Screen::SubMenu(SubMenuKind::Updates),
                        ExternalTask::ApplyCommitAndSwitch {
                            closure_path: None,
                            msg,
                            return_screen: SubMenuKind::Updates,
                            success_title: "FULL UPDATE CYCLE COMPLETED".to_string(),
                            success_message: "System successfully updated to latest package versions, committed, and activated".to_string(),
                        },
                    );
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
                    preview_closure_diff_and_wait(
                        app,
                        &flake_target,
                        &flake_dir,
                        needs_sudo,
                        "Selective Update",
                        Screen::SubMenu(SubMenuKind::SelectiveUpdate),
                        ExternalTask::ApplySelectiveFullCycle {
                            closure_path: None,
                            inputs,
                            msg: None,
                        },
                    );
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
                    preview_closure_diff_and_wait(
                        app,
                        &flake_target,
                        &flake_dir,
                        needs_sudo,
                        "Selective Update",
                        Screen::SubMenu(SubMenuKind::SelectiveUpdate),
                        ExternalTask::ApplySelectiveFullCycle {
                            closure_path: None,
                            inputs,
                            msg: Some(msg),
                        },
                    );
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

            preview_closure_diff_and_wait(
                app,
                &flake_target,
                &flake_dir,
                needs_sudo,
                &format!("Soft Revert to {hash}"),
                Screen::SubMenu(SubMenuKind::GitHistory),
                ExternalTask::ApplySoftRevertSwitch {
                    closure_path: None,
                    hash,
                    msg,
                },
            );
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

            preview_closure_diff_and_wait(
                app,
                &flake_target,
                &flake_dir,
                needs_sudo,
                &format!("Restore {file} from {hash}"),
                Screen::SubMenu(SubMenuKind::GitHistory),
                ExternalTask::ApplyRestoreFileSwitch {
                    closure_path: None,
                    hash,
                    file,
                    msg,
                },
            );
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

            preview_closure_diff_and_wait(
                app,
                &flake_target,
                &flake_dir,
                needs_sudo,
                &format!("Hard Reset to {hash}"),
                Screen::SubMenu(SubMenuKind::GitHistory),
                ExternalTask::ApplyHardResetSwitch {
                    closure_path: None,
                    hash,
                },
            );
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

        // --- Activation Tasks (Invoked after user confirms on ClosureDiff screen) ---
        ExternalTask::ApplySwitchedSystem {
            closure_path,
            return_screen,
            success_title,
            success_message,
        } => {
            let res = nix::switch_system(closure_path.as_deref(), &flake_target, &flake_dir);
            nix::clean_result_link(&flake_dir, needs_sudo);
            match res {
                Ok(()) => {
                    let push_warn = if is_git {
                        try_push(&flake_dir, needs_sudo, false)
                    } else {
                        String::new()
                    };

                    app.screen = Screen::Result(ResultState {
                        is_success: true,
                        title: success_title,
                        message: format!("{success_message}{push_warn}"),
                        return_screen: Box::new(Screen::SubMenu(return_screen)),
                    });
                }
                Err(err) => {
                    app.screen = Screen::Result(ResultState {
                        is_success: false,
                        title: "ACTIVATION FAILED".to_string(),
                        message: err.to_string(),
                        return_screen: Box::new(Screen::SubMenu(return_screen)),
                    });
                }
            }
        }

        ExternalTask::ApplyCommitAndSwitch {
            closure_path,
            msg,
            return_screen,
            success_title,
            success_message,
        } => {
            let has_staged = if is_git {
                git::git_add(&flake_dir, needs_sudo, ".").is_ok()
                    && git::has_staged_changes(&flake_dir).unwrap_or(false)
            } else {
                false
            };

            if has_staged && git::git_commit(&flake_dir, needs_sudo, &msg).is_err() {
                nix::clean_result_link(&flake_dir, needs_sudo);
                app.screen = Screen::Result(ResultState {
                    is_success: false,
                    title: "GIT COMMIT FAILED".to_string(),
                    message: "Failed to record git commit before activating".to_string(),
                    return_screen: Box::new(Screen::SubMenu(return_screen)),
                });
                return Ok(());
            }

            let res = nix::switch_system(closure_path.as_deref(), &flake_target, &flake_dir);
            nix::clean_result_link(&flake_dir, needs_sudo);
            match res {
                Ok(()) => {
                    let push_warn = if is_git && has_staged {
                        try_push(&flake_dir, needs_sudo, false)
                    } else {
                        String::new()
                    };

                    let action_desc = if is_git && has_staged {
                        format!("{success_message}{push_warn}")
                    } else {
                        "System successfully rebuilt and activated (working tree had no changes to commit)".to_string()
                    };

                    app.screen = Screen::Result(ResultState {
                        is_success: true,
                        title: success_title,
                        message: action_desc,
                        return_screen: Box::new(Screen::SubMenu(return_screen)),
                    });
                }
                Err(err) => {
                    app.screen = Screen::Result(ResultState {
                        is_success: false,
                        title: "ACTIVATION FAILED".to_string(),
                        message: err.to_string(),
                        return_screen: Box::new(Screen::SubMenu(return_screen)),
                    });
                }
            }
        }

        ExternalTask::ApplySelectiveFullCycle {
            closure_path,
            inputs,
            msg,
        } => {
            let mut committed = false;
            if let Some(msg) = msg
                && is_git
            {
                let _ = git::git_add(&flake_dir, needs_sudo, "flake.lock");
                if git::has_staged_changes(&flake_dir).unwrap_or(false) {
                    if let Err(err) = git::git_commit(&flake_dir, needs_sudo, &msg) {
                        nix::clean_result_link(&flake_dir, needs_sudo);
                        app.screen = Screen::Result(ResultState {
                            is_success: false,
                            title: "GIT COMMIT FAILED".to_string(),
                            message: err.to_string(),
                            return_screen: Box::new(Screen::SubMenu(SubMenuKind::SelectiveUpdate)),
                        });
                        return Ok(());
                    }
                    committed = true;
                }
            }

            let res = nix::switch_system(closure_path.as_deref(), &flake_target, &flake_dir);
            nix::clean_result_link(&flake_dir, needs_sudo);
            match res {
                Err(err) => {
                    app.screen = Screen::Result(ResultState {
                        is_success: false,
                        title: "ACTIVATION FAILED".to_string(),
                        message: err.to_string(),
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::SelectiveUpdate)),
                    });
                }
                Ok(()) => {
                    let push_warn = if is_git && committed {
                        try_push(&flake_dir, needs_sudo, false)
                    } else {
                        String::new()
                    };

                    app.screen = Screen::Result(ResultState {
                        is_success: true,
                        title: "SELECTIVE FULL CYCLE COMPLETED".to_string(),
                        message: format!(
                            "Selected input(s) ({}) updated, system activated{push_warn}",
                            inputs.join(", ")
                        ),
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::SelectiveUpdate)),
                    });
                }
            }
        }

        ExternalTask::ApplySoftRevertSwitch {
            closure_path,
            hash,
            msg,
        } => {
            let has_staged = if is_git {
                git::has_staged_changes(&flake_dir).unwrap_or(false)
            } else {
                false
            };

            if has_staged && git::git_commit(&flake_dir, needs_sudo, &msg).is_err() {
                nix::clean_result_link(&flake_dir, needs_sudo);
                app.screen = Screen::Result(ResultState {
                    is_success: false,
                    title: "GIT COMMIT FAILED".to_string(),
                    message: "Failed to record soft revert commit".to_string(),
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                });
                return Ok(());
            }

            let res = nix::switch_system(closure_path.as_deref(), &flake_target, &flake_dir);
            nix::clean_result_link(&flake_dir, needs_sudo);
            match res {
                Err(err) => {
                    app.screen = Screen::Result(ResultState {
                        is_success: false,
                        title: "REBUILD AFTER SOFT REVERT FAILED".to_string(),
                        message: err.to_string(),
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                    });
                }
                Ok(()) => {
                    let push_warn = if is_git && has_staged {
                        try_push(&flake_dir, needs_sudo, false)
                    } else {
                        String::new()
                    };

                    let action_desc = if is_git && has_staged {
                        format!(
                            "Working tree reverted to {hash}, committed, and system activated{push_warn}"
                        )
                    } else {
                        format!("Working tree was already at {hash}, system activated")
                    };

                    app.screen = Screen::Result(ResultState {
                        is_success: true,
                        title: "SOFT REVERT SUCCESSFUL".to_string(),
                        message: action_desc,
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                    });
                }
            }
        }

        ExternalTask::ApplyRestoreFileSwitch {
            closure_path,
            hash,
            file,
            msg,
        } => {
            let has_staged = if is_git {
                git::git_add(&flake_dir, needs_sudo, ".").is_ok()
                    && git::has_staged_changes(&flake_dir).unwrap_or(false)
            } else {
                false
            };

            if has_staged && git::git_commit(&flake_dir, needs_sudo, &msg).is_err() {
                nix::clean_result_link(&flake_dir, needs_sudo);
                app.screen = Screen::Result(ResultState {
                    is_success: false,
                    title: "GIT COMMIT FAILED".to_string(),
                    message: format!("Failed to record commit restoring {file} from {hash}"),
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                });
                return Ok(());
            }

            let res = nix::switch_system(closure_path.as_deref(), &flake_target, &flake_dir);
            nix::clean_result_link(&flake_dir, needs_sudo);
            match res {
                Ok(()) => {
                    let push_warn = if is_git && has_staged {
                        try_push(&flake_dir, needs_sudo, false)
                    } else {
                        String::new()
                    };

                    let action_desc = if is_git && has_staged {
                        format!(
                            "Restored {file} from {hash}, committed, and system activated{push_warn}"
                        )
                    } else {
                        format!("File {file} was already identical to {hash}, system activated")
                    };

                    app.screen = Screen::Result(ResultState {
                        is_success: true,
                        title: "RESTORE & REBUILD SUCCESSFUL".to_string(),
                        message: action_desc,
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                    });
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

        ExternalTask::ApplyHardResetSwitch { closure_path, hash } => {
            let res = nix::switch_system(closure_path.as_deref(), &flake_target, &flake_dir);
            nix::clean_result_link(&flake_dir, needs_sudo);
            match res {
                Err(err) => {
                    app.screen = Screen::Result(ResultState {
                        is_success: false,
                        title: format!("HARD ROLLBACK TO {hash} FAILED"),
                        message: err.to_string(),
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                    });
                }
                Ok(()) => {
                    let push_warn = if is_git {
                        try_push(&flake_dir, needs_sudo, true)
                    } else {
                        String::new()
                    };

                    app.screen = Screen::Result(ResultState {
                        is_success: true,
                        title: "HARD ROLLBACK SUCCESSFUL".to_string(),
                        message: format!(
                            "System and git history successfully reverted to {hash}{push_warn}"
                        ),
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                    });
                }
            }
        }
    }

    app.refresh_metadata();

    Ok(())
}
