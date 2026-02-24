use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyModifiers, MouseEventKind};
use tokio::sync::mpsc;

use crate::message::Message;

/// Input mode determines how keystrokes are interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Search,
    Compose,
    LogViewer,
    Setup,
}

/// Polls crossterm events and converts them to Messages.
/// Runs in a dedicated async task so the main loop never blocks.
pub struct EventHandler {
    rx: mpsc::UnboundedReceiver<Message>,
    mode_tx: tokio::sync::watch::Sender<InputMode>,
}

impl EventHandler {
    pub fn new(tick_rate: Duration) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let (mode_tx, mode_rx) = tokio::sync::watch::channel(InputMode::Normal);

        tokio::spawn(async move {
            loop {
                if event::poll(tick_rate).unwrap_or(false) {
                    if let Ok(ev) = event::read() {
                        let mode = *mode_rx.borrow();
                        let msg = match ev {
                            Event::Key(key) => convert_key(key, mode),
                            Event::Mouse(mouse) => convert_mouse(mouse, mode),
                            Event::Resize(w, h) => Some(Message::Resize(w, h)),
                            _ => None,
                        };
                        if let Some(m) = msg
                            && tx.send(m).is_err()
                        {
                            break;
                        }
                    }
                } else {
                    // Tick for animations
                    if tx.send(Message::Tick).is_err() {
                        break;
                    }
                }
            }
        });

        Self { rx, mode_tx }
    }

    pub async fn next(&mut self) -> Option<Message> {
        self.rx.recv().await
    }

    /// Update the input mode so the event handler routes keys correctly.
    pub fn set_mode(&self, mode: InputMode) {
        let _ = self.mode_tx.send(mode);
    }
}

fn convert_key(key: event::KeyEvent, mode: InputMode) -> Option<Message> {
    if key.kind != event::KeyEventKind::Press {
        return None;
    }

    match mode {
        InputMode::Setup => convert_setup_key(key),
        InputMode::Search => convert_search_key(key),
        InputMode::Compose => convert_compose_key(key),
        InputMode::LogViewer => convert_log_viewer_key(key),
        InputMode::Normal => convert_normal_key(key),
    }
}

fn convert_normal_key(key: event::KeyEvent) -> Option<Message> {
    match (key.modifiers, key.code) {
        // Quit
        (KeyModifiers::NONE, KeyCode::Char('q')) => Some(Message::Quit),
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => Some(Message::Quit),

        // Navigation
        (KeyModifiers::NONE, KeyCode::Char('j') | KeyCode::Down) => Some(Message::SelectNext),
        (KeyModifiers::NONE, KeyCode::Char('k') | KeyCode::Up) => Some(Message::SelectPrevious),
        (KeyModifiers::NONE, KeyCode::Enter) => Some(Message::OpenSelected),
        (KeyModifiers::NONE, KeyCode::Tab) => Some(Message::SwitchPane),

        // Preview scrolling (Shift+j/k or Shift+arrows)
        (_, KeyCode::Char('J')) => Some(Message::ScrollPreviewDown),
        (KeyModifiers::SHIFT, KeyCode::Down) => Some(Message::ScrollPreviewDown),
        (_, KeyCode::Char('K')) => Some(Message::ScrollPreviewUp),
        (KeyModifiers::SHIFT, KeyCode::Up) => Some(Message::ScrollPreviewUp),

        // Search
        (KeyModifiers::NONE, KeyCode::Char('/')) => Some(Message::ToggleSearch),

        // Actions
        (KeyModifiers::NONE, KeyCode::Char('d')) => Some(Message::DeleteSelected),
        (KeyModifiers::NONE, KeyCode::Char('a')) => Some(Message::ArchiveSelected),
        (KeyModifiers::NONE, KeyCode::Char('s')) => Some(Message::ToggleStar),
        (KeyModifiers::NONE, KeyCode::Char('u')) => Some(Message::ToggleRead),
        (KeyModifiers::NONE, KeyCode::Char('r')) => Some(Message::Refresh),

        // Compose
        (KeyModifiers::NONE, KeyCode::Char('c')) => Some(Message::OpenCompose),
        (_, KeyCode::Char('R')) => Some(Message::OpenReply),
        (KeyModifiers::NONE, KeyCode::Char('A')) | (KeyModifiers::SHIFT, KeyCode::Char('A')) => {
            Some(Message::OpenReplyAll)
        }
        (KeyModifiers::NONE, KeyCode::Char('f')) => Some(Message::OpenForward),

        // Setup
        (_, KeyCode::Char('S')) => Some(Message::OpenSetup),
        (_, KeyCode::Char('X')) => Some(Message::ResetAccount),

        // Log Viewer
        (_, KeyCode::Char('L')) => Some(Message::OpenLogViewer),

        _ => None,
    }
}

