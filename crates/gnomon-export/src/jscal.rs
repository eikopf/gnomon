//! JSCalendar export: ImportValue → jscalendar Group → JSON (RFC 9553).

use gnomon_import::{ImportRecord, ImportValue};
use jscalendar::json::{IntoJson, TryFromJson};
use jscalendar::model::object::Group;
use serde_json::{Map, Value as Json, json};

// ── Public API ───────────────────────────────────────────────

/// Emit a JSCalendar Group JSON string from a calendar record and its entries.
///
/// The `calendar` parameter holds calendar-level properties (uid, title, etc.).
/// The `entries` parameter is the list of event/task records.
///
/// The output is a single JSCalendar Group object containing the entries.
// r[impl model.export.jscalendar.calendar+2]
pub fn emit_jscalendar(
    calendar: &ImportRecord,
    entries: &[ImportValue],
) -> Result<String, String> {
    let group_json = build_group_json(calendar, entries)?;
    let group = Group::<Json>::try_from_json(group_json)
        .map_err(|e| format!("JSCalendar validation error: {e}"))?;
    let rendered: Json = group.into_json();
    serde_json::to_string_pretty(&rendered).map_err(|e| e.to_string())
}

// ── Group builder ────────────────────────────────────────────

/// Known Gnomon calendar fields → JSCalendar Group property names.
const GROUP_FIELDS: &[(&str, &str)] = &[
    ("uid", "uid"),
    ("title", "title"),
    ("description", "description"),
    ("color", "color"),
    ("locale", "locale"),
    ("prod_id", "prodId"),
    ("categories", "categories"),
    ("keywords", "keywords"),
];

fn build_group_json(calendar: &ImportRecord, entries: &[ImportValue]) -> Result<Json, String> {
    let mut obj = Map::new();
    obj.insert("@type".into(), json!("Group"));

    for &(gnomon_key, jscal_key) in GROUP_FIELDS {
        if let Some(value) = calendar.get(gnomon_key) {
            obj.insert(jscal_key.into(), translate_field(gnomon_key, value));
        }
    }

    // Vendor properties on the calendar record.
    emit_vendor_properties(calendar, GROUP_FIELDS, &mut obj);

    // Build entries array.
    let mut jscal_entries = Vec::new();
    for entry in entries {
        let ImportValue::Record(record) = entry else {
            continue;
        };
        jscal_entries.push(build_entry_json(record)?);
    }
    obj.insert("entries".into(), Json::Array(jscal_entries));

    Ok(Json::Object(obj))
}

// ── Entry dispatch ───────────────────────────────────────────

