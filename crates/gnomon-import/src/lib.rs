//! Translation of foreign calendar formats into plain Gnomon import values.
//!
//! Supports iCalendar (RFC 5545) via `calico` and JSCalendar (RFC 8984) via `jscalendar`.
//!
//! This crate is salsa-free — it produces [`ImportValue`] trees that the downstream
//! `gnomon-db` crate converts into its interned `Value<'db>` representation.

use std::collections::BTreeMap;

use calico::model::component::{Calendar as ICalCalendar, CalendarComponent};
use calico::model::primitive::{
    Attachment, ClassValue, DateTime, DateTimeOrDate, Duration, ExactDuration, Geo,
    NominalDuration, RDateSeq, RequestStatus, Sign, SignedDuration, Status, TimeTransparency,
    Token, Utc, Weekday,
};
use calico::model::rrule::{FreqByRules, RRule};
use calico::model::string::CaselessStr;

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

// ── Refresh interval extraction ──────────────────────────────

/// Extract the REFRESH-INTERVAL from iCalendar content as total seconds.
///
/// Returns `None` if the content cannot be parsed, contains no VCALENDAR
/// objects, or lacks a REFRESH-INTERVAL property.
pub fn extract_ical_refresh_interval_secs(content: &str) -> Option<u64> {
    let calendars = ICalCalendar::parse(content).ok()?;
    let cal = calendars.first()?;
    let ri = cal.refresh_interval()?;
    signed_duration_to_secs(&ri.value)
}

/// Convert a calico `SignedDuration` to total seconds.
/// Returns `None` for negative durations.
fn signed_duration_to_secs(sd: &SignedDuration) -> Option<u64> {
    if sd.sign != Sign::Pos {
        return None;
    }
    match &sd.duration {
        Duration::Nominal(nom) => {
            let exact_secs = nom.exact.as_ref().map_or(0u64, |e| {
                e.hours as u64 * 3600 + e.minutes as u64 * 60 + e.seconds as u64
            });
            Some(nom.weeks as u64 * 604_800 + nom.days as u64 * 86_400 + exact_secs)
        }
        Duration::Exact(exact) => {
            Some(exact.hours as u64 * 3600 + exact.minutes as u64 * 60 + exact.seconds as u64)
        }
    }
}

// ── iCalendar ────────────────────────────────────────────────

// r[impl model.import.icalendar.components]
/// Translate an iCalendar string into a calendar record.
///
/// The result is a single record with `type: "calendar"`, VCALENDAR-level
/// properties, and an `entries` field containing the translated VEVENT and
/// VTODO component records.
pub fn translate_icalendar(content: &str) -> Result<ImportValue, String> {
    let calendars =
        ICalCalendar::parse(content).map_err(|e| format!("iCalendar parse error: {e}"))?;

    // Each VCALENDAR object becomes a calendar record with nested entries.
    let mut result: Vec<ImportValue> = Vec::new();
    for cal in &calendars {
        let mut cal_record = translate_vcalendar_properties(cal);

        let mut entries: Vec<ImportValue> = Vec::new();
        for component in cal.components() {
            match component {
                CalendarComponent::Event(event) => {
                    entries.push(ImportValue::Record(translate_ical_event(event)));
                }
                CalendarComponent::Todo(todo) => {
                    entries.push(ImportValue::Record(translate_ical_todo(todo)));
                }
                _ => {}
            }
        }

        cal_record.insert("entries".to_string(), ImportValue::List(entries));
        result.push(ImportValue::Record(cal_record));
    }

    Ok(ImportValue::List(result))
}

// r[impl model.import.icalendar.calendar]
/// Translate VCALENDAR-level properties into a calendar record.
fn translate_vcalendar_properties(cal: &ICalCalendar) -> ImportRecord {
    let mut fields: Vec<(&str, ImportValue)> = Vec::new();
    fields.push(("type", ImportValue::String("calendar".into())));

    // PRODID is required.
    fields.push(("prod_id", ImportValue::String(cal.prod_id().value.clone())));

    // Optional RFC 7986 properties.
    if let Some(uid) = cal.uid() {
        fields.push(("uid", ImportValue::String(uid.value.as_str().to_string())));
    }
    if let Some(names) = cal.name()
        && let Some(first) = names.first()
    {
        fields.push(("name", ImportValue::String(first.value.clone())));
    }
    if let Some(descs) = cal.description()
        && let Some(first) = descs.first()
    {
        fields.push(("description", ImportValue::String(first.value.clone())));
    }
    if let Some(color) = cal.color() {
        fields.push(("color", ImportValue::String(color.value.to_string())));
    }
    if let Some(url) = cal.url() {
        fields.push(("url", ImportValue::String(url.value.as_str().to_string())));
    }
    if let Some(cats) = cal.categories() {
        let all_cats: Vec<ImportValue> = cats
            .iter()
            .flat_map(|c| c.value.iter())
            .map(|s: &String| ImportValue::String(s.clone()))
            .collect();
        if !all_cats.is_empty() {
            fields.push(("categories", ImportValue::List(all_cats)));
        }
    }
    if let Some(lm) = cal.last_modified() {
        fields.push(("last_modified", translate_utc_datetime(&lm.value)));
    }
    if let Some(ri) = cal.refresh_interval()
        && let Some(val) = translate_signed_duration(&ri.value)
    {
        fields.push(("refresh_interval", val));
    }
    if let Some(source) = cal.source() {
        fields.push((
            "source",
            ImportValue::String(source.value.as_str().to_string()),
        ));
    }

    let mut record = make_record(&fields);
    append_x_properties(cal, &mut record);
    record
}

