//! Recurrence rule expansion for calendar entries.
//!
//! Converts gnomon-db `Value` records into `gnomon_rrule::RecurrenceRule`,
//! expands occurrences, and stores them back as a list of datetime records.

use super::desugar;
use super::interned::FieldName;
use super::types::{Blame, Blamed, Calendar, Record, Value};
use crate::input::SourceFile;
use crate::queries::{Diagnostic, Severity};

/// Maximum number of occurrences to materialize for infinite rules.
const INFINITE_CAP: usize = 1000;

/// Extract a field value from a record by name.
fn get_field<'db>(
    db: &'db dyn crate::Db,
    record: &Record<'db>,
    name: &str,
) -> Option<Value<'db>> {
    let key = FieldName::new(db, name.to_string());
    record.get(&key).map(|b| b.value.clone())
}

/// Convert a datetime record `{ date: { year, month, day }, time: { hour, minute, second } }`
/// into a `jiff::civil::DateTime`.
fn record_to_datetime(
    db: &dyn crate::Db,
    record: &Record<'_>,
) -> Result<gnomon_rrule::DateTime, String> {
    let date_rec = match get_field(db, record, "date") {
        Some(Value::Record(r)) => r,
        _ => return Err("missing or invalid 'date' sub-record".into()),
    };
    let time_rec = match get_field(db, record, "time") {
        Some(Value::Record(r)) => r,
        _ => return Err("missing or invalid 'time' sub-record".into()),
    };

    let year = match get_field(db, &date_rec, "year") {
        Some(Value::Integer(n)) => n as i16,
        _ => return Err("missing or invalid 'date.year'".into()),
    };
    let month = match get_field(db, &date_rec, "month") {
        Some(Value::Integer(n)) => n as i8,
        _ => return Err("missing or invalid 'date.month'".into()),
    };
    let day = match get_field(db, &date_rec, "day") {
        Some(Value::Integer(n)) => n as i8,
        _ => return Err("missing or invalid 'date.day'".into()),
    };

    let hour = match get_field(db, &time_rec, "hour") {
        Some(Value::Integer(n)) => n as i8,
        _ => return Err("missing or invalid 'time.hour'".into()),
    };
    let minute = match get_field(db, &time_rec, "minute") {
        Some(Value::Integer(n)) => n as i8,
        _ => return Err("missing or invalid 'time.minute'".into()),
    };
    let second = match get_field(db, &time_rec, "second") {
        Some(Value::Integer(n)) => n as i8,
        _ => return Err("missing or invalid 'time.second'".into()),
    };

    let date = jiff::civil::Date::new(year, month, day)
        .map_err(|e| format!("invalid date: {e}"))?;
    let time = jiff::civil::Time::new(hour, minute, second, 0)
        .map_err(|e| format!("invalid time: {e}"))?;
    Ok(jiff::civil::DateTime::from_parts(date, time))
}

/// Parse a frequency string into a `Frequency` enum value.
fn parse_frequency(s: &str) -> Result<gnomon_rrule::Frequency, String> {
    match s {
        "yearly" => Ok(gnomon_rrule::Frequency::Yearly),
        "monthly" => Ok(gnomon_rrule::Frequency::Monthly),
        "weekly" => Ok(gnomon_rrule::Frequency::Weekly),
        "daily" => Ok(gnomon_rrule::Frequency::Daily),
        "hourly" => Ok(gnomon_rrule::Frequency::Hourly),
        "minutely" => Ok(gnomon_rrule::Frequency::Minutely),
        "secondly" => Ok(gnomon_rrule::Frequency::Secondly),
        _ => Err(format!("invalid frequency: \"{s}\"")),
    }
}

/// Parse a weekday string into a `Weekday` enum value.
fn parse_weekday(s: &str) -> Result<gnomon_rrule::Weekday, String> {
    match s {
        "monday" => Ok(gnomon_rrule::Weekday::Monday),
        "tuesday" => Ok(gnomon_rrule::Weekday::Tuesday),
        "wednesday" => Ok(gnomon_rrule::Weekday::Wednesday),
        "thursday" => Ok(gnomon_rrule::Weekday::Thursday),
        "friday" => Ok(gnomon_rrule::Weekday::Friday),
        "saturday" => Ok(gnomon_rrule::Weekday::Saturday),
        "sunday" => Ok(gnomon_rrule::Weekday::Sunday),
        _ => Err(format!("invalid weekday: \"{s}\"")),
    }
}

