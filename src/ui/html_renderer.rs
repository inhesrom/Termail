use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use scraper::{Html, Node};

// ---------------------------------------------------------------------------
// Public API (unchanged)
// ---------------------------------------------------------------------------

/// Convert an HTML string to a vector of styled ratatui Lines.
///
/// Uses a custom DOM-walking renderer built on `scraper` (html5ever) for
/// spec-compliant HTML5 parsing. Falls back to `html2text` if the custom
/// renderer produces empty output.
pub fn render_html(html: &str, width: usize) -> Vec<Line<'static>> {
    let width = width.clamp(20, 500);

    let result = render_with_scraper(html, width);
    if !result.is_empty() {
        return result;
    }
    fallback_html2text(html, width)
}

/// Convert an HTML string to a sequence of [`ContentBlock`]s for mixed
/// text + image rendering.
///
/// The returned blocks preserve the order images appear in the source HTML.
/// Each image becomes its own `ContentBlock::Image` with the original `src`
/// and `alt` attributes; the caller is responsible for resolving CID
/// references to actual image bytes.
pub fn render_html_blocks(html: &str, width: usize) -> Vec<ContentBlock> {
    let width = width.clamp(20, 500);

    let document = Html::parse_fragment(html);
    let mut segments: Vec<Segment> = Vec::new();

    let ctx = RenderContext {
        style: Style::default().fg(Color::White),
        width,
        indent: 0,
        in_pre: false,
        list_stack: Vec::new(),
        blockquote_depth: 0,
    };

    walk_children(document.root_element().id(), &document, &ctx, &mut segments);

    segments_to_blocks(&segments, width)
}

// ---------------------------------------------------------------------------
// Phase 1 + 2 + 3: scraper-based renderer
// ---------------------------------------------------------------------------

/// Parse HTML with scraper and walk the DOM to produce styled ratatui Lines.
fn render_with_scraper(html: &str, width: usize) -> Vec<Line<'static>> {
    let document = Html::parse_fragment(html);
    let mut segments: Vec<Segment> = Vec::new();

    let ctx = RenderContext {
        style: Style::default().fg(Color::White),
        width,
        indent: 0,
        in_pre: false,
        list_stack: Vec::new(),
        blockquote_depth: 0,
    };

    walk_children(document.root_element().id(), &document, &ctx, &mut segments);

    segments_to_lines(&segments, width)
}

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct RenderContext {
    style: Style,
    width: usize,
    indent: usize,
    in_pre: bool,
    list_stack: Vec<ListType>,
    blockquote_depth: usize,
}

#[derive(Clone)]
enum ListType {
    Unordered,
    Ordered(usize),
}

#[derive(Clone)]
enum Segment {
    Text { text: String, style: Style },
    LineBreak,
    BlankLine,
    HorizontalRule,
    Image { src: String, alt: String },
}

