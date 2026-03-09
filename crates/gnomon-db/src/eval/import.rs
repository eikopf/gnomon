//! Translation of foreign calendar formats into Gnomon values.
//!
//! Supports iCalendar (RFC 5545) via `calico` and JSCalendar (RFC 8984) via `jscalendar`.

use calico::model::component::{Calendar as ICalCalendar, CalendarComponent};
use calico::model::primitive::{
    DateTimeOrDate, Duration, NominalDuration, Sign, SignedDuration, Status,
};

use jscalendar::json::TryFromJson;
use jscalendar::model::object::{Event as JsEvent, Group as JsGroup, Task as JsTask, TaskOrEvent};
use jscalendar::model::set::Priority as JsPriority;
use jscalendar::model::time::Duration as JsDuration;

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
/// Group objects are flattened into their entries.
pub fn translate_jscalendar<'db>(
    db: &'db dyn crate::Db,
    content: &str,
    blame: &Blame<'db>,
) -> Result<Value<'db>, String> {
    let json: serde_json::Value =
        serde_json::from_str(content).map_err(|e| format!("JSCalendar JSON parse error: {e}"))?;

    match &json {
        serde_json::Value::Array(arr) => {
            let mut items: Vec<Blamed<'db, Value<'db>>> = Vec::new();
            for element in arr {
                translate_jscal_top_level(db, element.clone(), blame, &mut items)?;
            }
            Ok(Value::List(items))
        }
        serde_json::Value::Object(_) => {
            let mut items: Vec<Blamed<'db, Value<'db>>> = Vec::new();
            translate_jscal_top_level(db, json, blame, &mut items)?;
            // Single object → unwrap from list.
            if items.len() == 1 {
                Ok(items.into_iter().next().unwrap().value)
            } else {
                Ok(Value::List(items))
            }
        }
        _ => Err("JSCalendar: expected a JSON object or array at top level".into()),
    }
}

/// Parse a single top-level JSON value as a JSCalendar object and append records.
fn translate_jscal_top_level<'db>(
    db: &'db dyn crate::Db,
    json: serde_json::Value,
    blame: &Blame<'db>,
    out: &mut Vec<Blamed<'db, Value<'db>>>,
) -> Result<(), String> {
    // Check if it's a Group first.
    if let Some(obj) = json.as_object() {
        if obj.get("@type").and_then(|v| v.as_str()) == Some("Group") {
            let group = JsGroup::<serde_json::Value>::try_from_json(json)
                .map_err(|e| format!("JSCalendar parse error: {e}"))?;
            for entry in group.entries() {
                let record = translate_task_or_event(db, entry, blame);
                out.push(Blamed { value: Value::Record(record), blame: blame.clone() });
            }
            return Ok(());
        }
    }

    let toe = TaskOrEvent::<serde_json::Value>::try_from_json(json)
        .map_err(|e| format!("JSCalendar parse error: {e}"))?;
    let record = translate_task_or_event(db, &toe, blame);
    out.push(Blamed { value: Value::Record(record), blame: blame.clone() });
    Ok(())
}

/// Translate a parsed `TaskOrEvent` into a Gnomon record.
fn translate_task_or_event<'db>(
    db: &'db dyn crate::Db,
    toe: &TaskOrEvent<serde_json::Value>,
    blame: &Blame<'db>,
) -> Record<'db> {
    match toe {
        TaskOrEvent::Event(event) => translate_js_event(db, event, blame),
        TaskOrEvent::Task(task) => translate_js_task(db, task, blame),
        _ => {
            // Future-proof: unknown variant → empty record with type.
            make_record(db, &[("type", Value::String("unknown".into()))], blame)
        }
    }
}

