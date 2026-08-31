use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    widgets::Clear,
};
use std::path::PathBuf;
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;

use crate::actions::{detect_flake_context, git, nix};
use crate::theme;
use crate::ui::{
    confirm::{ConfirmParams, render_confirm},
    filter::render_filter,
    header::{centered_rect, render_header},
    input_modal::render_input_modal,
    menu::render_menu,
    pager::render_pager,
    result::render_result,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubMenuKind {
    Updates,
    Maintenance,
    GitHistory,
}

#[derive(Debug, Clone)]
pub enum FilterFlow {
    HardReset,
    SoftRevert,
    TrimHistory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputFlow {
    Rebuild,
    Lockfile,
    FullCycle,
    SoftRevert(String),
    TrimHistory(String),
}

#[derive(Debug, Clone)]
pub enum PendingAction {
    CleanStore,
    HardResetExecute(String),
    TrimHistorySoftReset(String),
}

#[derive(Debug, Clone)]
pub struct ConfirmState {
    pub title: String,
    pub lines: Vec<String>,
    pub affirmative_label: String,
    pub negative_label: String,
    pub selected_button: usize, // 0: aff, 1: neg
    pub is_danger: bool,
    pub on_confirm: PendingAction,
    pub return_screen: Box<Screen>,
}

#[derive(Debug, Clone)]
pub struct InputModalState {
    pub action_name: String,
    pub default_text: String,
    pub input: Input,
    pub flow: InputFlow,
    pub return_screen: Box<Screen>,
}

#[derive(Debug, Clone)]
pub struct CommitFilterState {
    pub header_title: String,
    pub flow: FilterFlow,
    pub commits: Vec<String>,
    pub input: Input,
    pub selected_index: usize,
    pub return_screen: Box<Screen>,
}

#[derive(Debug, Clone)]
pub struct PagerState {
    pub title: String,
    pub lines: Vec<String>,
    pub scroll_offset: usize,
    pub return_screen: Box<Screen>,
}

#[derive(Debug, Clone)]
pub struct ResultState {
    pub is_success: bool,
    pub title: String,
    pub message: String,
    pub return_screen: Box<Screen>,
}

#[derive(Debug, Clone)]
pub enum Screen {
    TopMenu,
    SubMenu(SubMenuKind),
    Confirm(ConfirmState),
    InputModal(InputModalState),
    CommitFilter(CommitFilterState),
    Pager(PagerState),
    Result(ResultState),
}

/// Request to run a command outside the raw alternate screen.
#[derive(Debug, Clone)]
pub enum ExternalTask {
    None,
    RebuildSwitchOnly,
    RebuildCommitAndSwitch(String),
    UpdateFlakeOnly,
    UpdateFlakeAndPush(String),
    FullCycleCommitAndSwitch(String),
    FullCycleSwitchOnly,
    TestBuild,
    CleanStore,
    HardReset(String),
    SoftRevertCommitAndSwitch(String, String),
    TrimHistoryCommitAndPush(String, String),
}

pub struct App {
    pub should_quit: bool,
    pub screen: Screen,
    pub top_menu_index: usize,
    pub submenu_index: usize,
    pub host: String,
    pub user: String,
    pub generation: String,
    pub flake_dir: PathBuf,
    pub flake_target: String,
    pub is_git: bool,
    pub needs_sudo: bool,
    pub pending_external_task: ExternalTask,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        let host = whoami::fallible::hostname().unwrap_or_else(|_| "nixos".to_string());
        let user = whoami::fallible::username().unwrap_or_else(|_| "user".to_string());
        let generation = nix::get_system_generation();
        let ctx = detect_flake_context();

        Self {
            should_quit: false,
            screen: Screen::TopMenu,
            top_menu_index: 0,
            submenu_index: 0,
            host,
            user,
            generation,
            flake_dir: ctx.flake_dir,
            flake_target: ctx.flake_target,
            is_git: ctx.is_git,
            needs_sudo: ctx.needs_sudo,
            pending_external_task: ExternalTask::None,
        }
    }

    pub fn refresh_metadata(&mut self) {
        self.generation = nix::get_system_generation();
        let ctx = detect_flake_context();
        self.flake_dir = ctx.flake_dir;
        self.flake_target = ctx.flake_target;
        self.is_git = ctx.is_git;
        self.needs_sudo = ctx.needs_sudo;
    }

    pub fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();
        frame.render_widget(Clear, area);

        match &self.screen {
            Screen::TopMenu => {
                let chunks = Layout::vertical([
                    Constraint::Length(7), // Header box
                    Constraint::Length(1), // Spacer
                    Constraint::Length(7), // Menu items (4 items + borders)
                ])
                .split(centered_rect(64, 15, area));

                render_header(
                    frame,
                    chunks[0],
                    &self.host,
                    &self.user,
                    &self.generation,
                    &self.flake_target,
                );

                let item0 = format!("{}  Updates", theme::ICON_REBUILD);
                let item1 = format!("{}  Maintenance", theme::ICON_CLEANUP);
                let item2 = if self.is_git {
                    format!("{}  Git & History", theme::ICON_GIT)
                } else {
                    format!("{}  Git & History (No Git)", theme::ICON_GIT)
                };
                let item3 = format!("{}  Exit", theme::ICON_EXIT);
                let items = [
                    item0.as_str(),
                    item1.as_str(),
                    item2.as_str(),
                    item3.as_str(),
                ];

                render_menu(
                    frame,
                    chunks[2],
                    "Select an action",
                    &items,
                    self.top_menu_index,
                );
            }

            Screen::SubMenu(kind) => {
                let (title, items) = match kind {
                    SubMenuKind::Updates => (
                        "Updates",
                        vec![
                            format!("{}  Rebuild System (Rebuild & Switch)", theme::ICON_REBUILD),
                            format!("{}  Update Lockfile (Flake Update)", theme::ICON_PACKAGE),
                            format!("{}  Full Cycle (Update + Switch)", theme::ICON_FULL_CYCLE),
                            format!("{}  Test Build (Dry Run / Build)", theme::ICON_TEST_BUILD),
                            format!("{}  Back", theme::ICON_BACK),
                        ],
                    ),
                    SubMenuKind::Maintenance => (
                        "Maintenance",
                        vec![
                            format!("{}  Clean Garbage & Optimize Store", theme::ICON_CLEANUP),
                            format!("{}  System Generations History", theme::ICON_HISTORY),
                            format!("{}  Back", theme::ICON_BACK),
                        ],
                    ),
                    SubMenuKind::GitHistory => (
                        "Git & History",
                        vec![
                            format!("{}  Show Working Changes (Git Diff)", theme::ICON_DIFF),
                            format!(
                                "{}  Rollback (Hard Reset: git reset --hard)",
                                theme::ICON_HARD_RESET
                            ),
                            format!(
                                "{}  Rollback (Soft Revert: git checkout -- .)",
                                theme::ICON_SOFT_REVERT
                            ),
                            format!("{}  Trim History (git reset --soft)", theme::ICON_TRIM),
                            format!("{}  Back", theme::ICON_BACK),
                        ],
                    ),
                };

                let item_refs: Vec<&str> = items.iter().map(String::as_str).collect();
                let menu_height = (items.len() as u16) + 2;
                let centered = centered_rect(64, menu_height, area);
                render_menu(frame, centered, title, &item_refs, self.submenu_index);
            }

            Screen::Confirm(state) => {
                let lines_ref: Vec<&str> = state.lines.iter().map(String::as_str).collect();
                let params = ConfirmParams {
                    title: &state.title,
                    lines: &lines_ref,
                    affirmative_label: &state.affirmative_label,
                    negative_label: &state.negative_label,
                    selected_button: state.selected_button,
                    is_danger: state.is_danger,
                };
                render_confirm(frame, area, &params);
            }

            Screen::InputModal(state) => {
                render_input_modal(
                    frame,
                    area,
                    &state.action_name,
                    &state.input,
                    &state.default_text,
                );
            }

            Screen::CommitFilter(state) => {
                let filtered = filter_commits(&state.commits, state.input.value());
                let filtered_refs: Vec<(&str, Vec<usize>)> = filtered
                    .iter()
                    .map(|(s, indices)| (s.as_str(), indices.clone()))
                    .collect();

                render_filter(
                    frame,
                    area,
                    &state.header_title,
                    &state.input,
                    &filtered_refs,
                    state.selected_index,
                );
            }

            Screen::Pager(state) => {
                render_pager(frame, area, &state.title, &state.lines, state.scroll_offset);
            }

            Screen::Result(state) => {
                render_result(frame, area, state.is_success, &state.title, &state.message);
            }
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match self.screen.clone() {
            Screen::TopMenu => self.handle_top_menu_key(key),
            Screen::SubMenu(kind) => self.handle_sub_menu_key(kind, key),
            Screen::Confirm(_) => self.handle_confirm_key(key),
            Screen::InputModal(_) => self.handle_input_modal_key(key),
            Screen::CommitFilter(_) => self.handle_commit_filter_key(key),
            Screen::Pager(_) => self.handle_pager_key(key),
            Screen::Result(_) => self.handle_result_key(key),
        }
    }

    fn handle_top_menu_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.top_menu_index > 0 {
                    self.top_menu_index -= 1;
                } else {
                    self.top_menu_index = 3;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.top_menu_index < 3 {
                    self.top_menu_index += 1;
                } else {
                    self.top_menu_index = 0;
                }
            }
            KeyCode::Enter => match self.top_menu_index {
                0 => {
                    self.submenu_index = 0;
                    self.screen = Screen::SubMenu(SubMenuKind::Updates);
                }
                1 => {
                    self.submenu_index = 0;
                    self.screen = Screen::SubMenu(SubMenuKind::Maintenance);
                }
                2 => {
                    if !self.is_git {
                        self.screen = Screen::Result(ResultState {
                            is_success: false,
                            title: "GIT NOT DETECTED".to_string(),
                            message: format!(
                                "No Git repository found in {}.\nGit history and rollback operations are unavailable.",
                                self.flake_dir.display()
                            ),
                            return_screen: Box::new(Screen::TopMenu),
                        });
                    } else {
                        self.submenu_index = 0;
                        self.screen = Screen::SubMenu(SubMenuKind::GitHistory);
                    }
                }
                _ => {
                    self.should_quit = true;
                }
            },
            KeyCode::Esc | KeyCode::Char('q') => {
                self.should_quit = true;
            }
            _ => {}
        }
    }

    fn handle_sub_menu_key(&mut self, kind: SubMenuKind, key: KeyEvent) {
        let count = match kind {
            SubMenuKind::Updates => 5,
            SubMenuKind::Maintenance => 3,
            SubMenuKind::GitHistory => 5,
        };

        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.submenu_index > 0 {
                    self.submenu_index -= 1;
                } else {
                    self.submenu_index = count - 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.submenu_index + 1 < count {
                    self.submenu_index += 1;
                } else {
                    self.submenu_index = 0;
                }
            }
            KeyCode::Esc => {
                self.screen = Screen::TopMenu;
            }
            KeyCode::Enter => {
                self.trigger_sub_menu_item(kind, self.submenu_index);
            }
            _ => {}
        }
    }

    fn trigger_sub_menu_item(&mut self, kind: SubMenuKind, index: usize) {
        match kind {
            SubMenuKind::Updates => match index {
                0 => self.start_rebuild_system(),
                1 => self.start_update_lockfile(),
                2 => self.start_full_cycle(),
                3 => self.start_test_build(),
                _ => self.screen = Screen::TopMenu,
            },
            SubMenuKind::Maintenance => match index {
                0 => self.start_clean_store(),
                1 => self.start_generations_history(),
                _ => self.screen = Screen::TopMenu,
            },
            SubMenuKind::GitHistory => match index {
                0 => self.start_show_git_diff(),
                1 => self.start_hard_reset_flow(),
                2 => self.start_soft_revert_flow(),
                3 => self.start_trim_history_flow(),
                _ => self.screen = Screen::TopMenu,
            },
        }
    }

    fn start_rebuild_system(&mut self) {
        if self.is_git {
            let has_changes = git::has_uncommitted_changes(&self.flake_dir).unwrap_or(false);

            if has_changes {
                self.screen = Screen::InputModal(InputModalState {
                    action_name: "Rebuilding configuration".to_string(),
                    default_text: "rebuild".to_string(),
                    input: Input::default(),
                    flow: InputFlow::Rebuild,
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
                });
            } else {
                self.pending_external_task = ExternalTask::RebuildSwitchOnly;
            }
        } else {
            self.pending_external_task = ExternalTask::RebuildSwitchOnly;
        }
    }

    fn start_update_lockfile(&mut self) {
        self.pending_external_task = ExternalTask::UpdateFlakeOnly;
    }

    fn start_full_cycle(&mut self) {
        if self.is_git {
            self.screen = Screen::InputModal(InputModalState {
                action_name: "Full update cycle".to_string(),
                default_text: "full update".to_string(),
                input: Input::default(),
                flow: InputFlow::FullCycle,
                return_screen: Box::new(Screen::SubMenu(SubMenuKind::Updates)),
            });
        } else {
            self.pending_external_task = ExternalTask::FullCycleSwitchOnly;
        }
    }

    fn start_test_build(&mut self) {
        self.pending_external_task = ExternalTask::TestBuild;
    }

    fn start_clean_store(&mut self) {
        self.screen = Screen::Confirm(ConfirmState {
            title: format!(
                "{}  GARBAGE COLLECTION & STORE OPTIMIZATION",
                theme::ICON_CLEANUP
            ),
            lines: vec![
                "All old system generations except current will be deleted,".to_string(),
                "and duplicate files in /nix/store will be hardlinked.".to_string(),
            ],
            affirmative_label: "Start Cleanup".to_string(),
            negative_label: "Cancel".to_string(),
            selected_button: 0,
            is_danger: false,
            on_confirm: PendingAction::CleanStore,
            return_screen: Box::new(Screen::SubMenu(SubMenuKind::Maintenance)),
        });
    }

    fn start_generations_history(&mut self) {
        match nix::nixos_list_generations(&self.flake_dir) {
            Ok(output) => {
                let lines: Vec<String> = output.lines().map(String::from).collect();
                self.screen = Screen::Pager(PagerState {
                    title: format!("{}  NIXOS GENERATION HISTORY", theme::ICON_HISTORY),
                    lines,
                    scroll_offset: 0,
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::Maintenance)),
                });
            }
            Err(err) => {
                self.screen = Screen::Result(ResultState {
                    is_success: false,
                    title: "FAILED TO RETRIEVE GENERATIONS".to_string(),
                    message: err.to_string(),
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::Maintenance)),
                });
            }
        }
    }

    fn start_show_git_diff(&mut self) {
        match git::get_diff(&self.flake_dir) {
            Ok(diff) if diff.trim().is_empty() => {
                self.screen = Screen::Result(ResultState {
                    is_success: true,
                    title: "NO UNCOMMITTED CHANGES".to_string(),
                    message: "Working directory is clean.".to_string(),
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                });
            }
            Ok(diff) => {
                let lines: Vec<String> = diff.lines().map(String::from).collect();
                self.screen = Screen::Pager(PagerState {
                    title: format!("{}  VIEWING CHANGES (git diff)", theme::ICON_DIFF),
                    lines,
                    scroll_offset: 0,
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                });
            }
            Err(err) => {
                self.screen = Screen::Result(ResultState {
                    is_success: false,
                    title: "FAILED TO GET GIT DIFF".to_string(),
                    message: err.to_string(),
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                });
            }
        }
    }

    fn start_hard_reset_flow(&mut self) {
        match git::get_recent_commits(&self.flake_dir) {
            Ok(commits) if !commits.is_empty() => {
                self.screen = Screen::CommitFilter(CommitFilterState {
                    header_title:
                        "Select target commit → it will become HEAD, newer commits discarded"
                            .to_string(),
                    flow: FilterFlow::HardReset,
                    commits,
                    input: Input::default(),
                    selected_index: 0,
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                });
            }
            Ok(_) => {
                self.screen = Screen::Result(ResultState {
                    is_success: false,
                    title: "COMMIT HISTORY EMPTY".to_string(),
                    message: "No commits found in git log.".to_string(),
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                });
            }
            Err(err) => {
                self.screen = Screen::Result(ResultState {
                    is_success: false,
                    title: "FAILED TO READ GIT LOG".to_string(),
                    message: err.to_string(),
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                });
            }
        }
    }

    fn start_soft_revert_flow(&mut self) {
        match git::get_recent_commits(&self.flake_dir) {
            Ok(commits) if !commits.is_empty() => {
                self.screen = Screen::CommitFilter(CommitFilterState {
                    header_title:
                        "Select target commit → its state will be committed on top of history"
                            .to_string(),
                    flow: FilterFlow::SoftRevert,
                    commits,
                    input: Input::default(),
                    selected_index: 0,
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                });
            }
            Ok(_) => {
                self.screen = Screen::Result(ResultState {
                    is_success: false,
                    title: "COMMIT HISTORY EMPTY".to_string(),
                    message: "No commits found in git log.".to_string(),
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                });
            }
            Err(err) => {
                self.screen = Screen::Result(ResultState {
                    is_success: false,
                    title: "FAILED TO READ GIT LOG".to_string(),
                    message: err.to_string(),
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                });
            }
        }
    }

    fn start_trim_history_flow(&mut self) {
        match git::get_recent_commits(&self.flake_dir) {
            Ok(commits) if !commits.is_empty() => {
                self.screen = Screen::CommitFilter(CommitFilterState {
                    header_title:
                        "History will collapse to target commit while keeping current files intact"
                            .to_string(),
                    flow: FilterFlow::TrimHistory,
                    commits,
                    input: Input::default(),
                    selected_index: 0,
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                });
            }
            Ok(_) => {
                self.screen = Screen::Result(ResultState {
                    is_success: false,
                    title: "COMMIT HISTORY EMPTY".to_string(),
                    message: "No commits found in git log.".to_string(),
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                });
            }
            Err(err) => {
                self.screen = Screen::Result(ResultState {
                    is_success: false,
                    title: "FAILED TO READ GIT LOG".to_string(),
                    message: err.to_string(),
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                });
            }
        }
    }

    fn handle_confirm_key(&mut self, key: KeyEvent) {
        let (on_confirm, return_screen) = if let Screen::Confirm(ref mut state) = self.screen {
            match key.code {
                KeyCode::Left
                | KeyCode::Right
                | KeyCode::Tab
                | KeyCode::Char('h')
                | KeyCode::Char('l') => {
                    state.selected_button = 1 - state.selected_button;
                    return;
                }
                KeyCode::Esc => {
                    self.screen = *state.return_screen.clone();
                    return;
                }
                KeyCode::Enter => {
                    if state.selected_button == 1 {
                        // Cancelled
                        self.screen = *state.return_screen.clone();
                        return;
                    }
                    (state.on_confirm.clone(), state.return_screen.clone())
                }
                _ => return,
            }
        } else {
            return;
        };

        match on_confirm {
            PendingAction::CleanStore => {
                self.pending_external_task = ExternalTask::CleanStore;
            }
            PendingAction::HardResetExecute(hash) => {
                self.pending_external_task = ExternalTask::HardReset(hash);
            }
            PendingAction::TrimHistorySoftReset(hash) => {
                self.screen = Screen::InputModal(InputModalState {
                    action_name: format!("Trim commit history to {hash}"),
                    default_text: "trim history".to_string(),
                    input: Input::default(),
                    flow: InputFlow::TrimHistory(hash),
                    return_screen,
                });
            }
        }
    }

    fn handle_input_modal_key(&mut self, key: KeyEvent) {
        if let Screen::InputModal(ref mut state) = self.screen {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match key.code {
                    KeyCode::Char('u') => {
                        state.input.reset();
                        return;
                    }
                    KeyCode::Char('c') => {
                        let ret = *state.return_screen.clone();
                        self.screen = ret;
                        return;
                    }
                    _ => {}
                }
            }

            match key.code {
                KeyCode::Esc => {
                    let ret = *state.return_screen.clone();
                    self.screen = ret;
                }
                KeyCode::Enter => {
                    let msg = if state.input.value().trim().is_empty() {
                        state.default_text.clone()
                    } else {
                        state.input.value().trim().to_string()
                    };
                    let flow = state.flow.clone();
                    self.submit_input(flow, msg);
                }
                _ => {
                    state.input.handle_event(&Event::Key(key));
                }
            }
        }
    }

    fn submit_input(&mut self, flow: InputFlow, msg: String) {
        match flow {
            InputFlow::Rebuild => {
                self.pending_external_task = ExternalTask::RebuildCommitAndSwitch(msg);
            }
            InputFlow::Lockfile => {
                self.pending_external_task = ExternalTask::UpdateFlakeAndPush(msg);
            }
            InputFlow::FullCycle => {
                self.pending_external_task = ExternalTask::FullCycleCommitAndSwitch(msg);
            }
            InputFlow::SoftRevert(hash) => {
                self.pending_external_task = ExternalTask::SoftRevertCommitAndSwitch(hash, msg);
            }
            InputFlow::TrimHistory(hash) => {
                self.pending_external_task = ExternalTask::TrimHistoryCommitAndPush(hash, msg);
            }
        }
    }

    fn handle_commit_filter_key(&mut self, key: KeyEvent) {
        let (action_to_take, filtered_count) =
            if let Screen::CommitFilter(ref mut state) = self.screen {
                let filtered = filter_commits(&state.commits, state.input.value());
                let count = filtered.len();

                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    match key.code {
                        KeyCode::Char('u') => {
                            state.input.reset();
                            state.selected_index = 0;
                            return;
                        }
                        KeyCode::Char('c') => {
                            let ret = *state.return_screen.clone();
                            self.screen = ret;
                            return;
                        }
                        _ => {}
                    }
                }

                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        if state.selected_index > 0 {
                            state.selected_index -= 1;
                        } else if count > 0 {
                            state.selected_index = count - 1;
                        }
                        (None, count)
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if count > 0 && state.selected_index + 1 < count {
                            state.selected_index += 1;
                        } else {
                            state.selected_index = 0;
                        }
                        (None, count)
                    }
                    KeyCode::Esc => {
                        let ret = *state.return_screen.clone();
                        self.screen = ret;
                        return;
                    }
                    KeyCode::Enter => {
                        if count == 0 {
                            (None, 0)
                        } else {
                            let (ref commit_str, _) = filtered[state.selected_index];
                            let hash = commit_str
                                .split([' ', '│'])
                                .next()
                                .unwrap_or("")
                                .trim()
                                .to_string();
                            (
                                Some((
                                    state.flow.clone(),
                                    hash,
                                    Box::new(*state.return_screen.clone()),
                                )),
                                count,
                            )
                        }
                    }
                    _ => {
                        state.input.handle_event(&Event::Key(key));
                        state.selected_index = 0;
                        (None, count)
                    }
                }
            } else {
                return;
            };

        if let Some((flow, hash, ret)) = action_to_take {
            if !hash.is_empty() {
                self.submit_commit_filter(flow, hash, ret);
            }
        } else if filtered_count == 0 && key.code == KeyCode::Enter {
            // No action on enter when empty
        }
    }

    fn submit_commit_filter(
        &mut self,
        flow: FilterFlow,
        selected_hash: String,
        return_screen: Box<Screen>,
    ) {
        match flow {
            FilterFlow::HardReset => {
                self.screen = Screen::Confirm(ConfirmState {
                    title: format!(
                        "{}  WARNING: HARD ROLLBACK (GIT RESET --HARD)",
                        theme::ICON_WARNING
                    ),
                    lines: vec![
                        format!("Target commit: {selected_hash}"),
                        format!(
                            "All changes and commits after {selected_hash} will be PERMANENTLY LOST!"
                        ),
                    ],
                    affirmative_label: "Yes, Hard Reset".to_string(),
                    negative_label: "Cancel".to_string(),
                    selected_button: 0,
                    is_danger: true,
                    on_confirm: PendingAction::HardResetExecute(selected_hash),
                    return_screen,
                });
            }
            FilterFlow::SoftRevert => {
                self.screen = Screen::InputModal(InputModalState {
                    action_name: format!("Soft revert files to {selected_hash}"),
                    default_text: format!("revert to {selected_hash}"),
                    input: Input::default(),
                    flow: InputFlow::SoftRevert(selected_hash),
                    return_screen,
                });
            }
            FilterFlow::TrimHistory => {
                self.screen = Screen::Confirm(ConfirmState {
                    title: format!("{}  TRIM HISTORY (GIT RESET --SOFT)", theme::ICON_TRIM),
                    lines: vec![
                        format!("Target commit: {selected_hash}"),
                        "Disk files remain untouched; commits after target will be squashed."
                            .to_string(),
                    ],
                    affirmative_label: "Trim History".to_string(),
                    negative_label: "Cancel".to_string(),
                    selected_button: 0,
                    is_danger: false,
                    on_confirm: PendingAction::TrimHistorySoftReset(selected_hash),
                    return_screen,
                });
            }
        }
    }

    fn handle_pager_key(&mut self, key: KeyEvent) {
        if let Screen::Pager(ref mut state) = self.screen {
            let total = state.lines.len();
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    if state.scroll_offset > 0 {
                        state.scroll_offset -= 1;
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if state.scroll_offset + 1 < total {
                        state.scroll_offset += 1;
                    }
                }
                KeyCode::PageUp => {
                    state.scroll_offset = state.scroll_offset.saturating_sub(10);
                }
                KeyCode::PageDown => {
                    if state.scroll_offset + 10 < total {
                        state.scroll_offset += 10;
                    } else if total > 0 {
                        state.scroll_offset = total - 1;
                    }
                }
                KeyCode::Home => {
                    state.scroll_offset = 0;
                }
                KeyCode::End => {
                    if total > 0 {
                        state.scroll_offset = total - 1;
                    }
                }
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => {
                    let ret = *state.return_screen.clone();
                    self.screen = ret;
                }
                _ => {}
            }
        }
    }

    fn handle_result_key(&mut self, key: KeyEvent) {
        let return_screen = if let Screen::Result(ref state) = self.screen {
            match key.code {
                KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char(' ') => {
                    Some(*state.return_screen.clone())
                }
                _ => None,
            }
        } else {
            None
        };

        if let Some(screen) = return_screen {
            self.refresh_metadata();
            self.screen = screen;
        }
    }
}

