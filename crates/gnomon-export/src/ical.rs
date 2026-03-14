//! iCalendar export: ImportValue → calico model → RFC 5545 text.

use std::collections::BTreeSet;
use std::num::NonZero;

use calico::model::component::{Calendar, CalendarComponent, Event, Todo};
use calico::model::parameter::Params;
use calico::model::primitive::{
    Attachment, ClassValue, Date, DateTime, DateTimeOrDate, Day, Duration, ExactDuration, Geo,
    Hour, Minute, Month, NominalDuration, Priority, RDateSeq, Second, Sign, SignedDuration, Status,
    TimeFormat, TimeTransparency, Token, Utc, Version, Weekday, Year,
};
use calico::model::property::Prop;
use calico::model::rrule::weekday_num_set::WeekdayNumSet;
use calico::model::rrule::{
    ByMonthDayRule, ByPeriodDayRules, CoreByRules, FreqByRules, HourSet, Interval, MinuteSet,
    MonthDay, MonthDaySet, MonthDaySetIndex, MonthSet, RRule, SecondSet, Termination, WeekNoSet,
    WeekNoSetIndex, WeekdayNum, YearDayNum, YearlyByRules,
};
use calico::model::string::{TzId, Uid};
use calico::serializer::WriteIcal;

use gnomon_import::{ImportRecord, ImportValue};

// ── Unsafe helper to construct a calico Uri ───────────────────
//
// calico::model::string::Uri has `pub(crate) new` so it cannot be constructed
// from outside the crate. However it is repr(transparent) over str with a
// trivial (always-succeeding) invariant, so the transmutation is sound.
//
// The const block below asserts that `&Uri` and `&str` have the same size and
// alignment, catching any future change to Uri's layout at compile time.
const _: () = {
    use calico::model::string::Uri;
    assert!(
        std::mem::size_of::<&Uri>() == std::mem::size_of::<&str>(),
        "calico::model::string::Uri no longer has the same pointer size as str; \
         the transmute in make_calico_uri is unsound and must be revisited",
    );
    assert!(
        std::mem::align_of::<&Uri>() == std::mem::align_of::<&str>(),
        "calico::model::string::Uri no longer has the same pointer alignment as str; \
         the transmute in make_calico_uri is unsound and must be revisited",
    );
};

fn make_calico_uri(s: &str) -> Box<calico::model::string::Uri> {
    // SAFETY: calico::model::string::Uri is repr(transparent) over str with
    // a trivial invariant (the constructor is pub(crate) only for API reasons,
    // not because any str value is invalid). Transmuting Box<str> → Box<Uri>
    // is sound since both are repr(transparent) over str with the same memory layout.
    // The layout assumption is verified by the const assertions above.
    let b: Box<str> = s.into();
    unsafe { Box::from_raw(Box::into_raw(b) as *mut calico::model::string::Uri) }
}

// ── Public API ───────────────────────────────────────────────

/// Emit an iCalendar string from a calendar record and its entries.
///
/// The `calendar` parameter is the calendar-level properties (uid, prod_id, etc.).
/// The `entries` parameter is the list of event/task records.
// r[impl model.export.icalendar.calendar]
pub fn emit_icalendar(
    w: &mut impl std::fmt::Write,
    calendar: &ImportRecord,
    entries: &[ImportValue],
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    let prod_id = calendar
        .get("prod_id")
        .and_then(|v| as_str(v))
        .unwrap_or("-//gnomon//EN")
        .to_string();

    // r[impl model.export.icalendar.version]
    let version_prop: Prop<Token<Version, String>, Params> = Prop {
        value: Token::Known(Version::V2_0),
        params: Params::default(),
    };
    let prod_id_prop: Prop<String, Params> = Prop {
        value: prod_id,
        params: Params::default(),
    };

    // r[impl model.export.icalendar.entries]
    // Build components first so we can pass them to Calendar::new.
    let mut components: Vec<CalendarComponent> = Vec::new();
    for entry in entries {
        if let ImportValue::Record(record) = entry {
            match record.get("type").and_then(|v| as_str(v)) {
                Some("event") => {
                    let event = build_event(record, warnings)?;
                    components.push(CalendarComponent::Event(event));
                }
                Some("task") => {
                    let todo = build_todo(record, warnings)?;
                    components.push(CalendarComponent::Todo(todo));
                }
                _ => {}
            }
        }
    }

    let mut cal = Calendar::new(version_prop, prod_id_prop, components);

    // ── Optional VCALENDAR properties ────────────────────────

    if let Some(uid_str) = calendar.get("uid").and_then(|v| as_str(v)) {
        let uid = Uid::new(uid_str).map_err(|e| format!("Invalid UID in calendar: {}", e))?;
        cal.set_uid(Prop {
            value: uid.into(),
            params: Params::default(),
        });
    }

    if let Some(name_str) = calendar.get("name").and_then(|v| as_str(v)) {
        cal.set_name(vec![Prop {
            value: name_str.to_string(),
            params: Params::default(),
        }]);
    }

    if let Some(desc_str) = calendar.get("description").and_then(|v| as_str(v)) {
        cal.set_description(vec![Prop {
            value: desc_str.to_string(),
            params: Params::default(),
        }]);
    }

    if let Some(color_str) = calendar.get("color").and_then(|v| as_str(v))
        && let Ok(color) = color_str.parse::<calico::model::css::Css3Color>()
    {
        cal.set_color(Prop {
            value: color,
            params: Params::default(),
        });
    }

    if let Some(url_str) = calendar.get("url").and_then(|v| as_str(v)) {
        let uri = make_calico_uri(url_str);
        cal.set_url(Prop {
            value: uri,
            params: Params::default(),
        });
    }

    if let Some(ImportValue::List(cats)) = calendar.get("categories") {
        let cat_strings: Vec<String> = cats
            .iter()
            .filter_map(|v| as_str(v))
            .map(|s| s.to_string())
            .collect();
        if !cat_strings.is_empty() {
            cal.set_categories(vec![Prop {
                value: cat_strings,
                params: Params::default(),
            }]);
        }
    }

    if let Some(lm_val) = calendar.get("last_modified")
        && let Some(dt) = record_to_utc_datetime(lm_val)
    {
        cal.set_last_modified(Prop {
            value: dt,
            params: Params::default(),
        });
    }

    if let Some(ri_val) = calendar.get("refresh_interval")
        && let Some(sd) = record_to_signed_duration(ri_val)
    {
        cal.set_refresh_interval(Prop {
            value: sd,
            params: Params::default(),
        });
    }

    if let Some(source_str) = calendar.get("source").and_then(|v| as_str(v)) {
        let uri = make_calico_uri(source_str);
        cal.set_source(Prop {
            value: uri,
            params: Params::default(),
        });
    }

    // ── X-properties and unknown fields ─────────────────────

    // r[impl model.export.icalendar.extension]
    // r[impl model.export.icalendar.unknown+2]
    for (key, val) in calendar {
        if CALENDAR_KNOWN.contains(&key.as_str()) {
            continue;
        }
        if !key.starts_with("x_") {
            warnings.push(format!(
                "unrecognised non-extension field '{key}' on calendar record"
            ));
        }
        let prop_name = field_name_to_x_property(key);
        let x_val = import_value_to_ical_value(val);
        let prop = Prop {
            value: x_val,
            params: Params::default(),
        };
        cal.insert_x_property(prop_name.into(), vec![prop]);
    }

    w.write_str(&cal.to_ical_string())
        .map_err(|e| e.to_string())
}

const CALENDAR_KNOWN: &[&str] = &[
    "type",
    "entries",
    "prod_id",
    "uid",
    "name",
    "description",
    "color",
    "url",
    "categories",
    "last_modified",
    "refresh_interval",
    "source",
];

// ── Shared fields for event/todo known-field filtering ────────

