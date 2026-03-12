//! JSCalendar export: ImportValue → JSON (RFC 9553).

use gnomon_import::{ImportRecord, ImportValue};
use serde_json::{Map, Value as Json, json};

// ── Public API ───────────────────────────────────────────────

/// Emit a JSCalendar JSON string from a calendar record and its entries.
///
/// The `calendar` parameter is the calendar-level properties.
/// The `entries` parameter is the list of event/task records.
///
/// If there is a single entry, the output is a single JSCalendar object.
/// Otherwise, the output is a JSON array of JSCalendar objects.
// r[impl model.export.jscalendar.calendar]
pub fn emit_jscalendar(
    _calendar: &ImportRecord,
    entries: &[ImportValue],
) -> Result<String, String> {
    let objects: Vec<Json> = entries
        .iter()
        .map(|entry| {
            let ImportValue::Record(record) = entry else {
                return Err("entry is not a record".to_string());
            };
            record_to_jscal(record)
        })
        .collect::<Result<Vec<_>, _>>()?;

    let output = if objects.len() == 1 {
        objects.into_iter().next().unwrap()
    } else {
        Json::Array(objects)
    };

    serde_json::to_string_pretty(&output).map_err(|e| e.to_string())
}

// ── Record dispatch ──────────────────────────────────────────

fn record_to_jscal(record: &ImportRecord) -> Result<Json, String> {
    let entry_type = record.get("type").and_then(as_str).unwrap_or("event");

    match entry_type {
        // r[impl model.export.jscalendar.event]
        "event" => Ok(build_event(record)),
        // r[impl model.export.jscalendar.task]
        "task" => Ok(build_task(record)),
        other => Err(format!("unknown entry type: {other}")),
    }
}

// ── Event builder ────────────────────────────────────────────

/// Known Gnomon fields for events → JSCalendar camelCase property names.
const EVENT_FIELDS: &[(&str, &str)] = &[
    ("uid", "uid"),
    ("title", "title"),
    ("description", "description"),
    ("start", "start"),
    ("duration", "duration"),
    ("time_zone", "timeZone"),
    ("status", "status"),
    ("priority", "priority"),
    ("color", "color"),
    ("locale", "locale"),
    ("privacy", "privacy"),
    ("free_busy_status", "freeBusyStatus"),
    ("show_without_time", "showWithoutTime"),
    ("categories", "categories"),
    ("keywords", "keywords"),
];

fn build_event(record: &ImportRecord) -> Json {
    let mut obj = Map::new();
    obj.insert("@type".to_string(), json!("Event"));

    for &(gnomon_key, jscal_key) in EVENT_FIELDS {
        if let Some(value) = record.get(gnomon_key) {
            let json_val = translate_field(gnomon_key, value);
            obj.insert(jscal_key.to_string(), json_val);
        }
    }

    // r[impl model.export.jscalendar.vendor]
    emit_vendor_properties(record, EVENT_FIELDS, &mut obj);

    Json::Object(obj)
}

// ── Task builder ─────────────────────────────────────────────

/// Known Gnomon fields for tasks → JSCalendar camelCase property names.
const TASK_FIELDS: &[(&str, &str)] = &[
    ("uid", "uid"),
    ("title", "title"),
    ("description", "description"),
    ("start", "start"),
    ("due", "due"),
    ("estimated_duration", "estimatedDuration"),
    ("percent_complete", "percentComplete"),
    ("progress", "progress"),
    ("time_zone", "timeZone"),
    ("priority", "priority"),
    ("color", "color"),
    ("locale", "locale"),
    ("privacy", "privacy"),
    ("free_busy_status", "freeBusyStatus"),
    ("show_without_time", "showWithoutTime"),
    ("categories", "categories"),
    ("keywords", "keywords"),
];

fn build_task(record: &ImportRecord) -> Json {
    let mut obj = Map::new();
    obj.insert("@type".to_string(), json!("Task"));

    for &(gnomon_key, jscal_key) in TASK_FIELDS {
        if let Some(value) = record.get(gnomon_key) {
            let json_val = translate_field(gnomon_key, value);
            obj.insert(jscal_key.to_string(), json_val);
        }
    }

    // r[impl model.export.jscalendar.vendor]
    emit_vendor_properties(record, TASK_FIELDS, &mut obj);

    Json::Object(obj)
}

// ── Field translation ────────────────────────────────────────

