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
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::{self, stdout};
use std::time::Duration;

fn main() -> Result<()> {
    // Ensure terminal is restored on panic
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen, Show);
        default_hook(info);
    }));

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, Hide)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();

    let res = run_app(&mut terminal, &mut app);

    // Teardown terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, Show)?;
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

        if event::poll(Duration::from_millis(50))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            app.handle_key(key);
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

    match task {
        ExternalTask::None => {}

        ExternalTask::RebuildSwitchOnly => {
            match nix::nixos_rebuild_switch(&flake_target, &flake_dir) {
                Ok(()) => {
                    if is_git {
                        let _ = git::git_push(&flake_dir, needs_sudo, false);
                    }

                    app.screen = Screen::Result(ResultState {
                        is_success: true,
                        title: "SYSTEM REBUILT SUCCESSFULLY".to_string(),
                        message: "System successfully rebuilt and activated".to_string(),
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                    });
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
            let add_res = if is_git {
                git::git_add(&flake_dir, needs_sudo, ".")
            } else {
                Ok(())
            };

            if add_res.is_err()
                || (is_git && git::git_commit(&flake_dir, needs_sudo, &msg).is_err())
            {
                app.screen = Screen::Result(ResultState {
                    is_success: false,
                    title: "GIT COMMIT FAILED".to_string(),
                    message: "Failed to record git commit before rebuilding".to_string(),
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                });
            } else {
                match nix::nixos_rebuild_switch(&flake_target, &flake_dir) {
                    Ok(()) => {
                        if is_git {
                            let _ = git::git_push(&flake_dir, needs_sudo, false);
                        }

                        app.screen = Screen::Result(ResultState {
                            is_success: true,
                            title: "SYSTEM REBUILT SUCCESSFULLY".to_string(),
                            message: "System successfully rebuilt and activated".to_string(),
                            return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                        });
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
        }

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
                if is_git {
                    let _ = git::git_add(&flake_dir, needs_sudo, "flake.lock");
                    let has_staged = git::has_staged_changes(&flake_dir).unwrap_or(false);

                    if has_staged {
                        app.screen = Screen::InputModal(crate::app::InputModalState {
                            action_name: "Updating flake.lock".to_string(),
                            default_text: "flake update".to_string(),
                            input: tui_input::Input::default(),
                            flow: crate::app::InputFlow::Lockfile,
                            return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                        });
                    } else {
                        let _ = git::git_push(&flake_dir, needs_sudo, false);

                        app.screen = Screen::Result(ResultState {
                            is_success: true,
                            title: "FLAKE LOCKFILE UP TO DATE".to_string(),
                            message: "flake.lock is already up to date and pushed".to_string(),
                            return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                        });
                    }
                } else {
                    app.screen = Screen::Result(ResultState {
                        is_success: true,
                        title: "FLAKE LOCKFILE UPDATED".to_string(),
                        message: "flake.lock was successfully updated".to_string(),
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                    });
                }
            }
        },

        ExternalTask::UpdateFlakeAndPush(msg) => {
            if is_git && let Err(err) = git::git_commit(&flake_dir, needs_sudo, &msg) {
                app.screen = Screen::Result(ResultState {
                    is_success: false,
                    title: "GIT COMMIT FAILED".to_string(),
                    message: err.to_string(),
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                });
            } else if is_git {
                match git::git_push(&flake_dir, needs_sudo, false) {
                    Err(err) => {
                        app.screen = Screen::Result(ResultState {
                            is_success: false,
                            title: "GIT PUSH FAILED".to_string(),
                            message: err.to_string(),
                            return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                        });
                    }
                    Ok(()) => {
                        app.screen = Screen::Result(ResultState {
                            is_success: true,
                            title: "FLAKE LOCKFILE UPDATED".to_string(),
                            message: "flake.lock successfully updated and pushed to git"
                                .to_string(),
                            return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                        });
                    }
                }
            } else {
                app.screen = Screen::Result(ResultState {
                    is_success: true,
                    title: "FLAKE LOCKFILE UPDATED".to_string(),
                    message: "flake.lock successfully updated".to_string(),
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                });
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
                    if is_git {
                        let _ = git::git_add(&flake_dir, needs_sudo, ".");

                        if let Err(err) = git::git_commit(&flake_dir, needs_sudo, &msg) {
                            app.screen = Screen::Result(ResultState {
                                is_success: false,
                                title: "GIT COMMIT FAILED".to_string(),
                                message: err.to_string(),
                                return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                            });
                            return Ok(());
                        }
                    }

                    match nix::nixos_rebuild_switch(&flake_target, &flake_dir) {
                        Err(err) => {
                            app.screen = Screen::Result(ResultState {
                                is_success: false,
                                title: "REBUILD FAILED".to_string(),
                                message: err.to_string(),
                                return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                            });
                        }
                        Ok(()) => {
                            if is_git {
                                let _ = git::git_push(&flake_dir, needs_sudo, false);
                            }

                            app.screen = Screen::Result(ResultState {
                                is_success: true,
                                title: "FULL UPDATE CYCLE COMPLETED".to_string(),
                                message:
                                    "System successfully updated to latest package versions and activated"
                                        .to_string(),
                                return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                            });
                        }
                    }
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
            Ok(()) => match nix::nixos_rebuild_switch(&flake_target, &flake_dir) {
                Err(err) => {
                    app.screen = Screen::Result(ResultState {
                        is_success: false,
                        title: "REBUILD FAILED".to_string(),
                        message: err.to_string(),
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                    });
                }
                Ok(()) => {
                    if is_git {
                        let _ = git::git_push(&flake_dir, needs_sudo, false);
                    }

                    app.screen = Screen::Result(ResultState {
                        is_success: true,
                        title: "FULL UPDATE CYCLE COMPLETED".to_string(),
                        message:
                            "System successfully updated to latest package versions and activated"
                                .to_string(),
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                    });
                }
            },
        },

        ExternalTask::TestBuild => match nix::nixos_rebuild_build(&flake_target, &flake_dir) {
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
                    message: "Configuration built successfully (result in ./result)".to_string(),
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                });
            }
        },

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

        ExternalTask::HardReset(hash) => {
            let r1 = git::git_reset_hard(&flake_dir, needs_sudo, &hash);
            let r2 = if r1.is_ok() {
                git::git_push(&flake_dir, needs_sudo, true)
            } else {
                r1
            };
            let r3 = if r2.is_ok() {
                nix::nixos_rebuild_switch(&flake_target, &flake_dir)
            } else {
                r2
            };

            match r3 {
                Err(err) => {
                    app.screen = Screen::Result(ResultState {
                        is_success: false,
                        title: format!("HARD ROLLBACK TO {hash} FAILED"),
                        message: err.to_string(),
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                    });
                }
                Ok(()) => {
                    app.screen = Screen::Result(ResultState {
                        is_success: true,
                        title: "HARD ROLLBACK SUCCESSFUL".to_string(),
                        message: format!("System and git history successfully reverted to {hash}"),
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                    });
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
            let r3 = if r2.is_ok() {
                git::git_commit(&flake_dir, needs_sudo, &msg)
            } else {
                r2
            };

            match r3 {
                Err(err) => {
                    app.screen = Screen::Result(ResultState {
                        is_success: false,
                        title: "SOFT REVERT FAILED".to_string(),
                        message: err.to_string(),
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                    });
                }
                Ok(()) => {
                    let p_res = git::git_push(&flake_dir, needs_sudo, false);
                    let s_res = if p_res.is_ok() {
                        nix::nixos_rebuild_switch(&flake_target, &flake_dir)
                    } else {
                        p_res
                    };

                    match s_res {
                        Err(err) => {
                            app.screen = Screen::Result(ResultState {
                                is_success: false,
                                title: "REBUILD AFTER SOFT REVERT FAILED".to_string(),
                                message: err.to_string(),
                                return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                            });
                        }
                        Ok(()) => {
                            app.screen = Screen::Result(ResultState {
                                is_success: true,
                                title: "SOFT REVERT SUCCESSFUL".to_string(),
                                message: format!(
                                    "Earlier state from {hash} saved as new commit and system rebuilt"
                                ),
                                return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                            });
                        }
                    }
                }
            }
        }

        ExternalTask::TrimHistoryCommitAndPush(hash, msg) => {
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
                        title: "HISTORY TRIM PUSH FAILED".to_string(),
                        message: err.to_string(),
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                    });
                }
                Ok(()) => {
                    app.screen = Screen::Result(ResultState {
                        is_success: true,
                        title: "HISTORY TRIMMED SUCCESSFULLY".to_string(),
                        message: "History trimmed successfully and remote updated".to_string(),
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                    });
                }
            }
        }
    }

    app.refresh_metadata();

    Ok(())
}
