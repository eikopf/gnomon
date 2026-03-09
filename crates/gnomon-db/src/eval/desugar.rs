use std::collections::BTreeMap;

use gnomon_parser::SyntaxKind;

use super::interned::FieldName;
use super::literals;
use super::types::{Blame, Blamed, Record, Value};

/// Desugar a date literal (`YYYY-MM-DD`) into `{ year, month, day }`.
pub fn desugar_date<'db>(
    db: &'db dyn crate::Db,
    text: &str,
    blame: &Blame<'db>,
) -> Option<Value<'db>> {
    let (year, month, day) = literals::parse_date_components(text)?;
    let fields = [
        ("year", Value::Integer(year)),
        ("month", Value::Integer(month)),
        ("day", Value::Integer(day)),
    ];
    Some(Value::Record(make_record(db, &fields, blame)))
}

/// Desugar a month-day literal (`MM-DD`) into `{ month, day }`.
pub fn desugar_month_day<'db>(
    db: &'db dyn crate::Db,
    text: &str,
    blame: &Blame<'db>,
) -> Option<Value<'db>> {
    // Format: MM-DD
    let mut parts = text.splitn(2, '-');
    let month: u64 = parts.next()?.parse().ok()?;
    let day: u64 = parts.next()?.parse().ok()?;
    let fields = [
        ("month", Value::Integer(month)),
        ("day", Value::Integer(day)),
    ];
    Some(Value::Record(make_record(db, &fields, blame)))
}

/// Desugar a time literal (`HH:MM` or `HH:MM:SS`) into `{ hour, minute, second }`.
pub fn desugar_time<'db>(
    db: &'db dyn crate::Db,
    text: &str,
    blame: &Blame<'db>,
) -> Option<Value<'db>> {
    let (hour, minute, second) = literals::parse_time_components(text)?;
    let fields = [
        ("hour", Value::Integer(hour)),
        ("minute", Value::Integer(minute)),
        ("second", Value::Integer(second)),
    ];
    Some(Value::Record(make_record(db, &fields, blame)))
}

/// Desugar a datetime literal (`YYYY-MM-DDTHH:MM:SS`) into `{ date: {..}, time: {..} }`.
pub fn desugar_datetime<'db>(
    db: &'db dyn crate::Db,
    text: &str,
    blame: &Blame<'db>,
) -> Option<Value<'db>> {
    let (date_text, time_text) = text.split_once('T')?;
    let date_value = desugar_date(db, date_text, blame)?;
    let time_value = desugar_time(db, time_text, blame)?;
    let fields = [("date", date_value), ("time", time_value)];
    Some(Value::Record(make_record(db, &fields, blame)))
}

/// Desugar a separate date + time pair (from ShortDt) into `{ date: {..}, time: {..} }`.
pub fn desugar_date_and_time<'db>(
    db: &'db dyn crate::Db,
    date_text: &str,
    time_text: &str,
    blame: &Blame<'db>,
) -> Option<Value<'db>> {
    let date_value = desugar_date(db, date_text, blame)?;
    let time_value = desugar_time(db, time_text, blame)?;
    let fields = [("date", date_value), ("time", time_value)];
    Some(Value::Record(make_record(db, &fields, blame)))
}

/// Desugar a duration literal into `{ weeks, days, hours, minutes, seconds }`.
pub fn desugar_duration<'db>(
    db: &'db dyn crate::Db,
    text: &str,
    blame: &Blame<'db>,
) -> Option<Value<'db>> {
    let parts = literals::parse_duration_components(text)?;
    // The sign is represented as a signed integer wrapping each component,
    // but the spec says omitted units default to 0. Store all as integers;
    // sign is applied uniformly.
    if parts.positive {
        let fields = [
            ("weeks", Value::Integer(parts.weeks)),
            ("days", Value::Integer(parts.days)),
            ("hours", Value::Integer(parts.hours)),
            ("minutes", Value::Integer(parts.minutes)),
            ("seconds", Value::Integer(parts.seconds)),
        ];
        Some(Value::Record(make_record(db, &fields, blame)))
    } else {
        let fields = [
            ("weeks", Value::SignedInteger(-(parts.weeks as i64))),
            ("days", Value::SignedInteger(-(parts.days as i64))),
            ("hours", Value::SignedInteger(-(parts.hours as i64))),
            ("minutes", Value::SignedInteger(-(parts.minutes as i64))),
            ("seconds", Value::SignedInteger(-(parts.seconds as i64))),
        ];
        Some(Value::Record(make_record(db, &fields, blame)))
    }
}

