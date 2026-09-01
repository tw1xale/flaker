use crate::theme;
use crate::ui::header::centered_rect;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};

/// Renders the result modal screen with dynamic return hint.
pub fn render_result(
    frame: &mut Frame,
    area: Rect,
    is_success: bool,
    title: &str,
    message: &str,
    hint: &str,
) {
    let border_color: Color = if is_success {
        theme::SUCCESS
    } else {
        theme::DANGER
    };

    let popup_width = 68.min(area.width.saturating_sub(4));
    let msg_lines =
        (message.len() / (popup_width as usize - 4)).max(1) as u16 + message.lines().count() as u16;
    let popup_height = (msg_lines + 6).min(area.height.saturating_sub(2)).max(8);
    let popup_area = centered_rect(popup_width, popup_height, area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let chunks = Layout::vertical([
        Constraint::Length(2), // Title
        Constraint::Min(2),    // Message
        Constraint::Length(1), // Return hint
    ])
    .split(inner);

    // Title
    let title_icon = if is_success {
        theme::ICON_SUCCESS
    } else {
        theme::ICON_ERROR
    };

    let title_p = Paragraph::new(Line::from(vec![Span::styled(
        format!("{}  {}", title_icon, title),
        Style::default()
            .fg(border_color)
            .add_modifier(Modifier::BOLD),
    )]))
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true });
    frame.render_widget(title_p, chunks[0]);

    // Message
    let msg_p = Paragraph::new(Line::from(vec![Span::styled(
        message,
        Style::default().fg(theme::MUTED_TEXT),
    )]))
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true });
    frame.render_widget(msg_p, chunks[1]);

    // Hint
    let hint_p = Paragraph::new(Line::from(vec![Span::styled(
        hint,
        Style::default().fg(theme::FAINT_HINT),
    )]))
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true });
    frame.render_widget(hint_p, chunks[2]);
}