/// A content block produced by [`render_html_blocks`].
///
/// The rendered email is a sequence of text runs and image placeholders so that
/// the caller can interleave ratatui `Paragraph` widgets with
/// `ratatui_image::StatefulImage` widgets at the correct vertical positions.
#[derive(Debug, Clone)]
pub enum ContentBlock {
    /// One or more styled text lines.
    Text(Vec<Line<'static>>),
    /// An inline image reference (CID or URL) with a suggested display height.
    Image { src: String, alt: String, height: u16 },
}

// ---------------------------------------------------------------------------
// Phase 2: Recursive DOM walk
// ---------------------------------------------------------------------------

/// Recursively walk the children of `node_id`, emitting segments for each.
fn walk_children(
    node_id: ego_tree::NodeId,
    doc: &Html,
    ctx: &RenderContext,
    segments: &mut Vec<Segment>,
) {
    let node = doc.tree.get(node_id).unwrap();
    for child in node.children() {
        match child.value() {
            Node::Text(text) => {
                emit_text(&text.text, ctx, segments);
            }
            Node::Element(elem) => {
                render_element(child.id(), elem, doc, ctx, segments);
            }
            _ => {}
        }
    }
}

/// Emit a text node as a Segment, collapsing whitespace unless inside `<pre>`.
fn emit_text(text: &str, ctx: &RenderContext, segments: &mut Vec<Segment>) {
    if ctx.in_pre {
        // Preserve whitespace in <pre> blocks
        segments.push(Segment::Text {
            text: text.to_string(),
            style: ctx.style,
        });
    } else {
        // Collapse whitespace for normal content
        let collapsed = collapse_whitespace(text);
        if !collapsed.is_empty() {
            segments.push(Segment::Text {
                text: collapsed,
                style: ctx.style,
            });
        }
    }
}

/// Collapse consecutive whitespace characters to a single space.
fn collapse_whitespace(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut prev_was_space = false;
    for c in text.chars() {
        if c.is_whitespace() {
            if !prev_was_space {
                result.push(' ');
                prev_was_space = true;
            }
        } else {
            result.push(c);
            prev_was_space = false;
        }
    }
    result
}

/// Dispatch an HTML element by tag name, applying styles and recursing into children.
fn render_element(
    node_id: ego_tree::NodeId,
    elem: &scraper::node::Element,
    doc: &Html,
    ctx: &RenderContext,
    segments: &mut Vec<Segment>,
) {
    let tag = elem.name().to_ascii_lowercase();

    // Apply CSS color from style attribute to all elements
    let mut ctx = ctx.clone();
    if let Some(style_attr) = elem.attr("style")
        && let Some(color) = parse_css_color(style_attr)
    {
        ctx.style = ctx.style.fg(color);
    }

    match tag.as_str() {
        // --- Skipped elements (no traversal) ---
        "script" | "style" | "noscript" | "template" => {}

        // --- Block elements ---
        "p" => {
            push_block_spacing(segments);
            walk_children(node_id, doc, &ctx, segments);
            push_block_spacing(segments);
        }

        // <div> is a structural wrapper, not a visual paragraph — no extra spacing
        "div" => {
            walk_children(node_id, doc, &ctx, segments);
        }

        "h1" => render_heading(
            node_id,
            doc,
            &ctx,
            segments,
            ctx.style
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::UNDERLINED),
        ),

        "h2" => render_heading(
            node_id,
            doc,
            &ctx,
            segments,
            ctx.style.fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),

        "h3" | "h4" | "h5" | "h6" => render_heading(
            node_id,
            doc,
            &ctx,
            segments,
            ctx.style.fg(Color::White).add_modifier(Modifier::BOLD),
        ),

        "blockquote" => {
            push_block_spacing(segments);
            let mut child_ctx = ctx.clone();
            child_ctx.blockquote_depth += 1;
            child_ctx.style = child_ctx.style.fg(Color::Gray);
            // Collect child segments, then prefix each line with "> "
            let mut child_segments: Vec<Segment> = Vec::new();
            walk_children(node_id, doc, &child_ctx, &mut child_segments);
            // Prefix all text segments with blockquote markers
            let prefix = "> ".repeat(child_ctx.blockquote_depth);
            let child_lines = segments_to_lines(&child_segments, ctx.width.saturating_sub(prefix.len()));
            for line in child_lines {
                segments.push(Segment::Text {
                    text: prefix.clone(),
                    style: Style::default().fg(Color::Gray),
                });
                for span in line.spans {
                    segments.push(Segment::Text {
                        text: span.content.to_string(),
                        style: span.style,
                    });
                }
                segments.push(Segment::LineBreak);
            }
            push_block_spacing(segments);
        }

        "pre" => {
            push_block_spacing(segments);
            let mut child_ctx = ctx.clone();
            child_ctx.in_pre = true;
            child_ctx.style = child_ctx.style.fg(Color::DarkGray);
            walk_children(node_id, doc, &child_ctx, segments);
            push_block_spacing(segments);
        }

        "ul" => {
            push_block_spacing(segments);
            let mut child_ctx = ctx.clone();
            child_ctx.list_stack.push(ListType::Unordered);
            child_ctx.indent += 2;
            render_list_items(node_id, doc, &child_ctx, segments);
            push_block_spacing(segments);
        }

        "ol" => {
            push_block_spacing(segments);
            let mut child_ctx = ctx.clone();
            child_ctx.list_stack.push(ListType::Ordered(1));
            child_ctx.indent += 2;
            render_list_items(node_id, doc, &child_ctx, segments);
            push_block_spacing(segments);
        }

        "li" => {
            // Handled by render_list_items; if encountered bare, just walk children
            walk_children(node_id, doc, &ctx, segments);
        }

        "hr" => {
            push_block_spacing(segments);
            segments.push(Segment::HorizontalRule);
            push_block_spacing(segments);
        }

        "br" => {
            segments.push(Segment::LineBreak);
        }

        "table" => {
            push_block_spacing(segments);
            render_table(node_id, doc, &ctx, segments);
            push_block_spacing(segments);
        }

        // --- Inline elements ---
        "strong" | "b" => walk_with_modifier(node_id, doc, &ctx, segments, Modifier::BOLD),
        "em" | "i" => walk_with_modifier(node_id, doc, &ctx, segments, Modifier::ITALIC),
        "s" | "del" | "strike" => {
            walk_with_modifier(node_id, doc, &ctx, segments, Modifier::CROSSED_OUT)
        }
        "u" => walk_with_modifier(node_id, doc, &ctx, segments, Modifier::UNDERLINED),

        "a" => {
            let link_style = ctx
                .style
                .fg(Color::Cyan)
                .add_modifier(Modifier::UNDERLINED);
            let mut child_ctx = ctx.clone();
            child_ctx.style = link_style;
            walk_children(node_id, doc, &child_ctx, segments);
            if let Some(href) = elem.attr("href") {
                segments.push(Segment::Text {
                    text: format!(" [{}]", href),
                    style: link_style,
                });
            }
        }

        "code" => {
            let mut child_ctx = ctx.clone();
            child_ctx.style = child_ctx.style.fg(Color::Green);
            walk_children(node_id, doc, &child_ctx, segments);
        }

        "img" => {
            let alt = elem.attr("alt").unwrap_or("image").to_string();
            let src = elem.attr("src").unwrap_or("").to_string();
            segments.push(Segment::Image { src, alt });
        }

        "span" => {
            walk_children(node_id, doc, &ctx, segments);
        }

        // --- Passthrough elements (traverse children, no extra styling) ---
        "html" | "body" | "head" | "main" | "article" | "section" | "nav" | "header"
        | "footer" | "tbody" | "thead" | "tfoot" | "figure" | "figcaption" | "aside"
        | "details" | "summary" | "mark" | "time" | "small" | "sub" | "sup" | "abbr"
        | "cite" | "dfn" | "kbd" | "samp" | "var" | "wbr" | "label" | "form" | "input"
        | "button" | "select" | "textarea" | "fieldset" | "legend" | "dl" | "dt" | "dd"
        | "colgroup" | "col" | "caption" => {
            walk_children(node_id, doc, &ctx, segments);
        }

        // --- Default: traverse children ---
        _ => {
            walk_children(node_id, doc, &ctx, segments);
        }
    }
}

