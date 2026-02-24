use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{App, SyncStatus};

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let unread = app.unread_count();
    let total = app.envelopes.len();

    let sync_text = match &app.sync_status {
        SyncStatus::Idle => "Ready".to_string(),
        SyncStatus::Syncing => "Syncing...".to_string(),
        SyncStatus::Error(e) => format!("Error: {}", e),
        SyncStatus::LastSync(time) => {
            let elapsed = chrono::Local::now() - *time;
            if elapsed.num_seconds() < 60 {
                "Synced just now".to_string()
            } else {
                format!("Synced {}m ago", elapsed.num_minutes())
            }
        }
    };

    let sync_color = match &app.sync_status {
        SyncStatus::Syncing => Color::Yellow,
        SyncStatus::Error(_) => Color::Red,
        _ => Color::Green,
    };

    let status = Line::from(vec![
        Span::styled(" INBOX", Style::default().fg(Color::Cyan)),
        Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{} unread", unread), Style::default().fg(Color::White)),
        Span::styled(
            format!(" / {} total", total),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
        Span::styled(sync_text, Style::default().fg(sync_color)),
        Span::styled(
            "        j/k ↑↓ Navigate  Enter Open  Tab Switch  / Search  c Compose  r Refresh  S Setup  L Logs  X Reset  q Quit ",
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    let widget = Paragraph::new(status).style(Style::default().bg(Color::Rgb(30, 30, 30)));
    f.render_widget(widget, area);
}
