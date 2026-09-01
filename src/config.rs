use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// General configuration for Flaker (explicit flake directories and targets).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default)]
    pub flake_dir: String,

    #[serde(default)]
    pub flake_target: String,
}

/// Theme and color configuration for Flaker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    #[serde(default = "default_palette")]
    pub palette: String,

    pub border: Option<String>,
    pub header_title: Option<String>,
    pub secondary_info: Option<String>,
    pub muted_text: Option<String>,
    pub faint_hint: Option<String>,
    pub warning: Option<String>,
    pub danger: Option<String>,
    pub success: Option<String>,
    pub accent: Option<String>,
    pub selected: Option<String>,
    pub text: Option<String>,
    pub neutral_text: Option<String>,
}

fn default_palette() -> String {
    "mocha".to_string()
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            palette: default_palette(),
            border: None,
            header_title: None,
            secondary_info: None,
            muted_text: None,
            faint_hint: None,
            warning: None,
            danger: None,
            success: None,
            accent: None,
            selected: None,
            text: None,
            neutral_text: None,
        }
    }
}

/// Default commit message templates for automatic actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitTemplatesConfig {
    #[serde(default = "default_commit_rebuild")]
    pub rebuild: String,

    #[serde(default = "default_commit_flake_update")]
    pub flake_update: String,

    #[serde(default = "default_commit_full_cycle")]
    pub full_cycle: String,

    #[serde(default = "default_commit_soft_revert")]
    pub soft_revert: String,

    #[serde(default = "default_commit_trim_history")]
    pub trim_history: String,
}

fn default_commit_rebuild() -> String {
    "rebuild".to_string()
}

fn default_commit_flake_update() -> String {
    "flake update".to_string()
}

fn default_commit_full_cycle() -> String {
    "full update".to_string()
}

fn default_commit_soft_revert() -> String {
    "revert to {hash}".to_string()
}

fn default_commit_trim_history() -> String {
    "trim history to {hash}".to_string()
}

impl Default for CommitTemplatesConfig {
    fn default() -> Self {
        Self {
            rebuild: default_commit_rebuild(),
            flake_update: default_commit_flake_update(),
            full_cycle: default_commit_full_cycle(),
            soft_revert: default_commit_soft_revert(),
            trim_history: default_commit_trim_history(),
        }
    }
}

/// Keybinding configuration for Flaker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeybindingsConfig {
    #[serde(default = "default_enable_quick_digits")]
    pub enable_quick_digits: bool,

    #[serde(default = "default_back_item_key")]
    pub back_item_key: String,

    #[serde(default = "default_up")]
    pub up: Vec<String>,

    #[serde(default = "default_down")]
    pub down: Vec<String>,

    #[serde(default = "default_page_up")]
    pub page_up: Vec<String>,

    #[serde(default = "default_page_down")]
    pub page_down: Vec<String>,

    #[serde(default = "default_home")]
    pub home: Vec<String>,

    #[serde(default = "default_end")]
    pub end: Vec<String>,

    #[serde(default = "default_select")]
    pub select: Vec<String>,

    #[serde(default = "default_back")]
    pub back: Vec<String>,

    #[serde(default = "default_quit")]
    pub quit: Vec<String>,

    #[serde(default = "default_clear_input")]
    pub clear_input: Vec<String>,
}

fn default_enable_quick_digits() -> bool {
    true
}

fn default_back_item_key() -> String {
    "q".to_string()
}

fn default_up() -> Vec<String> {
    vec!["Up".to_string(), "k".to_string()]
}

fn default_down() -> Vec<String> {
    vec!["Down".to_string(), "j".to_string()]
}

fn default_page_up() -> Vec<String> {
    vec!["PageUp".to_string()]
}

fn default_page_down() -> Vec<String> {
    vec!["PageDown".to_string()]
}

fn default_home() -> Vec<String> {
    vec!["Home".to_string()]
}

fn default_end() -> Vec<String> {
    vec!["End".to_string()]
}

