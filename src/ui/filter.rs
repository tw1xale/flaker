use crate::theme::{self, Theme};
use crate::ui::header::centered_rect;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap},
};
use tui_input::Input;

pub struct FilterParams<'a> {
    pub header_title: &'a str,
    pub input: &'a Input,
    pub filtered_items: &'a [(&'a str, Vec<usize>)],
    pub selected_index: usize,
    pub hint: &'a str,
    pub preview_hash: &'a str,
    pub preview_lines: &'a [String],
    pub preview_scroll: usize,
}

/// Renders the commit selection fuzzy filter modal with live commit diff/stat preview pane.
pub fn render_filter(frame: &mut Frame, area: Rect, params: &FilterParams, theme: &Theme) {
    let is_wide = area.width >= 96;
    let max_popup_w = if is_wide { 136 } else { 76 };
    let popup_width = max_popup_w.min(area.width.saturating_sub(2));
    let popup_height = 28.min(area.height.saturating_sub(2));
    let popup_area = centered_rect(popup_width, popup_height, area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.warning));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let vertical_chunks = Layout::vertical([
        Constraint::Length(1), // Title
        Constraint::Length(1), // Subtitle
        Constraint::Length(1), // Hint
        Constraint::Length(1), // Spacer
        Constraint::Min(6),    // Main content area
    ])
    .split(inner);

    // Title
    let title_p = Paragraph::new(Line::from(vec![Span::styled(
        format!("{}  SELECT COMMIT", theme::ICON_HISTORY),
        Style::default()
            .fg(theme.warning)
            .add_modifier(Modifier::BOLD),
    )]))
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true });
    frame.render_widget(title_p, vertical_chunks[0]);

    // Subtitle
    let subtitle_p = Paragraph::new(Line::from(vec![Span::styled(
        params.header_title,
        Style::default().fg(theme.neutral_text),
    )]))
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true });
    frame.render_widget(subtitle_p, vertical_chunks[1]);

    // Hint
    let hint_p = Paragraph::new(Line::from(vec![Span::styled(
        params.hint,
        Style::default().fg(theme.faint_hint),
    )]))
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true });
    frame.render_widget(hint_p, vertical_chunks[2]);

    let main_area = vertical_chunks[4];

    let (left_pane, right_pane_opt) = if is_wide {
        let panes = Layout::horizontal([
            Constraint::Percentage(45), // Left: Commits list & search
            Constraint::Percentage(55), // Right: Commit preview pane
        ])
        .split(main_area);
        (panes[0], Some(panes[1]))
    } else {
        (main_area, None)
    };

    // --- Left Pane: Search bar & Commits list ---
    let left_chunks = Layout::vertical([
        Constraint::Length(3), // Search bar
        Constraint::Min(4),    // Commits list
    ])
    .split(left_pane);

    let search_area = Layout::horizontal([
        Constraint::Length(1), // Left padding
        Constraint::Min(10),   // Search input box
        Constraint::Length(1), // Right padding
    ])
    .split(left_chunks[0])[1];

    let prefix_text = format!(" {} Search: ", theme::ICON_SEARCH);
    let prefix_width = 1 + 1 + 9;

    let search_line = Line::from(vec![
        Span::styled(prefix_text, Style::default().fg(theme.border)),
        if params.input.value().is_empty() {
            Span::styled(
                "Filter by hash, date, or message...",
                Style::default().fg(theme.faint_hint),
            )
        } else {
            Span::styled(params.input.value(), Style::default().fg(theme.text))
        },
    ]);

    let search_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border));

    let search_p = Paragraph::new(search_line).block(search_block);
    frame.render_widget(search_p, search_area);

    // Commits list
    let list_area = left_chunks[1];
    let max_visible = list_area.height as usize;
    let scroll_offset = if params.selected_index >= max_visible {
        params.selected_index - max_visible + 1
    } else {
        0
    };

    let max_col_width = list_area.width.saturating_sub(2) as usize;

    let list_items: Vec<ListItem> = if params.filtered_items.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            " No matching commits found.",
            Style::default().fg(theme.faint_hint),
        )))]
    } else {
        params
            .filtered_items
            .iter()
            .skip(scroll_offset)
            .take(max_visible)
            .enumerate()
            .map(|(visible_idx, (commit_str, matches))| {
                let actual_idx = visible_idx + scroll_offset;
                let is_selected = actual_idx == params.selected_index;

                let mut spans = vec![Span::raw(" ")];

                let char_count = commit_str.chars().count();
                let truncated = char_count > max_col_width;
                let take_count = if truncated {
                    max_col_width.saturating_sub(1)
                } else {
                    char_count
                };

                for (char_idx, ch) in commit_str.chars().take(take_count).enumerate() {
                    let is_match = matches.contains(&char_idx);
                    let style = if is_match {
                        Style::default()
                            .fg(theme.header_title)
                            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                    } else if is_selected {
                        Style::default()
                            .fg(theme.selected)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.text)
                    };

                    spans.push(Span::styled(ch.to_string(), style));
                }

                if truncated {
                    spans.push(Span::styled("…", Style::default().fg(theme.faint_hint)));
                }

                ListItem::new(Line::from(spans))
            })
            .collect()
    };

    let list = List::new(list_items);
    frame.render_widget(list, list_area);

    // Set cursor on search query according to input.visual_cursor()
    let start_x = search_area.x + 1 + prefix_width;
    let cursor_x = start_x + params.input.visual_cursor() as u16;
    let cursor_y = search_area.y + 1;
    if cursor_x < search_area.x + search_area.width.saturating_sub(1) {
        frame.set_cursor_position((cursor_x, cursor_y));
    }

    // --- Right Pane: Commit Changes / Diff Preview ---
    if let Some(right_pane) = right_pane_opt {
        let total_lines = params.preview_lines.len();
        let preview_title = if !params.preview_hash.is_empty() {
            format!(" {} Changes: {} ", theme::ICON_DIFF, params.preview_hash)
        } else {
            format!(" {} Commit Preview ", theme::ICON_DIFF)
        };

        let preview_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border))
            .title(Span::styled(
                preview_title,
                Style::default()
                    .fg(theme.header_title)
                    .add_modifier(Modifier::BOLD),
            ));

        let preview_inner = preview_block.inner(right_pane);
        frame.render_widget(preview_block, right_pane);

        let preview_chunks = Layout::vertical([
            Constraint::Min(1),    // Diff lines
            Constraint::Length(1), // Footer scroll indicator
        ])
        .split(preview_inner);

        let preview_visible_height = preview_chunks[0].height as usize;

        let visible_lines: Vec<Line> = if params.preview_lines.is_empty() {
            vec![Line::from(Span::styled(
                " No preview available",
                Style::default().fg(theme.faint_hint),
            ))]
        } else {
            params
                .preview_lines
                .iter()
                .skip(params.preview_scroll)
                .take(preview_visible_height)
                .map(|raw| format_diff_line(raw, theme))
                .collect()
        };

        let preview_p = Paragraph::new(visible_lines);
        frame.render_widget(preview_p, preview_chunks[0]);

        // Footer with scroll position
        let scroll_hint = if total_lines > preview_visible_height {
            format!(
                "Lines {}-{} of {} [PgUp/PgDn]",
                params.preview_scroll + 1,
                (params.preview_scroll + preview_visible_height).min(total_lines),
                total_lines
            )
        } else if total_lines > 0 {
            format!("{} lines", total_lines)
        } else {
            String::new()
        };

        let footer_p = Paragraph::new(Line::from(vec![Span::styled(
            scroll_hint,
            Style::default().fg(theme.faint_hint),
        )]))
        .alignment(Alignment::Right);
        frame.render_widget(footer_p, preview_chunks[1]);
    }
}

