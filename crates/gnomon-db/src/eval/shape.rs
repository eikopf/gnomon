//! Shape-checking for Gnomon values.
//!
//! Validates that records conform to the type definitions in the specification.
//! Shape-checking collects all violations as diagnostics without aborting early
//! (`r[impl model.shape.diagnostic]`).

use super::interned::FieldName;
use super::types::{Calendar, Record, Value};
use crate::input::SourceFile;
use crate::queries::{Diagnostic, Severity};

// ── Shape definitions ──────────────────────────────────────

/// Identifies a named record shape for recursive checking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    Location,
    VirtualLocation,
    Link,
    Relation,
    Participant,
    Alert,
    RecurrenceRule,
    NDay,
    LeapMonth,
}

/// A field definition within a shape.
#[derive(Debug, Clone, Copy)]
struct FieldDef {
    name: &'static str,
    required: bool,
    expected: ExpectedType,
}

/// The expected type of a field's value.
#[derive(Debug, Clone, Copy)]
enum ExpectedType {
    String,
    Integer,
    Bool,
    Name,
    /// String or Record (description).
    StringOrRecord,
    /// A record conforming to a named shape.
    Record(Shape),
    /// Any record (no field constraints, e.g. desugared datetime/duration).
    AnyRecord,
    /// List of strings.
    ListOfStrings,
    /// List of records conforming to a named shape.
    ListOfRecords(Shape),
    /// List of unsigned integers in a range.
    ListOfUintRange(u64, u64),
    /// List of nonzero signed integers in a range.
    ListOfNonzeroSignedIntRange(i64, i64),
    /// List of signed integers (no range constraint).
    ListOfSignedIntegers,
    /// One of a fixed set of string values.
    Enum(&'static [&'static str]),
    /// Unsigned integer in a range.
    UintRange(u64, u64),
    /// Strictly positive unsigned integer (> 0).
    PositiveUint,
    /// Nonzero signed integer.
    NonzeroSignedInt,
    /// Recurrence rule termination: datetime record, unsigned integer, or undefined.
    RruleTermination,
}

fn shape_fields(shape: Shape) -> &'static [FieldDef] {
    match shape {
        Shape::Location => &LOCATION_FIELDS,
        Shape::VirtualLocation => &VIRTUAL_LOCATION_FIELDS,
        Shape::Link => &LINK_FIELDS,
        Shape::Relation => &RELATION_FIELDS,
        Shape::Participant => &PARTICIPANT_FIELDS,
        Shape::Alert => &ALERT_FIELDS,
        Shape::RecurrenceRule => &RECURRENCE_RULE_FIELDS,
        Shape::NDay => &NDAY_FIELDS,
        Shape::LeapMonth => &LEAP_MONTH_FIELDS,
    }
}

// ── Calendar fields ────────────────────────────────────────

// r[impl model.calendar.uid]
// r[impl field.uid.type]
const CALENDAR_FIELDS: [FieldDef; 1] = [FieldDef {
    name: "uid",
    required: true,
    expected: ExpectedType::String,
}];

// ── Event-specific fields ──────────────────────────────────

// r[impl record.event.name+2]
// r[impl record.event.start]
// r[impl record.event.uid+2]
// r[impl record.event.duration]
// r[impl record.event.status]
// r[impl record.event.end-time-zone]
const EVENT_FIELDS: [FieldDef; 6] = [
    FieldDef {
        name: "name",
        required: false,
        expected: ExpectedType::Name,
    },
    FieldDef {
        name: "start",
        required: true,
        expected: ExpectedType::AnyRecord,
    },
    FieldDef {
        name: "uid",
        required: true,
        expected: ExpectedType::String,
    },
    FieldDef {
        name: "duration",
        required: false,
        expected: ExpectedType::AnyRecord,
    },
    FieldDef {
        name: "status",
        required: false,
        expected: ExpectedType::Enum(&["tentative", "confirmed", "cancelled"]),
    },
    FieldDef {
        name: "end_time_zone",
        required: false,
        expected: ExpectedType::String,
    },
];

// ── Task-specific fields ───────────────────────────────────

// r[impl record.task.name+2]
// r[impl record.task.uid+2]
// r[impl record.task.due]
// r[impl record.task.start]
// r[impl record.task.estimated-duration]
// r[impl record.task.percent-complete]
// r[impl record.task.progress]
const TASK_FIELDS: [FieldDef; 7] = [
    FieldDef {
        name: "name",
        required: false,
        expected: ExpectedType::Name,
    },
    FieldDef {
        name: "uid",
        required: true,
        expected: ExpectedType::String,
    },
    FieldDef {
        name: "due",
        required: false,
        expected: ExpectedType::AnyRecord,
    },
    FieldDef {
        name: "start",
        required: false,
        expected: ExpectedType::AnyRecord,
    },
    FieldDef {
        name: "estimated_duration",
        required: false,
        expected: ExpectedType::AnyRecord,
    },
    FieldDef {
        name: "percent_complete",
        required: false,
        expected: ExpectedType::UintRange(0, 100),
    },
    FieldDef {
        name: "progress",
        required: false,
        expected: ExpectedType::Enum(&[
            "needs-action",
            "in-process",
            "completed",
            "failed",
            "cancelled",
        ]),
    },
];

// ── Common entry fields (events and tasks) ─────────────────

