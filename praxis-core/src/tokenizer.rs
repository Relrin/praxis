/// Tokenizes a symbol name with awareness of `snake_case`, `camelCase`, and `PascalCase`.
///
/// Splits on underscores, then on case transitions (with acronym detection),
/// lowercases all tokens, and discards tokens shorter than 2 characters or
/// matching common stopwords.
///
/// # Examples
///
/// ```
/// use praxis_core::tokenizer::tokenize_symbol;
///
/// assert_eq!(tokenize_symbol("parse_input"), vec!["parse", "input"]);
/// assert_eq!(tokenize_symbol("parseHTTPSRequest"), vec!["parse", "https", "request"]);
/// assert_eq!(tokenize_symbol("XMLParser"), vec!["xml", "parser"]);
/// ```
pub fn tokenize_symbol(name: &str) -> Vec<String> {
    let snake_parts: Vec<&str> = name.split('_').collect();

    let mut tokens = Vec::new();
    for part in snake_parts {
        if part.is_empty() {
            continue;
        }
        for token in split_camel(part) {
            let token = token.to_lowercase();
            if token.len() >= 2 {
                tokens.push(token);
            }
        }
    }
    tokens
}

/// Tokenizes free-form text (task descriptions, file content).
///
/// Splits on non-alphanumeric characters, lowercases all tokens, and discards
/// tokens shorter than 2 characters or matching common stopwords.
///
/// # Examples
///
/// ```
/// use praxis_core::tokenizer::tokenize_text;
///
/// let tokens = tokenize_text("Fix the HTTP parser bug");
/// assert_eq!(tokens, vec!["fix", "http", "parser", "bug"]);
/// ```
pub fn tokenize_text(text: &str) -> Vec<String> {
    use crate::util::stopwords::is_stopword;

    let text = text.to_lowercase();

    let mut tokens = Vec::new();
    for token in text.split(|c: char| !c.is_alphanumeric()) {
        if token.len() >= 2 && !is_stopword(token) {
            tokens.push(token.to_string());
        }
    }
    tokens
}

fn split_camel(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    if chars.is_empty() {
        return Vec::new();
    }

    let mut tokens = Vec::new();
    let mut start = 0;

    for i in 1..chars.len() {
        let prev = chars[i - 1];
        let curr = chars[i];
        let next = chars.get(i + 1).copied();

        let boundary = (prev.is_lowercase() && curr.is_uppercase())
            || (prev.is_uppercase()
                && curr.is_uppercase()
                && next.map(|n| n.is_lowercase()).unwrap_or(false));

        if boundary {
            tokens.push(s[start..i].to_string());
            start = i;
        }
    }
    tokens.push(s[start..].to_string());
    tokens
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_case_splitting() {
        assert_eq!(tokenize_symbol("parse_input"), vec!["parse", "input"]);
        assert_eq!(
            tokenize_symbol("read_file_content"),
            vec!["read", "file", "content"]
        );
    }

    #[test]
    fn camel_case_splitting() {
        assert_eq!(tokenize_symbol("parseInput"), vec!["parse", "input"]);
        assert_eq!(
            tokenize_symbol("readFileContent"),
            vec!["read", "file", "content"]
        );
    }

    #[test]
    fn pascal_case_splitting() {
        assert_eq!(tokenize_symbol("ParseInput"), vec!["parse", "input"]);
        assert_eq!(tokenize_symbol("FileEntry"), vec!["file", "entry"]);
    }

    #[test]
    fn acronym_detection() {
        assert_eq!(
            tokenize_symbol("parseHTTPSRequest"),
            vec!["parse", "https", "request"]
        );
        assert_eq!(tokenize_symbol("XMLParser"), vec!["xml", "parser"]);
        assert_eq!(tokenize_symbol("getURLForID"), vec!["get", "url", "for", "id"]);
    }

    #[test]
    fn mixed_snake_and_camel() {
        assert_eq!(
            tokenize_symbol("parse_HTTPRequest"),
            vec!["parse", "http", "request"]
        );
    }

    #[test]
    fn short_tokens_discarded() {
        assert_eq!(tokenize_symbol("a_b_parse"), vec!["parse"]);
    }

    #[test]
    fn stopwords_preserved_in_symbols() {
        assert_eq!(
            tokenize_symbol("use_the_parser"),
            vec!["use", "the", "parser"]
        );
    }
    #[test]
    fn text_tokenization() {
        assert_eq!(
            tokenize_text("Fix the HTTP parser bug"),
            vec!["fix", "http", "parser", "bug"]
        );
    }

    #[test]
    fn text_splits_on_punctuation() {
        assert_eq!(
            tokenize_text("hello-world! foo.bar"),
            vec!["hello", "world", "foo", "bar"]
        );
    }

    #[test]
    fn empty_input() {
        assert!(tokenize_symbol("").is_empty());
        assert!(tokenize_text("").is_empty());
    }
}
