use catppuccin::PALETTE;
use ratatui::style::Color;
use std::path::{Path, PathBuf};

use crate::config::{Config, PaletteConfig, ThemeConfig};

const fn cat_to_rat(c: catppuccin::Color) -> Color {
    Color::Rgb(c.rgb.r, c.rgb.g, c.rgb.b)
}

/// Parses a hex color string like "#89b4fa", "89b4fa", or 3-digit "#fff" into ratatui Color::Rgb.
pub fn parse_hex_color(hex: &str) -> Option<Color> {
    let s = hex.trim().trim_start_matches('#');
    if s.len() == 6 && s.is_ascii() {
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        Some(Color::Rgb(r, g, b))
    } else if s.len() == 3 && s.is_ascii() {
        let r_val = u8::from_str_radix(&s[0..1], 16).ok()?;
        let g_val = u8::from_str_radix(&s[1..2], 16).ok()?;
        let b_val = u8::from_str_radix(&s[2..3], 16).ok()?;
        Some(Color::Rgb(
            r_val.saturating_mul(17),
            g_val.saturating_mul(17),
            b_val.saturating_mul(17),
        ))
    } else {
        None
    }
}

/// Parses either a hex color or a standard named color (e.g. "cyan", "red", "white", "#89b4fa").
pub fn parse_color(val: &str) -> Option<Color> {
    let s = val.trim();
    match s.to_lowercase().as_str() {
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "white" => Some(Color::White),
        "gray" | "grey" => Some(Color::Gray),
        "darkgray" | "darkgrey" | "dark_gray" | "dark_grey" => Some(Color::DarkGray),
        _ => parse_hex_color(s),
    }
}

/// Converts a ratatui Color into an ANSI 24-bit/indexed foreground escape string.
pub fn to_ansi_fg(color: Color) -> String {
    match color {
        Color::Rgb(r, g, b) => format!("\x1b[38;2;{r};{g};{b}m"),
        Color::Indexed(i) => format!("\x1b[38;5;{i}m"),
        Color::Red => "\x1b[31m".to_string(),
        Color::Green => "\x1b[32m".to_string(),
        Color::Yellow => "\x1b[33m".to_string(),
        Color::Blue => "\x1b[34m".to_string(),
        Color::Magenta => "\x1b[35m".to_string(),
        Color::Cyan => "\x1b[36m".to_string(),
        Color::White => "\x1b[37m".to_string(),
        Color::Black => "\x1b[30m".to_string(),
        _ => "\x1b[39m".to_string(),
    }
}

/// Dynamic theme configuration supporting Catppuccin, Nord, Tokyo Night, Dracula, Gruvbox, and custom hex overrides.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub border: Color,
    pub header_title: Color,
    pub secondary_info: Color,
    pub muted_text: Color,
    pub faint_hint: Color,
    pub warning: Color,
    pub danger: Color,
    pub success: Color,
    pub accent: Color,
    pub selected: Color,
    pub text: Color,
    pub neutral_text: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self::mocha()
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
enum StandalonePaletteFile {
    Wrapped { palette: PaletteConfig },
    Direct(PaletteConfig),
}

impl StandalonePaletteFile {
    fn into_palette_config(self) -> PaletteConfig {
        match self {
            Self::Wrapped { palette } => palette,
            Self::Direct(palette) => palette,
        }
    }
}

/// Loads a custom palette from a standalone TOML file.
pub fn load_palette_file(path: &Path) -> Option<PaletteConfig> {
    if !path.is_file() {
        return None;
    }
    let content = std::fs::read_to_string(path).ok()?;
    match toml::from_str::<StandalonePaletteFile>(&content) {
        Ok(file) => Some(file.into_palette_config()),
        Err(err) => {
            eprintln!(
                "⚠ Warning: Failed to parse palette file {}: {err}",
                path.display()
            );
            None
        }
    }
}

