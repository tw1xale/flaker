use catppuccin::PALETTE;
use ratatui::style::Color;

const fn cat_to_rat(c: catppuccin::Color) -> Color {
    Color::Rgb(c.rgb.r, c.rgb.g, c.rgb.b)
}

/// Border / accent color (was ANSI 39)
pub const BORDER: Color = cat_to_rat(PALETTE.mocha.colors.blue);

/// Header title / bold color (was ANSI 81)
pub const HEADER_TITLE: Color = cat_to_rat(PALETTE.mocha.colors.sky);

/// Secondary info, e.g. flake target / action header details (was ANSI 75)
pub const SECONDARY_INFO: Color = cat_to_rat(PALETTE.mocha.colors.sapphire);

/// Muted text, descriptions, status subtext (was ANSI 245)
pub const MUTED_TEXT: Color = cat_to_rat(PALETTE.mocha.colors.subtext0);

/// Faint hints, e.g. "press Enter to return" (was ANSI 240)
pub const FAINT_HINT: Color = cat_to_rat(PALETTE.mocha.colors.overlay0);

/// Warning / borderline destructive (was ANSI 214)
pub const WARNING: Color = cat_to_rat(PALETTE.mocha.colors.peach);

/// Error / max danger — hard reset, force push, failed execution (was ANSI 196)
pub const DANGER: Color = cat_to_rat(PALETTE.mocha.colors.red);

/// Success indicators (was ANSI 42)
pub const SUCCESS: Color = cat_to_rat(PALETTE.mocha.colors.green);

/// Accent / panel titles / header prompt (Mauve #cba6f7)
pub const ACCENT: Color = cat_to_rat(PALETTE.mocha.colors.mauve);

/// Selected menu item / list row highlight (Catppuccin Mocha Yellow #f9e2af)
pub const SELECTED: Color = cat_to_rat(PALETTE.mocha.colors.yellow);

/// Regular menu item text (was ANSI 253)
pub const TEXT: Color = cat_to_rat(PALETTE.mocha.colors.text);

/// Neutral text, e.g. commit hash, commit picker subheader (was ANSI 250)
pub const NEUTRAL_TEXT: Color = cat_to_rat(PALETTE.mocha.colors.subtext1);

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
