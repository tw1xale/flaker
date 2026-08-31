use crate::theme;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem},
};

/// Renders a selectable menu list.
pub fn render_menu(
    frame: &mut Frame,
    area: Rect,
    header_title: &str,
    items: &[&str],
    selected_index: usize,
) {
    let list_items: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let is_selected = i == selected_index;
            let style = if is_selected {
                Style::default()
                    .fg(theme::SELECTED)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT)
            };

            let line = Line::from(vec![Span::raw(" "), Span::styled(*item, style)]);

            ListItem::new(line)
        })
        .collect();

    let block = Block::default()
        .title(Span::styled(
            format!("  {} {header_title}  ", theme::ICON_BOLT),
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER));

    let list = List::new(list_items).block(block);

    frame.render_widget(list, area);
}