/// Render a heading element with the given style, wrapped in block spacing.
fn render_heading(
    node_id: ego_tree::NodeId,
    doc: &Html,
    ctx: &RenderContext,
    segments: &mut Vec<Segment>,
    heading_style: Style,
) {
    push_block_spacing(segments);
    let mut child_ctx = ctx.clone();
    child_ctx.style = heading_style;
    walk_children(node_id, doc, &child_ctx, segments);
    push_block_spacing(segments);
}

/// Walk children after adding a single style modifier to the current context.
fn walk_with_modifier(
    node_id: ego_tree::NodeId,
    doc: &Html,
    ctx: &RenderContext,
    segments: &mut Vec<Segment>,
    modifier: Modifier,
) {
    let mut child_ctx = ctx.clone();
    child_ctx.style = child_ctx.style.add_modifier(modifier);
    walk_children(node_id, doc, &child_ctx, segments);
}

/// Append a blank-line segment, suppressing duplicates.
///
/// If the last segment is a `LineBreak`, replace it with `BlankLine`
/// instead of adding another one (prevents `<br>` + block boundary
/// double spacing).
fn push_block_spacing(segments: &mut Vec<Segment>) {
    match segments.last() {
        Some(Segment::BlankLine) => return,
        Some(Segment::LineBreak) => {
            // Replace the LineBreak with a BlankLine to avoid double spacing
            let len = segments.len();
            segments[len - 1] = Segment::BlankLine;
            return;
        }
        _ => {}
    }
    segments.push(Segment::BlankLine);
}

/// Render `<li>` children of a list element with bullet or number prefixes.
fn render_list_items(
    node_id: ego_tree::NodeId,
    doc: &Html,
    ctx: &RenderContext,
    segments: &mut Vec<Segment>,
) {
    let node = doc.tree.get(node_id).unwrap();
    let mut counter: usize = 1;

    for child in node.children() {
        if let Node::Element(elem) = child.value()
            && elem.name().eq_ignore_ascii_case("li")
        {
            // Determine the bullet/number prefix
            let prefix = match ctx.list_stack.last() {
                Some(ListType::Unordered) => "- ".to_string(),
                Some(ListType::Ordered(_)) => {
                    let p = format!("{}. ", counter);
                    counter += 1;
                    p
                }
                None => "- ".to_string(),
            };

            segments.push(Segment::Text {
                text: prefix,
                style: ctx.style,
            });
            walk_children(child.id(), doc, ctx, segments);
            segments.push(Segment::LineBreak);
        }
    }
}

