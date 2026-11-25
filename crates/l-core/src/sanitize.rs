//! Sanitization helpers for untrusted external strings.

/// Maximum length for a single displayed external string.
pub const MAX_DISPLAY_LEN: usize = 200;

/// Strip control characters, bidirectional overrides, and cap length.
/// Returns an owned, display-safe string.
#[must_use]
pub fn sanitize_display(input: &str) -> String {
    let mut out = String::with_capacity(input.len().min(MAX_DISPLAY_LEN));
    for ch in input.chars() {
        if out.chars().count() >= MAX_DISPLAY_LEN {
            break;
        }
        if ch.is_control()
            || matches!(
                ch,
                '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{200E}' | '\u{200F}'
            )
        {
            continue;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_bidi_and_control() {
        assert_eq!(sanitize_display("he\u{202E}llo"), "hello");
        assert_eq!(sanitize_display("a\u{0007}b"), "ab");
    }

    #[test]
    fn caps_length() {
        let long = "x".repeat(500);
        assert_eq!(sanitize_display(&long).len(), MAX_DISPLAY_LEN);
    }
}