// r[impl model.import.icalendar.event]
/// Translate a VEVENT component into an event record.
fn translate_ical_event(event: &calico::model::component::Event) -> ImportRecord {
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
            fields.push(("time_zone", ImportValue::String(tz.as_str().to_string())));
        }
    }
    if let Some(dur) = event.duration() {
        if let Some(val) = translate_signed_duration(&dur.value) {
            fields.push(("duration", val));
        }
    // r[impl model.import.icalendar.event.duration-fallback]
    } else if let (Some(dtstart), Some(dtend)) = (event.dtstart(), event.dtend())
        && let Some(val) = compute_duration_from_endpoints(&dtstart.value, &dtend.value)
    {
        fields.push(("duration", val));
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

    // Expanded properties.
    if let Some(dtstamp) = event.dtstamp() {
        fields.push(("dtstamp", translate_utc_datetime(&dtstamp.value)));
    }
    if let Some(class) = event.class() {
        fields.push(("class", translate_class(&class.value)));
    }
    if let Some(created) = event.created() {
        fields.push(("created", translate_utc_datetime(&created.value)));
    }
    if let Some(geo) = event.geo() {
        fields.push(("geo", translate_geo(&geo.value)));
    }
    if let Some(lm) = event.last_modified() {
        fields.push(("last_modified", translate_utc_datetime(&lm.value)));
    }
    if let Some(org) = event.organizer() {
        fields.push((
            "organizer",
            ImportValue::String(org.value.as_str().to_string()),
        ));
    }
    if let Some(seq) = event.sequence() {
        fields.push(("sequence", ImportValue::SignedInteger(seq.value as i64)));
    }
    if let Some(transp) = event.transp() {
        fields.push(("transparency", translate_transp(&transp.value)));
    }
    if let Some(url) = event.url() {
        fields.push(("url", ImportValue::String(url.value.as_str().to_string())));
    }
    if let Some(recurrence_id) = event.recurrence_id()
        && let Some(val) = translate_datetime_or_date(&recurrence_id.value)
    {
        fields.push(("recurrence_id", val));
    }
    if let Some(rrules) = event.rrule()
        && let Some(first) = rrules.first()
    {
        fields.push(("recur", translate_rrule(&first.value)));
    }
    if let Some(rdates) = event.rdate() {
        let items: Vec<ImportValue> = rdates
            .iter()
            .flat_map(|r| translate_rdate_seq(&r.value))
            .collect();
        if !items.is_empty() {
            fields.push(("rdates", ImportValue::List(items)));
        }
    }
    if let Some(exdates) = event.exdate() {
        let items: Vec<ImportValue> = exdates
            .iter()
            .flat_map(|e| translate_datetime_or_date(&e.value))
            .collect();
        if !items.is_empty() {
            fields.push(("exdates", ImportValue::List(items)));
        }
    }
    if let Some(attachments) = event.attach() {
        let items: Vec<ImportValue> = attachments
            .iter()
            .map(|a| translate_attachment(&a.value))
            .collect();
        if !items.is_empty() {
            fields.push(("attachments", ImportValue::List(items)));
        }
    }
    if let Some(attendees) = event.attendee() {
        let items: Vec<ImportValue> = attendees
            .iter()
            .map(|a| ImportValue::String(a.value.as_str().to_string()))
            .collect();
        if !items.is_empty() {
            fields.push(("attendees", ImportValue::List(items)));
        }
    }
    if let Some(comments) = event.comment() {
        let items: Vec<ImportValue> = comments
            .iter()
            .map(|c| ImportValue::String(c.value.clone()))
            .collect();
        if !items.is_empty() {
            fields.push(("comments", ImportValue::List(items)));
        }
    }
    if let Some(contacts) = event.contact() {
        let items: Vec<ImportValue> = contacts
            .iter()
            .map(|c| ImportValue::String(c.value.clone()))
            .collect();
        if !items.is_empty() {
            fields.push(("contacts", ImportValue::List(items)));
        }
    }
    if let Some(related) = event.related_to() {
        let items: Vec<ImportValue> = related
            .iter()
            .map(|r| ImportValue::String(r.value.as_str().to_string()))
            .collect();
        if !items.is_empty() {
            fields.push(("related_to", ImportValue::List(items)));
        }
    }
    if let Some(resources) = event.resources() {
        let items: Vec<ImportValue> = resources
            .iter()
            .map(|r| {
                ImportValue::List(
                    r.value
                        .iter()
                        .map(|s| ImportValue::String(s.clone()))
                        .collect(),
                )
            })
            .collect();
        if !items.is_empty() {
            fields.push(("resources", ImportValue::List(items)));
        }
    }
    if let Some(images) = event.image() {
        let items: Vec<ImportValue> = images
            .iter()
            .map(|i| translate_attachment(&i.value))
            .collect();
        if !items.is_empty() {
            fields.push(("images", ImportValue::List(items)));
        }
    }
    if let Some(conferences) = event.conference() {
        let items: Vec<ImportValue> = conferences
            .iter()
            .map(|c| ImportValue::String(c.value.as_str().to_string()))
            .collect();
        if !items.is_empty() {
            fields.push(("conferences", ImportValue::List(items)));
        }
    }
    if let Some(statuses) = event.request_status() {
        let items: Vec<ImportValue> = statuses
            .iter()
            .map(|s| translate_request_status(&s.value))
            .collect();
        if !items.is_empty() {
            fields.push(("request_statuses", ImportValue::List(items)));
        }
    }

    let mut record = make_record(&fields);
    append_x_properties(event, &mut record);
    record
}