/// Parse a skip strategy string into a `Skip` enum value.
fn parse_skip(s: &str) -> Result<gnomon_rrule::Skip, String> {
    match s {
        "omit" => Ok(gnomon_rrule::Skip::Omit),
        "backward" => Ok(gnomon_rrule::Skip::Backward),
        "forward" => Ok(gnomon_rrule::Skip::Forward),
        _ => Err(format!("invalid skip strategy: \"{s}\"")),
    }
}

/// Extract a signed integer from a Value, accepting both Integer and SignedInteger.
fn value_to_signed(v: &Value<'_>) -> Option<i64> {
    match v {
        Value::Integer(n) => Some(*n as i64),
        Value::SignedInteger(n) => Some(*n),
        _ => None,
    }
}

/// Convert a recurrence rule record into a `RecurrenceRule`.
fn record_to_rule(
    db: &dyn crate::Db,
    record: &Record<'_>,
) -> Result<gnomon_rrule::RecurrenceRule, String> {
    let frequency = match get_field(db, record, "frequency") {
        Some(Value::String(s)) => parse_frequency(&s)?,
        _ => return Err("missing or invalid 'frequency' field".into()),
    };

    let interval = match get_field(db, record, "interval") {
        Some(Value::Integer(n)) => n as u32,
        None => 1,
        _ => return Err("invalid 'interval' field: expected integer".into()),
    };

    let skip = match get_field(db, record, "skip") {
        Some(Value::String(s)) => parse_skip(&s)?,
        None => gnomon_rrule::Skip::default(),
        _ => return Err("invalid 'skip' field: expected string".into()),
    };

    let week_start = match get_field(db, record, "week_start") {
        Some(Value::String(s)) => parse_weekday(&s)?,
        None => gnomon_rrule::Weekday::Monday,
        _ => return Err("invalid 'week_start' field: expected string".into()),
    };

    let termination = match get_field(db, record, "termination") {
        Some(Value::Integer(n)) => gnomon_rrule::Termination::Count(n),
        Some(Value::Record(r)) => {
            let dt = record_to_datetime(db, &r)
                .map_err(|e| format!("invalid termination datetime: {e}"))?;
            gnomon_rrule::Termination::Until(dt)
        }
        Some(Value::Undefined) | None => gnomon_rrule::Termination::None,
        _ => return Err("invalid 'termination' field".into()),
    };

    let by_day = match get_field(db, record, "by_day") {
        Some(Value::List(items)) => {
            let mut result = Vec::new();
            for item in &items {
                match &item.value {
                    Value::Record(r) => {
                        let day = match get_field(db, r, "day") {
                            Some(Value::String(s)) => parse_weekday(&s)?,
                            _ => return Err("by_day entry missing 'day' field".into()),
                        };
                        let nth = match get_field(db, r, "nth") {
                            Some(Value::SignedInteger(n)) => Some(n as i8),
                            Some(Value::Integer(n)) => Some(n as i8),
                            None => None,
                            _ => return Err("invalid 'nth' in by_day entry".into()),
                        };
                        result.push(gnomon_rrule::NDay { day, nth });
                    }
                    _ => return Err("by_day entry is not a record".into()),
                }
            }
            result
        }
        None => Vec::new(),
        _ => return Err("invalid 'by_day' field: expected list".into()),
    };

    let by_month = match get_field(db, record, "by_month") {
        Some(Value::List(items)) => {
            let mut result = Vec::new();
            for item in &items {
                match &item.value {
                    Value::Record(r) => {
                        let month = match get_field(db, r, "month") {
                            Some(Value::Integer(n)) => n as u8,
                            _ => return Err("by_month entry missing 'month' field".into()),
                        };
                        let leap = match get_field(db, r, "leap") {
                            Some(Value::Bool(b)) => b,
                            None => false,
                            _ => return Err("invalid 'leap' in by_month entry".into()),
                        };
                        result.push(gnomon_rrule::ByMonth { month, leap });
                    }
                    _ => return Err("by_month entry is not a record".into()),
                }
            }
            result
        }
        None => Vec::new(),
        _ => return Err("invalid 'by_month' field: expected list".into()),
    };

    let by_month_day = extract_signed_list(db, record, "by_month_day")?
        .into_iter()
        .map(|n| n as i8)
        .collect();

    let by_year_day = extract_signed_list(db, record, "by_year_day")?
        .into_iter()
        .map(|n| n as i16)
        .collect();

    let by_week_no = extract_signed_list(db, record, "by_week_no")?
        .into_iter()
        .map(|n| n as i8)
        .collect();

    let by_hour = extract_unsigned_list(db, record, "by_hour")?
        .into_iter()
        .map(|n| n as u8)
        .collect();

    let by_minute = extract_unsigned_list(db, record, "by_minute")?
        .into_iter()
        .map(|n| n as u8)
        .collect();

    let by_second = extract_unsigned_list(db, record, "by_second")?
        .into_iter()
        .map(|n| n as u8)
        .collect();

    let by_set_position = extract_signed_list(db, record, "by_set_position")?
        .into_iter()
        .map(|n| n as i32)
        .collect();

    Ok(gnomon_rrule::RecurrenceRule {
        frequency,
        interval,
        skip,
        week_start,
        termination,
        by_day,
        by_month_day,
        by_month,
        by_year_day,
        by_week_no,
        by_hour,
        by_minute,
        by_second,
        by_set_position,
    })
}