/// Translate a single Gnomon field value to its JSCalendar JSON representation.
///
/// Special fields (datetime records, duration records, categories/keywords as
/// maps) are handled here; everything else uses the generic value translation.
fn translate_field(gnomon_key: &str, value: &ImportValue) -> Json {
    match gnomon_key {
        // Datetime fields → ISO 8601 local datetime string.
        "start" | "due" => datetime_record_to_json(value),
        // Duration fields → ISO 8601 duration string.
        "duration" | "estimated_duration" => duration_record_to_json(value),
        // Categories and keywords: JSCalendar uses { "key": true } map form.
        "categories" | "keywords" => list_to_jscal_map(value),
        // Everything else: generic translation.
        _ => import_value_to_json(value),
    }
}

/// Translate an ImportValue datetime record to an ISO 8601 local datetime string.
///
/// Input format: `{date: {year, month, day}, time: {hour, minute, second}}`
/// Output: `"2026-03-01T14:30:00"`
fn datetime_record_to_json(value: &ImportValue) -> Json {
    let ImportValue::Record(record) = value else {
        return import_value_to_json(value);
    };

    let date = record.get("date").and_then(as_record);
    let time = record.get("time").and_then(as_record);

    let (Some(date), Some(time)) = (date, time) else {
        return import_value_to_json(value);
    };

    let year = get_u64(date, "year").unwrap_or(0);
    let month = get_u64(date, "month").unwrap_or(1);
    let day = get_u64(date, "day").unwrap_or(1);
    let hour = get_u64(time, "hour").unwrap_or(0);
    let minute = get_u64(time, "minute").unwrap_or(0);
    let second = get_u64(time, "second").unwrap_or(0);

    Json::String(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}"
    ))
}

/// Translate an ImportValue duration record to an ISO 8601 duration string.
///
/// Input format: `{weeks, days, hours, minutes, seconds}`
/// Output: `"P1W"`, `"P1DT2H30M"`, `"PT1H"`, etc.
fn duration_record_to_json(value: &ImportValue) -> Json {
    let ImportValue::Record(record) = value else {
        return import_value_to_json(value);
    };

    let weeks = get_u64(record, "weeks").unwrap_or(0);
    let days = get_u64(record, "days").unwrap_or(0);
    let hours = get_u64(record, "hours").unwrap_or(0);
    let minutes = get_u64(record, "minutes").unwrap_or(0);
    let seconds = get_u64(record, "seconds").unwrap_or(0);

    let mut s = String::from("P");

    if weeks > 0 {
        s.push_str(&format!("{weeks}W"));
    }
    if days > 0 {
        s.push_str(&format!("{days}D"));
    }

    if hours > 0 || minutes > 0 || seconds > 0 {
        s.push('T');
        if hours > 0 {
            s.push_str(&format!("{hours}H"));
        }
        if minutes > 0 {
            s.push_str(&format!("{minutes}M"));
        }
        if seconds > 0 {
            s.push_str(&format!("{seconds}S"));
        }
    }

    // Edge case: zero duration.
    if s == "P" {
        s.push_str("T0S");
    }

    Json::String(s)
}

/// Translate a list of strings to the JSCalendar map form `{ "key": true }`.
///
/// JSCalendar represents categories/keywords as `{ "cat1": true, "cat2": true }`.
fn list_to_jscal_map(value: &ImportValue) -> Json {
    let ImportValue::List(items) = value else {
        return import_value_to_json(value);
    };

    let mut map = Map::new();
    for item in items {
        if let ImportValue::String(s) = item {
            map.insert(s.clone(), Json::Bool(true));
        }
    }
    Json::Object(map)
}

// ── Vendor properties ────────────────────────────────────────

/// Emit vendor (unknown) properties: any record field not in the known set
/// and not `type` is emitted as-is into the JSON object.
fn emit_vendor_properties(
    record: &ImportRecord,
    known_fields: &[(&str, &str)],
    obj: &mut Map<String, Json>,
) {
    for (key, value) in record {
        if key == "type" {
            continue;
        }
        if known_fields
            .iter()
            .any(|&(gnomon_key, _)| gnomon_key == key)
        {
            continue;
        }
        // Unknown field → emit as vendor property.
        obj.insert(key.clone(), import_value_to_json(value));
    }
}

// ── Generic value translation (inverse of translate_json_value) ──

/// Convert an ImportValue to a serde_json::Value recursively.
fn import_value_to_json(value: &ImportValue) -> Json {
    match value {
        ImportValue::String(s) => Json::String(s.clone()),
        ImportValue::Integer(n) => json!(*n),
        ImportValue::SignedInteger(n) => json!(*n),
        ImportValue::Bool(b) => Json::Bool(*b),
        ImportValue::Undefined => Json::Null,
        ImportValue::Record(r) => {
            let mut map = Map::new();
            for (k, v) in r {
                map.insert(k.clone(), import_value_to_json(v));
            }
            Json::Object(map)
        }
        ImportValue::List(items) => Json::Array(items.iter().map(import_value_to_json).collect()),
    }
}

