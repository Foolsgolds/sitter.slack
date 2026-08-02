//! Port of `MessageDisplayItem.ParseSegments` / `AddTextAndEmojiSegments` from the
//! legacy WinUI app (`SlackSitter/Models/MessageDisplayItem.cs`).
//!
//! This is a **naive stateful toggle parser**, not a delimiter-pairing markdown
//! parser: `*`, `_`, `~` just flip a boolean each time they're seen, and a lone
//! unmatched delimiter makes the rest of the span carry that style. This must be
//! replicated exactly — do not "fix" it into a real markdown parser, callers
//! depend on pixel/behavior parity with the old app.

use std::collections::HashMap;

use once_cell::sync::Lazy;
use regex::Regex;

use super::models::MessageInlineSegment;

static LINK_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"<(?P<url>https?://[^|>]+)(\|(?P<label>[^>]+))?>").unwrap());

static EMOJI_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r":(?P<name>[a-zA-Z0-9_+\-]+):").unwrap());

/// Parses raw Slack mrkdwn text into a flat list of inline segments.
pub fn parse_segments(source_text: Option<&str>) -> Vec<MessageInlineSegment> {
    let decoded = html_escape::decode_html_entities(source_text.unwrap_or("")).into_owned();

    let mut segments = Vec::new();
    let mut current_index = 0usize;

    for capture in LINK_REGEX.captures_iter(&decoded) {
        let full_match = capture.get(0).unwrap();
        if full_match.start() > current_index {
            add_text_and_emoji_segments(&mut segments, &decoded[current_index..full_match.start()]);
        }

        let url_text = capture.name("url").unwrap().as_str();
        let label_text = capture.name("label").map(|m| m.as_str());

        match url::Url::parse(url_text) {
            Ok(parsed) => {
                let host = parsed.host_str().unwrap_or("");
                let display_text = if !host.is_empty() { host } else { url_text };
                segments.push(MessageInlineSegment::link(display_text, url_text));
            }
            Err(_) => {
                // Malformed link markup: fall back to rendering the label (or
                // raw url text if there's no label) as plain text/emoji.
                add_text_and_emoji_segments(&mut segments, label_text.unwrap_or(url_text));
            }
        }

        current_index = full_match.end();
    }

    if current_index < decoded.len() {
        add_text_and_emoji_segments(&mut segments, &decoded[current_index..]);
    }

    if segments.is_empty() {
        segments.push(MessageInlineSegment::text("", false, false, false, false));
    }

    segments
}

/// Single left-to-right character scan over a plain-text span (no Slack link
/// markup inside it — that was already stripped out by the caller).
fn add_text_and_emoji_segments(segments: &mut Vec<MessageInlineSegment>, text: &str) {
    // Pass 1: index every emoji shortcode match by its byte start offset for
    // O(1) lookup during the scan below.
    let emoji_by_index: HashMap<usize, regex::Match> = EMOJI_REGEX
        .find_iter(text)
        .map(|m| (m.start(), m))
        .collect();

    // Pass 2: single forward scan with a text buffer and four style toggles.
    let mut buffer = String::new();
    let mut is_bold = false;
    let mut is_italic = false;
    let mut is_strikethrough = false;
    let mut is_code = false;

    let mut pos = 0usize;
    while pos < text.len() {
        if !is_code {
            if let Some(m) = emoji_by_index.get(&pos) {
                flush_text_segment(segments, &mut buffer, is_bold, is_italic, is_strikethrough, is_code);
                let name = &text[m.start() + 1..m.end() - 1];
                segments.push(MessageInlineSegment::emoji(name));
                pos = m.end();
                continue;
            }
        }

        let ch = text[pos..].chars().next().expect("pos is a valid char boundary");
        match ch {
            '`' => {
                flush_text_segment(segments, &mut buffer, is_bold, is_italic, is_strikethrough, is_code);
                is_code = !is_code;
            }
            '*' if !is_code => {
                flush_text_segment(segments, &mut buffer, is_bold, is_italic, is_strikethrough, is_code);
                is_bold = !is_bold;
            }
            '_' if !is_code => {
                flush_text_segment(segments, &mut buffer, is_bold, is_italic, is_strikethrough, is_code);
                is_italic = !is_italic;
            }
            '~' if !is_code => {
                flush_text_segment(segments, &mut buffer, is_bold, is_italic, is_strikethrough, is_code);
                is_strikethrough = !is_strikethrough;
            }
            other => buffer.push(other),
        }
        pos += ch.len_utf8();
    }

    flush_text_segment(segments, &mut buffer, is_bold, is_italic, is_strikethrough, is_code);
}