const COMMON_KNOWN: &[&str] = &[
    "type",
    "name",
    "uid",
    "title",
    "description",
    "start",
    "time_zone",
    "status",
    "priority",
    "location",
    "color",
    "categories",
    "dtstamp",
    "class",
    "created",
    "geo",
    "last_modified",
    "organizer",
    "sequence",
    "url",
    "recurrence_id",
    "recur",
    "exdates",
    "rdates",
    "attachments",
    "attendees",
    "comments",
    "contacts",
    "related_to",
    "resources",
    "images",
    "conferences",
    "request_statuses",
];

const EVENT_EXTRA_KNOWN: &[&str] = &["duration", "transparency"];
const TODO_EXTRA_KNOWN: &[&str] = &["due", "completed", "estimated_duration", "percent_complete"];

// ── Shared property-setting macro ─────────────────────────────
//
// Calico's Event and Todo structs share identical field names and types for all
// common RFC 5545 properties, but expose no shared trait. This macro emits the
// property-setting code once, parameterised by the component expression and a
// label used in error messages.

macro_rules! set_common_ical_fields {
    ($component:expr, $record:expr, $kind:expr) => {
        // UID
        if let Some(uid_str) = $record.get("uid").and_then(|v| as_str(v)) {
            let uid = Uid::new(uid_str).map_err(|e| format!("Invalid UID in {}: {}", $kind, e))?;
            $component.set_uid(Prop {
                value: uid.into(),
                params: Params::default(),
            });
        }

        // SUMMARY ← title
        if let Some(title) = $record.get("title").and_then(|v| as_str(v)) {
            $component.set_summary(Prop {
                value: title.to_string(),
                params: Params::default(),
            });
        }

        // DESCRIPTION
        if let Some(desc) = $record.get("description").and_then(|v| as_str(v)) {
            $component.set_description(Prop {
                value: desc.to_string(),
                params: Params::default(),
            });
        }

        // DTSTART ← start + time_zone
        if let Some(start_val) = $record.get("start") {
            let tz_str = $record.get("time_zone").and_then(|v| as_str(v));
            if let Some(dtstart) = import_value_to_dtstart(start_val, tz_str) {
                $component.set_dtstart(dtstart);
            }
        }

        // STATUS
        if let Some(status_str) = $record.get("status").and_then(|v| as_str(v))
            && let Some(status) = str_to_status(status_str)
        {
            $component.set_status(Prop {
                value: status,
                params: Params::default(),
            });
        }

        // PRIORITY
        if let Some(prio_val) = $record.get("priority") {
            let prio = import_value_to_priority(prio_val);
            $component.set_priority(Prop {
                value: prio,
                params: Params::default(),
            });
        }

        // LOCATION
        if let Some(loc) = $record.get("location").and_then(|v| as_str(v)) {
            $component.set_location(Prop {
                value: loc.to_string(),
                params: Params::default(),
            });
        }

        // COLOR
        if let Some(color_str) = $record.get("color").and_then(|v| as_str(v))
            && let Ok(color) = color_str.parse::<calico::model::css::Css3Color>()
        {
            $component.set_color(Prop {
                value: color,
                params: Params::default(),
            });
        }

        // CATEGORIES
        if let Some(ImportValue::List(cats)) = $record.get("categories") {
            let cat_strings: Vec<String> = cats
                .iter()
                .filter_map(|v| as_str(v))
                .map(|s| s.to_string())
                .collect();
            if !cat_strings.is_empty() {
                $component.set_categories(vec![Prop {
                    value: cat_strings,
                    params: Params::default(),
                }]);
            }
        }

        // DTSTAMP
        if let Some(dtstamp_val) = $record.get("dtstamp")
            && let Some(dt) = record_to_utc_datetime(dtstamp_val)
        {
            $component.set_dtstamp(Prop {
                value: dt,
                params: Params::default(),
            });
        }

        // CLASS
        if let Some(class_str) = $record.get("class").and_then(|v| as_str(v)) {
            let class_val = str_to_class(class_str);
            $component.set_class(Prop {
                value: class_val,
                params: Params::default(),
            });
        }

        // CREATED
        if let Some(created_val) = $record.get("created")
            && let Some(dt) = record_to_utc_datetime(created_val)
        {
            $component.set_created(Prop {
                value: dt,
                params: Params::default(),
            });
        }

        // GEO
        if let Some(geo_val) = $record.get("geo")
            && let Some(geo) = record_to_geo(geo_val)
        {
            $component.set_geo(Prop {
                value: geo,
                params: Params::default(),
            });
        }

        // LAST-MODIFIED
        if let Some(lm_val) = $record.get("last_modified")
            && let Some(dt) = record_to_utc_datetime(lm_val)
        {
            $component.set_last_modified(Prop {
                value: dt,
                params: Params::default(),
            });
        }

        // ORGANIZER
        if let Some(org_str) = $record.get("organizer").and_then(|v| as_str(v)) {
            let uri = make_calico_uri(org_str);
            $component.set_organizer(Prop {
                value: uri,
                params: Params::default(),
            });
        }

        // SEQUENCE
        if let Some(seq_val) = $record.get("sequence")
            && let Some(seq) = import_value_to_i32(seq_val)
        {
            $component.set_sequence(Prop {
                value: seq,
                params: Params::default(),
            });
        }

        // URL
        if let Some(url_str) = $record.get("url").and_then(|v| as_str(v)) {
            let uri = make_calico_uri(url_str);
            $component.set_url(Prop {
                value: uri,
                params: Params::default(),
            });
        }

        // RECURRENCE-ID
        if let Some(rid_val) = $record.get("recurrence_id") {
            let tz_str = $record.get("time_zone").and_then(|v| as_str(v));
            if let Some(p) = import_value_to_dtstart(rid_val, tz_str) {
                $component.set_recurrence_id(Prop {
                    value: p.value,
                    params: p.params,
                });
            }
        }

        // RRULE ← recur
        if let Some(recur_val) = $record.get("recur")
            && let ImportValue::Record(recur_rec) = recur_val
            && let Some(rrule) = record_to_rrule(recur_rec)
        {
            $component.set_rrule(vec![Prop {
                value: rrule,
                params: Params::default(),
            }]);
        }

        // EXDATE ← exdates
        if let Some(ImportValue::List(exdates)) = $record.get("exdates") {
            let tz_str = $record.get("time_zone").and_then(|v| as_str(v));
            let props: Vec<Prop<DateTimeOrDate, Params>> = exdates
                .iter()
                .filter_map(|v| import_value_to_dtstart(v, tz_str))
                .map(|p| Prop {
                    value: p.value,
                    params: p.params,
                })
                .collect();
            if !props.is_empty() {
                $component.set_exdate(props);
            }
        }

        // RDATE ← rdates
        if let Some(ImportValue::List(rdates)) = $record.get("rdates") {
            let tz_str = $record.get("time_zone").and_then(|v| as_str(v));
            let converted: Vec<Prop<DateTimeOrDate, Params>> = rdates
                .iter()
                .filter_map(|v| import_value_to_dtstart(v, tz_str))
                .collect();
            let mut rdate_props: Vec<Prop<RDateSeq, Params>> = Vec::new();
            let datetimes: Vec<DateTime<TimeFormat>> = converted
                .iter()
                .filter_map(|p| {
                    if let DateTimeOrDate::DateTime(dt) = p.value {
                        Some(dt)
                    } else {
                        None
                    }
                })
                .collect();
            if !datetimes.is_empty() {
                rdate_props.push(Prop {
                    value: RDateSeq::DateTime(datetimes),
                    params: Params::default(),
                });
            }
            let dates: Vec<Date> = converted
                .iter()
                .filter_map(|p| {
                    if let DateTimeOrDate::Date(d) = p.value {
                        Some(d)
                    } else {
                        None
                    }
                })
                .collect();
            if !dates.is_empty() {
                rdate_props.push(Prop {
                    value: RDateSeq::Date(dates),
                    params: Params::default(),
                });
            }
            if !rdate_props.is_empty() {
                $component.set_rdate(rdate_props);
            }
        }

        // ATTACH ← attachments
        if let Some(ImportValue::List(attaches)) = $record.get("attachments") {
            let props: Vec<Prop<Attachment, Params>> = attaches
                .iter()
                .filter_map(import_value_to_attachment)
                .collect();
            if !props.is_empty() {
                $component.set_attach(props);
            }
        }

        // ATTENDEE ← attendees
        if let Some(ImportValue::List(attendees)) = $record.get("attendees") {
            let props: Vec<Prop<Box<calico::model::string::Uri>, Params>> = attendees
                .iter()
                .filter_map(|v| as_str(v))
                .map(|s| Prop {
                    value: make_calico_uri(s),
                    params: Params::default(),
                })
                .collect();
            if !props.is_empty() {
                $component.set_attendee(props);
            }
        }

        // COMMENT ← comments
        if let Some(ImportValue::List(comments)) = $record.get("comments") {
            let props: Vec<Prop<String, Params>> = comments
                .iter()
                .filter_map(|v| as_str(v))
                .map(|s| Prop {
                    value: s.to_string(),
                    params: Params::default(),
                })
                .collect();
            if !props.is_empty() {
                $component.set_comment(props);
            }
        }

        // CONTACT ← contacts
        if let Some(ImportValue::List(contacts)) = $record.get("contacts") {
            let props: Vec<Prop<String, Params>> = contacts
                .iter()
                .filter_map(|v| as_str(v))
                .map(|s| Prop {
                    value: s.to_string(),
                    params: Params::default(),
                })
                .collect();
            if !props.is_empty() {
                $component.set_contact(props);
            }
        }

        // RELATED-TO ← related_to
        if let Some(ImportValue::List(related)) = $record.get("related_to") {
            let props: Vec<Prop<Box<Uid>, Params>> = related
                .iter()
                .filter_map(|v| as_str(v))
                .filter_map(|s| Uid::new(s).ok())
                .map(|uid| Prop {
                    value: uid.into(),
                    params: Params::default(),
                })
                .collect();
            if !props.is_empty() {
                $component.set_related_to(props);
            }
        }

        // RESOURCES ← resources
        if let Some(ImportValue::List(resources)) = $record.get("resources") {
            let props: Vec<Prop<Vec<String>, Params>> = resources
                .iter()
                .filter_map(|v| {
                    if let ImportValue::List(inner) = v {
                        let strings: Vec<String> = inner
                            .iter()
                            .filter_map(|iv| as_str(iv))
                            .map(|s| s.to_string())
                            .collect();
                        if strings.is_empty() {
                            None
                        } else {
                            Some(strings)
                        }
                    } else {
                        None
                    }
                })
                .map(|strings| Prop {
                    value: strings,
                    params: Params::default(),
                })
                .collect();
            if !props.is_empty() {
                $component.set_resources(props);
            }
        }

        // IMAGE ← images
        if let Some(ImportValue::List(images)) = $record.get("images") {
            let props: Vec<Prop<Attachment, Params>> = images
                .iter()
                .filter_map(import_value_to_attachment)
                .collect();
            if !props.is_empty() {
                $component.set_image(props);
            }
        }

        // CONFERENCE ← conferences
        if let Some(ImportValue::List(conferences)) = $record.get("conferences") {
            let props: Vec<Prop<Box<calico::model::string::Uri>, Params>> = conferences
                .iter()
                .filter_map(|v| as_str(v))
                .map(|s| Prop {
                    value: make_calico_uri(s),
                    params: Params::default(),
                })
                .collect();
            if !props.is_empty() {
                $component.set_conference(props);
            }
        }

        // REQUEST-STATUS ← request_statuses
        if let Some(ImportValue::List(statuses)) = $record.get("request_statuses") {
            let props: Vec<Prop<calico::model::primitive::RequestStatus, Params>> = statuses
                .iter()
                .filter_map(|v| as_str(v))
                .filter_map(str_to_request_status)
                .map(|rs| Prop {
                    value: rs,
                    params: Params::default(),
                })
                .collect();
            if !props.is_empty() {
                $component.set_request_status(props);
            }
        }
    };
}

