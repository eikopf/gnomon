//! JSCalendar export: ImportValue → jscalendar model → JSON (RFC 9553).

use std::collections::HashSet;
use std::str::FromStr;

use gnomon_import::{ImportRecord, ImportValue};
use jscalendar::json::{IntoJson, TryFromJson};
use jscalendar::model::object::{Event, Group, Task, TaskOrEvent};
use jscalendar::model::set::{
    Color, EventStatus, FreeBusyStatus, Percent, Priority, Privacy, TaskProgress,
};
use jscalendar::model::time::{
    Date, DateTime, Day, Duration, ExactDuration, Hour, Local, Minute, Month, NominalDuration,
    Second, Time, Year,
};
use serde_json::{Map, Value as Json};

use calendar_types::set::Token;
use calendar_types::string::Uid;

// ── Public API ───────────────────────────────────────────────

/// Emit a JSCalendar Group JSON string from a calendar record and its entries.
///
/// The `calendar` parameter holds calendar-level properties (uid, title, etc.).
/// The `entries` parameter is the list of event/task records.
/// The `warnings` parameter collects non-fatal errors during entry building.
///
/// The output is a single JSCalendar Group object containing the entries.
// r[impl model.export.jscalendar.calendar+2]
pub fn emit_jscalendar(
    w: &mut impl std::fmt::Write,
    calendar: &ImportRecord,
    entries: &[ImportValue],
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    let group = build_group(calendar, entries, warnings)?;
    let json: Json = group.into_json();
    let s = serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?;
    w.write_str(&s).map_err(|e| e.to_string())
}

// ── Group builder ────────────────────────────────────────────

fn build_group(
    calendar: &ImportRecord,
    entries: &[ImportValue],
    warnings: &mut Vec<String>,
) -> Result<Group<Json>, String> {
    let uid = get_uid(calendar)?;

    let jscal_entries: Vec<TaskOrEvent<Json>> = entries
        .iter()
        .filter_map(|entry| {
            let ImportValue::Record(record) = entry else {
                return None;
            };
            match build_entry(record) {
                Ok(entry) => Some(entry),
                Err(err) => {
                    warnings.push(err);
                    None
                }
            }
        })
        .collect();

    let mut group = Group::new(jscal_entries, uid);

    if let Some(s) = get_str(calendar, "title") {
        group.set_title(s.to_string());
    }
    if let Some(s) = get_str(calendar, "description") {
        group.set_description(s.to_string());
    }
    if let Some(s) = get_str(calendar, "prod_id") {
        group.set_prod_id(s.to_string());
    }
    if let Some(s) = get_str(calendar, "color")
        && let Ok(c) = Color::try_from_json(Json::String(s.to_string()))
    {
        group.set_color(c);
    }
    if let Some(s) = get_str(calendar, "locale")
        && let Ok(l) = calendar_types::string::LanguageTag::parse(s)
    {
        group.set_locale(l);
    }
    if let Some(set) = get_string_set(calendar, "categories") {
        group.set_categories(set);
    }
    if let Some(set) = get_string_set(calendar, "keywords") {
        group.set_keywords(set);
    }

    // Vendor properties.
    let vendor = collect_vendor_properties(calendar, GROUP_KNOWN);
    for (k, v) in vendor {
        group.insert_vendor_property(k.into(), v);
    }

    Ok(group)
}

const GROUP_KNOWN: &[&str] = &[
    "type",
    "entries",
    "name",
    "uid",
    "title",
    "description",
    "prod_id",
    "color",
    "locale",
    "categories",
    "keywords",
];

// ── Entry dispatch ───────────────────────────────────────────

fn build_entry(record: &ImportRecord) -> Result<TaskOrEvent<Json>, String> {
    let entry_type = get_str(record, "type").unwrap_or("event");
    match entry_type {
        // r[impl model.export.jscalendar.event]
        "event" => build_event(record).map(TaskOrEvent::Event),
        // r[impl model.export.jscalendar.task]
        "task" => build_task(record).map(TaskOrEvent::Task),
        other => Err(format!("unknown entry type: {other}")),
    }
}

