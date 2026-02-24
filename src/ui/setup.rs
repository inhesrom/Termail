use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::app::{App, SetupField};

/// Render the account setup overlay. Does nothing if `app.setup` is `None`.
pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let setup = match &app.setup {
        Some(s) => s,
        None => return,
    };

    // Clear the overlay area
    f.render_widget(Clear, area);

    let block = Block::default()
        .title(" Account Setup ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8), // Instructions
            Constraint::Length(1), // Separator
            Constraint::Length(1), // Name
            Constraint::Length(1), // Email
            Constraint::Length(1), // App Password
            Constraint::Length(1), // Spacer
            Constraint::Length(1), // Submit button
            Constraint::Min(1),   // Spacer
            Constraint::Length(1), // Help line
            Constraint::Length(1), // Status line
        ])
        .split(inner);

    let dim = Style::default().fg(Color::DarkGray);

    // Instructions
    let instructions = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            "Add your Gmail account.",
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from(Span::styled("  To create an App Password:", dim)),
        Line::from(Span::styled("  1. Go to myaccount.google.com", dim)),
        Line::from(Span::styled("  2. Security → 2-Step Verification", dim)),
        Line::from(Span::styled("  3. App passwords → Create one", dim)),
        Line::from(""),
    ])
    .wrap(Wrap { trim: false });
    f.render_widget(instructions, chunks[0]);

    // Separator
    let sep = Paragraph::new("─".repeat(inner.width as usize)).style(dim);
    f.render_widget(sep, chunks[1]);

    let active_style = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    let inactive_style = dim;
    let value_style = Style::default().fg(Color::White);
    let cursor = Span::styled("│", Style::default().fg(Color::Cyan).add_modifier(Modifier::SLOW_BLINK));

    // Name field
    let name_label_style = if setup.active_field == SetupField::Name { active_style } else { inactive_style };
    let mut name_spans = vec![
        Span::styled("Name:          ", name_label_style),
        Span::styled(&setup.name, value_style),
    ];
    if setup.active_field == SetupField::Name {
        name_spans.push(cursor.clone());
    }
    f.render_widget(Paragraph::new(Line::from(name_spans)), chunks[2]);

    // Email field
    let email_label_style = if setup.active_field == SetupField::Email { active_style } else { inactive_style };
    let mut email_spans = vec![
        Span::styled("Email:         ", email_label_style),
        Span::styled(&setup.email, value_style),
    ];
    if setup.active_field == SetupField::Email {
        email_spans.push(cursor.clone());
    }
    f.render_widget(Paragraph::new(Line::from(email_spans)), chunks[3]);

    // App Password field (masked)
    let pw_label_style = if setup.active_field == SetupField::Password { active_style } else { inactive_style };
    let masked: String = "●".repeat(setup.password.len());
    let mut pw_spans = vec![
        Span::styled("App Password:  ", pw_label_style),
        Span::styled(masked, value_style),
    ];
    if setup.active_field == SetupField::Password {
        pw_spans.push(cursor.clone());
    }
    f.render_widget(Paragraph::new(Line::from(pw_spans)), chunks[4]);

    // Submit button
    let submit_active = setup.active_field == SetupField::Submit;
    let button_text = if submit_active {
        Span::styled(
            "  [ Save Account ]  ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            "  [ Save Account ]  ",
            Style::default().fg(Color::Cyan),
        )
    };
    f.render_widget(Paragraph::new(Line::from(button_text)), chunks[6]);

    // Help line
    let help = Paragraph::new(Line::from(vec![
        Span::styled("Tab", Style::default().fg(Color::Cyan)),
        Span::styled(" Next  ", inactive_style),
        Span::styled("Enter", Style::default().fg(Color::Cyan)),
        Span::styled(" Select  ", inactive_style),
        Span::styled("Esc", Style::default().fg(Color::Cyan)),
        Span::styled(" Cancel", inactive_style),
    ]));
    f.render_widget(help, chunks[8]);

    // Status line
    if let Some(status) = &setup.status {
        let status_widget = Paragraph::new(Line::from(Span::styled(
            status.as_str(),
            Style::default().fg(Color::Red),
        )));
        f.render_widget(status_widget, chunks[9]);
    }
}
