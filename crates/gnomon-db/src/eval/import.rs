//! Translation of foreign calendar formats into Gnomon values.
//!
//! Supports iCalendar (RFC 5545) via `calico` and JSCalendar (RFC 8984) via `serde_json`.

use calico::model::component::{Calendar as ICalCalendar, CalendarComponent};
use calico::model::primitive::{
    DateTimeOrDate, Duration, NominalDuration, Sign, SignedDuration, Status,
};

use super::desugar::make_record;
use super::interned::FieldName;
use super::types::{Blame, Blamed, Record, Value};

// ── iCalendar ────────────────────────────────────────────────

/// Translate an iCalendar string into a Gnomon `Value::List` of records.
pub fn translate_icalendar<'db>(
    db: &'db dyn crate::Db,
    content: &str,
    blame: &Blame<'db>,
) -> Result<Value<'db>, String> {
    let calendars = ICalCalendar::parse(content).map_err(|e| format!("iCalendar parse error: {e}"))?;

    let mut records: Vec<Blamed<'db, Value<'db>>> = Vec::new();

    for cal in &calendars {
        for component in cal.components() {
            match component {
                CalendarComponent::Event(event) => {
                    let mut fields: Vec<(&str, Value<'db>)> = Vec::new();
                    fields.push(("type", Value::String("event".into())));

                    if let Some(uid_prop) = event.uid() {
                        fields.push(("uid", Value::String(uid_prop.value.as_str().to_string())));
                    }
                    if let Some(summary) = event.summary() {
                        fields.push(("title", Value::String(summary.value.clone())));
                    }
                    if let Some(desc) = event.description() {
                        fields.push(("description", Value::String(desc.value.clone())));
                    }
                    if let Some(dtstart) = event.dtstart() {
                        if let Some(val) = translate_datetime_or_date(db, &dtstart.value, blame) {
                            fields.push(("start", val));
                        }
                        if let Some(tz) = dtstart.params.tz_id() {
                            fields.push(("time_zone", Value::String(tz.as_str().to_string())));
                        }
                    }
                    if let Some(dur) = event.duration() {
                        if let Some(val) = translate_signed_duration(db, &dur.value, blame) {
                            fields.push(("duration", val));
                        }
                    } else if let (Some(dtstart), Some(dtend)) =
                        (event.dtstart(), event.dtend())
                    {
                        if let Some(val) =
                            compute_duration_from_endpoints(db, &dtstart.value, &dtend.value, blame)
                        {
                            fields.push(("duration", val));
                        }
                    }
                    if let Some(status_prop) = event.status() {
                        fields.push(("status", translate_status(&status_prop.value)));
                    }
                    if let Some(priority_prop) = event.priority() {
                        fields.push(("priority", Value::Integer(priority_to_u64(&priority_prop.value))));
                    }
                    if let Some(loc) = event.location() {
                        fields.push(("location", Value::String(loc.value.clone())));
                    }
                    if let Some(color) = event.color() {
                        fields.push(("color", Value::String(color.value.to_string())));
                    }
                    if let Some(cats) = event.categories() {
                        let all_cats: Vec<Blamed<'db, Value<'db>>> = cats
                            .iter()
                            .flat_map(|c| c.value.iter())
                            .map(|s: &String| Blamed {
                                value: Value::String(s.clone()),
                                blame: blame.clone(),
                            })
                            .collect();
                        if !all_cats.is_empty() {
                            fields.push(("categories", Value::List(all_cats)));
                        }
                    }

                    let record = make_record(db, &fields, blame);

                    records.push(Blamed {
                        value: Value::Record(record),
                        blame: blame.clone(),
                    });
                }
                CalendarComponent::Todo(todo) => {
                    let mut fields: Vec<(&str, Value<'db>)> = Vec::new();
                    fields.push(("type", Value::String("task".into())));

                    if let Some(uid_prop) = todo.uid() {
                        fields.push(("uid", Value::String(uid_prop.value.as_str().to_string())));
                    }
                    if let Some(summary) = todo.summary() {
                        fields.push(("title", Value::String(summary.value.clone())));
                    }
                    if let Some(desc) = todo.description() {
                        fields.push(("description", Value::String(desc.value.clone())));
                    }
                    if let Some(due_prop) = todo.due() {
                        if let Some(val) = translate_datetime_or_date(db, &due_prop.value, blame) {
                            fields.push(("due", val));
                        }
                    }
                    if let Some(dtstart) = todo.dtstart() {
                        if let Some(val) = translate_datetime_or_date(db, &dtstart.value, blame) {
                            fields.push(("start", val));
                        }
                        if let Some(tz) = dtstart.params.tz_id() {
                            fields.push(("time_zone", Value::String(tz.as_str().to_string())));
                        }
                    }
                    if let Some(dur) = todo.duration() {
                        if let Some(val) = translate_signed_duration(db, &dur.value, blame) {
                            fields.push(("estimated_duration", val));
                        }
                    }
                    if let Some(pct) = todo.percent_complete() {
                        fields.push((
                            "percent_complete",
                            Value::Integer(pct.value.get() as u64),
                        ));
                    }
                    if let Some(status_prop) = todo.status() {
                        fields.push(("status", translate_status(&status_prop.value)));
                    }
                    if let Some(priority_prop) = todo.priority() {
                        fields.push(("priority", Value::Integer(priority_to_u64(&priority_prop.value))));
                    }
                    if let Some(loc) = todo.location() {
                        fields.push(("location", Value::String(loc.value.clone())));
                    }
                    if let Some(color) = todo.color() {
                        fields.push(("color", Value::String(color.value.to_string())));
                    }
                    if let Some(cats) = todo.categories() {
                        let all_cats: Vec<Blamed<'db, Value<'db>>> = cats
                            .iter()
                            .flat_map(|c| c.value.iter())
                            .map(|s: &String| Blamed {
                                value: Value::String(s.clone()),
                                blame: blame.clone(),
                            })
                            .collect();
                        if !all_cats.is_empty() {
                            fields.push(("categories", Value::List(all_cats)));
                        }
                    }

                    let record = make_record(db, &fields, blame);

                    records.push(Blamed {
                        value: Value::Record(record),
                        blame: blame.clone(),
                    });
                }
                // Skip VJOURNAL, VFREEBUSY, VTIMEZONE, etc.
                _ => {}
            }
        }
    }

    Ok(Value::List(records))
}

// ── JSCalendar ───────────────────────────────────────────────

/// Translate a JSCalendar JSON string into a Gnomon value.
///
/// A single JSCalendar object produces `Value::Record`; an array produces `Value::List`.
pub fn translate_jscalendar<'db>(
    db: &'db dyn crate::Db,
    content: &str,
    blame: &Blame<'db>,
) -> Result<Value<'db>, String> {
    let json: serde_json::Value =
        serde_json::from_str(content).map_err(|e| format!("JSCalendar JSON parse error: {e}"))?;

    match &json {
        serde_json::Value::Array(arr) => {
            let items: Vec<Blamed<'db, Value<'db>>> = arr
                .iter()
                .map(|v| Blamed {
                    value: translate_json_value(db, v, blame),
                    blame: blame.clone(),
                })
                .collect();
            Ok(Value::List(items))
        }
        serde_json::Value::Object(_) => Ok(translate_json_object(db, &json, blame)),
        _ => Err("JSCalendar: expected a JSON object or array at top level".into()),
    }
}