fn default_select() -> Vec<String> {
    vec!["Enter".to_string()]
}

fn default_back() -> Vec<String> {
    vec!["Esc".to_string()]
}

fn default_quit() -> Vec<String> {
    vec!["q".to_string(), "Ctrl-c".to_string()]
}

fn default_clear_input() -> Vec<String> {
    vec!["Ctrl-u".to_string()]
}

impl Default for KeybindingsConfig {
    fn default() -> Self {
        Self {
            enable_quick_digits: default_enable_quick_digits(),
            back_item_key: default_back_item_key(),
            up: default_up(),
            down: default_down(),
            page_up: default_page_up(),
            page_down: default_page_down(),
            home: default_home(),
            end: default_end(),
            select: default_select(),
            back: default_back(),
            quit: default_quit(),
            clear_input: default_clear_input(),
        }
    }
}

impl KeybindingsConfig {
    pub fn is_up(&self, key: &KeyEvent) -> bool {
        self.up.iter().any(|k| matches_key(key, k))
    }

    pub fn is_down(&self, key: &KeyEvent) -> bool {
        self.down.iter().any(|k| matches_key(key, k))
    }

    pub fn is_page_up(&self, key: &KeyEvent) -> bool {
        self.page_up.iter().any(|k| matches_key(key, k))
    }

    pub fn is_page_down(&self, key: &KeyEvent) -> bool {
        self.page_down.iter().any(|k| matches_key(key, k))
    }

    pub fn is_home(&self, key: &KeyEvent) -> bool {
        self.home.iter().any(|k| matches_key(key, k))
    }

    pub fn is_end(&self, key: &KeyEvent) -> bool {
        self.end.iter().any(|k| matches_key(key, k))
    }

    pub fn is_select(&self, key: &KeyEvent) -> bool {
        self.select.iter().any(|k| matches_key(key, k))
    }

    pub fn is_back(&self, key: &KeyEvent) -> bool {
        self.back.iter().any(|k| matches_key(key, k))
    }

    pub fn is_quit(&self, key: &KeyEvent) -> bool {
        self.quit.iter().any(|k| matches_key(key, k))
    }

    pub fn is_clear_input(&self, key: &KeyEvent) -> bool {
        self.clear_input.iter().any(|k| matches_key(key, k))
    }

    pub fn is_back_item_key(&self, key: &KeyEvent) -> bool {
        if self.back_item_key.trim().is_empty() {
            return false;
        }
        matches_key(key, &self.back_item_key)
    }

    /// If quick digits are enabled and the pressed key is a digit '1'-'9', returns the 0-based index.
    pub fn get_digit_index(&self, key: &KeyEvent) -> Option<usize> {
        if !self.enable_quick_digits {
            return None;
        }

        if !key.modifiers.contains(KeyModifiers::CONTROL)
            && !key.modifiers.contains(KeyModifiers::ALT)
            && let KeyCode::Char(c) = key.code
            && let Some(digit) = c.to_digit(10)
            && digit >= 1
        {
            return Some((digit - 1) as usize);
        }

        None
    }

    pub fn nav_hint(&self) -> String {
        let up_s = format_keys_display(&self.up);
        let down_s = format_keys_display(&self.down);
        format!("[{up_s}/{down_s}] Navigate")
    }

    pub fn menu_hint(&self, count: usize, is_top_menu: bool) -> String {
        let nav = self.nav_hint();
        let sel = format_keys_display(&self.select);
        let back_lbl = if is_top_menu { "Exit" } else { "Back" };
        let back_hint = if !self.back_item_key.trim().is_empty() {
            format!("[{}] {back_lbl}", self.back_item_key.trim())
        } else {
            format!("[{}] {back_lbl}", format_keys_display(&self.back))
        };

        let action_count = count.saturating_sub(1);
        if self.enable_quick_digits && action_count > 0 {
            if action_count == 1 {
                format!("[1] Select  •  {back_hint}  •  {nav}  •  [{sel}] Select")
            } else {
                format!(
                    "[1-{action_count}] Quick select  •  {back_hint}  •  {nav}  •  [{sel}] Select"
                )
            }
        } else {
            format!("{back_hint}  •  {nav}  •  [{sel}] Select")
        }
    }