// r[impl field.title.type]
// r[impl field.description.type]
// r[impl field.description.type.string]
// r[impl field.description.type.record]
// r[impl field.time-zone.type]
// r[impl field.priority.type]
// r[impl field.privacy.type]
// r[impl field.free-busy-status.type]
// r[impl field.show-without-time.type]
// r[impl field.color.type]
// r[impl field.keywords.type]
// r[impl field.categories.type]
// r[impl field.locale.type]
// r[impl field.locations.type]
// r[impl field.virtual-locations.type]
// r[impl field.links.type]
// r[impl field.related-to.type]
// r[impl field.participants.type]
// r[impl field.alerts.type]
// r[impl field.recur.type]
const COMMON_ENTRY_FIELDS: [FieldDef; 18] = [
    FieldDef {
        name: "title",
        required: false,
        expected: ExpectedType::String,
    },
    FieldDef {
        name: "description",
        required: false,
        expected: ExpectedType::StringOrRecord,
    },
    FieldDef {
        name: "time_zone",
        required: false,
        expected: ExpectedType::String,
    },
    FieldDef {
        name: "priority",
        required: false,
        expected: ExpectedType::UintRange(0, 9),
    },
    FieldDef {
        name: "privacy",
        required: false,
        expected: ExpectedType::Enum(&["public", "private", "secret"]),
    },
    FieldDef {
        name: "free_busy_status",
        required: false,
        expected: ExpectedType::Enum(&["free", "busy"]),
    },
    FieldDef {
        name: "show_without_time",
        required: false,
        expected: ExpectedType::Bool,
    },
    FieldDef {
        name: "color",
        required: false,
        expected: ExpectedType::String,
    },
    FieldDef {
        name: "keywords",
        required: false,
        expected: ExpectedType::ListOfStrings,
    },
    FieldDef {
        name: "categories",
        required: false,
        expected: ExpectedType::ListOfStrings,
    },
    FieldDef {
        name: "locale",
        required: false,
        expected: ExpectedType::String,
    },
    FieldDef {
        name: "locations",
        required: false,
        expected: ExpectedType::ListOfRecords(Shape::Location),
    },
    FieldDef {
        name: "virtual_locations",
        required: false,
        expected: ExpectedType::ListOfRecords(Shape::VirtualLocation),
    },
    FieldDef {
        name: "links",
        required: false,
        expected: ExpectedType::ListOfRecords(Shape::Link),
    },
    FieldDef {
        name: "related_to",
        required: false,
        expected: ExpectedType::ListOfRecords(Shape::Relation),
    },
    FieldDef {
        name: "participants",
        required: false,
        expected: ExpectedType::ListOfRecords(Shape::Participant),
    },
    FieldDef {
        name: "alerts",
        required: false,
        expected: ExpectedType::ListOfRecords(Shape::Alert),
    },
    FieldDef {
        name: "recur",
        required: false,
        expected: ExpectedType::Record(Shape::RecurrenceRule),
    },
];

// ── Record type fields ─────────────────────────────────────

// r[impl record.location.syntax]
const LOCATION_FIELDS: [FieldDef; 4] = [
    FieldDef {
        name: "name",
        required: false,
        expected: ExpectedType::String,
    },
    FieldDef {
        name: "location_types",
        required: false,
        expected: ExpectedType::ListOfStrings,
    },
    FieldDef {
        name: "coordinates",
        required: false,
        expected: ExpectedType::String,
    },
    FieldDef {
        name: "links",
        required: false,
        expected: ExpectedType::ListOfRecords(Shape::Link),
    },
];

// r[impl record.virtual-location.syntax]
// r[impl record.virtual-location.uri]
// r[impl record.virtual-location.features]
const VIRTUAL_LOCATION_FIELDS: [FieldDef; 3] = [
    FieldDef {
        name: "name",
        required: false,
        expected: ExpectedType::String,
    },
    FieldDef {
        name: "uri",
        required: true,
        expected: ExpectedType::String,
    },
    FieldDef {
        name: "features",
        required: false,
        expected: ExpectedType::ListOfStrings,
    },
];

// r[impl record.link.syntax]
// r[impl record.link.href]
// r[impl record.link.display]
const LINK_FIELDS: [FieldDef; 6] = [
    FieldDef {
        name: "href",
        required: true,
        expected: ExpectedType::String,
    },
    FieldDef {
        name: "content_type",
        required: false,
        expected: ExpectedType::String,
    },
    FieldDef {
        name: "size",
        required: false,
        expected: ExpectedType::Integer,
    },
    FieldDef {
        name: "rel",
        required: false,
        expected: ExpectedType::String,
    },
    FieldDef {
        name: "display",
        required: false,
        expected: ExpectedType::ListOfStrings,
    },
    FieldDef {
        name: "title",
        required: false,
        expected: ExpectedType::String,
    },
];

// r[impl record.relation.syntax]
// r[impl record.relation.uid]
// r[impl record.relation.relation]
const RELATION_FIELDS: [FieldDef; 2] = [
    FieldDef {
        name: "uid",
        required: true,
        expected: ExpectedType::String,
    },
    FieldDef {
        name: "relation",
        required: false,
        expected: ExpectedType::ListOfStrings,
    },
];

// r[impl record.participant.syntax]
// r[impl record.participant.roles]
const PARTICIPANT_FIELDS: [FieldDef; 8] = [
    FieldDef {
        name: "name",
        required: false,
        expected: ExpectedType::String,
    },
    FieldDef {
        name: "email",
        required: false,
        expected: ExpectedType::String,
    },
    FieldDef {
        name: "description",
        required: false,
        expected: ExpectedType::String,
    },
    FieldDef {
        name: "calendar_address",
        required: false,
        expected: ExpectedType::String,
    },
    FieldDef {
        name: "kind",
        required: false,
        expected: ExpectedType::Enum(&["individual", "group", "location", "resource"]),
    },
    FieldDef {
        name: "roles",
        required: false,
        expected: ExpectedType::ListOfStrings,
    },
    FieldDef {
        name: "participation_status",
        required: false,
        expected: ExpectedType::Enum(&[
            "needs-action",
            "accepted",
            "declined",
            "tentative",
            "delegated",
        ]),
    },
    FieldDef {
        name: "expect_reply",
        required: false,
        expected: ExpectedType::Bool,
    },
];

