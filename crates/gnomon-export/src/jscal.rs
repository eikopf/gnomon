//! JSCalendar export: ImportValue → jscalendar model → JSON (RFC 9553).

use std::collections::{HashMap, HashSet};
use std::num::NonZero;
use std::str::FromStr;

use gnomon_import::{ImportRecord, ImportValue};
use jscalendar::json::{IntoJson, TryFromJson, UnsignedInt};
use jscalendar::model::object::{
    Event, Group, Link, Location, Participant, PatchObject, Relation, ReplyTo, Task,
    TaskOrEvent, TaskParticipant, VirtualLocation,
};
use jscalendar::model::rrule::{
    ByMonthDayRule, ByPeriodDayRules, CoreByRules, FreqByRules, Interval, RRule, Termination,
};
use jscalendar::model::set::{
    Color, EventStatus, FreeBusyStatus, LinkRelation, ParticipantRole, Percent, Priority, Privacy,
    TaskProgress,
};
use jscalendar::model::string::{CalAddress, GeoUri, Id, Uri};
use jscalendar::model::time::{
    Date, DateTime, Day, Duration, ExactDuration, Hour, Local, Minute, Month, NominalDuration,
    Second, Time, TimeFormat, Utc, Weekday, Year,
};
use calico::model::primitive::DateTimeOrDate;
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
            match build_entry(record, warnings) {
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
    let vendor = collect_vendor_properties(calendar, GROUP_KNOWN, "calendar", warnings);
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

fn build_entry(
    record: &ImportRecord,
    warnings: &mut Vec<String>,
) -> Result<TaskOrEvent<Json>, String> {
    let entry_type = get_str(record, "type").unwrap_or("event");
    match entry_type {
        // r[impl model.export.jscalendar.event]
        "event" => build_event(record, warnings).map(TaskOrEvent::Event),
        // r[impl model.export.jscalendar.task]
        "task" => build_task(record, warnings).map(TaskOrEvent::Task),
        other => Err(format!("unknown entry type: {other}")),
    }
}

// ── Event builder ────────────────────────────────────────────

fn build_event(record: &ImportRecord, warnings: &mut Vec<String>) -> Result<Event<Json>, String> {
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
        event.set_status(
            Token::<EventStatus, Box<str>>::from_str(s)
                .map_err(|e| format!("invalid status: {e}"))?,
        )
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
        event.set_privacy(
            Token::<Privacy, Box<str>>::from_str(s).map_err(|e| format!("invalid privacy: {e}"))?,
        )
    }
    if let Some(s) = get_str(record, "free_busy_status") {
        event.set_free_busy_status(
            Token::<FreeBusyStatus, Box<str>>::from_str(s)
                .map_err(|e| format!("invalid free_busy_status: {e}"))?,
        )
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

    // Metadata properties.
    if let Some(dt) = get_utc_datetime(record, "created") {
        event.set_created(dt);
    }
    if let Some(dt) = get_utc_datetime(record, "updated") {
        event.set_updated(dt);
    }
    if let Some(n) = get_u64(record, "sequence")
        && let Some(uint) = UnsignedInt::new(n)
    {
        event.set_sequence(uint);
    }
    if let Some(dt) = get_datetime(record, "recurrence_id") {
        event.set_recurrence_id(dt);
    }

    // Location properties.
    if let Some(locations) = build_locations(record) {
        event.set_locations(locations);
    }
    if let Some(vl) = build_virtual_locations(record) {
        event.set_virtual_locations(vl);
    }

    // Links (url, attachments, images).
    if let Some(links) = build_links(record) {
        event.set_links(links);
    }

    // Participants and organizer.
    if let Some(participants) = build_participants(record) {
        event.set_participants(participants);
    }
    if let Some(reply_to) = build_reply_to(record) {
        event.set_reply_to(reply_to);
    }

    // Relations.
    if let Some(related) = build_related_to(record) {
        event.set_related_to(related);
    }

    // Recurrence rules.
    if let Some(rules) = build_recurrence_rules(record) {
        event.set_recurrence_rules(rules);
    }

    // Recurrence overrides (exdates, rdates).
    if let Some(overrides) = build_recurrence_overrides(record) {
        event.set_recurrence_overrides(overrides);
    }

    // r[impl model.export.jscalendar.vendor+2]
    let vendor = collect_vendor_properties(record, EVENT_KNOWN, "event", warnings);
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
    "created",
    "updated",
    "sequence",
    "recurrence_id",
    "url",
    "location",
    "geo",
    "organizer",
    "attendees",
    "attachments",
    "images",
    "conferences",
    "related_to",
    "recur",
    "exdates",
    "rdates",
    "request_statuses",
    "end_time_zone",
    "recurrence_id_time_zone",
];

// ── Task builder ─────────────────────────────────────────────

fn build_task(record: &ImportRecord, warnings: &mut Vec<String>) -> Result<Task<Json>, String> {
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
        task.set_progress(
            Token::<TaskProgress, Box<str>>::from_str(s)
                .map_err(|e| format!("invalid progress: {e}"))?,
        )
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
        task.set_privacy(
            Token::<Privacy, Box<str>>::from_str(s).map_err(|e| format!("invalid privacy: {e}"))?,
        );
    }
    if let Some(s) = get_str(record, "free_busy_status") {
        task.set_free_busy_status(
            Token::<FreeBusyStatus, Box<str>>::from_str(s)
                .map_err(|e| format!("invalid free_busy_status: {e}"))?,
        );
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

    // Metadata properties.
    if let Some(dt) = get_utc_datetime(record, "created") {
        task.set_created(dt);
    }
    if let Some(dt) = get_utc_datetime(record, "updated") {
        task.set_updated(dt);
    }
    if let Some(n) = get_u64(record, "sequence")
        && let Some(uint) = UnsignedInt::new(n)
    {
        task.set_sequence(uint);
    }
    if let Some(dt) = get_datetime(record, "recurrence_id") {
        task.set_recurrence_id(dt);
    }

    // Location properties.
    if let Some(locations) = build_locations(record) {
        task.set_locations(locations);
    }
    if let Some(vl) = build_virtual_locations(record) {
        task.set_virtual_locations(vl);
    }

    // Links (url, attachments, images).
    if let Some(links) = build_links(record) {
        task.set_links(links);
    }

    // Participants and organizer.
    if let Some(participants) = build_task_participants(record) {
        task.set_participants(participants);
    }
    if let Some(reply_to) = build_reply_to(record) {
        task.set_reply_to(reply_to);
    }

    // Relations.
    if let Some(related) = build_related_to(record) {
        task.set_related_to(related);
    }

    // Recurrence rules.
    if let Some(rules) = build_recurrence_rules(record) {
        task.set_recurrence_rules(rules);
    }

    // Recurrence overrides (exdates, rdates).
    if let Some(overrides) = build_recurrence_overrides(record) {
        task.set_recurrence_overrides(overrides);
    }

    // r[impl model.export.jscalendar.vendor+2]
    let vendor = collect_vendor_properties(record, TASK_KNOWN, "task", warnings);
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
    "created",
    "updated",
    "sequence",
    "recurrence_id",
    "completed",
    "url",
    "location",
    "geo",
    "organizer",
    "attendees",
    "attachments",
    "images",
    "conferences",
    "related_to",
    "recur",
    "exdates",
    "rdates",
    "request_statuses",
    "recurrence_id_time_zone",
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

// ── JSCalendar structured property helpers ───────────────────

fn get_utc_datetime(record: &ImportRecord, key: &str) -> Option<DateTime<Utc>> {
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
        marker: Utc,
    })
}

