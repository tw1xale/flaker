use crate::theme::{self, Theme};
use crate::ui::header::centered_rect;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};
use tui_input::Input;

/// Renders the commit message input modal with dynamic keybinding hint and themed colors.
pub fn render_input_modal(
    frame: &mut Frame,
    area: Rect,
    action_name: &str,
    input: &Input,
    default_text: &str,
    hint: &str,
    theme: &Theme,
) {
    let popup_width = 68.min(area.width.saturating_sub(4));
    let popup_height = 11.min(area.height.saturating_sub(2));
    let popup_area = centered_rect(popup_width, popup_height, area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let chunks = Layout::vertical([
        Constraint::Length(1), // Title
        Constraint::Length(1), // Action description
        Constraint::Length(2), // Hint (can wrap)
        Constraint::Min(1),    // Spacer / dynamic breathing room
        Constraint::Length(3), // Input field
    ])
    .split(inner);

    // Title
    let title_p = Paragraph::new(Line::from(vec![Span::styled(
        format!("{}  COMMIT TO REPOSITORY", theme::ICON_COMMIT),
        Style::default()
            .fg(theme.header_title)
            .add_modifier(Modifier::BOLD),
    )]))
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true });
    frame.render_widget(title_p, chunks[0]);

    // Action name
    let action_p = Paragraph::new(Line::from(vec![Span::styled(
        format!("Action: {action_name}"),
        Style::default().fg(theme.muted_text),
    )]))
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true });
    frame.render_widget(action_p, chunks[1]);

    // Hint
    let hint_p = Paragraph::new(Line::from(vec![Span::styled(
        hint,
        Style::default().fg(theme.faint_hint),
    )]))
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true });
    frame.render_widget(hint_p, chunks[2]);

    // Input box
    let display_text = if input.value().is_empty() {
        Span::styled(default_text, Style::default().fg(theme.faint_hint))
    } else {
        Span::styled(input.value(), Style::default().fg(theme.text))
    };

    let input_line = Line::from(vec![Span::raw(" "), display_text]);

    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border));

    let input_p = Paragraph::new(input_line).block(input_block);
    frame.render_widget(input_p, chunks[4]);

    // Position cursor exactly based on input.visual_cursor()
    let start_x = chunks[4].x + 2; // block left border (1) + left margin space (1)
    let cursor_x = start_x + input.visual_cursor() as u16;
    let cursor_y = chunks[4].y + 1;
    if cursor_x < chunks[4].x + chunks[4].width.saturating_sub(1)
        && cursor_y < chunks[4].y + chunks[4].height.saturating_sub(1)
    {
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}