/// Emit x-properties and warn about unrecognised non-extension fields.
fn handle_x_properties(
    component: &mut impl XPropertySink,
    record: &ImportRecord,
    extra_known: &[&str],
    kind: &str,
    warnings: &mut Vec<String>,
) {
    for (key, val) in record {
        if COMMON_KNOWN.contains(&key.as_str()) || extra_known.contains(&key.as_str()) {
            continue;
        }
        if !key.starts_with("x_") {
            warnings.push(format!(
                "unrecognised non-extension field '{key}' on {kind} record"
            ));
        }
        let prop_name = field_name_to_x_property(key);
        let x_val = import_value_to_ical_value(val);
        let prop = Prop {
            value: x_val,
            params: Params::default(),
        };
        component.insert_x(prop_name, vec![prop]);
    }
}

/// Minimal trait to abstract x-property insertion over Event and Todo.
trait XPropertySink {
    fn insert_x(
        &mut self,
        name: String,
        props: Vec<Prop<calico::model::primitive::Value<String>, Params>>,
    );
}

impl XPropertySink for Event {
    fn insert_x(
        &mut self,
        name: String,
        props: Vec<Prop<calico::model::primitive::Value<String>, Params>>,
    ) {
        self.insert_x_property(name.into(), props);
    }
}

impl XPropertySink for Todo {
    fn insert_x(
        &mut self,
        name: String,
        props: Vec<Prop<calico::model::primitive::Value<String>, Params>>,
    ) {
        self.insert_x_property(name.into(), props);
    }
}

// ── Event builder ─────────────────────────────────────────────

// r[impl model.export.icalendar.event]
fn build_event(record: &ImportRecord, warnings: &mut Vec<String>) -> Result<Event, String> {
    let mut event = Event::new(vec![], vec![], vec![], vec![]);

    set_common_ical_fields!(event, record, "event");

    // DURATION ← duration (event-specific: reads "duration" field)
    if let Some(dur_val) = record.get("duration")
        && let Some(sd) = record_to_signed_duration(dur_val)
    {
        event.set_duration(Prop {
            value: sd,
            params: Params::default(),
        });
    }

    // TRANSP ← transparency (event-only)
    if let Some(transp_str) = record.get("transparency").and_then(|v| as_str(v)) {
        let transp = str_to_transp(transp_str);
        event.set_transp(Prop {
            value: transp,
            params: Params::default(),
        });
    }

    handle_x_properties(&mut event, record, EVENT_EXTRA_KNOWN, "event", warnings);

    Ok(event)
}

// ── Todo builder ──────────────────────────────────────────────

// r[impl model.export.icalendar.task]
fn build_todo(record: &ImportRecord, warnings: &mut Vec<String>) -> Result<Todo, String> {
    let mut todo = Todo::new(vec![], vec![], vec![], vec![]);

    set_common_ical_fields!(todo, record, "todo");

    // DUE ← due (todo-only)
    if let Some(due_val) = record.get("due") {
        let tz_str = record.get("time_zone").and_then(|v| as_str(v));
        if let Some(due) = import_value_to_dtstart(due_val, tz_str) {
            todo.set_due(due);
        }
    }

    // DURATION ← estimated_duration (todo-specific: reads "estimated_duration" field)
    if let Some(dur_val) = record.get("estimated_duration")
        && let Some(sd) = record_to_signed_duration(dur_val)
    {
        todo.set_duration(Prop {
            value: sd,
            params: Params::default(),
        });
    }

    // PERCENT-COMPLETE ← percent_complete (todo-only)
    if let Some(pct_val) = record.get("percent_complete")
        && let Some(pct) = import_value_to_u64(pct_val)
        && let Ok(pct_u8) = u8::try_from(pct)
        && let Some(cp) = calico::model::primitive::CompletionPercentage::new(pct_u8)
    {
        todo.set_percent_complete(Prop {
            value: cp,
            params: Params::default(),
        });
    }

    // COMPLETED (todo-only)
    if let Some(completed_val) = record.get("completed")
        && let Some(dt) = record_to_utc_datetime(completed_val)
    {
        todo.set_completed(Prop {
            value: dt,
            params: Params::default(),
        });
    }

    handle_x_properties(&mut todo, record, TODO_EXTRA_KNOWN, "task", warnings);

    Ok(todo)
}

