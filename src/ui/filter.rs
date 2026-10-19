use crate::theme::{self, Theme};
use crate::ui::diff::format_diff_line;
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
    pub title: &'a str,
    pub header_title: &'a str,
    pub input: &'a Input,
    pub search_placeholder: &'a str,
    pub empty_message: &'a str,
    pub filtered_items: &'a [(&'a str, Vec<usize>)],
    pub selected_index: usize,
    pub hint: &'a str,
    pub preview_hash: &'a str,
    pub preview_lines: &'a [String],
    pub preview_scroll: usize,
}

/// Renders the fuzzy filter modal with live diff/stat preview pane.
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

    let [title_area, subtitle_area, hint_area, _, main_area] = Layout::vertical([
        Constraint::Length(1), // Title
        Constraint::Length(1), // Subtitle
        Constraint::Length(1), // Hint
        Constraint::Length(1), // Spacer
        Constraint::Min(6),    // Main content area
    ])
    .areas(inner);

    // Title
    let title_p = Paragraph::new(Line::from(vec![Span::styled(
        params.title,
        Style::default()
            .fg(theme.warning)
            .add_modifier(Modifier::BOLD),
    )]))
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true });
    frame.render_widget(title_p, title_area);

    // Subtitle
    let subtitle_p = Paragraph::new(Line::from(vec![Span::styled(
        params.header_title,
        Style::default().fg(theme.neutral_text),
    )]))
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true });
    frame.render_widget(subtitle_p, subtitle_area);

    // Hint
    let hint_p = Paragraph::new(Line::from(vec![Span::styled(
        params.hint,
        Style::default().fg(theme.faint_hint),
    )]))
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true });
    frame.render_widget(hint_p, hint_area);

    let (left_pane, right_pane_opt) = if is_wide {
        let [left, right] = Layout::horizontal([
            Constraint::Percentage(45), // Left: Commits list & search
            Constraint::Percentage(55), // Right: Commit preview pane
        ])
        .areas(main_area);
        (left, Some(right))
    } else {
        (main_area, None)
    };

    // --- Left Pane: Search bar & Commits list ---
    let [search_outer, list_area] = Layout::vertical([
        Constraint::Length(3), // Search bar
        Constraint::Min(4),    // Commits list
    ])
    .areas(left_pane);

    let [_, search_area, _] = Layout::horizontal([
        Constraint::Length(1), // Left padding
        Constraint::Min(10),   // Search input box
        Constraint::Length(1), // Right padding
    ])
    .areas(search_outer);

    let prefix_text = format!(" {} Search: ", theme::ICON_SEARCH);
    let prefix_width = u16::try_from(unicode_width::UnicodeWidthStr::width(prefix_text.as_str()))
        .unwrap_or(u16::MAX);

    let input_max_w = usize::from(
        search_area
            .width
            .saturating_sub(prefix_width.saturating_add(3)),
    );
    let scroll = params.input.visual_scroll(input_max_w);

    let search_line = Line::from(vec![
        Span::styled(prefix_text, Style::default().fg(theme.border)),
        if params.input.value().is_empty() {
            Span::styled(
                params.search_placeholder,
                Style::default().fg(theme.faint_hint),
            )
        } else {
            let visible =
                crate::ui::slice_by_column_width(params.input.value(), scroll, input_max_w);
            Span::styled(visible, Style::default().fg(theme.text))
        },
    ]);

    let search_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border));

    let search_p = Paragraph::new(search_line).block(search_block);
    frame.render_widget(search_p, search_area);

    // Commits list
    let max_visible = usize::from(list_area.height);
    let scroll_offset = if max_visible > 0 && params.selected_index >= max_visible {
        params
            .selected_index
            .saturating_sub(max_visible)
            .saturating_add(1)
    } else {
        0
    };

    let max_col_width = usize::from(list_area.width.saturating_sub(2));

    let list_items: Vec<ListItem> = if params.filtered_items.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            params.empty_message,
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
                let actual_idx = visible_idx.saturating_add(scroll_offset);
                let is_selected = actual_idx == params.selected_index;

                let mut spans = vec![Span::raw(" ")];

                let mut current_width: usize = 0;
                let mut take_count: usize = 0;
                let limit = max_col_width.saturating_sub(1);
                for ch in commit_str.chars() {
                    let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
                    if current_width.saturating_add(w) > limit {
                        break;
                    }
                    current_width = current_width.saturating_add(w);
                    take_count = take_count.saturating_add(1);
                }
                let truncated = take_count < commit_str.chars().count();

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

                    let display_str = if ch == '\t' || u32::from(ch) < 32 {
                        " ".to_string()
                    } else {
                        ch.to_string()
                    };

                    spans.push(Span::styled(display_str, style));
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

    // Set cursor on search query according to input.visual_cursor() and scroll
    let start_x = search_area.x.saturating_add(1).saturating_add(prefix_width);
    let cursor_offset =
        u16::try_from(params.input.visual_cursor().saturating_sub(scroll)).unwrap_or(u16::MAX);
    let cursor_x = start_x.saturating_add(cursor_offset);
    let cursor_y = search_area.y.saturating_add(1);
    if cursor_x
        < search_area
            .x
            .saturating_add(search_area.width.saturating_sub(1))
    {
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

        let [preview_area, preview_footer] = Layout::vertical([
            Constraint::Min(1),    // Diff lines
            Constraint::Length(1), // Footer scroll indicator
        ])
        .areas(preview_inner);

        let max_preview_lines = usize::from(preview_area.height);
        let visible_preview_lines: Vec<ListItem> = params
            .preview_lines
            .iter()
            .skip(params.preview_scroll)
            .take(max_preview_lines)
            .map(|line| {
                let styled_line = format_diff_line(line, theme);
                ListItem::new(styled_line)
            })
            .collect();

        let preview_list = List::new(visible_preview_lines);
        frame.render_widget(preview_list, preview_area);

        // Diff pane scroll position footer
        let scroll_footer = if total_lines > max_preview_lines {
            let current_end =
                (params.preview_scroll.saturating_add(max_preview_lines)).min(total_lines);
            format!(
                "Lines {}-{}/{} (Scroll: \u{2191}/\u{2193})",
                params.preview_scroll.saturating_add(1),
                current_end,
                total_lines
            )
        } else if total_lines > 0 {
            format!("{total_lines} lines")
        } else {
            "No changes".to_string()
        };

        let scroll_footer_p = Paragraph::new(Line::from(vec![Span::styled(
            scroll_footer,
            Style::default().fg(theme.faint_hint),
        )]))
        .alignment(Alignment::Right);
        frame.render_widget(scroll_footer_p, preview_footer);
    }
}
