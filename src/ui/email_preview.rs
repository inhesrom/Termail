use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::{App, Pane};
use crate::ui::html_renderer::ContentBlock;

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

        // Body — use content blocks for mixed text + image rendering
        let body_area = chunks[2];
        let body_width = body_area.width as usize;

        let blocks = if let Some(html) = &email.body_html {
            super::html_renderer::render_html_blocks(html, body_width)
        } else {
            let lines: Vec<Line<'static>> = email
                .body_text
                .lines()
                .map(|l| {
                    Line::from(Span::styled(
                        l.to_string(),
                        Style::default().fg(Color::White),
                    ))
                })
                .collect();
            vec![ContentBlock::Text(lines)]
        };

        render_mixed_content(f, body_area, app, &blocks, &email.inline_images);
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

/// Render a sequence of content blocks (text + images) into the given area,
/// applying vertical scroll from `app.preview_scroll`.
fn render_mixed_content(
    f: &mut Frame,
    area: Rect,
    app: &App,
    blocks: &[ContentBlock],
    inline_images: &std::collections::HashMap<String, Vec<u8>>,
) {
    let scroll = app.preview_scroll;
    let viewport_height = area.height;

    // First pass: compute the height of each block.
    let block_heights: Vec<u16> = blocks
        .iter()
        .map(|b| match b {
            ContentBlock::Text(lines) => lines.len() as u16,
            ContentBlock::Image { height, .. } => *height,
        })
        .collect();

    // Walk blocks, tracking the current vertical offset in the virtual
    // (scrollable) content coordinate system.
    let mut content_y: u16 = 0;

    for (i, block) in blocks.iter().enumerate() {
        let block_h = block_heights[i];
        let block_end = content_y.saturating_add(block_h);

        // Skip blocks entirely above the viewport
        if block_end <= scroll {
            content_y = block_end;
            continue;
        }

        // Stop once we're below the viewport
        if content_y >= scroll.saturating_add(viewport_height) {
            break;
        }

        // How many rows of this block are hidden above the viewport?
        let top_clip = scroll.saturating_sub(content_y);
        // Screen y where this block starts rendering
        let screen_y = content_y.saturating_sub(scroll).saturating_add(area.y);
        // How many rows are available on screen for this block
        let available = viewport_height
            .saturating_sub(screen_y.saturating_sub(area.y));
        let visible_h = (block_h.saturating_sub(top_clip)).min(available);

        if visible_h == 0 {
            content_y = block_end;
            continue;
        }

        let rect = Rect::new(area.x, screen_y, area.width, visible_h);

        match block {
            ContentBlock::Text(lines) => {
                let widget = Paragraph::new(lines.clone())
                    .wrap(Wrap { trim: false })
                    .scroll((top_clip, 0));
                f.render_widget(widget, rect);
            }
            ContentBlock::Image { src, alt, .. } => {
                render_inline_image(f, rect, app, src, alt, inline_images);
            }
        }

        content_y = block_end;
    }
}

/// Render a yellow placeholder for an image that cannot be displayed.
fn render_image_placeholder(f: &mut Frame, area: Rect, alt: &str) {
    let widget = Paragraph::new(Line::from(Span::styled(
        format!("[Image: {}]", alt),
        Style::default().fg(Color::Yellow),
    )));
    f.render_widget(widget, area);
}