// r[impl record.alert.syntax]
// r[impl record.alert.trigger]
const ALERT_FIELDS: [FieldDef; 2] = [
    FieldDef {
        name: "trigger",
        required: true,
        expected: ExpectedType::AnyRecord,
    },
    FieldDef {
        name: "action",
        required: false,
        expected: ExpectedType::Enum(&["display", "email"]),
    },
];

// r[impl record.rrule.syntax]
const RECURRENCE_RULE_FIELDS: [FieldDef; 14] = [
    FieldDef {
        name: "frequency",
        required: true,
        expected: ExpectedType::Enum(&[
            "yearly", "monthly", "weekly", "daily", "hourly", "minutely", "secondly",
        ]),
    },
    FieldDef {
        name: "interval",
        required: false,
        expected: ExpectedType::PositiveUint,
    },
    FieldDef {
        name: "skip",
        required: false,
        expected: ExpectedType::Enum(&["omit", "forward", "backward"]),
    },
    FieldDef {
        name: "week_start",
        required: false,
        expected: ExpectedType::Enum(&WEEKDAYS),
    },
    FieldDef {
        name: "termination",
        required: false,
        expected: ExpectedType::RruleTermination,
    },
    FieldDef {
        name: "by_day",
        required: false,
        expected: ExpectedType::ListOfRecords(Shape::NDay),
    },
    FieldDef {
        name: "by_month_day",
        required: false,
        expected: ExpectedType::ListOfNonzeroSignedIntRange(-31, 31),
    },
    FieldDef {
        name: "by_month",
        required: false,
        expected: ExpectedType::ListOfRecords(Shape::LeapMonth),
    },
    FieldDef {
        name: "by_year_day",
        required: false,
        expected: ExpectedType::ListOfNonzeroSignedIntRange(-366, 366),
    },
    FieldDef {
        name: "by_week_no",
        required: false,
        expected: ExpectedType::ListOfNonzeroSignedIntRange(-53, 53),
    },
    FieldDef {
        name: "by_hour",
        required: false,
        expected: ExpectedType::ListOfUintRange(0, 23),
    },
    FieldDef {
        name: "by_minute",
        required: false,
        expected: ExpectedType::ListOfUintRange(0, 59),
    },
    FieldDef {
        name: "by_second",
        required: false,
        expected: ExpectedType::ListOfUintRange(0, 60),
    },
    FieldDef {
        name: "by_set_position",
        required: false,
        expected: ExpectedType::ListOfSignedIntegers,
    },
];

// r[impl record.rrule.weekday]
const WEEKDAYS: [&str; 7] = [
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
];

// r[impl record.rrule.n-day]
const NDAY_FIELDS: [FieldDef; 2] = [
    FieldDef {
        name: "day",
        required: true,
        expected: ExpectedType::Enum(&WEEKDAYS),
    },
    FieldDef {
        name: "nth",
        required: false,
        expected: ExpectedType::NonzeroSignedInt,
    },
];

// r[impl record.rrule.leap-month]
const LEAP_MONTH_FIELDS: [FieldDef; 2] = [
    FieldDef {
        name: "month",
        required: true,
        expected: ExpectedType::PositiveUint,
    },
    FieldDef {
        name: "leap",
        required: true,
        expected: ExpectedType::Bool,
    },
];

// ── Core checking ──────────────────────────────────────────

/// r[impl model.shape.diagnostic]
/// Check a calendar's shape, returning all constraint violations as diagnostics.
pub fn check_calendar_shape<'db>(
    db: &'db dyn crate::Db,
    calendar: &Calendar<'db>,
    root_source: SourceFile,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Use root source for calendar-level diagnostics.
    let calendar_source = root_source;

    // Check calendar properties.
    check_fields(
        db,
        &calendar.properties,
        &CALENDAR_FIELDS,
        calendar_source,
        "calendar",
        &mut diagnostics,
    );

    // r[impl model.entry.type]
    // Check each entry.
    let type_key = FieldName::new(db, "type".to_string());
    let name_key = FieldName::new(db, "name".to_string());
    let uid_key = FieldName::new(db, "uid".to_string());
    for entry in &calendar.entries {
        let source = entry.blame.decl.source(db);
        let entry_type = entry.value.get(&type_key).map(|v| &v.value);

        match entry_type {
            Some(Value::String(t)) if t == "event" => {
                check_fields(db, &entry.value, &EVENT_FIELDS, source, "event", &mut diagnostics);
                check_fields(
                    db,
                    &entry.value,
                    &COMMON_ENTRY_FIELDS,
                    source,
                    "event",
                    &mut diagnostics,
                );
                // r[impl record.event.name+2]
                if entry.value.get(&name_key).is_none() && entry.value.get(&uid_key).is_none() {
                    diagnostics.push(Diagnostic {
                        source,
                        range: rowan::TextRange::default(),
                        severity: Severity::Error,
                        message: "event: must have either `name` or `uid`".into(),
                    });
                }
            }
            Some(Value::String(t)) if t == "task" => {
                check_fields(db, &entry.value, &TASK_FIELDS, source, "task", &mut diagnostics);
                check_fields(
                    db,
                    &entry.value,
                    &COMMON_ENTRY_FIELDS,
                    source,
                    "task",
                    &mut diagnostics,
                );
                // r[impl record.task.name+2]
                if entry.value.get(&name_key).is_none() && entry.value.get(&uid_key).is_none() {
                    diagnostics.push(Diagnostic {
                        source,
                        range: rowan::TextRange::default(),
                        severity: Severity::Error,
                        message: "task: must have either `name` or `uid`".into(),
                    });
                }
            }
            Some(Value::String(t)) => {
                diagnostics.push(Diagnostic {
                    source,
                    range: rowan::TextRange::default(),
                    severity: Severity::Error,
                    message: format!(
                        "entry: field `type` must be \"event\" or \"task\", got \"{t}\""
                    ),
                });
            }
            Some(_) => {
                diagnostics.push(Diagnostic {
                    source,
                    range: rowan::TextRange::default(),
                    severity: Severity::Error,
                    message: "entry: field `type` must be a string".into(),
                });
            }
            None => {
                diagnostics.push(Diagnostic {
                    source,
                    range: rowan::TextRange::default(),
                    severity: Severity::Error,
                    message: "entry: missing required field `type`".into(),
                });
            }
        }
    }

    diagnostics
}