fn get_record<'a>(record: &'a ImportRecord, key: &str) -> Option<&'a ImportRecord> {
    match record.get(key)? {
        ImportValue::Record(r) => Some(r),
        _ => None,
    }
}

fn get_list<'a>(record: &'a ImportRecord, key: &str) -> Option<&'a Vec<ImportValue>> {
    match record.get(key)? {
        ImportValue::List(l) => Some(l),
        _ => None,
    }
}

fn get_string_list(record: &ImportRecord, key: &str) -> Option<Vec<String>> {
    let ImportValue::List(items) = record.get(key)? else {
        return None;
    };
    let list: Vec<String> = items
        .iter()
        .filter_map(|v| match v {
            ImportValue::String(s) => Some(s.clone()),
            _ => None,
        })
        .collect();
    if list.is_empty() { None } else { Some(list) }
}

fn make_id(s: &str) -> Option<Box<Id>> {
    Box::<Id>::try_from_json(Json::String(s.to_string())).ok()
}

fn build_locations(record: &ImportRecord) -> Option<HashMap<Box<Id>, Location<Json>>> {
    let location_str = get_str(record, "location");
    let geo_record = get_record(record, "geo");

    if location_str.is_none() && geo_record.is_none() {
        return None;
    }

    let mut locations = HashMap::new();
    let mut loc = Location::new();

    if let Some(name) = location_str {
        loc.set_name(name.to_string());
    }
    if let Some(geo) = geo_record {
        let lat = get_str(geo, "latitude").unwrap_or("0");
        let lon = get_str(geo, "longitude").unwrap_or("0");
        let geo_uri = format!("geo:{lat},{lon}");
        if let Ok(g) = GeoUri::new(&geo_uri) {
            loc.set_coordinates(g.into());
        }
    }

    locations.insert(make_id("1")?, loc);
    Some(locations)
}