fn convert_search_key(key: event::KeyEvent) -> Option<Message> {
    match (key.modifiers, key.code) {
        (KeyModifiers::NONE, KeyCode::Esc) => Some(Message::SearchClear),
        (KeyModifiers::NONE, KeyCode::Enter) => Some(Message::SearchSubmit),
        (KeyModifiers::NONE, KeyCode::Backspace) => Some(Message::SearchBackspace),
        (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(c)) => {
            Some(Message::SearchInput(c))
        }
        _ => None,
    }
}

fn convert_compose_key(key: event::KeyEvent) -> Option<Message> {
    match (key.modifiers, key.code) {
        (KeyModifiers::NONE, KeyCode::Esc) => Some(Message::ComposeCancel),
        (KeyModifiers::CONTROL, KeyCode::Char('s')) => Some(Message::ComposeSend),
        (KeyModifiers::NONE, KeyCode::Tab) => Some(Message::ComposeTabField),
        (KeyModifiers::NONE, KeyCode::Enter) => Some(Message::ComposeNewline),
        (KeyModifiers::NONE, KeyCode::Backspace) => Some(Message::ComposeBackspace),
        (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(c)) => {
            Some(Message::ComposeInput(c))
        }
        _ => None,
    }
}

fn convert_setup_key(key: event::KeyEvent) -> Option<Message> {
    match (key.modifiers, key.code) {
        (KeyModifiers::NONE, KeyCode::Esc) => Some(Message::SetupCancel),
        (KeyModifiers::CONTROL, KeyCode::Char('s')) => Some(Message::SetupSubmit),
        (KeyModifiers::NONE, KeyCode::Enter) => Some(Message::SetupEnter),
        (KeyModifiers::NONE, KeyCode::Tab) => Some(Message::SetupTabField),
        (KeyModifiers::NONE, KeyCode::Backspace) => Some(Message::SetupBackspace),
        (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(c)) => {
            Some(Message::SetupInput(c))
        }
        _ => None,
    }
}

fn convert_log_viewer_key(key: event::KeyEvent) -> Option<Message> {
    match (key.modifiers, key.code) {
        (KeyModifiers::NONE, KeyCode::Esc) | (KeyModifiers::NONE, KeyCode::Char('q')) => {
            Some(Message::CloseLogViewer)
        }
        (KeyModifiers::NONE, KeyCode::Char('j') | KeyCode::Down) => {
            Some(Message::LogViewerScrollDown)
        }
        (KeyModifiers::NONE, KeyCode::Char('k') | KeyCode::Up) => {
            Some(Message::LogViewerScrollUp)
        }
        (KeyModifiers::NONE, KeyCode::Tab) => Some(Message::LogViewerCycleLevel),
        _ => None,
    }
}

