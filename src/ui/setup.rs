use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::app::{
    App, GmailSetupField, OutlookSetupField, ProviderOption, SetupPhase,
};

/// Render the account setup overlay. Does nothing if `app.setup` is `None`.
pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let setup = match &app.setup {
        Some(s) => s,
        None => return,
    };

    f.render_widget(Clear, area);

    match &setup.phase {
        SetupPhase::AccountList { accounts, selected } => {
            render_account_list(f, area, accounts, *selected, setup.status.as_deref());
        }
        SetupPhase::ProviderSelect { selected } => {
            render_provider_select(f, area, *selected, setup.status.as_deref());
        }
        SetupPhase::GmailFields { name, email, password, active_field } => {
            render_gmail_fields(f, area, name, email, password, *active_field, setup.status.as_deref());
        }
        SetupPhase::OutlookFields { name, email, active_field } => {
            render_outlook_fields(f, area, name, email, *active_field, setup.status.as_deref());
        }
        SetupPhase::OutlookDeviceCode { verification_uri, user_code, .. } => {
            render_device_code(f, area, verification_uri, user_code, setup.status.as_deref());
        }
    }
}

fn render_account_list(
    f: &mut Frame,
    area: Rect,
    accounts: &[crate::app::AccountListItem],
    selected: usize,
    status: Option<&str>,
) {
    let block = Block::default()
        .title(" Accounts ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let dim = Style::default().fg(Color::DarkGray);
    let highlight = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    let normal = Style::default().fg(Color::White);

    // Layout: accounts + add + done + spacer + help + status
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));

    for (i, acct) in accounts.iter().enumerate() {
        let icon = match acct.provider {
            crate::models::account::Provider::Gmail => "G",
            crate::models::account::Provider::Outlook => "O",
        };
        let style = if i == selected { highlight } else { normal };
        let prefix = if i == selected { "> " } else { "  " };
        lines.push(Line::from(vec![
            Span::styled(prefix, style),
            Span::styled(format!("[{}] ", icon), Style::default().fg(Color::Yellow)),
            Span::styled(&acct.name, style),
            Span::styled(format!("  <{}>", acct.email), dim),
        ]));
    }

    lines.push(Line::from(""));

    // "Add Account" button
    let add_idx = accounts.len();
    let add_style = if selected == add_idx { highlight } else { normal };
    let add_prefix = if selected == add_idx { "> " } else { "  " };
    lines.push(Line::from(Span::styled(
        format!("{}[ + Add Account ]", add_prefix),
        add_style,
    )));

    // "Done" button
    let done_idx = accounts.len() + 1;
    let done_style = if selected == done_idx { highlight } else { normal };
    let done_prefix = if selected == done_idx { "> " } else { "  " };
    lines.push(Line::from(Span::styled(
        format!("{}[ Done ]", done_prefix),
        done_style,
    )));

    lines.push(Line::from(""));

    // Help
    lines.push(Line::from(vec![
        Span::styled("Up/Down", Style::default().fg(Color::Cyan)),
        Span::styled(" Navigate  ", dim),
        Span::styled("Enter", Style::default().fg(Color::Cyan)),
        Span::styled(" Select  ", dim),
        Span::styled("Esc", Style::default().fg(Color::Cyan)),
        Span::styled(" Close", dim),
    ]));

    if let Some(status) = status {
        lines.push(Line::from(Span::styled(
            status,
            Style::default().fg(Color::Green),
        )));
    }

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn render_provider_select(
    f: &mut Frame,
    area: Rect,
    selected: ProviderOption,
    status: Option<&str>,
) {
    let block = Block::default()
        .title(" Add Account — Choose Provider ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let dim = Style::default().fg(Color::DarkGray);
    let highlight = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    let normal = Style::default().fg(Color::White);

    let gmail_style = if selected == ProviderOption::Gmail { highlight } else { normal };
    let outlook_style = if selected == ProviderOption::Outlook { highlight } else { normal };
    let gmail_prefix = if selected == ProviderOption::Gmail { "> " } else { "  " };
    let outlook_prefix = if selected == ProviderOption::Outlook { "> " } else { "  " };

    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Choose your email provider:",
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(gmail_prefix, gmail_style),
            Span::styled("[G]  Gmail", gmail_style),
        ]),
        Line::from(vec![
            Span::styled(outlook_prefix, outlook_style),
            Span::styled("[O]  Outlook 365 / Microsoft", outlook_style),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Up/Down", Style::default().fg(Color::Cyan)),
            Span::styled(" Navigate  ", dim),
            Span::styled("Enter", Style::default().fg(Color::Cyan)),
            Span::styled(" Select  ", dim),
            Span::styled("Esc", Style::default().fg(Color::Cyan)),
            Span::styled(" Back", dim),
        ]),
    ];

    if let Some(status) = status {
        lines.push(Line::from(Span::styled(
            status,
            Style::default().fg(Color::Red),
        )));
    }

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn render_gmail_fields(
    f: &mut Frame,
    area: Rect,
    name: &str,
    email: &str,
    password: &str,
    active_field: GmailSetupField,
    status: Option<&str>,
) {
    let block = Block::default()
        .title(" Add Gmail Account ")
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
    let active_style = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    let inactive_style = dim;
    let value_style = Style::default().fg(Color::White);
    let cursor = Span::styled("│", Style::default().fg(Color::Cyan).add_modifier(Modifier::SLOW_BLINK));

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

    // Name field
    let name_label_style = if active_field == GmailSetupField::Name { active_style } else { inactive_style };
    let mut name_spans = vec![
        Span::styled("Name:          ", name_label_style),
        Span::styled(name, value_style),
    ];
    if active_field == GmailSetupField::Name {
        name_spans.push(cursor.clone());
    }
    f.render_widget(Paragraph::new(Line::from(name_spans)), chunks[2]);

    // Email field
    let email_label_style = if active_field == GmailSetupField::Email { active_style } else { inactive_style };
    let mut email_spans = vec![
        Span::styled("Email:         ", email_label_style),
        Span::styled(email, value_style),
    ];
    if active_field == GmailSetupField::Email {
        email_spans.push(cursor.clone());
    }
    f.render_widget(Paragraph::new(Line::from(email_spans)), chunks[3]);

    // App Password field (masked)
    let pw_label_style = if active_field == GmailSetupField::Password { active_style } else { inactive_style };
    let masked: String = "●".repeat(password.len());
    let mut pw_spans = vec![
        Span::styled("App Password:  ", pw_label_style),
        Span::styled(masked, value_style),
    ];
    if active_field == GmailSetupField::Password {
        pw_spans.push(cursor.clone());
    }
    f.render_widget(Paragraph::new(Line::from(pw_spans)), chunks[4]);

    // Submit button
    let submit_active = active_field == GmailSetupField::Submit;
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
        Span::styled(" Back", inactive_style),
    ]));
    f.render_widget(help, chunks[8]);

    // Status line
    if let Some(status) = status {
        let status_widget = Paragraph::new(Line::from(Span::styled(
            status,
            Style::default().fg(Color::Red),
        )));
        f.render_widget(status_widget, chunks[9]);
    }
}