fn build_virtual_locations(record: &ImportRecord) -> Option<HashMap<Box<Id>, VirtualLocation<Json>>> {
    let conferences = get_string_list(record, "conferences")?;
    let mut vl_map = HashMap::new();
    for (i, uri_str) in conferences.iter().enumerate() {
        if let Ok(uri) = Uri::new(uri_str) {
            let vloc = VirtualLocation::new(uri.into());
            let id = make_id(&(i + 1).to_string())?;
            vl_map.insert(id, vloc);
        }
    }
    if vl_map.is_empty() { None } else { Some(vl_map) }
}

fn build_links(record: &ImportRecord) -> Option<HashMap<Box<Id>, Link<Json>>> {
    let mut links = HashMap::new();
    let mut id_counter = 1u32;

    // url → link
    if let Some(url_str) = get_str(record, "url")
        && let Ok(uri) = Uri::new(url_str) {
            let link = Link::new(uri.into());
            if let Some(id) = make_id(&id_counter.to_string()) {
                links.insert(id, link);
                id_counter += 1;
            }
        }

    // attachments → links
    if let Some(items) = get_list(record, "attachments") {
        for item in items {
            match item {
                ImportValue::String(uri_str) => {
                    if let Ok(uri) = Uri::new(uri_str) {
                        let link = Link::new(uri.into());
                        if let Some(id) = make_id(&id_counter.to_string()) {
                            links.insert(id, link);
                            id_counter += 1;
                        }
                    }
                }
                ImportValue::Record(attach_rec) => {
                    if let Some(data) = get_str(attach_rec, "data") {
                        // base64 data → data URI
                        let uri_str =
                            format!("data:application/octet-stream;base64,{data}");
                        if let Ok(uri) = Uri::new(&uri_str) {
                            let link = Link::new(uri.into());
                            if let Some(id) = make_id(&id_counter.to_string()) {
                                links.insert(id, link);
                                id_counter += 1;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // images → links with rel="icon"
    if let Some(items) = get_list(record, "images") {
        for item in items {
            let uri_str = match item {
                ImportValue::String(s) => Some(s.as_str()),
                _ => None,
            };
            if let Some(uri_str) = uri_str
                && let Ok(uri) = Uri::new(uri_str) {
                    let mut link = Link::new(uri.into());
                    link.set_relation(LinkRelation::Icon);
                    if let Some(id) = make_id(&id_counter.to_string()) {
                        links.insert(id, link);
                        id_counter += 1;
                    }
                }
        }
    }

    if links.is_empty() { None } else { Some(links) }
}

fn build_participants(record: &ImportRecord) -> Option<HashMap<Box<Id>, Participant<Json>>> {
    let attendees = get_string_list(record, "attendees")?;
    let mut participants = HashMap::new();

    for (i, uri_str) in attendees.iter().enumerate() {
        let mut participant = Participant::new();
        // Set the role to attendee.
        let mut roles = HashSet::new();
        roles.insert(Token::Known(ParticipantRole::Attendee));
        participant.set_roles(roles);
        // If the URI is a mailto: URI, set the sendTo.imip field.
        if let Ok(cal_addr) = CalAddress::new(uri_str) {
            let mut send_to = jscalendar::model::object::SendToParticipant::new();
            send_to.set_imip(cal_addr.into());
            participant.set_send_to(send_to);
        }
        if let Some(id) = make_id(&(i + 1).to_string()) {
            participants.insert(id, participant);
        }
    }

    if participants.is_empty() {
        None
    } else {
        Some(participants)
    }
}

fn build_task_participants(
    record: &ImportRecord,
) -> Option<HashMap<Box<Id>, TaskParticipant<Json>>> {
    let attendees = get_string_list(record, "attendees")?;
    let mut participants = HashMap::new();

    for (i, uri_str) in attendees.iter().enumerate() {
        let mut participant = TaskParticipant::new();
        let mut roles = HashSet::new();
        roles.insert(Token::Known(ParticipantRole::Attendee));
        participant.set_roles(roles);
        if let Ok(cal_addr) = CalAddress::new(uri_str) {
            let mut send_to = jscalendar::model::object::SendToParticipant::new();
            send_to.set_imip(cal_addr.into());
            participant.set_send_to(send_to);
        }
        if let Some(id) = make_id(&(i + 1).to_string()) {
            participants.insert(id, participant);
        }
    }

    if participants.is_empty() {
        None
    } else {
        Some(participants)
    }
}

fn build_reply_to(record: &ImportRecord) -> Option<ReplyTo> {
    let organizer_uri = get_str(record, "organizer")?;
    if let Ok(cal_addr) = CalAddress::new(organizer_uri) {
        let mut reply_to = ReplyTo::new();
        reply_to.set_imip(cal_addr.into());
        Some(reply_to)
    } else {
        None
    }
}

fn build_related_to(record: &ImportRecord) -> Option<HashMap<Box<Uid>, Relation<Json>>> {
    let uids = get_string_list(record, "related_to")?;
    let mut map = HashMap::new();
    for uid_str in &uids {
        if let Ok(uid) = Uid::new(uid_str) {
            let relation = Relation::new(HashSet::new());
            map.insert(uid.into(), relation);
        }
    }
    if map.is_empty() { None } else { Some(map) }
}

fn build_recurrence_rules(record: &ImportRecord) -> Option<Vec<RRule>> {
    let recur_rec = get_record(record, "recur")?;
    let rrule = build_jscal_rrule(recur_rec)?;
    Some(vec![rrule])
}

fn build_jscal_rrule(recur: &ImportRecord) -> Option<RRule> {
    let freq_str = get_str(recur, "frequency")?;
    let empty_period_day = ByPeriodDayRules {
        by_month_day: None,
        by_year_day: None,
    };
    let freq_by_rules = match freq_str {
        "secondly" => FreqByRules::Secondly(empty_period_day),
        "minutely" => FreqByRules::Minutely(empty_period_day.clone()),
        "hourly" => FreqByRules::Hourly(empty_period_day.clone()),
        "daily" => FreqByRules::Daily(ByMonthDayRule { by_month_day: None }),
        "weekly" => FreqByRules::Weekly,
        "monthly" => FreqByRules::Monthly(ByMonthDayRule { by_month_day: None }),
        "yearly" => FreqByRules::Yearly(Default::default()),
        _ => return None,
    };

    let mut rrule = RRule {
        freq: freq_by_rules,
        core_by_rules: CoreByRules::default(),
        interval: None,
        termination: None,
        week_start: None,
    };

    if let Some(interval) = get_u64(recur, "interval")
        && let Some(nz) = NonZero::new(interval) {
            rrule.interval = Some(Interval::new(nz));
        }

    if let Some(count) = get_u64(recur, "count") {
        rrule.termination = Some(Termination::Count(count));
    } else if let Some(until_dt) = get_datetime(recur, "until") {
        rrule.termination = Some(Termination::Until(
            DateTimeOrDate::DateTime(DateTime {
                date: until_dt.date,
                time: until_dt.time,
                marker: TimeFormat::Local,
            }),
        ));
    }

    if let Some(wkst) = get_str(recur, "week_start") {
        rrule.week_start = match wkst {
            "mo" => Some(Weekday::Monday),
            "tu" => Some(Weekday::Tuesday),
            "we" => Some(Weekday::Wednesday),
            "th" => Some(Weekday::Thursday),
            "fr" => Some(Weekday::Friday),
            "sa" => Some(Weekday::Saturday),
            "su" => Some(Weekday::Sunday),
            _ => None,
        };
    }

    Some(rrule)
}

fn build_recurrence_overrides(
    record: &ImportRecord,
) -> Option<HashMap<DateTime<Local>, PatchObject<Json>>> {
    let mut overrides: HashMap<DateTime<Local>, PatchObject<Json>> = HashMap::new();

    // exdates → overrides with excluded: true
    if let Some(exdates) = get_list(record, "exdates") {
        for exdate in exdates {
            if let ImportValue::Record(dt_rec) = exdate
                && let Some(dt) = record_to_local_datetime(dt_rec) {
                    let mut patch_map = Map::new();
                    patch_map.insert("excluded".to_string(), Json::Bool(true));
                    if let Ok(patch) = PatchObject::try_from_json(Json::Object(patch_map)) {
                        overrides.insert(dt, patch);
                    }
                }
        }
    }

    // rdates → overrides with empty patch (just marks the date as an occurrence)
    if let Some(rdates) = get_list(record, "rdates") {
        for rdate in rdates {
            if let ImportValue::Record(dt_rec) = rdate
                && let Some(dt) = record_to_local_datetime(dt_rec) {
                    overrides.entry(dt).or_default();
                }
        }
    }

    if overrides.is_empty() {
        None
    } else {
        Some(overrides)
    }
}

fn record_to_local_datetime(dt_record: &ImportRecord) -> Option<DateTime<Local>> {
    let date_rec = get_record(dt_record, "date")?;
    let time_rec = get_record(dt_record, "time");

    let year = get_u64(date_rec, "year")?;
    let month = get_u64(date_rec, "month")?;
    let day = get_u64(date_rec, "day")?;

    let (hour, minute, second) = if let Some(time_rec) = time_rec {
        (
            get_u64(time_rec, "hour").unwrap_or(0),
            get_u64(time_rec, "minute").unwrap_or(0),
            get_u64(time_rec, "second").unwrap_or(0),
        )
    } else {
        (0, 0, 0)
    };

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

// ── Vendor properties ────────────────────────────────────────

/// Collect record fields not in the known set into a JSON object for vendor_property.
///
/// Fields that do not match the JSCalendar vendor property naming convention
/// (containing `:`) produce a warning, since they may be misspellings of
/// mapped properties.
// r[impl model.export.jscalendar.vendor+2]
fn collect_vendor_properties(
    record: &ImportRecord,
    known: &[&str],
    kind: &str,
    warnings: &mut Vec<String>,
) -> Map<String, Json> {
    let mut obj = Map::new();
    for (key, value) in record {
        if known.contains(&key.as_str()) {
            continue;
        }
        if !key.contains(':') {
            warnings.push(format!(
                "unrecognised non-vendor field '{key}' on {kind} record"
            ));
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

    // r[verify model.export.jscalendar.vendor+2]
    #[test]
    fn non_vendor_unknown_fields_produce_warnings() {
        let cal = make_cal("550e8400-e29b-41d4-a716-446655440000");
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            (
                "uid",
                ImportValue::String("a8df6573-0474-496d-8496-033ad45d7fea".into()),
            ),
            ("start", make_datetime(2026, 1, 1, 0, 0, 0)),
            // This is a valid vendor property (contains ':')
            (
                "com.example:custom",
                ImportValue::String("vendor-value".into()),
            ),
            // This is NOT a vendor property (no ':') and not a known field
            ("x_custom_field", ImportValue::String("legacy".into())),
        ]));

        let mut warnings = vec![];
        let mut result = String::new();
        emit_jscalendar(&mut result, &cal, &[event], &mut warnings).unwrap();

        // The non-vendor field should produce a warning.
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("x_custom_field") && w.contains("event")),
            "expected warning for x_custom_field, got: {warnings:?}"
        );
        // The vendor property should NOT produce a warning.
        assert!(
            !warnings
                .iter()
                .any(|w| w.contains("com.example:custom")),
            "vendor property should not produce a warning"
        );

        // Both should still be in the output.
        let parsed: Json = serde_json::from_str(&result).unwrap();
        let entries = parsed["entries"].as_array().unwrap();
        assert_eq!(entries[0]["com.example:custom"], "vendor-value");
        assert_eq!(entries[0]["x_custom_field"], "legacy");
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