// ── Event builder ────────────────────────────────────────────

fn build_event(record: &ImportRecord) -> Result<Event<Json>, String> {
    let uid = get_uid(record)?;
    let start = get_datetime(record, "start")
        .ok_or_else(|| "event missing required 'start' field".to_string())?;

    let mut event = Event::new(start, uid);

    if let Some(s) = get_str(record, "title") {
        event.set_title(s.to_string());
    }
    if let Some(s) = get_str(record, "description") {
        event.set_description(s.to_string());
    }
    if let Some(dur) = get_duration(record, "duration") {
        event.set_duration(dur);
    }
    if let Some(s) = get_str(record, "time_zone") {
        event.set_time_zone(s.to_string());
    }
    if let Some(s) = get_str(record, "status") {
        event.set_status(Token::<EventStatus, Box<str>>::from_str(s).unwrap());
    }
    if let Some(p) = get_priority(record) {
        event.set_priority(p);
    }
    if let Some(s) = get_str(record, "color")
        && let Ok(c) = Color::try_from_json(Json::String(s.to_string()))
    {
        event.set_color(c);
    }
    if let Some(s) = get_str(record, "locale")
        && let Ok(l) = calendar_types::string::LanguageTag::parse(s)
    {
        event.set_locale(l);
    }
    if let Some(s) = get_str(record, "privacy") {
        event.set_privacy(Token::<Privacy, Box<str>>::from_str(s).unwrap());
    }
    if let Some(s) = get_str(record, "free_busy_status") {
        event.set_free_busy_status(Token::<FreeBusyStatus, Box<str>>::from_str(s).unwrap());
    }
    if let Some(b) = get_bool(record, "show_without_time") {
        event.set_show_without_time(b);
    }
    if let Some(set) = get_string_set(record, "categories") {
        event.set_categories(set);
    }
    if let Some(set) = get_string_set(record, "keywords") {
        event.set_keywords(set);
    }

    // r[impl model.export.jscalendar.vendor]
    let vendor = collect_vendor_properties(record, EVENT_KNOWN);
    for (k, v) in vendor {
        event.insert_vendor_property(k.into(), v);
    }

    Ok(event)
}

const EVENT_KNOWN: &[&str] = &[
    "type",
    "name",
    "uid",
    "title",
    "description",
    "start",
    "duration",
    "time_zone",
    "status",
    "priority",
    "color",
    "locale",
    "privacy",
    "free_busy_status",
    "show_without_time",
    "categories",
    "keywords",
];

// ── Task builder ─────────────────────────────────────────────

