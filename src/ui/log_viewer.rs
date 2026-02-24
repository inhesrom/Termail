use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::app::App;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let lv = match &app.log_viewer {
        Some(lv) => lv,
        None => return,
    };

    f.render_widget(Clear, area);

    let block = Block::default()
        .title(" Logs ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // Log content
            Constraint::Length(1), // Help line
        ])
        .split(inner);

    if lv.lines.is_empty() {
        let loading = Paragraph::new("Loading logs...")
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(loading, chunks[0]);
    } else {
        let lines: Vec<Line> = lv
            .lines
            .iter()
            .map(|line| {
                let color = if line.contains(" ERROR ") {
                    Color::Red
                } else if line.contains(" WARN ") {
                    Color::Yellow
                } else if line.contains(" INFO ") {
                    Color::Green
                } else if line.contains(" DEBUG ") {
                    Color::DarkGray
                } else {
                    Color::White
                };
                Line::from(Span::styled(line.as_str(), Style::default().fg(color)))
            })
            .collect();

        let content = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((lv.scroll, 0));
        f.render_widget(content, chunks[0]);
    }

    let help = Paragraph::new(Line::from(vec![
        Span::styled("j/k", Style::default().fg(Color::Cyan)),
        Span::styled(" Scroll  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Esc/q", Style::default().fg(Color::Cyan)),
        Span::styled(" Close", Style::default().fg(Color::DarkGray)),
    ]));
    f.render_widget(help, chunks[1]);
}
