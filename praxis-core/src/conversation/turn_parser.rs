/// A single turn in a parsed conversation.
#[derive(Debug, Clone)]
pub struct Turn {
    /// 0-based sequential index.
    pub turn_index: usize,
    /// Optional speaker hint: Some("user"), Some("assistant"), or None.
    pub speaker_hint: Option<String>,
    /// The text lines of this turn, with speaker prefix already stripped.
    pub lines: Vec<String>,
}

/// Detected input layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// Blank-line separated blocks with optional "User:" / "Assistant:" prefix.
    BlankLineSeparated,
    /// Markdown heading per turn ("## User", "## Assistant").
    MarkdownHeading,
    /// Timestamped log lines ("[HH:MM]" or "[HH:MM:SS]" prefix).
    Timestamped,
}

/// Detect the input layout by scanning the first 50 non-empty lines.
///
/// Priority order (first match wins):
/// 1. MarkdownHeading — any line starts with "## user" or "## assistant"
/// 2. Timestamped — any line starts with `[HH:MM`
/// 3. BlankLineSeparated — default fallback
pub fn detect_layout(content: &str) -> Layout {
    let lines: Vec<&str> = content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(50)
        .collect();

    for line in &lines {
        let trimmed = line.trim().to_lowercase();
        if trimmed.starts_with("## user") || trimmed.starts_with("## assistant") {
            return Layout::MarkdownHeading;
        }
    }

    for line in &lines {
        if is_timestamp_line(line.trim()) {
            return Layout::Timestamped;
        }
    }

    Layout::BlankLineSeparated
}

/// Parse raw content into a sequence of turns.
///
/// Automatically detects layout and applies the appropriate splitting strategy.
pub fn parse_turns(content: &str) -> Vec<Turn> {
    let layout = detect_layout(content);
    match layout {
        Layout::BlankLineSeparated => parse_blank_line_separated(content),
        Layout::MarkdownHeading => parse_markdown_heading(content),
        Layout::Timestamped => parse_timestamped(content),
    }
}

/// Check if a trimmed line starts with a timestamp pattern `[HH:MM`.
fn is_timestamp_line(trimmed: &str) -> bool {
    if !trimmed.starts_with('[') {
        return false;
    }
    if let Some(colon_pos) = trimmed[1..].find(':') {
        let before_colon = &trimmed[1..1 + colon_pos];
        before_colon.len() <= 2
            && !before_colon.is_empty()
            && before_colon.chars().all(|c| c.is_ascii_digit())
    } else {
        false
    }
}

/// Strip timestamp prefix from a line, returning the content after `]`.
fn strip_timestamp(line: &str) -> &str {
    let trimmed = line.trim();
    if let Some(end) = trimmed.find(']') {
        trimmed[end + 1..].trim_start()
    } else {
        trimmed
    }
}

fn parse_blank_line_separated(content: &str) -> Vec<Turn> {
    let mut turns = Vec::new();
    let mut current_block: Vec<String> = Vec::new();

    // Group lines into blocks separated by blank lines
    let mut blocks: Vec<Vec<String>> = Vec::new();
    for line in content.lines() {
        if line.trim().is_empty() {
            if !current_block.is_empty() {
                blocks.push(std::mem::take(&mut current_block));
            }
        } else {
            current_block.push(line.trim_end().to_string());
        }
    }
    if !current_block.is_empty() {
        blocks.push(current_block);
    }

    for (idx, block) in blocks.into_iter().enumerate() {
        if block.is_empty() {
            continue;
        }

        let first_line = &block[0];
        let (speaker_hint, first_content) = extract_speaker_prefix(first_line);

        let mut lines = Vec::with_capacity(block.len());
        lines.push(first_content);
        for line in &block[1..] {
            lines.push(line.clone());
        }

        // Skip turns that are entirely empty after stripping
        if lines.iter().all(|l| l.trim().is_empty()) {
            continue;
        }

        turns.push(Turn {
            turn_index: idx,
            speaker_hint,
            lines,
        });
    }

    // Re-index sequentially (blocks may have been skipped)
    for (i, turn) in turns.iter_mut().enumerate() {
        turn.turn_index = i;
    }

    turns
}