// r[impl model.import.icalendar.task]
/// Translate a VTODO component into a task record.
fn translate_ical_todo(todo: &calico::model::component::Todo) -> ImportRecord {
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
    if let Some(due_prop) = todo.due()
        && let Some(val) = translate_datetime_or_date(&due_prop.value)
    {
        fields.push(("due", val));
    }
    if let Some(dtstart) = todo.dtstart() {
        if let Some(val) = translate_datetime_or_date(&dtstart.value) {
            fields.push(("start", val));
        }
        if let Some(tz) = dtstart.params.tz_id() {
            fields.push(("time_zone", ImportValue::String(tz.as_str().to_string())));
        }
    }
    if let Some(dur) = todo.duration()
        && let Some(val) = translate_signed_duration(&dur.value)
    {
        fields.push(("estimated_duration", val));
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

    // Expanded properties.
    if let Some(dtstamp) = todo.dtstamp() {
        fields.push(("dtstamp", translate_utc_datetime(&dtstamp.value)));
    }
    if let Some(class) = todo.class() {
        fields.push(("class", translate_class(&class.value)));
    }
    if let Some(created) = todo.created() {
        fields.push(("created", translate_utc_datetime(&created.value)));
    }
    if let Some(geo) = todo.geo() {
        fields.push(("geo", translate_geo(&geo.value)));
    }
    if let Some(lm) = todo.last_modified() {
        fields.push(("last_modified", translate_utc_datetime(&lm.value)));
    }
    if let Some(org) = todo.organizer() {
        fields.push((
            "organizer",
            ImportValue::String(org.value.as_str().to_string()),
        ));
    }
    if let Some(seq) = todo.sequence() {
        fields.push(("sequence", ImportValue::SignedInteger(seq.value as i64)));
    }
    if let Some(url) = todo.url() {
        fields.push(("url", ImportValue::String(url.value.as_str().to_string())));
    }
    if let Some(completed) = todo.completed() {
        fields.push(("completed", translate_utc_datetime(&completed.value)));
    }
    if let Some(recurrence_id) = todo.recurrence_id()
        && let Some(val) = translate_datetime_or_date(&recurrence_id.value)
    {
        fields.push(("recurrence_id", val));
    }
    if let Some(rrules) = todo.rrule()
        && let Some(first) = rrules.first()
    {
        fields.push(("recur", translate_rrule(&first.value)));
    }
    if let Some(rdates) = todo.rdate() {
        let items: Vec<ImportValue> = rdates
            .iter()
            .flat_map(|r| translate_rdate_seq(&r.value))
            .collect();
        if !items.is_empty() {
            fields.push(("rdates", ImportValue::List(items)));
        }
    }
    if let Some(exdates) = todo.exdate() {
        let items: Vec<ImportValue> = exdates
            .iter()
            .flat_map(|e| translate_datetime_or_date(&e.value))
            .collect();
        if !items.is_empty() {
            fields.push(("exdates", ImportValue::List(items)));
        }
    }
    if let Some(attachments) = todo.attach() {
        let items: Vec<ImportValue> = attachments
            .iter()
            .map(|a| translate_attachment(&a.value))
            .collect();
        if !items.is_empty() {
            fields.push(("attachments", ImportValue::List(items)));
        }
    }
    if let Some(attendees) = todo.attendee() {
        let items: Vec<ImportValue> = attendees
            .iter()
            .map(|a| ImportValue::String(a.value.as_str().to_string()))
            .collect();
        if !items.is_empty() {
            fields.push(("attendees", ImportValue::List(items)));
        }
    }
    if let Some(comments) = todo.comment() {
        let items: Vec<ImportValue> = comments
            .iter()
            .map(|c| ImportValue::String(c.value.clone()))
            .collect();
        if !items.is_empty() {
            fields.push(("comments", ImportValue::List(items)));
        }
    }
    if let Some(contacts) = todo.contact() {
        let items: Vec<ImportValue> = contacts
            .iter()
            .map(|c| ImportValue::String(c.value.clone()))
            .collect();
        if !items.is_empty() {
            fields.push(("contacts", ImportValue::List(items)));
        }
    }
    if let Some(related) = todo.related_to() {
        let items: Vec<ImportValue> = related
            .iter()
            .map(|r| ImportValue::String(r.value.as_str().to_string()))
            .collect();
        if !items.is_empty() {
            fields.push(("related_to", ImportValue::List(items)));
        }
    }
    if let Some(resources) = todo.resources() {
        let items: Vec<ImportValue> = resources
            .iter()
            .map(|r| {
                ImportValue::List(
                    r.value
                        .iter()
                        .map(|s| ImportValue::String(s.clone()))
                        .collect(),
                )
            })
            .collect();
        if !items.is_empty() {
            fields.push(("resources", ImportValue::List(items)));
        }
    }
    if let Some(images) = todo.image() {
        let items: Vec<ImportValue> = images
            .iter()
            .map(|i| translate_attachment(&i.value))
            .collect();
        if !items.is_empty() {
            fields.push(("images", ImportValue::List(items)));
        }
    }
    if let Some(conferences) = todo.conference() {
        let items: Vec<ImportValue> = conferences
            .iter()
            .map(|c| ImportValue::String(c.value.as_str().to_string()))
            .collect();
        if !items.is_empty() {
            fields.push(("conferences", ImportValue::List(items)));
        }
    }
    if let Some(statuses) = todo.request_status() {
        let items: Vec<ImportValue> = statuses
            .iter()
            .map(|s| translate_request_status(&s.value))
            .collect();
        if !items.is_empty() {
            fields.push(("request_statuses", ImportValue::List(items)));
        }
    }

    let mut record = make_record(&fields);
    append_x_properties(todo, &mut record);
    record
}

// ── iCalendar property translation helpers ────────────────────

/// Translate a UTC datetime into an import datetime record.
fn translate_utc_datetime(dt: &DateTime<Utc>) -> ImportValue {
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
        ("minute", ImportValue::Integer(time.minute() as u8 as u64)),
        ("second", ImportValue::Integer(time.second() as u8 as u64)),
    ];
    let dt_fields = [
        ("date", ImportValue::Record(make_record(&date_fields))),
        ("time", ImportValue::Record(make_record(&time_fields))),
    ];
    ImportValue::Record(make_record(&dt_fields))
}

