use crate::theme::{self, Theme};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem},
};

/// Renders a selectable menu list with optional digit prefixes and themed colors.
pub fn render_menu(
    frame: &mut Frame,
    area: Rect,
    header_title: &str,
    items: &[&str],
    selected_index: usize,
    show_numbers: bool,
    theme: &Theme,
) {
    let list_items: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let is_selected = i == selected_index;
            let item_style = if is_selected {
                Style::default()
                    .fg(theme.selected)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text)
            };

            let mut spans = vec![Span::raw(" ")];

            if show_numbers {
                let num_style = if is_selected {
                    Style::default()
                        .fg(theme.selected)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.secondary_info)
                };
                spans.push(Span::styled(format!("{}. ", i + 1), num_style));
            }

            spans.push(Span::styled(*item, item_style));

            let line = Line::from(spans);
            ListItem::new(line)
        })
        .collect();

    let block = Block::default()
        .title(Span::styled(
            format!("  {} {header_title}  ", theme::ICON_BOLT),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border));

    let list = List::new(list_items).block(block);

    frame.render_widget(list, area);
}
