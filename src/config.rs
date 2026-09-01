use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Keybinding configuration for Flaker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeybindingsConfig {
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
}

/// Global Flaker application configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
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
            let default_template = r#"# Flaker Configuration File
# Location: ~/.config/flaker/config.toml

[keybindings]
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
"#;
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
            [keybindings]
            up = ["Up", "w"]
            down = ["Down", "s"]
        "#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.keybindings.up, vec!["Up", "w"]);
        assert_eq!(cfg.keybindings.down, vec!["Down", "s"]);
        assert_eq!(cfg.keybindings.select, vec!["Enter"]);
    }
}