/// Helper to style individual lines in the commit preview pane.
fn format_diff_line<'a>(raw: &'a str, theme: &'a Theme) -> Line<'a> {
    if raw.starts_with('+') && !raw.starts_with("+++") {
        Line::from(Span::styled(raw, Style::default().fg(theme.success)))
    } else if raw.starts_with('-') && !raw.starts_with("---") {
        Line::from(Span::styled(raw, Style::default().fg(theme.danger)))
    } else if raw.starts_with("@@") {
        Line::from(Span::styled(
            raw,
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ))
    } else if raw.starts_with("diff --git") || raw.starts_with("index ") {
        Line::from(Span::styled(
            raw,
            Style::default()
                .fg(theme.secondary_info)
                .add_modifier(Modifier::BOLD),
        ))
    } else if raw.starts_with("commit ") {
        Line::from(Span::styled(
            raw,
            Style::default()
                .fg(theme.selected)
                .add_modifier(Modifier::BOLD),
        ))
    } else if raw.starts_with("Author:") || raw.starts_with("Date:") {
        Line::from(Span::styled(raw, Style::default().fg(theme.neutral_text)))
    } else if raw.contains("files changed") || raw.contains("file changed") {
        Line::from(Span::styled(
            raw,
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ))
    } else if raw.contains('|') && (raw.contains('+') || raw.contains('-')) {
        let parts: Vec<&str> = raw.splitn(2, '|').collect();
        if parts.len() == 2 {
            let mut spans = vec![
                Span::styled(parts[0], Style::default().fg(theme.text)),
                Span::styled("|", Style::default().fg(theme.faint_hint)),
            ];
            for ch in parts[1].chars() {
                let style = match ch {
                    '+' => Style::default()
                        .fg(theme.success)
                        .add_modifier(Modifier::BOLD),
                    '-' => Style::default()
                        .fg(theme.danger)
                        .add_modifier(Modifier::BOLD),
                    _ => Style::default().fg(theme.neutral_text),
                };
                spans.push(Span::styled(ch.to_string(), style));
            }
            Line::from(spans)
        } else {
            Line::from(Span::styled(raw, Style::default().fg(theme.text)))
        }
    } else {
        Line::from(Span::styled(raw, Style::default().fg(theme.text)))
    }
}