fn build_entry_json(record: &ImportRecord) -> Result<Json, String> {
    let entry_type = record.get("type").and_then(as_str).unwrap_or("event");

    match entry_type {
        // r[impl model.export.jscalendar.event]
        "event" => Ok(build_event_json(record)),
        // r[impl model.export.jscalendar.task]
        "task" => Ok(build_task_json(record)),
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

fn build_event_json(record: &ImportRecord) -> Json {
    let mut obj = Map::new();
    obj.insert("@type".into(), json!("Event"));

    for &(gnomon_key, jscal_key) in EVENT_FIELDS {
        if let Some(value) = record.get(gnomon_key) {
            obj.insert(jscal_key.into(), translate_field(gnomon_key, value));
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

fn build_task_json(record: &ImportRecord) -> Json {
    let mut obj = Map::new();
    obj.insert("@type".into(), json!("Task"));

    for &(gnomon_key, jscal_key) in TASK_FIELDS {
        if let Some(value) = record.get(gnomon_key) {
            obj.insert(jscal_key.into(), translate_field(gnomon_key, value));
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
/// and not `type`/`entries` is emitted as-is into the JSON object.
fn emit_vendor_properties(
    record: &ImportRecord,
    known_fields: &[(&str, &str)],
    obj: &mut Map<String, Json>,
) {
    for (key, value) in record {
        // Skip internal gnomon fields.
        if key == "type" || key == "entries" || key == "name" {
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

    fn make_cal(uid: &str) -> ImportRecord {
        make_record(&[
            ("type", ImportValue::String("calendar".into())),
            ("uid", ImportValue::String(uid.into())),
        ])
    }

    #[test]
    fn emit_single_event_as_group() {
        let cal = make_cal("550e8400-e29b-41d4-a716-446655440000");
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            ("uid", ImportValue::String("a8df6573-0474-496d-8496-033ad45d7fea".into())),
            ("title", ImportValue::String("Standup".into())),
            ("start", make_datetime(2026, 3, 12, 9, 0, 0)),
            ("duration", make_duration(0, 0, 1, 0, 0)),
            ("time_zone", ImportValue::String("America/New_York".into())),
        ]));

        let result = emit_jscalendar(&cal, &[event]).unwrap();
        let parsed: Json = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["@type"], "Group");
        assert_eq!(parsed["uid"], "550e8400-e29b-41d4-a716-446655440000");

        let entries = parsed["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["@type"], "Event");
        assert_eq!(entries[0]["uid"], "a8df6573-0474-496d-8496-033ad45d7fea");
        assert_eq!(entries[0]["title"], "Standup");
        assert_eq!(entries[0]["start"], "2026-03-12T09:00:00");
        assert_eq!(entries[0]["duration"], "PT1H");
        assert_eq!(entries[0]["timeZone"], "America/New_York");
        // `type` should not appear in entry output.
        assert!(entries[0].get("type").is_none());
    }

    #[test]
    fn emit_single_task_as_group() {
        let cal = make_cal("550e8400-e29b-41d4-a716-446655440000");
        let task = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("task".into())),
            ("uid", ImportValue::String("b9ef7684-1585-5a7e-b827-144b66551111".into())),
            ("title", ImportValue::String("Review PR".into())),
            ("due", make_datetime(2026, 3, 15, 17, 0, 0)),
            ("percent_complete", ImportValue::Integer(50)),
            ("progress", ImportValue::String("in-process".into())),
        ]));

        let result = emit_jscalendar(&cal, &[task]).unwrap();
        let parsed: Json = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["@type"], "Group");
        let entries = parsed["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["@type"], "Task");
        assert_eq!(entries[0]["uid"], "b9ef7684-1585-5a7e-b827-144b66551111");
        assert_eq!(entries[0]["due"], "2026-03-15T17:00:00");
        assert_eq!(entries[0]["percentComplete"], 50);
        assert_eq!(entries[0]["progress"], "in-process");
    }

    #[test]
    fn emit_multiple_entries_in_group() {
        let cal = make_cal("550e8400-e29b-41d4-a716-446655440000");
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            ("uid", ImportValue::String("a8df6573-0474-496d-8496-033ad45d7fea".into())),
            ("start", make_datetime(2026, 1, 1, 0, 0, 0)),
        ]));
        let task = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("task".into())),
            ("uid", ImportValue::String("b9ef7684-1585-5a7e-b827-144b66551111".into())),
        ]));

        let result = emit_jscalendar(&cal, &[event, task]).unwrap();
        let parsed: Json = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["@type"], "Group");
        let entries = parsed["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["@type"], "Event");
        assert_eq!(entries[1]["@type"], "Task");
    }

    #[test]
    fn categories_and_keywords_as_maps() {
        let cal = make_cal("550e8400-e29b-41d4-a716-446655440000");
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            ("uid", ImportValue::String("a8df6573-0474-496d-8496-033ad45d7fea".into())),
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

        let entries = parsed["entries"].as_array().unwrap();
        assert_eq!(entries[0]["categories"]["work"], true);
        assert_eq!(entries[0]["categories"]["meeting"], true);
        assert_eq!(entries[0]["keywords"]["important"], true);
    }

    #[test]
    fn vendor_properties_preserved() {
        let cal = make_cal("550e8400-e29b-41d4-a716-446655440000");
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            ("uid", ImportValue::String("a8df6573-0474-496d-8496-033ad45d7fea".into())),
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

        let entries = parsed["entries"].as_array().unwrap();
        assert_eq!(entries[0]["com.example:custom"], "vendor-value");
        assert_eq!(entries[0]["com.example:nested"]["key"], "val");
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
        let cal = make_cal("550e8400-e29b-41d4-a716-446655440000");
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            ("uid", ImportValue::String("a8df6573-0474-496d-8496-033ad45d7fea".into())),
            ("start", make_datetime(2026, 1, 1, 0, 0, 0)),
            ("show_without_time", ImportValue::Bool(true)),
        ]));

        let result = emit_jscalendar(&cal, &[event]).unwrap();
        let parsed: Json = serde_json::from_str(&result).unwrap();

        let entries = parsed["entries"].as_array().unwrap();
        assert_eq!(entries[0]["showWithoutTime"], true);
    }

    #[test]
    fn priority_as_integer() {
        let cal = make_cal("550e8400-e29b-41d4-a716-446655440000");
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            ("uid", ImportValue::String("a8df6573-0474-496d-8496-033ad45d7fea".into())),
            ("start", make_datetime(2026, 1, 1, 0, 0, 0)),
            ("priority", ImportValue::Integer(5)),
        ]));

        let result = emit_jscalendar(&cal, &[event]).unwrap();
        let parsed: Json = serde_json::from_str(&result).unwrap();

        let entries = parsed["entries"].as_array().unwrap();
        assert_eq!(entries[0]["priority"], 5);
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

    #[test]
    fn group_preserves_calendar_title() {
        let cal = make_record(&[
            ("type", ImportValue::String("calendar".into())),
            ("uid", ImportValue::String("550e8400-e29b-41d4-a716-446655440000".into())),
            ("title", ImportValue::String("My Calendar".into())),
        ]);
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            ("uid", ImportValue::String("a8df6573-0474-496d-8496-033ad45d7fea".into())),
            ("start", make_datetime(2026, 1, 1, 0, 0, 0)),
        ]));

        let result = emit_jscalendar(&cal, &[event]).unwrap();
        let parsed: Json = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["@type"], "Group");
        assert_eq!(parsed["title"], "My Calendar");
    }
}