// ── Primitive conversion helpers ──────────────────────────────

/// Extract a string from an ImportValue.
fn as_str(v: &ImportValue) -> Option<&str> {
    if let ImportValue::String(s) = v {
        Some(s.as_str())
    } else {
        None
    }
}

/// Convert an ImportValue to a u64 integer.
fn import_value_to_u64(v: &ImportValue) -> Option<u64> {
    match v {
        ImportValue::Integer(n) => Some(*n),
        ImportValue::SignedInteger(n) => u64::try_from(*n).ok(),
        _ => None,
    }
}

/// Convert an ImportValue to an i64.
fn import_value_to_i64(v: &ImportValue) -> Option<i64> {
    match v {
        ImportValue::Integer(n) => i64::try_from(*n).ok(),
        ImportValue::SignedInteger(n) => Some(*n),
        _ => None,
    }
}

/// Convert an ImportValue to an i32 (for SEQUENCE and INTEGER properties).
fn import_value_to_i32(v: &ImportValue) -> Option<i32> {
    match v {
        ImportValue::Integer(n) => i32::try_from(*n).ok(),
        ImportValue::SignedInteger(n) => i32::try_from(*n).ok(),
        _ => None,
    }
}

/// Convert a field name like `x_foo_bar` to `X-FOO-BAR`.
fn field_name_to_x_property(key: &str) -> String {
    key.to_uppercase().replace('_', "-")
}

/// Convert an ImportValue datetime record to a calico `DateTime<Utc>`.
fn record_to_utc_datetime(v: &ImportValue) -> Option<DateTime<Utc>> {
    let rec = match v {
        ImportValue::Record(r) => r,
        _ => return None,
    };
    let date_rec = match rec.get("date") {
        Some(ImportValue::Record(r)) => r,
        _ => return None,
    };
    let time_rec = match rec.get("time") {
        Some(ImportValue::Record(r)) => r,
        _ => return None,
    };

    let year = u16::try_from(import_value_to_u64(date_rec.get("year")?)?).ok()?;
    let month_n = u8::try_from(import_value_to_u64(date_rec.get("month")?)?).ok()?;
    let day_n = u8::try_from(import_value_to_u64(date_rec.get("day")?)?).ok()?;
    let hour_n = u8::try_from(import_value_to_u64(time_rec.get("hour")?)?).ok()?;
    let min_n = u8::try_from(import_value_to_u64(time_rec.get("minute")?)?).ok()?;
    let sec_n = u8::try_from(import_value_to_u64(time_rec.get("second")?)?).ok()?;

    let y = Year::new(year).ok()?;
    let mo = Month::new(month_n).ok()?;
    let d = Day::new(day_n).ok()?;
    let h = Hour::new(hour_n).ok()?;
    let mi = Minute::new(min_n).ok()?;
    let s = Second::new(sec_n).ok()?;

    let date = Date::new(y, mo, d).ok()?;
    let time = calico::model::primitive::Time::new(h, mi, s, None).ok()?;

    Some(DateTime {
        date,
        time,
        marker: Utc,
    })
}

/// Convert an ImportValue datetime or date record to a `Prop<DateTimeOrDate, Params>`.
///
/// If `tz_str` is Some, the TZID parameter is set on the resulting property
/// (unless the timezone is "UTC" or "Z", in which case the UTC marker is used).
fn import_value_to_dtstart(
    v: &ImportValue,
    tz_str: Option<&str>,
) -> Option<Prop<DateTimeOrDate, Params>> {
    let rec = match v {
        ImportValue::Record(r) => r,
        _ => return None,
    };

    // Date-only if no "time" sub-record and no "date" sub-record (flat year/month/day).
    if !rec.contains_key("time") {
        // Could be a flat date {year, month, day} or nested {date: {year, month, day}}.
        let (year, month_n, day_n) = if rec.contains_key("date") {
            let date_rec = match rec.get("date") {
                Some(ImportValue::Record(r)) => r,
                _ => return None,
            };
            (
                u16::try_from(import_value_to_u64(date_rec.get("year")?)?).ok()?,
                u8::try_from(import_value_to_u64(date_rec.get("month")?)?).ok()?,
                u8::try_from(import_value_to_u64(date_rec.get("day")?)?).ok()?,
            )
        } else {
            (
                u16::try_from(import_value_to_u64(rec.get("year")?)?).ok()?,
                u8::try_from(import_value_to_u64(rec.get("month")?)?).ok()?,
                u8::try_from(import_value_to_u64(rec.get("day")?)?).ok()?,
            )
        };

        let y = Year::new(year).ok()?;
        let mo = Month::new(month_n).ok()?;
        let d = Day::new(day_n).ok()?;
        let date = Date::new(y, mo, d).ok()?;

        return Some(Prop {
            value: DateTimeOrDate::Date(date),
            params: Params::default(),
        });
    }

    // DateTime: get date and time sub-records.
    let date_rec = match rec.get("date") {
        Some(ImportValue::Record(r)) => r,
        _ => return None,
    };
    let time_rec = match rec.get("time") {
        Some(ImportValue::Record(r)) => r,
        _ => return None,
    };

    let year = u16::try_from(import_value_to_u64(date_rec.get("year")?)?).ok()?;
    let month_n = u8::try_from(import_value_to_u64(date_rec.get("month")?)?).ok()?;
    let day_n = u8::try_from(import_value_to_u64(date_rec.get("day")?)?).ok()?;
    let hour_n = u8::try_from(import_value_to_u64(time_rec.get("hour")?)?).ok()?;
    let min_n = u8::try_from(import_value_to_u64(time_rec.get("minute")?)?).ok()?;
    let sec_n = u8::try_from(import_value_to_u64(time_rec.get("second")?)?).ok()?;

    let y = Year::new(year).ok()?;
    let mo = Month::new(month_n).ok()?;
    let d = Day::new(day_n).ok()?;
    let h = Hour::new(hour_n).ok()?;
    let mi = Minute::new(min_n).ok()?;
    let s = Second::new(sec_n).ok()?;

    let date = Date::new(y, mo, d).ok()?;
    let time = calico::model::primitive::Time::new(h, mi, s, None).ok()?;

    let mut params = Params::default();

    let dtod: DateTimeOrDate = match tz_str {
        Some(tz) if tz.eq_ignore_ascii_case("UTC") || tz.eq_ignore_ascii_case("Z") => {
            DateTimeOrDate::DateTime(DateTime {
                date,
                time,
                marker: TimeFormat::Utc,
            })
        }
        Some(tz) => {
            // Local datetime with TZID parameter.
            // TzId::new has a trivial invariant (infallible), so unwrap is safe.
            let tz_id = TzId::new(tz).unwrap();
            params.set_tz_id(tz_id.into());
            DateTimeOrDate::DateTime(DateTime {
                date,
                time,
                marker: TimeFormat::Local,
            })
        }
        None => {
            // Floating local time.
            DateTimeOrDate::DateTime(DateTime {
                date,
                time,
                marker: TimeFormat::Local,
            })
        }
    };

    Some(Prop {
        value: dtod,
        params,
    })
}