/// Extract speaker prefix from a line.
/// Returns (speaker_hint, remaining_content).
fn extract_speaker_prefix(line: &str) -> (Option<String>, String) {
    let trimmed = line.trim_start();
    let lower = trimmed.to_lowercase();

    if lower.starts_with("user:") {
        let rest = trimmed[5..].trim_start();
        (Some("user".to_string()), rest.to_string())
    } else if lower.starts_with("assistant:") {
        let rest = trimmed[10..].trim_start();
        (Some("assistant".to_string()), rest.to_string())
    } else if let Some(stripped) = trimmed.strip_prefix('>') {
        let rest = stripped.trim_start();
        (None, rest.to_string())
    } else {
        (None, line.trim_end().to_string())
    }
}

fn parse_markdown_heading(content: &str) -> Vec<Turn> {
    let mut turns = Vec::new();
    let mut current_speaker: Option<String> = None;
    let mut current_lines: Vec<String> = Vec::new();
    let mut has_heading = false;

    for line in content.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_lowercase();

        if lower.starts_with("## user") || lower.starts_with("## assistant") {
            // Save previous turn
            if has_heading {
                let non_empty = current_lines.iter().any(|l| !l.trim().is_empty());
                if non_empty {
                    turns.push(Turn {
                        turn_index: turns.len(),
                        speaker_hint: current_speaker.take(),
                        lines: std::mem::take(&mut current_lines),
                    });
                }
            } else if !current_lines.is_empty()
                && current_lines.iter().any(|l| !l.trim().is_empty())
            {
                // Content before first heading → turn 0
                turns.push(Turn {
                    turn_index: 0,
                    speaker_hint: None,
                    lines: std::mem::take(&mut current_lines),
                });
            }

            has_heading = true;
            current_lines.clear();

            if lower.starts_with("## user") {
                current_speaker = Some("user".to_string());
            } else {
                current_speaker = Some("assistant".to_string());
            }
        } else {
            current_lines.push(line.trim_end().to_string());
        }
    }

    // Don't forget the last section
    if has_heading {
        let non_empty = current_lines.iter().any(|l| !l.trim().is_empty());
        if non_empty {
            turns.push(Turn {
                turn_index: turns.len(),
                speaker_hint: current_speaker,
                lines: current_lines,
            });
        }
    } else if !current_lines.is_empty()
        && current_lines.iter().any(|l| !l.trim().is_empty())
    {
        turns.push(Turn {
            turn_index: 0,
            speaker_hint: None,
            lines: current_lines,
        });
    }

    // Re-index
    for (i, turn) in turns.iter_mut().enumerate() {
        turn.turn_index = i;
    }

    turns
}