/// Translate a JSON object into a Gnomon record, mapping JSCalendar field names
/// to Gnomon field names where applicable.
fn translate_json_object<'db>(
    db: &'db dyn crate::Db,
    json: &serde_json::Value,
    blame: &Blame<'db>,
) -> Value<'db> {
    let obj = match json.as_object() {
        Some(o) => o,
        None => return translate_json_value(db, json, blame),
    };

    let mut record = Record::new();

    for (key, val) in obj {
        let gnomon_key = map_jscal_field_name(key);
        let gnomon_val = match gnomon_key {
            // `@type` → `type`, lowercased
            "type" => {
                let type_str = val.as_str().unwrap_or("unknown");
                match type_str {
                    "Event" | "jsevent" => Value::String("event".into()),
                    "Task" | "jstask" => Value::String("task".into()),
                    _ => Value::String(type_str.to_lowercase()),
                }
            }
            // datetime strings → desugared records
            "start" | "due" => match val.as_str() {
                Some(s) => parse_jscal_datetime(db, s, blame).unwrap_or(Value::String(s.into())),
                None => translate_json_value(db, val, blame),
            },
            // duration strings → desugared records
            "duration" | "estimated_duration" => match val.as_str() {
                Some(s) => parse_jscal_duration(db, s, blame).unwrap_or(Value::String(s.into())),
                None => translate_json_value(db, val, blame),
            },
            // priority: JSCalendar uses 0-9 integers
            "priority" => match val.as_u64() {
                Some(n) => Value::Integer(n),
                None => translate_json_value(db, val, blame),
            },
            // percent_complete: JSCalendar uses 0-100 integers
            "percent_complete" => match val.as_u64() {
                Some(n) => Value::Integer(n),
                None => translate_json_value(db, val, blame),
            },
            // Default: recursively translate
            _ => translate_json_value(db, val, blame),
        };

        let field_name = FieldName::new(db, gnomon_key.to_string());
        record.insert(
            field_name,
            Blamed {
                value: gnomon_val,
                blame: blame.clone(),
            },
        );
    }

    Value::Record(record)
}

