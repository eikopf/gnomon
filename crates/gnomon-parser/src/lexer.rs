use logos::Logos;

use crate::syntax_kind::SyntaxKind;

/// A single token produced by the lexer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: SyntaxKind,
    pub text: String,
}

/// Internal logos token enum. Maps 1:1 to the token variants of `SyntaxKind`
/// but exists separately because `SyntaxKind` also contains node kinds.
#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq)]
enum LogosToken {
    // ── Trivia ───────────────────────────────────────────────────
    // r[impl lexer.whitespace]
    #[regex(r"[\t\n\x0B\x0C\r \u{85}\u{200E}\u{200F}\u{2028}\u{2029}]+")]
    Whitespace,

    // r[impl lexer.comment]
    #[regex(r";[^\n]*", allow_greedy = true)]
    Comment,

    // r[impl lexer.punctuation]
    // ── Punctuation ──────────────────────────────────────────────
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token(":")]
    Colon,
    #[token(",")]
    Comma,
    #[token("==")]
    EqEq,
    #[token("=")]
    Equals,
    #[token("!=")]
    BangEq,
    #[token("!")]
    Bang,
    #[token(".")]
    Dot,
    #[token("++")]
    PlusPlus,
    #[token("+")]
    Plus,
    #[token("//")]
    SlashSlash,
    #[token("/")]
    Slash,

    // ── Literals (ordered longest-match: datetime > date > month-day,
    //    time, duration > signed-int > integer) ───────────────────

    // r[impl lexer.datetime]
    #[regex(r"[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}(:[0-9]{2})?")]
    DatetimeLiteral,

    // r[impl lexer.date]
    #[regex(r"[0-9]{4}-[0-9]{2}-[0-9]{2}")]
    DateLiteral,

    // r[impl lexer.time]
    #[regex(r"[0-9]{2}:[0-9]{2}(:[0-9]{2})?")]
    TimeLiteral,

    // r[impl lexer.month-day]
    #[regex(r"[0-9]{2}-[0-9]{2}")]
    MonthDayLiteral,

    // r[impl lexer.duration]
    #[regex(r"[+-]?[0-9]+[wdhms]([0-9]+[wdhms])*")]
    DurationLiteral,

    // r[impl lexer.signed-integer]
    #[regex(r"[+-][0-9]+")]
    SignedIntegerLiteral,

    // r[impl lexer.string]
    // r[impl lexer.string.escape]
    #[regex(r#""([^"\\]|\\.)*""#, allow_greedy = true)]
    StringLiteral,

    // r[impl lexer.integer]
    #[regex(r"[0-9]+")]
    IntegerLiteral,

    // r[impl lexer.uri]
    #[regex(r"<[a-zA-Z][a-zA-Z0-9+.\-]*:[^>\n]*>")]
    UriLiteral,

    // r[impl lexer.atom]
    #[regex(r"#[a-zA-Z_][a-zA-Z0-9_\-]*")]
    AtomLiteral,

    // r[impl syntax.name]
    #[regex(r"@[a-zA-Z_][a-zA-Z0-9_-]*(\.[a-zA-Z_][a-zA-Z0-9_-]*)*")]
    Name,

    // r[impl lexer.keyword.strict]
    #[token("true")]
    True,
    #[token("false")]
    False,
    #[token("undefined")]
    Undefined,

    // r[impl lexer.path]
    #[regex(r"(\.\.|\.)/[a-zA-Z0-9_\-./]*|[a-zA-Z_][a-zA-Z0-9_\-.]*(/[a-zA-Z0-9_\-./]*)+")]
    PathLiteral,

    // r[impl lexer.ident]
    // r[impl lexer.keyword.weak]
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_-]*")]
    Ident,
}

