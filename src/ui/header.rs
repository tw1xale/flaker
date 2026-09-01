use crate::theme::{self, Theme};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
};

/// Renders the main top-level menu header with themed colors.
pub fn render_header(
    frame: &mut Frame,
    area: Rect,
    host: &str,
    user: &str,
    generation: &str,
    flake_target: &str,
    theme: &Theme,
) {
    let title_line = Line::from(vec![
        Span::styled(
            theme::ICON_SNOWFLAKE,
            Style::default().fg(theme.header_title),
        ),
        Span::raw("    "),
        Span::styled(
            "F L A K E R",
            Style::default()
                .fg(theme.header_title)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("    "),
        Span::styled(
            theme::ICON_SNOWFLAKE,
            Style::default().fg(theme.header_title),
        ),
    ]);

    let meta_line = Line::from(vec![Span::styled(
        format!("Host: {host}  •  User: {user}  •  Generation: #{generation}"),
        Style::default().fg(theme.muted_text),
    )]);

    let target_line = Line::from(vec![Span::styled(
        format!("Target: {flake_target}"),
        Style::default().fg(theme.secondary_info),
    )]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border));

    let paragraph = Paragraph::new(vec![
        Line::from(""),
        title_line,
        meta_line,
        target_line,
        Line::from(""),
    ])
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true })
    .block(block);

    frame.render_widget(paragraph, area);
}

/// Helper to center a rectangular area with fixed width and height.
pub fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let horizontal_margin = area.width.saturating_sub(width) / 2;
    let vertical_margin = area.height.saturating_sub(height) / 2;

    let vertical = Layout::vertical([
        Constraint::Length(vertical_margin),
        Constraint::Length(height),
        Constraint::Min(0),
    ])
    .split(area);

    Layout::horizontal([
        Constraint::Length(horizontal_margin),
        Constraint::Length(width),
        Constraint::Min(0),
    ])
    .split(vertical[1])[1]
}
