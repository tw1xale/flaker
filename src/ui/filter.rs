use crate::theme;
use crate::ui::header::centered_rect;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap},
};
use tui_input::Input;

/// Renders the commit selection fuzzy filter modal with dynamic keybinding hint.
pub fn render_filter(
    frame: &mut Frame,
    area: Rect,
    header_title: &str,
    input: &Input,
    filtered_items: &[(&str, Vec<usize>)], // (commit_line, matched_char_indices)
    selected_index: usize,
    hint: &str,
) {
    let popup_width = 76.min(area.width.saturating_sub(4));
    let popup_height = 23.min(area.height.saturating_sub(2));
    let popup_area = centered_rect(popup_width, popup_height, area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::WARNING));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let chunks = Layout::vertical([
        Constraint::Length(1), // Title
        Constraint::Length(1), // Subtitle
        Constraint::Length(2), // Hint (can wrap to 2 lines if needed)
        Constraint::Length(3), // Search bar
        Constraint::Length(1), // Spacer
        Constraint::Min(4),    // Commits list
    ])
    .split(inner);

    // Title
    let title_p = Paragraph::new(Line::from(vec![Span::styled(
        format!("{}  SELECT COMMIT", theme::ICON_HISTORY),
        Style::default()
            .fg(theme::WARNING)
            .add_modifier(Modifier::BOLD),
    )]))
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true });
    frame.render_widget(title_p, chunks[0]);

    // Subtitle
    let subtitle_p = Paragraph::new(Line::from(vec![Span::styled(
        header_title,
        Style::default().fg(theme::NEUTRAL_TEXT),
    )]))
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true });
    frame.render_widget(subtitle_p, chunks[1]);

    // Hint
    let hint_p = Paragraph::new(Line::from(vec![Span::styled(
        hint,
        Style::default().fg(theme::FAINT_HINT),
    )]))
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true });
    frame.render_widget(hint_p, chunks[2]);

    // Search bar with 1-cell horizontal inset to prevent border bleed
    let search_area = Layout::horizontal([
        Constraint::Length(1), // Left padding
        Constraint::Min(10),   // Search input box
        Constraint::Length(1), // Right padding
    ])
    .split(chunks[3])[1];

    let prefix_text = format!(" {} Search: ", theme::ICON_SEARCH);
    let prefix_width = 1 + 1 + 9; // " " + glyph (1) + " Search: " (9) = 11

    let search_line = Line::from(vec![
        Span::styled(prefix_text, Style::default().fg(theme::BORDER)),
        if input.value().is_empty() {
            Span::styled(
                "Filter by hash, date, or message...",
                Style::default().fg(theme::FAINT_HINT),
            )
        } else {
            Span::styled(input.value(), Style::default().fg(theme::TEXT))
        },
    ]);

    let search_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER));

    let search_p = Paragraph::new(search_line).block(search_block);
    frame.render_widget(search_p, search_area);

    // Commits list
    let list_area = chunks[5];
    let max_visible = list_area.height as usize;
    let scroll_offset = if selected_index >= max_visible {
        selected_index - max_visible + 1
    } else {
        0
    };

    let max_col_width = list_area.width.saturating_sub(2) as usize;

    let list_items: Vec<ListItem> = filtered_items
        .iter()
        .skip(scroll_offset)
        .take(max_visible)
        .enumerate()
        .map(|(visible_idx, (commit_str, matches))| {
            let actual_idx = visible_idx + scroll_offset;
            let is_selected = actual_idx == selected_index;

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
                        .fg(theme::HEADER_TITLE)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                } else if is_selected {
                    Style::default()
                        .fg(theme::SELECTED)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme::TEXT)
                };

                spans.push(Span::styled(ch.to_string(), style));
            }

            if truncated {
                spans.push(Span::styled("…", Style::default().fg(theme::FAINT_HINT)));
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(list_items);
    frame.render_widget(list, list_area);

    // Set cursor on search query according to input.visual_cursor()
    let start_x = search_area.x + 1 + prefix_width;
    let cursor_x = start_x + input.visual_cursor() as u16;
    let cursor_y = search_area.y + 1;
    if cursor_x < search_area.x + search_area.width.saturating_sub(1) {
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}
