use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::App;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    if app.search_active {
        let search_line = Line::from(vec![
            Span::styled(" 🔍 ", Style::default().fg(Color::Yellow)),
            Span::styled(&app.search_query, Style::default().fg(Color::White)),
            Span::styled("│", Style::default().fg(Color::Cyan).add_modifier(Modifier::SLOW_BLINK)),
        ]);

        let search_block = Block::default()
            .title(" Search ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let widget = Paragraph::new(search_line).block(search_block);
        f.render_widget(widget, area);
    } else {
        let header_line = Line::from(vec![
            Span::styled(" 📧 ", Style::default()),
            Span::styled("Gmail", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled(
                format!(" - {}", app.account_email),
                Style::default().fg(Color::DarkGray),
            ),
        ]);

        let widget = Paragraph::new(header_line).block(block);
        f.render_widget(widget, area);
    }
}