    pub fn confirm_hint(&self) -> String {
        let sel = format_keys_display(&self.select);
        let b = format_keys_display(&self.back);
        format!("Use [←/→/Tab] to toggle  •  [{sel}] Confirm  •  [{b}] Cancel")
    }

    pub fn input_modal_hint(&self) -> String {
        let sel = format_keys_display(&self.select);
        let b = format_keys_display(&self.back);
        format!("Press [{sel}] to confirm (or use default)  •  [{b}] Cancel")
    }

    pub fn filter_hint(&self) -> String {
        let up_s = format_keys_display(&self.up);
        let down_s = format_keys_display(&self.down);
        let sel = format_keys_display(&self.select);
        let b = format_keys_display(&self.back);
        format!("Type to filter • [{up_s}/{down_s}] Navigate • [{sel}] Select • [{b}] Cancel")
    }

    pub fn pager_hint(&self, current: usize, total: usize) -> String {
        let b = format_keys_display(&self.back);
        let q = format_keys_display(&self.quit);
        let sel = format_keys_display(&self.select);
        let up_s = format_keys_display(&self.up);
        let down_s = format_keys_display(&self.down);
        format!(
            " [{b} / {q} / {sel}] Return  •  [{up_s}/{down_s}/PgUp/PgDn] Scroll ({current}/{total}) "
        )
    }

    pub fn result_hint(&self) -> String {
        let sel = format_keys_display(&self.select);
        format!(" ⏎  Press [{sel}] to return...")
    }
}

fn format_keys_display(keys: &[String]) -> String {
    let mut display_list = Vec::new();
    for k in keys {
        let d = match k.to_lowercase().as_str() {
            "up" => "↑".to_string(),
            "down" => "↓".to_string(),
            "left" => "←".to_string(),
            "right" => "→".to_string(),
            "pageup" | "page_up" => "PgUp".to_string(),
            "pagedown" | "page_down" => "PgDn".to_string(),
            "enter" | "return" => "Enter".to_string(),
            "esc" | "escape" => "Esc".to_string(),
            "space" => "Space".to_string(),
            "tab" => "Tab".to_string(),
            "backspace" => "Backspace".to_string(),
            "ctrl-c" => "Ctrl-c".to_string(),
            "ctrl-u" => "Ctrl-u".to_string(),
            "ctrl-d" => "Ctrl-d".to_string(),
            _ => k.to_string(),
        };
        if !display_list.contains(&d) {
            display_list.push(d);
        }
    }
    display_list.join("/")
}

/// Global Flaker application configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub general: GeneralConfig,

    #[serde(default)]
    pub theme: ThemeConfig,

    #[serde(default)]
    pub commit_templates: CommitTemplatesConfig,

    #[serde(default)]
    pub keybindings: KeybindingsConfig,
}