/// Recursively translate a JSON value into a Gnomon value.
fn translate_json_value<'db>(
    db: &'db dyn crate::Db,
    val: &serde_json::Value,
    blame: &Blame<'db>,
) -> Value<'db> {
    match val {
        serde_json::Value::Null => Value::Undefined,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(u) = n.as_u64() {
                Value::Integer(u)
            } else if let Some(i) = n.as_i64() {
                Value::SignedInteger(i)
            } else {
                // Floats: store as string representation
                Value::String(n.to_string())
            }
        }
        serde_json::Value::String(s) => Value::String(s.clone()),
        serde_json::Value::Array(arr) => {
            let items: Vec<Blamed<'db, Value<'db>>> = arr
                .iter()
                .map(|v| Blamed {
                    value: translate_json_value(db, v, blame),
                    blame: blame.clone(),
                })
                .collect();
            Value::List(items)
        }
        serde_json::Value::Object(_) => translate_json_object(db, val, blame),
    }
}

/// Map JSCalendar field names to Gnomon field names.
fn map_jscal_field_name(key: &str) -> &str {
    match key {
        "@type" => "type",
        "uid" => "uid",
        "title" => "title",
        "description" => "description",
        "start" => "start",
        "duration" => "duration",
        "due" => "due",
        "estimatedDuration" => "estimated_duration",
        "percentComplete" => "percent_complete",
        "progress" => "progress",
        "status" => "status",
        "priority" => "priority",
        "timeZone" => "time_zone",
        "categories" => "categories",
        "keywords" => "keywords",
        "color" => "color",
        "locale" => "locale",
        "privacy" => "privacy",
        "freeBusyStatus" => "free_busy_status",
        "showWithoutTime" => "show_without_time",
        "locations" => "locations",
        "virtualLocations" => "virtual_locations",
        "links" => "links",
        "relatedTo" => "related_to",
        "participants" => "participants",
        "alerts" => "alerts",
        "recurrenceRules" => "recur",
        // Pass through unknown keys, converting camelCase to snake_case would be too lossy.
        other => other,
    }
}

// ── Datetime/Duration translation helpers ────────────────────

/// Translate a calico `DateTimeOrDate` into a Gnomon datetime/date record.
fn translate_datetime_or_date<'db>(
    db: &'db dyn crate::Db,
    dtod: &DateTimeOrDate,
    blame: &Blame<'db>,
) -> Option<Value<'db>> {
    match dtod {
        DateTimeOrDate::DateTime(dt) => {
            let date = dt.date;
            let time = dt.time;
            let date_fields = [
                ("year", Value::Integer(date.year().get() as u64)),
                ("month", Value::Integer(date.month().number().get() as u64)),
                ("day", Value::Integer(date.day() as u8 as u64)),
            ];
            let time_fields = [
                ("hour", Value::Integer(time.hour() as u8 as u64)),
                ("minute", Value::Integer(time.minute() as u8 as u64)),
                ("second", Value::Integer(time.second() as u8 as u64)),
            ];
            let date_rec = make_record(db, &date_fields, blame);
            let time_rec = make_record(db, &time_fields, blame);
            let dt_fields = [
                ("date", Value::Record(date_rec)),
                ("time", Value::Record(time_rec)),
            ];
            Some(Value::Record(make_record(db, &dt_fields, blame)))
        }
        DateTimeOrDate::Date(date) => {
            let fields = [
                ("year", Value::Integer(date.year().get() as u64)),
                ("month", Value::Integer(date.month().number().get() as u64)),
                ("day", Value::Integer(date.day() as u8 as u64)),
            ];
            Some(Value::Record(make_record(db, &fields, blame)))
        }
    }
}