// ── Helpers ──────────────────────────────────────────────────

fn as_str(v: &ImportValue) -> Option<&str> {
    match v {
        ImportValue::String(s) => Some(s),
        _ => None,
    }
}

fn as_record(v: &ImportValue) -> Option<&ImportRecord> {
    match v {
        ImportValue::Record(r) => Some(r),
        _ => None,
    }
}

fn get_u64(record: &ImportRecord, key: &str) -> Option<u64> {
    match record.get(key) {
        Some(ImportValue::Integer(n)) => Some(*n),
        _ => None,
    }
}

// ── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use gnomon_import::ImportRecord;

    fn make_record(fields: &[(&str, ImportValue)]) -> ImportRecord {
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    fn make_datetime(y: u64, mo: u64, d: u64, h: u64, mi: u64, s: u64) -> ImportValue {
        ImportValue::Record(make_record(&[
            (
                "date",
                ImportValue::Record(make_record(&[
                    ("year", ImportValue::Integer(y)),
                    ("month", ImportValue::Integer(mo)),
                    ("day", ImportValue::Integer(d)),
                ])),
            ),
            (
                "time",
                ImportValue::Record(make_record(&[
                    ("hour", ImportValue::Integer(h)),
                    ("minute", ImportValue::Integer(mi)),
                    ("second", ImportValue::Integer(s)),
                ])),
            ),
        ]))
    }

    fn make_duration(w: u64, d: u64, h: u64, m: u64, s: u64) -> ImportValue {
        ImportValue::Record(make_record(&[
            ("weeks", ImportValue::Integer(w)),
            ("days", ImportValue::Integer(d)),
            ("hours", ImportValue::Integer(h)),
            ("minutes", ImportValue::Integer(m)),
            ("seconds", ImportValue::Integer(s)),
        ]))
    }

    #[test]
    fn emit_single_event() {
        let cal = make_record(&[("type", ImportValue::String("calendar".into()))]);
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            ("uid", ImportValue::String("abc-123".into())),
            ("title", ImportValue::String("Standup".into())),
            ("start", make_datetime(2026, 3, 12, 9, 0, 0)),
            ("duration", make_duration(0, 0, 1, 0, 0)),
            ("time_zone", ImportValue::String("America/New_York".into())),
        ]));

        let result = emit_jscalendar(&cal, &[event]).unwrap();
        let parsed: Json = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["@type"], "Event");
        assert_eq!(parsed["uid"], "abc-123");
        assert_eq!(parsed["title"], "Standup");
        assert_eq!(parsed["start"], "2026-03-12T09:00:00");
        assert_eq!(parsed["duration"], "PT1H");
        assert_eq!(parsed["timeZone"], "America/New_York");
        // `type` should not appear in output.
        assert!(parsed.get("type").is_none());
    }

    #[test]
    fn emit_single_task() {
        let cal = make_record(&[("type", ImportValue::String("calendar".into()))]);
        let task = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("task".into())),
            ("uid", ImportValue::String("task-1".into())),
            ("title", ImportValue::String("Review PR".into())),
            ("due", make_datetime(2026, 3, 15, 17, 0, 0)),
            ("percent_complete", ImportValue::Integer(50)),
            ("progress", ImportValue::String("in-process".into())),
        ]));

        let result = emit_jscalendar(&cal, &[task]).unwrap();
        let parsed: Json = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["@type"], "Task");
        assert_eq!(parsed["uid"], "task-1");
        assert_eq!(parsed["due"], "2026-03-15T17:00:00");
        assert_eq!(parsed["percentComplete"], 50);
        assert_eq!(parsed["progress"], "in-process");
    }

    #[test]
    fn emit_multiple_entries_as_array() {
        let cal = make_record(&[("type", ImportValue::String("calendar".into()))]);
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            ("uid", ImportValue::String("e1".into())),
            ("start", make_datetime(2026, 1, 1, 0, 0, 0)),
        ]));
        let task = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("task".into())),
            ("uid", ImportValue::String("t1".into())),
        ]));

        let result = emit_jscalendar(&cal, &[event, task]).unwrap();
        let parsed: Json = serde_json::from_str(&result).unwrap();

        assert!(parsed.is_array());
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["@type"], "Event");
        assert_eq!(arr[1]["@type"], "Task");
    }

    #[test]
    fn categories_and_keywords_as_maps() {
        let cal = make_record(&[("type", ImportValue::String("calendar".into()))]);
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            ("uid", ImportValue::String("e1".into())),
            ("start", make_datetime(2026, 1, 1, 0, 0, 0)),
            (
                "categories",
                ImportValue::List(vec![
                    ImportValue::String("work".into()),
                    ImportValue::String("meeting".into()),
                ]),
            ),
            (
                "keywords",
                ImportValue::List(vec![ImportValue::String("important".into())]),
            ),
        ]));

        let result = emit_jscalendar(&cal, &[event]).unwrap();
        let parsed: Json = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["categories"]["work"], true);
        assert_eq!(parsed["categories"]["meeting"], true);
        assert_eq!(parsed["keywords"]["important"], true);
    }

    #[test]
    fn vendor_properties_preserved() {
        let cal = make_record(&[("type", ImportValue::String("calendar".into()))]);
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            ("uid", ImportValue::String("e1".into())),
            ("start", make_datetime(2026, 1, 1, 0, 0, 0)),
            (
                "com.example:custom",
                ImportValue::String("vendor-value".into()),
            ),
            (
                "com.example:nested",
                ImportValue::Record(make_record(&[("key", ImportValue::String("val".into()))])),
            ),
        ]));

        let result = emit_jscalendar(&cal, &[event]).unwrap();
        let parsed: Json = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["com.example:custom"], "vendor-value");
        assert_eq!(parsed["com.example:nested"]["key"], "val");
    }

    #[test]
    fn duration_formats() {
        // Weeks only.
        assert_eq!(
            duration_record_to_json(&make_duration(2, 0, 0, 0, 0)),
            json!("P2W")
        );
        // Days + time.
        assert_eq!(
            duration_record_to_json(&make_duration(0, 1, 2, 30, 0)),
            json!("P1DT2H30M")
        );
        // Time only.
        assert_eq!(
            duration_record_to_json(&make_duration(0, 0, 0, 45, 0)),
            json!("PT45M")
        );
        // Seconds.
        assert_eq!(
            duration_record_to_json(&make_duration(0, 0, 0, 0, 15)),
            json!("PT15S")
        );
        // Zero duration.
        assert_eq!(
            duration_record_to_json(&make_duration(0, 0, 0, 0, 0)),
            json!("PT0S")
        );
        // Mixed weeks + days + time.
        assert_eq!(
            duration_record_to_json(&make_duration(1, 2, 3, 4, 5)),
            json!("P1W2DT3H4M5S")
        );
    }

    #[test]
    fn datetime_format() {
        assert_eq!(
            datetime_record_to_json(&make_datetime(2026, 3, 1, 14, 30, 0)),
            json!("2026-03-01T14:30:00")
        );
        // Midnight.
        assert_eq!(
            datetime_record_to_json(&make_datetime(2026, 12, 25, 0, 0, 0)),
            json!("2026-12-25T00:00:00")
        );
    }

    #[test]
    fn show_without_time_bool() {
        let cal = make_record(&[("type", ImportValue::String("calendar".into()))]);
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            ("uid", ImportValue::String("e1".into())),
            ("start", make_datetime(2026, 1, 1, 0, 0, 0)),
            ("show_without_time", ImportValue::Bool(true)),
        ]));

        let result = emit_jscalendar(&cal, &[event]).unwrap();
        let parsed: Json = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["showWithoutTime"], true);
    }

    #[test]
    fn priority_as_integer() {
        let cal = make_record(&[("type", ImportValue::String("calendar".into()))]);
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            ("uid", ImportValue::String("e1".into())),
            ("start", make_datetime(2026, 1, 1, 0, 0, 0)),
            ("priority", ImportValue::Integer(5)),
        ]));

        let result = emit_jscalendar(&cal, &[event]).unwrap();
        let parsed: Json = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["priority"], 5);
    }

    #[test]
    fn import_value_to_json_recursive() {
        let value = ImportValue::Record(make_record(&[
            ("str", ImportValue::String("hello".into())),
            ("num", ImportValue::Integer(42)),
            ("neg", ImportValue::SignedInteger(-7)),
            ("flag", ImportValue::Bool(false)),
            ("nil", ImportValue::Undefined),
            (
                "list",
                ImportValue::List(vec![
                    ImportValue::Integer(1),
                    ImportValue::String("two".into()),
                ]),
            ),
        ]));

        let json = import_value_to_json(&value);
        assert_eq!(json["str"], "hello");
        assert_eq!(json["num"], 42);
        assert_eq!(json["neg"], -7);
        assert_eq!(json["flag"], false);
        assert!(json["nil"].is_null());
        assert_eq!(json["list"][0], 1);
        assert_eq!(json["list"][1], "two");
    }
}
