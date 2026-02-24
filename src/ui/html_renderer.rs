use html2text::render::RichAnnotation;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// Convert an HTML string to a vector of styled ratatui Lines.
///
/// Uses `html2text::from_read_rich` to parse HTML and extract semantic
/// annotations, then maps each annotation to appropriate ratatui styles.
/// Falls back to plain text conversion on parse failure.
pub fn render_html(html: &str, width: usize) -> Vec<Line<'static>> {
    let width = width.clamp(20, 500);

    // Try rich rendering first
    match html2text::from_read_rich(html.as_bytes(), width) {
        Ok(tagged_lines) => tagged_lines
            .into_iter()
            .map(|line| {
                let spans: Vec<Span<'static>> = line
                    .tagged_strings()
                    .map(|ts| {
                        let style = annotations_to_style(&ts.tag);
                        Span::styled(ts.s.clone(), style)
                    })
                    .collect();
                if spans.is_empty() {
                    Line::from("")
                } else {
                    Line::from(spans)
                }
            })
            .collect(),
        Err(_) => {
            // Fall back to plain text conversion
            match html2text::from_read(html.as_bytes(), width) {
                Ok(plain) => plain
                    .lines()
                    .map(|l| Line::from(Span::styled(l.to_string(), Style::default().fg(Color::White))))
                    .collect(),
                Err(_) => vec![Line::from(Span::styled(
                    "[Failed to render email body]".to_string(),
                    Style::default().fg(Color::Red),
                ))],
            }
        }
    }
}

/// Map a vector of RichAnnotation to a single combined ratatui Style.
fn annotations_to_style(annotations: &[RichAnnotation]) -> Style {
    let mut style = Style::default().fg(Color::White);

    for ann in annotations {
        match ann {
            RichAnnotation::Default => {}
            RichAnnotation::Strong => {
                style = style.add_modifier(Modifier::BOLD);
            }
            RichAnnotation::Emphasis => {
                style = style.add_modifier(Modifier::ITALIC);
            }
            RichAnnotation::Strikeout => {
                style = style.add_modifier(Modifier::CROSSED_OUT);
            }
            RichAnnotation::Code => {
                style = style.fg(Color::Green);
            }
            RichAnnotation::Preformat(_) => {
                style = style.fg(Color::DarkGray);
            }
            RichAnnotation::Link(_) => {
                style = style.fg(Color::Cyan).add_modifier(Modifier::UNDERLINED);
            }
            RichAnnotation::Image(_) => {
                style = style.fg(Color::Yellow);
            }
            RichAnnotation::Colour(c) => {
                style = style.fg(Color::Rgb(c.r, c.g, c.b));
            }
            RichAnnotation::BgColour(c) => {
                style = style.bg(Color::Rgb(c.r, c.g, c.b));
            }
            _ => {}
        }
    }

    style
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_html() {
        let lines = render_html("<p>Hello world</p>", 80);
        assert!(!lines.is_empty());
        let text: String = lines.iter().flat_map(|l| l.spans.iter()).map(|s| s.content.as_ref()).collect();
        assert!(text.contains("Hello world"));
    }

    #[test]
    fn test_empty_input() {
        let lines = render_html("", 80);
        // Should not panic, may return empty or single empty line
        assert!(lines.len() <= 1);
    }

    #[test]
    fn test_bold_detection() {
        let lines = render_html("<p><strong>Bold text</strong></p>", 80);
        let bold_spans: Vec<_> = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .filter(|s| s.style.add_modifier.contains(Modifier::BOLD) && s.content.contains("Bold"))
            .collect();
        assert!(!bold_spans.is_empty(), "Expected bold styling on 'Bold text'");
    }

    #[test]
    fn test_link_detection() {
        let lines = render_html("<a href=\"https://example.com\">Click here</a>", 80);
        let link_spans: Vec<_> = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .filter(|s| {
                s.style.fg == Some(Color::Cyan)
                    && s.style.add_modifier.contains(Modifier::UNDERLINED)
            })
            .collect();
        assert!(!link_spans.is_empty(), "Expected cyan underlined link styling");
    }

    #[test]
    fn test_narrow_width() {
        let lines = render_html("<p>Some text that should wrap at narrow width</p>", 20);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_plain_text_fallback() {
        // Even if not strictly HTML, from_read should handle it
        let lines = render_html("Just plain text, no tags", 80);
        assert!(!lines.is_empty());
        let text: String = lines.iter().flat_map(|l| l.spans.iter()).map(|s| s.content.as_ref()).collect();
        assert!(text.contains("Just plain text"));
    }
}