/// Returns standard directories where theme files (.toml) are searched.
pub fn get_theme_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.trim().is_empty()
    {
        let xdg_p = PathBuf::from(xdg.trim()).join("flaker");
        dirs.push(xdg_p.join("themes"));
        dirs.push(xdg_p.join("palettes"));
    }

    if let Ok(home) = std::env::var("HOME")
        && !home.trim().is_empty()
    {
        let home_p = PathBuf::from(home.trim());
        dirs.push(home_p.join(".config").join("flaker").join("themes"));
        dirs.push(home_p.join(".config").join("flaker").join("palettes"));
        dirs.push(home_p.join(".flaker").join("themes"));
        dirs.push(home_p.join(".flaker").join("palettes"));
    }

    dirs
}

/// Finds a theme file either by direct path or by searching standard theme directories.
pub fn find_theme_file(name: &str) -> Option<PathBuf> {
    let clean = name.trim();
    if clean.is_empty() {
        return None;
    }

    if clean.contains('/')
        || clean.contains('\\')
        || clean.starts_with('~')
        || clean.ends_with(".toml")
    {
        let path = crate::actions::expand_tilde(clean);
        if path.is_file() {
            return Some(path);
        }
    }

    let file_names = [
        format!("{clean}.toml"),
        format!("{}.toml", clean.to_lowercase()),
    ];

    for dir in get_theme_search_dirs() {
        for fn_name in &file_names {
            let candidate = dir.join(fn_name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

/// Looks up a custom palette in memory from `config.palettes` or `config.theme.palettes` (case-insensitive).
pub fn find_palette_in_config(name: &str, config: &Config) -> Option<PaletteConfig> {
    for (k, v) in &config.palettes {
        if k.eq_ignore_ascii_case(name) {
            return Some(v.clone());
        }
    }
    for (k, v) in &config.theme.palettes {
        if k.eq_ignore_ascii_case(name) {
            return Some(v.clone());
        }
    }
    None
}

impl Theme {
    /// Resolves a theme from application configuration.
    ///
    /// Resolution order:
    /// 1. Custom palette defined in `[palettes.<name>]` or `[theme.palettes.<name>]`
    /// 2. Standalone palette file in `~/.config/flaker/themes/<name>.toml` or explicit file path
    /// 3. Built-in palettes: "mocha", "macchiato", "frappe", "latte", "nord", "tokyonight", "dracula", "gruvbox"
    /// 4. Fallback to "mocha" with a warning if the named palette is unknown.
    ///
    /// Finally, any individual color overrides from `[theme]` are applied on top.
    pub fn from_config(config: &Config) -> Self {
        let palette_name = config.theme.palette.trim();
        let mut theme = Self::resolve_palette(palette_name, config, 0);
        theme.apply_theme_config(&config.theme);
        theme
    }

    /// Resolves a theme from `ThemeConfig`.
    pub fn from_theme_config(config: &ThemeConfig) -> Self {
        let default_config = Config {
            theme: config.clone(),
            ..Default::default()
        };
        Self::from_config(&default_config)
    }

    fn resolve_palette(name: &str, config: &Config, depth: usize) -> Self {
        let clean = name.trim();
        if depth > 5 {
            return Self::mocha();
        }

        // 1. In-memory custom palette
        if let Some(custom) = find_palette_in_config(clean, config) {
            let mut base_theme = if let Some(ref base_name) = custom.base {
                Self::resolve_palette(base_name, config, depth + 1)
            } else {
                Self::mocha()
            };
            base_theme.apply_palette_config(&custom);
            return base_theme;
        }

        // 2. Standalone theme file
        if let Some(file_path) = find_theme_file(clean)
            && let Some(file_palette) = load_palette_file(&file_path)
        {
            let mut base_theme = if let Some(ref base_name) = file_palette.base {
                Self::resolve_palette(base_name, config, depth + 1)
            } else {
                Self::mocha()
            };
            base_theme.apply_palette_config(&file_palette);
            return base_theme;
        }

        // 3. Built-in palettes
        if let Some(builtin) = Self::builtin(clean) {
            return builtin;
        }

        // 4. Fallback
        if !clean.is_empty()
            && !clean.eq_ignore_ascii_case("mocha")
            && !clean.eq_ignore_ascii_case("default")
        {
            eprintln!("⚠ Warning: Palette '{clean}' not found. Using default 'mocha'.");
        }
        Self::mocha()
    }

    /// Returns a built-in theme by name, or None if unknown.
    pub fn builtin(name: &str) -> Option<Self> {
        match name.to_lowercase().trim() {
            "mocha" => Some(Self::mocha()),
            "macchiato" => Some(Self::macchiato()),
            "frappe" | "frappé" => Some(Self::frappe()),
            "latte" => Some(Self::latte()),
            "nord" => Some(Self::nord()),
            "tokyonight" | "tokyo-night" => Some(Self::tokyonight()),
            "dracula" => Some(Self::dracula()),
            "gruvbox" => Some(Self::gruvbox()),
            _ => None,
        }
    }

    /// Applies individual color overrides from a PaletteConfig.
    pub fn apply_palette_config(&mut self, cfg: &PaletteConfig) {
        if let Some(ref val) = cfg.border
            && let Some(c) = parse_color(val)
        {
            self.border = c;
        }
        if let Some(ref val) = cfg.header_title
            && let Some(c) = parse_color(val)
        {
            self.header_title = c;
        }
        if let Some(ref val) = cfg.secondary_info
            && let Some(c) = parse_color(val)
        {
            self.secondary_info = c;
        }
        if let Some(ref val) = cfg.muted_text
            && let Some(c) = parse_color(val)
        {
            self.muted_text = c;
        }
        if let Some(ref val) = cfg.faint_hint
            && let Some(c) = parse_color(val)
        {
            self.faint_hint = c;
        }
        if let Some(ref val) = cfg.warning
            && let Some(c) = parse_color(val)
        {
            self.warning = c;
        }
        if let Some(ref val) = cfg.danger
            && let Some(c) = parse_color(val)
        {
            self.danger = c;
        }
        if let Some(ref val) = cfg.success
            && let Some(c) = parse_color(val)
        {
            self.success = c;
        }
        if let Some(ref val) = cfg.accent
            && let Some(c) = parse_color(val)
        {
            self.accent = c;
        }
        if let Some(ref val) = cfg.selected
            && let Some(c) = parse_color(val)
        {
            self.selected = c;
        }
        if let Some(ref val) = cfg.text
            && let Some(c) = parse_color(val)
        {
            self.text = c;
        }
        if let Some(ref val) = cfg.neutral_text
            && let Some(c) = parse_color(val)
        {
            self.neutral_text = c;
        }
    }

    /// Applies individual color overrides from a ThemeConfig.
    pub fn apply_theme_config(&mut self, cfg: &ThemeConfig) {
        if let Some(ref val) = cfg.border
            && let Some(c) = parse_color(val)
        {
            self.border = c;
        }
        if let Some(ref val) = cfg.header_title
            && let Some(c) = parse_color(val)
        {
            self.header_title = c;
        }
        if let Some(ref val) = cfg.secondary_info
            && let Some(c) = parse_color(val)
        {
            self.secondary_info = c;
        }
        if let Some(ref val) = cfg.muted_text
            && let Some(c) = parse_color(val)
        {
            self.muted_text = c;
        }
        if let Some(ref val) = cfg.faint_hint
            && let Some(c) = parse_color(val)
        {
            self.faint_hint = c;
        }
        if let Some(ref val) = cfg.warning
            && let Some(c) = parse_color(val)
        {
            self.warning = c;
        }
        if let Some(ref val) = cfg.danger
            && let Some(c) = parse_color(val)
        {
            self.danger = c;
        }
        if let Some(ref val) = cfg.success
            && let Some(c) = parse_color(val)
        {
            self.success = c;
        }
        if let Some(ref val) = cfg.accent
            && let Some(c) = parse_color(val)
        {
            self.accent = c;
        }
        if let Some(ref val) = cfg.selected
            && let Some(c) = parse_color(val)
        {
            self.selected = c;
        }
        if let Some(ref val) = cfg.text
            && let Some(c) = parse_color(val)
        {
            self.text = c;
        }
        if let Some(ref val) = cfg.neutral_text
            && let Some(c) = parse_color(val)
        {
            self.neutral_text = c;
        }
    }

    pub const fn mocha() -> Self {
        Self {
            border: cat_to_rat(PALETTE.mocha.colors.blue),
            header_title: cat_to_rat(PALETTE.mocha.colors.sky),
            secondary_info: cat_to_rat(PALETTE.mocha.colors.sapphire),
            muted_text: cat_to_rat(PALETTE.mocha.colors.subtext0),
            faint_hint: cat_to_rat(PALETTE.mocha.colors.overlay0),
            warning: cat_to_rat(PALETTE.mocha.colors.peach),
            danger: cat_to_rat(PALETTE.mocha.colors.red),
            success: cat_to_rat(PALETTE.mocha.colors.green),
            accent: cat_to_rat(PALETTE.mocha.colors.mauve),
            selected: cat_to_rat(PALETTE.mocha.colors.yellow),
            text: cat_to_rat(PALETTE.mocha.colors.text),
            neutral_text: cat_to_rat(PALETTE.mocha.colors.subtext1),
        }
    }

    pub const fn macchiato() -> Self {
        Self {
            border: cat_to_rat(PALETTE.macchiato.colors.blue),
            header_title: cat_to_rat(PALETTE.macchiato.colors.sky),
            secondary_info: cat_to_rat(PALETTE.macchiato.colors.sapphire),
            muted_text: cat_to_rat(PALETTE.macchiato.colors.subtext0),
            faint_hint: cat_to_rat(PALETTE.macchiato.colors.overlay0),
            warning: cat_to_rat(PALETTE.macchiato.colors.peach),
            danger: cat_to_rat(PALETTE.macchiato.colors.red),
            success: cat_to_rat(PALETTE.macchiato.colors.green),
            accent: cat_to_rat(PALETTE.macchiato.colors.mauve),
            selected: cat_to_rat(PALETTE.macchiato.colors.yellow),
            text: cat_to_rat(PALETTE.macchiato.colors.text),
            neutral_text: cat_to_rat(PALETTE.macchiato.colors.subtext1),
        }
    }

    pub const fn frappe() -> Self {
        Self {
            border: cat_to_rat(PALETTE.frappe.colors.blue),
            header_title: cat_to_rat(PALETTE.frappe.colors.sky),
            secondary_info: cat_to_rat(PALETTE.frappe.colors.sapphire),
            muted_text: cat_to_rat(PALETTE.frappe.colors.subtext0),
            faint_hint: cat_to_rat(PALETTE.frappe.colors.overlay0),
            warning: cat_to_rat(PALETTE.frappe.colors.peach),
            danger: cat_to_rat(PALETTE.frappe.colors.red),
            success: cat_to_rat(PALETTE.frappe.colors.green),
            accent: cat_to_rat(PALETTE.frappe.colors.mauve),
            selected: cat_to_rat(PALETTE.frappe.colors.yellow),
            text: cat_to_rat(PALETTE.frappe.colors.text),
            neutral_text: cat_to_rat(PALETTE.frappe.colors.subtext1),
        }
    }

    pub const fn latte() -> Self {
        Self {
            border: cat_to_rat(PALETTE.latte.colors.blue),
            header_title: cat_to_rat(PALETTE.latte.colors.sky),
            secondary_info: cat_to_rat(PALETTE.latte.colors.sapphire),
            muted_text: cat_to_rat(PALETTE.latte.colors.subtext0),
            faint_hint: cat_to_rat(PALETTE.latte.colors.overlay0),
            warning: cat_to_rat(PALETTE.latte.colors.peach),
            danger: cat_to_rat(PALETTE.latte.colors.red),
            success: cat_to_rat(PALETTE.latte.colors.green),
            accent: cat_to_rat(PALETTE.latte.colors.mauve),
            selected: cat_to_rat(PALETTE.latte.colors.yellow),
            text: cat_to_rat(PALETTE.latte.colors.text),
            neutral_text: cat_to_rat(PALETTE.latte.colors.subtext1),
        }
    }

    pub const fn nord() -> Self {
        Self {
            border: Color::Rgb(136, 192, 208),         // #88C0D0 (Frost blue)
            header_title: Color::Rgb(143, 188, 187),   // #8FBCBB (Frost teal)
            secondary_info: Color::Rgb(129, 161, 193), // #81A1C1
            muted_text: Color::Rgb(216, 222, 233),     // #D8DEE9
            faint_hint: Color::Rgb(76, 86, 106),       // #4C566A
            warning: Color::Rgb(208, 135, 112),        // #D08770 (Orange)
            danger: Color::Rgb(191, 97, 106),          // #BF616A (Red)
            success: Color::Rgb(163, 190, 140),        // #A3BE8C (Green)
            accent: Color::Rgb(180, 142, 173),         // #B48EAD (Purple)
            selected: Color::Rgb(235, 203, 139),       // #EBCB8B (Yellow)
            text: Color::Rgb(236, 239, 244),           // #ECEFF4
            neutral_text: Color::Rgb(229, 233, 240),   // #E5E9F0
        }
    }

    pub const fn tokyonight() -> Self {
        Self {
            border: Color::Rgb(122, 162, 247),        // #7aa2f7 (Blue)
            header_title: Color::Rgb(125, 207, 255),  // #7dcfff (Cyan)
            secondary_info: Color::Rgb(42, 195, 222), // #2ac3de
            muted_text: Color::Rgb(169, 177, 214),    // #a9b1d6
            faint_hint: Color::Rgb(86, 95, 137),      // #565f89
            warning: Color::Rgb(255, 158, 100),       // #ff9e64 (Orange)
            danger: Color::Rgb(247, 118, 142),        // #f7768e (Red)
            success: Color::Rgb(158, 206, 106),       // #9ece6a (Green)
            accent: Color::Rgb(187, 154, 247),        // #bb9af7 (Purple)
            selected: Color::Rgb(224, 175, 104),      // #e0af68 (Yellow)
            text: Color::Rgb(192, 202, 245),          // #c0caf5
            neutral_text: Color::Rgb(154, 165, 206),  // #9aa5ce
        }
    }

    pub const fn dracula() -> Self {
        Self {
            border: Color::Rgb(189, 147, 249),        // #bd93f9 (Purple)
            header_title: Color::Rgb(139, 233, 253),  // #8be9fd (Cyan)
            secondary_info: Color::Rgb(98, 114, 164), // #6272a4 (Comment)
            muted_text: Color::Rgb(191, 191, 191),    // #bfbfbf
            faint_hint: Color::Rgb(98, 114, 164),     // #6272a4
            warning: Color::Rgb(255, 184, 108),       // #ffb86c (Orange)
            danger: Color::Rgb(255, 85, 85),          // #ff5555 (Red)
            success: Color::Rgb(80, 250, 123),        // #50fa7b (Green)
            accent: Color::Rgb(255, 121, 198),        // #ff79c6 (Pink)
            selected: Color::Rgb(241, 250, 140),      // #f1fa8c (Yellow)
            text: Color::Rgb(248, 248, 242),          // #f8f8f2 (Foreground)
            neutral_text: Color::Rgb(226, 226, 220),  // #e2e2dc
        }
    }

    pub const fn gruvbox() -> Self {
        Self {
            border: Color::Rgb(131, 165, 152),        // #83a598 (Blue)
            header_title: Color::Rgb(142, 192, 124),  // #8ec07c (Aqua)
            secondary_info: Color::Rgb(177, 98, 134), // #b16286 (Purple)
            muted_text: Color::Rgb(213, 196, 161),    // #d5c4a1
            faint_hint: Color::Rgb(146, 131, 116),    // #928374
            warning: Color::Rgb(254, 128, 25),        // #fe8019 (Orange)
            danger: Color::Rgb(251, 73, 52),          // #fb4934 (Red)
            success: Color::Rgb(184, 187, 38),        // #b8bb26 (Green)
            accent: Color::Rgb(211, 134, 155),        // #d3869b (Light purple)
            selected: Color::Rgb(250, 189, 47),       // #fabd2f (Yellow)
            text: Color::Rgb(235, 219, 178),          // #ebdbb2
            neutral_text: Color::Rgb(189, 174, 147),  // #bdae93
        }
    }
}

impl From<&Config> for Theme {
    fn from(config: &Config) -> Self {
        Self::from_config(config)
    }
}

impl From<&ThemeConfig> for Theme {
    fn from(config: &ThemeConfig) -> Self {
        Self::from_theme_config(config)
    }
}

// ==========================================
// Nerd Font Icons (Single Unicode codepoints)
// ==========================================

pub const ICON_SNOWFLAKE: &str = "\u{f2dc}"; //  nf-fa-snowflake_o
pub const ICON_BOLT: &str = "\u{f0e7}"; //  nf-fa-bolt
pub const ICON_FULL_CYCLE: &str = "\u{f0e7}"; //  nf-fa-bolt
pub const ICON_REBUILD: &str = "\u{f0e2}"; //  nf-fa-undo / rebuild
pub const ICON_PACKAGE: &str = "\u{f487}"; //  nf-oct-package
pub const ICON_GIT: &str = "\u{f418}"; //  nf-oct-git_branch
pub const ICON_TEST_BUILD: &str = "\u{f0c3}"; //  nf-fa-flask
pub const ICON_CLEANUP: &str = "\u{f1f8}"; //  nf-fa-trash
pub const ICON_HISTORY: &str = "\u{f1da}"; //  nf-fa-history
pub const ICON_DIFF: &str = "\u{f002}"; //  nf-fa-search / diff
pub const ICON_HARD_RESET: &str = "\u{f04a}"; //  nf-fa-backward
pub const ICON_SOFT_REVERT: &str = "\u{f418}"; //  nf-oct-git_branch
pub const ICON_TRIM: &str = "\u{f0c4}"; //  nf-fa-scissors
pub const ICON_BACK: &str = "\u{f060}"; //  nf-fa-arrow_left
pub const ICON_EXIT: &str = "\u{f08b}"; //  nf-fa-sign_out
pub const ICON_SUCCESS: &str = "\u{f058}"; //  nf-fa-check_circle
pub const ICON_ERROR: &str = "\u{f057}"; //  nf-fa-times_circle
pub const ICON_WARNING: &str = "\u{f071}"; //  nf-fa-exclamation_triangle
pub const ICON_COMMIT: &str = "\u{f417}"; //  nf-oct-git_commit
pub const ICON_SEARCH: &str = "\u{f002}"; //  nf-fa-search
pub const ICON_FILE: &str = "\u{f15b}"; //  nf-fa-file
pub const ICON_PATCH: &str = "\u{f044}"; //  nf-fa-pencil_square_o

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_color() {
        assert_eq!(
            parse_hex_color("#89b4fa"),
            Some(Color::Rgb(0x89, 0xb4, 0xfa))
        );
        assert_eq!(
            parse_hex_color("cba6f7"),
            Some(Color::Rgb(0xcb, 0xa6, 0xf7))
        );
        assert_eq!(parse_hex_color("#fff"), Some(Color::Rgb(255, 255, 255)));
        assert_eq!(parse_hex_color("#000"), Some(Color::Rgb(0, 0, 0)));
        assert_eq!(parse_hex_color("8cd"), Some(Color::Rgb(136, 204, 221)));
        assert_eq!(parse_hex_color("invalid"), None);
        assert_eq!(parse_hex_color("你好"), None);
        assert_eq!(parse_hex_color("краска"), None);
    }

    #[test]
    fn test_parse_color_named() {
        assert_eq!(parse_color("cyan"), Some(Color::Cyan));
        assert_eq!(parse_color("Red"), Some(Color::Red));
        assert_eq!(parse_color("WHITE"), Some(Color::White));
        assert_eq!(parse_color("#89b4fa"), Some(Color::Rgb(0x89, 0xb4, 0xfa)));
        assert_eq!(parse_color("#fff"), Some(Color::Rgb(255, 255, 255)));
        assert_eq!(parse_color("unknown_color"), None);
    }

    #[test]
    fn test_palette_selection() {
        let cfg = ThemeConfig {
            palette: "nord".to_string(),
            ..Default::default()
        };
        let t = Theme::from_theme_config(&cfg);
        assert_eq!(t.border, Color::Rgb(136, 192, 208));
    }

    #[test]
    fn test_custom_palette_in_config() {
        let mut palettes = std::collections::BTreeMap::new();
        palettes.insert(
            "cyberpunk".to_string(),
            PaletteConfig {
                border: Some("#ff007f".to_string()),
                header_title: Some("cyan".to_string()),
                ..Default::default()
            },
        );

        let config = Config {
            theme: ThemeConfig {
                palette: "CyberPunk".to_string(), // test case-insensitive matching
                ..Default::default()
            },
            palettes,
            ..Default::default()
        };

        let t = Theme::from_config(&config);
        assert_eq!(t.border, Color::Rgb(0xff, 0x00, 0x7f));
        assert_eq!(t.header_title, Color::Cyan);
    }

    #[test]
    fn test_custom_palette_with_base_inheritance() {
        let mut palettes = std::collections::BTreeMap::new();
        palettes.insert(
            "my-nord".to_string(),
            PaletteConfig {
                base: Some("nord".to_string()),
                accent: Some("#ff00ff".to_string()),
                ..Default::default()
            },
        );

        let config = Config {
            theme: ThemeConfig {
                palette: "my-nord".to_string(),
                ..Default::default()
            },
            palettes,
            ..Default::default()
        };

        let t = Theme::from_config(&config);
        // Border inherits from Nord
        assert_eq!(t.border, Color::Rgb(136, 192, 208));
        // Accent is overridden
        assert_eq!(t.accent, Color::Rgb(0xff, 0x00, 0xff));
    }

    #[test]
    fn test_theme_override_precedence() {
        let mut palettes = std::collections::BTreeMap::new();
        palettes.insert(
            "custom".to_string(),
            PaletteConfig {
                border: Some("#111111".to_string()),
                accent: Some("#222222".to_string()),
                ..Default::default()
            },
        );

        let config = Config {
            theme: ThemeConfig {
                palette: "custom".to_string(),
                border: Some("#ffffff".to_string()), // overrides custom palette
                ..Default::default()
            },
            palettes,
            ..Default::default()
        };

        let t = Theme::from_config(&config);
        assert_eq!(t.border, Color::Rgb(0xff, 0xff, 0xff));
        assert_eq!(t.accent, Color::Rgb(0x22, 0x22, 0x22));
    }

    #[test]
    fn test_standalone_palette_files() {
        let temp_dir =
            std::env::temp_dir().join(format!("flaker_test_palettes_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);

        // 1. Direct flat format
        let flat_file = temp_dir.join("matrix.toml");
        std::fs::write(
            &flat_file,
            r##"
base = "mocha"
border = "#00ff00"
header_title = "green"
"##,
        )
        .unwrap();

        let loaded_flat = load_palette_file(&flat_file).expect("Failed to load flat palette");
        assert_eq!(loaded_flat.border.as_deref(), Some("#00ff00"));
        assert_eq!(loaded_flat.header_title.as_deref(), Some("green"));

        // 2. Wrapped [palette] format
        let wrapped_file = temp_dir.join("wrapped.toml");
        std::fs::write(
            &wrapped_file,
            r##"
[palette]
base = "nord"
border = "#ff0000"
"##,
        )
        .unwrap();

        let loaded_wrapped =
            load_palette_file(&wrapped_file).expect("Failed to load wrapped palette");
        assert_eq!(loaded_wrapped.border.as_deref(), Some("#ff0000"));
        assert_eq!(loaded_wrapped.base.as_deref(), Some("nord"));

        // Test direct file path in config.palette
        let config = Config {
            theme: ThemeConfig {
                palette: flat_file.to_string_lossy().to_string(),
                ..Default::default()
            },
            ..Default::default()
        };

        let t = Theme::from_config(&config);
        assert_eq!(t.border, Color::Rgb(0, 255, 0));
        assert_eq!(t.header_title, Color::Green);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_circular_palette_reference_terminates() {
        let mut palettes = std::collections::BTreeMap::new();
        palettes.insert(
            "loop_a".to_string(),
            PaletteConfig {
                base: Some("loop_b".to_string()),
                ..Default::default()
            },
        );
        palettes.insert(
            "loop_b".to_string(),
            PaletteConfig {
                base: Some("loop_a".to_string()),
                ..Default::default()
            },
        );

        let config = Config {
            theme: ThemeConfig {
                palette: "loop_a".to_string(),
                ..Default::default()
            },
            palettes,
            ..Default::default()
        };

        // Must not hang or stack overflow
        let t = Theme::from_config(&config);
        // Falls back to mocha
        assert_eq!(t.border, Theme::mocha().border);
    }

    #[test]
    fn test_to_ansi_fg() {
        assert_eq!(
            to_ansi_fg(Color::Rgb(136, 192, 208)),
            "\x1b[38;2;136;192;208m"
        );
        assert_eq!(to_ansi_fg(Color::Red), "\x1b[31m");
    }
}