fn build_task(record: &ImportRecord) -> Result<Task<Json>, String> {
    let uid = get_uid(record)?;

    let mut task = Task::new(uid);

    if let Some(s) = get_str(record, "title") {
        task.set_title(s.to_string());
    }
    if let Some(s) = get_str(record, "description") {
        task.set_description(s.to_string());
    }
    if let Some(dt) = get_datetime(record, "start") {
        task.set_start(dt);
    }
    if let Some(dt) = get_datetime(record, "due") {
        task.set_due(dt);
    }
    if let Some(dur) = get_duration(record, "estimated_duration") {
        task.set_estimated_duration(dur);
    }
    if let Some(n) = get_u64(record, "percent_complete")
        && let Ok(n) = u8::try_from(n)
        && let Some(pct) = Percent::new(n)
    {
        task.set_percent_complete(pct);
    }
    if let Some(s) = get_str(record, "progress") {
        task.set_progress(Token::<TaskProgress, Box<str>>::from_str(s).unwrap());
    }
    if let Some(s) = get_str(record, "time_zone") {
        task.set_time_zone(s.to_string());
    }
    if let Some(p) = get_priority(record) {
        task.set_priority(p);
    }
    if let Some(s) = get_str(record, "color")
        && let Ok(c) = Color::try_from_json(Json::String(s.to_string()))
    {
        task.set_color(c);
    }
    if let Some(s) = get_str(record, "locale")
        && let Ok(l) = calendar_types::string::LanguageTag::parse(s)
    {
        task.set_locale(l);
    }
    if let Some(s) = get_str(record, "privacy") {
        task.set_privacy(Token::<Privacy, Box<str>>::from_str(s).unwrap());
    }
    if let Some(s) = get_str(record, "free_busy_status") {
        task.set_free_busy_status(Token::<FreeBusyStatus, Box<str>>::from_str(s).unwrap());
    }
    if let Some(b) = get_bool(record, "show_without_time") {
        task.set_show_without_time(b);
    }
    if let Some(set) = get_string_set(record, "categories") {
        task.set_categories(set);
    }
    if let Some(set) = get_string_set(record, "keywords") {
        task.set_keywords(set);
    }

    // r[impl model.export.jscalendar.vendor]
    let vendor = collect_vendor_properties(record, TASK_KNOWN);
    for (k, v) in vendor {
        task.insert_vendor_property(k.into(), v);
    }

    Ok(task)
}

const TASK_KNOWN: &[&str] = &[
    "type",
    "name",
    "uid",
    "title",
    "description",
    "start",
    "due",
    "estimated_duration",
    "percent_complete",
    "progress",
    "time_zone",
    "priority",
    "color",
    "locale",
    "privacy",
    "free_busy_status",
    "show_without_time",
    "categories",
    "keywords",
];

// ── Value extraction helpers ─────────────────────────────────

fn get_str<'a>(record: &'a ImportRecord, key: &str) -> Option<&'a str> {
    match record.get(key)? {
        ImportValue::String(s) => Some(s),
        _ => None,
    }
}

fn get_u64(record: &ImportRecord, key: &str) -> Option<u64> {
    match record.get(key)? {
        ImportValue::Integer(n) => Some(*n),
        _ => None,
    }
}

fn get_bool(record: &ImportRecord, key: &str) -> Option<bool> {
    match record.get(key)? {
        ImportValue::Bool(b) => Some(*b),
        _ => None,
    }
}

fn get_uid(record: &ImportRecord) -> Result<Box<Uid>, String> {
    let s = get_str(record, "uid").ok_or("missing required 'uid' field")?;
    Uid::new(s)
        .map(Into::into)
        .map_err(|e| format!("invalid uid '{s}': {e}"))
}

fn get_datetime(record: &ImportRecord, key: &str) -> Option<DateTime<Local>> {
    let ImportValue::Record(dt_record) = record.get(key)? else {
        return None;
    };
    let ImportValue::Record(date_rec) = dt_record.get("date")? else {
        return None;
    };
    let ImportValue::Record(time_rec) = dt_record.get("time")? else {
        return None;
    };

    let year = get_u64(date_rec, "year")?;
    let month = get_u64(date_rec, "month")?;
    let day = get_u64(date_rec, "day")?;
    let hour = get_u64(time_rec, "hour").unwrap_or(0);
    let minute = get_u64(time_rec, "minute").unwrap_or(0);
    let second = get_u64(time_rec, "second").unwrap_or(0);

    let date = Date::new(
        Year::new(u16::try_from(year).ok()?).ok()?,
        Month::new(u8::try_from(month).ok()?).ok()?,
        Day::new(u8::try_from(day).ok()?).ok()?,
    )
    .ok()?;
    let time = Time::new(
        Hour::new(u8::try_from(hour).ok()?).ok()?,
        Minute::new(u8::try_from(minute).ok()?).ok()?,
        Second::new(u8::try_from(second).ok()?).ok()?,
        None,
    )
    .ok()?;

    Some(DateTime {
        date,
        time,
        marker: Local,
    })
}

