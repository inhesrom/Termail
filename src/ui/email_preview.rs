use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::{App, Pane};

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let is_focused = app.active_pane == Pane::EmailPreview;

    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .title(" Email ")
        .borders(Borders::ALL)
        .border_style(border_style);

    if let Some(email) = &app.selected_email {
        // Split into header area and body area
        let inner = block.inner(area);
        f.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(5), // Headers
                Constraint::Length(1), // Separator
                Constraint::Min(1),   // Body
            ])
            .split(inner);

        // Email headers
        let mut headers = vec![
            Line::from(vec![
                Span::styled("From: ", Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
                Span::styled(
                    format!("{} <{}>", email.from_name, email.from_address),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::styled("To:   ", Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
                Span::styled(
                    email.to.join(", "),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::styled("Subj: ", Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
                Span::styled(
                    &email.subject,
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("Date: ", Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
                Span::styled(
                    email.date.format("%b %d, %Y %l:%M %p").to_string().trim().to_string(),
                    Style::default().fg(Color::White),
                ),
            ]),
        ];

        if !email.attachments.is_empty() {
            headers.push(Line::from(vec![
                Span::styled("📎 ", Style::default()),
                Span::styled(
                    format!("{} attachment(s)", email.attachments.len()),
                    Style::default().fg(Color::Yellow),
                ),
            ]));
        }

        let header_widget = Paragraph::new(headers);
        f.render_widget(header_widget, chunks[0]);

        // Separator
        let separator = Paragraph::new(Line::from("─".repeat(inner.width as usize)))
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(separator, chunks[1]);

        // Body
        let body = Paragraph::new(email.body_text.as_str())
            .wrap(Wrap { trim: false })
            .scroll((app.preview_scroll, 0))
            .style(Style::default().fg(Color::White));
        f.render_widget(body, chunks[2]);
    } else if !app.has_accounts {
        // No accounts configured — show welcome message
        let welcome = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "Welcome to Termail!",
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "No accounts configured.",
                Style::default().fg(Color::White),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Press S to add an account",
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "to connect your Gmail account.",
                Style::default().fg(Color::White),
            )),
        ])
        .block(block)
        .wrap(Wrap { trim: false });
        f.render_widget(welcome, area);
    } else {
        // No email selected
        let empty = Paragraph::new("No email selected")
            .block(block)
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(empty, area);
    }
}
