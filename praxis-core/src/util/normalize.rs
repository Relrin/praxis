/// Normalize a text string for hashing and comparison.
///
/// Steps applied in order:
/// 1. Convert to lowercase
/// 2. Strip leading speaker prefix (patterns: "User:", "Assistant:", ">", "##", "[timestamp]")
/// 3. Strip all ASCII punctuation except underscores, hyphens, dots, and forward slashes
///    (these are preserved because they appear in file paths and identifiers)
/// 4. Collapse multiple whitespace into single space
/// 5. Trim leading and trailing whitespace
///
/// This function is deterministic: same input always produces same output.
pub fn normalize(input: &str) -> String {
    normalize_with_options(input, false)
}

/// Normalize with control over prefix stripping.
///
/// When `skip_prefix_strip` is true, steps 2 (speaker prefix, heading,
/// blockquote, timestamp stripping) are skipped. Use this when the caller
/// has already stripped prefixes (e.g., the turn parser).
pub fn normalize_with_options(input: &str, skip_prefix_strip: bool) -> String {
    let mut s = input.to_lowercase();

    if !skip_prefix_strip {
        // Strip speaker prefixes
        let speaker_patterns = ["user:", "assistant:"];
        for prefix in &speaker_patterns {
            if s.trim_start().starts_with(prefix) {
                s = s.trim_start().strip_prefix(prefix).unwrap_or(&s).to_string();
            }
        }

        // Strip markdown heading prefix "## " or "# "
        let trimmed = s.trim_start();
        if trimmed.starts_with("## ") {
            s = trimmed.strip_prefix("## ").unwrap_or(trimmed).to_string();
        } else if trimmed.starts_with("# ") {
            s = trimmed.strip_prefix("# ").unwrap_or(trimmed).to_string();
        }

        // Strip blockquote prefix "> "
        let trimmed = s.trim_start();
        if trimmed.starts_with("> ") {
            s = trimmed.strip_prefix("> ").unwrap_or(trimmed).to_string();
        }

        // Strip timestamp prefix [HH:MM] or [HH:MM:SS]
        let trimmed = s.trim_start();
        if trimmed.starts_with('[')
            && let Some(end) = trimmed.find(']')
        {
            let inside = &trimmed[1..end];
            // Verify it looks like a timestamp (contains only digits and colons)
            if inside.chars().all(|c| c.is_ascii_digit() || c == ':') && inside.contains(':') {
                s = trimmed[end + 1..].trim_start().to_string();
            }
        }
    }

    // Strip apostrophes entirely (so "don't" -> "dont", not "don t")
    s = s.replace('\'', "");

    // Replace remaining punctuation with space (keep _, -, ., /)
    s = s
        .chars()
        .map(|c| {
            if c.is_alphanumeric()
                || c.is_whitespace()
                || c == '_'
                || c == '-'
                || c == '.'
                || c == '/'
            {
                c
            } else {
                ' '
            }
        })
        .collect();

    // Collapse whitespace
    let parts: Vec<&str> = s.split_whitespace().collect();
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_user_prefix() {
        assert_eq!(normalize("User: we must use JWT"), "we must use jwt");
    }

    #[test]
    fn strip_heading_prefix() {
        assert_eq!(normalize("## Assistant"), "assistant");
    }

    #[test]
    fn strip_blockquote_prefix() {
        assert_eq!(normalize("> avoid global state!"), "avoid global state");
    }

    #[test]
    fn strip_timestamp_prefix() {
        assert_eq!(
            normalize("[10:32] decided to use BTreeMap"),
            "decided to use btreemap"
        );
    }

    #[test]
    fn preserve_file_paths() {
        assert_eq!(
            normalize("src/auth.rs needs a rewrite"),
            "src/auth.rs needs a rewrite"
        );
    }

    #[test]
    fn collapse_whitespace() {
        assert_eq!(normalize("  multiple   spaces  "), "multiple spaces");
    }

    #[test]
    fn strip_punctuation_preserving_path_chars() {
        assert_eq!(normalize("don't use eval()"), "dont use eval");
    }

    #[test]
    fn with_options_skip_prefix() {
        // With skip_prefix_strip, speaker prefix is preserved (lowercased)
        assert_eq!(
            normalize_with_options("User: we must use JWT", true),
            "user we must use jwt"
        );
    }

    #[test]
    fn with_options_no_skip_matches_normalize() {
        let input = "User: we must use JWT";
        assert_eq!(normalize(input), normalize_with_options(input, false));
    }

    #[test]
    fn with_options_skip_still_strips_punctuation() {
        assert_eq!(
            normalize_with_options("don't use eval()", true),
            "dont use eval"
        );
    }
}
