use crate::change::content_hash;
use crate::types::FileChunk;

/// Estimates token count from text using a simple chars/4 heuristic.
fn estimate_tokens(text: &str) -> usize {
    // Rough approximation: 1 token ~= 4 characters for English text
    (text.len() + 3) / 4
}

/// Splits file content into overlapping chunks suitable for embedding.
///
/// Each chunk contains at most `max_tokens` estimated tokens, with
/// `overlap_tokens` of overlap between consecutive chunks.
///
/// Returns an empty Vec for empty content.
pub fn chunk_file(
    file_path: &str,
    content: &str,
    max_tokens: usize,
    overlap_tokens: usize,
) -> Vec<FileChunk> {
    if content.is_empty() {
        return Vec::new();
    }

    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return Vec::new();
    }

    let max_chars = max_tokens * 4;
    let overlap_chars = overlap_tokens * 4;

    let mut chunks = Vec::new();
    let mut chunk_start_line = 0usize;
    let mut chunk_chars = 0usize;
    let mut chunk_lines: Vec<&str> = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let line_chars = line.len() + 1; // +1 for newline

        if !chunk_lines.is_empty() && chunk_chars + line_chars > max_chars {
            // Emit the current chunk
            let text = chunk_lines.join("\n");
            chunks.push(FileChunk {
                file_path: file_path.to_string(),
                chunk_index: chunks.len() as u32,
                content_hash: content_hash(&text),
                start_line: (chunk_start_line + 1) as u32,
                end_line: i as u32, // last included line (1-based would be i, since we didn't add current)
                text,
            });

            // Start new chunk with overlap
            let mut overlap_collected = 0usize;
            let mut overlap_start = chunk_lines.len();
            for j in (0..chunk_lines.len()).rev() {
                let lc = chunk_lines[j].len() + 1;
                if overlap_collected + lc > overlap_chars {
                    break;
                }
                overlap_collected += lc;
                overlap_start = j;
            }

            let overlap_line_offset = chunk_start_line + overlap_start;
            chunk_lines = chunk_lines[overlap_start..].to_vec();
            chunk_chars = overlap_collected;
            chunk_start_line = overlap_line_offset;
        }

        chunk_lines.push(line);
        chunk_chars += line_chars;
    }

    // Emit the final chunk
    if !chunk_lines.is_empty() {
        let text = chunk_lines.join("\n");
        chunks.push(FileChunk {
            file_path: file_path.to_string(),
            chunk_index: chunks.len() as u32,
            content_hash: content_hash(&text),
            start_line: (chunk_start_line + 1) as u32,
            end_line: lines.len() as u32,
            text,
        });
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_content_produces_no_chunks() {
        let chunks = chunk_file("test.rs", "", 256, 32);
        assert!(chunks.is_empty());
    }

    #[test]
    fn small_file_single_chunk() {
        let content = "fn main() {\n    println!(\"hello\");\n}";
        let chunks = chunk_file("main.rs", content, 256, 32);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].chunk_index, 0);
        assert_eq!(chunks[0].start_line, 1);
        assert_eq!(chunks[0].end_line, 3);
        assert_eq!(chunks[0].file_path, "main.rs");
        assert_eq!(chunks[0].text, content);
    }

    #[test]
    fn large_file_produces_multiple_chunks() {
        // Create content with 100 lines, each ~40 chars = ~1000 tokens
        let lines: Vec<String> = (0..100)
            .map(|i| format!("// This is line number {:03} with some padding text.", i))
            .collect();
        let content = lines.join("\n");

        // max_tokens = 50 (~200 chars), overlap = 10 (~40 chars)
        let chunks = chunk_file("big.rs", &content, 50, 10);

        assert!(chunks.len() > 1, "Expected multiple chunks, got {}", chunks.len());

        // All chunks should have valid line numbers
        for chunk in &chunks {
            assert!(chunk.start_line >= 1);
            assert!(chunk.end_line >= chunk.start_line);
            assert!(chunk.end_line <= 100);
            assert!(!chunk.text.is_empty());
        }

        // Chunk indices should be sequential
        for (i, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.chunk_index, i as u32);
        }
    }

    #[test]
    fn chunk_content_hashes_are_deterministic() {
        let content = "line one\nline two\nline three";
        let chunks1 = chunk_file("a.rs", content, 256, 32);
        let chunks2 = chunk_file("a.rs", content, 256, 32);

        assert_eq!(chunks1.len(), chunks2.len());
        for (c1, c2) in chunks1.iter().zip(chunks2.iter()) {
            assert_eq!(c1.content_hash, c2.content_hash);
        }
    }

    #[test]
    fn chunks_have_overlap_when_splitting() {
        // 20 lines, each ~80 chars = ~20 tokens per line
        let lines: Vec<String> = (0..20)
            .map(|i| format!("fn function_{i}() {{ /* body with some content to fill space */  }}"))
            .collect();
        let content = lines.join("\n");

        // max_tokens = 60 (~240 chars, ~3 lines), overlap = 20 (~80 chars, ~1 line)
        let chunks = chunk_file("funcs.rs", &content, 60, 20);

        if chunks.len() >= 2 {
            // Verify overlap: second chunk's start should be before first chunk's end
            assert!(
                chunks[1].start_line <= chunks[0].end_line,
                "Expected overlap between chunks: chunk0 ends at {}, chunk1 starts at {}",
                chunks[0].end_line,
                chunks[1].start_line
            );
        }
    }

    #[test]
    fn estimate_tokens_approximation() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcdefgh"), 2);
        // 100 chars ≈ 25 tokens
        let s: String = "a".repeat(100);
        assert_eq!(estimate_tokens(&s), 25);
    }
}