/// Check a record's fields against a set of field definitions.
fn check_fields<'db>(
    db: &'db dyn crate::Db,
    record: &Record<'db>,
    fields: &[FieldDef],
    source: SourceFile,
    context: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for field_def in fields {
        let field_name = FieldName::new(db, field_def.name.to_string());
        match record.get(&field_name) {
            // r[impl model.shape.required]
            None if field_def.required => {
                diagnostics.push(Diagnostic {
                    source,
                    range: rowan::TextRange::default(),
                    severity: Severity::Error,
                    message: format!("{context}: missing required field `{}`", field_def.name),
                });
            }
            None => {}
            Some(blamed_value) => {
                check_value_type(
                    db,
                    &blamed_value.value,
                    field_def.expected,
                    field_def.name,
                    source,
                    context,
                    diagnostics,
                );
            }
        }
    }
    // r[impl model.shape.open]
    // Unknown fields are silently preserved — no checking needed.
}

/// Check that a value conforms to an expected type.
fn check_value_type<'db>(
    db: &'db dyn crate::Db,
    value: &Value<'db>,
    expected: ExpectedType,
    field_name: &str,
    source: SourceFile,
    context: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match expected {
        // r[impl model.shape.type]
        ExpectedType::String => {
            if !matches!(value, Value::String(_)) {
                diagnostics.push(type_error(source, context, field_name, "string", value));
            }
        }
        ExpectedType::Integer => {
            if !matches!(value, Value::Integer(_)) {
                diagnostics.push(type_error(
                    source,
                    context,
                    field_name,
                    "unsigned integer",
                    value,
                ));
            }
        }
        ExpectedType::Bool => {
            if !matches!(value, Value::Bool(_)) {
                diagnostics.push(type_error(source, context, field_name, "boolean", value));
            }
        }
        ExpectedType::Name => {
            if !matches!(value, Value::Name(_)) {
                diagnostics.push(type_error(source, context, field_name, "name", value));
            }
        }
        ExpectedType::StringOrRecord => {
            if !matches!(value, Value::String(_) | Value::Record(_)) {
                diagnostics.push(type_error(
                    source,
                    context,
                    field_name,
                    "string or record",
                    value,
                ));
            }
        }
        // r[impl model.shape.recursive]
        ExpectedType::Record(shape) => {
            if let Value::Record(rec) = value {
                let nested = shape_fields(shape);
                let nested_ctx = format!("{context}.{field_name}");
                check_fields(db, rec, nested, source, &nested_ctx, diagnostics);
            } else {
                diagnostics.push(type_error(source, context, field_name, "record", value));
            }
        }
        ExpectedType::AnyRecord => {
            if !matches!(value, Value::Record(_)) {
                diagnostics.push(type_error(source, context, field_name, "record", value));
            }
        }
        ExpectedType::ListOfStrings => {
            check_list_elements(db, value, field_name, source, context, diagnostics, |v| {
                matches!(v, Value::String(_))
            }, "string");
        }
        ExpectedType::ListOfRecords(shape) => {
            if let Value::List(items) = value {
                for (i, item) in items.iter().enumerate() {
                    if let Value::Record(rec) = &item.value {
                        let nested = shape_fields(shape);
                        let nested_ctx = format!("{context}.{field_name}[{i}]");
                        check_fields(db, rec, nested, source, &nested_ctx, diagnostics);
                    } else {
                        diagnostics.push(type_error(
                            source,
                            context,
                            &format!("{field_name}[{i}]"),
                            "record",
                            &item.value,
                        ));
                    }
                }
            } else {
                diagnostics.push(type_error(source, context, field_name, "list", value));
            }
        }
        ExpectedType::ListOfUintRange(lo, hi) => {
            if let Value::List(items) = value {
                for (i, item) in items.iter().enumerate() {
                    match &item.value {
                        Value::Integer(n) if *n >= lo && *n <= hi => {}
                        Value::Integer(n) => {
                            diagnostics.push(Diagnostic {
                                source,
                                range: rowan::TextRange::default(),
                                severity: Severity::Error,
                                message: format!(
                                    "{context}: element `{field_name}[{i}]` must be in range {lo}..={hi}, got {n}"
                                ),
                            });
                        }
                        other => {
                            diagnostics.push(type_error(
                                source,
                                context,
                                &format!("{field_name}[{i}]"),
                                "unsigned integer",
                                other,
                            ));
                        }
                    }
                }
            } else {
                diagnostics.push(type_error(source, context, field_name, "list", value));
            }
        }
        ExpectedType::ListOfNonzeroSignedIntRange(lo, hi) => {
            if let Value::List(items) = value {
                for (i, item) in items.iter().enumerate() {
                    match &item.value {
                        Value::SignedInteger(n) if *n != 0 && *n >= lo && *n <= hi => {}
                        Value::SignedInteger(n) if *n == 0 => {
                            diagnostics.push(Diagnostic {
                                source,
                                range: rowan::TextRange::default(),
                                severity: Severity::Error,
                                message: format!(
                                    "{context}: element `{field_name}[{i}]` must be nonzero"
                                ),
                            });
                        }
                        Value::SignedInteger(n) => {
                            diagnostics.push(Diagnostic {
                                source,
                                range: rowan::TextRange::default(),
                                severity: Severity::Error,
                                message: format!(
                                    "{context}: element `{field_name}[{i}]` must be in range {lo}..={hi}, got {n}"
                                ),
                            });
                        }
                        other => {
                            diagnostics.push(type_error(
                                source,
                                context,
                                &format!("{field_name}[{i}]"),
                                "signed integer",
                                other,
                            ));
                        }
                    }
                }
            } else {
                diagnostics.push(type_error(source, context, field_name, "list", value));
            }
        }
        ExpectedType::ListOfSignedIntegers => {
            check_list_elements(db, value, field_name, source, context, diagnostics, |v| {
                matches!(v, Value::SignedInteger(_))
            }, "signed integer");
        }
        // r[impl model.shape.enum]
        ExpectedType::Enum(variants) => {
            if let Value::String(s) = value {
                if !variants.contains(&s.as_str()) {
                    let allowed = variants
                        .iter()
                        .map(|v| format!("\"{v}\""))
                        .collect::<Vec<_>>()
                        .join(", ");
                    diagnostics.push(Diagnostic {
                        source,
                        range: rowan::TextRange::default(),
                        severity: Severity::Error,
                        message: format!(
                            "{context}: field `{field_name}` must be one of {allowed}, got \"{s}\""
                        ),
                    });
                }
            } else {
                diagnostics.push(type_error(source, context, field_name, "string", value));
            }
        }
        ExpectedType::UintRange(lo, hi) => {
            match value {
                Value::Integer(n) if *n >= lo && *n <= hi => {}
                Value::Integer(n) => {
                    diagnostics.push(Diagnostic {
                        source,
                        range: rowan::TextRange::default(),
                        severity: Severity::Error,
                        message: format!(
                            "{context}: field `{field_name}` must be in range {lo}..={hi}, got {n}"
                        ),
                    });
                }
                _ => {
                    diagnostics.push(type_error(
                        source,
                        context,
                        field_name,
                        "unsigned integer",
                        value,
                    ));
                }
            }
        }
        ExpectedType::PositiveUint => match value {
            Value::Integer(n) if *n > 0 => {}
            Value::Integer(0) => {
                diagnostics.push(Diagnostic {
                    source,
                    range: rowan::TextRange::default(),
                    severity: Severity::Error,
                    message: format!(
                        "{context}: field `{field_name}` must be a positive integer, got 0"
                    ),
                });
            }
            _ => {
                diagnostics.push(type_error(
                    source,
                    context,
                    field_name,
                    "positive integer",
                    value,
                ));
            }
        },
        ExpectedType::NonzeroSignedInt => match value {
            Value::SignedInteger(n) if *n != 0 => {}
            Value::SignedInteger(0) => {
                diagnostics.push(Diagnostic {
                    source,
                    range: rowan::TextRange::default(),
                    severity: Severity::Error,
                    message: format!(
                        "{context}: field `{field_name}` must be a nonzero signed integer, got 0"
                    ),
                });
            }
            _ => {
                diagnostics.push(type_error(
                    source,
                    context,
                    field_name,
                    "nonzero signed integer",
                    value,
                ));
            }
        },
        ExpectedType::RruleTermination => {
            // Accepts: record (datetime), unsigned integer (count), or undefined.
            if !matches!(
                value,
                Value::Record(_) | Value::Integer(_) | Value::Undefined
            ) {
                diagnostics.push(type_error(
                    source,
                    context,
                    field_name,
                    "datetime record, unsigned integer, or undefined",
                    value,
                ));
            }
        }
    }
}

