use std::collections::HashSet;

use rowan::ast::AstNode;

use crate::ast::{self, EventDecl, RecordExpr, TaskDecl};
use crate::syntax_kind::{SyntaxKind, SyntaxNode};

/// A syntactic validation error with a byte range and message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    pub range: rowan::TextRange,
    pub message: String,
}

/// Walk the CST and report syntactic validation errors.
///
/// These are checks on token values and structural well-formedness that go
/// beyond what the parser can enforce. Semantic validation (type checking,
/// name resolution) is not performed here.
pub fn validate_syntax(root: &SyntaxNode) -> Vec<SyntaxError> {
    let mut errors = Vec::new();

    // Token-level checks
    for element in root.descendants_with_tokens() {
        if let Some(token) = element.as_token() {
            match token.kind() {
                // r[lexer.integer.max]
                SyntaxKind::INTEGER_LITERAL => {
                    check_integer(token.text(), token.text_range(), &mut errors);
                }
                // r[lexer.signed-integer.range]
                SyntaxKind::SIGNED_INTEGER_LITERAL => {
                    check_signed_integer(token.text(), token.text_range(), &mut errors);
                }
                // r[lexer.string.no-multiline]
                SyntaxKind::STRING_LITERAL => {
                    check_string_no_multiline(token.text(), token.text_range(), &mut errors);
                }
                // r[lexer.duration.part.multiplicity]
                SyntaxKind::DURATION_LITERAL => {
                    check_duration_units(token.text(), token.text_range(), &mut errors);
                }
                SyntaxKind::DATE_LITERAL => {
                    check_date(token.text(), token.text_range(), &mut errors);
                }
                SyntaxKind::MONTH_DAY_LITERAL => {
                    check_month_day(token.text(), token.text_range(), &mut errors);
                }
                SyntaxKind::TIME_LITERAL => {
                    check_time(token.text(), token.text_range(), &mut errors);
                }
                SyntaxKind::DATETIME_LITERAL => {
                    check_datetime(token.text(), token.text_range(), &mut errors);
                }
                _ => {}
            }
        }
    }

    // AST-level checks
    let file = match ast::SourceFile::cast(root.clone()) {
        Some(f) => f,
        None => return errors,
    };

    for decl in file.decls() {
        match decl {
            ast::Decl::EventDecl(ev) => check_event_decl(&ev, &mut errors),
            ast::Decl::TaskDecl(task) => check_task_decl(&task, &mut errors),
            ast::Decl::CalendarDecl(cal) => {
                if let Some(body) = cal.body() {
                    check_duplicate_keys(&body, &mut errors);
                }
            }
        }
    }

    errors
}

// ── Token-level checks ─────────────────────────────────────────────

fn check_integer(text: &str, range: rowan::TextRange, errors: &mut Vec<SyntaxError>) {
    if text.parse::<u64>().is_err() {
        errors.push(SyntaxError {
            range,
            message: "integer literal overflows u64".into(),
        });
    }
}

fn check_signed_integer(text: &str, range: rowan::TextRange, errors: &mut Vec<SyntaxError>) {
    if text.parse::<i64>().is_err() {
        errors.push(SyntaxError {
            range,
            message: "signed integer literal overflows i64".into(),
        });
    }
}

fn check_string_no_multiline(text: &str, range: rowan::TextRange, errors: &mut Vec<SyntaxError>) {
    // text includes the outer quotes; check interior for bare newlines
    if text.len() >= 2 {
        let interior = &text[1..text.len() - 1];
        if interior.contains('\n') {
            errors.push(SyntaxError {
                range,
                message: "string literal must not span multiple lines".into(),
            });
        }
    }
}

fn check_duration_units(text: &str, range: rowan::TextRange, errors: &mut Vec<SyntaxError>) {
    let mut seen = HashSet::new();
    for ch in text.chars() {
        if matches!(ch, 'w' | 'd' | 'h' | 'm' | 's') {
            if !seen.insert(ch) {
                errors.push(SyntaxError {
                    range,
                    message: format!("duplicate duration unit `{ch}`"),
                });
                return;
            }
        }
    }
}

