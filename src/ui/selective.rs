use crate::app::{SelectiveMode, SelectiveUpdateState};
use crate::theme::{self, Theme};
use crate::ui::header::centered_rect;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap},
};

/// Renders the interactive flake inputs selection screen for selective updates / exclusions.
pub fn render_selective(
    frame: &mut Frame,
    area: Rect,
    state: &SelectiveUpdateState,
    theme: &Theme,
) {
    let popup_width = 78.min(area.width.saturating_sub(4));
    let item_count = u16::try_from(state.inputs.len()).unwrap_or(u16::MAX);
    let popup_height = item_count
        .saturating_add(8)
        .max(10)
        .min(area.height.saturating_sub(2));
    let popup_area = centered_rect(popup_width, popup_height, area);

    frame.render_widget(Clear, popup_area);

    let border_color = match state.mode {
        SelectiveMode::Include => theme.accent,
        SelectiveMode::Exclude => theme.warning,
    };

    let title = match state.mode {
        SelectiveMode::Include => {
            format!("{}  UPDATE SELECTED INPUTS (INCLUDE)", theme::ICON_PACKAGE)
        }
        SelectiveMode::Exclude => {
            format!("{}  UPDATE ALL EXCEPT... (EXCLUDE)", theme::ICON_CLEANUP)
        }
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let chunks = Layout::vertical([
        Constraint::Length(2), // Title
        Constraint::Length(1), // Subtitle
        Constraint::Min(4),    // Input list
        Constraint::Length(1), // Spacer
        Constraint::Length(1), // Hint bar
    ])
    .split(inner);

    // Title
    let title_p = Paragraph::new(Line::from(vec![Span::styled(
        title,
        Style::default()
            .fg(border_color)
            .add_modifier(Modifier::BOLD),
    )]))
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true });
    frame.render_widget(title_p, chunks[0]);

    // Subtitle
    let subtitle = match state.mode {
        SelectiveMode::Include => "Space to toggle [x] for inputs to update:",
        SelectiveMode::Exclude => "Space to toggle [x] for inputs to KEEP UNTOUCHED:",
    };
    let subtitle_p = Paragraph::new(Line::from(vec![Span::styled(
        subtitle,
        Style::default().fg(theme.neutral_text),
    )]))
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true });
    frame.render_widget(subtitle_p, chunks[1]);

    // Input list items
    let max_visible = chunks[2].height as usize;
    let scroll_offset = if state.cursor >= max_visible {
        state.cursor - max_visible + 1
    } else {
        0
    };

    let list_items: Vec<ListItem> = state
        .inputs
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(max_visible)
        .map(|(idx, input)| {
            let is_selected = idx == state.cursor;
            let is_checked = state.selected.get(idx).copied().unwrap_or(false);

            let check_icon = if is_checked { "[x]" } else { "[ ]" };
            let check_style = if is_checked {
                match state.mode {
                    SelectiveMode::Include => Style::default()
                        .fg(theme.success)
                        .add_modifier(Modifier::BOLD),
                    SelectiveMode::Exclude => Style::default()
                        .fg(theme.danger)
                        .add_modifier(Modifier::BOLD),
                }
            } else {
                Style::default().fg(theme.faint_hint)
            };

            let pointer = if is_selected { "> " } else { "  " };
            let pointer_style = if is_selected {
                Style::default()
                    .fg(theme.selected)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Reset)
            };

            let name_style = if is_selected {
                Style::default()
                    .fg(theme.selected)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text)
            };

            let details_str = if !input.details.is_empty() {
                format!("  ({})", input.details)
            } else {
                String::new()
            };

            ListItem::new(Line::from(vec![
                Span::styled(pointer, pointer_style),
                Span::styled(check_icon, check_style),
                Span::raw(" "),
                Span::styled(&input.name, name_style),
                Span::styled(details_str, Style::default().fg(theme.faint_hint)),
            ]))
        })
        .collect();

    let list = List::new(list_items);
    frame.render_widget(list, chunks[2]);

    // Hint bar
    let hint_line = Line::from(vec![
        Span::styled(
            "Space",
            Style::default()
                .fg(theme.selected)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(": Toggle  •  ", Style::default().fg(theme.faint_hint)),
        Span::styled(
            "a",
            Style::default()
                .fg(theme.selected)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(": All  •  ", Style::default().fg(theme.faint_hint)),
        Span::styled(
            "n",
            Style::default()
                .fg(theme.selected)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(": None  •  ", Style::default().fg(theme.faint_hint)),
        Span::styled(
            "i",
            Style::default()
                .fg(theme.selected)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(": Invert  •  ", Style::default().fg(theme.faint_hint)),
        Span::styled(
            "Enter",
            Style::default()
                .fg(theme.selected)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(": Continue  •  ", Style::default().fg(theme.faint_hint)),
        Span::styled(
            "Esc",
            Style::default()
                .fg(theme.selected)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(": Back", Style::default().fg(theme.faint_hint)),
    ]);
    let hint_p = Paragraph::new(hint_line).alignment(Alignment::Center);
    frame.render_widget(hint_p, chunks[4]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::nix::FlakeInput;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_render_selective_include_mode() {
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();

        let state = SelectiveUpdateState {
            mode: SelectiveMode::Include,
            inputs: vec![
                FlakeInput {
                    name: "nixpkgs".to_string(),
                    details: "NixOS/nixpkgs@abcdef1".to_string(),
                },
                FlakeInput {
                    name: "home-manager".to_string(),
                    details: "nix-community/home-manager@1234567".to_string(),
                },
            ],
            selected: vec![true, false],
            cursor: 0,
            return_screen: Box::new(crate::app::Screen::TopMenu),
        };

        terminal
            .draw(|frame| {
                render_selective(frame, frame.area(), &state, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_render_selective_exclude_mode_small_area() {
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();

        let state = SelectiveUpdateState {
            mode: SelectiveMode::Exclude,
            inputs: vec![FlakeInput {
                name: "nixpkgs".to_string(),
                details: "".to_string(),
            }],
            selected: vec![false],
            cursor: 0,
            return_screen: Box::new(crate::app::Screen::TopMenu),
        };

        terminal
            .draw(|frame| {
                render_selective(frame, frame.area(), &state, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_render_selective_very_small_terminal() {
        let backend = TestBackend::new(30, 4);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();

        let state = SelectiveUpdateState {
            mode: SelectiveMode::Include,
            inputs: vec![FlakeInput {
                name: "nixpkgs".to_string(),
                details: "".to_string(),
            }],
            selected: vec![true],
            cursor: 0,
            return_screen: Box::new(crate::app::Screen::TopMenu),
        };

        terminal
            .draw(|frame| {
                render_selective(frame, frame.area(), &state, &theme);
            })
            .unwrap();
    }
}