/// Check that a value is a list and each element satisfies a predicate.
fn check_list_elements<'db>(
    _db: &'db dyn crate::Db,
    value: &Value<'db>,
    field_name: &str,
    source: SourceFile,
    context: &str,
    diagnostics: &mut Vec<Diagnostic>,
    pred: impl Fn(&Value<'db>) -> bool,
    expected_elem_type: &str,
) {
    if let Value::List(items) = value {
        for (i, item) in items.iter().enumerate() {
            if !pred(&item.value) {
                diagnostics.push(type_error(
                    source,
                    context,
                    &format!("{field_name}[{i}]"),
                    expected_elem_type,
                    &item.value,
                ));
            }
        }
    } else {
        diagnostics.push(type_error(source, context, field_name, "list", value));
    }
}

// ── Helpers ────────────────────────────────────────────────

fn type_error(
    source: SourceFile,
    context: &str,
    field_name: &str,
    expected: &str,
    actual: &Value<'_>,
) -> Diagnostic {
    Diagnostic {
        source,
        range: rowan::TextRange::default(),
        severity: Severity::Error,
        message: format!(
            "{context}: field `{field_name}` expected {expected}, got {}",
            value_type_name(actual)
        ),
    }
}

fn value_type_name(value: &Value<'_>) -> &'static str {
    match value {
        Value::String(_) => "string",
        Value::Integer(_) => "unsigned integer",
        Value::SignedInteger(_) => "signed integer",
        Value::Bool(_) => "boolean",
        Value::Undefined => "undefined",
        Value::Name(_) => "name",
        Value::Record(_) => "record",
        Value::List(_) => "list",
        Value::Path(_) => "path",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Database;
    use std::path::PathBuf;

    /// Helper: evaluate and validate, returning shape-checking diagnostics only.
    fn shape_diags(db: &Database, files: &[(&str, &str)]) -> Vec<String> {
        let (path, text) = files[0];
        let source = SourceFile::new(db, PathBuf::from(path), text.into());
        let eval = crate::evaluate(db, source);
        let result = crate::eval::merge::validate_calendar(db, source, eval.value, eval.diagnostics);
        // Shape diagnostics are appended after validation diagnostics;
        // filter to only shape-related ones by re-running the check.
        let diags = check_calendar_shape(db, &result.calendar, source);
        diags.iter().map(|d| d.message.clone()).collect()
    }

    /// Helper: evaluate and validate, returning all diagnostics.
    fn all_diags(db: &Database, files: &[(&str, &str)]) -> Vec<String> {
        let (path, text) = files[0];
        let source = SourceFile::new(db, PathBuf::from(path), text.into());
        let eval = crate::evaluate(db, source);
        let result = crate::eval::merge::validate_calendar(db, source, eval.value, eval.diagnostics);
        result.diagnostics.iter().map(|d| d.message.clone()).collect()
    }

    // ── Missing mandatory fields ────────────────────────────

    // r[verify model.shape.required]
    // r[verify model.calendar.uid]
    #[test]
    fn calendar_missing_uid() {
        let db = Database::default();
        let diags = shape_diags(&db, &[("a.gnomon", "calendar {}")]);
        assert!(
            diags.iter().any(|d| d.contains("missing required field `uid`")),
            "expected uid error, got: {diags:?}"
        );
    }

    // r[verify model.shape.required]
    // r[verify record.event.name+2]
    #[test]
    fn event_missing_name_and_uid() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "test" }
                event { start: 2026-03-01T09:00 }
                "#,
            )],
        );
        assert!(
            diags.iter().any(|d| d.contains("must have either `name` or `uid`")),
            "expected name-or-uid error, got: {diags:?}"
        );
    }

    // r[verify model.shape.required]
    // r[verify record.event.start]
    #[test]
    fn event_missing_start() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "test" }
                event { name: @foo }
                "#,
            )],
        );
        assert!(
            diags.iter().any(|d| d.contains("event: missing required field `start`")),
            "expected start error, got: {diags:?}"
        );
    }

    // r[verify model.shape.required]
    // r[verify record.task.name+2]
    #[test]
    fn task_missing_name_and_uid() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "test" }
                task { title: "no name" }
                "#,
            )],
        );
        assert!(
            diags.iter().any(|d| d.contains("must have either `name` or `uid`")),
            "expected name-or-uid error, got: {diags:?}"
        );
    }

    // ── Type mismatches ─────────────────────────────────────

    // r[verify model.shape.type]
    // r[verify field.priority.type]
    #[test]
    fn event_priority_wrong_type() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "test" }
                event @foo 2026-03-01T09:00 1h "Foo" { priority: "high" }
                "#,
            )],
        );
        assert!(
            diags.iter().any(|d| d.contains("priority") && d.contains("unsigned integer")),
            "expected priority type error, got: {diags:?}"
        );
    }

    // r[verify field.priority.type]
    #[test]
    fn event_priority_out_of_range() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "test" }
                event @foo 2026-03-01T09:00 1h "Foo" { priority: 15 }
                "#,
            )],
        );
        assert!(
            diags.iter().any(|d| d.contains("priority") && d.contains("0..=9")),
            "expected priority range error, got: {diags:?}"
        );
    }

    // r[verify model.shape.type]
    #[test]
    fn calendar_uid_wrong_type() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[("a.gnomon", "calendar { uid: 42 }")],
        );
        assert!(
            diags.iter().any(|d| d.contains("uid") && d.contains("string")),
            "expected uid type error, got: {diags:?}"
        );
    }

    // ── Enum violations ─────────────────────────────────────

    // r[verify model.shape.enum]
    // r[verify record.event.status]
    #[test]
    fn event_status_invalid() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "test" }
                event @foo 2026-03-01T09:00 1h "Foo" { status: "bogus" }
                "#,
            )],
        );
        assert!(
            diags.iter().any(|d| d.contains("status") && d.contains("bogus")),
            "expected status enum error, got: {diags:?}"
        );
    }

    // r[verify model.shape.enum]
    // r[verify field.privacy.type]
    #[test]
    fn event_privacy_invalid() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "test" }
                event @foo 2026-03-01T09:00 1h "Foo" { privacy: "open" }
                "#,
            )],
        );
        assert!(
            diags.iter().any(|d| d.contains("privacy") && d.contains("open")),
            "expected privacy enum error, got: {diags:?}"
        );
    }

    // ── Open records ────────────────────────────────────────

    // r[verify model.shape.open]
    #[test]
    fn unknown_fields_no_diagnostics() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "f47ac10b-58cc-4372-a567-0e02b2c3d479", custom_field: "hello" }
                event @foo 2026-03-01T09:00 1h "Foo" { x_custom: 42 }
                "#,
            )],
        );
        assert!(
            diags.is_empty(),
            "expected no diagnostics for unknown fields, got: {diags:?}"
        );
    }

    // ── Recursive checking ──────────────────────────────────

    // r[verify model.shape.recursive]
    // r[verify record.virtual-location.syntax]
    // r[verify record.virtual-location.uri]
    #[test]
    fn nested_virtual_location_missing_uri() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "test" }
                event @foo 2026-03-01T09:00 1h "Foo" {
                    virtual_locations: [{ name: "Zoom" }]
                }
                "#,
            )],
        );
        assert!(
            diags.iter().any(|d| d.contains("virtual_locations") && d.contains("uri")),
            "expected nested uri error, got: {diags:?}"
        );
    }

    // ── Multiple violations ─────────────────────────────────

    // r[verify model.shape.diagnostic]
    #[test]
    fn multiple_violations_all_reported() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar {}
                event { title: "no name or start" }
                "#,
            )],
        );
        // Should report: calendar missing uid, event missing start, event missing name-or-uid
        let has_cal_uid = diags.iter().any(|d| d.contains("calendar: missing required field `uid`"));
        let has_start = diags.iter().any(|d| d.contains("missing required field `start`"));
        let has_name_or_uid = diags.iter().any(|d| d.contains("must have either `name` or `uid`"));
        assert!(
            has_cal_uid && has_start && has_name_or_uid,
            "expected all three violations, got: {diags:?}"
        );
    }

    // ── Valid records ───────────────────────────────────────

    // r[verify field.recur.type]
    #[test]
    fn valid_calendar_no_diagnostics() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "f47ac10b-58cc-4372-a567-0e02b2c3d479" }
                event @meeting 2026-03-01T14:30 1h "Standup"
                task @review "Code review"
                "#,
            )],
        );
        assert!(
            diags.is_empty(),
            "expected no diagnostics for valid input, got: {diags:?}"
        );
    }

    // r[verify record.event.name+2]
    // r[verify record.event.uid+2]
    #[test]
    fn event_with_uid_but_no_name_is_valid() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "f47ac10b-58cc-4372-a567-0e02b2c3d479" }
                event { uid: "imported-uid", start: 2026-03-01T09:00, title: "From iCal" }
                "#,
            )],
        );
        assert!(
            diags.is_empty(),
            "expected no diagnostics for event with uid but no name, got: {diags:?}"
        );
    }

    // r[verify record.task.name+2]
    // r[verify record.task.uid+2]
    #[test]
    fn task_with_uid_but_no_name_is_valid() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "f47ac10b-58cc-4372-a567-0e02b2c3d479" }
                task { uid: "imported-uid", title: "From JSCal" }
                "#,
            )],
        );
        assert!(
            diags.is_empty(),
            "expected no diagnostics for task with uid but no name, got: {diags:?}"
        );
    }

    // r[verify field.title.type]
    // r[verify field.priority.type]
    // r[verify field.privacy.type]
    // r[verify field.free-busy-status.type]
    // r[verify field.show-without-time.type]
    // r[verify field.color.type]
    // r[verify field.keywords.type]
    // r[verify field.categories.type]
    // r[verify field.locale.type]
    // r[verify field.time-zone.type]
    #[test]
    fn valid_event_with_all_common_fields() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r##"
                calendar { uid: "f47ac10b-58cc-4372-a567-0e02b2c3d479" }
                event @meeting 2026-03-01T14:30 1h "Standup" {
                    priority: 5,
                    privacy: "public",
                    free_busy_status: "busy",
                    show_without_time: false,
                    color: "#ff0000",
                    keywords: ["work", "daily"],
                    categories: ["meetings"],
                    locale: "en-US",
                    time_zone: "America/New_York"
                }
                "##,
            )],
        );
        assert!(
            diags.is_empty(),
            "expected no diagnostics, got: {diags:?}"
        );
    }

    // r[verify record.task.progress]
    // r[verify record.task.percent-complete]
    #[test]
    fn task_with_progress_valid() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "f47ac10b-58cc-4372-a567-0e02b2c3d479" }
                task @review "Code review" { progress: "in-process", percent_complete: 50 }
                "#,
            )],
        );
        assert!(
            diags.is_empty(),
            "expected no diagnostics, got: {diags:?}"
        );
    }

    // r[verify record.task.progress]
    // r[verify model.shape.enum]
    #[test]
    fn task_progress_invalid_enum() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "test" }
                task @review "Code review" { progress: "done" }
                "#,
            )],
        );
        assert!(
            diags.iter().any(|d| d.contains("progress") && d.contains("done")),
            "expected progress enum error, got: {diags:?}"
        );
    }

    // r[verify record.task.percent-complete]
    #[test]
    fn task_percent_complete_out_of_range() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "test" }
                task @review "Code review" { percent_complete: 150 }
                "#,
            )],
        );
        assert!(
            diags.iter().any(|d| d.contains("percent_complete") && d.contains("0..=100")),
            "expected percent_complete range error, got: {diags:?}"
        );
    }

    // ── Shape-checking wired into validation ────────────────

    // r[verify model.shape.diagnostic]
    #[test]
    fn shape_errors_appear_in_check_diagnostics() {
        let db = Database::default();
        let diags = all_diags(
            &db,
            &[("a.gnomon", "calendar {}")],
        );
        assert!(
            diags.iter().any(|d| d.contains("missing required field `uid`")),
            "expected uid error in check diagnostics, got: {diags:?}"
        );
    }

    // ── Common entry field type violations ───────────────────

    // r[verify field.description.type]
    #[test]
    fn description_wrong_type() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "test" }
                event @e 2026-03-01T09:00 1h "E" { description: 42 }
                "#,
            )],
        );
        assert!(
            diags.iter().any(|d| d.contains("description")),
            "expected description type error, got: {diags:?}"
        );
    }

    // r[verify field.locations.type]
    #[test]
    fn locations_wrong_type() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "test" }
                event @e 2026-03-01T09:00 1h "E" { locations: "bad" }
                "#,
            )],
        );
        assert!(
            diags.iter().any(|d| d.contains("locations")),
            "expected locations type error, got: {diags:?}"
        );
    }

    // r[verify field.virtual-locations.type]
    #[test]
    fn virtual_locations_wrong_type() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "test" }
                event @e 2026-03-01T09:00 1h "E" { virtual_locations: "bad" }
                "#,
            )],
        );
        assert!(
            diags.iter().any(|d| d.contains("virtual_locations")),
            "expected virtual_locations type error, got: {diags:?}"
        );
    }

    // r[verify field.links.type]
    #[test]
    fn links_wrong_type() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "test" }
                event @e 2026-03-01T09:00 1h "E" { links: "bad" }
                "#,
            )],
        );
        assert!(
            diags.iter().any(|d| d.contains("links")),
            "expected links type error, got: {diags:?}"
        );
    }

    // r[verify field.related-to.type]
    #[test]
    fn related_to_wrong_type() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "test" }
                event @e 2026-03-01T09:00 1h "E" { related_to: "bad" }
                "#,
            )],
        );
        assert!(
            diags.iter().any(|d| d.contains("related_to")),
            "expected related_to type error, got: {diags:?}"
        );
    }

    // r[verify field.participants.type]
    #[test]
    fn participants_wrong_type() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "test" }
                event @e 2026-03-01T09:00 1h "E" { participants: "bad" }
                "#,
            )],
        );
        assert!(
            diags.iter().any(|d| d.contains("participants")),
            "expected participants type error, got: {diags:?}"
        );
    }

    // r[verify field.alerts.type]
    #[test]
    fn alerts_wrong_type() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "test" }
                event @e 2026-03-01T09:00 1h "E" { alerts: "bad" }
                "#,
            )],
        );
        assert!(
            diags.iter().any(|d| d.contains("alerts")),
            "expected alerts type error, got: {diags:?}"
        );
    }

    // ── Sub-record structure tests ──────────────────────────

    // r[verify record.alert.syntax]
    // r[verify record.alert.trigger]
    #[test]
    fn alert_missing_trigger() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "test" }
                event @e 2026-03-01T09:00 1h "E" {
                    alerts: [ { action: "display" } ]
                }
                "#,
            )],
        );
        assert!(
            diags.iter().any(|d| d.contains("trigger")),
            "expected trigger required error, got: {diags:?}"
        );
    }

    // r[verify record.link.syntax]
    // r[verify record.link.href]
    #[test]
    fn link_missing_href() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "test" }
                event @e 2026-03-01T09:00 1h "E" {
                    links: [ { title: "Example" } ]
                }
                "#,
            )],
        );
        assert!(
            diags.iter().any(|d| d.contains("href")),
            "expected href required error, got: {diags:?}"
        );
    }

    // r[verify record.link.display]
    #[test]
    fn link_display_wrong_type() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "test" }
                event @e 2026-03-01T09:00 1h "E" {
                    links: [ { href: "https://example.com", display: 42 } ]
                }
                "#,
            )],
        );
        assert!(
            diags.iter().any(|d| d.contains("display")),
            "expected display type error, got: {diags:?}"
        );
    }

    // r[verify record.location.syntax]
    #[test]
    fn location_valid() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "f47ac10b-58cc-4372-a567-0e02b2c3d479" }
                event @e 2026-03-01T09:00 1h "E" {
                    locations: [ { name: "HQ", coordinates: "40.7,-74.0" } ]
                }
                "#,
            )],
        );
        assert!(diags.is_empty(), "expected no errors, got: {diags:?}");
    }

    // r[verify record.relation.syntax]
    // r[verify record.relation.uid]
    #[test]
    fn relation_missing_uid() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "test" }
                event @e 2026-03-01T09:00 1h "E" {
                    related_to: [ { relation: ["next"] } ]
                }
                "#,
            )],
        );
        assert!(
            diags.iter().any(|d| d.contains("uid")),
            "expected uid required error, got: {diags:?}"
        );
    }

    // r[verify record.relation.relation]
    #[test]
    fn relation_relation_wrong_type() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "test" }
                event @e 2026-03-01T09:00 1h "E" {
                    related_to: [ { uid: "other", relation: 42 } ]
                }
                "#,
            )],
        );
        assert!(
            diags.iter().any(|d| d.contains("relation")),
            "expected relation type error, got: {diags:?}"
        );
    }

    // r[verify record.participant.syntax]
    // r[verify record.participant.roles]
    #[test]
    fn participant_roles_wrong_type() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "test" }
                event @e 2026-03-01T09:00 1h "E" {
                    participants: [ { name: "Alice", roles: 42 } ]
                }
                "#,
            )],
        );
        assert!(
            diags.iter().any(|d| d.contains("roles")),
            "expected roles type error, got: {diags:?}"
        );
    }

    // r[verify record.virtual-location.features]
    #[test]
    fn virtual_location_features_wrong_type() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "test" }
                event @e 2026-03-01T09:00 1h "E" {
                    virtual_locations: [ { uri: "https://meet.example.com", features: 42 } ]
                }
                "#,
            )],
        );
        assert!(
            diags.iter().any(|d| d.contains("features")),
            "expected features type error, got: {diags:?}"
        );
    }

    // r[verify record.rrule.leap-month]
    #[test]
    fn leap_month_valid() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "f47ac10b-58cc-4372-a567-0e02b2c3d479" }
                event @e 2026-03-01T09:00 1h "E" {
                    recur: {
                        frequency: "yearly",
                        by_month: [ { month: 3, leap: false } ]
                    }
                }
                "#,
            )],
        );
        assert!(diags.is_empty(), "expected no errors, got: {diags:?}");
    }

    // r[verify record.rrule.leap-month]
    #[test]
    fn leap_month_missing_fields() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "test" }
                event @e 2026-03-01T09:00 1h "E" {
                    recur: {
                        frequency: "yearly",
                        by_month: [ { month: 3 } ]
                    }
                }
                "#,
            )],
        );
        assert!(
            diags.iter().any(|d| d.contains("leap")),
            "expected leap required error, got: {diags:?}"
        );
    }

    // ── Event/Task specific field tests ─────────────────────

    // r[verify record.event.end-time-zone]
    #[test]
    fn event_end_time_zone_wrong_type() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "test" }
                event @e 2026-03-01T09:00 1h "E" { end_time_zone: 42 }
                "#,
            )],
        );
        assert!(
            diags.iter().any(|d| d.contains("end_time_zone")),
            "expected end_time_zone type error, got: {diags:?}"
        );
    }

    // r[verify record.task.start]
    #[test]
    fn task_start_wrong_type() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "test" }
                task @t "Review" { start: "bad" }
                "#,
            )],
        );
        assert!(
            diags.iter().any(|d| d.contains("start")),
            "expected start type error, got: {diags:?}"
        );
    }

    // r[verify record.task.estimated-duration]
    #[test]
    fn task_estimated_duration_wrong_type() {
        let db = Database::default();
        let diags = shape_diags(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "test" }
                task @t "Review" { estimated_duration: "bad" }
                "#,
            )],
        );
        assert!(
            diags.iter().any(|d| d.contains("estimated_duration")),
            "expected estimated_duration type error, got: {diags:?}"
        );
    }
}