/// Translate a JSCalendar Event into a Gnomon record.
fn translate_js_event<'db>(
    db: &'db dyn crate::Db,
    event: &JsEvent<serde_json::Value>,
    blame: &Blame<'db>,
) -> Record<'db> {
    let mut fields: Vec<(&str, Value<'db>)> = Vec::new();
    fields.push(("type", Value::String("event".into())));

    // Required fields.
    fields.push(("uid", Value::String(event.uid().as_str().to_string())));
    fields.push(("start", translate_local_datetime(db, event.start(), blame)));

    // Optional fields.
    if let Some(title) = event.title() {
        fields.push(("title", Value::String(title.clone())));
    }
    if let Some(desc) = event.description() {
        fields.push(("description", Value::String(desc.clone())));
    }
    if let Some(dur) = event.duration() {
        fields.push(("duration", translate_jscal_duration(db, dur, blame)));
    }
    if let Some(tz) = event.time_zone() {
        fields.push(("time_zone", Value::String(tz.clone())));
    }
    if let Some(status) = event.status() {
        fields.push(("status", Value::String(status.to_string())));
    }
    if let Some(priority) = event.priority() {
        fields.push(("priority", Value::Integer(js_priority_to_u64(priority))));
    }
    if let Some(color) = event.color() {
        fields.push(("color", Value::String(color.to_string())));
    }
    if let Some(locale) = event.locale() {
        fields.push(("locale", Value::String(locale.to_string())));
    }
    if let Some(privacy) = event.privacy() {
        fields.push(("privacy", Value::String(privacy.to_string())));
    }
    if let Some(fbs) = event.free_busy_status() {
        fields.push(("free_busy_status", Value::String(fbs.to_string())));
    }
    if let Some(&swt) = event.show_without_time() {
        fields.push(("show_without_time", Value::Bool(swt)));
    }
    if let Some(cats) = event.categories() {
        let items: Vec<Blamed<'db, Value<'db>>> = cats
            .iter()
            .map(|s| Blamed { value: Value::String(s.clone()), blame: blame.clone() })
            .collect();
        if !items.is_empty() {
            fields.push(("categories", Value::List(items)));
        }
    }
    if let Some(kw) = event.keywords() {
        let items: Vec<Blamed<'db, Value<'db>>> = kw
            .iter()
            .map(|s| Blamed { value: Value::String(s.clone()), blame: blame.clone() })
            .collect();
        if !items.is_empty() {
            fields.push(("keywords", Value::List(items)));
        }
    }

    let mut record = make_record(db, &fields, blame);

    // Vendor (unknown) properties.
    for (key, val) in event.vendor_property_iter() {
        let field_name = FieldName::new(db, key.to_string());
        record.insert(
            field_name,
            Blamed {
                value: translate_json_value(db, val, blame),
                blame: blame.clone(),
            },
        );
    }

    record
}

/// Translate a JSCalendar Task into a Gnomon record.
fn translate_js_task<'db>(
    db: &'db dyn crate::Db,
    task: &JsTask<serde_json::Value>,
    blame: &Blame<'db>,
) -> Record<'db> {
    let mut fields: Vec<(&str, Value<'db>)> = Vec::new();
    fields.push(("type", Value::String("task".into())));

    // Required fields.
    fields.push(("uid", Value::String(task.uid().as_str().to_string())));

    // Optional fields.
    if let Some(title) = task.title() {
        fields.push(("title", Value::String(title.clone())));
    }
    if let Some(desc) = task.description() {
        fields.push(("description", Value::String(desc.clone())));
    }
    if let Some(start) = task.start() {
        fields.push(("start", translate_local_datetime(db, start, blame)));
    }
    if let Some(due) = task.due() {
        fields.push(("due", translate_local_datetime(db, due, blame)));
    }
    if let Some(dur) = task.estimated_duration() {
        fields.push(("estimated_duration", translate_jscal_duration(db, dur, blame)));
    }
    if let Some(pct) = task.percent_complete() {
        fields.push(("percent_complete", Value::Integer(pct.get() as u64)));
    }
    if let Some(progress) = task.progress() {
        fields.push(("progress", Value::String(progress.to_string())));
    }
    if let Some(tz) = task.time_zone() {
        fields.push(("time_zone", Value::String(tz.clone())));
    }
    if let Some(priority) = task.priority() {
        fields.push(("priority", Value::Integer(js_priority_to_u64(priority))));
    }
    if let Some(color) = task.color() {
        fields.push(("color", Value::String(color.to_string())));
    }
    if let Some(locale) = task.locale() {
        fields.push(("locale", Value::String(locale.to_string())));
    }
    if let Some(privacy) = task.privacy() {
        fields.push(("privacy", Value::String(privacy.to_string())));
    }
    if let Some(fbs) = task.free_busy_status() {
        fields.push(("free_busy_status", Value::String(fbs.to_string())));
    }
    if let Some(&swt) = task.show_without_time() {
        fields.push(("show_without_time", Value::Bool(swt)));
    }
    if let Some(cats) = task.categories() {
        let items: Vec<Blamed<'db, Value<'db>>> = cats
            .iter()
            .map(|s| Blamed { value: Value::String(s.clone()), blame: blame.clone() })
            .collect();
        if !items.is_empty() {
            fields.push(("categories", Value::List(items)));
        }
    }
    if let Some(kw) = task.keywords() {
        let items: Vec<Blamed<'db, Value<'db>>> = kw
            .iter()
            .map(|s| Blamed { value: Value::String(s.clone()), blame: blame.clone() })
            .collect();
        if !items.is_empty() {
            fields.push(("keywords", Value::List(items)));
        }
    }

    let mut record = make_record(db, &fields, blame);

    // Vendor (unknown) properties.
    for (key, val) in task.vendor_property_iter() {
        let field_name = FieldName::new(db, key.to_string());
        record.insert(
            field_name,
            Blamed {
                value: translate_json_value(db, val, blame),
                blame: blame.clone(),
            },
        );
    }

    record
}

