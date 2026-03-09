//! Translation of foreign calendar formats into plain Gnomon import values.
//!
//! Supports iCalendar (RFC 5545) via `calico` and JSCalendar (RFC 8984) via `jscalendar`.
//!
//! This crate is salsa-free — it produces [`ImportValue`] trees that the downstream
//! `gnomon-db` crate converts into its interned `Value<'db>` representation.

use std::collections::BTreeMap;

use calico::model::component::{Calendar as ICalCalendar, CalendarComponent};
use calico::model::primitive::{
    DateTimeOrDate, Duration, NominalDuration, Sign, SignedDuration, Status,
};

use jscalendar::json::TryFromJson;
use jscalendar::model::object::{Event as JsEvent, Group as JsGroup, Task as JsTask, TaskOrEvent};
use jscalendar::model::set::Priority as JsPriority;
use jscalendar::model::time::Duration as JsDuration;

/// A record represented as a string-keyed ordered map.
pub type ImportRecord = BTreeMap<String, ImportValue>;

/// A salsa-free value type mirroring `gnomon_db::eval::types::Value` without lifetimes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportValue {
    String(String),
    Integer(u64),
    SignedInteger(i64),
    Bool(bool),
    Undefined,
    Record(ImportRecord),
    List(Vec<ImportValue>),
}

// ── Helpers ─────────────────────────────────────────────────

fn make_record(fields: &[(&str, ImportValue)]) -> ImportRecord {
    fields
        .iter()
        .map(|(k, v)| ((*k).to_string(), v.clone()))
        .collect()
}

// ── iCalendar ────────────────────────────────────────────────