fn get_duration(record: &ImportRecord, key: &str) -> Option<Duration> {
    let ImportValue::Record(dur_record) = record.get(key)? else {
        return None;
    };

    let weeks = u32::try_from(get_u64(dur_record, "weeks").unwrap_or(0)).ok()?;
    let days = u32::try_from(get_u64(dur_record, "days").unwrap_or(0)).ok()?;
    let hours = u32::try_from(get_u64(dur_record, "hours").unwrap_or(0)).ok()?;
    let minutes = u32::try_from(get_u64(dur_record, "minutes").unwrap_or(0)).ok()?;
    let seconds = u32::try_from(get_u64(dur_record, "seconds").unwrap_or(0)).ok()?;

    if weeks > 0 || days > 0 {
        let exact = (hours > 0 || minutes > 0 || seconds > 0).then_some(ExactDuration {
            hours,
            minutes,
            seconds,
            frac: None,
        });
        Some(Duration::Nominal(NominalDuration { weeks, days, exact }))
    } else {
        Some(Duration::Exact(ExactDuration {
            hours,
            minutes,
            seconds,
            frac: None,
        }))
    }
}

fn get_priority(record: &ImportRecord) -> Option<Priority> {
    let n = get_u64(record, "priority")?;
    match n {
        0 => Some(Priority::Zero),
        1 => Some(Priority::A1),
        2 => Some(Priority::A2),
        3 => Some(Priority::A3),
        4 => Some(Priority::B1),
        5 => Some(Priority::B2),
        6 => Some(Priority::B3),
        7 => Some(Priority::C1),
        8 => Some(Priority::C2),
        9 => Some(Priority::C3),
        _ => None,
    }
}

fn get_string_set(record: &ImportRecord, key: &str) -> Option<HashSet<String>> {
    let ImportValue::List(items) = record.get(key)? else {
        return None;
    };
    let set: HashSet<String> = items
        .iter()
        .filter_map(|v| match v {
            ImportValue::String(s) => Some(s.clone()),
            _ => None,
        })
        .collect();
    if set.is_empty() { None } else { Some(set) }
}

// ── Vendor properties ────────────────────────────────────────

/// Collect record fields not in the known set into a JSON object for vendor_property.
fn collect_vendor_properties(record: &ImportRecord, known: &[&str]) -> Map<String, Json> {
    let mut obj = Map::new();
    for (key, value) in record {
        if known.contains(&key.as_str()) {
            continue;
        }
        obj.insert(key.clone(), import_value_to_json(value));
    }
    obj
}