impl LogosToken {
    fn to_syntax_kind(self) -> SyntaxKind {
        match self {
            LogosToken::Whitespace => SyntaxKind::WHITESPACE,
            LogosToken::Comment => SyntaxKind::COMMENT,
            LogosToken::LBrace => SyntaxKind::L_BRACE,
            LogosToken::RBrace => SyntaxKind::R_BRACE,
            LogosToken::LBracket => SyntaxKind::L_BRACKET,
            LogosToken::RBracket => SyntaxKind::R_BRACKET,
            LogosToken::LParen => SyntaxKind::L_PAREN,
            LogosToken::RParen => SyntaxKind::R_PAREN,
            LogosToken::Colon => SyntaxKind::COLON,
            LogosToken::Comma => SyntaxKind::COMMA,
            LogosToken::EqEq => SyntaxKind::EQ_EQ,
            LogosToken::Equals => SyntaxKind::EQUALS,
            LogosToken::BangEq => SyntaxKind::BANG_EQ,
            LogosToken::Bang => SyntaxKind::BANG,
            LogosToken::Dot => SyntaxKind::DOT,
            LogosToken::PlusPlus => SyntaxKind::PLUS_PLUS,
            LogosToken::Plus => SyntaxKind::PLUS,
            LogosToken::SlashSlash => SyntaxKind::SLASH_SLASH,
            LogosToken::Slash => SyntaxKind::SLASH,
            LogosToken::DatetimeLiteral => SyntaxKind::DATETIME_LITERAL,
            LogosToken::DateLiteral => SyntaxKind::DATE_LITERAL,
            LogosToken::TimeLiteral => SyntaxKind::TIME_LITERAL,
            LogosToken::MonthDayLiteral => SyntaxKind::MONTH_DAY_LITERAL,
            LogosToken::DurationLiteral => SyntaxKind::DURATION_LITERAL,
            LogosToken::SignedIntegerLiteral => SyntaxKind::SIGNED_INTEGER_LITERAL,
            LogosToken::StringLiteral => SyntaxKind::STRING_LITERAL,
            LogosToken::IntegerLiteral => SyntaxKind::INTEGER_LITERAL,
            LogosToken::UriLiteral => SyntaxKind::URI_LITERAL,
            LogosToken::AtomLiteral => SyntaxKind::ATOM_LITERAL,
            LogosToken::PathLiteral => SyntaxKind::PATH_LITERAL,
            LogosToken::Name => SyntaxKind::NAME,
            LogosToken::True => SyntaxKind::TRUE_KW,
            LogosToken::False => SyntaxKind::FALSE_KW,
            LogosToken::Undefined => SyntaxKind::UNDEFINED_KW,
            LogosToken::Ident => SyntaxKind::IDENT,
        }
    }
}