/// Translate a calico `SignedDuration` into a Gnomon duration record.
fn translate_signed_duration<'db>(
    db: &'db dyn crate::Db,
    sd: &SignedDuration,
    blame: &Blame<'db>,
) -> Option<Value<'db>> {
    let positive = sd.sign == Sign::Pos;
    match &sd.duration {
        Duration::Nominal(nom) => {
            let exact = nom.exact.as_ref();
            translate_nominal_duration(db, positive, nom, exact, blame)
        }
        Duration::Exact(exact) => {
            if positive {
                let fields = [
                    ("weeks", Value::Integer(0)),
                    ("days", Value::Integer(0)),
                    ("hours", Value::Integer(exact.hours as u64)),
                    ("minutes", Value::Integer(exact.minutes as u64)),
                    ("seconds", Value::Integer(exact.seconds as u64)),
                ];
                Some(Value::Record(make_record(db, &fields, blame)))
            } else {
                let fields = [
                    ("weeks", Value::SignedInteger(0)),
                    ("days", Value::SignedInteger(0)),
                    ("hours", Value::SignedInteger(-(exact.hours as i64))),
                    ("minutes", Value::SignedInteger(-(exact.minutes as i64))),
                    ("seconds", Value::SignedInteger(-(exact.seconds as i64))),
                ];
                Some(Value::Record(make_record(db, &fields, blame)))
            }
        }
    }
}

fn translate_nominal_duration<'db>(
    db: &'db dyn crate::Db,
    positive: bool,
    nom: &NominalDuration,
    exact: Option<&calico::model::primitive::ExactDuration>,
    blame: &Blame<'db>,
) -> Option<Value<'db>> {
    let hours = exact.map_or(0, |e| e.hours as u64);
    let minutes = exact.map_or(0, |e| e.minutes as u64);
    let seconds = exact.map_or(0, |e| e.seconds as u64);

    if positive {
        let fields = [
            ("weeks", Value::Integer(nom.weeks as u64)),
            ("days", Value::Integer(nom.days as u64)),
            ("hours", Value::Integer(hours)),
            ("minutes", Value::Integer(minutes)),
            ("seconds", Value::Integer(seconds)),
        ];
        Some(Value::Record(make_record(db, &fields, blame)))
    } else {
        let fields = [
            ("weeks", Value::SignedInteger(-(nom.weeks as i64))),
            ("days", Value::SignedInteger(-(nom.days as i64))),
            ("hours", Value::SignedInteger(-(hours as i64))),
            ("minutes", Value::SignedInteger(-(minutes as i64))),
            ("seconds", Value::SignedInteger(-(seconds as i64))),
        ];
        Some(Value::Record(make_record(db, &fields, blame)))
    }
}

/// Compute duration = end - start for datetime-only endpoints (date-only falls back to None).
fn compute_duration_from_endpoints<'db>(
    db: &'db dyn crate::Db,
    start: &DateTimeOrDate,
    end: &DateTimeOrDate,
    blame: &Blame<'db>,
) -> Option<Value<'db>> {
    match (start, end) {
        (DateTimeOrDate::DateTime(s), DateTimeOrDate::DateTime(e)) => {
            let s_secs = datetime_to_total_seconds(s);
            let e_secs = datetime_to_total_seconds(e);
            let diff = e_secs.saturating_sub(s_secs);
            let hours = diff / 3600;
            let minutes = (diff % 3600) / 60;
            let seconds = diff % 60;
            let fields = [
                ("weeks", Value::Integer(0)),
                ("days", Value::Integer(0)),
                ("hours", Value::Integer(hours)),
                ("minutes", Value::Integer(minutes)),
                ("seconds", Value::Integer(seconds)),
            ];
            Some(Value::Record(make_record(db, &fields, blame)))
        }
        _ => None,
    }
}