/// Convert an ImportValue to a serde_json::Value recursively.
fn import_value_to_json(value: &ImportValue) -> Json {
    match value {
        ImportValue::String(s) => Json::String(s.clone()),
        ImportValue::Integer(n) => Json::Number((*n).into()),
        ImportValue::SignedInteger(n) => Json::Number((*n).into()),
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
            (
                "uid",
                ImportValue::String("a8df6573-0474-496d-8496-033ad45d7fea".into()),
            ),
            ("title", ImportValue::String("Standup".into())),
            ("start", make_datetime(2026, 3, 12, 9, 0, 0)),
            ("duration", make_duration(0, 0, 1, 0, 0)),
            ("time_zone", ImportValue::String("America/New_York".into())),
        ]));

        let mut result = String::new();
        emit_jscalendar(&mut result, &cal, &[event], &mut vec![]).unwrap();
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
        assert!(entries[0].get("type").is_none());
    }

    #[test]
    fn emit_single_task_as_group() {
        let cal = make_cal("550e8400-e29b-41d4-a716-446655440000");
        let task = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("task".into())),
            (
                "uid",
                ImportValue::String("b9ef7684-1585-5a7e-b827-144b66551111".into()),
            ),
            ("title", ImportValue::String("Review PR".into())),
            ("due", make_datetime(2026, 3, 15, 17, 0, 0)),
            ("percent_complete", ImportValue::Integer(50)),
            ("progress", ImportValue::String("in-process".into())),
        ]));

        let mut result = String::new();
        emit_jscalendar(&mut result, &cal, &[task], &mut vec![]).unwrap();
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
            (
                "uid",
                ImportValue::String("a8df6573-0474-496d-8496-033ad45d7fea".into()),
            ),
            ("start", make_datetime(2026, 1, 1, 0, 0, 0)),
        ]));
        let task = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("task".into())),
            (
                "uid",
                ImportValue::String("b9ef7684-1585-5a7e-b827-144b66551111".into()),
            ),
        ]));

        let mut result = String::new();
        emit_jscalendar(&mut result, &cal, &[event, task], &mut vec![]).unwrap();
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
            (
                "uid",
                ImportValue::String("a8df6573-0474-496d-8496-033ad45d7fea".into()),
            ),
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

        let mut result = String::new();
        emit_jscalendar(&mut result, &cal, &[event], &mut vec![]).unwrap();
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
            (
                "uid",
                ImportValue::String("a8df6573-0474-496d-8496-033ad45d7fea".into()),
            ),
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

        let mut result = String::new();
        emit_jscalendar(&mut result, &cal, &[event], &mut vec![]).unwrap();
        let parsed: Json = serde_json::from_str(&result).unwrap();

        let entries = parsed["entries"].as_array().unwrap();
        assert_eq!(entries[0]["com.example:custom"], "vendor-value");
        assert_eq!(entries[0]["com.example:nested"]["key"], "val");
    }

    #[test]
    fn show_without_time_bool() {
        let cal = make_cal("550e8400-e29b-41d4-a716-446655440000");
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            (
                "uid",
                ImportValue::String("a8df6573-0474-496d-8496-033ad45d7fea".into()),
            ),
            ("start", make_datetime(2026, 1, 1, 0, 0, 0)),
            ("show_without_time", ImportValue::Bool(true)),
        ]));

        let mut result = String::new();
        emit_jscalendar(&mut result, &cal, &[event], &mut vec![]).unwrap();
        let parsed: Json = serde_json::from_str(&result).unwrap();

        let entries = parsed["entries"].as_array().unwrap();
        assert_eq!(entries[0]["showWithoutTime"], true);
    }

    #[test]
    fn priority_as_integer() {
        let cal = make_cal("550e8400-e29b-41d4-a716-446655440000");
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            (
                "uid",
                ImportValue::String("a8df6573-0474-496d-8496-033ad45d7fea".into()),
            ),
            ("start", make_datetime(2026, 1, 1, 0, 0, 0)),
            ("priority", ImportValue::Integer(5)),
        ]));

        let mut result = String::new();
        emit_jscalendar(&mut result, &cal, &[event], &mut vec![]).unwrap();
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
            (
                "uid",
                ImportValue::String("550e8400-e29b-41d4-a716-446655440000".into()),
            ),
            ("title", ImportValue::String("My Calendar".into())),
        ]);
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            (
                "uid",
                ImportValue::String("a8df6573-0474-496d-8496-033ad45d7fea".into()),
            ),
            ("start", make_datetime(2026, 1, 1, 0, 0, 0)),
        ]));

        let mut result = String::new();
        emit_jscalendar(&mut result, &cal, &[event], &mut vec![]).unwrap();
        let parsed: Json = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["@type"], "Group");
        assert_eq!(parsed["title"], "My Calendar");
    }

    // r[verify model.export.jscalendar.roundtrip]
    #[test]
    fn roundtrip_event_fields() {
        let json = r#"{
            "@type": "Event",
            "uid": "a8df6573-0474-496d-8496-033ad45d7fea",
            "updated": "2020-01-02T18:23:04Z",
            "title": "Roundtrip Test",
            "description": "An event for round-trip testing",
            "start": "2026-03-15T14:00:00",
            "timeZone": "America/New_York",
            "duration": "PT2H",
            "status": "confirmed",
            "priority": 3,
            "showWithoutTime": false,
            "categories": { "work": true, "meeting": true },
            "keywords": { "important": true }
        }"#;

        // Import the event.
        let import_result = gnomon_import::translate_jscalendar(json).unwrap();
        let ImportValue::Record(event_rec) = &import_result else {
            panic!("expected record");
        };

        // Wrap in a calendar and re-emit.
        let cal = make_cal("550e8400-e29b-41d4-a716-446655440000");
        let mut emitted = String::new();
        emit_jscalendar(
            &mut emitted,
            &cal,
            &[ImportValue::Record(event_rec.clone())],
            &mut vec![],
        )
        .unwrap();

        // Re-parse via the jscalendar crate to validate structure.
        let re_parsed: Json = serde_json::from_str(&emitted).unwrap();
        let group = Group::<Json>::try_from_json(re_parsed).expect("re-parse as Group failed");

        assert_eq!(
            group.uid().to_string(),
            "550e8400-e29b-41d4-a716-446655440000"
        );
        assert_eq!(group.entries().len(), 1);

        let TaskOrEvent::Event(event) = &group.entries()[0] else {
            panic!("expected Event");
        };

        assert_eq!(
            event.uid().to_string(),
            "a8df6573-0474-496d-8496-033ad45d7fea"
        );
        assert_eq!(event.title().map(|s| s.as_str()), Some("Roundtrip Test"));
        assert_eq!(
            event.description().map(|s| s.as_str()),
            Some("An event for round-trip testing")
        );
        assert_eq!(
            event.time_zone().map(|s| s.as_str()),
            Some("America/New_York")
        );
        assert_eq!(event.show_without_time(), Some(&false));
        assert!(event.categories().as_ref().unwrap().contains("work"));
        assert!(event.categories().as_ref().unwrap().contains("meeting"));
        assert!(event.keywords().as_ref().unwrap().contains("important"));
    }

    // r[verify model.export.jscalendar.roundtrip]
    #[test]
    fn roundtrip_task_fields() {
        let json = r#"{
            "@type": "Task",
            "uid": "b9ef7684-1585-5a7e-b827-144b66551111",
            "updated": "2020-01-02T18:23:04Z",
            "title": "Review PR",
            "due": "2026-03-20T18:00:00",
            "estimatedDuration": "PT30M",
            "percentComplete": 50,
            "progress": "in-process",
            "priority": 5
        }"#;

        // Import the task.
        let import_result = gnomon_import::translate_jscalendar(json).unwrap();
        let ImportValue::Record(task_rec) = &import_result else {
            panic!("expected record");
        };

        // Wrap in a calendar and re-emit.
        let cal = make_cal("550e8400-e29b-41d4-a716-446655440000");
        let mut emitted = String::new();
        emit_jscalendar(
            &mut emitted,
            &cal,
            &[ImportValue::Record(task_rec.clone())],
            &mut vec![],
        )
        .unwrap();

        // Re-parse via the jscalendar crate to validate structure.
        let re_parsed: Json = serde_json::from_str(&emitted).unwrap();
        let group = Group::<Json>::try_from_json(re_parsed).expect("re-parse as Group failed");

        assert_eq!(group.entries().len(), 1);

        let TaskOrEvent::Task(task) = &group.entries()[0] else {
            panic!("expected Task");
        };

        assert_eq!(
            task.uid().to_string(),
            "b9ef7684-1585-5a7e-b827-144b66551111"
        );
        assert_eq!(task.title().map(|s| s.as_str()), Some("Review PR"));
        assert_eq!(task.percent_complete().unwrap().get(), 50);
        assert_eq!(task.progress().as_ref().unwrap().to_string(), "in-process");
    }
}