/// Translate an iCalendar string into an `ImportValue::List` of records.
pub fn translate_icalendar(content: &str) -> Result<ImportValue, String> {
    let calendars =
        ICalCalendar::parse(content).map_err(|e| format!("iCalendar parse error: {e}"))?;

    let mut records: Vec<ImportValue> = Vec::new();

    for cal in &calendars {
        for component in cal.components() {
            match component {
                CalendarComponent::Event(event) => {
                    let mut fields: Vec<(&str, ImportValue)> = Vec::new();
                    fields.push(("type", ImportValue::String("event".into())));

                    if let Some(uid_prop) = event.uid() {
                        fields.push((
                            "uid",
                            ImportValue::String(uid_prop.value.as_str().to_string()),
                        ));
                    }
                    if let Some(summary) = event.summary() {
                        fields.push(("title", ImportValue::String(summary.value.clone())));
                    }
                    if let Some(desc) = event.description() {
                        fields.push(("description", ImportValue::String(desc.value.clone())));
                    }
                    if let Some(dtstart) = event.dtstart() {
                        if let Some(val) = translate_datetime_or_date(&dtstart.value) {
                            fields.push(("start", val));
                        }
                        if let Some(tz) = dtstart.params.tz_id() {
                            fields.push((
                                "time_zone",
                                ImportValue::String(tz.as_str().to_string()),
                            ));
                        }
                    }
                    if let Some(dur) = event.duration() {
                        if let Some(val) = translate_signed_duration(&dur.value) {
                            fields.push(("duration", val));
                        }
                    } else if let (Some(dtstart), Some(dtend)) =
                        (event.dtstart(), event.dtend())
                    {
                        if let Some(val) =
                            compute_duration_from_endpoints(&dtstart.value, &dtend.value)
                        {
                            fields.push(("duration", val));
                        }
                    }
                    if let Some(status_prop) = event.status() {
                        fields.push(("status", translate_status(&status_prop.value)));
                    }
                    if let Some(priority_prop) = event.priority() {
                        fields.push((
                            "priority",
                            ImportValue::Integer(priority_to_u64(&priority_prop.value)),
                        ));
                    }
                    if let Some(loc) = event.location() {
                        fields.push(("location", ImportValue::String(loc.value.clone())));
                    }
                    if let Some(color) = event.color() {
                        fields.push(("color", ImportValue::String(color.value.to_string())));
                    }
                    if let Some(cats) = event.categories() {
                        let all_cats: Vec<ImportValue> = cats
                            .iter()
                            .flat_map(|c| c.value.iter())
                            .map(|s: &String| ImportValue::String(s.clone()))
                            .collect();
                        if !all_cats.is_empty() {
                            fields.push(("categories", ImportValue::List(all_cats)));
                        }
                    }

                    records.push(ImportValue::Record(make_record(&fields)));
                }
                CalendarComponent::Todo(todo) => {
                    let mut fields: Vec<(&str, ImportValue)> = Vec::new();
                    fields.push(("type", ImportValue::String("task".into())));

                    if let Some(uid_prop) = todo.uid() {
                        fields.push((
                            "uid",
                            ImportValue::String(uid_prop.value.as_str().to_string()),
                        ));
                    }
                    if let Some(summary) = todo.summary() {
                        fields.push(("title", ImportValue::String(summary.value.clone())));
                    }
                    if let Some(desc) = todo.description() {
                        fields.push(("description", ImportValue::String(desc.value.clone())));
                    }
                    if let Some(due_prop) = todo.due() {
                        if let Some(val) = translate_datetime_or_date(&due_prop.value) {
                            fields.push(("due", val));
                        }
                    }
                    if let Some(dtstart) = todo.dtstart() {
                        if let Some(val) = translate_datetime_or_date(&dtstart.value) {
                            fields.push(("start", val));
                        }
                        if let Some(tz) = dtstart.params.tz_id() {
                            fields.push((
                                "time_zone",
                                ImportValue::String(tz.as_str().to_string()),
                            ));
                        }
                    }
                    if let Some(dur) = todo.duration() {
                        if let Some(val) = translate_signed_duration(&dur.value) {
                            fields.push(("estimated_duration", val));
                        }
                    }
                    if let Some(pct) = todo.percent_complete() {
                        fields.push((
                            "percent_complete",
                            ImportValue::Integer(pct.value.get() as u64),
                        ));
                    }
                    if let Some(status_prop) = todo.status() {
                        fields.push(("status", translate_status(&status_prop.value)));
                    }
                    if let Some(priority_prop) = todo.priority() {
                        fields.push((
                            "priority",
                            ImportValue::Integer(priority_to_u64(&priority_prop.value)),
                        ));
                    }
                    if let Some(loc) = todo.location() {
                        fields.push(("location", ImportValue::String(loc.value.clone())));
                    }
                    if let Some(color) = todo.color() {
                        fields.push(("color", ImportValue::String(color.value.to_string())));
                    }
                    if let Some(cats) = todo.categories() {
                        let all_cats: Vec<ImportValue> = cats
                            .iter()
                            .flat_map(|c| c.value.iter())
                            .map(|s: &String| ImportValue::String(s.clone()))
                            .collect();
                        if !all_cats.is_empty() {
                            fields.push(("categories", ImportValue::List(all_cats)));
                        }
                    }

                    records.push(ImportValue::Record(make_record(&fields)));
                }
                // Skip VJOURNAL, VFREEBUSY, VTIMEZONE, etc.
                _ => {}
            }
        }
    }

    Ok(ImportValue::List(records))
}

// ── JSCalendar ───────────────────────────────────────────────

/// Translate a JSCalendar JSON string into an import value.
///
/// A single JSCalendar object produces `ImportValue::Record`; an array produces `ImportValue::List`.
/// Group objects are flattened into their entries.
pub fn translate_jscalendar(content: &str) -> Result<ImportValue, String> {
    let json: serde_json::Value =
        serde_json::from_str(content).map_err(|e| format!("JSCalendar JSON parse error: {e}"))?;

    match &json {
        serde_json::Value::Array(arr) => {
            let mut items: Vec<ImportValue> = Vec::new();
            for element in arr {
                translate_jscal_top_level(element.clone(), &mut items)?;
            }
            Ok(ImportValue::List(items))
        }
        serde_json::Value::Object(_) => {
            let mut items: Vec<ImportValue> = Vec::new();
            translate_jscal_top_level(json, &mut items)?;
            // Single object → unwrap from list.
            if items.len() == 1 {
                Ok(items.into_iter().next().unwrap())
            } else {
                Ok(ImportValue::List(items))
            }
        }
        _ => Err("JSCalendar: expected a JSON object or array at top level".into()),
    }
}