fn datetime_to_total_seconds<M>(dt: &calico::model::primitive::DateTime<M>) -> u64 {
    let date = dt.date;
    // Approximate: just convert to a day count + time seconds.
    let y = date.year().get() as u64;
    let m = date.month().number().get() as u64;
    let d = date.day() as u8 as u64;
    // Rough day count (exact isn't needed — we just want the difference).
    let days = y * 365 + y / 4 + m * 30 + d;
    let time_secs =
        dt.time.hour() as u8 as u64 * 3600
        + dt.time.minute() as u8 as u64 * 60
        + dt.time.second() as u8 as u64;
    days * 86400 + time_secs
}

/// Translate a calico Status to a Gnomon string value.
fn translate_status(status: &Status) -> Value<'static> {
    let s = match status {
        Status::Tentative => "tentative",
        Status::Confirmed => "confirmed",
        Status::Cancelled => "cancelled",
        Status::NeedsAction => "needs-action",
        Status::Completed => "completed",
        Status::InProcess => "in-process",
        Status::Draft => "draft",
        Status::Final => "final",
        _ => "unknown",
    };
    Value::String(s.into())
}

/// Convert a calico Priority to a u64 (0-9).
fn priority_to_u64(p: &calico::model::primitive::Priority) -> u64 {
    use calico::model::primitive::Priority;
    match p {
        Priority::Zero => 0,
        Priority::A1 => 1,
        Priority::A2 => 2,
        Priority::A3 => 3,
        Priority::B1 => 4,
        Priority::B2 => 5,
        Priority::B3 => 6,
        Priority::C1 => 7,
        Priority::C2 => 8,
        Priority::C3 => 9,
    }
}


// ── JSCalendar datetime/duration parsing ─────────────────────

/// Parse a JSCalendar local-datetime string (e.g., "2024-01-15T09:00:00")
/// into a Gnomon datetime record.
fn parse_jscal_datetime<'db>(
    db: &'db dyn crate::Db,
    s: &str,
    blame: &Blame<'db>,
) -> Option<Value<'db>> {
    // JSCalendar uses ISO 8601 local datetimes: YYYY-MM-DDTHH:MM:SS
    let (date_str, time_str) = s.split_once('T')?;
    let date_parts: Vec<&str> = date_str.splitn(3, '-').collect();
    if date_parts.len() != 3 {
        return None;
    }
    let year: u64 = date_parts[0].parse().ok()?;
    let month: u64 = date_parts[1].parse().ok()?;
    let day: u64 = date_parts[2].parse().ok()?;

    let time_parts: Vec<&str> = time_str.splitn(3, ':').collect();
    if time_parts.len() < 2 {
        return None;
    }
    let hour: u64 = time_parts[0].parse().ok()?;
    let minute: u64 = time_parts[1].parse().ok()?;
    let second: u64 = if time_parts.len() > 2 {
        time_parts[2].parse().ok()?
    } else {
        0
    };

    let date_fields = [
        ("year", Value::Integer(year)),
        ("month", Value::Integer(month)),
        ("day", Value::Integer(day)),
    ];
    let time_fields = [
        ("hour", Value::Integer(hour)),
        ("minute", Value::Integer(minute)),
        ("second", Value::Integer(second)),
    ];
    let date_rec = make_record(db, &date_fields, blame);
    let time_rec = make_record(db, &time_fields, blame);
    let fields = [
        ("date", Value::Record(date_rec)),
        ("time", Value::Record(time_rec)),
    ];
    Some(Value::Record(make_record(db, &fields, blame)))
}