/// Extract a list of signed integers from a record field, accepting both Integer and SignedInteger.
fn extract_signed_list(
    db: &dyn crate::Db,
    record: &Record<'_>,
    field: &str,
) -> Result<Vec<i64>, String> {
    match get_field(db, record, field) {
        Some(Value::List(items)) => {
            let mut result = Vec::new();
            for item in &items {
                match value_to_signed(&item.value) {
                    Some(n) => result.push(n),
                    None => return Err(format!("{field} entry is not an integer")),
                }
            }
            Ok(result)
        }
        None => Ok(Vec::new()),
        _ => Err(format!("invalid '{field}' field: expected list")),
    }
}

/// Extract a list of unsigned integers from a record field.
fn extract_unsigned_list(
    db: &dyn crate::Db,
    record: &Record<'_>,
    field: &str,
) -> Result<Vec<u64>, String> {
    match get_field(db, record, field) {
        Some(Value::List(items)) => {
            let mut result = Vec::new();
            for item in &items {
                match &item.value {
                    Value::Integer(n) => result.push(*n),
                    _ => return Err(format!("{field} entry is not an unsigned integer")),
                }
            }
            Ok(result)
        }
        None => Ok(Vec::new()),
        _ => Err(format!("invalid '{field}' field: expected list")),
    }
}

/// Convert a `jiff::civil::DateTime` into a datetime record value.
fn datetime_to_record<'db>(
    db: &'db dyn crate::Db,
    dt: gnomon_rrule::DateTime,
    blame: &Blame<'db>,
) -> Value<'db> {
    let date_fields = [
        ("year", Value::Integer(dt.year() as u64)),
        ("month", Value::Integer(dt.month() as u64)),
        ("day", Value::Integer(dt.day() as u64)),
    ];
    let time_fields = [
        ("hour", Value::Integer(dt.hour() as u64)),
        ("minute", Value::Integer(dt.minute() as u64)),
        ("second", Value::Integer(dt.second() as u64)),
    ];
    let fields = [
        ("date", Value::Record(desugar::make_record(db, &date_fields, blame))),
        ("time", Value::Record(desugar::make_record(db, &time_fields, blame))),
    ];
    Value::Record(desugar::make_record(db, &fields, blame))
}