/// Translate a CLASS property value to a lowercase string.
fn translate_class(val: &Token<ClassValue, String>) -> ImportValue {
    let s = match val {
        Token::Known(ClassValue::Public) => "public".to_string(),
        Token::Known(ClassValue::Private) => "private".to_string(),
        Token::Known(ClassValue::Confidential) => "confidential".to_string(),
        Token::Known(_) => "unknown".to_string(),
        Token::Unknown(s) => s.to_lowercase(),
    };
    ImportValue::String(s)
}

/// Translate a TRANSP property value to a lowercase string.
fn translate_transp(val: &TimeTransparency) -> ImportValue {
    let s = match val {
        TimeTransparency::Opaque => "opaque",
        TimeTransparency::Transparent => "transparent",
        _ => "opaque",
    };
    ImportValue::String(s.into())
}

/// Translate a GEO property to a record with latitude and longitude strings.
fn translate_geo(geo: &Geo) -> ImportValue {
    let fields = [
        ("latitude", ImportValue::String(geo.lat.to_string())),
        ("longitude", ImportValue::String(geo.lon.to_string())),
    ];
    ImportValue::Record(make_record(&fields))
}

/// Translate an ATTACH or IMAGE property.
fn translate_attachment(val: &Attachment) -> ImportValue {
    match val {
        Attachment::Uri(uri) => ImportValue::String(uri.as_str().to_string()),
        Attachment::Binary(data) => {
            let fields = [
                ("encoding", ImportValue::String("base64".into())),
                ("data", ImportValue::String(base64_encode(data))),
            ];
            ImportValue::Record(make_record(&fields))
        }
    }
}