/// Convert an ImportValue duration record to a calico `SignedDuration`.
fn record_to_signed_duration(v: &ImportValue) -> Option<SignedDuration> {
    let rec = match v {
        ImportValue::Record(r) => r,
        _ => return None,
    };

    let weeks_raw = rec.get("weeks").and_then(import_value_to_i64).unwrap_or(0);
    let days_raw = rec.get("days").and_then(import_value_to_i64).unwrap_or(0);
    let hours_raw = rec.get("hours").and_then(import_value_to_i64).unwrap_or(0);
    let minutes_raw = rec
        .get("minutes")
        .and_then(import_value_to_i64)
        .unwrap_or(0);
    let seconds_raw = rec
        .get("seconds")
        .and_then(import_value_to_i64)
        .unwrap_or(0);

    // If any field is negative, the whole duration is negative.
    let sign =
        if weeks_raw < 0 || days_raw < 0 || hours_raw < 0 || minutes_raw < 0 || seconds_raw < 0 {
            Sign::Neg
        } else {
            Sign::Pos
        };

    let weeks = u32::try_from(weeks_raw.unsigned_abs()).unwrap_or(u32::MAX);
    let days = u32::try_from(days_raw.unsigned_abs()).unwrap_or(u32::MAX);
    let hours = u32::try_from(hours_raw.unsigned_abs()).unwrap_or(u32::MAX);
    let minutes = u32::try_from(minutes_raw.unsigned_abs()).unwrap_or(u32::MAX);
    let secs = u32::try_from(seconds_raw.unsigned_abs()).unwrap_or(u32::MAX);

    let nominal = if hours == 0 && minutes == 0 && secs == 0 {
        NominalDuration {
            weeks,
            days,
            exact: None,
        }
    } else {
        NominalDuration {
            weeks,
            days,
            exact: Some(ExactDuration {
                hours,
                minutes,
                seconds: secs,
                frac: None,
            }),
        }
    };

    Some(SignedDuration {
        sign,
        duration: Duration::Nominal(nominal),
    })
}

/// Convert an ImportValue geo record to a calico `Geo`.
fn record_to_geo(v: &ImportValue) -> Option<Geo> {
    let rec = match v {
        ImportValue::Record(r) => r,
        _ => return None,
    };
    let lat: f64 = rec.get("latitude").and_then(|v| as_str(v))?.parse().ok()?;
    let lon: f64 = rec.get("longitude").and_then(|v| as_str(v))?.parse().ok()?;
    Some(Geo { lat, lon })
}

/// Convert a status string to a calico `Status`.
// r[impl model.export.icalendar.status]
fn str_to_status(s: &str) -> Option<Status> {
    match s {
        "tentative" => Some(Status::Tentative),
        "confirmed" => Some(Status::Confirmed),
        "cancelled" => Some(Status::Cancelled),
        "needs-action" => Some(Status::NeedsAction),
        "completed" => Some(Status::Completed),
        "in-process" => Some(Status::InProcess),
        "draft" => Some(Status::Draft),
        "final" => Some(Status::Final),
        _ => None,
    }
}

/// Convert a priority ImportValue (0–9) to a calico `Priority`.
fn import_value_to_priority(v: &ImportValue) -> Priority {
    let n = import_value_to_u64(v).unwrap_or(0);
    match n {
        0 => Priority::Zero,
        1 => Priority::A1,
        2 => Priority::A2,
        3 => Priority::A3,
        4 => Priority::B1,
        5 => Priority::B2,
        6 => Priority::B3,
        7 => Priority::C1,
        8 => Priority::C2,
        9 => Priority::C3,
        _ => Priority::Zero,
    }
}

/// Convert a class string to a calico `Token<ClassValue, String>`.
fn str_to_class(s: &str) -> Token<ClassValue, String> {
    match s {
        "public" => Token::Known(ClassValue::Public),
        "private" => Token::Known(ClassValue::Private),
        "confidential" => Token::Known(ClassValue::Confidential),
        other => Token::Unknown(other.to_uppercase()),
    }
}

/// Convert a transparency string to a calico `TimeTransparency`.
fn str_to_transp(s: &str) -> TimeTransparency {
    match s {
        "transparent" => TimeTransparency::Transparent,
        _ => TimeTransparency::Opaque,
    }
}

/// Convert a weekday string to a calico `Weekday`.
fn str_to_weekday(s: &str) -> Option<Weekday> {
    match s {
        "monday" => Some(Weekday::Monday),
        "tuesday" => Some(Weekday::Tuesday),
        "wednesday" => Some(Weekday::Wednesday),
        "thursday" => Some(Weekday::Thursday),
        "friday" => Some(Weekday::Friday),
        "saturday" => Some(Weekday::Saturday),
        "sunday" => Some(Weekday::Sunday),
        _ => None,
    }
}

/// Convert an ImportValue to a calico `Attachment` wrapped in a `Prop`.
///
/// String values become URI attachments; records with a `data` field become binary attachments.
fn import_value_to_attachment(v: &ImportValue) -> Option<Prop<Attachment, Params>> {
    match v {
        ImportValue::String(s) => {
            // Attachment::Uri uses calendar_types::string::Uri, which has a public `new`.
            use calendar_types::string::Uri as CtUri;
            let uri = CtUri::new(s.as_str()).ok()?;
            Some(Prop {
                value: Attachment::Uri(uri.into()),
                params: Params::default(),
            })
        }
        ImportValue::Record(rec) => {
            // Binary attachment: has "data" field with base64 content.
            let data_str = rec.get("data").and_then(|v| as_str(v))?;
            let binary = base64_decode(data_str)?;
            Some(Prop {
                value: Attachment::Binary(binary),
                params: Params::default(),
            })
        }
        _ => None,
    }
}

/// Simple base64 decoder.
fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let s: String = s.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let chars: Vec<u8> = s.bytes().collect();
    for chunk in chars.chunks(4) {
        if chunk.len() < 2 {
            return None;
        }
        let decode_char = |c: u8| -> Option<u8> {
            match c {
                b'A'..=b'Z' => Some(c - b'A'),
                b'a'..=b'z' => Some(c - b'a' + 26),
                b'0'..=b'9' => Some(c - b'0' + 52),
                b'+' => Some(62),
                b'/' => Some(63),
                b'=' => Some(0),
                _ => None,
            }
        };
        let b0 = decode_char(chunk[0])?;
        let b1 = decode_char(chunk[1])?;
        out.push((b0 << 2) | (b1 >> 4));
        if chunk.len() > 2 && chunk[2] != b'=' {
            let b2 = decode_char(chunk[2])?;
            out.push((b1 << 4) | (b2 >> 2));
            if chunk.len() > 3 && chunk[3] != b'=' {
                let b3 = decode_char(chunk[3])?;
                out.push((b2 << 6) | b3);
            }
        }
    }
    Some(out)
}

/// Parse a request-status string like "2.0;Success" into a calico `RequestStatus`.
fn str_to_request_status(s: &str) -> Option<calico::model::primitive::RequestStatus> {
    use calico::model::primitive::{Class, RequestStatus, RequestStatusCode};

    let mut parts = s.splitn(3, ';');
    let code_str = parts.next()?;
    let description = parts.next()?;
    let exception_data = parts.next();

    let mut code_parts = code_str.split('.');
    let class_u8: u8 = code_parts.next()?.parse().ok()?;
    let major: u8 = code_parts.next()?.parse().ok()?;
    let minor: Option<u8> = code_parts.next().and_then(|s| s.parse().ok());

    let class = Class::from_u8(class_u8)?;
    let code = RequestStatusCode {
        class,
        major,
        minor,
    };

    Some(RequestStatus {
        code,
        description: description.into(),
        exception_data: exception_data.map(|s| s.into()),
    })
}

