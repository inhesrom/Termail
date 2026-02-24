pub mod animations;
pub mod compose;
pub mod email_preview;
pub mod html_renderer;
pub mod inbox_list;
pub mod log_viewer;
pub mod search_bar;
pub mod setup;
pub mod status_bar;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::app::App;

/// Root view function: renders the entire UI from app state.
pub fn view(f: &mut Frame, app: &App) {
    let size = f.area();

    // Top-level vertical split: header, main content, status bar
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header + search bar
            Constraint::Min(5),    // Main content
            Constraint::Length(1), // Status bar
        ])
        .split(size);

    // Header / search bar
    search_bar::render(f, outer[0], app);

    // Main content: horizontal split (30% inbox, 70% preview)
    let main_area = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Percentage(70),
        ])
        .split(outer[1]);

    // Left pane: inbox list
    inbox_list::render(f, main_area[0], app);

    // Right pane: email preview
    email_preview::render(f, main_area[1], app);

    // Bottom: status bar
    status_bar::render(f, outer[2], app);

    // Compose overlay (renders on top of everything)
    if app.compose.is_some() {
        let overlay_area = centered_rect(60, 70, size);
        compose::render(f, overlay_area, app);
    }

    // Log viewer overlay
    if app.log_viewer.is_some() {
        let overlay_area = centered_rect(80, 80, size);
        log_viewer::render(f, overlay_area, app);
    }

    // Setup overlay (renders on top of everything, takes priority over compose)
    if app.setup.is_some() {
        let overlay_area = centered_rect(60, 50, size);
        setup::render(f, overlay_area, app);
    }
}

/// Helper to create a centered rectangle for overlays.
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_centered_rect_50_50() {
        let area = Rect::new(0, 0, 100, 100);
        let result = centered_rect(50, 50, area);
        // Should be roughly centered: x ≈ 25, y ≈ 25, w ≈ 50, h ≈ 50
        assert!(result.x >= 20 && result.x <= 30, "x={}", result.x);
        assert!(result.y >= 20 && result.y <= 30, "y={}", result.y);
        assert!(result.width >= 45 && result.width <= 55, "w={}", result.width);
        assert!(result.height >= 45 && result.height <= 55, "h={}", result.height);
    }

    #[test]
    fn test_centered_rect_full() {
        let area = Rect::new(0, 0, 80, 40);
        let result = centered_rect(100, 100, area);
        assert_eq!(result.x, 0);
        assert_eq!(result.y, 0);
        assert_eq!(result.width, area.width);
        assert_eq!(result.height, area.height);
    }

    #[test]
    fn test_centered_rect_asymmetric() {
        let area = Rect::new(0, 0, 200, 100);
        let result = centered_rect(60, 40, area);
        // Horizontal: 20% margin each side on 200 → x ≈ 40, w ≈ 120
        assert!(result.x >= 35 && result.x <= 45, "x={}", result.x);
        assert!(result.width >= 115 && result.width <= 125, "w={}", result.width);
        // Vertical: 30% margin each side on 100 → y ≈ 30, h ≈ 40
        assert!(result.y >= 25 && result.y <= 35, "y={}", result.y);
        assert!(result.height >= 35 && result.height <= 45, "h={}", result.height);
    }
}