/// Simple base64 encoder (no external dep needed — just use a manual implementation).
fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(ALPHABET[((triple >> 18) & 0x3F) as usize] as char);
        result.push(ALPHABET[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(ALPHABET[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(ALPHABET[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

/// Translate a REQUEST-STATUS property to a string.
fn translate_request_status(rs: &RequestStatus) -> ImportValue {
    let code = &rs.code;
    let class = code.class.as_u8();
    let desc = &rs.description;
    let s = if let Some(minor) = code.minor {
        format!("{}.{}.{};{}", class, code.major, minor, desc)
    } else {
        format!("{}.{};{}", class, code.major, desc)
    };
    ImportValue::String(s)
}

// r[impl model.import.icalendar.rrule]
/// Translate an RRULE property to a recurrence rule record.
fn translate_rrule(rrule: &RRule) -> ImportValue {
    let mut fields: Vec<(&str, ImportValue)> = Vec::new();

    // Frequency.
    let freq_str = match &rrule.freq {
        FreqByRules::Secondly(_) => "secondly",
        FreqByRules::Minutely(_) => "minutely",
        FreqByRules::Hourly(_) => "hourly",
        FreqByRules::Daily(_) => "daily",
        FreqByRules::Weekly => "weekly",
        FreqByRules::Monthly(_) => "monthly",
        FreqByRules::Yearly(_) => "yearly",
    };
    fields.push(("frequency", ImportValue::String(freq_str.into())));

    // Interval.
    if let Some(interval) = rrule.interval {
        fields.push(("interval", ImportValue::Integer(interval.get().get())));
    }

    // Termination.
    if let Some(ref term) = rrule.termination {
        match term {
            calico::model::rrule::Termination::Count(c) => {
                fields.push(("count", ImportValue::Integer(*c)));
            }
            calico::model::rrule::Termination::Until(dtod) => {
                if let Some(val) = translate_datetime_or_date(dtod) {
                    fields.push(("until", val));
                }
            }
        }
    }

    // Week start.
    if let Some(wkst) = rrule.week_start {
        fields.push(("week_start", translate_weekday(wkst)));
    }

    // Core BY rules.
    let core = &rrule.core_by_rules;
    if let Some(ref by_second) = core.by_second {
        let items: Vec<ImportValue> = (0..=60u8)
            .filter_map(|s| {
                let sec = calico::model::rrule::Second::from_repr(s)?;
                if by_second.get(sec) {
                    Some(ImportValue::Integer(s as u64))
                } else {
                    None
                }
            })
            .collect();
        if !items.is_empty() {
            fields.push(("by_second", ImportValue::List(items)));
        }
    }
    if let Some(ref by_minute) = core.by_minute {
        let items: Vec<ImportValue> = (0..=59u8)
            .filter_map(|m| {
                let min = calico::model::rrule::Minute::from_repr(m)?;
                if by_minute.get(min) {
                    Some(ImportValue::Integer(m as u64))
                } else {
                    None
                }
            })
            .collect();
        if !items.is_empty() {
            fields.push(("by_minute", ImportValue::List(items)));
        }
    }
    if let Some(ref by_hour) = core.by_hour {
        let items: Vec<ImportValue> = (0..=23u8)
            .filter_map(|h| {
                let hour = calico::model::rrule::Hour::from_repr(h)?;
                if by_hour.get(hour) {
                    Some(ImportValue::Integer(h as u64))
                } else {
                    None
                }
            })
            .collect();
        if !items.is_empty() {
            fields.push(("by_hour", ImportValue::List(items)));
        }
    }
    if let Some(ref by_month) = core.by_month {
        let items: Vec<ImportValue> = (1..=12u8)
            .filter_map(|m| {
                let month = calico::model::primitive::Month::new(m).ok()?;
                if by_month.get(month) {
                    Some(ImportValue::Integer(m as u64))
                } else {
                    None
                }
            })
            .collect();
        if !items.is_empty() {
            fields.push(("by_month", ImportValue::List(items)));
        }
    }
    if let Some(ref by_day) = core.by_day {
        let items: Vec<ImportValue> = by_day
            .iter()
            .map(|wdn| {
                let day_str = weekday_to_str(wdn.weekday);
                if let Some((sign, week)) = wdn.ordinal {
                    let ord = match sign {
                        Sign::Pos => week as u8 as i64,
                        Sign::Neg => -(week as u8 as i64),
                    };
                    let fields = [
                        ("day", ImportValue::String(day_str.into())),
                        ("ordinal", ImportValue::SignedInteger(ord)),
                    ];
                    ImportValue::Record(make_record(&fields))
                } else {
                    ImportValue::String(day_str.into())
                }
            })
            .collect();
        if !items.is_empty() {
            fields.push(("by_day", ImportValue::List(items)));
        }
    }
    if let Some(ref by_set_pos) = core.by_set_pos {
        let items: Vec<ImportValue> = by_set_pos
            .iter()
            .map(|ydn| ImportValue::SignedInteger(ydn.get() as i64))
            .collect();
        if !items.is_empty() {
            fields.push(("by_set_pos", ImportValue::List(items)));
        }
    }

    // Frequency-dependent BY rules.
    match &rrule.freq {
        FreqByRules::Secondly(rules)
        | FreqByRules::Minutely(rules)
        | FreqByRules::Hourly(rules) => {
            translate_by_month_day(&rules.by_month_day, &mut fields);
            translate_by_year_day(&rules.by_year_day, &mut fields);
        }
        FreqByRules::Daily(rules) | FreqByRules::Monthly(rules) => {
            translate_by_month_day(&rules.by_month_day, &mut fields);
        }
        FreqByRules::Yearly(rules) => {
            translate_by_month_day(&rules.by_month_day, &mut fields);
            translate_by_year_day(&rules.by_year_day, &mut fields);
            translate_by_week_no(&rules.by_week_no, &mut fields);
        }
        FreqByRules::Weekly => {}
    }

    ImportValue::Record(make_record(&fields))
}

fn translate_by_month_day(
    by_month_day: &Option<calico::model::rrule::MonthDaySet>,
    fields: &mut Vec<(&str, ImportValue)>,
) {
    if let Some(set) = by_month_day {
        let mut items: Vec<ImportValue> = Vec::new();
        // Positive days 1..=31
        for d in 1..=31u8 {
            if let Some(day) = calico::model::rrule::MonthDay::from_repr(d) {
                let idx =
                    calico::model::rrule::MonthDaySetIndex::from_signed_month_day(Sign::Pos, day);
                if set.get(idx) {
                    items.push(ImportValue::SignedInteger(d as i64));
                }
            }
        }
        // Negative days -31..=-1
        for d in 1..=31u8 {
            if let Some(day) = calico::model::rrule::MonthDay::from_repr(d) {
                let idx =
                    calico::model::rrule::MonthDaySetIndex::from_signed_month_day(Sign::Neg, day);
                if set.get(idx) {
                    items.push(ImportValue::SignedInteger(-(d as i64)));
                }
            }
        }
        if !items.is_empty() {
            fields.push(("by_month_day", ImportValue::List(items)));
        }
    }
}

fn translate_by_year_day(
    by_year_day: &Option<std::collections::BTreeSet<calico::model::rrule::YearDayNum>>,
    fields: &mut Vec<(&str, ImportValue)>,
) {
    if let Some(set) = by_year_day {
        let items: Vec<ImportValue> = set
            .iter()
            .map(|ydn| ImportValue::SignedInteger(ydn.get() as i64))
            .collect();
        if !items.is_empty() {
            fields.push(("by_year_day", ImportValue::List(items)));
        }
    }
}

fn translate_by_week_no(
    by_week_no: &Option<calico::model::rrule::WeekNoSet>,
    fields: &mut Vec<(&str, ImportValue)>,
) {
    if let Some(set) = by_week_no {
        let mut items: Vec<ImportValue> = Vec::new();
        // Positive weeks 1..=53
        for w in 1..=53u8 {
            if let Some(week) = calico::model::primitive::IsoWeek::from_index(w) {
                let idx = calico::model::rrule::WeekNoSetIndex::from_signed_week(Sign::Pos, week);
                if set.get(idx) {
                    items.push(ImportValue::SignedInteger(w as i64));
                }
            }
        }
        // Negative weeks -53..=-1
        for w in 1..=53u8 {
            if let Some(week) = calico::model::primitive::IsoWeek::from_index(w) {
                let idx = calico::model::rrule::WeekNoSetIndex::from_signed_week(Sign::Neg, week);
                if set.get(idx) {
                    items.push(ImportValue::SignedInteger(-(w as i64)));
                }
            }
        }
        if !items.is_empty() {
            fields.push(("by_week_no", ImportValue::List(items)));
        }
    }
}

fn translate_weekday(wd: Weekday) -> ImportValue {
    ImportValue::String(weekday_to_str(wd).into())
}

fn weekday_to_str(wd: Weekday) -> &'static str {
    match wd {
        Weekday::Monday => "monday",
        Weekday::Tuesday => "tuesday",
        Weekday::Wednesday => "wednesday",
        Weekday::Thursday => "thursday",
        Weekday::Friday => "friday",
        Weekday::Saturday => "saturday",
        Weekday::Sunday => "sunday",
    }
}

/// Translate an RDATE sequence into a list of datetime/date records.
fn translate_rdate_seq(seq: &RDateSeq) -> Vec<ImportValue> {
    match seq {
        RDateSeq::DateTime(dts) => dts
            .iter()
            .filter_map(|dt| translate_datetime_or_date(&DateTimeOrDate::DateTime(*dt)))
            .collect(),
        RDateSeq::Date(dates) => dates
            .iter()
            .filter_map(|d| translate_datetime_or_date(&DateTimeOrDate::Date(*d)))
            .collect(),
        RDateSeq::Period(periods) => periods
            .iter()
            .filter_map(|p| {
                let start_dt = match p {
                    calico::model::primitive::Period::Explicit { start, .. } => start,
                    calico::model::primitive::Period::Start { start, .. } => start,
                };
                let start = DateTimeOrDate::DateTime(*start_dt);
                translate_datetime_or_date(&start)
            })
            .collect(),
    }
}

/// Translate a calico `Value<String>` (used for x-properties) to an ImportValue.
fn translate_ical_value(val: &calico::model::primitive::Value<String>) -> ImportValue {
    use calico::model::primitive::Value;
    match val {
        Value::Text(s) => ImportValue::String(s.clone()),
        Value::Integer(i) => ImportValue::SignedInteger(*i as i64),
        Value::Boolean(b) => ImportValue::Bool(*b),
        Value::DateTime(dt) => {
            let dtod = DateTimeOrDate::DateTime(*dt);
            translate_datetime_or_date(&dtod).unwrap_or(ImportValue::Undefined)
        }
        Value::Date(d) => {
            translate_datetime_or_date(&DateTimeOrDate::Date(*d)).unwrap_or(ImportValue::Undefined)
        }
        Value::Duration(sd) => translate_signed_duration(sd).unwrap_or(ImportValue::Undefined),
        Value::Uri(u) => ImportValue::String(u.as_str().to_string()),
        Value::CalAddress(u) => ImportValue::String(u.as_str().to_string()),
        Value::Float(f) => ImportValue::String(f.to_string()),
        Value::Binary(b) => ImportValue::String(base64_encode(b)),
        Value::Recur(rrule) => translate_rrule(rrule),
        Value::Time(t, _) => {
            let fields = [
                ("hour", ImportValue::Integer(t.hour() as u8 as u64)),
                ("minute", ImportValue::Integer(t.minute() as u8 as u64)),
                ("second", ImportValue::Integer(t.second() as u8 as u64)),
            ];
            ImportValue::Record(make_record(&fields))
        }
        Value::UtcOffset(off) => {
            let sign_char = match off.sign {
                Sign::Pos => '+',
                Sign::Neg => '-',
            };
            ImportValue::String(format!(
                "{}{:02}:{:02}",
                sign_char, off.hour as u8, off.minute as u8,
            ))
        }
        Value::Period(p) => {
            let start_dt = match p {
                calico::model::primitive::Period::Explicit { start, .. } => start,
                calico::model::primitive::Period::Start { start, .. } => start,
            };
            let start = DateTimeOrDate::DateTime(*start_dt);
            translate_datetime_or_date(&start).unwrap_or(ImportValue::Undefined)
        }
        Value::Other { value, .. } => ImportValue::String(value.clone()),
    }
}

/// Trait for components that have x_property_iter().
trait HasXProperties {
    fn x_property_pairs(&self) -> Vec<(&CaselessStr, &calico::model::primitive::Value<String>)>;
}

impl HasXProperties for ICalCalendar {
    fn x_property_pairs(&self) -> Vec<(&CaselessStr, &calico::model::primitive::Value<String>)> {
        self.x_property_iter()
            .flat_map(|(k, props)| props.iter().map(move |prop| (k.as_ref(), &prop.value)))
            .collect()
    }
}

impl HasXProperties for calico::model::component::Event {
    fn x_property_pairs(&self) -> Vec<(&CaselessStr, &calico::model::primitive::Value<String>)> {
        self.x_property_iter()
            .flat_map(|(k, props)| props.iter().map(move |prop| (k.as_ref(), &prop.value)))
            .collect()
    }
}

impl HasXProperties for calico::model::component::Todo {
    fn x_property_pairs(&self) -> Vec<(&CaselessStr, &calico::model::primitive::Value<String>)> {
        self.x_property_iter()
            .flat_map(|(k, props)| props.iter().map(move |prop| (k.as_ref(), &prop.value)))
            .collect()
    }
}

// r[impl model.import.icalendar.extension]
// r[impl model.import.preserve]
/// Append x-property fields to a record.
fn append_x_properties<T: HasXProperties>(component: &T, record: &mut ImportRecord) {
    for (key, value) in component.x_property_pairs() {
        let field_name = key.as_str().to_lowercase().replace('-', "_");
        record.insert(field_name, translate_ical_value(value));
    }
}

// ── JSCalendar ───────────────────────────────────────────────

// r[impl model.import.jscalendar.types]
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
    if let Some(obj) = json.as_object()
        && obj.get("@type").and_then(|v| v.as_str()) == Some("Group")
    {
        let group = JsGroup::<serde_json::Value>::try_from_json(json)
            .map_err(|e| format!("JSCalendar parse error: {e}"))?;
        for entry in group.entries() {
            let record = translate_task_or_event(entry);
            out.push(ImportValue::Record(record));
        }
        return Ok(());
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

// r[impl model.import.jscalendar.event]
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
        fields.push((
            "priority",
            ImportValue::Integer(js_priority_to_u64(priority)),
        ));
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
        let items: Vec<ImportValue> = kw.iter().map(|s| ImportValue::String(s.clone())).collect();
        if !items.is_empty() {
            fields.push(("keywords", ImportValue::List(items)));
        }
    }

    let mut record = make_record(&fields);

    // r[impl model.import.jscalendar.vendor]
    // Vendor (unknown) properties.
    for (key, val) in event.vendor_property_iter() {
        record.insert(key.to_string(), translate_json_value(val));
    }

    record
}

// r[impl model.import.jscalendar.task]
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
        fields.push((
            "priority",
            ImportValue::Integer(js_priority_to_u64(priority)),
        ));
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
        let items: Vec<ImportValue> = kw.iter().map(|s| ImportValue::String(s.clone())).collect();
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

// r[impl model.import.jscalendar.priority]
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
            let items: Vec<ImportValue> = arr.iter().map(translate_json_value).collect();
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
                ("minute", ImportValue::Integer(time.minute() as u8 as u64)),
                ("second", ImportValue::Integer(time.second() as u8 as u64)),
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
    exact: Option<&ExactDuration>,
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

fn datetime_to_total_seconds<M>(dt: &DateTime<M>) -> u64 {
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

// r[impl model.import.icalendar.status]
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

// r[impl model.import.icalendar.priority]
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

    /// Helper to extract the calendar record and its entries from an iCal result.
    fn split_ical_result(result: &ImportValue) -> (&ImportRecord, Vec<&ImportRecord>) {
        let calendars = match result {
            ImportValue::List(items) => items,
            _ => panic!("expected list, got {result:?}"),
        };
        assert_eq!(calendars.len(), 1, "expected singleton list");
        let cal = match &calendars[0] {
            ImportValue::Record(r) => r,
            _ => panic!("expected record"),
        };
        assert_eq!(
            get_field(cal, "type"),
            &ImportValue::String("calendar".into()),
        );
        let entries: Vec<&ImportRecord> = match get_field(cal, "entries") {
            ImportValue::List(items) => items
                .iter()
                .map(|item| match item {
                    ImportValue::Record(r) => r,
                    _ => panic!("expected record"),
                })
                .collect(),
            _ => panic!("expected entries list"),
        };
        (cal, entries)
    }

    // ── iCalendar tests ──────────────────────────────────────

    // r[verify model.import.icalendar.components]
    // r[verify model.import.icalendar.event]
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
        let (cal, entries) = split_ical_result(&result);

        // Calendar record.
        assert_eq!(
            get_field(cal, "prod_id"),
            &ImportValue::String("-//Test//Test//EN".into())
        );

        // Event record.
        assert_eq!(entries.len(), 1);
        let rec = entries[0];
        assert_eq!(get_field(rec, "type"), &ImportValue::String("event".into()));
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

    // r[verify model.import.icalendar.task]
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
        let (_cal, entries) = split_ical_result(&result);
        assert_eq!(entries.len(), 1);
        let rec = entries[0];
        assert_eq!(get_field(rec, "type"), &ImportValue::String("task".into()));
        assert_eq!(get_field(rec, "uid"), &ImportValue::String("todo-1".into()));
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

    // r[verify model.import.icalendar.event.duration-fallback]
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
        let (_cal, entries) = split_ical_result(&result);
        let rec = entries[0];
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

    // r[verify model.import.icalendar.calendar]
    #[test]
    fn ical_vcalendar_properties() {
        let ics = "\
BEGIN:VCALENDAR\r\n\
VERSION:2.0\r\n\
PRODID:-//Acme//Calendar//EN\r\n\
NAME:Work Calendar\r\n\
COLOR:indigo\r\n\
BEGIN:VEVENT\r\n\
UID:e1\r\n\
SUMMARY:Test\r\n\
DTSTART:20260101T000000\r\n\
END:VEVENT\r\n\
END:VCALENDAR\r\n";

        let result = translate_icalendar(ics).unwrap();
        let (cal, _entries) = split_ical_result(&result);
        assert_eq!(
            get_field(cal, "prod_id"),
            &ImportValue::String("-//Acme//Calendar//EN".into())
        );
        assert_eq!(
            get_field(cal, "name"),
            &ImportValue::String("Work Calendar".into())
        );
        assert_eq!(
            get_field(cal, "color"),
            &ImportValue::String("indigo".into())
        );
    }

    // r[verify model.import.icalendar.extension]
    // r[verify model.import.preserve]
    #[test]
    fn ical_x_properties_on_event() {
        let ics = "\
BEGIN:VCALENDAR\r\n\
VERSION:2.0\r\n\
PRODID:-//Test//Test//EN\r\n\
BEGIN:VEVENT\r\n\
UID:xprop-test\r\n\
SUMMARY:Test Event\r\n\
DTSTART:20260101T000000\r\n\
X-CUSTOM-FIELD:hello world\r\n\
END:VEVENT\r\n\
END:VCALENDAR\r\n";

        let result = translate_icalendar(ics).unwrap();
        let (_cal, entries) = split_ical_result(&result);
        let rec = entries[0];
        assert_eq!(
            get_field(rec, "x_custom_field"),
            &ImportValue::String("hello world".into())
        );
    }

    #[test]
    fn ical_expanded_event_properties() {
        let ics = "\
BEGIN:VCALENDAR\r\n\
VERSION:2.0\r\n\
PRODID:-//Test//Test//EN\r\n\
BEGIN:VEVENT\r\n\
UID:expanded-1\r\n\
SUMMARY:Full Event\r\n\
DTSTART:20260315T140000\r\n\
DTSTAMP:20260101T120000Z\r\n\
CREATED:20260101T100000Z\r\n\
LAST-MODIFIED:20260102T080000Z\r\n\
CLASS:CONFIDENTIAL\r\n\
TRANSP:TRANSPARENT\r\n\
SEQUENCE:3\r\n\
GEO:48.856614;2.352222\r\n\
URL:https://example.com/event\r\n\
ORGANIZER:mailto:boss@example.com\r\n\
COMMENT:First comment\r\n\
COMMENT:Second comment\r\n\
ATTENDEE:mailto:alice@example.com\r\n\
ATTENDEE:mailto:bob@example.com\r\n\
END:VEVENT\r\n\
END:VCALENDAR\r\n";

        let result = translate_icalendar(ics).unwrap();
        let (_cal, entries) = split_ical_result(&result);
        let rec = entries[0];

        assert!(has_field(rec, "dtstamp"));
        assert!(has_field(rec, "created"));
        assert!(has_field(rec, "last_modified"));

        assert_eq!(
            get_field(rec, "class"),
            &ImportValue::String("confidential".into())
        );
        assert_eq!(
            get_field(rec, "transparency"),
            &ImportValue::String("transparent".into())
        );
        assert_eq!(get_field(rec, "sequence"), &ImportValue::SignedInteger(3));
        assert_eq!(
            get_field(rec, "url"),
            &ImportValue::String("https://example.com/event".into())
        );
        assert_eq!(
            get_field(rec, "organizer"),
            &ImportValue::String("mailto:boss@example.com".into())
        );

        // GEO.
        match get_field(rec, "geo") {
            ImportValue::Record(geo) => {
                assert!(has_field(geo, "latitude"));
                assert!(has_field(geo, "longitude"));
            }
            _ => panic!("expected geo record"),
        }

        // Comments.
        match get_field(rec, "comments") {
            ImportValue::List(items) => {
                assert_eq!(items.len(), 2);
                assert_eq!(items[0], ImportValue::String("First comment".into()));
                assert_eq!(items[1], ImportValue::String("Second comment".into()));
            }
            _ => panic!("expected comments list"),
        }

        // Attendees.
        match get_field(rec, "attendees") {
            ImportValue::List(items) => {
                assert_eq!(items.len(), 2);
                assert_eq!(
                    items[0],
                    ImportValue::String("mailto:alice@example.com".into())
                );
            }
            _ => panic!("expected attendees list"),
        }
    }

    // r[verify model.import.icalendar.rrule]
    #[test]
    fn ical_rrule_event() {
        let ics = "\
BEGIN:VCALENDAR\r\n\
VERSION:2.0\r\n\
PRODID:-//Test//Test//EN\r\n\
BEGIN:VEVENT\r\n\
UID:rrule-1\r\n\
SUMMARY:Weekly Meeting\r\n\
DTSTART:20260315T140000\r\n\
RRULE:FREQ=WEEKLY;COUNT=10;BYDAY=MO,WE,FR\r\n\
END:VEVENT\r\n\
END:VCALENDAR\r\n";

        let result = translate_icalendar(ics).unwrap();
        let (_cal, entries) = split_ical_result(&result);
        let rec = entries[0];

        assert!(has_field(rec, "recur"));
        match get_field(rec, "recur") {
            ImportValue::Record(rrule) => {
                assert_eq!(
                    get_field(rrule, "frequency"),
                    &ImportValue::String("weekly".into())
                );
                assert_eq!(get_field(rrule, "count"), &ImportValue::Integer(10));
                assert!(has_field(rrule, "by_day"));
            }
            _ => panic!("expected recur record"),
        }
    }

    // r[verify model.import.icalendar.status]
    #[test]
    fn ical_todo_completed() {
        let ics = "\
BEGIN:VCALENDAR\r\n\
VERSION:2.0\r\n\
PRODID:-//Test//Test//EN\r\n\
BEGIN:VTODO\r\n\
UID:todo-completed-1\r\n\
SUMMARY:Done task\r\n\
STATUS:COMPLETED\r\n\
COMPLETED:20260301T120000Z\r\n\
END:VTODO\r\n\
END:VCALENDAR\r\n";

        let result = translate_icalendar(ics).unwrap();
        let (_cal, entries) = split_ical_result(&result);
        let rec = entries[0];
        assert_eq!(
            get_field(rec, "status"),
            &ImportValue::String("completed".into())
        );
        assert!(has_field(rec, "completed"));
    }

    // ── JSCalendar tests ─────────────────────────────────────

    // r[verify model.import.jscalendar.event]
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
        assert_eq!(get_field(rec, "type"), &ImportValue::String("event".into()));
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

    // r[verify model.import.jscalendar.task]
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
        assert_eq!(get_field(rec, "type"), &ImportValue::String("task".into()));
        assert_eq!(get_field(rec, "uid"), &ImportValue::String("task-1".into()));
        assert!(has_field(rec, "due"));
    }

    // r[verify model.import.jscalendar.vendor]
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

    // r[verify model.import.jscalendar.types]
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

    // r[verify model.import.icalendar.priority]
    #[test]
    fn ical_event_priority() {
        let ics = "\
BEGIN:VCALENDAR\r\n\
VERSION:2.0\r\n\
PRODID:-//Test//EN\r\n\
BEGIN:VEVENT\r\n\
UID:prio-ev1\r\n\
SUMMARY:Priority Event\r\n\
DTSTART:20260315T120000\r\n\
DURATION:PT1H\r\n\
PRIORITY:1\r\n\
END:VEVENT\r\n\
END:VCALENDAR\r\n";

        let result = translate_icalendar(ics).unwrap();
        let (_, entries) = split_ical_result(&result);
        assert_eq!(entries.len(), 1);
        assert_eq!(*get_field(entries[0], "priority"), ImportValue::Integer(1));
    }

    // r[verify model.import.jscalendar.priority]
    #[test]
    fn jscal_event_priority() {
        let json = r#"{
            "@type": "Event",
            "uid": "prio-js-1",
            "title": "Priority Event",
            "start": "2026-03-15T12:00:00",
            "duration": "PT1H",
            "priority": 3
        }"#;

        let result = translate_jscalendar(json).unwrap();
        match &result {
            ImportValue::Record(r) => {
                assert_eq!(*get_field(r, "priority"), ImportValue::Integer(3));
            }
            _ => panic!("expected record"),
        }
    }
}