/// Render a `<table>` element with cells separated by ` | `.
fn render_table(
    node_id: ego_tree::NodeId,
    doc: &Html,
    ctx: &RenderContext,
    segments: &mut Vec<Segment>,
) {
    // Collect all rows, handling <thead>, <tbody>, <tfoot> wrappers
    let rows = collect_table_rows(node_id, doc);

    for row_id in rows {
        let row_node = doc.tree.get(row_id).unwrap();
        let mut first_cell = true;

        for cell_child in row_node.children() {
            if let Node::Element(cell_elem) = cell_child.value() {
                let cell_tag = cell_elem.name().to_ascii_lowercase();
                if cell_tag == "td" || cell_tag == "th" {
                    if !first_cell {
                        segments.push(Segment::Text {
                            text: " | ".to_string(),
                            style: ctx.style,
                        });
                    }
                    first_cell = false;

                    let mut cell_ctx = ctx.clone();
                    if cell_tag == "th" {
                        cell_ctx.style = cell_ctx.style.add_modifier(Modifier::BOLD);
                    }
                    walk_children(cell_child.id(), doc, &cell_ctx, segments);
                }
            }
        }
        segments.push(Segment::LineBreak);
    }
}

/// Collect all `<tr>` NodeIds from a table, unwrapping `<thead>`/`<tbody>`/`<tfoot>`.
fn collect_table_rows(
    table_id: ego_tree::NodeId,
    doc: &Html,
) -> Vec<ego_tree::NodeId> {
    let mut rows = Vec::new();
    let table_node = doc.tree.get(table_id).unwrap();

    for child in table_node.children() {
        if let Node::Element(elem) = child.value() {
            let tag = elem.name().to_ascii_lowercase();
            match tag.as_str() {
                "tr" => rows.push(child.id()),
                "thead" | "tbody" | "tfoot" => {
                    // Look for <tr> inside section wrappers
                    for section_child in child.children() {
                        if let Node::Element(sc_elem) = section_child.value()
                            && sc_elem.name().eq_ignore_ascii_case("tr")
                        {
                            rows.push(section_child.id());
                        }
                    }
                }
                _ => {}
            }
        }
    }

    rows
}

// ---------------------------------------------------------------------------
// CSS color parsing
// ---------------------------------------------------------------------------

/// Extract the foreground `color` CSS property from a `style` attribute string.
fn parse_css_color(style_attr: &str) -> Option<Color> {
    // Look for color: ... in the style attribute
    let lower = style_attr.to_ascii_lowercase();
    let color_start = lower.find("color:")?;
    // Make sure it's not "background-color:"
    if color_start > 0 && lower[..color_start].ends_with("background-") {
        // Try to find a bare "color:" after this background-color
        let rest = &lower[color_start + 6..];
        if let Some(next_color) = rest.find("color:") {
            let value_start = color_start + 6 + next_color + 6;
            let value = extract_css_value(&lower[value_start..]);
            return parse_color_value(value.trim());
        }
        return None;
    }
    let value_start = color_start + 6; // length of "color:"
    let value = extract_css_value(&lower[value_start..]);
    parse_color_value(value.trim())
}

/// Trim a CSS property value from `s`, stopping at `;` or end of string.
fn extract_css_value(s: &str) -> &str {
    let s = s.trim_start();
    // Value ends at ; or end of string
    match s.find(';') {
        Some(pos) => &s[..pos],
        None => s,
    }
}