/// Parse a single top-level JSON value as a JSCalendar object and append records.
fn translate_jscal_top_level(
    json: serde_json::Value,
    out: &mut Vec<ImportValue>,
) -> Result<(), String> {
    // Check if it's a Group first.
    if let Some(obj) = json.as_object() {
        if obj.get("@type").and_then(|v| v.as_str()) == Some("Group") {
            let group = JsGroup::<serde_json::Value>::try_from_json(json)
                .map_err(|e| format!("JSCalendar parse error: {e}"))?;
            for entry in group.entries() {
                let record = translate_task_or_event(entry);
                out.push(ImportValue::Record(record));
            }
            return Ok(());
        }
    }

    let toe = TaskOrEvent::<serde_json::Value>::try_from_json(json)
        .map_err(|e| format!("JSCalendar parse error: {e}"))?;
    let record = translate_task_or_event(&toe);
    out.push(ImportValue::Record(record));
    Ok(())
}

/// Translate a parsed `TaskOrEvent` into an import record.
fn translate_task_or_event(toe: &TaskOrEvent<serde_json::Value>) -> ImportRecord {
    match toe {
        TaskOrEvent::Event(event) => translate_js_event(event),
        TaskOrEvent::Task(task) => translate_js_task(task),
        _ => {
            // Future-proof: unknown variant → empty record with type.
            make_record(&[("type", ImportValue::String("unknown".into()))])
        }
    }
}

/// Translate a JSCalendar Event into an import record.
fn translate_js_event(event: &JsEvent<serde_json::Value>) -> ImportRecord {
    let mut fields: Vec<(&str, ImportValue)> = Vec::new();
    fields.push(("type", ImportValue::String("event".into())));

    // Required fields.
    fields.push(("uid", ImportValue::String(event.uid().as_str().to_string())));
    fields.push(("start", translate_local_datetime(event.start())));

    // Optional fields.
    if let Some(title) = event.title() {
        fields.push(("title", ImportValue::String(title.clone())));
    }
    if let Some(desc) = event.description() {
        fields.push(("description", ImportValue::String(desc.clone())));
    }
    if let Some(dur) = event.duration() {
        fields.push(("duration", translate_jscal_duration(dur)));
    }
    if let Some(tz) = event.time_zone() {
        fields.push(("time_zone", ImportValue::String(tz.clone())));
    }
    if let Some(status) = event.status() {
        fields.push(("status", ImportValue::String(status.to_string())));
    }
    if let Some(priority) = event.priority() {
        fields.push(("priority", ImportValue::Integer(js_priority_to_u64(priority))));
    }
    if let Some(color) = event.color() {
        fields.push(("color", ImportValue::String(color.to_string())));
    }
    if let Some(locale) = event.locale() {
        fields.push(("locale", ImportValue::String(locale.to_string())));
    }
    if let Some(privacy) = event.privacy() {
        fields.push(("privacy", ImportValue::String(privacy.to_string())));
    }
    if let Some(fbs) = event.free_busy_status() {
        fields.push(("free_busy_status", ImportValue::String(fbs.to_string())));
    }
    if let Some(&swt) = event.show_without_time() {
        fields.push(("show_without_time", ImportValue::Bool(swt)));
    }
    if let Some(cats) = event.categories() {
        let items: Vec<ImportValue> = cats
            .iter()
            .map(|s| ImportValue::String(s.clone()))
            .collect();
        if !items.is_empty() {
            fields.push(("categories", ImportValue::List(items)));
        }
    }
    if let Some(kw) = event.keywords() {
        let items: Vec<ImportValue> = kw
            .iter()
            .map(|s| ImportValue::String(s.clone()))
            .collect();
        if !items.is_empty() {
            fields.push(("keywords", ImportValue::List(items)));
        }
    }

    let mut record = make_record(&fields);

    // Vendor (unknown) properties.
    for (key, val) in event.vendor_property_iter() {
        record.insert(key.to_string(), translate_json_value(val));
    }

    record
}