fn render_outlook_fields(
    f: &mut Frame,
    area: Rect,
    name: &str,
    email: &str,
    active_field: OutlookSetupField,
    status: Option<&str>,
) {
    let block = Block::default()
        .title(" Add Outlook 365 Account ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6), // Instructions
            Constraint::Length(1), // Separator
            Constraint::Length(1), // Name
            Constraint::Length(1), // Email
            Constraint::Length(1), // Spacer
            Constraint::Length(1), // Submit button
            Constraint::Min(1),   // Spacer
            Constraint::Length(1), // Help line
            Constraint::Length(1), // Status line
        ])
        .split(inner);

    let dim = Style::default().fg(Color::DarkGray);
    let active_style = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    let inactive_style = dim;
    let value_style = Style::default().fg(Color::White);
    let cursor = Span::styled("│", Style::default().fg(Color::Cyan).add_modifier(Modifier::SLOW_BLINK));

    // Instructions
    let instructions = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            "Add your Microsoft 365 / Outlook account.",
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from(Span::styled("  You will sign in with your Microsoft account", dim)),
        Line::from(Span::styled("  using a browser-based device code flow.", dim)),
        Line::from(""),
    ])
    .wrap(Wrap { trim: false });
    f.render_widget(instructions, chunks[0]);

    // Separator
    let sep = Paragraph::new("─".repeat(inner.width as usize)).style(dim);
    f.render_widget(sep, chunks[1]);

    // Name field
    let name_label_style = if active_field == OutlookSetupField::Name { active_style } else { inactive_style };
    let mut name_spans = vec![
        Span::styled("Name:   ", name_label_style),
        Span::styled(name, value_style),
    ];
    if active_field == OutlookSetupField::Name {
        name_spans.push(cursor.clone());
    }
    f.render_widget(Paragraph::new(Line::from(name_spans)), chunks[2]);

    // Email field
    let email_label_style = if active_field == OutlookSetupField::Email { active_style } else { inactive_style };
    let mut email_spans = vec![
        Span::styled("Email:  ", email_label_style),
        Span::styled(email, value_style),
    ];
    if active_field == OutlookSetupField::Email {
        email_spans.push(cursor.clone());
    }
    f.render_widget(Paragraph::new(Line::from(email_spans)), chunks[3]);

    // Submit button
    let submit_active = active_field == OutlookSetupField::Submit;
    let button_text = if submit_active {
        Span::styled(
            "  [ Sign in with Microsoft ]  ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            "  [ Sign in with Microsoft ]  ",
            Style::default().fg(Color::Cyan),
        )
    };
    f.render_widget(Paragraph::new(Line::from(button_text)), chunks[5]);

    // Help line
    let help = Paragraph::new(Line::from(vec![
        Span::styled("Tab", Style::default().fg(Color::Cyan)),
        Span::styled(" Next  ", inactive_style),
        Span::styled("Enter", Style::default().fg(Color::Cyan)),
        Span::styled(" Select  ", inactive_style),
        Span::styled("Esc", Style::default().fg(Color::Cyan)),
        Span::styled(" Back", inactive_style),
    ]));
    f.render_widget(help, chunks[7]);

    // Status line
    if let Some(status) = status {
        let status_widget = Paragraph::new(Line::from(Span::styled(
            status,
            Style::default().fg(Color::Red),
        )));
        f.render_widget(status_widget, chunks[8]);
    }
}

fn render_device_code(
    f: &mut Frame,
    area: Rect,
    verification_uri: &str,
    user_code: &str,
    status: Option<&str>,
) {
    let block = Block::default()
        .title(" Microsoft Sign-In ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let dim = Style::default().fg(Color::DarkGray);
    let bold_white = Style::default().fg(Color::White).add_modifier(Modifier::BOLD);
    let code_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);

    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  To sign in, open a browser and go to:",
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!("    {}", verification_uri),
            bold_white,
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  Then enter this code:",
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!("    {}", user_code),
            code_style,
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  Waiting for authentication...",
            dim,
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("Esc", Style::default().fg(Color::Cyan)),
            Span::styled(" Cancel", dim),
        ]),
    ];

    if let Some(status) = status {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            status,
            Style::default().fg(Color::Red),
        )));
    }

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}
