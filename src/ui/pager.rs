use crate::theme;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

/// Renders a scrollable text pager for diffs or generation history with dynamic keybinding hint.
pub fn render_pager(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    content_lines: &[String],
    scroll_offset: usize,
    footer_hint: &str,
) {
    let popup_area = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Span::styled(
            format!("  {title}  "),
            Style::default()
                .fg(theme::HEADER_TITLE)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let chunks = Layout::vertical([
        Constraint::Min(1),    // Text content
        Constraint::Length(1), // Footer / Navigation hint
    ])
    .split(inner);

    let visible_height = chunks[0].height as usize;
    let visible_lines: Vec<Line> = content_lines
        .iter()
        .skip(scroll_offset)
        .take(visible_height)
        .map(|raw_line| {
            let style = if raw_line.starts_with('+') && !raw_line.starts_with("+++") {
                Style::default().fg(theme::SUCCESS)
            } else if raw_line.starts_with('-') && !raw_line.starts_with("---") {
                Style::default().fg(theme::DANGER)
            } else if raw_line.starts_with("@@") {
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD)
            } else if raw_line.starts_with("diff --git") || raw_line.starts_with("index ") {
                Style::default()
                    .fg(theme::SECONDARY_INFO)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT)
            };

            Line::from(vec![Span::styled(raw_line.as_str(), style)])
        })
        .collect();

    let paragraph = Paragraph::new(visible_lines);
    frame.render_widget(paragraph, chunks[0]);

    let footer_p = Paragraph::new(Line::from(vec![Span::styled(
        footer_hint,
        Style::default().fg(theme::FAINT_HINT),
    )]))
    .alignment(Alignment::Right);
    frame.render_widget(footer_p, chunks[1]);
}