/// Parse a CSS color value (`#rgb`, `#rrggbb`, `rgb(r,g,b)`, or named) into a ratatui Color.
fn parse_color_value(value: &str) -> Option<Color> {
    let value = value.trim();

    // #rrggbb
    if value.len() == 7 && value.starts_with('#') {
        let r = u8::from_str_radix(&value[1..3], 16).ok()?;
        let g = u8::from_str_radix(&value[3..5], 16).ok()?;
        let b = u8::from_str_radix(&value[5..7], 16).ok()?;
        return Some(Color::Rgb(r, g, b));
    }

    // #rgb shorthand
    if value.len() == 4 && value.starts_with('#') {
        let r = u8::from_str_radix(&value[1..2], 16).ok()? * 17;
        let g = u8::from_str_radix(&value[2..3], 16).ok()? * 17;
        let b = u8::from_str_radix(&value[3..4], 16).ok()? * 17;
        return Some(Color::Rgb(r, g, b));
    }

    // rgb(r, g, b)
    if value.starts_with("rgb(") && value.ends_with(')') {
        let inner = &value[4..value.len() - 1];
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() == 3 {
            let r = parts[0].trim().parse::<u8>().ok()?;
            let g = parts[1].trim().parse::<u8>().ok()?;
            let b = parts[2].trim().parse::<u8>().ok()?;
            return Some(Color::Rgb(r, g, b));
        }
    }

    // Named colors
    match value {
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "blue" => Some(Color::Blue),
        "yellow" => Some(Color::Yellow),
        "cyan" => Some(Color::Cyan),
        "magenta" => Some(Color::Magenta),
        "white" => Some(Color::White),
        "black" => Some(Color::Black),
        "gray" | "grey" => Some(Color::Gray),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Phase 3: Layout — segments to lines
// ---------------------------------------------------------------------------

/// Returns true if a `Line` has no spans or all spans are whitespace-only.
fn is_visually_blank(line: &Line<'_>) -> bool {
    line.spans.is_empty()
        || line.spans.iter().all(|s| s.content.trim().is_empty())
}

/// Convert a flat segment list into wrapped, styled ratatui Lines.
fn segments_to_lines(segments: &[Segment], width: usize) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut current_spans: Vec<Span<'static>> = Vec::new();
    let mut current_width: usize = 0;

    for segment in segments {
        match segment {
            Segment::Text { text, style } => {
                // Handle newlines within text (e.g., pre-formatted content)
                let parts: Vec<&str> = text.split('\n').collect();
                for (i, part) in parts.iter().enumerate() {
                    if i > 0 {
                        // Newline encountered — flush current line
                        lines.push(Line::from(std::mem::take(&mut current_spans)));
                        current_width = 0;
                    }

                    if part.is_empty() {
                        continue;
                    }

                    let part_len = part.chars().count();

                    // Basic wrapping: if this text would overflow, start a new line
                    if current_width > 0 && current_width + part_len > width {
                        lines.push(Line::from(std::mem::take(&mut current_spans)));
                        current_width = 0;
                    }

                    current_spans.push(Span::styled(part.to_string(), *style));
                    current_width += part_len;
                }
            }
            Segment::LineBreak => {
                lines.push(Line::from(std::mem::take(&mut current_spans)));
                current_width = 0;
            }
            Segment::BlankLine => {
                if !current_spans.is_empty() {
                    lines.push(Line::from(std::mem::take(&mut current_spans)));
                    current_width = 0;
                }
                // Only add blank line if last line wasn't already blank
                if !lines.last().is_some_and(|l| is_visually_blank(l)) {
                    lines.push(Line::from(""));
                }
            }
            Segment::HorizontalRule => {
                if !current_spans.is_empty() {
                    lines.push(Line::from(std::mem::take(&mut current_spans)));
                    current_width = 0;
                }
                let rule = "─".repeat(width);
                lines.push(Line::from(Span::styled(
                    rule,
                    Style::default().fg(Color::DarkGray),
                )));
            }
            Segment::Image { alt, .. } => {
                // Fallback: render as yellow placeholder text in text-only mode
                let placeholder = format!("[Image: {}]", alt);
                let placeholder_len = placeholder.chars().count();
                if current_width > 0 && current_width + placeholder_len > width {
                    lines.push(Line::from(std::mem::take(&mut current_spans)));
                    current_width = 0;
                }
                current_spans.push(Span::styled(
                    placeholder,
                    Style::default().fg(Color::Yellow),
                ));
                current_width += placeholder_len;
            }
        }
    }

    // Flush remaining spans
    if !current_spans.is_empty() {
        lines.push(Line::from(current_spans));
    }

    // Collapse consecutive visually-blank lines to at most 1
    let mut deduped: Vec<Line<'static>> = Vec::with_capacity(lines.len());
    for line in lines {
        if is_visually_blank(&line) {
            if !deduped.last().is_some_and(|l| is_visually_blank(l)) {
                deduped.push(line);
            }
        } else {
            deduped.push(line);
        }
    }
    let mut lines = deduped;

    // Trim leading/trailing blank lines
    while lines.first().is_some_and(|l| is_visually_blank(l)) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|l| is_visually_blank(l)) {
        lines.pop();
    }

    lines
}

/// Convert a flat segment list into a sequence of [`ContentBlock`]s.
///
/// Text segments are accumulated into `ContentBlock::Text` runs.  When a
/// `Segment::Image` is encountered the current text block is flushed and a
/// `ContentBlock::Image` is emitted.
/// Flush accumulated text segments to a `ContentBlock::Text`, then clear the buffer.
fn flush_text_buffer(
    text_buf: &mut Vec<Segment>,
    blocks: &mut Vec<ContentBlock>,
    width: usize,
) {
    if text_buf.is_empty() {
        return;
    }
    let lines = segments_to_lines(text_buf, width);
    if !lines.is_empty() {
        blocks.push(ContentBlock::Text(lines));
    }
    text_buf.clear();
}