/// Translate a JSCalendar Task into an import record.
fn translate_js_task(task: &JsTask<serde_json::Value>) -> ImportRecord {
    let mut fields: Vec<(&str, ImportValue)> = Vec::new();
    fields.push(("type", ImportValue::String("task".into())));

    // Required fields.
    fields.push(("uid", ImportValue::String(task.uid().as_str().to_string())));

    // Optional fields.
    if let Some(title) = task.title() {
        fields.push(("title", ImportValue::String(title.clone())));
    }
    if let Some(desc) = task.description() {
        fields.push(("description", ImportValue::String(desc.clone())));
    }
    if let Some(start) = task.start() {
        fields.push(("start", translate_local_datetime(start)));
    }
    if let Some(due) = task.due() {
        fields.push(("due", translate_local_datetime(due)));
    }
    if let Some(dur) = task.estimated_duration() {
        fields.push(("estimated_duration", translate_jscal_duration(dur)));
    }
    if let Some(pct) = task.percent_complete() {
        fields.push(("percent_complete", ImportValue::Integer(pct.get() as u64)));
    }
    if let Some(progress) = task.progress() {
        fields.push(("progress", ImportValue::String(progress.to_string())));
    }
    if let Some(tz) = task.time_zone() {
        fields.push(("time_zone", ImportValue::String(tz.clone())));
    }
    if let Some(priority) = task.priority() {
        fields.push(("priority", ImportValue::Integer(js_priority_to_u64(priority))));
    }
    if let Some(color) = task.color() {
        fields.push(("color", ImportValue::String(color.to_string())));
    }
    if let Some(locale) = task.locale() {
        fields.push(("locale", ImportValue::String(locale.to_string())));
    }
    if let Some(privacy) = task.privacy() {
        fields.push(("privacy", ImportValue::String(privacy.to_string())));
    }
    if let Some(fbs) = task.free_busy_status() {
        fields.push(("free_busy_status", ImportValue::String(fbs.to_string())));
    }
    if let Some(&swt) = task.show_without_time() {
        fields.push(("show_without_time", ImportValue::Bool(swt)));
    }
    if let Some(cats) = task.categories() {
        let items: Vec<ImportValue> = cats
            .iter()
            .map(|s| ImportValue::String(s.clone()))
            .collect();
        if !items.is_empty() {
            fields.push(("categories", ImportValue::List(items)));
        }
    }
    if let Some(kw) = task.keywords() {
        let items: Vec<ImportValue> = kw
            .iter()
            .map(|s| ImportValue::String(s.clone()))
            .collect();
        if !items.is_empty() {
            fields.push(("keywords", ImportValue::List(items)));
        }
    }

    let mut record = make_record(&fields);

    // Vendor (unknown) properties.
    for (key, val) in task.vendor_property_iter() {
        record.insert(key.to_string(), translate_json_value(val));
    }

    record
}

/// Translate a `jscalendar` local datetime into an import datetime record.
fn translate_local_datetime(
    dt: &jscalendar::model::time::DateTime<jscalendar::model::time::Local>,
) -> ImportValue {
    let date_fields = [
        ("year", ImportValue::Integer(dt.date.year().get() as u64)),
        (
            "month",
            ImportValue::Integer(dt.date.month().number().get() as u64),
        ),
        ("day", ImportValue::Integer(dt.date.day() as u8 as u64)),
    ];
    let time_fields = [
        ("hour", ImportValue::Integer(dt.time.hour() as u8 as u64)),
        (
            "minute",
            ImportValue::Integer(dt.time.minute() as u8 as u64),
        ),
        (
            "second",
            ImportValue::Integer(dt.time.second() as u8 as u64),
        ),
    ];
    let dt_fields = [
        ("date", ImportValue::Record(make_record(&date_fields))),
        ("time", ImportValue::Record(make_record(&time_fields))),
    ];
    ImportValue::Record(make_record(&dt_fields))
}

