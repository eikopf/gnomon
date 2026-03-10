/// Evaluate a string literal token: strip outer quotes and process escape sequences.
///
/// Recognized escapes: `\"`, `\\`, `\n`, `\t`.
// r[impl lexer.string]
// r[impl lexer.string.escape]
pub fn eval_string(text: &str) -> String {
    // Strip surrounding quotes.
    let inner = &text[1..text.len() - 1];
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
