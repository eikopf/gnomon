/// Evaluate a string literal token: strip outer quotes and process escape sequences.
///
/// Recognized escapes: `\"`, `\\`, `\n`, `\t`.
// r[impl lexer.string]
// r[impl lexer.string.escape]
pub fn eval_string(text: &str) -> String {
    // Strip surrounding quotes.
    let inner = &text[1..text.len() - 1];
    process_escapes(inner)
}

/// Evaluate a triple-quoted string literal: strip `"""` delimiters,
/// process escape sequences, and apply auto-dedent.
// r[impl lexer.triple-string]
// r[impl lexer.triple-string.escape]
// r[impl lexer.triple-string.dedent]
// r[impl lexer.triple-string.desugar]
pub fn eval_triple_string(text: &str) -> String {
    // Strip surrounding """ delimiters (3 chars each side).
    let inner = &text[3..text.len() - 3];
    // Process escape sequences first, then dedent.
    let escaped = process_escapes(inner);
    dedent(&escaped)
}

/// Process escape sequences in a string interior.
///
/// Recognized escapes: `\"`, `\\`, `\n`, `\t`.
/// Unknown escapes are preserved literally.
fn process_escapes(inner: &str) -> String {
    let mut result = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('"') => result.push('"'),
                Some('\\') => result.push('\\'),
                Some('n') => result.push('\n'),
                Some('t') => result.push('\t'),
                Some(other) => {
                    // Unknown escape — preserve literally.
                    result.push('\\');
                    result.push(other);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(ch);
        }
    }
    result
}

/// Apply auto-dedent to a multi-line string.
///
/// Algorithm:
/// 1. Split into lines.
/// 2. If the first line is empty, remove it.
/// 3. If the last line is whitespace-only, use its length as baseline and remove it.
/// 4. Otherwise, compute minimum indentation across non-empty lines.
/// 5. Strip that many leading whitespace chars from each line.
/// 6. Join with newlines.
fn dedent(s: &str) -> String {
    let mut lines: Vec<&str> = s.split('\n').collect();

    // 1. If the first line is empty, strip it.
    if let Some(first) = lines.first()
        && first.is_empty()
    {
        lines.remove(0);
    }

    if lines.is_empty() {
        return String::new();
    }

    // 2. If the last line is whitespace-only (including empty), use its indentation as
    //    baseline and remove it.
    let baseline_indent = if let Some(last) = lines.last() {
        if last.chars().all(|c| c == ' ' || c == '\t') {
            let indent = last.len();
            lines.pop();
            Some(indent)
        } else {
            None
        }
    } else {
        None
    };

    // 3. Compute minimum indentation.
    let min_indent = baseline_indent.unwrap_or_else(|| {
        lines
            .iter()
            .filter(|line| !line.is_empty())
            .map(|line| line.len() - line.trim_start().len())
            .min()
            .unwrap_or(0)
    });

    // 4. Strip min_indent characters from each line.
    let dedented: Vec<&str> = lines
        .iter()
        .map(|line| {
            if line.len() >= min_indent {
                &line[min_indent..]
            } else {
                // Empty or shorter-than-indent lines become empty.
                ""
            }
        })
        .collect();

    // 5. Join with newlines.
    dedented.join("\n")
}

/// Parse an integer literal token as `u64`.
// r[impl lexer.integer]
pub fn eval_integer(text: &str) -> Option<u64> {
    text.parse().ok()
}

/// Parse a signed integer literal token as `i64`.
// r[impl lexer.signed-integer]
pub fn eval_signed_integer(text: &str) -> Option<i64> {
    text.parse().ok()
}

/// Strip angle brackets from a URI literal: `<scheme:body>` → `scheme:body`.
// r[impl lexer.uri.desugar]
pub fn eval_uri(text: &str) -> String {
    text[1..text.len() - 1].to_string()
}

/// Strip the `#` prefix from an atom literal: `#confirmed` → `confirmed`.
// r[impl lexer.atom.desugar]
pub fn eval_atom(text: &str) -> String {
    text[1..].to_string()
}

/// Strip the `@` prefix from a name token: `@foo.bar` → `foo.bar`.
// r[impl syntax.name]
pub fn eval_name(text: &str) -> String {
    text[1..].to_string()
}

/// Parsed date components (year, month, day).
pub fn parse_date_components(text: &str) -> Option<(u64, u64, u64)> {
    // Format: YYYY-MM-DD
    let mut parts = text.splitn(3, '-');
    let year = parts.next()?.parse().ok()?;
    let month = parts.next()?.parse().ok()?;
    let day = parts.next()?.parse().ok()?;
    Some((year, month, day))
}