/// Convert an ImportValue recur record to a calico `RRule`.
fn record_to_rrule(rec: &ImportRecord) -> Option<RRule> {
    let freq_str = rec.get("frequency").and_then(|v| as_str(v))?;

    let mut core = CoreByRules::default();

    // BYSECOND
    if let Some(ImportValue::List(by_second)) = rec.get("by_second") {
        let mut set = SecondSet::default();
        for v in by_second {
            if let Some(n) = import_value_to_u64(v)
                && let Some(sec) = u8::try_from(n)
                    .ok()
                    .and_then(calico::model::rrule::Second::from_repr)
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
            if let Some(n) = import_value_to_u64(v)
                && let Some(min) = u8::try_from(n)
                    .ok()
                    .and_then(calico::model::rrule::Minute::from_repr)
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
            if let Some(n) = import_value_to_u64(v)
                && let Some(h) = u8::try_from(n)
                    .ok()
                    .and_then(calico::model::rrule::Hour::from_repr)
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
            if let Some(n) = import_value_to_u64(v)
                && let Some(month) = u8::try_from(n).ok().and_then(|b| Month::new(b).ok())
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
                    if let Some(day_str) = day_rec.get("day").and_then(|v| as_str(v))
                        && let Some(wd) = str_to_weekday(day_str)
                    {
                        let ordinal_i64 = day_rec
                            .get("ordinal")
                            .and_then(import_value_to_i64)
                            .unwrap_or(0);
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
                                .and_then(calico::model::primitive::IsoWeek::from_index)
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
        let mut set: BTreeSet<YearDayNum> = BTreeSet::new();
        for v in by_set_pos {
            if let Some(n) = import_value_to_i64(v)
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

    // Helper closures for frequency-specific BY rules.
    let build_month_day_set = |rec: &ImportRecord| -> Option<MonthDaySet> {
        let ImportValue::List(by_month_day) = rec.get("by_month_day")? else {
            return None;
        };
        let mut set = MonthDaySet::default();
        for v in by_month_day {
            if let Some(n) = import_value_to_i64(v)
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

    let build_year_day_set = |rec: &ImportRecord| -> Option<BTreeSet<YearDayNum>> {
        let ImportValue::List(by_year_day) = rec.get("by_year_day")? else {
            return None;
        };
        let mut set: BTreeSet<YearDayNum> = BTreeSet::new();
        for v in by_year_day {
            if let Some(n) = import_value_to_i64(v)
                && let Ok(abs) = u16::try_from(n.unsigned_abs())
                && let Some(ydn) =
                    YearDayNum::from_signed_index(if n < 0 { Sign::Neg } else { Sign::Pos }, abs)
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
            if let Some(n) = import_value_to_i64(v)
                && let Ok(abs) = u8::try_from(n.unsigned_abs())
                && let Some(week) = calico::model::primitive::IsoWeek::from_index(abs)
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
    let interval = rec
        .get("interval")
        .and_then(import_value_to_u64)
        .and_then(|n| NonZero::new(n).map(Interval::new));

    // TERMINATION (COUNT or UNTIL)
    let termination = if let Some(count_val) = rec.get("count") {
        import_value_to_u64(count_val).map(Termination::Count)
    } else if let Some(until_val) = rec.get("until") {
        import_value_to_dtstart(until_val, None).map(|p| Termination::Until(p.value))
    } else {
        None
    };

    // WKST
    let week_start = rec
        .get("week_start")
        .and_then(|v| as_str(v))
        .and_then(str_to_weekday);

    Some(RRule {
        freq,
        core_by_rules: core,
        interval,
        termination,
        week_start,
    })
}

/// Convert an `ImportValue` to a `serde_json::Value` so that composite values
/// can be round-tripped through iCalendar x-properties as JSON text.
fn import_value_to_json(value: &ImportValue) -> serde_json::Value {
    use serde_json::{Map, Value as Json};
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

/// Convert an ImportValue to a calico x-property `Value<String>`.
///
/// Scalar values map to their native iCal counterparts. Composite values
/// (`Record`, `List`) and `Undefined` are serialised as JSON text so that
/// no data is lost.
fn import_value_to_ical_value(v: &ImportValue) -> calico::model::primitive::Value<String> {
    use calico::model::primitive::Value;
    match v {
        ImportValue::String(s) => Value::Text(s.clone()),
        ImportValue::Integer(n) => Value::Integer(i32::try_from(*n).unwrap_or(i32::MAX)),
        ImportValue::SignedInteger(n) => {
            Value::Integer(i32::try_from(*n).unwrap_or(if *n < 0 { i32::MIN } else { i32::MAX }))
        }
        ImportValue::Bool(b) => Value::Boolean(*b),
        ImportValue::Undefined => Value::Text(String::new()),
        ImportValue::Record(_) | ImportValue::List(_) => {
            Value::Text(import_value_to_json(v).to_string())
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use gnomon_import::translate_icalendar;

    fn make_record(fields: &[(&str, ImportValue)]) -> ImportRecord {
        fields
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect()
    }

    fn make_start_dt(
        year: u64,
        month: u64,
        day: u64,
        hour: u64,
        min: u64,
        sec: u64,
    ) -> ImportValue {
        ImportValue::Record(make_record(&[
            (
                "date",
                ImportValue::Record(make_record(&[
                    ("year", ImportValue::Integer(year)),
                    ("month", ImportValue::Integer(month)),
                    ("day", ImportValue::Integer(day)),
                ])),
            ),
            (
                "time",
                ImportValue::Record(make_record(&[
                    ("hour", ImportValue::Integer(hour)),
                    ("minute", ImportValue::Integer(min)),
                    ("second", ImportValue::Integer(sec)),
                ])),
            ),
        ]))
    }

    fn make_cal(prod_id: &str) -> ImportRecord {
        make_record(&[
            ("type", ImportValue::String("calendar".into())),
            ("prod_id", ImportValue::String(prod_id.into())),
        ])
    }

    // r[verify model.export.icalendar.calendar]
    #[test]
    fn emit_empty_calendar() {
        let calendar = make_cal("-//Test//Test//EN");
        let mut result = String::new();
        emit_icalendar(&mut result, &calendar, &[], &mut vec![]).unwrap();
        assert!(result.contains("BEGIN:VCALENDAR"), "missing VCALENDAR");
        assert!(
            result.contains("PRODID:-//Test//Test//EN"),
            "missing PRODID"
        );
        assert!(result.contains("VERSION:2.0"), "missing VERSION");
        assert!(result.contains("END:VCALENDAR"), "missing END:VCALENDAR");
    }

    // r[verify model.export.icalendar.event]
    #[test]
    fn emit_minimal_event() {
        let calendar = make_cal("-//Test//Test//EN");
        let duration = ImportValue::Record(make_record(&[
            ("weeks", ImportValue::Integer(0)),
            ("days", ImportValue::Integer(0)),
            ("hours", ImportValue::Integer(1)),
            ("minutes", ImportValue::Integer(30)),
            ("seconds", ImportValue::Integer(0)),
        ]));
        let entry = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            ("uid", ImportValue::String("test-event-uid".into())),
            ("title", ImportValue::String("Team Meeting".into())),
            ("start", make_start_dt(2026, 3, 15, 14, 0, 0)),
            ("duration", duration),
        ]));
        let mut result = String::new();
        emit_icalendar(&mut result, &calendar, &[entry], &mut vec![]).unwrap();
        assert!(result.contains("BEGIN:VEVENT"), "missing VEVENT");
        assert!(result.contains("UID:test-event-uid"), "missing UID");
        assert!(result.contains("SUMMARY:Team Meeting"), "missing SUMMARY");
        assert!(result.contains("DTSTART"), "missing DTSTART");
        assert!(result.contains("DURATION:PT1H30M"), "missing DURATION");
        assert!(result.contains("END:VEVENT"), "missing END:VEVENT");
    }

    // r[verify model.export.icalendar.task]
    #[test]
    fn emit_minimal_task() {
        let calendar = make_cal("-//Test//Test//EN");
        let entry = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("task".into())),
            ("uid", ImportValue::String("test-task-uid".into())),
            ("title", ImportValue::String("Buy groceries".into())),
        ]));
        let mut result = String::new();
        emit_icalendar(&mut result, &calendar, &[entry], &mut vec![]).unwrap();
        assert!(result.contains("BEGIN:VTODO"), "missing VTODO");
        assert!(result.contains("UID:test-task-uid"), "missing UID");
        assert!(result.contains("SUMMARY:Buy groceries"), "missing SUMMARY");
        assert!(result.contains("END:VTODO"), "missing END:VTODO");
    }

    // r[verify model.export.icalendar.roundtrip]
    #[test]
    fn roundtrip_event_fields() {
        let ics = "\
BEGIN:VCALENDAR\r\n\
VERSION:2.0\r\n\
PRODID:-//Roundtrip//Test//EN\r\n\
BEGIN:VEVENT\r\n\
UID:roundtrip-uid-42\r\n\
SUMMARY:Roundtrip Test\r\n\
DESCRIPTION:An event for round-trip testing\r\n\
DTSTART;TZID=America/New_York:20260315T140000\r\n\
DURATION:PT2H\r\n\
STATUS:CONFIRMED\r\n\
PRIORITY:3\r\n\
LOCATION:Conference Room A\r\n\
END:VEVENT\r\n\
END:VCALENDAR\r\n";

        let import_result = translate_icalendar(ics).unwrap();
        let ImportValue::List(calendars) = import_result else {
            panic!("expected list")
        };
        let ImportValue::Record(cal_rec) = &calendars[0] else {
            panic!("expected record")
        };
        let ImportValue::List(entries) = cal_rec.get("entries").unwrap() else {
            panic!("expected entries list")
        };

        let mut emitted = String::new();
        emit_icalendar(&mut emitted, cal_rec, entries, &mut vec![]).unwrap();
        let re_parsed =
            calico::model::component::Calendar::parse(&emitted).expect("re-parse failed");
        let cal = &re_parsed[0];

        assert_eq!(cal.prod_id().value, "-//Roundtrip//Test//EN");

        let event = cal.components().iter().find_map(|c| {
            if let calico::model::component::CalendarComponent::Event(e) = c {
                Some(e)
            } else {
                None
            }
        });
        let event = event.expect("event not found");

        assert_eq!(event.uid().unwrap().value.as_str(), "roundtrip-uid-42");
        assert_eq!(event.summary().unwrap().value, "Roundtrip Test");
        assert_eq!(
            event.description().unwrap().value,
            "An event for round-trip testing"
        );
        assert_eq!(event.location().unwrap().value, "Conference Room A");
    }

    // r[verify model.export.icalendar.roundtrip]
    #[test]
    fn roundtrip_task_fields() {
        // RFC 5545 §3.6.2: DUE and DURATION are mutually exclusive in VTODO.
        // This test covers DUE, PERCENT-COMPLETE, STATUS, and COMPLETED via
        // a full iCalendar import→export→re-parse cycle.  The estimated_duration
        // (DURATION) property is verified in the second half of the test using a
        // directly-constructed record, since it cannot coexist with DUE.
        let ics = "\
BEGIN:VCALENDAR\r\n\
VERSION:2.0\r\n\
PRODID:-//Roundtrip//Task//EN\r\n\
BEGIN:VTODO\r\n\
UID:roundtrip-task-uid-1\r\n\
SUMMARY:Roundtrip Task\r\n\
DESCRIPTION:A task for round-trip testing\r\n\
DUE:20260320T180000\r\n\
PERCENT-COMPLETE:75\r\n\
STATUS:IN-PROCESS\r\n\
COMPLETED:20260319T120000Z\r\n\
END:VTODO\r\n\
END:VCALENDAR\r\n";

        let import_result = translate_icalendar(ics).unwrap();
        let ImportValue::List(calendars) = import_result else {
            panic!("expected list")
        };
        let ImportValue::Record(cal_rec) = &calendars[0] else {
            panic!("expected record")
        };
        let ImportValue::List(entries) = cal_rec.get("entries").unwrap() else {
            panic!("expected entries list")
        };

        let mut emitted = String::new();
        emit_icalendar(&mut emitted, cal_rec, entries, &mut vec![]).unwrap();
        let re_parsed =
            calico::model::component::Calendar::parse(&emitted).expect("re-parse failed");
        let cal = &re_parsed[0];

        assert_eq!(cal.prod_id().value, "-//Roundtrip//Task//EN");

        let todo = cal.components().iter().find_map(|c| {
            if let calico::model::component::CalendarComponent::Todo(t) = c {
                Some(t)
            } else {
                None
            }
        });
        let todo = todo.expect("todo not found");

        assert_eq!(todo.uid().unwrap().value.as_str(), "roundtrip-task-uid-1");
        assert_eq!(todo.summary().unwrap().value, "Roundtrip Task");
        assert_eq!(
            todo.description().unwrap().value,
            "A task for round-trip testing"
        );
        assert!(todo.due().is_some(), "DUE should be present");
        assert!(
            todo.percent_complete().is_some(),
            "PERCENT-COMPLETE should be present"
        );
        assert_eq!(todo.percent_complete().unwrap().value.get(), 75);
        assert!(todo.completed().is_some(), "COMPLETED should be present");

        // Verify DURATION (estimated_duration) round-trips correctly.  Because
        // RFC 5545 forbids DUE and DURATION in the same VTODO, this leg uses a
        // directly-constructed task record.
        let duration_val = ImportValue::Record(make_record(&[
            ("weeks", ImportValue::Integer(0)),
            ("days", ImportValue::Integer(0)),
            ("hours", ImportValue::Integer(0)),
            ("minutes", ImportValue::Integer(30)),
            ("seconds", ImportValue::Integer(0)),
        ]));
        let duration_entry = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("task".into())),
            ("uid", ImportValue::String("roundtrip-task-dur-uid".into())),
            ("title", ImportValue::String("Duration Task".into())),
            ("estimated_duration", duration_val),
        ]));
        let cal_rec2 = make_cal("-//Roundtrip//Task//EN");
        let mut emitted2 = String::new();
        emit_icalendar(&mut emitted2, &cal_rec2, &[duration_entry], &mut vec![]).unwrap();
        let re_parsed2 =
            calico::model::component::Calendar::parse(&emitted2).expect("duration re-parse failed");
        let todo2 = re_parsed2[0]
            .components()
            .iter()
            .find_map(|c| {
                if let calico::model::component::CalendarComponent::Todo(t) = c {
                    Some(t)
                } else {
                    None
                }
            })
            .expect("duration todo not found");
        assert!(
            todo2.duration().is_some(),
            "DURATION (estimated_duration) should be present"
        );
    }

    // r[verify model.export.icalendar.status_priority]
    #[test]
    fn emit_status_and_priority() {
        let calendar = make_cal("-//Test//Test//EN");
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            ("uid", ImportValue::String("status-prio-uid".into())),
            ("status", ImportValue::String("tentative".into())),
            ("priority", ImportValue::Integer(5)),
        ]));
        let mut result = String::new();
        emit_icalendar(&mut result, &calendar, &[event], &mut vec![]).unwrap();
        assert!(
            result.contains("STATUS:TENTATIVE"),
            "missing STATUS: {result}"
        );
        assert!(result.contains("PRIORITY:5"), "missing PRIORITY: {result}");
    }

    // r[verify model.export.icalendar.geo]
    #[test]
    fn emit_geo() {
        let calendar = make_cal("-//Test//Test//EN");
        let geo = ImportValue::Record(make_record(&[
            ("latitude", ImportValue::String("37.7749".into())),
            ("longitude", ImportValue::String("-122.4194".into())),
        ]));
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            ("uid", ImportValue::String("geo-uid".into())),
            ("geo", geo),
        ]));
        let mut result = String::new();
        emit_icalendar(&mut result, &calendar, &[event], &mut vec![]).unwrap();
        assert!(result.contains("GEO:"), "missing GEO: {result}");
        assert!(result.contains("37.7749"), "missing latitude: {result}");
    }

    // r[verify model.export.icalendar.rrule]
    #[test]
    fn emit_rrule_weekly() {
        let calendar = make_cal("-//Test//Test//EN");
        let recur = ImportValue::Record(make_record(&[
            ("frequency", ImportValue::String("weekly".into())),
            ("interval", ImportValue::Integer(2)),
            ("count", ImportValue::Integer(10)),
            (
                "by_day",
                ImportValue::List(vec![
                    ImportValue::String("monday".into()),
                    ImportValue::String("wednesday".into()),
                ]),
            ),
        ]));
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            ("uid", ImportValue::String("rrule-uid".into())),
            ("start", make_start_dt(2026, 1, 5, 9, 0, 0)),
            ("recur", recur),
        ]));
        let mut result = String::new();
        emit_icalendar(&mut result, &calendar, &[event], &mut vec![]).unwrap();
        assert!(result.contains("RRULE:"), "missing RRULE: {result}");
        assert!(
            result.contains("FREQ=WEEKLY"),
            "missing FREQ=WEEKLY: {result}"
        );
        assert!(
            result.contains("INTERVAL=2"),
            "missing INTERVAL=2: {result}"
        );
        assert!(result.contains("COUNT=10"), "missing COUNT=10: {result}");
    }

    // r[verify model.export.icalendar.x_properties]
    #[test]
    fn emit_x_properties() {
        let calendar = make_record(&[
            ("type", ImportValue::String("calendar".into())),
            ("prod_id", ImportValue::String("-//Test//Test//EN".into())),
            ("x_custom_prop", ImportValue::String("custom-value".into())),
        ]);
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            ("uid", ImportValue::String("x-prop-uid".into())),
            ("x_my_extension", ImportValue::String("ext-value".into())),
        ]));
        let mut result = String::new();
        emit_icalendar(&mut result, &calendar, &[event], &mut vec![]).unwrap();
        assert!(
            result.contains("X-CUSTOM-PROP:custom-value"),
            "missing cal x-prop: {result}"
        );
        assert!(
            result.contains("X-MY-EXTENSION:ext-value"),
            "missing event x-prop: {result}"
        );
    }

    // r[verify model.export.icalendar.duration_negative]
    #[test]
    fn emit_negative_duration() {
        let calendar = make_cal("-//Test//Test//EN");
        let duration = ImportValue::Record(make_record(&[
            ("weeks", ImportValue::SignedInteger(-1)),
            ("days", ImportValue::SignedInteger(0)),
            ("hours", ImportValue::SignedInteger(0)),
            ("minutes", ImportValue::SignedInteger(0)),
            ("seconds", ImportValue::SignedInteger(0)),
        ]));
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            ("uid", ImportValue::String("neg-dur-uid".into())),
            ("duration", duration),
        ]));
        let mut result = String::new();
        emit_icalendar(&mut result, &calendar, &[event], &mut vec![]).unwrap();
        assert!(
            result.contains("DURATION:-P1W"),
            "missing negative duration: {result}"
        );
    }

    // r[verify model.export.icalendar.utc_datetime]
    #[test]
    fn emit_utc_datetime() {
        let calendar = make_cal("-//Test//Test//EN");
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            ("uid", ImportValue::String("utc-uid".into())),
            ("start", make_start_dt(2026, 6, 21, 12, 0, 0)),
            ("time_zone", ImportValue::String("UTC".into())),
        ]));
        let mut result = String::new();
        emit_icalendar(&mut result, &calendar, &[event], &mut vec![]).unwrap();
        assert!(
            result.contains("DTSTART:20260621T120000Z"),
            "expected UTC DTSTART: {result}"
        );
    }

    // r[verify model.export.icalendar.date_only]
    #[test]
    fn emit_date_only_dtstart() {
        let calendar = make_cal("-//Test//Test//EN");
        let start = ImportValue::Record(make_record(&[
            ("year", ImportValue::Integer(2026)),
            ("month", ImportValue::Integer(12)),
            ("day", ImportValue::Integer(25)),
        ]));
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            ("uid", ImportValue::String("date-uid".into())),
            ("start", start),
        ]));
        let mut result = String::new();
        emit_icalendar(&mut result, &calendar, &[event], &mut vec![]).unwrap();
        assert!(
            result.contains("DTSTART;VALUE=DATE:20261225"),
            "expected date-only DTSTART: {result}"
        );
    }

    // r[verify model.export.icalendar.categories]
    #[test]
    fn emit_categories() {
        let calendar = make_cal("-//Test//Test//EN");
        let event = ImportValue::Record(make_record(&[
            ("type", ImportValue::String("event".into())),
            ("uid", ImportValue::String("cat-uid".into())),
            (
                "categories",
                ImportValue::List(vec![
                    ImportValue::String("WORK".into()),
                    ImportValue::String("MEETING".into()),
                ]),
            ),
        ]));
        let mut result = String::new();
        emit_icalendar(&mut result, &calendar, &[event], &mut vec![]).unwrap();
        assert!(
            result.contains("CATEGORIES:"),
            "missing CATEGORIES: {result}"
        );
        assert!(result.contains("WORK"), "missing WORK: {result}");
        assert!(result.contains("MEETING"), "missing MEETING: {result}");
    }

    // r[verify model.export.icalendar.roundtrip_task]
    #[test]
    fn roundtrip_task_fields_with_tzid() {
        let ics = "\
BEGIN:VCALENDAR\r\n\
VERSION:2.0\r\n\
PRODID:-//Roundtrip//Task//EN\r\n\
BEGIN:VTODO\r\n\
UID:roundtrip-task-42\r\n\
SUMMARY:Finish report\r\n\
DESCRIPTION:A task for round-trip testing\r\n\
DTSTART;TZID=Europe/London:20260401T090000\r\n\
DUE;TZID=Europe/London:20260401T170000\r\n\
PERCENT-COMPLETE:75\r\n\
COMPLETED:20260401T160000Z\r\n\
STATUS:IN-PROCESS\r\n\
PRIORITY:2\r\n\
LOCATION:Home Office\r\n\
END:VTODO\r\n\
END:VCALENDAR\r\n";

        // Import
        let import_result = gnomon_import::translate_icalendar(ics).expect("import failed");
        let ImportValue::List(calendars) = import_result else {
            panic!("expected list")
        };
        let ImportValue::Record(cal_rec) = &calendars[0] else {
            panic!("expected record")
        };
        let ImportValue::List(entries) = cal_rec.get("entries").unwrap() else {
            panic!("expected entries list")
        };

        // Export
        let mut output = String::new();
        let mut warnings = Vec::new();
        emit_icalendar(&mut output, cal_rec, entries, &mut warnings).unwrap();
        assert!(warnings.is_empty(), "unexpected warnings: {:?}", warnings);

        // Re-parse
        let cals = calico::model::component::Calendar::parse(&output).expect("re-parse failed");
        let cal = &cals[0];
        let todos: Vec<&Todo> = cal
            .components()
            .iter()
            .filter_map(|c| match c {
                CalendarComponent::Todo(t) => Some(t),
                _ => None,
            })
            .collect();
        assert_eq!(todos.len(), 1, "expected 1 VTODO");
        let todo = todos[0];

        assert_eq!(todo.uid().unwrap().value.as_str(), "roundtrip-task-42");
        assert_eq!(todo.summary().unwrap().value, "Finish report");
        assert_eq!(
            todo.description().unwrap().value,
            "A task for round-trip testing"
        );
        assert_eq!(todo.location().unwrap().value, "Home Office");
        assert_eq!(todo.priority().unwrap().value, Priority::A2);

        // Status
        assert_eq!(todo.status().unwrap().value, Status::InProcess);

        // PERCENT-COMPLETE
        assert_eq!(todo.percent_complete().unwrap().value.get(), 75);

        // COMPLETED
        let completed = todo.completed().unwrap().value;
        assert_eq!(completed.time.hour() as u8, 16);

        // DUE
        let due = todo.due().unwrap();
        let due_tz = due.params.tz_id().expect("DUE should have TZID");
        assert_eq!(due_tz.as_str(), "Europe/London");
    }
}
