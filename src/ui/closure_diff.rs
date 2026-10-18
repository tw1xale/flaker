use crate::actions::nix::DiffKind;
use crate::app::ClosureDiffState;
use crate::theme::{self, Theme};
use crate::ui::header::centered_rect;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
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
    let title_text = if state.on_confirm_task.is_some() {
        if state.title_suffix.is_empty() {
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
        }
    } else if state.title_suffix.is_empty() {
        format!("{}  SYSTEM UPDATE SUMMARY", theme::ICON_PACKAGE)
    } else {
        format!(
            "{}  SYSTEM UPDATE SUMMARY • {}",
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

    let count_detail = if filter_count > chunks[3].height as usize && chunks[3].height > 0 {
        let max_visible = chunks[3].height as usize;
        let max_scroll = filter_count.saturating_sub(max_visible);
        let actual_scroll = state.scroll_offset.min(max_scroll);
        let first = actual_scroll + 1;
        let last = (actual_scroll + max_visible).min(filter_count);
        format!("  ({filter_count} / {total_count} shown  •  {first}–{last} of {filter_count})")
    } else {
        format!("  ({filter_count} / {total_count} shown)")
    };

    let prefix_text = format!(" {} Filter: ", theme::ICON_SEARCH);
    let prefix_width = unicode_width::UnicodeWidthStr::width(prefix_text.as_str()) as u16;
    let count_detail_w = unicode_width::UnicodeWidthStr::width(count_detail.as_str()) as u16;

    let input_max_w = (chunks[2]
        .width
        .saturating_sub(prefix_width)
        .saturating_sub(count_detail_w)) as usize;
    let input_max_w = input_max_w.max(1);
    let scroll = state.search_input.visual_scroll(input_max_w);

    let query_span = if query.is_empty() {
        Span::styled(
            "Type to search packages...",
            Style::default().fg(theme.faint_hint),
        )
    } else {
        let visible = crate::ui::slice_by_column_width(query, scroll, input_max_w);
        Span::styled(
            visible,
            Style::default()
                .fg(theme.text)
                .add_modifier(Modifier::UNDERLINED),
        )
    };

    let search_line = Line::from(vec![
        Span::styled(
            prefix_text,
            Style::default()
                .fg(theme.selected)
                .add_modifier(Modifier::BOLD),
        ),
        query_span,
        Span::styled(count_detail, Style::default().fg(theme.faint_hint)),
    ]);
    let search_p = Paragraph::new(search_line);
    frame.render_widget(search_p, chunks[2]);

    let cursor_offset = u16::try_from(state.search_input.visual_cursor().saturating_sub(scroll))
        .unwrap_or(u16::MAX);
    let cursor_x = chunks[2]
        .x
        .saturating_add(prefix_width)
        .saturating_add(cursor_offset);
    let cursor_y = chunks[2].y;
    if cursor_x < chunks[2].right() {
        frame.set_cursor_position((cursor_x, cursor_y));
    }

    // Items list
    if state.filtered_indices.is_empty() {
        let msg = if state.items.is_empty() {
            if !std::path::Path::new("/run/current-system").exists() {
                "No active system profile (/run/current-system) found to compare against."
            } else if state.is_identical_closure {
                "Previous system profile and new configuration closures are completely identical (No changes)."
            } else {
                "System configuration changed (rebuilt closure), but no package additions, removals, or version bumps detected."
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
        let max_scroll = state.filtered_indices.len().saturating_sub(max_visible);
        let scroll_offset = state.scroll_offset.min(max_scroll);

        let list_items: Vec<ListItem> = state
            .filtered_indices
            .iter()
            .skip(scroll_offset)
            .take(max_visible)
            .filter_map(|&item_idx| {
                let item = state.items.get(item_idx)?;

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

                let pkg_style = Style::default().fg(theme.text).add_modifier(Modifier::BOLD);

                let mut spans = vec![
                    Span::raw("  "),
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
                        if let Some(ref ver) = item.after_version {
                            spans.push(Span::styled(
                                format!(": {ver} (rebuilt)"),
                                Style::default().fg(theme.faint_hint),
                            ));
                        } else {
                            spans.push(Span::styled(
                                ": rebuilt",
                                Style::default().fg(theme.faint_hint),
                            ));
                        }
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
        hint_spans.extend(vec![
            Span::styled(
                "[Esc] ",
                Style::default()
                    .fg(theme.danger)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Cancel / Abort  •  ", Style::default().fg(theme.faint_hint)),
            Span::styled("Type to Filter  •  ", Style::default().fg(theme.faint_hint)),
            Span::styled("↑/↓ Scroll", Style::default().fg(theme.faint_hint)),
        ]);
    } else {
        hint_spans.extend(vec![
            Span::styled(
                " [Enter / Esc] ",
                Style::default()
                    .fg(theme.selected)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Back to Menu  •  ", Style::default().fg(theme.text)),
            Span::styled("Type to Filter  •  ", Style::default().fg(theme.faint_hint)),
            Span::styled("↑/↓ Scroll", Style::default().fg(theme.faint_hint)),
        ]);
    }
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
            scroll_offset: 0,
            is_identical_closure: false,
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
                scroll_offset: 0,
                is_identical_closure: false,
                on_confirm_task: None,
                return_screen: Box::new(crate::app::Screen::TopMenu),
                title_suffix: String::new(),
            };

            terminal
                .draw(|f| {
                    render_closure_diff(f, f.area(), &state, &theme);
                })
                .unwrap();
        }
    }

    #[test]
    fn test_long_closure_search_must_not_overflow_cursor() {
        let backend = TestBackend::new(80, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = ClosureDiffState {
            items: Vec::new(),
            filtered_indices: Vec::new(),
            search_input: Input::new("x".repeat(usize::from(u16::MAX))),
            scroll_offset: 0,
            is_identical_closure: false,
            on_confirm_task: None,
            return_screen: Box::new(crate::app::Screen::TopMenu),
            title_suffix: String::new(),
        };
        terminal
            .draw(|frame| {
                render_closure_diff(frame, frame.area(), &state, &Theme::default());
            })
            .unwrap();
    }
}
