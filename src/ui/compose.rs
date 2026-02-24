use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::app::{App, ComposeField};

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let compose = match &app.compose {
        Some(c) => c,
        None => return,
    };

    // Clear the overlay area
    f.render_widget(Clear, area);

    let block = Block::default()
        .title(" Compose ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // To
            Constraint::Length(1), // Cc
            Constraint::Length(1), // Subject
            Constraint::Length(1), // Separator
            Constraint::Min(3),   // Body
            Constraint::Length(1), // Help line
        ])
        .split(inner);

    let active_style = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    let inactive_style = Style::default().fg(Color::DarkGray);
    let value_style = Style::default().fg(Color::White);
    let cursor = Span::styled("│", Style::default().fg(Color::Cyan).add_modifier(Modifier::SLOW_BLINK));

    // To field
    let to_label_style = if compose.active_field == ComposeField::To { active_style } else { inactive_style };
    let mut to_spans = vec![
        Span::styled("To:      ", to_label_style),
        Span::styled(&compose.to, value_style),
    ];
    if compose.active_field == ComposeField::To {
        to_spans.push(cursor.clone());
    }
    f.render_widget(Paragraph::new(Line::from(to_spans)), chunks[0]);

    // Cc field
    let cc_label_style = if compose.active_field == ComposeField::Cc { active_style } else { inactive_style };
    let mut cc_spans = vec![
        Span::styled("Cc:      ", cc_label_style),
        Span::styled(&compose.cc, value_style),
    ];
    if compose.active_field == ComposeField::Cc {
        cc_spans.push(cursor.clone());
    }
    f.render_widget(Paragraph::new(Line::from(cc_spans)), chunks[1]);

    // Subject field
    let subj_label_style = if compose.active_field == ComposeField::Subject { active_style } else { inactive_style };
    let mut subj_spans = vec![
        Span::styled("Subject: ", subj_label_style),
        Span::styled(&compose.subject, value_style),
    ];
    if compose.active_field == ComposeField::Subject {
        subj_spans.push(cursor.clone());
    }
    f.render_widget(Paragraph::new(Line::from(subj_spans)), chunks[2]);

    // Separator
    let sep = Paragraph::new("─".repeat(inner.width as usize))
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(sep, chunks[3]);

    // Body
    let body_style = if compose.active_field == ComposeField::Body { active_style } else { inactive_style };
    let body_text = if compose.active_field == ComposeField::Body {
        format!("{}│", compose.body)
    } else {
        compose.body.clone()
    };
    let body_widget = Paragraph::new(body_text)
        .style(if compose.active_field == ComposeField::Body { value_style } else { body_style })
        .wrap(Wrap { trim: false });
    f.render_widget(body_widget, chunks[4]);

    // Help line
    let help = Paragraph::new(Line::from(vec![
        Span::styled("Tab", Style::default().fg(Color::Cyan)),
        Span::styled(" Next field  ", inactive_style),
        Span::styled("Ctrl+S", Style::default().fg(Color::Cyan)),
        Span::styled(" Send  ", inactive_style),
        Span::styled("Esc", Style::default().fg(Color::Cyan)),
        Span::styled(" Cancel", inactive_style),
    ]));
    f.render_widget(help, chunks[5]);
}