/// Translate a `jscalendar` duration into an import duration record.
fn translate_jscal_duration(dur: &JsDuration) -> ImportValue {
    match dur {
        JsDuration::Nominal(nom) => {
            let (hours, minutes, seconds) = match &nom.exact {
                Some(e) => (e.hours as u64, e.minutes as u64, e.seconds as u64),
                None => (0, 0, 0),
            };
            let fields = [
                ("weeks", ImportValue::Integer(nom.weeks as u64)),
                ("days", ImportValue::Integer(nom.days as u64)),
                ("hours", ImportValue::Integer(hours)),
                ("minutes", ImportValue::Integer(minutes)),
                ("seconds", ImportValue::Integer(seconds)),
            ];
            ImportValue::Record(make_record(&fields))
        }
        JsDuration::Exact(exact) => {
            let fields = [
                ("weeks", ImportValue::Integer(0)),
                ("days", ImportValue::Integer(0)),
                ("hours", ImportValue::Integer(exact.hours as u64)),
                ("minutes", ImportValue::Integer(exact.minutes as u64)),
                ("seconds", ImportValue::Integer(exact.seconds as u64)),
            ];
            ImportValue::Record(make_record(&fields))
        }
    }
}

/// Convert a `jscalendar` Priority to a u64 (0-9).
fn js_priority_to_u64(p: &JsPriority) -> u64 {
    match p {
        JsPriority::Zero => 0,
        JsPriority::A1 => 1,
        JsPriority::A2 => 2,
        JsPriority::A3 => 3,
        JsPriority::B1 => 4,
        JsPriority::B2 => 5,
        JsPriority::B3 => 6,
        JsPriority::C1 => 7,
        JsPriority::C2 => 8,
        JsPriority::C3 => 9,
    }
}

/// Recursively translate a `serde_json::Value` into an import value.
///
/// Used for vendor (unknown) properties on JSCalendar objects.
fn translate_json_value(val: &serde_json::Value) -> ImportValue {
    match val {
        serde_json::Value::Null => ImportValue::Undefined,
        serde_json::Value::Bool(b) => ImportValue::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(u) = n.as_u64() {
                ImportValue::Integer(u)
            } else if let Some(i) = n.as_i64() {
                ImportValue::SignedInteger(i)
            } else {
                // Floats: store as string representation.
                ImportValue::String(n.to_string())
            }
        }
        serde_json::Value::String(s) => ImportValue::String(s.clone()),
        serde_json::Value::Array(arr) => {
            let items: Vec<ImportValue> = arr.iter().map(|v| translate_json_value(v)).collect();
            ImportValue::List(items)
        }
        serde_json::Value::Object(obj) => {
            let mut record = ImportRecord::new();
            for (key, v) in obj {
                record.insert(key.clone(), translate_json_value(v));
            }
            ImportValue::Record(record)
        }
    }
}

// ── Datetime/Duration translation helpers ────────────────────

/// Translate a calico `DateTimeOrDate` into an import datetime/date record.
fn translate_datetime_or_date(dtod: &DateTimeOrDate) -> Option<ImportValue> {
    match dtod {
        DateTimeOrDate::DateTime(dt) => {
            let date = dt.date;
            let time = dt.time;
            let date_fields = [
                ("year", ImportValue::Integer(date.year().get() as u64)),
                (
                    "month",
                    ImportValue::Integer(date.month().number().get() as u64),
                ),
                ("day", ImportValue::Integer(date.day() as u8 as u64)),
            ];
            let time_fields = [
                ("hour", ImportValue::Integer(time.hour() as u8 as u64)),
                (
                    "minute",
                    ImportValue::Integer(time.minute() as u8 as u64),
                ),
                (
                    "second",
                    ImportValue::Integer(time.second() as u8 as u64),
                ),
            ];
            let dt_fields = [
                ("date", ImportValue::Record(make_record(&date_fields))),
                ("time", ImportValue::Record(make_record(&time_fields))),
            ];
            Some(ImportValue::Record(make_record(&dt_fields)))
        }
        DateTimeOrDate::Date(date) => {
            let fields = [
                ("year", ImportValue::Integer(date.year().get() as u64)),
                (
                    "month",
                    ImportValue::Integer(date.month().number().get() as u64),
                ),
                ("day", ImportValue::Integer(date.day() as u8 as u64)),
            ];
            Some(ImportValue::Record(make_record(&fields)))
        }
    }
}