/// Matches a crossterm KeyEvent against a string representation like "Up", "k", "Ctrl-c", "Enter", "Esc".
pub fn matches_key(key: &KeyEvent, key_str: &str) -> bool {
    let s = key_str.trim();

    if s.eq_ignore_ascii_case("ctrl-c") {
        return key.modifiers.contains(KeyModifiers::CONTROL)
            && (key.code == KeyCode::Char('c') || key.code == KeyCode::Char('C'));
    }
    if s.eq_ignore_ascii_case("ctrl-u") {
        return key.modifiers.contains(KeyModifiers::CONTROL)
            && (key.code == KeyCode::Char('u') || key.code == KeyCode::Char('U'));
    }
    if s.eq_ignore_ascii_case("ctrl-d") {
        return key.modifiers.contains(KeyModifiers::CONTROL)
            && (key.code == KeyCode::Char('d') || key.code == KeyCode::Char('D'));
    }
    if s.eq_ignore_ascii_case("enter") || s.eq_ignore_ascii_case("return") {
        return key.code == KeyCode::Enter;
    }
    if s.eq_ignore_ascii_case("esc") || s.eq_ignore_ascii_case("escape") {
        return key.code == KeyCode::Esc;
    }
    if s.eq_ignore_ascii_case("tab") {
        return key.code == KeyCode::Tab;
    }
    if s.eq_ignore_ascii_case("backspace") {
        return key.code == KeyCode::Backspace;
    }
    if s.eq_ignore_ascii_case("space") {
        return key.code == KeyCode::Char(' ');
    }
    if s.eq_ignore_ascii_case("up") {
        return key.code == KeyCode::Up;
    }
    if s.eq_ignore_ascii_case("down") {
        return key.code == KeyCode::Down;
    }
    if s.eq_ignore_ascii_case("left") {
        return key.code == KeyCode::Left;
    }
    if s.eq_ignore_ascii_case("right") {
        return key.code == KeyCode::Right;
    }
    if s.eq_ignore_ascii_case("pageup") || s.eq_ignore_ascii_case("page_up") {
        return key.code == KeyCode::PageUp;
    }
    if s.eq_ignore_ascii_case("pagedown") || s.eq_ignore_ascii_case("page_down") {
        return key.code == KeyCode::PageDown;
    }
    if s.eq_ignore_ascii_case("home") {
        return key.code == KeyCode::Home;
    }
    if s.eq_ignore_ascii_case("end") {
        return key.code == KeyCode::End;
    }

    // Single character matching without Control / Alt
    if !key.modifiers.contains(KeyModifiers::CONTROL)
        && !key.modifiers.contains(KeyModifiers::ALT)
        && let KeyCode::Char(c) = key.code
        && let Some(target_char) = s.chars().next()
        && s.chars().count() == 1
    {
        return c.eq_ignore_ascii_case(&target_char);
    }

    false
}

/// Locates or creates the configuration file at ~/.config/flaker/config.toml.
pub fn load_config() -> Config {
    let config_path = get_config_path();

    if let Some(ref path) = config_path {
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(path)
                && let Ok(cfg) = toml::from_str::<Config>(&content)
            {
                return cfg;
            }
        } else {
            // Automatically generate a default commented configuration file on first run
            let default_template = r##"# Flaker Configuration File
# Location: ~/.config/flaker/config.toml

[general]
# Explicit flake directory path (leave empty for automatic detection)
# Examples: "/etc/nixos", "~/dotfiles/nixos", "~/my-config"
flake_dir = ""

# Explicit flake target (leave empty for automatic detection)
# Examples: "/etc/nixos#hostname", "~/dotfiles#my-user"
flake_target = ""

[theme]
# Color palette. Available options:
# "mocha" (default dark), "macchiato", "frappe", "latte" (light),
# "nord", "tokyonight", "dracula", "gruvbox"
palette = "mocha"

# Optional custom color overrides (HEX format like "#89b4fa"):
# border = "#89b4fa"
# accent = "#cba6f7"
# selected = "#f9e2af"
# header_title = "#89dceb"

[commit_templates]
# Default commit messages for actions
rebuild = "rebuild"
flake_update = "flake update"
full_cycle = "full update"
soft_revert = "revert to {hash}"
trim_history = "trim history to {hash}"

[keybindings]
# Enable instant single-digit selection (1, 2, 3...) for menu items
enable_quick_digits = true

# Key assigned to the Back / Exit menu item (e.g. "q", "b", "Esc")
back_item_key = "q"

# Navigation keys
up = ["Up", "k"]
down = ["Down", "j"]
page_up = ["PageUp"]
page_down = ["PageDown"]
home = ["Home"]
end = ["End"]

# Action keys
select = ["Enter"]
back = ["Esc"]
quit = ["q", "Ctrl-c"]
clear_input = ["Ctrl-u"]
"##;
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
                let _ = std::fs::write(path, default_template);
            }
        }
    }

    Config::default()
}