/// Expand recurrence rules on calendar entries into materialized occurrences.
///
/// For each entry with a `recur` field that is a Record, this function:
/// 1. Extracts the `start` datetime and `recur` rule record
/// 2. Converts them to gnomon-rrule types
/// 3. Expands occurrences
/// 4. Stores the result as an `occurrences` list field on the entry
pub fn expand_entry_recurrences<'db>(
    db: &'db dyn crate::Db,
    calendar: &mut Calendar<'db>,
    _sources: &[SourceFile],
    diagnostics: &mut Vec<Diagnostic>,
    has_errors: &mut bool,
) {
    let recur_key = FieldName::new(db, "recur".to_string());
    let start_key = FieldName::new(db, "start".to_string());
    let occurrences_key = FieldName::new(db, "occurrences".to_string());

    for entry in &mut calendar.entries {
        let recur_record = match entry.value.get(&recur_key) {
            Some(blamed) => match &blamed.value {
                Value::Record(r) => r.clone(),
                _ => continue, // Not a record — shape-check already reported.
            },
            None => continue, // No recur field.
        };

        let source = entry.blame.decl.source(db);

        // Extract start datetime.
        let start_record = match entry.value.get(&start_key) {
            Some(blamed) => match &blamed.value {
                Value::Record(r) => r,
                _ => {
                    *has_errors = true;
                    diagnostics.push(Diagnostic {
                        source,
                        range: rowan::TextRange::default(),
                        severity: Severity::Error,
                        message: "recurrence requires a 'start' datetime record".into(),
                    });
                    continue;
                }
            },
            None => {
                *has_errors = true;
                diagnostics.push(Diagnostic {
                    source,
                    range: rowan::TextRange::default(),
                    severity: Severity::Error,
                    message: "recurrence requires 'start' field".into(),
                });
                continue;
            }
        };

        let dtstart = match record_to_datetime(db, start_record) {
            Ok(dt) => dt,
            Err(e) => {
                *has_errors = true;
                diagnostics.push(Diagnostic {
                    source,
                    range: rowan::TextRange::default(),
                    severity: Severity::Error,
                    message: format!("invalid start for recurrence: {e}"),
                });
                continue;
            }
        };

        let rule = match record_to_rule(db, &recur_record) {
            Ok(r) => r,
            Err(e) => {
                *has_errors = true;
                diagnostics.push(Diagnostic {
                    source,
                    range: rowan::TextRange::default(),
                    severity: Severity::Error,
                    message: format!("invalid recurrence rule: {e}"),
                });
                continue;
            }
        };

        let occurrences = gnomon_rrule::Occurrences::new(rule, dtstart);
        let is_finite = occurrences.is_finite();

        let dates: Vec<gnomon_rrule::DateTime> = if is_finite {
            occurrences.into_iter().collect()
        } else {
            diagnostics.push(Diagnostic {
                source,
                range: rowan::TextRange::default(),
                severity: Severity::Warning,
                message: format!(
                    "infinite recurrence rule (no COUNT or UNTIL); capped at {INFINITE_CAP} occurrences"
                ),
            });
            occurrences.into_iter().take(INFINITE_CAP).collect()
        };

        let blame = &entry.blame;
        let occurrence_values: Vec<Blamed<'db, Value<'db>>> = dates
            .into_iter()
            .map(|dt| Blamed {
                value: datetime_to_record(db, dt, blame),
                blame: blame.clone(),
            })
            .collect();

        entry.value.insert(
            occurrences_key.clone(),
            Blamed {
                value: Value::List(occurrence_values),
                blame: entry.blame.clone(),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::interned::{DeclId, DeclKind, FieldPath};
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

    fn make_datetime_record<'db>(
        db: &'db Database,
        blame: &Blame<'db>,
        year: u64,
        month: u64,
        day: u64,
        hour: u64,
        minute: u64,
        second: u64,
    ) -> Record<'db> {
        let date_fields: Vec<(&str, Value<'db>)> = vec![
            ("year", Value::Integer(year)),
            ("month", Value::Integer(month)),
            ("day", Value::Integer(day)),
        ];
        let time_fields: Vec<(&str, Value<'db>)> = vec![
            ("hour", Value::Integer(hour)),
            ("minute", Value::Integer(minute)),
            ("second", Value::Integer(second)),
        ];
        let fields: Vec<(&str, Value<'db>)> = vec![
            ("date", Value::Record(desugar::make_record(db, &date_fields, blame))),
            ("time", Value::Record(desugar::make_record(db, &time_fields, blame))),
        ];
        desugar::make_record(db, &fields, blame)
    }

    #[test]
    fn datetime_record_conversion() {
        let db = Database::default();
        let blame = test_blame(&db);
        let record = make_datetime_record(&db, &blame, 2026, 3, 15, 14, 30, 0);

        let dt = record_to_datetime(&db, &record).unwrap();
        assert_eq!(dt.year(), 2026);
        assert_eq!(dt.month(), 3);
        assert_eq!(dt.day(), 15);
        assert_eq!(dt.hour(), 14);
        assert_eq!(dt.minute(), 30);
        assert_eq!(dt.second(), 0);

        // Round-trip back to record.
        let value = datetime_to_record(&db, dt, &blame);
        match value {
            Value::Record(r) => {
                let rt_dt = record_to_datetime(&db, &r).unwrap();
                assert_eq!(rt_dt, dt);
            }
            _ => panic!("expected Record"),
        }
    }

    #[test]
    fn simple_daily_rule_conversion() {
        let db = Database::default();
        let blame = test_blame(&db);
        let fields: Vec<(&str, Value<'_>)> = vec![
            ("frequency", Value::String("daily".into())),
        ];
        let record = desugar::make_record(&db, &fields, &blame);

        let rule = record_to_rule(&db, &record).unwrap();
        assert_eq!(rule.frequency, gnomon_rrule::Frequency::Daily);
        assert_eq!(rule.interval, 1);
        assert!(matches!(rule.termination, gnomon_rrule::Termination::None));
    }

    #[test]
    fn weekly_rule_with_by_day() {
        let db = Database::default();
        let blame = test_blame(&db);

        let nday_fields: Vec<(&str, Value<'_>)> = vec![
            ("day", Value::String("monday".into())),
        ];
        let nday_record = desugar::make_record(&db, &nday_fields, &blame);

        let fields: Vec<(&str, Value<'_>)> = vec![
            ("frequency", Value::String("weekly".into())),
            ("by_day", Value::List(vec![Blamed {
                value: Value::Record(nday_record),
                blame: blame.clone(),
            }])),
        ];
        let record = desugar::make_record(&db, &fields, &blame);

        let rule = record_to_rule(&db, &record).unwrap();
        assert_eq!(rule.frequency, gnomon_rrule::Frequency::Weekly);
        assert_eq!(rule.by_day.len(), 1);
        assert_eq!(rule.by_day[0].day, gnomon_rrule::Weekday::Monday);
        assert_eq!(rule.by_day[0].nth, None);
    }

    #[test]
    fn rule_with_count_termination() {
        let db = Database::default();
        let blame = test_blame(&db);
        let fields: Vec<(&str, Value<'_>)> = vec![
            ("frequency", Value::String("daily".into())),
            ("termination", Value::Integer(5)),
        ];
        let record = desugar::make_record(&db, &fields, &blame);

        let rule = record_to_rule(&db, &record).unwrap();
        assert_eq!(rule.termination, gnomon_rrule::Termination::Count(5));
    }

    #[test]
    fn rule_with_until_termination() {
        let db = Database::default();
        let blame = test_blame(&db);
        let until_record = make_datetime_record(&db, &blame, 2026, 1, 10, 0, 0, 0);
        let fields: Vec<(&str, Value<'_>)> = vec![
            ("frequency", Value::String("daily".into())),
            ("termination", Value::Record(until_record)),
        ];
        let record = desugar::make_record(&db, &fields, &blame);

        let rule = record_to_rule(&db, &record).unwrap();
        match rule.termination {
            gnomon_rrule::Termination::Until(dt) => {
                assert_eq!(dt.year(), 2026);
                assert_eq!(dt.month(), 1);
                assert_eq!(dt.day(), 10);
            }
            other => panic!("expected Until, got: {other:?}"),
        }
    }

    #[test]
    fn invalid_frequency_error() {
        let db = Database::default();
        let blame = test_blame(&db);
        let fields: Vec<(&str, Value<'_>)> = vec![
            ("frequency", Value::String("biweekly".into())),
        ];
        let record = desugar::make_record(&db, &fields, &blame);

        let err = record_to_rule(&db, &record).unwrap_err();
        assert!(err.contains("invalid frequency"), "got: {err}");
    }
}