/// Render a single inline image block. Resolves both `cid:` references and
/// external URL keys against the email's `inline_images` map, uses the
/// protocol cache for efficiency.
fn render_inline_image(
    f: &mut Frame,
    area: Rect,
    app: &App,
    src: &str,
    alt: &str,
    inline_images: &std::collections::HashMap<String, Vec<u8>>,
) {
    tracing::debug!(
        "render_inline_image: src={}, area={}x{}, protocol={:?}, font_size={:?}",
        src,
        area.width,
        area.height,
        app.image_picker.protocol_type(),
        app.image_picker.font_size()
    );
    // Determine the lookup key: CID images use normalized CID, URL images use the URL itself
    let cache_key = if let Some(stripped) = src.strip_prefix("cid:") {
        let cid = stripped.trim().trim_start_matches('<').trim_end_matches('>');
        cid.to_ascii_lowercase()
    } else {
        src.to_string()
    };

    // Look up image bytes (case-insensitive for CID, exact for URLs)
    let bytes = match inline_images
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(&cache_key))
        .map(|(_, v)| v)
    {
        Some(b) => b,
        None => {
            if src.starts_with("cid:") {
                let available: Vec<&String> = inline_images.keys().collect();
                tracing::warn!(
                    "CID not found in inline_images: '{}' (available keys: {:?})",
                    cache_key,
                    available
                );
            }
            render_image_placeholder(f, area, alt);
            return;
        }
    };

    let picker = &app.image_picker;

    // Get or create cached protocol
    let mut cache = app.image_protocol_cache.borrow_mut();
    if !cache.contains_key(&cache_key) {
        match image::load_from_memory(bytes) {
            Ok(img) => {
                tracing::debug!("Decoded image {} ({}x{})", cache_key, img.width(), img.height());
                let protocol = picker.new_resize_protocol(img);
                cache.insert(cache_key.clone(), protocol);
            }
            Err(e) => {
                tracing::warn!("Failed to decode image {}: {}", cache_key, e);
                let err_msg = Paragraph::new(Line::from(Span::styled(
                    format!("[Image decode error: {}]", alt),
                    Style::default().fg(Color::Red),
                )));
                f.render_widget(err_msg, area);
                return;
            }
        }
    }

    if let Some(protocol) = cache.get_mut(&cache_key) {
        tracing::debug!("Rendering cached protocol for {}", cache_key);
        let widget = ratatui_image::StatefulImage::default();
        f.render_stateful_widget(widget, area, protocol);
    } else {
        tracing::warn!(
            "Protocol cache miss after insertion for {} — this should not happen",
            cache_key
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// End-to-end test: generate a tiny PNG, decode it, create a protocol,
    /// and render into a `TestBackend`. Verifies the pipeline doesn't panic
    /// and produces non-empty output (Halfblocks writes colored half-block
    /// Unicode characters).
    #[test]
    fn test_image_pipeline_roundtrip() {
        // 1. Generate a 2×2 pixel RGBA test image
        let mut img = image::RgbaImage::new(2, 2);
        img.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        img.put_pixel(1, 0, image::Rgba([0, 255, 0, 255]));
        img.put_pixel(0, 1, image::Rgba([0, 0, 255, 255]));
        img.put_pixel(1, 1, image::Rgba([255, 255, 0, 255]));

        // Encode to PNG bytes in memory
        let mut png_bytes: Vec<u8> = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
        img.write_with_encoder(encoder).expect("PNG encode failed");

        // 2. Create a headless picker (defaults to Halfblocks)
        let picker = ratatui_image::picker::Picker::halfblocks();

        // 3. Decode the bytes and create a protocol
        let decoded = image::load_from_memory(&png_bytes).expect("image decode failed");
        let mut protocol = picker.new_resize_protocol(decoded);

        // 4. Render into a TestBackend terminal
        let backend = ratatui::backend::TestBackend::new(20, 5);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                let area = Rect::new(0, 0, 20, 5);
                let widget = ratatui_image::StatefulImage::default();
                f.render_stateful_widget(widget, area, &mut protocol);
            })
            .unwrap();

        // 5. Verify that the buffer contains non-default cells
        //    Halfblocks writes colored half-block Unicode characters (▀, ▄, █, etc.)
        let buf = terminal.backend().buffer();
        let has_content = (0..buf.area().height).any(|y| {
            (0..buf.area().width).any(|x| {
                let cell = &buf[(x, y)];
                cell.symbol() != " "
                    || cell.fg != Color::Reset
                    || cell.bg != Color::Reset
            })
        });
        assert!(
            has_content,
            "Expected non-empty buffer after rendering image with Halfblocks protocol"
        );
    }
}
