//! JSCalendar export: ImportValue → jscalendar model → JSON (RFC 9553).

use std::collections::{HashMap, HashSet};
use std::num::NonZero;
use std::str::FromStr;

use gnomon_import::{ImportRecord, ImportValue};
use jscalendar::json::{IntoJson, TryFromJson, UnsignedInt};
use jscalendar::model::object::{
    Event, Group, Link, Location, Participant, Relation, ReplyTo, Task, TaskOrEvent,
    TaskParticipant, VirtualLocation,
};
use jscalendar::model::rrule::{
    ByMonthDayRule, ByPeriodDayRules, CoreByRules, FreqByRules, HourSet, Interval, MinuteSet,
    MonthDay, MonthDaySet, MonthDaySetIndex, MonthSet, RRule, SecondSet, Termination, WeekNoSet,
    WeekNoSetIndex, WeekdayNum, YearDayNum, YearlyByRules,
};
use jscalendar::model::rrule::weekday_num_set::WeekdayNumSet;
use jscalendar::model::set::{
    Color, EventStatus, FreeBusyStatus, Method, ParticipantRole, Percent, Priority, Privacy,
    RelationValue, TaskProgress,
};
use jscalendar::model::string::{CalAddress, EmailAddr, GeoUri, Id, Uri};
use jscalendar::model::time::{
    Date, DateTime, Day, Duration, ExactDuration, Hour, Local, Minute, Month, NominalDuration,
    Second, Sign, Time, TimeFormat, Utc, Weekday, Year,
};
use serde_json::{Map, Value as Json};

use calendar_types::set::Token;
use calendar_types::string::Uid;
use calendar_types::time::IsoWeek;

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

    // r[impl model.export.jscalendar.vendor]
    // r[impl model.export.jscalendar.unknown]
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
        // r[impl model.export.jscalendar.event+2]
        "event" => build_event(record, warnings).map(TaskOrEvent::Event),
        // r[impl model.export.jscalendar.task+2]
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

    // ── Metadata properties ──────────────────────────────────
    if let Some(dt) = get_utc_datetime(record, "created") {
        event.set_created(dt);
    }
    // "last_modified" (iCal import) or "updated" (JSCal import) → JSCalendar `updated`
    if let Some(dt) = get_utc_datetime(record, "updated")
        .or_else(|| get_utc_datetime(record, "last_modified"))
    {
        event.set_updated(dt);
    }
    if let Some(n) = get_u64(record, "sequence") {
        if let Some(ui) = UnsignedInt::new(n) {
            event.set_sequence(ui);
        }
    }
    if let Some(s) = get_str(record, "method") {
        let m = Token::<Method, Box<str>>::from_str(s).unwrap();
        event.set_method(m);
    }

    // ── Recurrence properties ────────────────────────────────
    if let Some(dt) = get_datetime(record, "recurrence_id") {
        event.set_recurrence_id(dt);
    }
    if let Some(rules) = get_recurrence_rules(record) {
        event.set_recurrence_rules(rules);
    }
    if let Some(rules) = get_excluded_recurrence_rules(record) {
        event.set_excluded_recurrence_rules(rules);
    }

    // ── Location properties ──────────────────────────────────
    if let Some(locations) = build_locations(record) {
        event.set_locations(locations);
    }
    if let Some(vlocs) = build_virtual_locations(record) {
        event.set_virtual_locations(vlocs);
    }

    // ── Link properties ──────────────────────────────────────
    if let Some(links) = build_links(record) {
        event.set_links(links);
    }

    // ── Relation properties ──────────────────────────────────
    if let Some(related) = build_related_to(record) {
        event.set_related_to(related);
    }

    // ── Participant properties ───────────────────────────────
    if let Some(participants) = build_event_participants(record) {
        event.set_participants(participants);
    }

    // ── Scheduling properties ────────────────────────────────
    if let Some(reply_to) = build_reply_to(record) {
        event.set_reply_to(reply_to);
    }
    if let Some(rs) = build_request_status(record) {
        event.set_request_status(rs);
    }

    // r[impl model.export.jscalendar.vendor]
    // r[impl model.export.jscalendar.unknown]
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
    // Metadata
    "created",
    "updated",
    "last_modified",
    "sequence",
    "method",
    // Recurrence
    "recurrence_id",
    "recur",
    "recurrence_rules",
    "excluded_recurrence_rules",
    // Locations
    "location",
    "geo",
    "locations",
    "virtual_locations",
    // Links
    "url",
    "attachments",
    "links",
    // Relations
    "related_to",
    // Participants
    "organizer",
    "attendees",
    "participants",
    // Scheduling
    "reply_to",
    "request_status",
    "request_statuses",
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

    // ── Metadata properties ──────────────────────────────────
    if let Some(dt) = get_utc_datetime(record, "created") {
        task.set_created(dt);
    }
    if let Some(dt) = get_utc_datetime(record, "updated")
        .or_else(|| get_utc_datetime(record, "last_modified"))
    {
        task.set_updated(dt);
    }
    if let Some(n) = get_u64(record, "sequence") {
        if let Some(ui) = UnsignedInt::new(n) {
            task.set_sequence(ui);
        }
    }
    if let Some(s) = get_str(record, "method") {
        let m = Token::<Method, Box<str>>::from_str(s).unwrap();
        task.set_method(m);
    }

    // ── Recurrence properties ────────────────────────────────
    if let Some(dt) = get_datetime(record, "recurrence_id") {
        task.set_recurrence_id(dt);
    }
    if let Some(rules) = get_recurrence_rules(record) {
        task.set_recurrence_rules(rules);
    }
    if let Some(rules) = get_excluded_recurrence_rules(record) {
        task.set_excluded_recurrence_rules(rules);
    }

    // ── Location properties ──────────────────────────────────
    if let Some(locations) = build_locations(record) {
        task.set_locations(locations);
    }
    if let Some(vlocs) = build_virtual_locations(record) {
        task.set_virtual_locations(vlocs);
    }

    // ── Link properties ──────────────────────────────────────
    if let Some(links) = build_links(record) {
        task.set_links(links);
    }

    // ── Relation properties ──────────────────────────────────
    if let Some(related) = build_related_to(record) {
        task.set_related_to(related);
    }

    // ── Participant properties ───────────────────────────────
    if let Some(participants) = build_task_participants(record) {
        task.set_participants(participants);
    }

    // ── Scheduling properties ────────────────────────────────
    if let Some(reply_to) = build_reply_to(record) {
        task.set_reply_to(reply_to);
    }
    if let Some(rs) = build_request_status(record) {
        task.set_request_status(rs);
    }

    // r[impl model.export.jscalendar.vendor]
    // r[impl model.export.jscalendar.unknown]
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
    // Metadata
    "created",
    "updated",
    "last_modified",
    "sequence",
    "method",
    // Recurrence
    "recurrence_id",
    "recur",
    "recurrence_rules",
    "excluded_recurrence_rules",
    // Locations
    "location",
    "geo",
    "locations",
    "virtual_locations",
    // Links
    "url",
    "attachments",
    "links",
    // Relations
    "related_to",
    // Participants
    "organizer",
    "attendees",
    "participants",
    // Scheduling
    "reply_to",
    "request_status",
    "request_statuses",
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