/// Parsed time components (hour, minute, second). Seconds default to 0 if absent.
// r[impl lexer.time.default-second]
pub fn parse_time_components(text: &str) -> Option<(u64, u64, u64)> {
    // Format: HH:MM or HH:MM:SS
    let mut parts = text.splitn(3, ':');
    let hour = parts.next()?.parse().ok()?;
    let minute = parts.next()?.parse().ok()?;
    let second = match parts.next() {
        Some(s) => s.parse().ok()?,
        None => 0,
    };
    Some((hour, minute, second))
}

/// Parsed duration components.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurationParts {
    pub positive: bool,
    pub weeks: u64,
    pub days: u64,
    pub hours: u64,
    pub minutes: u64,
    pub seconds: u64,
}

/// Parse a duration literal into its component parts.
///
/// Format: `[+|-]<int>w<int>d<int>h<int>m<int>s` (each unit optional, at least one present).
// r[impl lexer.duration.part.default]
pub fn parse_duration_components(text: &str) -> Option<DurationParts> {
    let (positive, rest) = match text.as_bytes().first()? {
        b'+' => (true, &text[1..]),
        b'-' => (false, &text[1..]),
        _ => (true, text),
    };

    let mut parts = DurationParts {
        positive,
        weeks: 0,
        days: 0,
        hours: 0,
        minutes: 0,
        seconds: 0,
    };

    let mut num_start = 0;
    for (i, ch) in rest.char_indices() {
        match ch {
            'w' | 'd' | 'h' | 'm' | 's' => {
                let n: u64 = rest[num_start..i].parse().ok()?;
                match ch {
                    'w' => parts.weeks = n,
                    'd' => parts.days = n,
                    'h' => parts.hours = n,
                    'm' => parts.minutes = n,
                    's' => parts.seconds = n,
                    _ => unreachable!(),
                }
                num_start = i + 1;
            }
            '0'..='9' => {}
            _ => return None,
        }
    }

    Some(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── eval_string ──────────────────────────────────────────────

    #[test]
    fn string_simple() {
        assert_eq!(eval_string(r#""hello""#), "hello");
    }

    #[test]
    fn string_empty() {
        assert_eq!(eval_string(r#""""#), "");
    }

    #[test]
    fn string_escaped_quote() {
        assert_eq!(eval_string(r#""say \"hi\"""#), r#"say "hi""#);
    }

    #[test]
    fn string_escaped_backslash() {
        assert_eq!(eval_string(r#""a\\b""#), r"a\b");
    }

    #[test]
    fn string_escaped_newline_and_tab() {
        assert_eq!(eval_string(r#""a\nb\tc""#), "a\nb\tc");
    }

    // ── eval_triple_string ─────────────────────────────────────

    // r[verify lexer.triple-string.desugar]
    #[test]
    fn triple_string_simple() {
        assert_eq!(eval_triple_string(r#""""hello""""#), "hello");
    }

    #[test]
    fn triple_string_empty() {
        assert_eq!(eval_triple_string(r#""""""""#), "");
    }

    // r[verify lexer.triple-string.multiline]
    #[test]
    fn triple_string_multiline() {
        // Content: \nhello\nworld\n
        // After dedent: first empty line stripped, last empty line stripped
        assert_eq!(
            eval_triple_string("\"\"\"\nhello\nworld\n\"\"\""),
            "hello\nworld"
        );
    }

    // r[verify lexer.triple-string.dedent]
    #[test]
    fn triple_string_dedent_with_baseline() {
        // The closing """ is indented 4 spaces, establishing the baseline.
        let input = "\"\"\"\n    hello\n    world\n    \"\"\"";
        assert_eq!(eval_triple_string(input), "hello\nworld");
    }

    // r[verify lexer.triple-string.dedent]
    #[test]
    fn triple_string_dedent_mixed_indent() {
        // Baseline is 4 from closing line; "world" has 8 → becomes 4.
        let input = "\"\"\"\n    hello\n        world\n    \"\"\"";
        assert_eq!(eval_triple_string(input), "hello\n    world");
    }

    // r[verify lexer.triple-string.embedded-quotes]
    #[test]
    fn triple_string_embedded_quotes() {
        assert_eq!(
            eval_triple_string(r#""""he said "hi" ok""""#),
            r#"he said "hi" ok"#
        );
    }

    // r[verify lexer.triple-string.escape]
    #[test]
    fn triple_string_escapes() {
        assert_eq!(eval_triple_string(r#""""a\nb\tc""""#), "a\nb\tc");
    }

    #[test]
    fn triple_string_no_dedent_inline() {
        // Content on same line as delimiters — no first/last line stripping.
        assert_eq!(eval_triple_string(r#""""hello world""""#), "hello world");
    }

    #[test]
    fn triple_string_dedent_without_baseline() {
        // No trailing whitespace-only line, so min-indent is computed.
        let input = "\"\"\"\n  a\n    b\n  c\"\"\"";
        assert_eq!(eval_triple_string(input), "a\n  b\nc");
    }

    #[test]
    fn triple_string_preserves_empty_lines() {
        let input = "\"\"\"\n    hello\n\n    world\n    \"\"\"";
        assert_eq!(eval_triple_string(input), "hello\n\nworld");
    }

    // ── eval_integer / eval_signed_integer ───────────────────────

    #[test]
    fn integer_ok() {
        assert_eq!(eval_integer("42"), Some(42));
    }

    #[test]
    fn integer_zero() {
        assert_eq!(eval_integer("0"), Some(0));
    }

    #[test]
    fn integer_max() {
        assert_eq!(eval_integer("18446744073709551615"), Some(u64::MAX));
    }

    #[test]
    fn signed_integer_positive() {
        assert_eq!(eval_signed_integer("+5"), Some(5));
    }

    #[test]
    fn signed_integer_negative() {
        assert_eq!(eval_signed_integer("-42"), Some(-42));
    }

    // ── eval_uri / eval_atom / eval_name ─────────────────────────

    // r[verify lexer.uri.desugar]
    #[test]
    fn uri_strips_brackets() {
        assert_eq!(eval_uri("<https://example.com>"), "https://example.com");
    }

    // r[verify lexer.atom.desugar]
    #[test]
    fn atom_strips_hash() {
        assert_eq!(eval_atom("#confirmed"), "confirmed");
    }

    #[test]
    fn name_strips_at() {
        assert_eq!(eval_name("@foo.bar"), "foo.bar");
    }

    #[test]
    fn name_simple() {
        assert_eq!(eval_name("@meeting"), "meeting");
    }

    // ── parse_date_components ────────────────────────────────────

    #[test]
    fn date_components() {
        assert_eq!(parse_date_components("2026-03-15"), Some((2026, 3, 15)));
    }

    #[test]
    fn date_components_leading_zeros() {
        assert_eq!(parse_date_components("2026-01-01"), Some((2026, 1, 1)));
    }

    // ── parse_time_components ────────────────────────────────────

    #[test]
    fn time_with_seconds() {
        assert_eq!(parse_time_components("14:30:59"), Some((14, 30, 59)));
    }

    // r[verify lexer.time.default-second]
    #[test]
    fn time_without_seconds() {
        assert_eq!(parse_time_components("14:30"), Some((14, 30, 0)));
    }

    #[test]
    fn time_midnight() {
        assert_eq!(parse_time_components("00:00:00"), Some((0, 0, 0)));
    }

    // ── parse_duration_components ────────────────────────────────

    // r[verify lexer.duration.part.default]
    #[test]
    fn duration_simple() {
        assert_eq!(
            parse_duration_components("1h30m"),
            Some(DurationParts {
                positive: true,
                weeks: 0,
                days: 0,
                hours: 1,
                minutes: 30,
                seconds: 0,
            })
        );
    }

    #[test]
    fn duration_positive_sign() {
        assert_eq!(
            parse_duration_components("+5h"),
            Some(DurationParts {
                positive: true,
                weeks: 0,
                days: 0,
                hours: 5,
                minutes: 0,
                seconds: 0,
            })
        );
    }

    #[test]
    fn duration_negative() {
        assert_eq!(
            parse_duration_components("-1w3d"),
            Some(DurationParts {
                positive: false,
                weeks: 1,
                days: 3,
                hours: 0,
                minutes: 0,
                seconds: 0,
            })
        );
    }

    #[test]
    fn duration_all_units() {
        assert_eq!(
            parse_duration_components("2w3d4h5m6s"),
            Some(DurationParts {
                positive: true,
                weeks: 2,
                days: 3,
                hours: 4,
                minutes: 5,
                seconds: 6,
            })
        );
    }

    #[test]
    fn duration_single_seconds() {
        assert_eq!(
            parse_duration_components("30s"),
            Some(DurationParts {
                positive: true,
                weeks: 0,
                days: 0,
                hours: 0,
                minutes: 0,
                seconds: 30,
            })
        );
    }
}