/// Translate a calico `SignedDuration` into an import duration record.
fn translate_signed_duration(sd: &SignedDuration) -> Option<ImportValue> {
    let positive = sd.sign == Sign::Pos;
    match &sd.duration {
        Duration::Nominal(nom) => {
            let exact = nom.exact.as_ref();
            translate_nominal_duration(positive, nom, exact)
        }
        Duration::Exact(exact) => {
            if positive {
                let fields = [
                    ("weeks", ImportValue::Integer(0)),
                    ("days", ImportValue::Integer(0)),
                    ("hours", ImportValue::Integer(exact.hours as u64)),
                    ("minutes", ImportValue::Integer(exact.minutes as u64)),
                    ("seconds", ImportValue::Integer(exact.seconds as u64)),
                ];
                Some(ImportValue::Record(make_record(&fields)))
            } else {
                let fields = [
                    ("weeks", ImportValue::SignedInteger(0)),
                    ("days", ImportValue::SignedInteger(0)),
                    ("hours", ImportValue::SignedInteger(-(exact.hours as i64))),
                    (
                        "minutes",
                        ImportValue::SignedInteger(-(exact.minutes as i64)),
                    ),
                    (
                        "seconds",
                        ImportValue::SignedInteger(-(exact.seconds as i64)),
                    ),
                ];
                Some(ImportValue::Record(make_record(&fields)))
            }
        }
    }
}

fn translate_nominal_duration(
    positive: bool,
    nom: &NominalDuration,
    exact: Option<&calico::model::primitive::ExactDuration>,
) -> Option<ImportValue> {
    let hours = exact.map_or(0, |e| e.hours as u64);
    let minutes = exact.map_or(0, |e| e.minutes as u64);
    let seconds = exact.map_or(0, |e| e.seconds as u64);

    if positive {
        let fields = [
            ("weeks", ImportValue::Integer(nom.weeks as u64)),
            ("days", ImportValue::Integer(nom.days as u64)),
            ("hours", ImportValue::Integer(hours)),
            ("minutes", ImportValue::Integer(minutes)),
            ("seconds", ImportValue::Integer(seconds)),
        ];
        Some(ImportValue::Record(make_record(&fields)))
    } else {
        let fields = [
            ("weeks", ImportValue::SignedInteger(-(nom.weeks as i64))),
            ("days", ImportValue::SignedInteger(-(nom.days as i64))),
            ("hours", ImportValue::SignedInteger(-(hours as i64))),
            ("minutes", ImportValue::SignedInteger(-(minutes as i64))),
            ("seconds", ImportValue::SignedInteger(-(seconds as i64))),
        ];
        Some(ImportValue::Record(make_record(&fields)))
    }
}

