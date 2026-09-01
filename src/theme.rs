use catppuccin::PALETTE;
use ratatui::style::Color;

use crate::config::ThemeConfig;

const fn cat_to_rat(c: catppuccin::Color) -> Color {
    Color::Rgb(c.rgb.r, c.rgb.g, c.rgb.b)
}

/// Parses a hex color string like "#89b4fa" or "89b4fa" into ratatui Color::Rgb.
pub fn parse_hex_color(hex: &str) -> Option<Color> {
    let s = hex.trim().trim_start_matches('#');
    if s.len() == 6 {
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        Some(Color::Rgb(r, g, b))
    } else {
        None
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

impl Theme {
    pub fn from_config(config: &ThemeConfig) -> Self {
        let mut theme = match config.palette.to_lowercase().trim() {
            "macchiato" => Self::macchiato(),
            "frappe" | "frappé" => Self::frappe(),
            "latte" => Self::latte(),
            "nord" => Self::nord(),
            "tokyonight" | "tokyo-night" => Self::tokyonight(),
            "dracula" => Self::dracula(),
            "gruvbox" => Self::gruvbox(),
            _ => Self::mocha(),
        };

        // Apply any explicit custom color overrides
        if let Some(ref hex) = config.border
            && let Some(c) = parse_hex_color(hex)
        {
            theme.border = c;
        }
        if let Some(ref hex) = config.header_title
            && let Some(c) = parse_hex_color(hex)
        {
            theme.header_title = c;
        }
        if let Some(ref hex) = config.secondary_info
            && let Some(c) = parse_hex_color(hex)
        {
            theme.secondary_info = c;
        }
        if let Some(ref hex) = config.muted_text
            && let Some(c) = parse_hex_color(hex)
        {
            theme.muted_text = c;
        }
        if let Some(ref hex) = config.faint_hint
            && let Some(c) = parse_hex_color(hex)
        {
            theme.faint_hint = c;
        }
        if let Some(ref hex) = config.warning
            && let Some(c) = parse_hex_color(hex)
        {
            theme.warning = c;
        }
        if let Some(ref hex) = config.danger
            && let Some(c) = parse_hex_color(hex)
        {
            theme.danger = c;
        }
        if let Some(ref hex) = config.success
            && let Some(c) = parse_hex_color(hex)
        {
            theme.success = c;
        }
        if let Some(ref hex) = config.accent
            && let Some(c) = parse_hex_color(hex)
        {
            theme.accent = c;
        }
        if let Some(ref hex) = config.selected
            && let Some(c) = parse_hex_color(hex)
        {
            theme.selected = c;
        }
        if let Some(ref hex) = config.text
            && let Some(c) = parse_hex_color(hex)
        {
            theme.text = c;
        }
        if let Some(ref hex) = config.neutral_text
            && let Some(c) = parse_hex_color(hex)
        {
            theme.neutral_text = c;
        }

        theme
    }

    pub fn mocha() -> Self {
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

    pub fn macchiato() -> Self {
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

    pub fn frappe() -> Self {
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

    pub fn latte() -> Self {
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

    pub fn nord() -> Self {
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

    pub fn tokyonight() -> Self {
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

    pub fn dracula() -> Self {
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

    pub fn gruvbox() -> Self {
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
        assert_eq!(parse_hex_color("invalid"), None);
    }

    #[test]
    fn test_palette_selection() {
        let cfg = ThemeConfig {
            palette: "nord".to_string(),
            ..Default::default()
        };
        let t = Theme::from_config(&cfg);
        assert_eq!(t.border, Color::Rgb(136, 192, 208));
    }
}
