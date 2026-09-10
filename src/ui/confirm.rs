use crate::theme::Theme;
use crate::ui::header::centered_rect;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};

pub struct ConfirmParams<'a> {
    pub title: &'a str,
    pub lines: &'a [&'a str],
    pub affirmative_label: &'a str,
    pub negative_label: &'a str,
    pub selected_button: usize, // 0 for affirmative, 1 for negative
    pub is_danger: bool,
    pub hint: &'a str,
}

/// Renders a confirmation dialog with themed colors.
pub fn render_confirm(frame: &mut Frame, area: Rect, params: &ConfirmParams, theme: &Theme) {
    let border_color: Color = if params.is_danger {
        theme.danger
    } else {
        theme.warning
    };

    let popup_width = 68.min(area.width.saturating_sub(4));
    let content_lines = u16::try_from(params.lines.len()).unwrap_or(u16::MAX);
    let popup_height = content_lines
        .saturating_add(8)
        .min(area.height.saturating_sub(2));
    let popup_area = centered_rect(popup_width, popup_height, area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let chunks = Layout::vertical([
        Constraint::Length(2),          // Title
        Constraint::Min(content_lines), // Message lines
        Constraint::Length(1),          // Spacer
        Constraint::Length(2),          // Buttons
        Constraint::Length(1),          // Keybind hint
    ])
    .split(inner);

    // Title
    let title_p = Paragraph::new(Line::from(vec![Span::styled(
        params.title,
        Style::default()
            .fg(border_color)
            .add_modifier(Modifier::BOLD),
    )]))
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true });
    frame.render_widget(title_p, chunks[0]);

    // Body
    let text_lines: Vec<Line> = params
        .lines
        .iter()
        .map(|line| {
            Line::from(vec![Span::styled(
                *line,
                Style::default().fg(theme.muted_text),
            )])
        })
        .collect();

    let body_p = Paragraph::new(text_lines)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    frame.render_widget(body_p, chunks[1]);

    // Buttons
    let aff_style = if params.selected_button == 0 {
        Style::default()
            .fg(theme.text)
            .bg(border_color)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.muted_text)
    };

    let neg_style = if params.selected_button == 1 {
        Style::default()
            .fg(theme.text)
            .bg(theme.border)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.muted_text)
    };

    let buttons_line = Line::from(vec![
        Span::raw("   "),
        Span::styled(format!(" [ {} ] ", params.affirmative_label), aff_style),
        Span::raw("      "),
        Span::styled(format!(" [ {} ] ", params.negative_label), neg_style),
        Span::raw("   "),
    ]);

    let buttons_p = Paragraph::new(buttons_line).alignment(Alignment::Center);
    frame.render_widget(buttons_p, chunks[3]);

    // Hint
    let hint_p = Paragraph::new(Line::from(vec![Span::styled(
        params.hint,
        Style::default().fg(theme.faint_hint),
    )]))
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true });
    frame.render_widget(hint_p, chunks[4]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_render_confirm_no_panic() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();

        let params = ConfirmParams {
            title: "CONFIRM TEST",
            lines: &["Line 1", "Line 2"],
            affirmative_label: "Yes",
            negative_label: "No",
            selected_button: 0,
            is_danger: true,
            hint: "Esc to cancel",
        };

        let res = terminal.draw(|f| {
            render_confirm(f, f.area(), &params, &theme);
        });
        assert!(res.is_ok());
    }

    #[test]
    fn test_render_confirm_small_area() {
        let backend = TestBackend::new(10, 4);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();

        let params = ConfirmParams {
            title: "T",
            lines: &["L1"],
            affirmative_label: "Y",
            negative_label: "N",
            selected_button: 1,
            is_danger: false,
            hint: "H",
        };

        let res = terminal.draw(|f| {
            render_confirm(f, f.area(), &params, &theme);
        });
        assert!(res.is_ok());
    }
}
