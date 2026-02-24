use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};

use crate::app::{App, Pane};

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let is_focused = app.active_pane == Pane::InboxList;

    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .title(" Inbox ")
        .borders(Borders::ALL)
        .border_style(border_style);

    let items: Vec<ListItem> = app
        .envelopes
        .iter()
        .map(|env| {
            let star = if env.is_starred { "★ " } else { "  " };
            let unread_marker = if !env.is_read { "● " } else { "  " };
            let attachment = if env.has_attachments { " 📎" } else { "" };
            let date = env.display_date();

            let name_style = if !env.is_read {
                Style::default().add_modifier(Modifier::BOLD).fg(Color::White)
            } else {
                Style::default().fg(Color::Gray)
            };

            let subject_style = if !env.is_read {
                Style::default().fg(Color::White)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let line1 = Line::from(vec![
                Span::styled(unread_marker, Style::default().fg(Color::Cyan)),
                Span::styled(star, Style::default().fg(Color::Yellow)),
                Span::styled(&env.from_name, name_style),
                Span::styled(attachment, Style::default()),
                Span::styled(
                    format!("  {}", date),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);

            let line2 = Line::from(vec![
                Span::raw("    "),
                Span::styled(&env.subject, subject_style),
            ]);

            ListItem::new(vec![line1, line2])
        })
        .collect();

    let highlight_style = Style::default()
        .bg(Color::DarkGray)
        .add_modifier(Modifier::BOLD);

    let list = List::new(items)
        .block(block)
        .highlight_style(highlight_style);

    let mut state = ListState::default();
    state.select(Some(app.selected_index));

    f.render_stateful_widget(list, area, &mut state);
}