// r[verify lexer.date.valid]
fn check_date(text: &str, range: rowan::TextRange, errors: &mut Vec<SyntaxError>) {
    // Format: YYYY-MM-DD
    let parts: Vec<&str> = text.split('-').collect();
    if parts.len() == 3 {
        check_month_value(parts[1], range, errors);
        check_day_value(parts[2], range, errors);
        // Calendrical validation: check day is valid for the given month and year
        if let (Ok(year), Ok(month), Ok(day)) = (
            parts[0].parse::<u32>(),
            parts[1].parse::<u32>(),
            parts[2].parse::<u32>(),
        ) {
            if (1..=12).contains(&month) && (1..=31).contains(&day) {
                let max = max_day_in_month(month, is_leap_year(year));
                if day > max {
                    errors.push(SyntaxError {
                        range,
                        message: format!(
                            "day {day} is invalid for month {month:02} (max {max})"
                        ),
                    });
                }
            }
        }
    }
}

// r[verify lexer.month-day.valid]
fn check_month_day(text: &str, range: rowan::TextRange, errors: &mut Vec<SyntaxError>) {
    // Format: MM-DD
    let parts: Vec<&str> = text.split('-').collect();
    if parts.len() == 2 {
        check_month_value(parts[0], range, errors);
        check_day_value(parts[1], range, errors);
        // Calendrical validation: reject days that are impossible for the month.
        // 02-29 is allowed because the year is unknown.
        if let (Ok(month), Ok(day)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
            if (1..=12).contains(&month) && (1..=31).contains(&day) {
                let max = max_day_in_month(month, true); // assume leap year (most permissive)
                if day > max {
                    errors.push(SyntaxError {
                        range,
                        message: format!(
                            "day {day} is invalid for month {month:02} (max {max})"
                        ),
                    });
                }
            }
        }
    }
}

fn is_leap_year(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn max_day_in_month(month: u32, is_leap: bool) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => if is_leap { 29 } else { 28 },
        _ => 31,
    }
}

fn check_time(text: &str, range: rowan::TextRange, errors: &mut Vec<SyntaxError>) {
    // Format: HH:MM or HH:MM:SS
    let parts: Vec<&str> = text.split(':').collect();
    if parts.len() >= 2 {
        check_hour_value(parts[0], range, errors);
        check_minute_value(parts[1], range, errors);
        if parts.len() == 3 {
            check_second_value(parts[2], range, errors);
        }
    }
}

fn check_datetime(text: &str, range: rowan::TextRange, errors: &mut Vec<SyntaxError>) {
    // Format: YYYY-MM-DDTHH:MM or YYYY-MM-DDTHH:MM:SS
    if let Some(t_pos) = text.find('T') {
        check_date(&text[..t_pos], range, errors);
        check_time(&text[t_pos + 1..], range, errors);
    }
}

fn check_month_value(s: &str, range: rowan::TextRange, errors: &mut Vec<SyntaxError>) {
    if let Ok(m) = s.parse::<u32>() {
        if !(1..=12).contains(&m) {
            errors.push(SyntaxError {
                range,
                message: format!("month `{s}` out of range (01..=12)"),
            });
        }
    }
}

fn check_day_value(s: &str, range: rowan::TextRange, errors: &mut Vec<SyntaxError>) {
    if let Ok(d) = s.parse::<u32>() {
        if !(1..=31).contains(&d) {
            errors.push(SyntaxError {
                range,
                message: format!("day `{s}` out of range (01..=31)"),
            });
        }
    }
}

fn check_hour_value(s: &str, range: rowan::TextRange, errors: &mut Vec<SyntaxError>) {
    if let Ok(h) = s.parse::<u32>() {
        if h > 23 {
            errors.push(SyntaxError {
                range,
                message: format!("hour `{s}` out of range (00..=23)"),
            });
        }
    }
}

fn check_minute_value(s: &str, range: rowan::TextRange, errors: &mut Vec<SyntaxError>) {
    if let Ok(m) = s.parse::<u32>() {
        if m > 59 {
            errors.push(SyntaxError {
                range,
                message: format!("minute `{s}` out of range (00..=59)"),
            });
        }
    }
}