fn flush_text_segment(
    segments: &mut Vec<MessageInlineSegment>,
    buffer: &mut String,
    is_bold: bool,
    is_italic: bool,
    is_strikethrough: bool,
    is_code: bool,
) {
    if buffer.is_empty() {
        return;
    }
    segments.push(MessageInlineSegment::text(
        std::mem::take(buffer),
        is_bold,
        is_italic,
        is_strikethrough,
        is_code,
    ));
}

#[cfg(test)]
mod tests {
    use super::super::models::SegmentType;
    use super::*;

    fn texts(segments: &[MessageInlineSegment]) -> Vec<&str> {
        segments.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn empty_input_yields_one_empty_text_segment() {
        let segments = parse_segments(None);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].seg_type, SegmentType::Text);
        assert_eq!(segments[0].text, "");
    }

    #[test]
    fn plain_text_is_a_single_segment() {
        let segments = parse_segments(Some("hello world"));
        assert_eq!(texts(&segments), vec!["hello world"]);
        assert!(!segments[0].is_bold);
    }

    #[test]
    fn bold_toggle_wraps_matching_pair() {
        let segments = parse_segments(Some("*bold*"));
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "bold");
        assert!(segments[0].is_bold);
    }

    #[test]
    fn unmatched_asterisk_bleeds_bold_to_end_of_string() {
        // Naive toggle semantics: a lone `*` (e.g. in "5 * 3 = 15") starts a
        // bold run that never closes, unlike a real markdown parser.
        let segments = parse_segments(Some("5 * 3 = 15"));
        assert_eq!(texts(&segments), vec!["5 ", " 3 = 15"]);
        assert!(!segments[0].is_bold);
        assert!(segments[1].is_bold);
    }

    #[test]
    fn combined_styles_apply_to_same_segment() {
        let segments = parse_segments(Some("*_both_*"));
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "both");
        assert!(segments[0].is_bold);
        assert!(segments[0].is_italic);
    }

    #[test]
    fn code_span_suppresses_emoji_and_style_delimiters() {
        let segments = parse_segments(Some("`:smile:*not-bold*`"));
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, ":smile:*not-bold*");
        assert!(segments[0].is_code);
        assert!(!segments[0].is_bold);
    }

    #[test]
    fn emoji_shortcode_becomes_emoji_segment() {
        let segments = parse_segments(Some("hi :wave: there"));
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].text, "hi ");
        assert_eq!(segments[1].seg_type, SegmentType::Emoji);
        assert_eq!(segments[1].text, "wave");
        assert_eq!(segments[2].text, " there");
    }

    #[test]
    fn slack_link_with_label_renders_as_hostname() {
        let segments = parse_segments(Some("see <https://example.com/foo|Example>"));
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[1].seg_type, SegmentType::Link);
        assert_eq!(segments[1].text, "example.com");
        assert_eq!(segments[1].uri.as_deref(), Some("https://example.com/foo"));
    }

    #[test]
    fn slack_link_without_label_uses_full_url_as_fallback_when_host_empty() {
        let segments = parse_segments(Some("<https://example.com>"));
        assert_eq!(segments[0].seg_type, SegmentType::Link);
        assert_eq!(segments[0].text, "example.com");
    }

    #[test]
    fn html_entities_are_decoded_before_parsing() {
        let segments = parse_segments(Some("AT&amp;T"));
        assert_eq!(texts(&segments), vec!["AT&T"]);
    }

    #[test]
    fn multiline_text_is_kept_in_one_segment_with_newline() {
        let segments = parse_segments(Some("line1\nline2"));
        assert_eq!(texts(&segments), vec!["line1\nline2"]);
    }
}