fn segments_to_blocks(segments: &[Segment], width: usize) -> Vec<ContentBlock> {
    let mut blocks: Vec<ContentBlock> = Vec::new();
    let mut text_buf: Vec<Segment> = Vec::new();

    for segment in segments {
        match segment {
            Segment::Image { src, alt } => {
                flush_text_buffer(&mut text_buf, &mut blocks, width);
                blocks.push(ContentBlock::Image {
                    src: src.clone(),
                    alt: alt.clone(),
                    height: 12,
                });
            }
            other => {
                text_buf.push(other.clone());
            }
        }
    }

    flush_text_buffer(&mut text_buf, &mut blocks, width);

    blocks
}

// ---------------------------------------------------------------------------
// Fallback: html2text-based renderer (preserved from original)
// ---------------------------------------------------------------------------

/// Fallback renderer using html2text, preserved from the original implementation.
fn fallback_html2text(html: &str, width: usize) -> Vec<Line<'static>> {
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
        Err(_) => match html2text::from_read(html.as_bytes(), width) {
            Ok(plain) => plain
                .lines()
                .map(|l| {
                    Line::from(Span::styled(
                        l.to_string(),
                        Style::default().fg(Color::White),
                    ))
                })
                .collect(),
            Err(_) => vec![Line::from(Span::styled(
                "[Failed to render email body]".to_string(),
                Style::default().fg(Color::Red),
            ))],
        },
    }
}

