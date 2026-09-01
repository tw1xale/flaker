use crate::theme::{self, Theme};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem},
};

pub struct MenuParams<'a> {
    pub header_title: &'a str,
    pub items: &'a [&'a str],
    pub selected_index: usize,
    pub show_numbers: bool,
    pub back_item_key: &'a str,
}

/// Renders a selectable menu list with digit prefixes for actions, custom key for Back/Exit, and themed colors.
pub fn render_menu(frame: &mut Frame, area: Rect, params: &MenuParams, theme: &Theme) {
    let total_items = params.items.len();
    let list_items: Vec<ListItem> = params
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let is_selected = i == params.selected_index;
            let item_style = if is_selected {
                Style::default()
                    .fg(theme.selected)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text)
            };

            let mut spans = vec![Span::raw(" ")];

            if params.show_numbers {
                let num_style = if is_selected {
                    Style::default()
                        .fg(theme.selected)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.secondary_info)
                };

                if i + 1 == total_items && !params.back_item_key.trim().is_empty() {
                    spans.push(Span::styled(
                        format!("{}. ", params.back_item_key.trim()),
                        num_style,
                    ));
                } else {
                    spans.push(Span::styled(format!("{}. ", i + 1), num_style));
                }
            }

            spans.push(Span::styled(*item, item_style));

            let line = Line::from(spans);
            ListItem::new(line)
        })
        .collect();

    let block = Block::default()
        .title(Span::styled(
            format!("  {} {}  ", theme::ICON_BOLT, params.header_title),
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
