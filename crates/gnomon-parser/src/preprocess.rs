// r[impl lexer.input-format.rule-order]
// r[impl lexer.input-format.bom-removal]
// r[impl lexer.input-format.crlf-normalization]
// r[impl lexer.input-format.shebang-removal]
/// Apply input-format normalization per `r[lexer.input-format.rule-order]`:
/// 1. BOM removal
/// 2. CRLF normalization
/// 3. Shebang removal
///
/// Returns `Cow::Borrowed` when no transformation is needed, avoiding
/// allocation for typical source files (no BOM, no CRLF, no shebang).
pub fn preprocess(input: &str) -> std::borrow::Cow<'_, str> {
    use std::borrow::Cow;

    let has_bom = input.starts_with('\u{FEFF}');
    let has_crlf = input.contains("\r\n");
    let after_bom = if has_bom {
        &input['\u{FEFF}'.len_utf8()..]
    } else {
        input
    };
    let has_shebang = after_bom.starts_with("#!");

    // Fast path: no transformations needed.
    if !has_bom && !has_crlf && !has_shebang {
        return Cow::Borrowed(input);
    }

    // r[impl lexer.input-format.bom-removal]
    // (already computed as `after_bom`)

    // r[impl lexer.input-format.crlf-normalization]
    let normalized: Cow<'_, str> = if has_crlf {
        Cow::Owned(after_bom.replace("\r\n", "\n"))
    } else {
        Cow::Borrowed(after_bom)
    };

    // r[impl lexer.input-format.shebang-removal]
    if normalized.starts_with("#!") {
        match normalized.find('\n') {
            Some(pos) => Cow::Owned(normalized[pos + 1..].to_string()),
            None => Cow::Owned(String::new()),
        }
    } else {
        normalized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_changes() {
        assert_eq!(preprocess("hello world"), "hello world");
    }

    #[test]
    fn no_changes_returns_borrowed() {
        let input = "hello world";
        assert!(matches!(preprocess(input), std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn empty_input_returns_borrowed() {
        assert!(matches!(preprocess(""), std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn bom_only_returns_borrowed_slice() {
        // BOM removal is a slice, not an allocation, so still Borrowed.
        assert!(matches!(
            preprocess("\u{FEFF}hello"),
            std::borrow::Cow::Borrowed(_)
        ));
    }

    #[test]
    fn crlf_returns_owned() {
        assert!(matches!(preprocess("a\r\nb"), std::borrow::Cow::Owned(_)));
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

    // r[verify lexer.input-format.rule-order]
    #[test]
    fn whitespace_only_after_preprocessing() {
        assert_eq!(preprocess("   \n\t\n  "), "   \n\t\n  ");
    }

    // r[verify lexer.input-format.crlf-normalization]
    #[test]
    fn mixed_crlf_and_lf() {
        assert_eq!(preprocess("a\r\nb\nc\r\nd"), "a\nb\nc\nd");
    }

    // r[verify lexer.input-format.rule-order]
    #[test]
    fn shebang_with_crlf_followed_by_whitespace() {
        assert_eq!(preprocess("#!/usr/bin/env gnomon\r\n   \n\t"), "   \n\t");
    }

    // r[verify lexer.input-format.rule-order]
    #[test]
    fn bom_crlf_shebang_combined() {
        let input = "\u{FEFF}#!/usr/bin/env gnomon\r\n\r\nhello\r\nworld";
        assert_eq!(preprocess(input), "\nhello\nworld");
    }
}