/// Map html2text RichAnnotation tags to a combined ratatui Style.
fn annotations_to_style(annotations: &[html2text::render::RichAnnotation]) -> Style {
    use html2text::render::RichAnnotation;

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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // === Existing tests (preserved) ===

    #[test]
    fn test_simple_html() {
        let lines = render_html("<p>Hello world</p>", 80);
        assert!(!lines.is_empty());
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
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
            .filter(|s| {
                s.style.add_modifier.contains(Modifier::BOLD) && s.content.contains("Bold")
            })
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
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(text.contains("Just plain text"));
    }

    // === New tests ===

    fn collect_text(lines: &[Line<'_>]) -> String {
        lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect()
    }

    fn find_spans_with_text<'a>(lines: &'a [Line<'_>], needle: &str) -> Vec<&'a Span<'a>> {
        lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .filter(|s| s.content.contains(needle))
            .collect()
    }

    #[test]
    fn test_nested_bold_italic() {
        let lines = render_html("<p><strong><em>bold italic</em></strong></p>", 80);
        let spans = find_spans_with_text(&lines, "bold italic");
        assert!(!spans.is_empty(), "Expected text 'bold italic'");
        let s = spans[0];
        assert!(
            s.style.add_modifier.contains(Modifier::BOLD),
            "Expected BOLD modifier"
        );
        assert!(
            s.style.add_modifier.contains(Modifier::ITALIC),
            "Expected ITALIC modifier"
        );
    }

    #[test]
    fn test_heading_h1() {
        let lines = render_html("<h1>Title</h1>", 80);
        let spans = find_spans_with_text(&lines, "Title");
        assert!(!spans.is_empty(), "Expected heading text");
        let s = spans[0];
        assert_eq!(s.style.fg, Some(Color::Cyan));
        assert!(s.style.add_modifier.contains(Modifier::BOLD));
        assert!(s.style.add_modifier.contains(Modifier::UNDERLINED));
    }

    #[test]
    fn test_heading_h2() {
        let lines = render_html("<h2>Subtitle</h2>", 80);
        let spans = find_spans_with_text(&lines, "Subtitle");
        assert!(!spans.is_empty());
        let s = spans[0];
        assert_eq!(s.style.fg, Some(Color::Cyan));
        assert!(s.style.add_modifier.contains(Modifier::BOLD));
        assert!(
            !s.style.add_modifier.contains(Modifier::UNDERLINED),
            "h2 should not be underlined"
        );
    }

    #[test]
    fn test_heading_h3_to_h6() {
        for tag in &["h3", "h4", "h5", "h6"] {
            let html = format!("<{}>Heading</{}>", tag, tag);
            let lines = render_html(&html, 80);
            let spans = find_spans_with_text(&lines, "Heading");
            assert!(!spans.is_empty(), "{} should render text", tag);
            let s = spans[0];
            assert_eq!(s.style.fg, Some(Color::White), "{} should be white", tag);
            assert!(
                s.style.add_modifier.contains(Modifier::BOLD),
                "{} should be bold",
                tag
            );
        }
    }

    #[test]
    fn test_unordered_list() {
        let lines = render_html("<ul><li>Alpha</li><li>Beta</li></ul>", 80);
        let text = collect_text(&lines);
        assert!(text.contains("- Alpha"), "Expected bullet for Alpha");
        assert!(text.contains("- Beta"), "Expected bullet for Beta");
    }

    #[test]
    fn test_ordered_list() {
        let lines = render_html("<ol><li>First</li><li>Second</li></ol>", 80);
        let text = collect_text(&lines);
        assert!(text.contains("1."), "Expected numbered prefix 1.");
        assert!(text.contains("2."), "Expected numbered prefix 2.");
        assert!(text.contains("First"));
        assert!(text.contains("Second"));
    }

    #[test]
    fn test_blockquote() {
        let lines = render_html("<blockquote>Quoted text</blockquote>", 80);
        let text = collect_text(&lines);
        assert!(text.contains(">"), "Expected blockquote '>' prefix");
        assert!(text.contains("Quoted text"));
    }

    #[test]
    fn test_horizontal_rule() {
        let lines = render_html("<p>Above</p><hr><p>Below</p>", 80);
        let has_rule = lines.iter().any(|l| {
            l.spans
                .iter()
                .any(|s| s.content.contains('─'))
        });
        assert!(has_rule, "Expected horizontal rule with ─ characters");
    }

    #[test]
    fn test_image_alt_text() {
        let lines = render_html("<img alt=\"Photo\">", 80);
        let spans = find_spans_with_text(&lines, "[Image: Photo]");
        assert!(!spans.is_empty(), "Expected [Image: Photo]");
        assert_eq!(spans[0].style.fg, Some(Color::Yellow));
    }

    #[test]
    fn test_pre_preserves_whitespace() {
        let lines = render_html("<pre>  hello   world  </pre>", 80);
        let text = collect_text(&lines);
        assert!(
            text.contains("  hello   world  "),
            "Expected preserved whitespace, got: {:?}",
            text
        );
    }

    #[test]
    fn test_inline_color_hex() {
        let lines =
            render_html("<span style=\"color: #ff0000\">Red text</span>", 80);
        let spans = find_spans_with_text(&lines, "Red text");
        assert!(!spans.is_empty());
        assert_eq!(spans[0].style.fg, Some(Color::Rgb(255, 0, 0)));
    }

    #[test]
    fn test_inline_color_rgb() {
        let lines = render_html(
            "<span style=\"color: rgb(0,128,255)\">Blue text</span>",
            80,
        );
        let spans = find_spans_with_text(&lines, "Blue text");
        assert!(!spans.is_empty());
        assert_eq!(spans[0].style.fg, Some(Color::Rgb(0, 128, 255)));
    }

    #[test]
    fn test_mixed_content() {
        let lines = render_html("<p>Hello <strong>bold</strong> world</p>", 80);
        let text = collect_text(&lines);
        assert!(text.contains("Hello"));
        assert!(text.contains("bold"));
        assert!(text.contains("world"));
        // Check that "bold" has BOLD modifier
        let bold_spans = find_spans_with_text(&lines, "bold");
        assert!(bold_spans[0].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn test_deeply_nested() {
        let lines = render_html(
            "<p><strong><em><u>deep</u></em></strong></p>",
            80,
        );
        let spans = find_spans_with_text(&lines, "deep");
        assert!(!spans.is_empty());
        let s = spans[0];
        assert!(s.style.add_modifier.contains(Modifier::BOLD));
        assert!(s.style.add_modifier.contains(Modifier::ITALIC));
        assert!(s.style.add_modifier.contains(Modifier::UNDERLINED));
    }

    #[test]
    fn test_table_basic() {
        let lines = render_html(
            "<table><tr><th>Name</th><th>Age</th></tr><tr><td>Alice</td><td>30</td></tr></table>",
            80,
        );
        let text = collect_text(&lines);
        assert!(text.contains("Name"));
        assert!(text.contains("Age"));
        assert!(text.contains("Alice"));
        assert!(text.contains("30"));
        assert!(text.contains("|"), "Expected | separator in table");
    }

    #[test]
    fn test_strikethrough() {
        let lines = render_html("<p><s>deleted</s></p>", 80);
        let spans = find_spans_with_text(&lines, "deleted");
        assert!(!spans.is_empty());
        assert!(spans[0].style.add_modifier.contains(Modifier::CROSSED_OUT));
    }

    #[test]
    fn test_link_shows_url() {
        let lines = render_html(
            "<a href=\"https://example.com\">Click</a>",
            80,
        );
        let text = collect_text(&lines);
        assert!(text.contains("Click"), "Expected link text");
        assert!(
            text.contains("[https://example.com]"),
            "Expected URL suffix, got: {:?}",
            text
        );
    }

    #[test]
    fn test_script_and_style_hidden() {
        let lines = render_html(
            "<p>visible</p><script>alert('x')</script><style>.x{}</style><p>also visible</p>",
            80,
        );
        let text = collect_text(&lines);
        assert!(text.contains("visible"));
        assert!(text.contains("also visible"));
        assert!(!text.contains("alert"), "Script content should be hidden");
        assert!(!text.contains(".x{"), "Style content should be hidden");
    }

    // === render_html_blocks tests ===

    #[test]
    fn test_blocks_text_image_text() {
        let blocks = render_html_blocks(
            r#"<p>text</p><img src="cid:abc" alt="photo"><p>more</p>"#,
            80,
        );
        assert_eq!(blocks.len(), 3, "Expected [Text, Image, Text], got {} blocks", blocks.len());
        assert!(matches!(&blocks[0], ContentBlock::Text(_)));
        assert!(matches!(&blocks[1], ContentBlock::Image { src, alt, .. }
            if src == "cid:abc" && alt == "photo"));
        assert!(matches!(&blocks[2], ContentBlock::Text(_)));
    }

    #[test]
    fn test_blocks_image_attributes() {
        let blocks = render_html_blocks(
            r#"<img src="cid:img001" alt="Logo">"#,
            80,
        );
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Image { src, alt, height } => {
                assert_eq!(src, "cid:img001");
                assert_eq!(alt, "Logo");
                assert_eq!(*height, 12);
            }
            other => panic!("Expected Image block, got {:?}", other),
        }
    }

    #[test]
    fn test_blocks_no_images() {
        let blocks = render_html_blocks("<p>Hello</p><p>World</p>", 80);
        assert_eq!(blocks.len(), 1, "HTML with no images should produce a single Text block");
        assert!(matches!(&blocks[0], ContentBlock::Text(_)));
    }

    #[test]
    fn test_blocks_consecutive_images() {
        let blocks = render_html_blocks(
            r#"<img src="cid:a" alt="A"><img src="cid:b" alt="B">"#,
            80,
        );
        assert_eq!(blocks.len(), 2, "Consecutive images should be separate blocks");
        assert!(matches!(&blocks[0], ContentBlock::Image { .. }));
        assert!(matches!(&blocks[1], ContentBlock::Image { .. }));
    }

    // === Whitespace / blank-line dedup tests ===

    fn count_blank_lines(lines: &[Line<'_>]) -> Vec<usize> {
        let mut runs = Vec::new();
        let mut count = 0;
        for line in lines {
            if is_visually_blank(line) {
                count += 1;
            } else {
                if count > 0 {
                    runs.push(count);
                }
                count = 0;
            }
        }
        if count > 0 {
            runs.push(count);
        }
        runs
    }

    #[test]
    fn test_no_consecutive_blanks_p_br_p() {
        let lines = render_html("<p>Hello</p><br><p>World</p>", 80);
        let runs = count_blank_lines(&lines);
        for (i, &run) in runs.iter().enumerate() {
            assert!(run <= 1, "Blank-line run #{} has {} consecutive blanks", i, run);
        }
    }

    #[test]
    fn test_no_consecutive_blanks_nested_divs() {
        let lines = render_html(
            "<div><div><p>Inner</p></div></div><p>Outer</p>",
            80,
        );
        let runs = count_blank_lines(&lines);
        for (i, &run) in runs.iter().enumerate() {
            assert!(run <= 1, "Blank-line run #{} has {} consecutive blanks", i, run);
        }
    }

    #[test]
    fn test_no_consecutive_blanks_spacer_p() {
        let lines = render_html("<p><br></p><p>Content</p>", 80);
        let runs = count_blank_lines(&lines);
        for (i, &run) in runs.iter().enumerate() {
            assert!(run <= 1, "Blank-line run #{} has {} consecutive blanks", i, run);
        }
    }

    #[test]
    fn test_div_no_extra_spacing() {
        let lines = render_html("<div>Hello</div><div>World</div>", 80);
        let text = collect_text(&lines);
        assert!(text.contains("Hello"));
        assert!(text.contains("World"));
        let runs = count_blank_lines(&lines);
        assert!(runs.is_empty(), "Adjacent divs should not produce blank lines, got runs: {:?}", runs);
    }

    #[test]
    fn test_is_visually_blank_helper() {
        assert!(is_visually_blank(&Line::from("")));
        assert!(is_visually_blank(&Line::from("  ")));
        assert!(is_visually_blank(&Line::from(vec![Span::raw(""), Span::raw("  ")])));
        assert!(!is_visually_blank(&Line::from("hello")));
        // Line with 0 spans
        assert!(is_visually_blank(&Line { spans: vec![], ..Default::default() }));
    }
}