fn check_second_value(s: &str, range: rowan::TextRange, errors: &mut Vec<SyntaxError>) {
    if let Ok(sec) = s.parse::<u32>() {
        if sec > 60 {
            errors.push(SyntaxError {
                range,
                message: format!("second `{s}` out of range (00..=60)"),
            });
        }
    }
}

// ── AST-level checks ───────────────────────────────────────────────

// r[expr.record.keys]
fn check_duplicate_keys(record: &RecordExpr, errors: &mut Vec<SyntaxError>) {
    let mut seen = HashSet::new();
    for field in record.fields() {
        if let Some(name_tok) = field.name() {
            let name = name_tok.text().to_string();
            if !seen.insert(name) {
                errors.push(SyntaxError {
                    range: name_tok.text_range(),
                    message: format!("duplicate field `{}`", name_tok.text()),
                });
            }
        }
        // Recurse into nested records
        if let Some(value) = field.value() {
            if let ast::Expr::RecordExpr(nested) = value {
                check_duplicate_keys(&nested, errors);
            }
        }
    }
}

fn check_event_decl(ev: &EventDecl, errors: &mut Vec<SyntaxError>) {
    if let Some(body) = ev.body() {
        check_duplicate_keys(&body, errors);
        // Prefix form: no short-form name token on EventDecl itself
        if ev.name().is_none() {
            check_required_field(&body, "name", "event", errors);
            check_required_field(&body, "start", "event", errors);
        }
    }
}

fn check_task_decl(task: &TaskDecl, errors: &mut Vec<SyntaxError>) {
    if let Some(body) = task.body() {
        check_duplicate_keys(&body, errors);
        // Prefix form: no short-form name token on TaskDecl itself
        if task.name().is_none() {
            check_required_field(&body, "name", "task", errors);
        }
    }
}