/// Returns the primary configuration path (~/.config/flaker/config.toml).
pub fn get_config_path() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("HOME") {
        let standard_dir = PathBuf::from(&home).join(".config").join("flaker");
        let standard_file = standard_dir.join("config.toml");
        if standard_file.exists() {
            return Some(standard_file);
        }

        let alt_file = PathBuf::from(&home).join(".config").join("flaker.toml");
        if alt_file.exists() {
            return Some(alt_file);
        }

        return Some(standard_file);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_key_char() {
        let key = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE);
        assert!(matches_key(&key, "k"));
        assert!(matches_key(&key, "K"));
        assert!(!matches_key(&key, "j"));
    }

    #[test]
    fn test_matches_key_special() {
        let up = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        assert!(matches_key(&up, "Up"));
        assert!(matches_key(&up, "up"));

        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        assert!(matches_key(&enter, "Enter"));

        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        assert!(matches_key(&esc, "Esc"));
    }

    #[test]
    fn test_matches_key_ctrl() {
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(matches_key(&ctrl_c, "Ctrl-c"));
        assert!(matches_key(&ctrl_c, "ctrl-c"));
        assert!(!matches_key(&ctrl_c, "c"));
    }

    #[test]
    fn test_default_config_parsing() {
        let toml_str = r#"
            [general]
            flake_dir = "/etc/nixos"
            flake_target = "/etc/nixos#desktop"

            [theme]
            palette = "nord"

            [commit_templates]
            rebuild = "chore: rebuild system"

            [keybindings]
            enable_quick_digits = true
            back_item_key = "q"
            up = ["Up", "w"]
            down = ["Down", "s"]
        "#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.general.flake_dir, "/etc/nixos");
        assert_eq!(cfg.general.flake_target, "/etc/nixos#desktop");
        assert_eq!(cfg.theme.palette, "nord");
        assert_eq!(cfg.commit_templates.rebuild, "chore: rebuild system");
        assert!(cfg.keybindings.enable_quick_digits);
        assert_eq!(cfg.keybindings.back_item_key, "q");
        assert_eq!(cfg.keybindings.up, vec!["Up", "w"]);
        assert_eq!(cfg.keybindings.down, vec!["Down", "s"]);
        assert_eq!(cfg.keybindings.select, vec!["Enter"]);
    }

    #[test]
    fn test_get_digit_index() {
        let kb = KeybindingsConfig::default();
        let key1 = KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE);
        let key2 = KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE);
        let key0 = KeyEvent::new(KeyCode::Char('0'), KeyModifiers::NONE);
        let key_a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);

        assert_eq!(kb.get_digit_index(&key1), Some(0));
        assert_eq!(kb.get_digit_index(&key2), Some(1));
        assert_eq!(kb.get_digit_index(&key0), None);
        assert_eq!(kb.get_digit_index(&key_a), None);
    }

    #[test]
    fn test_dynamic_hints() {
        let kb = KeybindingsConfig {
            up: vec!["w".to_string()],
            down: vec!["s".to_string()],
            select: vec!["Space".to_string()],
            back: vec!["Esc".to_string()],
            back_item_key: "q".to_string(),
            ..Default::default()
        };

        assert_eq!(kb.nav_hint(), "[w/s] Navigate");
        assert_eq!(
            kb.menu_hint(4, true),
            "[1-3] Quick select  •  [q] Exit  •  [w/s] Navigate  •  [Space] Select"
        );
        assert_eq!(
            kb.menu_hint(5, false),
            "[1-4] Quick select  •  [q] Back  •  [w/s] Navigate  •  [Space] Select"
        );
        assert_eq!(
            kb.confirm_hint(),
            "Use [←/→/Tab] to toggle  •  [Space] Confirm  •  [Esc] Cancel"
        );
        assert_eq!(
            kb.input_modal_hint(),
            "Press [Space] to confirm (or use default)  •  [Esc] Cancel"
        );
        assert_eq!(
            kb.filter_hint(),
            "Type to filter • [w/s] Navigate • [Space] Select • [Esc] Cancel"
        );
    }
}