/// Desugar an `every` expression into a recurrence rule record.
pub fn desugar_every<'db>(
    db: &'db dyn crate::Db,
    every: &gnomon_parser::ast::EveryExpr,
    blame: &Blame<'db>,
) -> Option<Value<'db>> {
    let mut fields: Vec<(&str, Value<'db>)> = Vec::new();

    // Determine frequency and additional fields from the subject.
    if every.day_kw().is_some() {
        fields.push(("frequency", Value::String("daily".into())));
    } else if every.year_kw().is_some() {
        fields.push(("frequency", Value::String("yearly".into())));
        if let Some(md_token) = every.month_day() {
            let md_text = md_token.text();
            let mut parts = md_text.splitn(2, '-');
            let month: u64 = parts.next()?.parse().ok()?;
            let day: u64 = parts.next()?.parse().ok()?;
            let year_day = month_day_to_year_day(month, day)?;
            fields.push((
                "by_year_day",
                Value::List(vec![Blamed {
                    value: Value::Integer(year_day),
                    blame: blame.clone(),
                }]),
            ));
        }
    } else if let Some(weekday_token) = every.weekday() {
        fields.push(("frequency", Value::String("weekly".into())));
        let day_name = weekday_to_name(weekday_token.kind());
        // r[impl record.rrule.every.desugar.subject.weekday+2]
        // by_day is a list of N-day records: [{ day: <weekday> }]
        let nday_fields = [("day", Value::String(day_name.into()))];
        let nday_record = make_record(db, &nday_fields, blame);
        fields.push((
            "by_day",
            Value::List(vec![Blamed {
                value: Value::Record(nday_record),
                blame: blame.clone(),
            }]),
        ));
    } else {
        return None;
    }

    // Terminator -> termination field.
    if every.until_kw().is_some() {
        let termination = if let Some(dt_token) = every.until_datetime() {
            desugar_datetime(db, dt_token.text(), blame)?
        } else if let Some(date_token) = every.until_date() {
            // Date treated as datetime with 00:00:00.
            let date_text = date_token.text();
            let datetime_text = format!("{date_text}T00:00:00");
            desugar_datetime(db, &datetime_text, blame)?
        } else if let Some(count_token) = every.until_count() {
            let n: u64 = count_token.text().parse().ok()?;
            Value::Integer(n)
        } else {
            Value::Undefined
        };
        fields.push(("termination", termination));
    }

    Some(Value::Record(make_record(db, &fields, blame)))
}

/// Build a `Record` from a slice of `(field_name_str, value)` pairs.
pub(super) fn make_record<'db>(
    db: &'db dyn crate::Db,
    fields: &[(&str, Value<'db>)],
    blame: &Blame<'db>,
) -> Record<'db> {
    let mut map = BTreeMap::new();
    for (name, value) in fields {
        let field_name = FieldName::new(db, (*name).to_string());
        map.insert(
            field_name,
            Blamed {
                value: value.clone(),
                blame: blame.clone(),
            },
        );
    }
    Record(map)
}