fn get_i64(record: &ImportRecord, key: &str) -> Option<i64> {
    match record.get(key)? {
        ImportValue::Integer(n) => i64::try_from(*n).ok(),
        ImportValue::SignedInteger(n) => Some(*n),
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

/// Extract a UTC datetime from a record field.
///
/// Accepts the same nested `{ date: { year, month, day }, time: { hour, minute, second } }` format
/// as `get_datetime`, but produces a `DateTime<Utc>` instead.
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

// ── Recurrence rule builders ─────────────────────────────────

/// Extract recurrence rules from a record.
///
/// Handles both:
/// - `recurrence_rules`: list of rrule records (from JSCalendar import)
/// - `recur`: single rrule record (from iCalendar import)
fn get_recurrence_rules(record: &ImportRecord) -> Option<Vec<RRule>> {
    // Try JSCalendar-style list first.
    if let Some(ImportValue::List(rules)) = record.get("recurrence_rules") {
        let rrules: Vec<RRule> = rules
            .iter()
            .filter_map(|v| {
                if let ImportValue::Record(rec) = v {
                    record_to_rrule(rec)
                } else {
                    None
                }
            })
            .collect();
        if !rrules.is_empty() {
            return Some(rrules);
        }
    }
    // Fall back to iCalendar-style single record.
    if let Some(ImportValue::Record(rec)) = record.get("recur") {
        if let Some(rrule) = record_to_rrule(rec) {
            return Some(vec![rrule]);
        }
    }
    None
}

/// Extract excluded recurrence rules from a record.
fn get_excluded_recurrence_rules(record: &ImportRecord) -> Option<Vec<RRule>> {
    if let Some(ImportValue::List(rules)) = record.get("excluded_recurrence_rules") {
        let rrules: Vec<RRule> = rules
            .iter()
            .filter_map(|v| {
                if let ImportValue::Record(rec) = v {
                    record_to_rrule(rec)
                } else {
                    None
                }
            })
            .collect();
        if !rrules.is_empty() {
            return Some(rrules);
        }
    }
    None
}

/// Convert an ImportRecord representing a recurrence rule to an RRule.
fn record_to_rrule(rec: &ImportRecord) -> Option<RRule> {
    let freq_str = get_str(rec, "frequency")?;

    let mut core = CoreByRules::default();

    // BYSECOND
    if let Some(ImportValue::List(by_second)) = rec.get("by_second") {
        let mut set = SecondSet::default();
        for v in by_second {
            if let ImportValue::Integer(n) = v
                && let Ok(n8) = u8::try_from(*n)
                && let Some(sec) = jscalendar::model::rrule::Second::from_repr(n8)
            {
                set.set(sec);
            }
        }
        if set != SecondSet::default() {
            core.by_second = Some(set);
        }
    }

    // BYMINUTE
    if let Some(ImportValue::List(by_minute)) = rec.get("by_minute") {
        let mut set = MinuteSet::default();
        for v in by_minute {
            if let ImportValue::Integer(n) = v
                && let Ok(n8) = u8::try_from(*n)
                && let Some(min) = jscalendar::model::rrule::Minute::from_repr(n8)
            {
                set.set(min);
            }
        }
        if set != MinuteSet::default() {
            core.by_minute = Some(set);
        }
    }

    // BYHOUR
    if let Some(ImportValue::List(by_hour)) = rec.get("by_hour") {
        let mut set = HourSet::default();
        for v in by_hour {
            if let ImportValue::Integer(n) = v
                && let Ok(n8) = u8::try_from(*n)
                && let Some(h) = jscalendar::model::rrule::Hour::from_repr(n8)
            {
                set.set(h);
            }
        }
        if set != HourSet::default() {
            core.by_hour = Some(set);
        }
    }

    // BYMONTH
    if let Some(ImportValue::List(by_month)) = rec.get("by_month") {
        let mut set = MonthSet::default();
        for v in by_month {
            if let ImportValue::Integer(n) = v
                && let Ok(n8) = u8::try_from(*n)
                && let Ok(month) = Month::new(n8)
            {
                set.set(month);
            }
        }
        if set != MonthSet::default() {
            core.by_month = Some(set);
        }
    }

    // BYDAY
    if let Some(ImportValue::List(by_day)) = rec.get("by_day") {
        let mut set = WeekdayNumSet::default();
        for v in by_day {
            match v {
                ImportValue::String(s) => {
                    if let Some(wd) = str_to_weekday(s) {
                        set.insert(WeekdayNum {
                            weekday: wd,
                            ordinal: None,
                        });
                    }
                }
                ImportValue::Record(day_rec) => {
                    if let Some(day_str) = get_str(day_rec, "day")
                        && let Some(wd) = str_to_weekday(day_str)
                    {
                        let ordinal_i64 = get_i64(day_rec, "ordinal").unwrap_or(0);
                        let ord = if ordinal_i64 == 0 {
                            None
                        } else {
                            let sign = if ordinal_i64 < 0 {
                                Sign::Neg
                            } else {
                                Sign::Pos
                            };
                            u8::try_from(ordinal_i64.unsigned_abs())
                                .ok()
                                .and_then(IsoWeek::from_index)
                                .map(|w| (sign, w))
                        };
                        set.insert(WeekdayNum {
                            weekday: wd,
                            ordinal: ord,
                        });
                    }
                }
                _ => {}
            }
        }
        if !set.is_empty() {
            core.by_day = Some(set);
        }
    }

    // BYSETPOS
    if let Some(ImportValue::List(by_set_pos)) = rec.get("by_set_pos") {
        let mut set: std::collections::BTreeSet<YearDayNum> = std::collections::BTreeSet::new();
        for v in by_set_pos {
            let n = match v {
                ImportValue::Integer(n) => i64::try_from(*n).ok(),
                ImportValue::SignedInteger(n) => Some(*n),
                _ => None,
            };
            if let Some(n) = n
                && let Ok(abs) = u16::try_from(n.unsigned_abs())
                && let Some(ydn) =
                    YearDayNum::from_signed_index(if n < 0 { Sign::Neg } else { Sign::Pos }, abs)
            {
                set.insert(ydn);
            }
        }
        if !set.is_empty() {
            core.by_set_pos = Some(set);
        }
    }

    // Helpers for frequency-specific BY rules.
    let build_month_day_set = |rec: &ImportRecord| -> Option<MonthDaySet> {
        let ImportValue::List(by_month_day) = rec.get("by_month_day")? else {
            return None;
        };
        let mut set = MonthDaySet::default();
        for v in by_month_day {
            let n = match v {
                ImportValue::Integer(n) => i64::try_from(*n).ok(),
                ImportValue::SignedInteger(n) => Some(*n),
                _ => None,
            };
            if let Some(n) = n
                && let Ok(abs) = u8::try_from(n.unsigned_abs())
                && let Some(day) = MonthDay::from_repr(abs)
            {
                let sign = if n < 0 { Sign::Neg } else { Sign::Pos };
                let idx = MonthDaySetIndex::from_signed_month_day(sign, day);
                set.set(idx);
            }
        }
        if set == MonthDaySet::default() {
            None
        } else {
            Some(set)
        }
    };

    let build_year_day_set =
        |rec: &ImportRecord| -> Option<std::collections::BTreeSet<YearDayNum>> {
            let ImportValue::List(by_year_day) = rec.get("by_year_day")? else {
                return None;
            };
            let mut set: std::collections::BTreeSet<YearDayNum> =
                std::collections::BTreeSet::new();
            for v in by_year_day {
                let n = match v {
                    ImportValue::Integer(n) => i64::try_from(*n).ok(),
                    ImportValue::SignedInteger(n) => Some(*n),
                    _ => None,
                };
                if let Some(n) = n
                    && let Ok(abs) = u16::try_from(n.unsigned_abs())
                    && let Some(ydn) = YearDayNum::from_signed_index(
                        if n < 0 { Sign::Neg } else { Sign::Pos },
                        abs,
                    )
                {
                    set.insert(ydn);
                }
            }
            if set.is_empty() { None } else { Some(set) }
        };

    let build_week_no_set = |rec: &ImportRecord| -> Option<WeekNoSet> {
        let ImportValue::List(by_week_no) = rec.get("by_week_no")? else {
            return None;
        };
        let mut set = WeekNoSet::default();
        for v in by_week_no {
            let n = match v {
                ImportValue::Integer(n) => i64::try_from(*n).ok(),
                ImportValue::SignedInteger(n) => Some(*n),
                _ => None,
            };
            if let Some(n) = n
                && let Ok(abs) = u8::try_from(n.unsigned_abs())
                && let Some(week) = IsoWeek::from_index(abs)
            {
                let sign = if n < 0 { Sign::Neg } else { Sign::Pos };
                let idx = WeekNoSetIndex::from_signed_week(sign, week);
                set.set(idx);
            }
        }
        if set == WeekNoSet::default() {
            None
        } else {
            Some(set)
        }
    };

    let freq = match freq_str {
        "secondly" => FreqByRules::Secondly(ByPeriodDayRules {
            by_month_day: build_month_day_set(rec),
            by_year_day: build_year_day_set(rec),
        }),
        "minutely" => FreqByRules::Minutely(ByPeriodDayRules {
            by_month_day: build_month_day_set(rec),
            by_year_day: build_year_day_set(rec),
        }),
        "hourly" => FreqByRules::Hourly(ByPeriodDayRules {
            by_month_day: build_month_day_set(rec),
            by_year_day: build_year_day_set(rec),
        }),
        "daily" => FreqByRules::Daily(ByMonthDayRule {
            by_month_day: build_month_day_set(rec),
        }),
        "weekly" => FreqByRules::Weekly,
        "monthly" => FreqByRules::Monthly(ByMonthDayRule {
            by_month_day: build_month_day_set(rec),
        }),
        "yearly" => FreqByRules::Yearly(YearlyByRules {
            by_month_day: build_month_day_set(rec),
            by_year_day: build_year_day_set(rec),
            by_week_no: build_week_no_set(rec),
        }),
        _ => return None,
    };

    // INTERVAL
    let interval = get_u64(rec, "interval").and_then(|n| NonZero::new(n).map(Interval::new));

    // TERMINATION (COUNT or UNTIL)
    let termination = if let Some(count) = get_u64(rec, "count") {
        Some(Termination::Count(count))
    } else if let Some(dt) = get_datetime(rec, "until") {
        Some(Termination::Until(
            rfc5545_types::time::DateTimeOrDate::DateTime(DateTime {
                date: dt.date,
                time: dt.time,
                marker: TimeFormat::Local,
            }),
        ))
    } else {
        None
    };

    // WKST
    let week_start = get_str(rec, "week_start").and_then(str_to_weekday);

    Some(RRule {
        freq,
        core_by_rules: core,
        interval,
        termination,
        week_start,
    })
}

/// Convert a string to a Weekday.
fn str_to_weekday(s: &str) -> Option<Weekday> {
    match s {
        "monday" | "MO" => Some(Weekday::Monday),
        "tuesday" | "TU" => Some(Weekday::Tuesday),
        "wednesday" | "WE" => Some(Weekday::Wednesday),
        "thursday" | "TH" => Some(Weekday::Thursday),
        "friday" | "FR" => Some(Weekday::Friday),
        "saturday" | "SA" => Some(Weekday::Saturday),
        "sunday" | "SU" => Some(Weekday::Sunday),
        _ => None,
    }
}

// ── Location builders ────────────────────────────────────────

/// Build JSCalendar locations from record fields.
///
/// Handles:
/// - `locations`: passthrough map of location records (from JSCalendar import)
/// - `location` + `geo`: iCalendar-style fields combined into a single location entry
fn build_locations(record: &ImportRecord) -> Option<HashMap<Box<Id>, Location<Json>>> {
    // Try JSCalendar-style locations map first (passthrough as JSON).
    if let Some(ImportValue::Record(locs)) = record.get("locations") {
        let mut map = HashMap::new();
        for (id_str, val) in locs {
            if let Ok(id) = Id::new(id_str) {
                if let ImportValue::Record(loc_rec) = val {
                    let mut loc = Location::new();
                    if let Some(name) = get_str(loc_rec, "name") {
                        loc.set_name(name.to_string());
                    }
                    if let Some(desc) = get_str(loc_rec, "description") {
                        loc.set_description(desc.to_string());
                    }
                    if let Some(tz) = get_str(loc_rec, "time_zone") {
                        loc.set_time_zone(tz.to_string());
                    }
                    if let Some(coords) = get_str(loc_rec, "coordinates") {
                        if let Ok(geo) = GeoUri::new(coords) {
                            loc.set_coordinates(geo.into());
                        }
                    }
                    map.insert(id.into(), loc);
                }
            }
        }
        if !map.is_empty() {
            return Some(map);
        }
    }

    // Fall back to iCalendar-style location/geo.
    let has_location = record.contains_key("location");
    let has_geo = record.contains_key("geo");

    if !has_location && !has_geo {
        return None;
    }

    let mut loc = Location::new();

    if let Some(name) = get_str(record, "location") {
        loc.set_name(name.to_string());
    }

    if let Some(ImportValue::Record(geo_rec)) = record.get("geo") {
        let lat = get_str(geo_rec, "latitude").unwrap_or("0");
        let lon = get_str(geo_rec, "longitude").unwrap_or("0");
        let geo_uri_str = format!("geo:{lat},{lon}");
        if let Ok(geo) = GeoUri::new(&geo_uri_str) {
            loc.set_coordinates(geo.into());
        }
    }

    let id_str = "1";
    let id: Box<Id> = Id::new(id_str).unwrap().into();
    let mut map = HashMap::new();
    map.insert(id, loc);
    Some(map)
}

/// Build JSCalendar virtual locations from record fields.
fn build_virtual_locations(
    record: &ImportRecord,
) -> Option<HashMap<Box<Id>, VirtualLocation<Json>>> {
    let ImportValue::Record(vlocs) = record.get("virtual_locations")? else {
        return None;
    };
    let mut map = HashMap::new();
    for (id_str, val) in vlocs {
        if let Ok(id) = Id::new(id_str) {
            if let ImportValue::Record(vloc_rec) = val {
                let uri_str = get_str(vloc_rec, "uri").unwrap_or("https://example.com");
                if let Ok(uri) = Uri::new(uri_str) {
                    let mut vloc = VirtualLocation::new(uri.into());
                    if let Some(name) = get_str(vloc_rec, "name") {
                        vloc.set_name(name.to_string());
                    }
                    if let Some(desc) = get_str(vloc_rec, "description") {
                        vloc.set_description(desc.to_string());
                    }
                    map.insert(id.into(), vloc);
                }
            }
        }
    }
    if map.is_empty() { None } else { Some(map) }
}

// ── Link builders ────────────────────────────────────────────

/// Build JSCalendar links from record fields.
///
/// Handles:
/// - `links`: passthrough map of link records (from JSCalendar import)
/// - `url`: single URL → link entry
/// - `attachments`: list of attachment URIs → link entries
fn build_links(record: &ImportRecord) -> Option<HashMap<Box<Id>, Link<Json>>> {
    // Try JSCalendar-style links map first.
    if let Some(ImportValue::Record(links_map)) = record.get("links") {
        let mut map = HashMap::new();
        for (id_str, val) in links_map {
            if let Ok(id) = Id::new(id_str) {
                if let ImportValue::Record(link_rec) = val {
                    let href_str = get_str(link_rec, "href").unwrap_or("https://example.com");
                    if let Ok(href) = Uri::new(href_str) {
                        let link = Link::new(href.into());
                        map.insert(id.into(), link);
                    }
                }
            }
        }
        if !map.is_empty() {
            return Some(map);
        }
    }

    // Build from iCalendar-style url and attachments.
    let mut map: HashMap<Box<Id>, Link<Json>> = HashMap::new();
    let mut counter = 1u32;

    if let Some(url_str) = get_str(record, "url") {
        if let Ok(href) = Uri::new(url_str) {
            let id_str = counter.to_string();
            if let Ok(id) = Id::new(&id_str) {
                map.insert(id.into(), Link::new(href.into()));
                counter += 1;
            }
        }
    }

    if let Some(ImportValue::List(attachments)) = record.get("attachments") {
        for att in attachments {
            let uri_str = match att {
                ImportValue::String(s) => Some(s.as_str()),
                ImportValue::Record(rec) => get_str(rec, "uri").or_else(|| get_str(rec, "url")),
                _ => None,
            };
            if let Some(uri_str) = uri_str {
                if let Ok(href) = Uri::new(uri_str) {
                    let id_str = counter.to_string();
                    if let Ok(id) = Id::new(&id_str) {
                        map.insert(id.into(), Link::new(href.into()));
                        counter += 1;
                    }
                }
            }
        }
    }

    if map.is_empty() { None } else { Some(map) }
}

// ── Relation builder ─────────────────────────────────────────

/// Build JSCalendar relatedTo from record fields.
///
/// Handles:
/// - `related_to` as a map (from JSCalendar import passthrough)
/// - `related_to` as a list of UID strings (from iCalendar import)
fn build_related_to(record: &ImportRecord) -> Option<HashMap<Box<Uid>, Relation<Json>>> {
    match record.get("related_to")? {
        ImportValue::List(items) => {
            // iCalendar-style: list of UID strings.
            let mut map: HashMap<Box<Uid>, Relation<Json>> = HashMap::new();
            for item in items {
                if let ImportValue::String(s) = item {
                    if let Ok(uid) = Uid::new(s) {
                        let relation = Relation::new(HashSet::new());
                        map.insert(uid.into(), relation);
                    }
                }
            }
            if map.is_empty() { None } else { Some(map) }
        }
        ImportValue::Record(rel_map) => {
            // JSCalendar-style: map of uid → relation record.
            let mut map: HashMap<Box<Uid>, Relation<Json>> = HashMap::new();
            for (uid_str, val) in rel_map {
                if let Ok(uid) = Uid::new(uid_str) {
                    let mut relation_types = HashSet::new();
                    if let ImportValue::Record(rel_rec) = val {
                        if let Some(ImportValue::List(rels)) = rel_rec.get("relation") {
                            for rel in rels {
                                if let ImportValue::String(r) = rel {
                                    let rv =
                                        Token::<RelationValue, Box<str>>::from_str(r).unwrap();
                                    relation_types.insert(rv);
                                }
                            }
                        }
                    }
                    let relation = Relation::new(relation_types);
                    map.insert(uid.into(), relation);
                }
            }
            if map.is_empty() { None } else { Some(map) }
        }
        _ => None,
    }
}

// ── Participant builders ─────────────────────────────────────

/// Build JSCalendar participants for events from organizer/attendees/participants fields.
fn build_event_participants(
    record: &ImportRecord,
) -> Option<HashMap<Box<Id>, Participant<Json>>> {
    // Try JSCalendar-style participants map first.
    if let Some(ImportValue::Record(parts)) = record.get("participants") {
        let mut map = HashMap::new();
        for (id_str, val) in parts {
            if let Ok(id) = Id::new(id_str) {
                if let ImportValue::Record(part_rec) = val {
                    let mut participant = Participant::new();
                    if let Some(name) = get_str(part_rec, "name") {
                        participant.set_name(name.to_string());
                    }
                    if let Some(email) = get_str(part_rec, "email") {
                        if let Ok(addr) = EmailAddr::new(email) {
                            participant.set_email(addr.into());
                        }
                    }
                    map.insert(id.into(), participant);
                }
            }
        }
        if !map.is_empty() {
            return Some(map);
        }
    }

    // Build from iCalendar-style organizer + attendees.
    let has_organizer = record.contains_key("organizer");
    let has_attendees = record.contains_key("attendees");

    if !has_organizer && !has_attendees {
        return None;
    }

    let mut map: HashMap<Box<Id>, Participant<Json>> = HashMap::new();
    let mut counter = 1u32;

    if let Some(org_str) = get_str(record, "organizer") {
        let id_str = counter.to_string();
        if let Ok(id) = Id::new(&id_str) {
            let mut participant = Participant::new();
            // Set the organizer email if it's a mailto: URI.
            if let Some(email) = org_str.strip_prefix("mailto:") {
                if let Ok(addr) = EmailAddr::new(email) {
                    participant.set_email(addr.into());
                }
            } else {
                participant.set_name(org_str.to_string());
            }
            let mut roles = HashSet::new();
            roles.insert(Token::Known(ParticipantRole::Owner));
            participant.set_roles(roles);
            map.insert(id.into(), participant);
            counter += 1;
        }
    }

    if let Some(ImportValue::List(attendees)) = record.get("attendees") {
        for att in attendees {
            if let ImportValue::String(s) = att {
                let id_str = counter.to_string();
                if let Ok(id) = Id::new(&id_str) {
                    let mut participant = Participant::new();
                    if let Some(email) = s.strip_prefix("mailto:") {
                        if let Ok(addr) = EmailAddr::new(email) {
                            participant.set_email(addr.into());
                        }
                    } else {
                        participant.set_name(s.to_string());
                    }
                    let mut roles = HashSet::new();
                    roles.insert(Token::Known(ParticipantRole::Attendee));
                    participant.set_roles(roles);
                    map.insert(id.into(), participant);
                    counter += 1;
                }
            }
        }
    }

    if map.is_empty() { None } else { Some(map) }
}

/// Build JSCalendar participants for tasks from organizer/attendees/participants fields.
fn build_task_participants(
    record: &ImportRecord,
) -> Option<HashMap<Box<Id>, TaskParticipant<Json>>> {
    // Try JSCalendar-style participants map first.
    if let Some(ImportValue::Record(parts)) = record.get("participants") {
        let mut map = HashMap::new();
        for (id_str, val) in parts {
            if let Ok(id) = Id::new(id_str) {
                if let ImportValue::Record(part_rec) = val {
                    let mut participant = TaskParticipant::new();
                    if let Some(name) = get_str(part_rec, "name") {
                        participant.set_name(name.to_string());
                    }
                    if let Some(email) = get_str(part_rec, "email") {
                        if let Ok(addr) = EmailAddr::new(email) {
                            participant.set_email(addr.into());
                        }
                    }
                    map.insert(id.into(), participant);
                }
            }
        }
        if !map.is_empty() {
            return Some(map);
        }
    }

    // Build from iCalendar-style organizer + attendees.
    let has_organizer = record.contains_key("organizer");
    let has_attendees = record.contains_key("attendees");

    if !has_organizer && !has_attendees {
        return None;
    }

    let mut map: HashMap<Box<Id>, TaskParticipant<Json>> = HashMap::new();
    let mut counter = 1u32;

    if let Some(org_str) = get_str(record, "organizer") {
        let id_str = counter.to_string();
        if let Ok(id) = Id::new(&id_str) {
            let mut participant = TaskParticipant::new();
            // Set the organizer email if it's a mailto: URI.
            if let Some(email) = org_str.strip_prefix("mailto:") {
                if let Ok(addr) = EmailAddr::new(email) {
                    participant.set_email(addr.into());
                }
            } else {
                participant.set_name(org_str.to_string());
            }
            let mut roles = HashSet::new();
            roles.insert(Token::Known(ParticipantRole::Owner));
            participant.set_roles(roles);
            map.insert(id.into(), participant);
            counter += 1;
        }
    }

    if let Some(ImportValue::List(attendees)) = record.get("attendees") {
        for att in attendees {
            if let ImportValue::String(s) = att {
                let id_str = counter.to_string();
                if let Ok(id) = Id::new(&id_str) {
                    let mut participant = TaskParticipant::new();
                    if let Some(email) = s.strip_prefix("mailto:") {
                        if let Ok(addr) = EmailAddr::new(email) {
                            participant.set_email(addr.into());
                        }
                    } else {
                        participant.set_name(s.to_string());
                    }
                    let mut roles = HashSet::new();
                    roles.insert(Token::Known(ParticipantRole::Attendee));
                    participant.set_roles(roles);
                    map.insert(id.into(), participant);
                    counter += 1;
                }
            }
        }
    }

    if map.is_empty() { None } else { Some(map) }
}

// ── Scheduling builders ──────────────────────────────────────

/// Build JSCalendar replyTo from record fields.
fn build_reply_to(record: &ImportRecord) -> Option<ReplyTo> {
    let ImportValue::Record(rt_rec) = record.get("reply_to")? else {
        return None;
    };

    let mut reply_to = ReplyTo::new();
    if let Some(imip_str) = get_str(rt_rec, "imip") {
        if let Ok(addr) = CalAddress::new(imip_str) {
            reply_to.set_imip(addr.into());
        }
    }
    if let Some(web_str) = get_str(rt_rec, "web") {
        if let Ok(uri) = Uri::new(web_str) {
            reply_to.set_web(uri.into());
        }
    }

    Some(reply_to)
}

/// Build JSCalendar requestStatus from record fields.
///
/// Handles:
/// - `request_status`: single record (from JSCalendar)
/// - `request_statuses`: list of status strings (from iCalendar import, takes first)
fn build_request_status(
    record: &ImportRecord,
) -> Option<jscalendar::model::request_status::RequestStatus> {
    use jscalendar::model::request_status::RequestStatus;

    // Try JSCalendar-style request_status record first.
    if let Some(ImportValue::Record(rs_rec)) = record.get("request_status") {
        let code = parse_status_code(get_str(rs_rec, "code")?)?;
        let description = get_str(rs_rec, "description")
            .unwrap_or("")
            .to_string()
            .into_boxed_str();
        return Some(RequestStatus {
            code,
            description,
            exception_data: None,
        });
    }

    // Fall back to iCalendar-style request_statuses (list of status strings).
    // JSCalendar only supports a single requestStatus, so take the first.
    if let Some(ImportValue::List(statuses)) = record.get("request_statuses") {
        for status in statuses {
            if let ImportValue::String(s) = status {
                // Parse "code;description;data" format.
                let parts: Vec<&str> = s.splitn(3, ';').collect();
                if let Some(code_str) = parts.first()
                    && let Some(code) = parse_status_code(code_str)
                {
                    let description = parts
                        .get(1)
                        .unwrap_or(&"")
                        .to_string()
                        .into_boxed_str();
                    let exception_data = parts.get(2).map(|d| d.to_string().into_boxed_str());
                    return Some(RequestStatus {
                        code,
                        description,
                        exception_data,
                    });
                }
            }
        }
    }

    None
}

/// Parse a dotted status code like "2.0" or "3.1.2" into a StatusCode.
fn parse_status_code(
    s: &str,
) -> Option<jscalendar::model::request_status::StatusCode> {
    use jscalendar::model::request_status::{Class, StatusCode};

    let mut parts = s.split('.');
    let class_n: u8 = parts.next()?.parse().ok()?;
    let major: u8 = parts.next()?.parse().ok()?;
    let minor: Option<u8> = parts.next().and_then(|p| p.parse().ok());

    let class = Class::from_u8(class_n)?;
    Some(StatusCode {
        class,
        major,
        minor,
    })
}

// ── Vendor properties ────────────────────────────────────────

/// Collect record fields not in the known set into a JSON object for vendor_property.
///
/// Also emits warnings for fields that are not vendor-prefixed (i.e. don't contain a colon).
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
        // r[impl model.export.jscalendar.unknown]
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

    #[test]
    fn created_and_updated_exported() {
        let cal = make_cal("550e8400-e29b-41d4-a716-446655440000");
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            (
                "uid",
                ImportValue::String("a8df6573-0474-496d-8496-033ad45d7fea".into()),
            ),
            ("start", make_datetime(2026, 1, 1, 0, 0, 0)),
            ("created", make_datetime(2025, 6, 1, 12, 0, 0)),
            ("last_modified", make_datetime(2025, 12, 25, 8, 30, 0)),
        ]));

        let mut result = String::new();
        emit_jscalendar(&mut result, &cal, &[event], &mut vec![]).unwrap();
        let parsed: Json = serde_json::from_str(&result).unwrap();

        let entries = parsed["entries"].as_array().unwrap();
        assert_eq!(entries[0]["created"], "2025-06-01T12:00:00Z");
        assert_eq!(entries[0]["updated"], "2025-12-25T08:30:00Z");
    }

    #[test]
    fn sequence_exported() {
        let cal = make_cal("550e8400-e29b-41d4-a716-446655440000");
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            (
                "uid",
                ImportValue::String("a8df6573-0474-496d-8496-033ad45d7fea".into()),
            ),
            ("start", make_datetime(2026, 1, 1, 0, 0, 0)),
            ("sequence", ImportValue::Integer(3)),
        ]));

        let mut result = String::new();
        emit_jscalendar(&mut result, &cal, &[event], &mut vec![]).unwrap();
        let parsed: Json = serde_json::from_str(&result).unwrap();

        let entries = parsed["entries"].as_array().unwrap();
        assert_eq!(entries[0]["sequence"], 3);
    }

    #[test]
    fn recurrence_id_exported() {
        let cal = make_cal("550e8400-e29b-41d4-a716-446655440000");
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            (
                "uid",
                ImportValue::String("a8df6573-0474-496d-8496-033ad45d7fea".into()),
            ),
            ("start", make_datetime(2026, 1, 1, 0, 0, 0)),
            ("recurrence_id", make_datetime(2026, 1, 1, 0, 0, 0)),
        ]));

        let mut result = String::new();
        emit_jscalendar(&mut result, &cal, &[event], &mut vec![]).unwrap();
        let parsed: Json = serde_json::from_str(&result).unwrap();

        let entries = parsed["entries"].as_array().unwrap();
        assert_eq!(entries[0]["recurrenceId"], "2026-01-01T00:00:00");
    }

    #[test]
    fn location_and_geo_exported_as_locations() {
        let cal = make_cal("550e8400-e29b-41d4-a716-446655440000");
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            (
                "uid",
                ImportValue::String("a8df6573-0474-496d-8496-033ad45d7fea".into()),
            ),
            ("start", make_datetime(2026, 1, 1, 0, 0, 0)),
            ("location", ImportValue::String("Conference Room A".into())),
            (
                "geo",
                ImportValue::Record(make_record(&[
                    ("latitude", ImportValue::String("37.7749".into())),
                    ("longitude", ImportValue::String("-122.4194".into())),
                ])),
            ),
        ]));

        let mut result = String::new();
        emit_jscalendar(&mut result, &cal, &[event], &mut vec![]).unwrap();
        let parsed: Json = serde_json::from_str(&result).unwrap();

        let entries = parsed["entries"].as_array().unwrap();
        let locations = entries[0]["locations"].as_object().unwrap();
        assert_eq!(locations.len(), 1);
        let loc = locations.values().next().unwrap();
        assert_eq!(loc["name"], "Conference Room A");
        assert_eq!(loc["coordinates"], "geo:37.7749,-122.4194");
    }

    #[test]
    fn url_exported_as_link() {
        let cal = make_cal("550e8400-e29b-41d4-a716-446655440000");
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            (
                "uid",
                ImportValue::String("a8df6573-0474-496d-8496-033ad45d7fea".into()),
            ),
            ("start", make_datetime(2026, 1, 1, 0, 0, 0)),
            (
                "url",
                ImportValue::String("https://example.com/event".into()),
            ),
        ]));

        let mut result = String::new();
        emit_jscalendar(&mut result, &cal, &[event], &mut vec![]).unwrap();
        let parsed: Json = serde_json::from_str(&result).unwrap();

        let entries = parsed["entries"].as_array().unwrap();
        let links = entries[0]["links"].as_object().unwrap();
        assert_eq!(links.len(), 1);
        let link = links.values().next().unwrap();
        assert_eq!(link["@type"], "Link");
        assert_eq!(link["href"], "https://example.com/event");
    }

    #[test]
    fn related_to_list_exported() {
        let cal = make_cal("550e8400-e29b-41d4-a716-446655440000");
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            (
                "uid",
                ImportValue::String("a8df6573-0474-496d-8496-033ad45d7fea".into()),
            ),
            ("start", make_datetime(2026, 1, 1, 0, 0, 0)),
            (
                "related_to",
                ImportValue::List(vec![ImportValue::String(
                    "b9ef7684-1585-5a7e-b827-144b66551111".into(),
                )]),
            ),
        ]));

        let mut result = String::new();
        emit_jscalendar(&mut result, &cal, &[event], &mut vec![]).unwrap();
        let parsed: Json = serde_json::from_str(&result).unwrap();

        let entries = parsed["entries"].as_array().unwrap();
        let related = entries[0]["relatedTo"].as_object().unwrap();
        assert!(related.contains_key("b9ef7684-1585-5a7e-b827-144b66551111"));
    }

    #[test]
    fn organizer_and_attendees_exported_as_participants() {
        let cal = make_cal("550e8400-e29b-41d4-a716-446655440000");
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            (
                "uid",
                ImportValue::String("a8df6573-0474-496d-8496-033ad45d7fea".into()),
            ),
            ("start", make_datetime(2026, 1, 1, 0, 0, 0)),
            (
                "organizer",
                ImportValue::String("mailto:org@example.com".into()),
            ),
            (
                "attendees",
                ImportValue::List(vec![ImportValue::String(
                    "mailto:att@example.com".into(),
                )]),
            ),
        ]));

        let mut result = String::new();
        emit_jscalendar(&mut result, &cal, &[event], &mut vec![]).unwrap();
        let parsed: Json = serde_json::from_str(&result).unwrap();

        let entries = parsed["entries"].as_array().unwrap();
        let participants = entries[0]["participants"].as_object().unwrap();
        assert_eq!(participants.len(), 2);

        // Find the organizer and attendee by role.
        let mut found_owner = false;
        let mut found_attendee = false;
        for part in participants.values() {
            if let Some(roles) = part["roles"].as_object() {
                if roles.contains_key("owner") {
                    found_owner = true;
                }
                if roles.contains_key("attendee") {
                    found_attendee = true;
                }
            }
        }
        assert!(found_owner, "organizer should have owner role");
        assert!(found_attendee, "attendee should have attendee role");
    }

    #[test]
    fn recurrence_rule_exported() {
        let cal = make_cal("550e8400-e29b-41d4-a716-446655440000");
        let rrule = ImportValue::Record(make_record(&[
            ("frequency", ImportValue::String("weekly".into())),
            ("interval", ImportValue::Integer(2)),
            ("count", ImportValue::Integer(10)),
        ]));
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            (
                "uid",
                ImportValue::String("a8df6573-0474-496d-8496-033ad45d7fea".into()),
            ),
            ("start", make_datetime(2026, 1, 1, 0, 0, 0)),
            ("recur", rrule),
        ]));

        let mut result = String::new();
        emit_jscalendar(&mut result, &cal, &[event], &mut vec![]).unwrap();
        let parsed: Json = serde_json::from_str(&result).unwrap();

        let entries = parsed["entries"].as_array().unwrap();
        let rules = entries[0]["recurrenceRules"].as_array().unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0]["@type"], "RecurrenceRule");
        assert_eq!(rules[0]["frequency"], "weekly");
        assert_eq!(rules[0]["interval"], 2);
        assert_eq!(rules[0]["count"], 10);
    }

    #[test]
    fn unknown_non_vendor_field_warns() {
        let cal = make_cal("550e8400-e29b-41d4-a716-446655440000");
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            (
                "uid",
                ImportValue::String("a8df6573-0474-496d-8496-033ad45d7fea".into()),
            ),
            ("start", make_datetime(2026, 1, 1, 0, 0, 0)),
            ("transparency", ImportValue::String("opaque".into())),
        ]));

        let mut warnings = vec![];
        let mut result = String::new();
        emit_jscalendar(&mut result, &cal, &[event], &mut warnings).unwrap();

        assert!(
            warnings
                .iter()
                .any(|w| w.contains("transparency") && w.contains("event")),
            "should warn about unrecognised non-vendor field: {:?}",
            warnings
        );
    }

    #[test]
    fn vendor_prefixed_fields_no_warning() {
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
        ]));

        let mut warnings = vec![];
        let mut result = String::new();
        emit_jscalendar(&mut result, &cal, &[event], &mut warnings).unwrap();

        assert!(
            warnings.is_empty(),
            "vendor-prefixed fields should not warn: {:?}",
            warnings
        );
    }

    #[test]
    fn reply_to_exported() {
        let cal = make_cal("550e8400-e29b-41d4-a716-446655440000");
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            (
                "uid",
                ImportValue::String("a8df6573-0474-496d-8496-033ad45d7fea".into()),
            ),
            ("start", make_datetime(2026, 1, 1, 0, 0, 0)),
            (
                "reply_to",
                ImportValue::Record(make_record(&[(
                    "imip",
                    ImportValue::String("mailto:reply@example.com".into()),
                )])),
            ),
        ]));

        let mut result = String::new();
        emit_jscalendar(&mut result, &cal, &[event], &mut vec![]).unwrap();
        let parsed: Json = serde_json::from_str(&result).unwrap();

        let entries = parsed["entries"].as_array().unwrap();
        assert_eq!(
            entries[0]["replyTo"]["imip"],
            "mailto:reply@example.com"
        );
    }

    #[test]
    fn method_exported() {
        let cal = make_cal("550e8400-e29b-41d4-a716-446655440000");
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            (
                "uid",
                ImportValue::String("a8df6573-0474-496d-8496-033ad45d7fea".into()),
            ),
            ("start", make_datetime(2026, 1, 1, 0, 0, 0)),
            ("method", ImportValue::String("request".into())),
        ]));

        let mut result = String::new();
        emit_jscalendar(&mut result, &cal, &[event], &mut vec![]).unwrap();
        let parsed: Json = serde_json::from_str(&result).unwrap();

        let entries = parsed["entries"].as_array().unwrap();
        assert_eq!(entries[0]["method"], "REQUEST");
    }
}