fn convert_mouse(mouse: event::MouseEvent, _mode: InputMode) -> Option<Message> {
    match mouse.kind {
        MouseEventKind::ScrollDown => Some(Message::SelectNext),
        MouseEventKind::ScrollUp => Some(Message::SelectPrevious),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn make_key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn make_key_kind(code: KeyCode, modifiers: KeyModifiers, kind: KeyEventKind) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn test_non_press_events_ignored() {
        let repeat = make_key_kind(KeyCode::Char('q'), KeyModifiers::NONE, KeyEventKind::Repeat);
        let release = make_key_kind(KeyCode::Char('q'), KeyModifiers::NONE, KeyEventKind::Release);
        assert!(convert_key(repeat, InputMode::Normal).is_none());
        assert!(convert_key(release, InputMode::Normal).is_none());
    }

    #[test]
    fn test_normal_quit() {
        assert!(matches!(
            convert_key(make_key(KeyCode::Char('q'), KeyModifiers::NONE), InputMode::Normal),
            Some(Message::Quit)
        ));
        assert!(matches!(
            convert_key(make_key(KeyCode::Char('c'), KeyModifiers::CONTROL), InputMode::Normal),
            Some(Message::Quit)
        ));
    }

    #[test]
    fn test_normal_navigation() {
        assert!(matches!(
            convert_key(make_key(KeyCode::Char('j'), KeyModifiers::NONE), InputMode::Normal),
            Some(Message::SelectNext)
        ));
        assert!(matches!(
            convert_key(make_key(KeyCode::Down, KeyModifiers::NONE), InputMode::Normal),
            Some(Message::SelectNext)
        ));
        assert!(matches!(
            convert_key(make_key(KeyCode::Char('k'), KeyModifiers::NONE), InputMode::Normal),
            Some(Message::SelectPrevious)
        ));
        assert!(matches!(
            convert_key(make_key(KeyCode::Up, KeyModifiers::NONE), InputMode::Normal),
            Some(Message::SelectPrevious)
        ));
        assert!(matches!(
            convert_key(make_key(KeyCode::Enter, KeyModifiers::NONE), InputMode::Normal),
            Some(Message::OpenSelected)
        ));
        assert!(matches!(
            convert_key(make_key(KeyCode::Tab, KeyModifiers::NONE), InputMode::Normal),
            Some(Message::SwitchPane)
        ));
    }

    #[test]
    fn test_normal_preview_scroll() {
        assert!(matches!(
            convert_key(make_key(KeyCode::Char('J'), KeyModifiers::SHIFT), InputMode::Normal),
            Some(Message::ScrollPreviewDown)
        ));
        assert!(matches!(
            convert_key(make_key(KeyCode::Down, KeyModifiers::SHIFT), InputMode::Normal),
            Some(Message::ScrollPreviewDown)
        ));
        assert!(matches!(
            convert_key(make_key(KeyCode::Char('K'), KeyModifiers::SHIFT), InputMode::Normal),
            Some(Message::ScrollPreviewUp)
        ));
        assert!(matches!(
            convert_key(make_key(KeyCode::Up, KeyModifiers::SHIFT), InputMode::Normal),
            Some(Message::ScrollPreviewUp)
        ));
    }

    #[test]
    fn test_normal_actions() {
        assert!(matches!(
            convert_key(make_key(KeyCode::Char('d'), KeyModifiers::NONE), InputMode::Normal),
            Some(Message::DeleteSelected)
        ));
        assert!(matches!(
            convert_key(make_key(KeyCode::Char('a'), KeyModifiers::NONE), InputMode::Normal),
            Some(Message::ArchiveSelected)
        ));
        assert!(matches!(
            convert_key(make_key(KeyCode::Char('s'), KeyModifiers::NONE), InputMode::Normal),
            Some(Message::ToggleStar)
        ));
        assert!(matches!(
            convert_key(make_key(KeyCode::Char('u'), KeyModifiers::NONE), InputMode::Normal),
            Some(Message::ToggleRead)
        ));
        assert!(matches!(
            convert_key(make_key(KeyCode::Char('r'), KeyModifiers::NONE), InputMode::Normal),
            Some(Message::Refresh)
        ));
        assert!(matches!(
            convert_key(make_key(KeyCode::Char('c'), KeyModifiers::NONE), InputMode::Normal),
            Some(Message::OpenCompose)
        ));
        assert!(matches!(
            convert_key(make_key(KeyCode::Char('R'), KeyModifiers::SHIFT), InputMode::Normal),
            Some(Message::OpenReply)
        ));
        assert!(matches!(
            convert_key(make_key(KeyCode::Char('S'), KeyModifiers::SHIFT), InputMode::Normal),
            Some(Message::OpenSetup)
        ));
        assert!(matches!(
            convert_key(make_key(KeyCode::Char('L'), KeyModifiers::SHIFT), InputMode::Normal),
            Some(Message::OpenLogViewer)
        ));
    }

    #[test]
    fn test_search_keys() {
        assert!(matches!(
            convert_key(make_key(KeyCode::Esc, KeyModifiers::NONE), InputMode::Search),
            Some(Message::SearchClear)
        ));
        assert!(matches!(
            convert_key(make_key(KeyCode::Enter, KeyModifiers::NONE), InputMode::Search),
            Some(Message::SearchSubmit)
        ));
        assert!(matches!(
            convert_key(make_key(KeyCode::Backspace, KeyModifiers::NONE), InputMode::Search),
            Some(Message::SearchBackspace)
        ));
        match convert_key(make_key(KeyCode::Char('x'), KeyModifiers::NONE), InputMode::Search) {
            Some(Message::SearchInput('x')) => {}
            other => panic!("expected SearchInput('x'), got {:?}", other),
        }
    }

    #[test]
    fn test_compose_keys() {
        assert!(matches!(
            convert_key(make_key(KeyCode::Esc, KeyModifiers::NONE), InputMode::Compose),
            Some(Message::ComposeCancel)
        ));
        assert!(matches!(
            convert_key(make_key(KeyCode::Char('s'), KeyModifiers::CONTROL), InputMode::Compose),
            Some(Message::ComposeSend)
        ));
        assert!(matches!(
            convert_key(make_key(KeyCode::Tab, KeyModifiers::NONE), InputMode::Compose),
            Some(Message::ComposeTabField)
        ));
        match convert_key(make_key(KeyCode::Char('a'), KeyModifiers::NONE), InputMode::Compose) {
            Some(Message::ComposeInput('a')) => {}
            other => panic!("expected ComposeInput('a'), got {:?}", other),
        }
    }

    #[test]
    fn test_setup_keys() {
        assert!(matches!(
            convert_key(make_key(KeyCode::Esc, KeyModifiers::NONE), InputMode::Setup),
            Some(Message::SetupCancel)
        ));
        assert!(matches!(
            convert_key(make_key(KeyCode::Char('s'), KeyModifiers::CONTROL), InputMode::Setup),
            Some(Message::SetupSubmit)
        ));
        assert!(matches!(
            convert_key(make_key(KeyCode::Tab, KeyModifiers::NONE), InputMode::Setup),
            Some(Message::SetupTabField)
        ));
        match convert_key(make_key(KeyCode::Char('z'), KeyModifiers::NONE), InputMode::Setup) {
            Some(Message::SetupInput('z')) => {}
            other => panic!("expected SetupInput('z'), got {:?}", other),
        }
    }

    #[test]
    fn test_log_viewer_keys() {
        assert!(matches!(
            convert_key(make_key(KeyCode::Esc, KeyModifiers::NONE), InputMode::LogViewer),
            Some(Message::CloseLogViewer)
        ));
        assert!(matches!(
            convert_key(make_key(KeyCode::Char('q'), KeyModifiers::NONE), InputMode::LogViewer),
            Some(Message::CloseLogViewer)
        ));
        assert!(matches!(
            convert_key(make_key(KeyCode::Char('j'), KeyModifiers::NONE), InputMode::LogViewer),
            Some(Message::LogViewerScrollDown)
        ));
        assert!(matches!(
            convert_key(make_key(KeyCode::Char('k'), KeyModifiers::NONE), InputMode::LogViewer),
            Some(Message::LogViewerScrollUp)
        ));
    }

    #[test]
    fn test_mode_routing() {
        // 'q' in Normal → Quit, but 'q' in Search → SearchInput('q'), in LogViewer → CloseLogViewer
        assert!(matches!(
            convert_key(make_key(KeyCode::Char('q'), KeyModifiers::NONE), InputMode::Normal),
            Some(Message::Quit)
        ));
        match convert_key(make_key(KeyCode::Char('q'), KeyModifiers::NONE), InputMode::Search) {
            Some(Message::SearchInput('q')) => {}
            other => panic!("expected SearchInput('q'), got {:?}", other),
        }
        assert!(matches!(
            convert_key(make_key(KeyCode::Char('q'), KeyModifiers::NONE), InputMode::LogViewer),
            Some(Message::CloseLogViewer)
        ));
    }

    #[test]
    fn test_unrecognized_key_returns_none() {
        // F12 is not bound in any mode
        assert!(convert_key(make_key(KeyCode::F(12), KeyModifiers::NONE), InputMode::Normal).is_none());
        assert!(convert_key(make_key(KeyCode::F(12), KeyModifiers::NONE), InputMode::Search).is_none());
        assert!(convert_key(make_key(KeyCode::F(12), KeyModifiers::NONE), InputMode::Compose).is_none());
        assert!(convert_key(make_key(KeyCode::F(12), KeyModifiers::NONE), InputMode::Setup).is_none());
        assert!(convert_key(make_key(KeyCode::F(12), KeyModifiers::NONE), InputMode::LogViewer).is_none());
    }
}