/// Convert a month-day to a day-of-year in a non-leap year.
/// Returns `None` for out-of-range month (must be 1..=12).
fn month_day_to_year_day(month: u64, day: u64) -> Option<u64> {
    const DAYS_BEFORE: [u64; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let index = month.checked_sub(1)?;
    Some(*DAYS_BEFORE.get(index as usize)? + day)
}

/// Map a weekday keyword SyntaxKind to its canonical name string.
fn weekday_to_name(kind: SyntaxKind) -> &'static str {
    match kind {
        SyntaxKind::MONDAY_KW => "monday",
        SyntaxKind::TUESDAY_KW => "tuesday",
        SyntaxKind::WEDNESDAY_KW => "wednesday",
        SyntaxKind::THURSDAY_KW => "thursday",
        SyntaxKind::FRIDAY_KW => "friday",
        SyntaxKind::SATURDAY_KW => "saturday",
        SyntaxKind::SUNDAY_KW => "sunday",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::interned::{DeclId, DeclKind, FieldPath};
    use crate::input::SourceFile;
    use crate::Database;
    use std::path::PathBuf;

    fn test_blame(db: &Database) -> Blame<'_> {
        let source = SourceFile::new(db, PathBuf::from("test.gnomon"), String::new());
        let decl_id = DeclId::new(db, source, 0, DeclKind::Event);
        Blame {
            decl: decl_id,
            path: FieldPath::root(),
        }
    }

    fn get_field<'db>(record: &Record<'db>, db: &'db Database, name: &str) -> Value<'db> {
        let field_name = FieldName::new(db, name.to_string());
        record.get(&field_name).unwrap().value.clone()
    }

    #[test]
    fn date_desugar() {
        let db = Database::default();
        let blame = test_blame(&db);
        let value = desugar_date(&db, "2026-03-15", &blame).unwrap();
        match value {
            Value::Record(r) => {
                assert_eq!(get_field(&r, &db, "year"), Value::Integer(2026));
                assert_eq!(get_field(&r, &db, "month"), Value::Integer(3));
                assert_eq!(get_field(&r, &db, "day"), Value::Integer(15));
            }
            _ => panic!("expected Record"),
        }
    }

    #[test]
    fn month_day_desugar() {
        let db = Database::default();
        let blame = test_blame(&db);
        let value = desugar_month_day(&db, "03-15", &blame).unwrap();
        match value {
            Value::Record(r) => {
                assert_eq!(get_field(&r, &db, "month"), Value::Integer(3));
                assert_eq!(get_field(&r, &db, "day"), Value::Integer(15));
            }
            _ => panic!("expected Record"),
        }
    }

    #[test]
    fn time_desugar() {
        let db = Database::default();
        let blame = test_blame(&db);
        let value = desugar_time(&db, "14:30:59", &blame).unwrap();
        match value {
            Value::Record(r) => {
                assert_eq!(get_field(&r, &db, "hour"), Value::Integer(14));
                assert_eq!(get_field(&r, &db, "minute"), Value::Integer(30));
                assert_eq!(get_field(&r, &db, "second"), Value::Integer(59));
            }
            _ => panic!("expected Record"),
        }
    }

    #[test]
    fn time_desugar_no_seconds() {
        let db = Database::default();
        let blame = test_blame(&db);
        let value = desugar_time(&db, "14:30", &blame).unwrap();
        match value {
            Value::Record(r) => {
                assert_eq!(get_field(&r, &db, "second"), Value::Integer(0));
            }
            _ => panic!("expected Record"),
        }
    }

    #[test]
    fn datetime_desugar() {
        let db = Database::default();
        let blame = test_blame(&db);
        let value = desugar_datetime(&db, "2026-03-15T14:30:00", &blame).unwrap();
        match value {
            Value::Record(r) => {
                // date sub-record
                match get_field(&r, &db, "date") {
                    Value::Record(date) => {
                        assert_eq!(get_field(&date, &db, "year"), Value::Integer(2026));
                    }
                    _ => panic!("expected date Record"),
                }
                // time sub-record
                match get_field(&r, &db, "time") {
                    Value::Record(time) => {
                        assert_eq!(get_field(&time, &db, "hour"), Value::Integer(14));
                    }
                    _ => panic!("expected time Record"),
                }
            }
            _ => panic!("expected Record"),
        }
    }

    #[test]
    fn duration_desugar_positive() {
        let db = Database::default();
        let blame = test_blame(&db);
        let value = desugar_duration(&db, "1h30m", &blame).unwrap();
        match value {
            Value::Record(r) => {
                assert_eq!(get_field(&r, &db, "hours"), Value::Integer(1));
                assert_eq!(get_field(&r, &db, "minutes"), Value::Integer(30));
                assert_eq!(get_field(&r, &db, "weeks"), Value::Integer(0));
            }
            _ => panic!("expected Record"),
        }
    }

    #[test]
    fn duration_desugar_negative() {
        let db = Database::default();
        let blame = test_blame(&db);
        let value = desugar_duration(&db, "-2w3d", &blame).unwrap();
        match value {
            Value::Record(r) => {
                assert_eq!(get_field(&r, &db, "weeks"), Value::SignedInteger(-2));
                assert_eq!(get_field(&r, &db, "days"), Value::SignedInteger(-3));
                assert_eq!(get_field(&r, &db, "hours"), Value::SignedInteger(0));
            }
            _ => panic!("expected Record"),
        }
    }

    #[test]
    fn month_day_to_year_day_jan_1() {
        assert_eq!(month_day_to_year_day(1, 1), Some(1));
    }

    #[test]
    fn month_day_to_year_day_mar_15() {
        // Jan=31, Feb=28, so March 15 = 31+28+15 = 74
        assert_eq!(month_day_to_year_day(3, 15), Some(74));
    }

    #[test]
    fn month_day_to_year_day_dec_31() {
        assert_eq!(month_day_to_year_day(12, 31), Some(365));
    }

    #[test]
    fn month_day_to_year_day_month_zero() {
        assert_eq!(month_day_to_year_day(0, 15), None);
    }

    #[test]
    fn month_day_to_year_day_month_13() {
        assert_eq!(month_day_to_year_day(13, 1), None);
    }

    #[test]
    fn every_day_desugar() {
        let db = Database::default();
        let blame = test_blame(&db);
        let source =
            SourceFile::new(&db, PathBuf::from("t.gnomon"), "event @e { rrule: every day }".into());
        let parse_result = crate::parse(&db, source);
        let tree = parse_result.tree(&db);
        let decl = tree.decls().next().unwrap();
        let event = match decl {
            gnomon_parser::ast::Decl::EventDecl(e) => e,
            _ => panic!("expected event"),
        };
        let body = event.body().unwrap();
        let field = body.fields().next().unwrap();
        let every = match field.value().unwrap() {
            gnomon_parser::ast::Expr::EveryExpr(e) => e,
            _ => panic!("expected every"),
        };

        let value = desugar_every(&db, &every, &blame).unwrap();
        match value {
            Value::Record(r) => {
                assert_eq!(
                    get_field(&r, &db, "frequency"),
                    Value::String("daily".into())
                );
            }
            _ => panic!("expected Record"),
        }
    }

    #[test]
    fn every_weekday_desugar() {
        let db = Database::default();
        let blame = test_blame(&db);
        let source = SourceFile::new(
            &db,
            PathBuf::from("t.gnomon"),
            "event @e { rrule: every monday }".into(),
        );
        let parse_result = crate::parse(&db, source);
        let tree = parse_result.tree(&db);
        let decl = tree.decls().next().unwrap();
        let event = match decl {
            gnomon_parser::ast::Decl::EventDecl(e) => e,
            _ => panic!("expected event"),
        };
        let body = event.body().unwrap();
        let field = body.fields().next().unwrap();
        let every = match field.value().unwrap() {
            gnomon_parser::ast::Expr::EveryExpr(e) => e,
            _ => panic!("expected every"),
        };

        let value = desugar_every(&db, &every, &blame).unwrap();
        match value {
            Value::Record(r) => {
                assert_eq!(
                    get_field(&r, &db, "frequency"),
                    Value::String("weekly".into())
                );
                match get_field(&r, &db, "by_day") {
                    Value::List(items) => {
                        assert_eq!(items.len(), 1);
                        match &items[0].value {
                            Value::Record(nday) => {
                                assert_eq!(
                                    get_field(nday, &db, "day"),
                                    Value::String("monday".into())
                                );
                            }
                            _ => panic!("expected N-day record"),
                        }
                    }
                    _ => panic!("expected list"),
                }
            }
            _ => panic!("expected Record"),
        }
    }

    #[test]
    fn every_year_on_month_day_desugar() {
        let db = Database::default();
        let blame = test_blame(&db);
        let source = SourceFile::new(
            &db,
            PathBuf::from("t.gnomon"),
            "event @e { rrule: every year on 03-15 }".into(),
        );
        let parse_result = crate::parse(&db, source);
        let tree = parse_result.tree(&db);
        let decl = tree.decls().next().unwrap();
        let event = match decl {
            gnomon_parser::ast::Decl::EventDecl(e) => e,
            _ => panic!("expected event"),
        };
        let body = event.body().unwrap();
        let field = body.fields().next().unwrap();
        let every = match field.value().unwrap() {
            gnomon_parser::ast::Expr::EveryExpr(e) => e,
            _ => panic!("expected every"),
        };

        let value = desugar_every(&db, &every, &blame).unwrap();
        match value {
            Value::Record(r) => {
                assert_eq!(
                    get_field(&r, &db, "frequency"),
                    Value::String("yearly".into())
                );
                match get_field(&r, &db, "by_year_day") {
                    Value::List(items) => {
                        assert_eq!(items.len(), 1);
                        // March 15 = day 74 of non-leap year
                        assert_eq!(items[0].value, Value::Integer(74));
                    }
                    _ => panic!("expected list"),
                }
            }
            _ => panic!("expected Record"),
        }
    }
}