/// Simple subsequence fuzzy matching that returns matched character indices.
pub fn filter_commits(commits: &[String], query: &str) -> Vec<(String, Vec<usize>)> {
    if query.trim().is_empty() {
        return commits.iter().map(|c| (c.clone(), Vec::new())).collect();
    }

    let query_lower: Vec<char> = query.to_lowercase().chars().collect();
    let mut results = Vec::new();

    for commit in commits {
        let commit_lower: Vec<char> = commit.to_lowercase().chars().collect();

        let mut q_idx = 0;
        let mut matched_indices = Vec::new();

        for (c_idx, &c) in commit_lower.iter().enumerate() {
            if q_idx < query_lower.len() && c == query_lower[q_idx] {
                matched_indices.push(c_idx);
                q_idx += 1;
            }
        }

        if q_idx == query_lower.len() {
            results.push((commit.clone(), matched_indices));
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_commits_empty_query() {
        let commits = vec![
            "2b813b0 │ 2 hours ago │ Initial commit".to_string(),
            "f68bd3b │ 1 hour ago │ Translate menu".to_string(),
        ];
        let result = filter_commits(&commits, "");
        assert_eq!(result.len(), 2);
        assert!(result[0].1.is_empty());
        assert!(result[1].1.is_empty());
    }

    #[test]
    fn test_filter_commits_matching() {
        let commits = vec![
            "2b813b0 │ 2 hours ago │ Initial commit".to_string(),
            "f68bd3b │ 1 hour ago │ Translate menu".to_string(),
        ];
        let result = filter_commits(&commits, "trans");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "f68bd3b │ 1 hour ago │ Translate menu");
        assert!(!result[0].1.is_empty());
    }

    #[test]
    fn test_filter_commits_no_match() {
        let commits = vec!["2b813b0 │ 2 hours ago │ Initial commit".to_string()];
        let result = filter_commits(&commits, "nonexistent");
        assert!(result.is_empty());
    }

    #[test]
    fn test_app_initial_state() {
        let app = App::new();
        assert_eq!(app.top_menu_index, 0);
        assert_eq!(app.submenu_index, 0);
        assert!(!app.should_quit);
        assert!(matches!(app.screen, Screen::TopMenu));
    }

    #[test]
    fn test_input_modal_esc_cancels() {
        let mut app = App::new();
        app.screen = Screen::InputModal(InputModalState {
            action_name: "Test action".to_string(),
            default_text: "default msg".to_string(),
            input: Input::default(),
            flow: InputFlow::Rebuild,
            return_screen: Box::new(Screen::TopMenu),
        });

        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(app.screen, Screen::TopMenu));
        assert!(matches!(app.pending_external_task, ExternalTask::None));
    }
}
