use crate::actions::nix::DiffKind;
use crate::app::ClosureDiffState;
use crate::theme::{self, Theme};
use crate::ui::header::centered_rect;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap},
};

/// Renders the interactive pre-activation closure diff screen.
pub fn render_closure_diff(frame: &mut Frame, area: Rect, state: &ClosureDiffState, theme: &Theme) {
    let popup_width = 96.min(area.width.saturating_sub(2));
    let popup_height = 28.min(area.height.saturating_sub(2));
    let popup_area = centered_rect(popup_width, popup_height, area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let chunks = Layout::vertical([
        Constraint::Length(1), // Title
        Constraint::Length(1), // Summary badge line
        Constraint::Length(1), // Search / filter bar
        Constraint::Min(4),    // List of items
        Constraint::Length(1), // Actions & hint bar
    ])
    .split(inner);

    // Title
    let title_text = if state.title_suffix.is_empty() {
        format!(
            "{}  CLOSURE DIFF (PRE-ACTIVATION PREVIEW)",
            theme::ICON_PACKAGE
        )
    } else {
        format!(
            "{}  CLOSURE DIFF (PREVIEW) • {}",
            theme::ICON_PACKAGE,
            state.title_suffix
        )
    };
    let title_p = Paragraph::new(Line::from(vec![Span::styled(
        title_text,
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )]))
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true });
    frame.render_widget(title_p, chunks[0]);

    // Summary statistics
    let mut added = 0;
    let mut removed = 0;
    let mut updated = 0;
    let mut rebuilt = 0;

    for item in &state.items {
        match item.kind {
            DiffKind::Added => added += 1,
            DiffKind::Removed => removed += 1,
            DiffKind::Updated => updated += 1,
            DiffKind::Rebuilt => rebuilt += 1,
        }
    }

    let summary_line = Line::from(vec![
        Span::styled(
            format!("Total: {} packages  │  ", state.items.len()),
            Style::default().fg(theme.neutral_text),
        ),
        Span::styled(
            format!("+{added} Added  "),
            Style::default()
                .fg(theme.success)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("│  ", Style::default().fg(theme.faint_hint)),
        Span::styled(
            format!("-{removed} Removed  "),
            Style::default()
                .fg(theme.danger)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("│  ", Style::default().fg(theme.faint_hint)),
        Span::styled(
            format!("~{updated} Updated  "),
            Style::default()
                .fg(theme.warning)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("│  ", Style::default().fg(theme.faint_hint)),
        Span::styled(
            format!("•{rebuilt} Rebuilt"),
            Style::default().fg(theme.faint_hint),
        ),
    ]);
    let summary_p = Paragraph::new(summary_line)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    frame.render_widget(summary_p, chunks[1]);

    // Search / filter bar
    let query = state.search_input.value();
    let filter_count = state.filtered_indices.len();
    let total_count = state.items.len();

    let search_line = Line::from(vec![
        Span::styled(
            " 🔍 Filter: ",
            Style::default()
                .fg(theme.selected)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            if query.is_empty() {
                "Type to search packages..."
            } else {
                query
            },
            if query.is_empty() {
                Style::default().fg(theme.faint_hint)
            } else {
                Style::default()
                    .fg(theme.text)
                    .add_modifier(Modifier::UNDERLINED)
            },
        ),
        Span::styled(
            format!("  ({filter_count} / {total_count} shown)"),
            Style::default().fg(theme.faint_hint),
        ),
    ]);
    let search_p = Paragraph::new(search_line);
    frame.render_widget(search_p, chunks[2]);

    // Items list
    if state.filtered_indices.is_empty() {
        let msg = if state.items.is_empty() {
            if !std::path::Path::new("/run/current-system").exists() {
                "No active system profile (/run/current-system) found to compare against."
            } else {
                "No package differences detected between active system and new build (Identical closure)."
            }
        } else {
            "No packages matched your search query."
        };
        let empty_p = Paragraph::new(Line::from(vec![Span::styled(
            msg,
            Style::default().fg(theme.faint_hint),
        )]))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
        frame.render_widget(empty_p, chunks[3]);
    } else {
        let max_visible = chunks[3].height as usize;
        let scroll_offset = if max_visible > 0 && state.cursor >= max_visible {
            state.cursor.saturating_sub(max_visible).saturating_add(1)
        } else {
            0
        };

        let list_items: Vec<ListItem> = state
            .filtered_indices
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .take(max_visible)
            .filter_map(|(pos, &item_idx)| {
                let item = state.items.get(item_idx)?;
                let is_selected = pos == state.cursor;

                let pointer = if is_selected { "> " } else { "  " };
                let pointer_style = if is_selected {
                    Style::default()
                        .fg(theme.selected)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Reset)
                };

                let (badge, badge_style) = match item.kind {
                    DiffKind::Added => (
                        "[+] ",
                        Style::default()
                            .fg(theme.success)
                            .add_modifier(Modifier::BOLD),
                    ),
                    DiffKind::Removed => (
                        "[-] ",
                        Style::default()
                            .fg(theme.danger)
                            .add_modifier(Modifier::BOLD),
                    ),
                    DiffKind::Updated => (
                        "[~] ",
                        Style::default()
                            .fg(theme.warning)
                            .add_modifier(Modifier::BOLD),
                    ),
                    DiffKind::Rebuilt => ("[•] ", Style::default().fg(theme.faint_hint)),
                };

                let pkg_style = if is_selected {
                    Style::default()
                        .fg(theme.selected)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.text)
                };

                let mut spans = vec![
                    Span::styled(pointer, pointer_style),
                    Span::styled(badge, badge_style),
                    Span::styled(&item.package, pkg_style),
                ];

                match item.kind {
                    DiffKind::Added => {
                        spans.push(Span::styled(
                            format!(
                                ": ∅ → {}",
                                item.after_version.as_deref().unwrap_or("unknown")
                            ),
                            Style::default().fg(theme.success),
                        ));
                    }
                    DiffKind::Removed => {
                        spans.push(Span::styled(
                            format!(
                                ": {} → ∅",
                                item.before_version.as_deref().unwrap_or("unknown")
                            ),
                            Style::default().fg(theme.danger),
                        ));
                    }
                    DiffKind::Updated => {
                        spans.push(Span::styled(
                            format!(": {} ", item.before_version.as_deref().unwrap_or("?")),
                            Style::default().fg(theme.neutral_text),
                        ));
                        spans.push(Span::styled("→ ", Style::default().fg(theme.accent)));
                        spans.push(Span::styled(
                            item.after_version.as_deref().unwrap_or("?"),
                            Style::default().fg(theme.success),
                        ));
                    }
                    DiffKind::Rebuilt => {
                        spans.push(Span::styled(
                            ": rebuilt",
                            Style::default().fg(theme.faint_hint),
                        ));
                    }
                }

                if let Some(ref sz) = item.size_delta {
                    let sz_style = if sz.starts_with('+') {
                        Style::default().fg(theme.success)
                    } else if sz.starts_with('-') {
                        Style::default().fg(theme.danger)
                    } else {
                        Style::default().fg(theme.faint_hint)
                    };
                    spans.push(Span::styled(format!("  ({sz})"), sz_style));
                }

                Some(ListItem::new(Line::from(spans)))
            })
            .collect();

        let list = List::new(list_items);
        frame.render_widget(list, chunks[3]);
    }

    // Actions & hint footer
    let mut hint_spans = Vec::new();
    if state.on_confirm_task.is_some() {
        hint_spans.push(Span::styled(
            " [Enter] ",
            Style::default()
                .fg(theme.selected)
                .add_modifier(Modifier::BOLD),
        ));
        hint_spans.push(Span::styled(
            "Switch to Configuration  •  ",
            Style::default().fg(theme.text),
        ));
    }
    hint_spans.extend(vec![
        Span::styled(
            "[Esc] ",
            Style::default()
                .fg(theme.danger)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("Cancel / Abort  •  ", Style::default().fg(theme.faint_hint)),
        Span::styled("Type to Filter  •  ", Style::default().fg(theme.faint_hint)),
        Span::styled("↑/↓ Navigate", Style::default().fg(theme.faint_hint)),
    ]);
    let hint_line = Line::from(hint_spans);
    let hint_p = Paragraph::new(hint_line)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    frame.render_widget(hint_p, chunks[4]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::nix::{ClosureDiffItem, DiffKind};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use tui_input::Input;

    #[test]
    fn test_render_closure_diff_items() {
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();

        let state = ClosureDiffState {
            items: vec![
                ClosureDiffItem {
                    package: "firefox".to_string(),
                    before_version: Some("134.0".to_string()),
                    after_version: Some("135.0".to_string()),
                    size_delta: Some("+12.4 MiB".to_string()),
                    kind: DiffKind::Updated,
                    raw: "firefox: 134.0 → 135.0, +12.4 MiB".to_string(),
                },
                ClosureDiffItem {
                    package: "antigravity".to_string(),
                    before_version: Some("1.0".to_string()),
                    after_version: None,
                    size_delta: Some("-500 MiB".to_string()),
                    kind: DiffKind::Removed,
                    raw: "antigravity: 1.0 → ∅, -500 MiB".to_string(),
                },
            ],
            filtered_indices: vec![0, 1],
            search_input: Input::default(),
            cursor: 0,
            on_confirm_task: None,
            return_screen: Box::new(crate::app::Screen::TopMenu),
            title_suffix: "Rebuild & Switch".to_string(),
        };

        terminal
            .draw(|f| {
                render_closure_diff(f, f.area(), &state, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_render_closure_diff_small_term() {
        for (w, h) in [(20, 4), (1, 1), (0, 0)] {
            let backend = TestBackend::new(w, h);
            let mut terminal = Terminal::new(backend).unwrap();
            let theme = Theme::default();

            let state = ClosureDiffState {
                items: vec![],
                filtered_indices: vec![],
                search_input: Input::default(),
                cursor: 0,
                on_confirm_task: None,
                return_screen: Box::new(crate::app::Screen::TopMenu),
                title_suffix: "".to_string(),
            };

            terminal
                .draw(|f| {
                    render_closure_diff(f, f.area(), &state, &theme);
                })
                .unwrap();
        }
    }
}