/// Tokenize the input string into a sequence of tokens.
/// Unrecognized bytes produce `ERROR` tokens.
pub fn lex(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut lexer = LogosToken::lexer(input);

    while let Some(result) = lexer.next() {
        let text = lexer.slice().to_string();
        let kind = match result {
            Ok(tok) => tok.to_syntax_kind(),
            Err(()) => SyntaxKind::ERROR,
        };
        tokens.push(Token { kind, text });
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(input: &str) -> Vec<(SyntaxKind, &str)> {
        let tokens = lex(input);
        // Re-lex from input to get &str slices for comparison
        let mut result = Vec::new();
        let mut pos = 0;
        for tok in &tokens {
            let end = pos + tok.text.len();
            result.push((tok.kind, &input[pos..end]));
            pos = end;
        }
        result
    }

    // ── Ambiguity resolution ─────────────────────────────────────

    // r[verify lexer.datetime]
    #[test]
    fn datetime_wins_over_date() {
        let toks = kinds("2026-03-01T14:30");
        assert_eq!(
            toks,
            vec![(SyntaxKind::DATETIME_LITERAL, "2026-03-01T14:30")]
        );
    }

    // r[verify lexer.datetime]
    #[test]
    fn datetime_with_seconds() {
        let toks = kinds("2026-03-01T14:30:00");
        assert_eq!(
            toks,
            vec![(SyntaxKind::DATETIME_LITERAL, "2026-03-01T14:30:00")]
        );
    }

    // r[verify lexer.date]
    #[test]
    fn date_wins_over_integer() {
        let toks = kinds("2026-03-01");
        assert_eq!(toks, vec![(SyntaxKind::DATE_LITERAL, "2026-03-01")]);
    }

    // r[verify lexer.month-day]
    #[test]
    fn month_day_wins_over_integer() {
        let toks = kinds("03-15");
        assert_eq!(toks, vec![(SyntaxKind::MONTH_DAY_LITERAL, "03-15")]);
    }

    // r[verify lexer.time]
    #[test]
    fn time_literal() {
        let toks = kinds("14:30");
        assert_eq!(toks, vec![(SyntaxKind::TIME_LITERAL, "14:30")]);
    }

    // r[verify lexer.time]
    #[test]
    fn time_literal_with_seconds() {
        let toks = kinds("14:30:59");
        assert_eq!(toks, vec![(SyntaxKind::TIME_LITERAL, "14:30:59")]);
    }

    // r[verify lexer.duration]
    #[test]
    fn duration_wins_over_signed_int() {
        let toks = kinds("+5h30m");
        assert_eq!(toks, vec![(SyntaxKind::DURATION_LITERAL, "+5h30m")]);
    }

    // r[verify lexer.duration]
    #[test]
    fn unsigned_duration() {
        let toks = kinds("1h30m");
        assert_eq!(toks, vec![(SyntaxKind::DURATION_LITERAL, "1h30m")]);
    }

    // r[verify lexer.signed-integer]
    #[test]
    fn signed_int_when_no_unit() {
        let toks = kinds("+5");
        assert_eq!(toks, vec![(SyntaxKind::SIGNED_INTEGER_LITERAL, "+5")]);
    }

    // r[verify lexer.signed-integer]
    #[test]
    fn negative_signed_int() {
        let toks = kinds("-42");
        assert_eq!(toks, vec![(SyntaxKind::SIGNED_INTEGER_LITERAL, "-42")]);
    }

    // r[verify syntax.name]
    #[test]
    fn name_token() {
        let toks = kinds("@foo.bar");
        assert_eq!(toks, vec![(SyntaxKind::NAME, "@foo.bar")]);
    }

    // r[verify syntax.name]
    #[test]
    fn name_simple() {
        let toks = kinds("@meeting");
        assert_eq!(toks, vec![(SyntaxKind::NAME, "@meeting")]);
    }

    // ── Strict keywords ──────────────────────────────────────────

    // r[verify lexer.keyword.strict]
    #[test]
    fn strict_keywords() {
        let toks = kinds("true false undefined");
        assert_eq!(
            toks,
            vec![
                (SyntaxKind::TRUE_KW, "true"),
                (SyntaxKind::WHITESPACE, " "),
                (SyntaxKind::FALSE_KW, "false"),
                (SyntaxKind::WHITESPACE, " "),
                (SyntaxKind::UNDEFINED_KW, "undefined"),
            ]
        );
    }

    // ── Weak keywords lex as IDENT ───────────────────────────────

    // r[verify lexer.keyword.weak]
    #[test]
    fn weak_keywords_are_idents() {
        for kw in [
            "calendar",
            "include",
            "bind",
            "override",
            "event",
            "task",
            "every",
            "day",
            "year",
            "on",
            "until",
            "times",
            "omit",
            "forward",
            "backward",
            "monday",
            "tuesday",
            "wednesday",
            "thursday",
            "friday",
            "saturday",
            "sunday",
            "local",
            "import",
            "as",
            "let",
            "in",
            "gnomon",
            "icalendar",
            "jscalendar",
        ] {
            let toks = kinds(kw);
            assert_eq!(toks, vec![(SyntaxKind::IDENT, kw)], "keyword: {kw}");
        }
    }

    // ── Punctuation ──────────────────────────────────────────────

    // r[verify lexer.punctuation]
    #[test]
    fn punctuation() {
        let toks = kinds("{}[]():,=!.+/");
        assert_eq!(
            toks,
            vec![
                (SyntaxKind::L_BRACE, "{"),
                (SyntaxKind::R_BRACE, "}"),
                (SyntaxKind::L_BRACKET, "["),
                (SyntaxKind::R_BRACKET, "]"),
                (SyntaxKind::L_PAREN, "("),
                (SyntaxKind::R_PAREN, ")"),
                (SyntaxKind::COLON, ":"),
                (SyntaxKind::COMMA, ","),
                (SyntaxKind::EQUALS, "="),
                (SyntaxKind::BANG, "!"),
                (SyntaxKind::DOT, "."),
                (SyntaxKind::PLUS, "+"),
                (SyntaxKind::SLASH, "/"),
            ]
        );
    }

    #[test]
    fn multi_char_punctuation() {
        let toks = kinds("== != ++ //");
        assert_eq!(
            toks,
            vec![
                (SyntaxKind::EQ_EQ, "=="),
                (SyntaxKind::WHITESPACE, " "),
                (SyntaxKind::BANG_EQ, "!="),
                (SyntaxKind::WHITESPACE, " "),
                (SyntaxKind::PLUS_PLUS, "++"),
                (SyntaxKind::WHITESPACE, " "),
                (SyntaxKind::SLASH_SLASH, "//"),
            ]
        );
    }

    // ── Path literals ─────────────────────────────────────────────

    #[test]
    fn path_relative_dot() {
        let toks = kinds("./foo.gnomon");
        assert_eq!(toks, vec![(SyntaxKind::PATH_LITERAL, "./foo.gnomon")]);
    }

    #[test]
    fn path_relative_dotdot() {
        let toks = kinds("../bar/baz");
        assert_eq!(toks, vec![(SyntaxKind::PATH_LITERAL, "../bar/baz")]);
    }

    #[test]
    fn path_named() {
        let toks = kinds("lib/core.gnomon");
        assert_eq!(toks, vec![(SyntaxKind::PATH_LITERAL, "lib/core.gnomon")]);
    }

    #[test]
    fn plus_signed_integer_still_works() {
        // +5 should still lex as SIGNED_INTEGER_LITERAL (longest match)
        let toks = kinds("+5");
        assert_eq!(toks, vec![(SyntaxKind::SIGNED_INTEGER_LITERAL, "+5")]);
    }

    // ── Strings ──────────────────────────────────────────────────

    // r[verify lexer.string]
    #[test]
    fn string_literal() {
        let toks = kinds(r#""hello world""#);
        assert_eq!(toks, vec![(SyntaxKind::STRING_LITERAL, r#""hello world""#)]);
    }

    // r[verify lexer.string.escape]
    #[test]
    fn string_with_escapes() {
        let toks = kinds(r#""say \"hi\"""#);
        assert_eq!(toks, vec![(SyntaxKind::STRING_LITERAL, r#""say \"hi\"""#)]);
    }

    // ── Integer ──────────────────────────────────────────────────

    // r[verify lexer.integer]
    #[test]
    fn integer_literal() {
        let toks = kinds("42");
        assert_eq!(toks, vec![(SyntaxKind::INTEGER_LITERAL, "42")]);
    }

    // ── Comments ─────────────────────────────────────────────────

    // r[verify lexer.comment]
    #[test]
    fn comment() {
        let toks = kinds("; this is a comment\nhello");
        assert_eq!(
            toks,
            vec![
                (SyntaxKind::COMMENT, "; this is a comment"),
                (SyntaxKind::WHITESPACE, "\n"),
                (SyntaxKind::IDENT, "hello"),
            ]
        );
    }

    // ── Whitespace ───────────────────────────────────────────────

    // r[verify lexer.whitespace]
    #[test]
    fn whitespace_preserved() {
        let toks = kinds("  \t\n  ");
        assert_eq!(toks, vec![(SyntaxKind::WHITESPACE, "  \t\n  ")]);
    }

    // ── Error tokens ─────────────────────────────────────────────

    #[test]
    fn unrecognized_char() {
        let toks = kinds("~");
        assert_eq!(toks, vec![(SyntaxKind::ERROR, "~")]);
    }

    // ── Identifier with hyphens ──────────────────────────────────

    // r[verify lexer.ident]
    #[test]
    fn identifier_with_hyphens() {
        let toks = kinds("x-custom-field");
        assert_eq!(toks, vec![(SyntaxKind::IDENT, "x-custom-field")]);
    }

    // ── Negative duration ────────────────────────────────────────

    // r[verify lexer.duration]
    #[test]
    fn negative_duration() {
        let toks = kinds("-1w3d");
        assert_eq!(toks, vec![(SyntaxKind::DURATION_LITERAL, "-1w3d")]);
    }

    // ── URI literals ──────────────────────────────────────────────

    // r[verify lexer.uri]
    #[test]
    fn uri_https() {
        let toks = kinds("<https://example.com/path?q=1#frag>");
        assert_eq!(
            toks,
            vec![(
                SyntaxKind::URI_LITERAL,
                "<https://example.com/path?q=1#frag>"
            )]
        );
    }

    // r[verify lexer.uri]
    #[test]
    fn uri_mailto() {
        let toks = kinds("<mailto:user@example.com>");
        assert_eq!(
            toks,
            vec![(SyntaxKind::URI_LITERAL, "<mailto:user@example.com>")]
        );
    }

    // r[verify lexer.uri]
    #[test]
    fn uri_urn() {
        let toks = kinds("<urn:uuid:f81d4fae-7dec-11d0-a765-00a0c91e6bf6>");
        assert_eq!(
            toks,
            vec![(
                SyntaxKind::URI_LITERAL,
                "<urn:uuid:f81d4fae-7dec-11d0-a765-00a0c91e6bf6>"
            )]
        );
    }

    // r[verify lexer.uri]
    #[test]
    fn uri_does_not_swallow_past_close() {
        let toks = kinds("<https://a.com> <https://b.com>");
        assert_eq!(
            toks,
            vec![
                (SyntaxKind::URI_LITERAL, "<https://a.com>"),
                (SyntaxKind::WHITESPACE, " "),
                (SyntaxKind::URI_LITERAL, "<https://b.com>"),
            ]
        );
    }

    // ── Atom literals ─────────────────────────────────────────────

    // r[verify lexer.atom]
    #[test]
    fn atom_simple() {
        let toks = kinds("#confirmed");
        assert_eq!(toks, vec![(SyntaxKind::ATOM_LITERAL, "#confirmed")]);
    }

    // r[verify lexer.atom]
    #[test]
    fn atom_with_hyphens() {
        let toks = kinds("#x-custom");
        assert_eq!(toks, vec![(SyntaxKind::ATOM_LITERAL, "#x-custom")]);
    }

    // r[verify lexer.atom]
    #[test]
    fn atom_no_conflict_with_record() {
        let toks = kinds("status: #confirmed");
        assert_eq!(
            toks,
            vec![
                (SyntaxKind::IDENT, "status"),
                (SyntaxKind::COLON, ":"),
                (SyntaxKind::WHITESPACE, " "),
                (SyntaxKind::ATOM_LITERAL, "#confirmed"),
            ]
        );
    }

    // r[verify lexer.atom]
    #[test]
    fn bare_hash_is_error() {
        let toks = kinds("# ");
        assert_eq!(
            toks,
            vec![(SyntaxKind::ERROR, "#"), (SyntaxKind::WHITESPACE, " "),]
        );
    }

    // ── Complete token sequence ──────────────────────────────────

    #[test]
    fn event_declaration_tokens() {
        let input = r#"event @meeting 2026-03-01T14:30 1h30m "Standup""#;
        let toks = kinds(input);
        assert_eq!(
            toks,
            vec![
                (SyntaxKind::IDENT, "event"),
                (SyntaxKind::WHITESPACE, " "),
                (SyntaxKind::NAME, "@meeting"),
                (SyntaxKind::WHITESPACE, " "),
                (SyntaxKind::DATETIME_LITERAL, "2026-03-01T14:30"),
                (SyntaxKind::WHITESPACE, " "),
                (SyntaxKind::DURATION_LITERAL, "1h30m"),
                (SyntaxKind::WHITESPACE, " "),
                (SyntaxKind::STRING_LITERAL, r#""Standup""#),
            ]
        );
    }
}
