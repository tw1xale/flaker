use crate::theme::{self, Theme};
use crate::ui::header::centered_rect;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};

/// Renders the result modal screen with dynamic return hint and themed colors.
pub fn render_result(
    frame: &mut Frame,
    area: Rect,
    is_success: bool,
    title: &str,
    message: &str,
    hint: &str,
    theme: &Theme,
) {
    let border_color: Color = if is_success {
        theme.success
    } else {
        theme.danger
    };

    let popup_width = 68.min(area.width.saturating_sub(4));
    let wrap_width = (popup_width as usize).saturating_sub(4).max(1);
    let msg_lines = (message.len() / wrap_width).max(1) as u16 + message.lines().count() as u16;
    let popup_height = (msg_lines + 6).min(area.height.saturating_sub(2)).max(1);
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
        format!("{title_icon}  {title}"),
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
        Style::default().fg(theme.muted_text),
    )]))
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true });
    frame.render_widget(msg_p, chunks[1]);

    // Hint
    let hint_p = Paragraph::new(Line::from(vec![Span::styled(
        hint,
        Style::default().fg(theme.faint_hint),
    )]))
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true });
    frame.render_widget(hint_p, chunks[2]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_render_result_small_area_no_panic() {
        let backend = TestBackend::new(4, 4);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();

        let res = terminal.draw(|f| {
            render_result(
                f,
                f.area(),
                true,
                "TEST TITLE",
                "Some message that is relatively long",
                "Press Enter",
                &theme,
            );
        });
        assert!(res.is_ok());
    }

    #[test]
    fn test_render_result_standard_area() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();

        let res = terminal.draw(|f| {
            render_result(
                f,
                f.area(),
                false,
                "ERROR TITLE",
                "Operation failed with error description",
                "Press Enter to return",
                &theme,
            );
        });
        assert!(res.is_ok());
    }
}