/// Translate a `jscalendar` local datetime into a Gnomon datetime record.
fn translate_local_datetime<'db>(
    db: &'db dyn crate::Db,
    dt: &jscalendar::model::time::DateTime<jscalendar::model::time::Local>,
    blame: &Blame<'db>,
) -> Value<'db> {
    let date_fields = [
        ("year", Value::Integer(dt.date.year().get() as u64)),
        ("month", Value::Integer(dt.date.month().number().get() as u64)),
        ("day", Value::Integer(dt.date.day() as u8 as u64)),
    ];
    let time_fields = [
        ("hour", Value::Integer(dt.time.hour() as u8 as u64)),
        ("minute", Value::Integer(dt.time.minute() as u8 as u64)),
        ("second", Value::Integer(dt.time.second() as u8 as u64)),
    ];
    let date_rec = make_record(db, &date_fields, blame);
    let time_rec = make_record(db, &time_fields, blame);
    let dt_fields = [
        ("date", Value::Record(date_rec)),
        ("time", Value::Record(time_rec)),
    ];
    Value::Record(make_record(db, &dt_fields, blame))
}

/// Translate a `jscalendar` duration into a Gnomon duration record.
fn translate_jscal_duration<'db>(
    db: &'db dyn crate::Db,
    dur: &JsDuration,
    blame: &Blame<'db>,
) -> Value<'db> {
    match dur {
        JsDuration::Nominal(nom) => {
            let (hours, minutes, seconds) = match &nom.exact {
                Some(e) => (e.hours as u64, e.minutes as u64, e.seconds as u64),
                None => (0, 0, 0),
            };
            let fields = [
                ("weeks", Value::Integer(nom.weeks as u64)),
                ("days", Value::Integer(nom.days as u64)),
                ("hours", Value::Integer(hours)),
                ("minutes", Value::Integer(minutes)),
                ("seconds", Value::Integer(seconds)),
            ];
            Value::Record(make_record(db, &fields, blame))
        }
        JsDuration::Exact(exact) => {
            let fields = [
                ("weeks", Value::Integer(0)),
                ("days", Value::Integer(0)),
                ("hours", Value::Integer(exact.hours as u64)),
                ("minutes", Value::Integer(exact.minutes as u64)),
                ("seconds", Value::Integer(exact.seconds as u64)),
            ];
            Value::Record(make_record(db, &fields, blame))
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

/// Recursively translate a `serde_json::Value` into a Gnomon value.
///
/// Used for vendor (unknown) properties on JSCalendar objects.
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
                // Floats: store as string representation.
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
        serde_json::Value::Object(obj) => {
            let mut record = Record::new();
            for (key, v) in obj {
                let field_name = FieldName::new(db, key.clone());
                record.insert(
                    field_name,
                    Blamed {
                        value: translate_json_value(db, v, blame),
                        blame: blame.clone(),
                    },
                );
            }
            Value::Record(record)
        }
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
            "updated": "2020-01-02T18:23:04Z",
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
            "updated": "2020-01-02T18:23:04Z",
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
            "updated": "2020-01-02T18:23:04Z",
            "title": "Test",
            "start": "2026-01-01T00:00:00",
            "example.com:customField": "custom value",
            "example.com:nestedObj": { "a": 1, "b": true }
        }"#;

        let result = translate_jscalendar(&db, json, &blame).unwrap();
        let rec = match &result {
            Value::Record(r) => r,
            _ => panic!("expected record"),
        };
        assert_eq!(
            get_field(rec, &db, "example.com:customField"),
            Value::String("custom value".into())
        );
        match get_field(rec, &db, "example.com:nestedObj") {
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
            { "@type": "Event", "uid": "e1", "updated": "2020-01-02T18:23:04Z", "title": "A", "start": "2026-01-01T00:00:00" },
            { "@type": "Event", "uid": "e2", "updated": "2020-01-02T18:23:04Z", "title": "B", "start": "2026-01-01T00:00:00" }
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
}