fn check_required_field(
    record: &RecordExpr,
    field_name: &str,
    decl_kind: &str,
    errors: &mut Vec<SyntaxError>,
) {
    let has_field = record
        .fields()
        .any(|f| f.name().is_some_and(|n| n.text() == field_name));
    if !has_field {
        errors.push(SyntaxError {
            range: record.syntax().text_range(),
            message: format!("{decl_kind} record is missing required field `{field_name}`"),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    fn validate(input: &str) -> Vec<SyntaxError> {
        let p = parse(input);
        validate_syntax(&p.syntax())
    }

    // ── Integer overflow ─────────────────────────────────────────

    #[test]
    fn integer_overflow() {
        let errs = validate("calendar { count: 99999999999999999999999 }");
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("overflows u64"));
    }

    #[test]
    fn integer_valid() {
        let errs = validate("calendar { count: 42 }");
        assert!(errs.is_empty());
    }

    // ── Signed integer overflow ──────────────────────────────────

    #[test]
    fn signed_integer_overflow() {
        let errs = validate("calendar { offset: -99999999999999999999999 }");
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("overflows i64"));
    }

    #[test]
    fn signed_integer_valid() {
        let errs = validate("calendar { offset: -42 }");
        assert!(errs.is_empty());
    }

    // ── String multiline ─────────────────────────────────────────

    #[test]
    fn string_multiline() {
        let errs = validate("calendar { uid: \"line1\nline2\" }");
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("must not span multiple lines"));
    }

    #[test]
    fn string_single_line() {
        let errs = validate(r#"calendar { uid: "hello" }"#);
        assert!(errs.is_empty());
    }

    // ── Duration unit multiplicity ───────────────────────────────

    #[test]
    fn duration_duplicate_unit() {
        let errs = validate("event @e 2026-01-01T00:00 1h2h \"x\"");
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("duplicate duration unit"));
    }

    #[test]
    fn duration_valid() {
        let errs = validate("event @e 2026-01-01T00:00 1h30m \"x\"");
        assert!(errs.is_empty());
    }

    // ── Date/time range checks ───────────────────────────────────

    #[test]
    fn date_month_out_of_range() {
        let errs = validate("event @e 2026-13-01T00:00 1h \"x\"");
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("month"));
    }

    #[test]
    fn date_day_out_of_range() {
        let errs = validate("event @e 2026-01-32T00:00 1h \"x\"");
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("day"));
    }

    #[test]
    fn time_hour_out_of_range() {
        let errs = validate("event @e 2026-01-01T25:00 1h \"x\"");
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("hour"));
    }

    #[test]
    fn time_minute_out_of_range() {
        let errs = validate("event @e 2026-01-01T00:60 1h \"x\"");
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("minute"));
    }

    #[test]
    fn month_day_out_of_range() {
        let errs = validate(
            "event { name: @e, start: 2026-01-01T00:00, recurrence: every year on 13-01 }",
        );
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("month"));
    }

    #[test]
    fn date_valid() {
        let errs = validate("event @e 2026-12-31T23:59 1h \"x\"");
        assert!(errs.is_empty());
    }

    // ── Calendrical date validation ──────────────────────────────

    #[test]
    fn date_feb_29_leap_year_valid() {
        let errs = validate("event @e 2024-02-29T00:00 1h \"x\"");
        assert!(errs.is_empty());
    }

    #[test]
    fn date_feb_29_non_leap_year_invalid() {
        let errs = validate("event @e 2023-02-29T00:00 1h \"x\"");
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("day 29 is invalid for month 02"));
    }

    #[test]
    fn date_apr_31_invalid() {
        let errs = validate("event @e 2024-04-31T00:00 1h \"x\"");
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("day 31 is invalid for month 04"));
    }

    #[test]
    fn date_feb_30_invalid() {
        let errs = validate("event @e 2024-02-30T00:00 1h \"x\"");
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("day 30 is invalid for month 02"));
    }

    #[test]
    fn month_day_feb_30_invalid() {
        let errs = validate(
            "event { name: @e, start: 2026-01-01T00:00, recurrence: every year on 02-30 }",
        );
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("day 30 is invalid for month 02"));
    }

    #[test]
    fn month_day_feb_29_valid() {
        // No year context, so 02-29 must be allowed
        let errs = validate(
            "event { name: @e, start: 2026-01-01T00:00, recurrence: every year on 02-29 }",
        );
        assert!(errs.is_empty());
    }

    // ── Duplicate record keys ────────────────────────────────────

    #[test]
    fn duplicate_keys() {
        let errs = validate("calendar { uid: \"a\", uid: \"b\" }");
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("duplicate field `uid`"));
    }

    #[test]
    fn no_duplicate_keys() {
        let errs = validate("calendar { uid: \"a\", name: \"b\" }");
        assert!(errs.is_empty());
    }

    // ── Required fields (event prefix form) ──────────────────────

    #[test]
    fn event_missing_name() {
        let errs = validate("event { start: 2026-01-01T00:00 }");
        assert!(
            errs.iter()
                .any(|e| e.message.contains("missing required field `name`"))
        );
    }

    #[test]
    fn event_missing_start() {
        let errs = validate("event { name: @e }");
        assert!(
            errs.iter()
                .any(|e| e.message.contains("missing required field `start`"))
        );
    }

    #[test]
    fn event_prefix_complete() {
        let errs = validate("event { name: @e, start: 2026-01-01T00:00 }");
        assert!(errs.is_empty());
    }

    #[test]
    fn event_short_form_no_required_check() {
        // Short form has structural guarantees, should not trigger required field errors
        let errs = validate("event @e 2026-01-01T00:00 1h \"title\"");
        assert!(errs.is_empty());
    }

    // ── Required fields (task prefix form) ───────────────────────

    #[test]
    fn task_missing_name() {
        let errs = validate("task { priority: 1 }");
        assert!(
            errs.iter()
                .any(|e| e.message.contains("missing required field `name`"))
        );
    }

    #[test]
    fn task_prefix_complete() {
        let errs = validate("task { name: @t }");
        assert!(errs.is_empty());
    }

    #[test]
    fn task_short_form_no_required_check() {
        let errs = validate(r#"task @t "title""#);
        assert!(errs.is_empty());
    }
}
