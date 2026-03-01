// r[impl lexer.input-format.rule-order]
// r[impl lexer.input-format.bom-removal]
// r[impl lexer.input-format.crlf-normalization]
// r[impl lexer.input-format.shebang-removal]
/// Apply input-format normalization per `r[lexer.input-format.rule-order]`:
/// 1. BOM removal
/// 2. CRLF normalization
/// 3. Shebang removal
pub fn preprocess(input: &str) -> String {
    // r[impl lexer.input-format.bom-removal]
    let input = input.strip_prefix('\u{FEFF}').unwrap_or(input);

    // r[impl lexer.input-format.crlf-normalization]
    let input = input.replace("\r\n", "\n");

    // r[impl lexer.input-format.shebang-removal]
    if input.starts_with("#!") {
        match input.find('\n') {
            Some(pos) => input[pos + 1..].to_string(),
            None => String::new(),
        }
    } else {
        input
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_changes() {
        assert_eq!(preprocess("hello world"), "hello world");
    }

    // r[verify lexer.input-format.bom-removal]
    #[test]
    fn bom_removal() {
        assert_eq!(preprocess("\u{FEFF}hello"), "hello");
    }

    // r[verify lexer.input-format.bom-removal]
    #[test]
    fn bom_only_at_start() {
        assert_eq!(preprocess("a\u{FEFF}b"), "a\u{FEFF}b");
    }

    // r[verify lexer.input-format.crlf-normalization]
    #[test]
    fn crlf_normalization() {
        assert_eq!(preprocess("a\r\nb\r\nc"), "a\nb\nc");
    }

    #[test]
    fn lone_cr_unchanged() {
        assert_eq!(preprocess("a\rb"), "a\rb");
    }

    // r[verify lexer.input-format.shebang-removal]
    #[test]
    fn shebang_removal() {
        assert_eq!(preprocess("#!/usr/bin/env gnomon\nhello"), "hello");
    }

    // r[verify lexer.input-format.shebang-removal]
    #[test]
    fn shebang_only_line() {
        assert_eq!(preprocess("#!/usr/bin/env gnomon"), "");
    }

    #[test]
    fn not_shebang_if_no_bang() {
        assert_eq!(preprocess("#hello\nworld"), "#hello\nworld");
    }

    // r[verify lexer.input-format.rule-order]
    #[test]
    fn rule_order_bom_then_crlf_then_shebang() {
        let input = "\u{FEFF}#!/usr/bin/env gnomon\r\nhello\r\n";
        assert_eq!(preprocess(input), "hello\n");
    }

    #[test]
    fn empty_input() {
        assert_eq!(preprocess(""), "");
    }

    // r[verify lexer.input-format.bom-removal]
    #[test]
    fn bom_only() {
        assert_eq!(preprocess("\u{FEFF}"), "");
    }
}