/// Compute duration = end - start for datetime-only endpoints (date-only falls back to None).
fn compute_duration_from_endpoints(
    start: &DateTimeOrDate,
    end: &DateTimeOrDate,
) -> Option<ImportValue> {
    match (start, end) {
        (DateTimeOrDate::DateTime(s), DateTimeOrDate::DateTime(e)) => {
            let s_secs = datetime_to_total_seconds(s);
            let e_secs = datetime_to_total_seconds(e);
            let diff = e_secs.saturating_sub(s_secs);
            let hours = diff / 3600;
            let minutes = (diff % 3600) / 60;
            let seconds = diff % 60;
            let fields = [
                ("weeks", ImportValue::Integer(0)),
                ("days", ImportValue::Integer(0)),
                ("hours", ImportValue::Integer(hours)),
                ("minutes", ImportValue::Integer(minutes)),
                ("seconds", ImportValue::Integer(seconds)),
            ];
            Some(ImportValue::Record(make_record(&fields)))
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
    let time_secs = dt.time.hour() as u8 as u64 * 3600
        + dt.time.minute() as u8 as u64 * 60
        + dt.time.second() as u8 as u64;
    days * 86400 + time_secs
}

/// Translate a calico Status to an import string value.
fn translate_status(status: &Status) -> ImportValue {
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
    ImportValue::String(s.into())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn get_field<'a>(record: &'a ImportRecord, name: &str) -> &'a ImportValue {
        record.get(name).unwrap()
    }

    fn has_field(record: &ImportRecord, name: &str) -> bool {
        record.contains_key(name)
    }

    // ── iCalendar tests ──────────────────────────────────────

    #[test]
    fn ical_minimal_event() {
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

        let result = translate_icalendar(ics).unwrap();
        let items = match &result {
            ImportValue::List(items) => items,
            _ => panic!("expected list"),
        };
        assert_eq!(items.len(), 1);
        let rec = match &items[0] {
            ImportValue::Record(r) => r,
            _ => panic!("expected record"),
        };
        assert_eq!(
            get_field(rec, "type"),
            &ImportValue::String("event".into())
        );
        assert_eq!(
            get_field(rec, "uid"),
            &ImportValue::String("test-uid-123".into())
        );
        assert_eq!(
            get_field(rec, "title"),
            &ImportValue::String("Team Meeting".into())
        );
        assert!(has_field(rec, "start"));
        assert!(has_field(rec, "duration"));

        // Check start datetime structure.
        match get_field(rec, "start") {
            ImportValue::Record(dt) => {
                match get_field(dt, "date") {
                    ImportValue::Record(d) => {
                        assert_eq!(get_field(d, "year"), &ImportValue::Integer(2026));
                        assert_eq!(get_field(d, "month"), &ImportValue::Integer(3));
                        assert_eq!(get_field(d, "day"), &ImportValue::Integer(15));
                    }
                    _ => panic!("expected date record"),
                }
                match get_field(dt, "time") {
                    ImportValue::Record(t) => {
                        assert_eq!(get_field(t, "hour"), &ImportValue::Integer(14));
                        assert_eq!(get_field(t, "minute"), &ImportValue::Integer(0));
                    }
                    _ => panic!("expected time record"),
                }
            }
            _ => panic!("expected start record"),
        }

        // Check duration structure.
        match get_field(rec, "duration") {
            ImportValue::Record(dur) => {
                assert_eq!(get_field(dur, "hours"), &ImportValue::Integer(1));
                assert_eq!(get_field(dur, "minutes"), &ImportValue::Integer(30));
            }
            _ => panic!("expected duration record"),
        }
    }

    #[test]
    fn ical_minimal_todo() {
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

        let result = translate_icalendar(ics).unwrap();
        let items = match &result {
            ImportValue::List(items) => items,
            _ => panic!("expected list"),
        };
        assert_eq!(items.len(), 1);
        let rec = match &items[0] {
            ImportValue::Record(r) => r,
            _ => panic!("expected record"),
        };
        assert_eq!(
            get_field(rec, "type"),
            &ImportValue::String("task".into())
        );
        assert_eq!(
            get_field(rec, "uid"),
            &ImportValue::String("todo-1".into())
        );
        assert_eq!(
            get_field(rec, "title"),
            &ImportValue::String("Buy groceries".into())
        );
        assert!(has_field(rec, "due"));
        assert_eq!(
            get_field(rec, "percent_complete"),
            &ImportValue::Integer(25)
        );
    }

    #[test]
    fn ical_parse_error() {
        let result = translate_icalendar("not valid icalendar");
        assert!(result.is_err());
    }

    #[test]
    fn ical_event_with_dtend() {
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

        let result = translate_icalendar(ics).unwrap();
        let items = match &result {
            ImportValue::List(items) => items,
            _ => panic!("expected list"),
        };
        let rec = match &items[0] {
            ImportValue::Record(r) => r,
            _ => panic!("expected record"),
        };
        // Duration should be computed from DTEND - DTSTART = 1h30m.
        match get_field(rec, "duration") {
            ImportValue::Record(dur) => {
                assert_eq!(get_field(dur, "hours"), &ImportValue::Integer(1));
                assert_eq!(get_field(dur, "minutes"), &ImportValue::Integer(30));
                assert_eq!(get_field(dur, "seconds"), &ImportValue::Integer(0));
            }
            _ => panic!("expected duration record"),
        }
    }

    // ── JSCalendar tests ─────────────────────────────────────

    #[test]
    fn jscal_minimal_event() {
        let json = r#"{
            "@type": "Event",
            "uid": "jscal-uid-1",
            "updated": "2020-01-02T18:23:04Z",
            "title": "Lunch",
            "start": "2026-03-15T12:00:00",
            "duration": "PT1H"
        }"#;

        let result = translate_jscalendar(json).unwrap();
        let rec = match &result {
            ImportValue::Record(r) => r,
            _ => panic!("expected record"),
        };
        assert_eq!(
            get_field(rec, "type"),
            &ImportValue::String("event".into())
        );
        assert_eq!(
            get_field(rec, "uid"),
            &ImportValue::String("jscal-uid-1".into())
        );
        assert_eq!(
            get_field(rec, "title"),
            &ImportValue::String("Lunch".into())
        );
        // start should be desugared to datetime record.
        match get_field(rec, "start") {
            ImportValue::Record(dt) => {
                assert!(has_field(dt, "date"));
                assert!(has_field(dt, "time"));
            }
            _ => panic!("expected start record"),
        }
        // duration should be desugared.
        match get_field(rec, "duration") {
            ImportValue::Record(dur) => {
                assert_eq!(get_field(dur, "hours"), &ImportValue::Integer(1));
            }
            _ => panic!("expected duration record"),
        }
    }

    #[test]
    fn jscal_task() {
        let json = r#"{
            "@type": "Task",
            "uid": "task-1",
            "updated": "2020-01-02T18:23:04Z",
            "title": "Do laundry",
            "due": "2026-03-20T18:00:00"
        }"#;

        let result = translate_jscalendar(json).unwrap();
        let rec = match &result {
            ImportValue::Record(r) => r,
            _ => panic!("expected record"),
        };
        assert_eq!(
            get_field(rec, "type"),
            &ImportValue::String("task".into())
        );
        assert_eq!(
            get_field(rec, "uid"),
            &ImportValue::String("task-1".into())
        );
        assert!(has_field(rec, "due"));
    }

    #[test]
    fn jscal_passthrough_unknown_fields() {
        let json = r#"{
            "@type": "Event",
            "uid": "e1",
            "updated": "2020-01-02T18:23:04Z",
            "title": "Test",
            "start": "2026-01-01T00:00:00",
            "example.com:customField": "custom value",
            "example.com:nestedObj": { "a": 1, "b": true }
        }"#;

        let result = translate_jscalendar(json).unwrap();
        let rec = match &result {
            ImportValue::Record(r) => r,
            _ => panic!("expected record"),
        };
        assert_eq!(
            get_field(rec, "example.com:customField"),
            &ImportValue::String("custom value".into())
        );
        match get_field(rec, "example.com:nestedObj") {
            ImportValue::Record(nested) => {
                assert_eq!(get_field(nested, "a"), &ImportValue::Integer(1));
                assert_eq!(get_field(nested, "b"), &ImportValue::Bool(true));
            }
            _ => panic!("expected nested record"),
        }
    }

    #[test]
    fn jscal_array_of_objects() {
        let json = r#"[
            { "@type": "Event", "uid": "e1", "updated": "2020-01-02T18:23:04Z", "title": "A", "start": "2026-01-01T00:00:00" },
            { "@type": "Event", "uid": "e2", "updated": "2020-01-02T18:23:04Z", "title": "B", "start": "2026-01-01T00:00:00" }
        ]"#;

        let result = translate_jscalendar(json).unwrap();
        match &result {
            ImportValue::List(items) => assert_eq!(items.len(), 2),
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn jscal_parse_error() {
        let result = translate_jscalendar("not json{");
        assert!(result.is_err());
    }
}