/// Parse a JSCalendar duration string (e.g., "PT1H30M", "P1D") into a Gnomon duration record.
fn parse_jscal_duration<'db>(
    db: &'db dyn crate::Db,
    s: &str,
    blame: &Blame<'db>,
) -> Option<Value<'db>> {
    // JSCalendar uses ISO 8601 durations: P[nW] or P[nD][T[nH][nM][nS]]
    let input = s.strip_prefix('P')?;

    let mut weeks: u64 = 0;
    let mut days: u64 = 0;
    let mut hours: u64 = 0;
    let mut minutes: u64 = 0;
    let mut seconds: u64 = 0;

    let (date_part, time_part) = match input.split_once('T') {
        Some((d, t)) => (d, Some(t)),
        None => (input, None),
    };

    // Parse date part.
    if !date_part.is_empty() {
        let mut num_buf = String::new();
        for ch in date_part.chars() {
            if ch.is_ascii_digit() {
                num_buf.push(ch);
            } else {
                let n: u64 = num_buf.parse().ok()?;
                num_buf.clear();
                match ch {
                    'W' => weeks = n,
                    'D' => days = n,
                    _ => return None,
                }
            }
        }
    }

    // Parse time part.
    if let Some(tp) = time_part {
        let mut num_buf = String::new();
        for ch in tp.chars() {
            if ch.is_ascii_digit() {
                num_buf.push(ch);
            } else {
                let n: u64 = num_buf.parse().ok()?;
                num_buf.clear();
                match ch {
                    'H' => hours = n,
                    'M' => minutes = n,
                    'S' => seconds = n,
                    _ => return None,
                }
            }
        }
    }

    let fields = [
        ("weeks", Value::Integer(weeks)),
        ("days", Value::Integer(days)),
        ("hours", Value::Integer(hours)),
        ("minutes", Value::Integer(minutes)),
        ("seconds", Value::Integer(seconds)),
    ];
    Some(Value::Record(make_record(db, &fields, blame)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::interned::{DeclId, DeclKind, FieldPath};
    use crate::input::SourceFile;
    use crate::Database;
    use std::path::PathBuf;

    fn test_blame(db: &Database) -> Blame<'_> {
        let source = SourceFile::new(db, PathBuf::from("test.ics"), String::new());
        let decl_id = DeclId::new(db, source, 0, DeclKind::Expr);
        Blame {
            decl: decl_id,
            path: FieldPath::root(),
        }
    }

    fn get_field<'db>(record: &Record<'db>, db: &'db Database, name: &str) -> Value<'db> {
        let field_name = FieldName::new(db, name.to_string());
        record.get(&field_name).unwrap().value.clone()
    }

    fn has_field<'db>(record: &Record<'db>, db: &'db Database, name: &str) -> bool {
        let field_name = FieldName::new(db, name.to_string());
        record.get(&field_name).is_some()
    }

    // ── iCalendar tests ──────────────────────────────────────

    #[test]
    fn ical_minimal_event() {
        let db = Database::default();
        let blame = test_blame(&db);
        let ics = "\
BEGIN:VCALENDAR\r\n\
VERSION:2.0\r\n\
PRODID:-//Test//Test//EN\r\n\
BEGIN:VEVENT\r\n\
UID:test-uid-123\r\n\
SUMMARY:Team Meeting\r\n\
DTSTART:20260315T140000\r\n\
DURATION:PT1H30M\r\n\
END:VEVENT\r\n\
END:VCALENDAR\r\n";

        let result = translate_icalendar(&db, ics, &blame).unwrap();
        let items = match &result {
            Value::List(items) => items,
            _ => panic!("expected list"),
        };
        assert_eq!(items.len(), 1);
        let rec = match &items[0].value {
            Value::Record(r) => r,
            _ => panic!("expected record"),
        };
        assert_eq!(get_field(rec, &db, "type"), Value::String("event".into()));
        assert_eq!(
            get_field(rec, &db, "uid"),
            Value::String("test-uid-123".into())
        );
        assert_eq!(
            get_field(rec, &db, "title"),
            Value::String("Team Meeting".into())
        );
        assert!(has_field(rec, &db, "start"));
        assert!(has_field(rec, &db, "duration"));

        // Check start datetime structure.
        match get_field(rec, &db, "start") {
            Value::Record(dt) => {
                match get_field(&dt, &db, "date") {
                    Value::Record(d) => {
                        assert_eq!(get_field(&d, &db, "year"), Value::Integer(2026));
                        assert_eq!(get_field(&d, &db, "month"), Value::Integer(3));
                        assert_eq!(get_field(&d, &db, "day"), Value::Integer(15));
                    }
                    _ => panic!("expected date record"),
                }
                match get_field(&dt, &db, "time") {
                    Value::Record(t) => {
                        assert_eq!(get_field(&t, &db, "hour"), Value::Integer(14));
                        assert_eq!(get_field(&t, &db, "minute"), Value::Integer(0));
                    }
                    _ => panic!("expected time record"),
                }
            }
            _ => panic!("expected start record"),
        }

        // Check duration structure.
        match get_field(rec, &db, "duration") {
            Value::Record(dur) => {
                assert_eq!(get_field(&dur, &db, "hours"), Value::Integer(1));
                assert_eq!(get_field(&dur, &db, "minutes"), Value::Integer(30));
            }
            _ => panic!("expected duration record"),
        }
    }

    #[test]
    fn ical_minimal_todo() {
        let db = Database::default();
        let blame = test_blame(&db);
        let ics = "\
BEGIN:VCALENDAR\r\n\
VERSION:2.0\r\n\
PRODID:-//Test//Test//EN\r\n\
BEGIN:VTODO\r\n\
UID:todo-1\r\n\
SUMMARY:Buy groceries\r\n\
DUE:20260320T170000\r\n\
PERCENT-COMPLETE:25\r\n\
END:VTODO\r\n\
END:VCALENDAR\r\n";

        let result = translate_icalendar(&db, ics, &blame).unwrap();
        let items = match &result {
            Value::List(items) => items,
            _ => panic!("expected list"),
        };
        assert_eq!(items.len(), 1);
        let rec = match &items[0].value {
            Value::Record(r) => r,
            _ => panic!("expected record"),
        };
        assert_eq!(get_field(rec, &db, "type"), Value::String("task".into()));
        assert_eq!(get_field(rec, &db, "uid"), Value::String("todo-1".into()));
        assert_eq!(
            get_field(rec, &db, "title"),
            Value::String("Buy groceries".into())
        );
        assert!(has_field(rec, &db, "due"));
        assert_eq!(get_field(rec, &db, "percent_complete"), Value::Integer(25));
    }

    #[test]
    fn ical_parse_error() {
        let db = Database::default();
        let blame = test_blame(&db);
        let result = translate_icalendar(&db, "not valid icalendar", &blame);
        assert!(result.is_err());
    }

    #[test]
    fn ical_event_with_dtend() {
        let db = Database::default();
        let blame = test_blame(&db);
        let ics = "\
BEGIN:VCALENDAR\r\n\
VERSION:2.0\r\n\
PRODID:-//Test//Test//EN\r\n\
BEGIN:VEVENT\r\n\
UID:dtend-test\r\n\
SUMMARY:Meeting\r\n\
DTSTART:20260315T140000\r\n\
DTEND:20260315T153000\r\n\
END:VEVENT\r\n\
END:VCALENDAR\r\n";

        let result = translate_icalendar(&db, ics, &blame).unwrap();
        let items = match &result {
            Value::List(items) => items,
            _ => panic!("expected list"),
        };
        let rec = match &items[0].value {
            Value::Record(r) => r,
            _ => panic!("expected record"),
        };
        // Duration should be computed from DTEND - DTSTART = 1h30m.
        match get_field(rec, &db, "duration") {
            Value::Record(dur) => {
                assert_eq!(get_field(&dur, &db, "hours"), Value::Integer(1));
                assert_eq!(get_field(&dur, &db, "minutes"), Value::Integer(30));
                assert_eq!(get_field(&dur, &db, "seconds"), Value::Integer(0));
            }
            _ => panic!("expected duration record"),
        }
    }

    // ── JSCalendar tests ─────────────────────────────────────

    #[test]
    fn jscal_minimal_event() {
        let db = Database::default();
        let blame = test_blame(&db);
        let json = r#"{
            "@type": "Event",
            "uid": "jscal-uid-1",
            "title": "Lunch",
            "start": "2026-03-15T12:00:00",
            "duration": "PT1H"
        }"#;

        let result = translate_jscalendar(&db, json, &blame).unwrap();
        let rec = match &result {
            Value::Record(r) => r,
            _ => panic!("expected record"),
        };
        assert_eq!(get_field(rec, &db, "type"), Value::String("event".into()));
        assert_eq!(
            get_field(rec, &db, "uid"),
            Value::String("jscal-uid-1".into())
        );
        assert_eq!(
            get_field(rec, &db, "title"),
            Value::String("Lunch".into())
        );
        // start should be desugared to datetime record.
        match get_field(rec, &db, "start") {
            Value::Record(dt) => {
                assert!(has_field(&dt, &db, "date"));
                assert!(has_field(&dt, &db, "time"));
            }
            _ => panic!("expected start record"),
        }
        // duration should be desugared.
        match get_field(rec, &db, "duration") {
            Value::Record(dur) => {
                assert_eq!(get_field(&dur, &db, "hours"), Value::Integer(1));
            }
            _ => panic!("expected duration record"),
        }
    }

    #[test]
    fn jscal_task() {
        let db = Database::default();
        let blame = test_blame(&db);
        let json = r#"{
            "@type": "Task",
            "uid": "task-1",
            "title": "Do laundry",
            "due": "2026-03-20T18:00:00"
        }"#;

        let result = translate_jscalendar(&db, json, &blame).unwrap();
        let rec = match &result {
            Value::Record(r) => r,
            _ => panic!("expected record"),
        };
        assert_eq!(get_field(rec, &db, "type"), Value::String("task".into()));
        assert_eq!(get_field(rec, &db, "uid"), Value::String("task-1".into()));
        assert!(has_field(rec, &db, "due"));
    }

    #[test]
    fn jscal_passthrough_unknown_fields() {
        let db = Database::default();
        let blame = test_blame(&db);
        let json = r#"{
            "@type": "Event",
            "uid": "e1",
            "title": "Test",
            "customField": "custom value",
            "nestedObj": { "a": 1, "b": true }
        }"#;

        let result = translate_jscalendar(&db, json, &blame).unwrap();
        let rec = match &result {
            Value::Record(r) => r,
            _ => panic!("expected record"),
        };
        assert_eq!(
            get_field(rec, &db, "customField"),
            Value::String("custom value".into())
        );
        match get_field(rec, &db, "nestedObj") {
            Value::Record(nested) => {
                assert_eq!(get_field(&nested, &db, "a"), Value::Integer(1));
                assert_eq!(get_field(&nested, &db, "b"), Value::Bool(true));
            }
            _ => panic!("expected nested record"),
        }
    }

    #[test]
    fn jscal_array_of_objects() {
        let db = Database::default();
        let blame = test_blame(&db);
        let json = r#"[
            { "@type": "Event", "uid": "e1", "title": "A" },
            { "@type": "Event", "uid": "e2", "title": "B" }
        ]"#;

        let result = translate_jscalendar(&db, json, &blame).unwrap();
        match &result {
            Value::List(items) => assert_eq!(items.len(), 2),
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn jscal_parse_error() {
        let db = Database::default();
        let blame = test_blame(&db);
        let result = translate_jscalendar(&db, "not json{", &blame);
        assert!(result.is_err());
    }

    // ── Duration parsing tests ───────────────────────────────

    #[test]
    fn jscal_duration_parsing() {
        let db = Database::default();
        let blame = test_blame(&db);

        let val = parse_jscal_duration(&db, "P1W", &blame).unwrap();
        match val {
            Value::Record(r) => {
                assert_eq!(get_field(&r, &db, "weeks"), Value::Integer(1));
                assert_eq!(get_field(&r, &db, "days"), Value::Integer(0));
            }
            _ => panic!("expected record"),
        }

        let val = parse_jscal_duration(&db, "P2DT3H15M", &blame).unwrap();
        match val {
            Value::Record(r) => {
                assert_eq!(get_field(&r, &db, "days"), Value::Integer(2));
                assert_eq!(get_field(&r, &db, "hours"), Value::Integer(3));
                assert_eq!(get_field(&r, &db, "minutes"), Value::Integer(15));
            }
            _ => panic!("expected record"),
        }
    }
}
