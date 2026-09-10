use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    widgets::{Clear, Paragraph},
};
use std::path::PathBuf;
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;

use crate::actions::{detect_flake_context, git, nix};
use crate::config::{Config, load_config};
use crate::theme::{self, Theme};
use crate::ui::{
    confirm::{ConfirmParams, render_confirm},
    diff::prettify_diff,
    filter::{FilterParams, render_filter},
    header::{centered_rect, render_header},
    input_modal::render_input_modal,
    menu::{MenuParams, render_menu},
    pager::render_pager,
    result::render_result,
    selective::render_selective,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubMenuKind {
    Updates,
    SelectiveUpdate,
    Maintenance,
    GitHistory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectiveMode {
    Include,
    Exclude,
}

#[derive(Debug, Clone)]
pub struct SelectiveUpdateState {
    pub mode: SelectiveMode,
    pub inputs: Vec<crate::actions::nix::FlakeInput>,
    pub selected: Vec<bool>,
    pub cursor: usize,
    pub return_screen: Box<Screen>,
}

#[derive(Debug, Clone)]
pub enum FilterFlow {
    HardReset,
    SoftRevert,
    TrimHistory,
    Restore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputFlow {
    Rebuild,
    Lockfile,
    FullCycle,
    SoftRevert(String),
    TrimHistory(String),
    RestoreFile(String, String),
    SelectiveFullCycle(Vec<String>),
}

#[derive(Debug, Clone)]
pub enum PendingAction {
    CleanStore,
    HardResetExecute(String),
    TrimHistorySoftReset(String),
    RestoreFileExecute(String, String),
    RestoreCommitAndSwitch(String, String),
    SelectiveUpdateLockfile(Vec<String>),
    SelectiveFullCycle(Vec<String>),
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
    pub on_secondary: Option<PendingAction>,
    pub return_screen: Box<Screen>,
    pub on_cancel_screen: Option<Box<Screen>>,
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
    pub preview_hash: String,
    pub preview_lines: Vec<String>,
    pub preview_scroll: usize,
}

#[derive(Debug, Clone)]
pub struct FileFilterState {
    pub commit_hash: String,
    pub files: Vec<String>,
    pub input: Input,
    pub selected_index: usize,
    pub return_screen: Box<Screen>,
    pub preview_file: String,
    pub preview_lines: Vec<String>,
    pub preview_scroll: usize,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenTag {
    TopMenu,
    SubMenu(SubMenuKind),
    Confirm,
    InputModal,
    CommitFilter,
    FileFilter,
    Pager,
    Result,
    SelectiveUpdate,
}

#[derive(Debug, Clone)]
pub enum Screen {
    TopMenu,
    SubMenu(SubMenuKind),
    Confirm(ConfirmState),
    InputModal(InputModalState),
    CommitFilter(CommitFilterState),
    FileFilter(FileFilterState),
    Pager(PagerState),
    Result(ResultState),
    SelectiveUpdate(SelectiveUpdateState),
}

impl Screen {
    pub fn tag(&self) -> ScreenTag {
        match self {
            Screen::TopMenu => ScreenTag::TopMenu,
            Screen::SubMenu(kind) => ScreenTag::SubMenu(*kind),
            Screen::Confirm(_) => ScreenTag::Confirm,
            Screen::InputModal(_) => ScreenTag::InputModal,
            Screen::CommitFilter(_) => ScreenTag::CommitFilter,
            Screen::FileFilter(_) => ScreenTag::FileFilter,
            Screen::Pager(_) => ScreenTag::Pager,
            Screen::Result(_) => ScreenTag::Result,
            Screen::SelectiveUpdate(_) => ScreenTag::SelectiveUpdate,
        }
    }
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
    SelectiveUpdateFlakeOnly(Vec<String>),
    SelectiveFullCycleCommitAndSwitch(Vec<String>, String),
    SelectiveFullCycleSwitchOnly(Vec<String>),
    TestBuild,
    CleanStore,
    HardReset(String),
    SoftRevertCommitAndSwitch(String, String),
    TrimHistoryCommitAndPush(String, String),
    RestoreFile(String, String),
    RestorePatch(String, Option<String>),
    RestoreCommitAndSwitch(String, String, String),
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
    pub config: Config,
    pub theme: Theme,
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
        let config = load_config();
        let theme = Theme::from_config(&config.theme);
        let ctx = detect_flake_context(&config);

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
            config,
            theme,
            pending_external_task: ExternalTask::None,
        }
    }

    pub fn refresh_metadata(&mut self) {
        self.generation = nix::get_system_generation();
        let ctx = detect_flake_context(&self.config);
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
                    Constraint::Length(6), // Menu items box (4 items + 2 borders)
                    Constraint::Length(1), // Spacer
                    Constraint::Length(1), // Centered navigation hint below
                ])
                .split(centered_rect(64, 16, area));

                render_header(
                    frame,
                    chunks[0],
                    &self.host,
                    &self.user,
                    &self.generation,
                    &self.flake_target,
                    &self.theme,
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

                let menu_params = MenuParams {
                    header_title: "Select an action",
                    items: &items,
                    selected_index: self.top_menu_index,
                    show_numbers: self.config.keybindings.enable_quick_digits,
                    back_item_key: &self.config.keybindings.back_item_key,
                };
                render_menu(frame, chunks[2], &menu_params, &self.theme);

                let hint = self.config.keybindings.menu_hint(items.len());
                let hint_area = Rect {
                    x: area.x,
                    y: chunks[4].y,
                    width: area.width,
                    height: chunks[4].height,
                };
                let hint_p = Paragraph::new(hint)
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(self.theme.faint_hint));
                frame.render_widget(hint_p, hint_area);
            }

            Screen::SubMenu(kind) => {
                let (title, items) = match kind {
                    SubMenuKind::Updates => (
                        "Updates",
                        vec![
                            format!("{}  Rebuild System (Rebuild & Switch)", theme::ICON_REBUILD),
                            format!("{}  Update Lockfile (Flake Update)", theme::ICON_PACKAGE),
                            format!("{}  Full Cycle (Update + Switch)", theme::ICON_FULL_CYCLE),
                            format!(
                                "{}  Selective Update (Pick / Exclude Inputs)",
                                theme::ICON_PATCH
                            ),
                            format!("{}  Test Build (Dry Run / Build)", theme::ICON_TEST_BUILD),
                            format!("{}  Back", theme::ICON_BACK),
                        ],
                    ),
                    SubMenuKind::SelectiveUpdate => (
                        "Selective Flake Update",
                        vec![
                            format!("{}  Update Selected Inputs (Include)", theme::ICON_PACKAGE),
                            format!("{}  Update All Except... (Exclude)", theme::ICON_CLEANUP),
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
                            format!("{}  Restore File or Lines from History", theme::ICON_FILE),
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
                let menu_box_height = (items.len() as u16) + 2;
                let total_height = menu_box_height + 2;
                let centered = centered_rect(64, total_height, area);

                let chunks = Layout::vertical([
                    Constraint::Length(menu_box_height), // Menu box with solid rounded borders
                    Constraint::Length(1),               // Spacer
                    Constraint::Length(1),               // Centered navigation hint below
                ])
                .split(centered);

                let menu_params = MenuParams {
                    header_title: title,
                    items: &item_refs,
                    selected_index: self.submenu_index,
                    show_numbers: self.config.keybindings.enable_quick_digits,
                    back_item_key: &self.config.keybindings.back_item_key,
                };
                render_menu(frame, chunks[0], &menu_params, &self.theme);

                let hint = self.config.keybindings.menu_hint(items.len());
                let hint_area = Rect {
                    x: area.x,
                    y: chunks[2].y,
                    width: area.width,
                    height: chunks[2].height,
                };
                let hint_p = Paragraph::new(hint)
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(self.theme.faint_hint));
                frame.render_widget(hint_p, hint_area);
            }

            Screen::Confirm(state) => {
                let lines_ref: Vec<&str> = state.lines.iter().map(String::as_str).collect();
                let hint = self.config.keybindings.confirm_hint();
                let params = ConfirmParams {
                    title: &state.title,
                    lines: &lines_ref,
                    affirmative_label: &state.affirmative_label,
                    negative_label: &state.negative_label,
                    selected_button: state.selected_button,
                    is_danger: state.is_danger,
                    hint: &hint,
                };
                render_confirm(frame, area, &params, &self.theme);
            }

            Screen::InputModal(state) => {
                let hint = self.config.keybindings.input_modal_hint();
                render_input_modal(
                    frame,
                    area,
                    &state.action_name,
                    &state.input,
                    &state.default_text,
                    &hint,
                    &self.theme,
                );
            }

            Screen::CommitFilter(state) => {
                let filtered = filter_commits(&state.commits, state.input.value());
                let filtered_refs: Vec<(&str, Vec<usize>)> = filtered
                    .iter()
                    .map(|(s, indices)| (s.as_str(), indices.clone()))
                    .collect();
                let hint = self.config.keybindings.filter_hint();
                let title = format!("{}  SELECT COMMIT", theme::ICON_HISTORY);
                let params = FilterParams {
                    title: &title,
                    header_title: &state.header_title,
                    input: &state.input,
                    search_placeholder: "Filter by hash, date, or message...",
                    empty_message: " No matching commits found.",
                    filtered_items: &filtered_refs,
                    selected_index: state.selected_index,
                    hint: &hint,
                    preview_hash: &state.preview_hash,
                    preview_lines: &state.preview_lines,
                    preview_scroll: state.preview_scroll,
                };

                render_filter(frame, area, &params, &self.theme);
            }

            Screen::FileFilter(state) => {
                let filtered = filter_commits(&state.files, state.input.value());
                let filtered_refs: Vec<(&str, Vec<usize>)> = filtered
                    .iter()
                    .map(|(s, indices)| (s.as_str(), indices.clone()))
                    .collect();
                let hint =
                    "[Enter] Restore  •  [Ctrl-p] Patch  •  [PgUp/PgDn] Scroll  •  [Esc] Back";
                let title = format!("{}  SELECT FILE TO RESTORE", theme::ICON_FILE);
                let header_title = format!(
                    "Commit {} • Select file to restore or patch",
                    state.commit_hash
                );
                let params = FilterParams {
                    title: &title,
                    header_title: &header_title,
                    input: &state.input,
                    search_placeholder: "Filter files...",
                    empty_message: " No matching files found.",
                    filtered_items: &filtered_refs,
                    selected_index: state.selected_index,
                    hint,
                    preview_hash: &state.preview_file,
                    preview_lines: &state.preview_lines,
                    preview_scroll: state.preview_scroll,
                };

                render_filter(frame, area, &params, &self.theme);
            }

            Screen::Pager(state) => {
                let hint = self
                    .config
                    .keybindings
                    .pager_hint(state.scroll_offset + 1, state.lines.len());
                render_pager(
                    frame,
                    area,
                    &state.title,
                    &state.lines,
                    state.scroll_offset,
                    &hint,
                    &self.theme,
                );
            }

            Screen::Result(state) => {
                let hint = self.config.keybindings.result_hint();
                render_result(
                    frame,
                    area,
                    state.is_success,
                    &state.title,
                    &state.message,
                    &hint,
                    &self.theme,
                );
            }

            Screen::SelectiveUpdate(state) => {
                render_selective(frame, area, state, &self.theme);
            }
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match self.screen.tag() {
            ScreenTag::TopMenu => self.handle_top_menu_key(key),
            ScreenTag::SubMenu(kind) => self.handle_sub_menu_key(kind, key),
            ScreenTag::Confirm => self.handle_confirm_key(key),
            ScreenTag::InputModal => self.handle_input_modal_key(key),
            ScreenTag::CommitFilter => self.handle_commit_filter_key(key),
            ScreenTag::FileFilter => self.handle_file_filter_key(key),
            ScreenTag::Pager => self.handle_pager_key(key),
            ScreenTag::Result => self.handle_result_key(key),
            ScreenTag::SelectiveUpdate => self.handle_selective_update_key(key),
        }
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent, term_width: u16) {
        match mouse.kind {
            MouseEventKind::ScrollUp => match self.screen.tag() {
                ScreenTag::CommitFilter => {
                    let mut should_update = false;
                    if let Screen::CommitFilter(ref mut state) = self.screen {
                        let is_wide = term_width >= 96;
                        let split_x = term_width / 2;
                        if is_wide && mouse.column >= split_x {
                            state.preview_scroll = state.preview_scroll.saturating_sub(1);
                        } else if state.selected_index > 0 {
                            state.selected_index -= 1;
                            should_update = true;
                        }
                    }
                    if should_update {
                        self.update_commit_preview();
                    }
                }
                ScreenTag::FileFilter => {
                    let mut should_update = false;
                    if let Screen::FileFilter(ref mut state) = self.screen {
                        let is_wide = term_width >= 96;
                        let split_x = term_width / 2;
                        if is_wide && mouse.column >= split_x {
                            state.preview_scroll = state.preview_scroll.saturating_sub(1);
                        } else if state.selected_index > 0 {
                            state.selected_index -= 1;
                            should_update = true;
                        }
                    }
                    if should_update {
                        self.update_file_preview();
                    }
                }
                ScreenTag::Pager => {
                    if let Screen::Pager(ref mut state) = self.screen {
                        state.scroll_offset = state.scroll_offset.saturating_sub(1);
                    }
                }
                ScreenTag::SelectiveUpdate => {
                    if let Screen::SelectiveUpdate(ref mut state) = self.screen {
                        state.cursor = state.cursor.saturating_sub(1);
                    }
                }
                ScreenTag::TopMenu if self.top_menu_index > 0 => {
                    self.top_menu_index -= 1;
                }
                ScreenTag::SubMenu(_) if self.submenu_index > 0 => {
                    self.submenu_index -= 1;
                }
                _ => {}
            },
            MouseEventKind::ScrollDown => match self.screen.tag() {
                ScreenTag::CommitFilter => {
                    let mut should_update = false;
                    if let Screen::CommitFilter(ref mut state) = self.screen {
                        let is_wide = term_width >= 96;
                        let split_x = term_width / 2;
                        if is_wide && mouse.column >= split_x {
                            let total = state.preview_lines.len();
                            state.preview_scroll =
                                (state.preview_scroll + 1).min(total.saturating_sub(1));
                        } else {
                            let filtered = filter_commits(&state.commits, state.input.value());
                            let count = filtered.len();
                            if count > 0 && state.selected_index + 1 < count {
                                state.selected_index += 1;
                                should_update = true;
                            }
                        }
                    }
                    if should_update {
                        self.update_commit_preview();
                    }
                }
                ScreenTag::FileFilter => {
                    let mut should_update = false;
                    if let Screen::FileFilter(ref mut state) = self.screen {
                        let is_wide = term_width >= 96;
                        let split_x = term_width / 2;
                        if is_wide && mouse.column >= split_x {
                            let total = state.preview_lines.len();
                            state.preview_scroll =
                                (state.preview_scroll + 1).min(total.saturating_sub(1));
                        } else {
                            let filtered = filter_commits(&state.files, state.input.value());
                            let count = filtered.len();
                            if count > 0 && state.selected_index + 1 < count {
                                state.selected_index += 1;
                                should_update = true;
                            }
                        }
                    }
                    if should_update {
                        self.update_file_preview();
                    }
                }
                ScreenTag::Pager => {
                    if let Screen::Pager(ref mut state) = self.screen {
                        let total = state.lines.len();
                        state.scroll_offset =
                            (state.scroll_offset + 1).min(total.saturating_sub(1));
                    }
                }
                ScreenTag::SelectiveUpdate => {
                    if let Screen::SelectiveUpdate(ref mut state) = self.screen
                        && state.cursor + 1 < state.inputs.len()
                    {
                        state.cursor += 1;
                    }
                }
                ScreenTag::TopMenu if self.top_menu_index + 1 < 4 => {
                    self.top_menu_index += 1;
                }
                ScreenTag::SubMenu(kind)
                    if self.submenu_index + 1 < self.submenu_item_count(kind) =>
                {
                    self.submenu_index += 1;
                }
                _ => {}
            },
            _ => {}
        }
    }

    pub fn submenu_item_count(&self, kind: SubMenuKind) -> usize {
        match kind {
            SubMenuKind::Updates => 6,
            SubMenuKind::SelectiveUpdate => 3,
            SubMenuKind::Maintenance => 3,
            SubMenuKind::GitHistory => 6,
        }
    }

    fn handle_top_menu_key(&mut self, key: KeyEvent) {
        // Direct key shortcut for the Back / Exit item (e.g. 'q')
        if self.config.keybindings.is_back_item_key(&key) {
            self.should_quit = true;
            return;
        }

        // Quick digit selection for actions (items 0, 1, 2)
        if let Some(digit_idx) = self.config.keybindings.get_digit_index(&key)
            && digit_idx < 3
        {
            self.top_menu_index = digit_idx;
            self.trigger_top_menu_item(digit_idx);
            return;
        }

        if self.config.keybindings.is_up(&key) {
            if self.top_menu_index > 0 {
                self.top_menu_index -= 1;
            } else {
                self.top_menu_index = 3;
            }
        } else if self.config.keybindings.is_down(&key) {
            if self.top_menu_index < 3 {
                self.top_menu_index += 1;
            } else {
                self.top_menu_index = 0;
            }
        } else if self.config.keybindings.is_select(&key) {
            self.trigger_top_menu_item(self.top_menu_index);
        } else if self.config.keybindings.is_back(&key) || self.config.keybindings.is_quit(&key) {
            self.should_quit = true;
        }
    }

    fn trigger_top_menu_item(&mut self, index: usize) {
        match index {
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
        }
    }

    fn handle_sub_menu_key(&mut self, kind: SubMenuKind, key: KeyEvent) {
        let count: usize = self.submenu_item_count(kind);

        // Direct key shortcut for the Back item (e.g. 'q')
        if self.config.keybindings.is_back_item_key(&key) {
            if kind == SubMenuKind::SelectiveUpdate {
                self.submenu_index = 3;
                self.screen = Screen::SubMenu(SubMenuKind::Updates);
            } else {
                self.screen = Screen::TopMenu;
            }
            return;
        }

        // Quick digit selection for actions (excluding the last Back item)
        let action_count = count.saturating_sub(1);
        if let Some(digit_idx) = self.config.keybindings.get_digit_index(&key)
            && digit_idx < action_count
        {
            self.submenu_index = digit_idx;
            self.trigger_sub_menu_item(kind, digit_idx);
            return;
        }

        if self.config.keybindings.is_up(&key) {
            if self.submenu_index > 0 {
                self.submenu_index -= 1;
            } else {
                self.submenu_index = count - 1;
            }
        } else if self.config.keybindings.is_down(&key) {
            if self.submenu_index + 1 < count {
                self.submenu_index += 1;
            } else {
                self.submenu_index = 0;
            }
        } else if self.config.keybindings.is_back(&key) {
            if kind == SubMenuKind::SelectiveUpdate {
                self.submenu_index = 3;
                self.screen = Screen::SubMenu(SubMenuKind::Updates);
            } else {
                self.screen = Screen::TopMenu;
            }
        } else if self.config.keybindings.is_quit(&key) {
            self.should_quit = true;
        } else if self.config.keybindings.is_select(&key) {
            self.trigger_sub_menu_item(kind, self.submenu_index);
        }
    }

    fn trigger_sub_menu_item(&mut self, kind: SubMenuKind, index: usize) {
        match kind {
            SubMenuKind::Updates => match index {
                0 => self.start_rebuild_system(),
                1 => self.start_update_lockfile(),
                2 => self.start_full_cycle(),
                3 => {
                    self.submenu_index = 0;
                    self.screen = Screen::SubMenu(SubMenuKind::SelectiveUpdate);
                }
                4 => self.start_test_build(),
                _ => self.screen = Screen::TopMenu,
            },
            SubMenuKind::SelectiveUpdate => match index {
                0 => self.start_selective_update(SelectiveMode::Include),
                1 => self.start_selective_update(SelectiveMode::Exclude),
                _ => {
                    self.submenu_index = 3;
                    self.screen = Screen::SubMenu(SubMenuKind::Updates);
                }
            },
            SubMenuKind::Maintenance => match index {
                0 => self.start_clean_store(),
                1 => self.start_generations_history(),
                _ => self.screen = Screen::TopMenu,
            },
            SubMenuKind::GitHistory => match index {
                0 => self.start_show_git_diff(),
                1 => self.start_restore_flow(),
                2 => self.start_hard_reset_flow(),
                3 => self.start_soft_revert_flow(),
                4 => self.start_trim_history_flow(),
                _ => self.screen = Screen::TopMenu,
            },
        }
    }

    fn start_rebuild_system(&mut self) {
        if self.is_git {
            let has_changes = git::has_uncommitted_changes(&self.flake_dir).unwrap_or(false);

            if has_changes {
                let default_text = self.config.commit_templates.rebuild.clone();
                self.screen = Screen::InputModal(InputModalState {
                    action_name: "Rebuilding configuration".to_string(),
                    default_text,
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
            let default_text = self.config.commit_templates.full_cycle.clone();
            self.screen = Screen::InputModal(InputModalState {
                action_name: "Full update cycle".to_string(),
                default_text,
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
            on_secondary: None,
            return_screen: Box::new(Screen::SubMenu(SubMenuKind::Maintenance)),
            on_cancel_screen: None,
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
                let lines: Vec<String> = prettify_diff(&diff);
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

    fn start_restore_flow(&mut self) {
        match git::get_recent_commits(&self.flake_dir) {
            Ok(commits) if !commits.is_empty() => {
                self.screen = Screen::CommitFilter(CommitFilterState {
                    header_title: "Select commit to restore a file or lines/hunks from".to_string(),
                    flow: FilterFlow::Restore,
                    commits,
                    input: Input::default(),
                    selected_index: 0,
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                    preview_hash: String::new(),
                    preview_lines: Vec::new(),
                    preview_scroll: 0,
                });
                self.update_commit_preview();
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
                    preview_hash: String::new(),
                    preview_lines: Vec::new(),
                    preview_scroll: 0,
                });
                self.update_commit_preview();
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
                    preview_hash: String::new(),
                    preview_lines: Vec::new(),
                    preview_scroll: 0,
                });
                self.update_commit_preview();
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
                    preview_hash: String::new(),
                    preview_lines: Vec::new(),
                    preview_scroll: 0,
                });
                self.update_commit_preview();
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
        let (chosen_action, return_screen) = if let Screen::Confirm(ref mut state) = self.screen {
            if key.code == KeyCode::Left
                || key.code == KeyCode::Right
                || key.code == KeyCode::Tab
                || key.code == KeyCode::Char('h')
                || key.code == KeyCode::Char('l')
            {
                state.selected_button = 1 - state.selected_button;
                return;
            } else if self.config.keybindings.is_back(&key) {
                if let Some(cancel_screen) = state.on_cancel_screen.take() {
                    self.screen = *cancel_screen;
                } else {
                    self.screen = *state.return_screen.clone();
                }
                return;
            } else if self.config.keybindings.is_select(&key) {
                if state.selected_button == 1 {
                    if let Some(sec) = state.on_secondary.take() {
                        (sec, state.return_screen.clone())
                    } else if let Some(cancel_screen) = state.on_cancel_screen.take() {
                        self.screen = *cancel_screen;
                        return;
                    } else {
                        self.screen = *state.return_screen.clone();
                        return;
                    }
                } else {
                    (state.on_confirm.clone(), state.return_screen.clone())
                }
            } else {
                return;
            }
        } else {
            return;
        };

        match chosen_action {
            PendingAction::CleanStore => {
                self.pending_external_task = ExternalTask::CleanStore;
            }
            PendingAction::HardResetExecute(hash) => {
                self.pending_external_task = ExternalTask::HardReset(hash);
            }
            PendingAction::TrimHistorySoftReset(hash) => {
                let default_text = self
                    .config
                    .commit_templates
                    .trim_history
                    .replace("{hash}", &hash);
                self.screen = Screen::InputModal(InputModalState {
                    action_name: format!("Trim commit history to {hash}"),
                    default_text,
                    input: Input::default(),
                    flow: InputFlow::TrimHistory(hash),
                    return_screen,
                });
            }
            PendingAction::RestoreFileExecute(hash, file) => {
                self.pending_external_task = ExternalTask::RestoreFile(hash, file);
            }
            PendingAction::RestoreCommitAndSwitch(hash, file) => {
                let default_text = self
                    .config
                    .commit_templates
                    .restore_file
                    .replace("{file}", &file)
                    .replace("{hash}", &hash);
                self.screen = Screen::InputModal(InputModalState {
                    action_name: format!("Restore {file} from {hash}"),
                    default_text,
                    input: Input::default(),
                    flow: InputFlow::RestoreFile(hash, file),
                    return_screen,
                });
            }
            PendingAction::SelectiveUpdateLockfile(inputs) => {
                self.pending_external_task = ExternalTask::SelectiveUpdateFlakeOnly(inputs);
            }
            PendingAction::SelectiveFullCycle(inputs) => {
                if self.is_git {
                    let default_text = format!("chore(flake): update {}", inputs.join(", "));
                    self.screen = Screen::InputModal(InputModalState {
                        action_name: "Selective full update cycle".to_string(),
                        default_text,
                        input: Input::default(),
                        flow: InputFlow::SelectiveFullCycle(inputs),
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::SelectiveUpdate)),
                    });
                } else {
                    self.pending_external_task = ExternalTask::SelectiveFullCycleSwitchOnly(inputs);
                }
            }
        }
    }

    fn handle_input_modal_key(&mut self, key: KeyEvent) {
        if let Screen::InputModal(ref mut state) = self.screen {
            if self.config.keybindings.is_clear_input(&key) {
                state.input.reset();
                return;
            }

            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                let ret = *state.return_screen.clone();
                self.screen = ret;
                return;
            }

            if self.config.keybindings.is_back(&key) {
                let ret = *state.return_screen.clone();
                self.screen = ret;
            } else if self.config.keybindings.is_select(&key) {
                let msg = if state.input.value().trim().is_empty() {
                    state.default_text.clone()
                } else {
                    state.input.value().trim().to_string()
                };
                let flow = state.flow.clone();
                self.submit_input(flow, msg);
            } else {
                state.input.handle_event(&Event::Key(key));
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
            InputFlow::RestoreFile(hash, file) => {
                self.pending_external_task = ExternalTask::RestoreCommitAndSwitch(hash, file, msg);
            }
            InputFlow::SelectiveFullCycle(inputs) => {
                self.pending_external_task =
                    ExternalTask::SelectiveFullCycleCommitAndSwitch(inputs, msg);
            }
        }
    }

    pub fn start_selective_update(&mut self, mode: SelectiveMode) {
        match nix::get_flake_inputs(&self.flake_dir) {
            Ok(inputs) if !inputs.is_empty() => {
                let len = inputs.len();
                self.screen = Screen::SelectiveUpdate(SelectiveUpdateState {
                    mode,
                    inputs,
                    selected: vec![false; len],
                    cursor: 0,
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::SelectiveUpdate)),
                });
            }
            Ok(_) | Err(_) => {
                self.screen = Screen::Result(ResultState {
                    is_success: false,
                    title: "NO FLAKE INPUTS FOUND".to_string(),
                    message: format!(
                        "Could not locate inputs in flake.lock or flake.nix within {}.",
                        self.flake_dir.display()
                    ),
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::SelectiveUpdate)),
                });
            }
        }
    }

    fn handle_selective_update_key(&mut self, key: KeyEvent) {
        let (mode, _inputs_len, _cursor, selected_inputs, cancel_screen) =
            if let Screen::SelectiveUpdate(ref mut state) = self.screen {
                if self.config.keybindings.is_back(&key) || self.config.keybindings.is_quit(&key) {
                    self.screen = *state.return_screen.clone();
                    return;
                }

                if state.inputs.is_empty() {
                    return;
                }

                if self.config.keybindings.is_up(&key) {
                    if state.cursor > 0 {
                        state.cursor -= 1;
                    } else {
                        state.cursor = state.inputs.len().saturating_sub(1);
                    }
                    return;
                } else if self.config.keybindings.is_down(&key) {
                    if state.cursor + 1 < state.inputs.len() {
                        state.cursor += 1;
                    } else {
                        state.cursor = 0;
                    }
                    return;
                } else if key.code == KeyCode::Char(' ') {
                    if let Some(val) = state.selected.get_mut(state.cursor) {
                        *val = !*val;
                    }
                    if state.cursor + 1 < state.inputs.len() {
                        state.cursor += 1;
                    }
                    return;
                } else if key.code == KeyCode::Char('a') {
                    state.selected.fill(true);
                    return;
                } else if key.code == KeyCode::Char('n') {
                    state.selected.fill(false);
                    return;
                } else if key.code == KeyCode::Char('i') {
                    for s in &mut state.selected {
                        *s = !*s;
                    }
                    return;
                } else if self.config.keybindings.is_select(&key) {
                    let target: Vec<String> = match state.mode {
                        SelectiveMode::Include => state
                            .inputs
                            .iter()
                            .enumerate()
                            .filter(|(idx, _)| state.selected.get(*idx).copied().unwrap_or(false))
                            .map(|(_, inp)| inp.name.clone())
                            .collect(),
                        SelectiveMode::Exclude => state
                            .inputs
                            .iter()
                            .enumerate()
                            .filter(|(idx, _)| !state.selected.get(*idx).copied().unwrap_or(false))
                            .map(|(_, inp)| inp.name.clone())
                            .collect(),
                    };

                    (
                        state.mode,
                        state.inputs.len(),
                        state.cursor,
                        target,
                        Box::new(Screen::SelectiveUpdate(state.clone())),
                    )
                } else {
                    return;
                }
            } else {
                return;
            };

        if selected_inputs.is_empty() {
            let msg = match mode {
                SelectiveMode::Include => {
                    "No inputs were selected. Press Space to select at least one input to update."
                }
                SelectiveMode::Exclude => {
                    "All inputs were excluded. Uncheck at least one input to update."
                }
            };
            self.screen = Screen::Result(ResultState {
                is_success: false,
                title: "EMPTY SELECTION".to_string(),
                message: msg.to_string(),
                return_screen: cancel_screen,
            });
            return;
        }

        let count = selected_inputs.len();
        let title = format!("{}  UPDATE {} INPUT(S)", theme::ICON_PACKAGE, count);
        let list_preview = if count <= 4 {
            selected_inputs.join(", ")
        } else {
            format!(
                "{}, and {} more",
                selected_inputs[..3].join(", "),
                count - 3
            )
        };

        let mode_desc = match mode {
            SelectiveMode::Include => format!("Target inputs: {list_preview}"),
            SelectiveMode::Exclude => {
                format!("Target inputs (excluding unchecked): {list_preview}")
            }
        };

        self.screen = Screen::Confirm(ConfirmState {
            title,
            lines: vec![mode_desc, "Choose how to proceed:".to_string()],
            affirmative_label: "Lockfile Only".to_string(),
            negative_label: "Full Cycle (Update & Switch)".to_string(),
            selected_button: 0,
            is_danger: false,
            on_confirm: PendingAction::SelectiveUpdateLockfile(selected_inputs.clone()),
            on_secondary: Some(PendingAction::SelectiveFullCycle(selected_inputs)),
            return_screen: Box::new(Screen::SubMenu(SubMenuKind::SelectiveUpdate)),
            on_cancel_screen: Some(cancel_screen),
        });
    }

    pub fn update_commit_preview(&mut self) {
        if let Screen::CommitFilter(ref mut state) = self.screen {
            let filtered = filter_commits(&state.commits, state.input.value());
            let current_hash = if !filtered.is_empty() && state.selected_index < filtered.len() {
                let (ref commit_str, _) = filtered[state.selected_index];
                commit_str
                    .split([' ', '│'])
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string()
            } else {
                String::new()
            };

            if current_hash != state.preview_hash {
                state.preview_hash = current_hash.clone();
                state.preview_scroll = 0;
                if !current_hash.is_empty() {
                    match git::get_commit_diff(&self.flake_dir, &current_hash) {
                        Ok(diff) => {
                            state.preview_lines = prettify_diff(&diff);
                        }
                        Err(err) => {
                            state.preview_lines =
                                vec![format!("Failed to load commit diff: {err}")];
                        }
                    }
                } else {
                    state.preview_lines = vec!["No commit selected".to_string()];
                }
            }
        }
    }

    fn handle_commit_filter_key(&mut self, key: KeyEvent) {
        let (action_to_take, filtered_count) =
            if let Screen::CommitFilter(ref mut state) = self.screen {
                let filtered = filter_commits(&state.commits, state.input.value());
                let count = filtered.len();

                if self.config.keybindings.is_clear_input(&key) {
                    state.input.reset();
                    state.selected_index = 0;
                    (None, count)
                } else if (key.modifiers.contains(KeyModifiers::CONTROL)
                    && key.code == KeyCode::Char('c'))
                    || self.config.keybindings.is_back(&key)
                {
                    let ret = *state.return_screen.clone();
                    self.screen = ret;
                    return;
                } else if key.code == KeyCode::Up {
                    if state.selected_index > 0 {
                        state.selected_index -= 1;
                    } else if count > 0 {
                        state.selected_index = count - 1;
                    }
                    (None, count)
                } else if key.code == KeyCode::Down {
                    if count > 0 && state.selected_index + 1 < count {
                        state.selected_index += 1;
                    } else {
                        state.selected_index = 0;
                    }
                    (None, count)
                } else if self.config.keybindings.is_page_up(&key) {
                    state.preview_scroll = state.preview_scroll.saturating_sub(10);
                    (None, count)
                } else if self.config.keybindings.is_page_down(&key) {
                    let total = state.preview_lines.len();
                    state.preview_scroll = (state.preview_scroll + 10).min(total.saturating_sub(1));
                    (None, count)
                } else if self.config.keybindings.is_select(&key) {
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
                } else {
                    state.input.handle_event(&Event::Key(key));
                    state.selected_index = 0;
                    (None, count)
                }
            } else {
                return;
            };

        if let Some((flow, hash, ret)) = action_to_take {
            if !hash.is_empty() {
                self.submit_commit_filter(flow, hash, ret);
            }
        } else if filtered_count == 0 && self.config.keybindings.is_select(&key) {
            // No action on enter when empty
        } else {
            self.update_commit_preview();
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
                    on_secondary: None,
                    return_screen,
                    on_cancel_screen: None,
                });
            }
            FilterFlow::SoftRevert => {
                let default_text = self
                    .config
                    .commit_templates
                    .soft_revert
                    .replace("{hash}", &selected_hash);
                self.screen = Screen::InputModal(InputModalState {
                    action_name: format!("Soft revert files to {selected_hash}"),
                    default_text,
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
                    on_secondary: None,
                    return_screen,
                    on_cancel_screen: None,
                });
            }
            FilterFlow::Restore => match git::get_commit_files(&self.flake_dir, &selected_hash) {
                Ok(files) => {
                    let mut all_items = vec![format!(
                        "{} All files in commit (Interactive Line/Hunk Patch)",
                        theme::ICON_PATCH
                    )];
                    all_items.extend(files);

                    let prev_screen = Box::new(self.screen.clone());
                    self.screen = Screen::FileFilter(FileFilterState {
                        commit_hash: selected_hash,
                        files: all_items,
                        input: Input::default(),
                        selected_index: 0,
                        return_screen: prev_screen,
                        preview_file: String::new(),
                        preview_lines: Vec::new(),
                        preview_scroll: 0,
                    });
                    self.update_file_preview();
                }
                Err(err) => {
                    self.screen = Screen::Result(ResultState {
                        is_success: false,
                        title: "FAILED TO LIST COMMIT FILES".to_string(),
                        message: err.to_string(),
                        return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                    });
                }
            },
        }
    }

    pub fn update_file_preview(&mut self) {
        if let Screen::FileFilter(ref mut state) = self.screen {
            let filtered = filter_commits(&state.files, state.input.value());
            let current_file = if !filtered.is_empty() && state.selected_index < filtered.len() {
                let (ref file_str, _) = filtered[state.selected_index];
                file_str.clone()
            } else {
                String::new()
            };

            if current_file != state.preview_file {
                state.preview_file = current_file.clone();
                state.preview_scroll = 0;
                if !current_file.is_empty() {
                    if current_file.contains("All files in commit") {
                        match git::get_commit_diff(&self.flake_dir, &state.commit_hash) {
                            Ok(diff) => {
                                state.preview_lines = prettify_diff(&diff);
                            }
                            Err(err) => {
                                state.preview_lines =
                                    vec![format!("Failed to load commit diff: {err}")];
                            }
                        }
                    } else {
                        match git::get_file_diff_from_commit(
                            &self.flake_dir,
                            &state.commit_hash,
                            &current_file,
                        ) {
                            Ok(diff) => {
                                state.preview_lines = prettify_diff(&diff);
                            }
                            Err(err) => {
                                state.preview_lines =
                                    vec![format!("Failed to load file diff: {err}")];
                            }
                        }
                    }
                } else {
                    state.preview_lines = vec!["No file selected".to_string()];
                }
            }
        }
    }

    fn handle_file_filter_key(&mut self, key: KeyEvent) {
        let (action_to_take, filtered_count) =
            if let Screen::FileFilter(ref mut state) = self.screen {
                let filtered = filter_commits(&state.files, state.input.value());
                let count = filtered.len();

                if self.config.keybindings.is_clear_input(&key) {
                    state.input.reset();
                    state.selected_index = 0;
                    (None, count)
                } else if (key.modifiers.contains(KeyModifiers::CONTROL)
                    && key.code == KeyCode::Char('c'))
                    || self.config.keybindings.is_back(&key)
                {
                    self.screen = *state.return_screen.clone();
                    return;
                } else if key.code == KeyCode::Up {
                    if state.selected_index > 0 {
                        state.selected_index -= 1;
                    } else if count > 0 {
                        state.selected_index = count - 1;
                    }
                    (None, count)
                } else if key.code == KeyCode::Down {
                    if count > 0 && state.selected_index + 1 < count {
                        state.selected_index += 1;
                    } else {
                        state.selected_index = 0;
                    }
                    (None, count)
                } else if self.config.keybindings.is_page_up(&key) {
                    state.preview_scroll = state.preview_scroll.saturating_sub(10);
                    (None, count)
                } else if self.config.keybindings.is_page_down(&key) {
                    let total = state.preview_lines.len();
                    state.preview_scroll = (state.preview_scroll + 10).min(total.saturating_sub(1));
                    (None, count)
                } else if key.modifiers.contains(KeyModifiers::CONTROL)
                    && key.code == KeyCode::Char('p')
                {
                    if count > 0 {
                        let (ref file_str, _) = filtered[state.selected_index];
                        let file_opt = if file_str.contains("All files in commit") {
                            None
                        } else {
                            Some(file_str.clone())
                        };
                        (Some((state.commit_hash.clone(), file_opt, true)), count)
                    } else {
                        (None, 0)
                    }
                } else if self.config.keybindings.is_select(&key) {
                    if count == 0 {
                        (None, 0)
                    } else {
                        let (ref file_str, _) = filtered[state.selected_index];
                        if file_str.contains("All files in commit") {
                            (Some((state.commit_hash.clone(), None, true)), count)
                        } else {
                            (
                                Some((state.commit_hash.clone(), Some(file_str.clone()), false)),
                                count,
                            )
                        }
                    }
                } else {
                    state.input.handle_event(&Event::Key(key));
                    state.selected_index = 0;
                    (None, count)
                }
            } else {
                return;
            };

        if let Some((hash, file_opt, is_patch)) = action_to_take {
            let has_parent = git::has_parent_commit(&self.flake_dir, &hash);
            let is_rollback = if let Some(ref file) = file_opt {
                git::is_file_identical_to_head(&self.flake_dir, &hash, file) && has_parent
            } else {
                git::is_commit_identical_to_head(&self.flake_dir, &hash) && has_parent
            };
            let target_ref = if is_rollback {
                format!("{hash}~1")
            } else {
                hash.clone()
            };

            if is_patch {
                self.pending_external_task = ExternalTask::RestorePatch(target_ref, file_opt);
            } else if let Some(file) = file_opt {
                let (title, line_desc, button_label) = if is_rollback {
                    (
                        format!("{}  ROLLBACK FILE: {}", theme::ICON_SOFT_REVERT, file),
                        format!(
                            "Revert changes to '{file}' made in {hash} (restore from parent commit {target_ref})?"
                        ),
                        "Rollback File".to_string(),
                    )
                } else {
                    (
                        format!("{}  RESTORE FILE: {}", theme::ICON_FILE, file),
                        format!("Overwrite '{file}' with the version from snapshot {target_ref}?"),
                        "Restore Entire File".to_string(),
                    )
                };

                self.screen = Screen::Confirm(ConfirmState {
                    title,
                    lines: vec![
                        format!("Target: {target_ref}"),
                        line_desc,
                        "Any uncommitted working changes in this file will be replaced."
                            .to_string(),
                    ],
                    affirmative_label: button_label,
                    negative_label: "Cancel".to_string(),
                    selected_button: 0,
                    is_danger: false,
                    on_confirm: PendingAction::RestoreFileExecute(target_ref, file),
                    on_secondary: None,
                    return_screen: Box::new(Screen::SubMenu(SubMenuKind::GitHistory)),
                    on_cancel_screen: None,
                });
            }
        } else if filtered_count == 0 && self.config.keybindings.is_select(&key) {
            // No action
        } else {
            self.update_file_preview();
        }
    }

    fn handle_pager_key(&mut self, key: KeyEvent) {
        if let Screen::Pager(ref mut state) = self.screen {
            let total = state.lines.len();
            if self.config.keybindings.is_up(&key) {
                if state.scroll_offset > 0 {
                    state.scroll_offset -= 1;
                }
            } else if self.config.keybindings.is_down(&key) {
                if state.scroll_offset + 1 < total {
                    state.scroll_offset += 1;
                }
            } else if self.config.keybindings.is_page_up(&key) {
                state.scroll_offset = state.scroll_offset.saturating_sub(10);
            } else if self.config.keybindings.is_page_down(&key) {
                state.scroll_offset = (state.scroll_offset + 10).min(total.saturating_sub(1));
            } else if self.config.keybindings.is_home(&key) {
                state.scroll_offset = 0;
            } else if self.config.keybindings.is_end(&key) {
                // Scroll to show the last lines; exact visible_height is
                // computed at render time, so use a reasonable estimate.
                state.scroll_offset = total.saturating_sub(1);
            } else if self.config.keybindings.is_back(&key)
                || self.config.keybindings.is_quit(&key)
                || self.config.keybindings.is_select(&key)
            {
                let ret = *state.return_screen.clone();
                self.screen = ret;
            }
        }
    }

    fn handle_result_key(&mut self, key: KeyEvent) {
        let return_screen = if let Screen::Result(ref state) = self.screen {
            if self.config.keybindings.is_select(&key)
                || self.config.keybindings.is_back(&key)
                || self.config.keybindings.is_quit(&key)
                || key.code == KeyCode::Char(' ')
            {
                Some(*state.return_screen.clone())
            } else {
                None
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
        let mut q_idx = 0;
        let mut matched_indices = Vec::new();

        for (c_idx, c) in commit.chars().enumerate() {
            if q_idx < query_lower.len() {
                let lower_chars: Vec<char> = c.to_lowercase().collect();
                if lower_chars.contains(&query_lower[q_idx]) {
                    matched_indices.push(c_idx);
                    q_idx += 1;
                }
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
    fn test_filter_commits_unicode() {
        let commits = vec![
            "1234567 │ 1 hour ago │ Исправление конфигурации".to_string(),
            "abcdef0 │ 2 hours ago │ Update README".to_string(),
        ];
        let result = filter_commits(&commits, "исправ");
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].0,
            "1234567 │ 1 hour ago │ Исправление конфигурации"
        );
        assert_eq!(result[0].1.len(), 6);
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

    #[test]
    fn test_app_keybindings_custom() {
        let mut app = App::new();
        app.config.keybindings.enable_quick_digits = false;
        app.config.keybindings.down = vec!["s".to_string()];
        app.config.keybindings.up = vec!["w".to_string()];

        app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
        assert_eq!(app.top_menu_index, 1);

        app.handle_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::NONE));
        assert_eq!(app.top_menu_index, 0);
    }

    #[test]
    fn test_app_digit_quick_selection() {
        let mut app = App::new();
        app.config.keybindings.enable_quick_digits = true;
        app.config.keybindings.back_item_key = "q".to_string();

        // Press '1' on top menu -> opens Updates submenu
        app.handle_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE));
        assert!(matches!(app.screen, Screen::SubMenu(SubMenuKind::Updates)));

        // Press 'q' on Updates submenu -> Back to top menu
        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(matches!(app.screen, Screen::TopMenu));

        // Press '2' on top menu -> opens Maintenance submenu
        app.handle_key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE));
        assert!(matches!(
            app.screen,
            Screen::SubMenu(SubMenuKind::Maintenance)
        ));

        // Press 'q' on Maintenance submenu -> Back to top menu
        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(matches!(app.screen, Screen::TopMenu));

        // Press 'q' on Top menu -> should_quit
        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(app.should_quit);
    }

    #[test]
    fn test_commit_filter_preview_scroll() {
        let mut app = App::new();
        app.screen = Screen::CommitFilter(CommitFilterState {
            header_title: "Test header".to_string(),
            flow: FilterFlow::HardReset,
            commits: vec![
                "1234567 │ 1 hour ago │ Initial commit".to_string(),
                "abcdef0 │ 2 hours ago │ Second commit".to_string(),
            ],
            input: Input::default(),
            selected_index: 0,
            return_screen: Box::new(Screen::TopMenu),
            preview_hash: "1234567".to_string(),
            preview_lines: (0..50).map(|i| format!("diff line {i}")).collect(),
            preview_scroll: 0,
        });

        // PageDown scrolls preview by 10 lines
        app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
        if let Screen::CommitFilter(ref state) = app.screen {
            assert_eq!(state.preview_scroll, 10);
        } else {
            panic!("Expected CommitFilter screen");
        }

        // PageUp scrolls back
        app.handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));
        if let Screen::CommitFilter(ref state) = app.screen {
            assert_eq!(state.preview_scroll, 0);
        } else {
            panic!("Expected CommitFilter screen");
        }
    }

    #[test]
    fn test_commit_filter_mouse_scroll_commits() {
        let mut app = App::new();
        app.screen = Screen::CommitFilter(CommitFilterState {
            header_title: "Test header".to_string(),
            flow: FilterFlow::HardReset,
            commits: vec![
                "1234567 │ 1 hour ago │ Initial commit".to_string(),
                "abcdef0 │ 2 hours ago │ Second commit".to_string(),
                "7654321 │ 3 hours ago │ Third commit".to_string(),
            ],
            input: Input::default(),
            selected_index: 0,
            return_screen: Box::new(Screen::TopMenu),
            preview_hash: "1234567".to_string(),
            preview_lines: vec!["diff line".to_string()],
            preview_scroll: 0,
        });

        // Mouse scroll down over the left pane (column 20) moves by exactly 1 commit
        app.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: 20,
                row: 10,
                modifiers: KeyModifiers::NONE,
            },
            120,
        );
        if let Screen::CommitFilter(ref state) = app.screen {
            assert_eq!(state.selected_index, 1);
        }

        // Another scroll down moves by 1 again
        app.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: 20,
                row: 10,
                modifiers: KeyModifiers::NONE,
            },
            120,
        );
        if let Screen::CommitFilter(ref state) = app.screen {
            assert_eq!(state.selected_index, 2);
        }

        // Scroll up moves back by 1 commit
        app.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::ScrollUp,
                column: 20,
                row: 10,
                modifiers: KeyModifiers::NONE,
            },
            120,
        );
        if let Screen::CommitFilter(ref state) = app.screen {
            assert_eq!(state.selected_index, 1);
        }
    }

    #[test]
    fn test_commit_filter_mouse_scroll_preview() {
        let mut app = App::new();
        app.screen = Screen::CommitFilter(CommitFilterState {
            header_title: "Test header".to_string(),
            flow: FilterFlow::HardReset,
            commits: vec!["1234567 │ 1 hour ago │ Initial commit".to_string()],
            input: Input::default(),
            selected_index: 0,
            return_screen: Box::new(Screen::TopMenu),
            preview_hash: "1234567".to_string(),
            preview_lines: (0..50).map(|i| format!("diff line {i}")).collect(),
            preview_scroll: 0,
        });

        // Mouse scroll down over the right pane (column 80 in 120-wide terminal) scrolls diff preview
        app.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: 80,
                row: 10,
                modifiers: KeyModifiers::NONE,
            },
            120,
        );
        if let Screen::CommitFilter(ref state) = app.screen {
            assert_eq!(state.preview_scroll, 1);
            assert_eq!(state.selected_index, 0);
        }
    }

    #[test]
    fn test_file_filter_navigation_and_selection() {
        let mut app = App::new();
        app.screen = Screen::FileFilter(FileFilterState {
            commit_hash: "abcdef1".to_string(),
            files: vec![
                "All files in commit (Interactive Line/Hunk Patch)".to_string(),
                "configuration.nix".to_string(),
                "flake.nix".to_string(),
            ],
            input: Input::default(),
            selected_index: 0,
            return_screen: Box::new(Screen::TopMenu),
            preview_file: String::new(),
            preview_lines: Vec::new(),
            preview_scroll: 0,
        });

        // Down arrow moves to configuration.nix
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        if let Screen::FileFilter(ref state) = app.screen {
            assert_eq!(state.selected_index, 1);
        }

        // Enter on configuration.nix opens Confirm modal
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(app.screen, Screen::Confirm(_)));
        if let Screen::Confirm(ref state) = app.screen {
            assert!(
                matches!(state.on_confirm, PendingAction::RestoreFileExecute(ref h, ref f) if h == "abcdef1" && f == "configuration.nix")
            );
        }
    }

    #[test]
    fn test_file_filter_patch_all_files() {
        let mut app = App::new();
        app.screen = Screen::FileFilter(FileFilterState {
            commit_hash: "abcdef1".to_string(),
            files: vec![
                "All files in commit (Interactive Line/Hunk Patch)".to_string(),
                "configuration.nix".to_string(),
            ],
            input: Input::default(),
            selected_index: 0,
            return_screen: Box::new(Screen::TopMenu),
            preview_file: String::new(),
            preview_lines: Vec::new(),
            preview_scroll: 0,
        });

        // Enter on index 0 (All files in commit) triggers RestorePatch
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(
            app.pending_external_task,
            ExternalTask::RestorePatch(ref h, None) if h == "abcdef1"
        ));
    }

    #[test]
    fn test_file_filter_ctrl_p_shortcut() {
        let mut app = App::new();
        app.screen = Screen::FileFilter(FileFilterState {
            commit_hash: "abcdef1".to_string(),
            files: vec![
                "All files in commit (Interactive Line/Hunk Patch)".to_string(),
                "configuration.nix".to_string(),
            ],
            input: Input::default(),
            selected_index: 1,
            return_screen: Box::new(Screen::TopMenu),
            preview_file: String::new(),
            preview_lines: Vec::new(),
            preview_scroll: 0,
        });

        // Ctrl-p on configuration.nix triggers RestorePatch with Some("configuration.nix")
        app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
        assert!(matches!(
            app.pending_external_task,
            ExternalTask::RestorePatch(ref h, Some(ref f)) if h == "abcdef1" && f == "configuration.nix"
        ));
    }

    #[test]
    fn test_file_filter_esc_returns() {
        let mut app = App::new();
        app.screen = Screen::FileFilter(FileFilterState {
            commit_hash: "abcdef1".to_string(),
            files: vec!["configuration.nix".to_string()],
            input: Input::default(),
            selected_index: 0,
            return_screen: Box::new(Screen::TopMenu),
            preview_file: String::new(),
            preview_lines: Vec::new(),
            preview_scroll: 0,
        });

        // Esc returns to return_screen (TopMenu)
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(app.screen, Screen::TopMenu));
    }

    #[test]
    fn test_file_filter_head_rollback() {
        let repo_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        if !crate::actions::is_git_repo(repo_dir) || !git::has_parent_commit(repo_dir, "HEAD") {
            return;
        }

        let mut app = App::new();
        app.screen = Screen::FileFilter(FileFilterState {
            commit_hash: "HEAD".to_string(),
            files: vec![
                "All files in commit (Interactive Line/Hunk Patch)".to_string(),
                "configuration.nix".to_string(),
            ],
            input: Input::default(),
            selected_index: 1,
            return_screen: Box::new(Screen::TopMenu),
            preview_file: String::new(),
            preview_lines: Vec::new(),
            preview_scroll: 0,
        });

        // Enter on configuration.nix when commit is HEAD targets HEAD~1 for rollback
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(app.screen, Screen::Confirm(_)));
        if let Screen::Confirm(ref state) = app.screen {
            assert!(state.title.contains("ROLLBACK FILE"));
            assert!(
                matches!(state.on_confirm, PendingAction::RestoreFileExecute(ref h, ref f) if h == "HEAD~1" && f == "configuration.nix")
            );
        }
    }

    #[test]
    fn test_selective_update_include_flow() {
        let mut app = App::new();
        let inputs = vec![
            crate::actions::nix::FlakeInput {
                name: "nixpkgs".to_string(),
                details: "NixOS/nixpkgs@abcdef1".to_string(),
            },
            crate::actions::nix::FlakeInput {
                name: "home-manager".to_string(),
                details: "nix-community/home-manager@1234567".to_string(),
            },
            crate::actions::nix::FlakeInput {
                name: "hyprland".to_string(),
                details: "hyprwm/Hyprland".to_string(),
            },
        ];

        app.screen = Screen::SelectiveUpdate(SelectiveUpdateState {
            mode: SelectiveMode::Include,
            inputs,
            selected: vec![false, false, false],
            cursor: 0,
            return_screen: Box::new(Screen::TopMenu),
        });

        // Space toggles index 0 (nixpkgs) and advances cursor down to 1 (yazi-style)
        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        if let Screen::SelectiveUpdate(ref state) = app.screen {
            assert!(state.selected[0]);
            assert!(!state.selected[1]);
            assert!(!state.selected[2]);
            assert_eq!(state.cursor, 1);
        }

        // Enter submits selection
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(app.screen, Screen::Confirm(_)));
        if let Screen::Confirm(ref state) = app.screen {
            assert_eq!(state.selected_button, 0);
            assert!(matches!(
                state.on_confirm,
                PendingAction::SelectiveUpdateLockfile(ref inps) if inps == &vec!["nixpkgs".to_string()]
            ));
            assert!(matches!(
                state.on_secondary,
                Some(PendingAction::SelectiveFullCycle(ref inps)) if inps == &vec!["nixpkgs".to_string()]
            ));
        }

        // Select button 0 (Lockfile only)
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(
            app.pending_external_task,
            ExternalTask::SelectiveUpdateFlakeOnly(ref inps) if inps == &vec!["nixpkgs".to_string()]
        ));
    }

    #[test]
    fn test_selective_update_exclude_flow() {
        let mut app = App::new();
        app.is_git = true;
        let inputs = vec![
            crate::actions::nix::FlakeInput {
                name: "nixpkgs".to_string(),
                details: "NixOS/nixpkgs@abcdef1".to_string(),
            },
            crate::actions::nix::FlakeInput {
                name: "home-manager".to_string(),
                details: "nix-community/home-manager@1234567".to_string(),
            },
        ];

        app.screen = Screen::SelectiveUpdate(SelectiveUpdateState {
            mode: SelectiveMode::Exclude,
            inputs,
            selected: vec![false, false],
            cursor: 0,
            return_screen: Box::new(Screen::TopMenu),
        });

        // In Exclude mode, check nixpkgs (means exclude nixpkgs)
        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        if let Screen::SelectiveUpdate(ref state) = app.screen {
            assert!(state.selected[0]);
        }

        // Enter submits: target should be everything NOT selected (home-manager)
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(app.screen, Screen::Confirm(_)));
        if let Screen::Confirm(ref state) = app.screen {
            assert!(matches!(
                state.on_confirm,
                PendingAction::SelectiveUpdateLockfile(ref inps) if inps == &vec!["home-manager".to_string()]
            ));
        }

        // Switch to button 1 (Full Cycle) and press Enter
        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        if let Screen::Confirm(ref state) = app.screen {
            assert_eq!(state.selected_button, 1);
        }

        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        // If is_git is true, it prompts for commit message via InputModal
        assert!(matches!(app.screen, Screen::InputModal(_)));
        // Pressing Enter on InputModal submits the message and triggers SelectiveFullCycleCommitAndSwitch
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(
            app.pending_external_task,
            ExternalTask::SelectiveFullCycleCommitAndSwitch(ref inps, _) if inps == &vec!["home-manager".to_string()]
        ));
    }

    #[test]
    fn test_selective_update_non_git_full_cycle() {
        let mut app = App::new();
        app.is_git = false;

        app.screen = Screen::Confirm(ConfirmState {
            title: "Test".to_string(),
            lines: vec![],
            affirmative_label: "Lockfile".to_string(),
            negative_label: "Full Cycle".to_string(),
            selected_button: 1,
            is_danger: false,
            on_confirm: PendingAction::SelectiveUpdateLockfile(vec!["nixpkgs".to_string()]),
            on_secondary: Some(PendingAction::SelectiveFullCycle(vec![
                "nixpkgs".to_string(),
            ])),
            return_screen: Box::new(Screen::TopMenu),
            on_cancel_screen: None,
        });

        // When is_git is false, selecting button 1 directly sets SelectiveFullCycleSwitchOnly
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(
            app.pending_external_task,
            ExternalTask::SelectiveFullCycleSwitchOnly(ref inps) if inps == &vec!["nixpkgs".to_string()]
        ));
    }

    #[test]
    fn test_selective_update_hotkeys() {
        let mut app = App::new();
        let inputs = vec![
            crate::actions::nix::FlakeInput {
                name: "pkg1".to_string(),
                details: String::new(),
            },
            crate::actions::nix::FlakeInput {
                name: "pkg2".to_string(),
                details: String::new(),
            },
        ];

        app.screen = Screen::SelectiveUpdate(SelectiveUpdateState {
            mode: SelectiveMode::Include,
            inputs,
            selected: vec![false, false],
            cursor: 0,
            return_screen: Box::new(Screen::TopMenu),
        });

        // 'a' selects all
        app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        if let Screen::SelectiveUpdate(ref state) = app.screen {
            assert_eq!(state.selected, vec![true, true]);
        }

        // 'n' deselects all
        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        if let Screen::SelectiveUpdate(ref state) = app.screen {
            assert_eq!(state.selected, vec![false, false]);
        }

        // 'i' inverts selection
        app.handle_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
        if let Screen::SelectiveUpdate(ref state) = app.screen {
            assert_eq!(state.selected, vec![true, true]);
        }

        // Submitting when empty shows error
        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(app.screen, Screen::Result(ref res) if !res.is_success));
    }

    #[test]
    fn test_selective_update_space_advances_cursor() {
        let mut app = App::new();
        let inputs = vec![
            crate::actions::nix::FlakeInput {
                name: "pkg1".to_string(),
                details: String::new(),
            },
            crate::actions::nix::FlakeInput {
                name: "pkg2".to_string(),
                details: String::new(),
            },
            crate::actions::nix::FlakeInput {
                name: "pkg3".to_string(),
                details: String::new(),
            },
        ];

        app.screen = Screen::SelectiveUpdate(SelectiveUpdateState {
            mode: SelectiveMode::Include,
            inputs,
            selected: vec![false, false, false],
            cursor: 0,
            return_screen: Box::new(Screen::TopMenu),
        });

        // First space toggles pkg1 and advances cursor to 1
        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        if let Screen::SelectiveUpdate(ref state) = app.screen {
            assert_eq!(state.selected, vec![true, false, false]);
            assert_eq!(state.cursor, 1);
        }

        // Second space toggles pkg2 and advances cursor to 2
        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        if let Screen::SelectiveUpdate(ref state) = app.screen {
            assert_eq!(state.selected, vec![true, true, false]);
            assert_eq!(state.cursor, 2);
        }

        // Third space toggles pkg3 and stays at cursor 2 (last item)
        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        if let Screen::SelectiveUpdate(ref state) = app.screen {
            assert_eq!(state.selected, vec![true, true, true]);
            assert_eq!(state.cursor, 2);
        }
    }
}