fn parse_timestamped(content: &str) -> Vec<Turn> {
    let mut turns: Vec<Turn> = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if is_timestamp_line(trimmed) {
            let content_after_ts = strip_timestamp(trimmed).to_string();
            turns.push(Turn {
                turn_index: turns.len(),
                speaker_hint: None,
                lines: vec![content_after_ts],
            });
        } else if let Some(last) = turns.last_mut() {
            // Continuation of previous turn
            last.lines.push(trimmed.to_string());
        }
        // Lines before any timestamp are discarded
    }

    turns
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Layout detection ---

    #[test]
    fn detect_markdown_heading() {
        assert_eq!(
            detect_layout("## User\nHello\n\n## Assistant\nHi"),
            Layout::MarkdownHeading
        );
    }

    #[test]
    fn detect_timestamped() {
        assert_eq!(
            detect_layout("[10:32] hello\n[10:33] world"),
            Layout::Timestamped
        );
    }

    #[test]
    fn detect_blank_line_separated_with_prefix() {
        assert_eq!(
            detect_layout("User: hello\n\nAssistant: hi"),
            Layout::BlankLineSeparated
        );
    }

    #[test]
    fn detect_blank_line_separated_plain() {
        assert_eq!(
            detect_layout("just some text\nmore text"),
            Layout::BlankLineSeparated
        );
    }

    #[test]
    fn detect_markdown_heading_lowercase() {
        assert_eq!(
            detect_layout("## user\nlowercase heading"),
            Layout::MarkdownHeading
        );
    }

    #[test]
    fn detect_timestamped_single_digit_hour() {
        assert_eq!(
            detect_layout("[9:05] single digit hour"),
            Layout::Timestamped
        );
    }

    // --- Turn splitting: BlankLineSeparated ---

    #[test]
    fn blank_line_three_blocks() {
        let turns = parse_turns("User: hello\n\nAssistant: world\n\nsome text");
        assert_eq!(turns.len(), 3);
        assert_eq!(turns[0].speaker_hint, Some("user".to_string()));
        assert_eq!(turns[0].lines, vec!["hello"]);
        assert_eq!(turns[1].speaker_hint, Some("assistant".to_string()));
        assert_eq!(turns[1].lines, vec!["world"]);
        assert_eq!(turns[2].speaker_hint, None);
        assert_eq!(turns[2].lines, vec!["some text"]);
    }

    #[test]
    fn blank_line_multiple_blank_lines() {
        let turns = parse_turns("User: a\n\n\n\nAssistant: b\n\n\nno prefix");
        assert_eq!(turns.len(), 3);
        assert_eq!(turns[0].turn_index, 0);
        assert_eq!(turns[1].turn_index, 1);
        assert_eq!(turns[2].turn_index, 2);
    }

    #[test]
    fn blank_line_blockquote_prefix() {
        let turns = parse_turns("> quoted text");
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].speaker_hint, None);
        assert_eq!(turns[0].lines, vec!["quoted text"]);
    }

    // --- Turn splitting: MarkdownHeading ---

    #[test]
    fn markdown_heading_four_turns() {
        let input = "## User\nQ1\n\n## Assistant\nA1\n\n## User\nQ2\n\n## Assistant\nA2";
        let turns = parse_turns(input);
        assert_eq!(turns.len(), 4);
        assert_eq!(turns[0].speaker_hint, Some("user".to_string()));
        assert_eq!(turns[1].speaker_hint, Some("assistant".to_string()));
        assert_eq!(turns[2].speaker_hint, Some("user".to_string()));
        assert_eq!(turns[3].speaker_hint, Some("assistant".to_string()));
    }

    #[test]
    fn markdown_heading_content_before_first() {
        let input = "Preamble text\n\n## User\nHello";
        let turns = parse_turns(input);
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].speaker_hint, None);
        assert_eq!(turns[0].lines.iter().any(|l| l.contains("Preamble")), true);
        assert_eq!(turns[1].speaker_hint, Some("user".to_string()));
    }

    // --- Turn splitting: Timestamped ---

    #[test]
    fn timestamped_with_continuations() {
        let input = "[10:00] first line\ncontinuation\n[10:01] second\n[10:02] third";
        let turns = parse_turns(input);
        assert_eq!(turns.len(), 3);
        assert_eq!(turns[0].lines.len(), 2); // "first line" + "continuation"
        assert_eq!(turns[0].lines[0], "first line");
        assert_eq!(turns[0].lines[1], "continuation");
        assert_eq!(turns[1].lines, vec!["second"]);
        assert_eq!(turns[2].lines, vec!["third"]);
    }

    #[test]
    fn timestamped_strips_timestamp() {
        let turns = parse_turns("[14:30:05] hello world");
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].lines, vec!["hello world"]);
    }

    // --- Edge cases ---

    #[test]
    fn empty_string() {
        assert!(parse_turns("").is_empty());
    }

    #[test]
    fn only_blank_lines() {
        assert!(parse_turns("\n\n\n").is_empty());
    }

    #[test]
    fn single_line_no_speaker() {
        let turns = parse_turns("hello");
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].speaker_hint, None);
        assert_eq!(turns[0].turn_index, 0);
    }

    #[test]
    fn turn_indices_sequential() {
        let turns = parse_turns("a\n\nb\n\nc\n\nd");
        for (i, turn) in turns.iter().enumerate() {
            assert_eq!(turn.turn_index, i);
        }
    }

    #[test]
    fn multiline_turn() {
        let turns = parse_turns("User: line one\nline two\nline three");
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].lines.len(), 3);
        assert_eq!(turns[0].lines[0], "line one");
        assert_eq!(turns[0].lines[1], "line two");
    }
}
