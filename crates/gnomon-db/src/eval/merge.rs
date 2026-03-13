use std::collections::HashMap;

use uuid::Uuid;

use super::interned::{DeclKind, FieldName};
use super::types::{Blamed, Calendar, Record, Value};
use crate::input::SourceFile;
use crate::queries::{Diagnostic, Severity};

/// Result of validating a calendar.
pub struct CheckResult<'db> {
    pub calendars: Vec<Calendar<'db>>,
    /// All diagnostics: parse, validation, lowering, and check-level.
    pub diagnostics: Vec<Diagnostic>,
    pub has_errors: bool,
}

/// Validate a pre-evaluated value as a calendar.
///
/// Takes the root source file (for diagnostic attribution), the evaluated value,
/// and any diagnostics from evaluation. Flattens the value into records, checks
/// uniqueness constraints (single calendar, unique names), derives UIDs,
/// runs shape-checking, and expands recurrence rules.
// r[impl model.shape.diagnostic]
pub fn validate_calendar<'db>(
    db: &'db dyn crate::Db,
    root_source: SourceFile,
    value: Value<'db>,
    eval_diagnostics: Vec<Diagnostic>,
) -> CheckResult<'db> {
    let mut calendars: Vec<Calendar<'db>> = Vec::new();
    let mut loose_entries: Vec<Blamed<'db, Record<'db>>> = Vec::new();
    let mut diagnostics = Vec::new();
    let mut has_errors = false;

    // Collect parse + validation diagnostics from tracked queries on root source.
    let check_diags = crate::check_syntax::accumulated::<Diagnostic>(db, root_source);
    for diag in check_diags {
        has_errors |= diag.severity == Severity::Error;
        diagnostics.push(diag.clone());
    }

    // Fold in eval diagnostics (includes diagnostics from imported files).
    for diag in eval_diagnostics {
        has_errors |= diag.severity == Severity::Error;
        diagnostics.push(diag);
    }

    // Track names for collision detection (global namespace across events/tasks).
    let mut seen_names: HashMap<String, SourceFile> = HashMap::new();

    let name_key = FieldName::new(db, "name".to_string());
    let type_key = FieldName::new(db, "type".to_string());
    let entries_key = FieldName::new(db, "entries".to_string());

    // r[impl model.calendar.entries]
    // Flatten value into records.
    let records = flatten_to_records(db, root_source, value);

    for (record, blame) in records {
        let source = blame.decl.source(db);
        let type_val = record.get(&type_key).map(|v| &v.value);

        match type_val {
            Some(Value::String(s)) if s == "event" || s == "task" => {
                check_name_collision(
                    db,
                    &record,
                    &name_key,
                    source,
                    &mut seen_names,
                    &mut diagnostics,
                    &mut has_errors,
                );
                loose_entries.push(Blamed {
                    value: record,
                    blame,
                });
            }
            // r[impl model.calendar.singular+4]
            Some(Value::String(s)) if s == "calendar" => {
                // r[impl model.calendar.uid+2]
                // Gnomon `calendar { ... }` expressions have DeclKind::Calendar;
                // foreign import results have DeclKind::Expr.
                let foreign_import = blame.decl.kind(db) != DeclKind::Calendar;
                let mut calendar = Calendar {
                    foreign_import,
                    ..Calendar::default()
                };

                // Extract nested entries from the calendar record's `entries` field.
                if let Some(entries_blamed) = record.get(&entries_key)
                    && let Value::List(items) = &entries_blamed.value
                {
                    for item in items {
                        if let Value::Record(r) = &item.value {
                            check_name_collision(
                                db,
                                r,
                                &name_key,
                                source,
                                &mut seen_names,
                                &mut diagnostics,
                                &mut has_errors,
                            );
                            calendar.entries.push(Blamed {
                                value: r.clone(),
                                blame: item.blame.clone(),
                            });
                        }
                    }
                }

                calendar.properties = record;
                calendars.push(calendar);
            }
            _ => {
                has_errors = true;
                diagnostics.push(Diagnostic {
                    source,
                    range: rowan::TextRange::default(),
                    severity: Severity::Error,
                    message: "record has no recognized type field".into(),
                });
            }
        }
    }

    // Distribute loose (top-level) entries.
    if !loose_entries.is_empty() {
        if calendars.len() == 1 {
            calendars[0].entries.extend(loose_entries);
        } else if calendars.is_empty() {
            // Will be reported as "no calendar record" below.
        } else {
            has_errors = true;
            diagnostics.push(Diagnostic {
                source: root_source,
                range: rowan::TextRange::default(),
                severity: Severity::Error,
                message:
                    "top-level event/task records are ambiguous when multiple calendars exist; \
                          nest them inside a calendar's entries field instead"
                        .into(),
            });
        }
    }

    // Check that at least one calendar was found.
    if calendars.is_empty() {
        has_errors = true;
        diagnostics.push(Diagnostic {
            source: root_source,
            range: rowan::TextRange::default(),
            severity: Severity::Error,
            message: "no calendar record found".into(),
        });
    }

    // Per-calendar post-processing.
    for calendar in &mut calendars {
        // r[impl model.calendar.uid.derivation]
        // Derive UUIDv5 UIDs for entries that omit an explicit uid.
        derive_uids(db, calendar, root_source, &mut diagnostics, &mut has_errors);

        // Shape-check the merged calendar.
        let shape_diags = super::shape::check_calendar_shape(db, calendar, root_source);
        for diag in shape_diags {
            has_errors |= diag.severity == Severity::Error;
            diagnostics.push(diag);
        }

        // Validate recurrence rules (without materializing occurrences).
        super::rrule::validate_entry_recurrences(db, calendar, &mut diagnostics, &mut has_errors);
    }

    CheckResult {
        calendars,
        diagnostics,
        has_errors,
    }
}

// r[impl model.calendar.uid.derivation.non-uuid]
/// Derive UUIDv5 UIDs for entries that omit an explicit `uid` field.
///
/// Uses the calendar's `uid` as the UUIDv5 namespace and the entry's `name`
/// as the key. If the calendar uid is not a valid UUID, emits a diagnostic
/// and skips derivation.
fn derive_uids<'db>(
    db: &'db dyn crate::Db,
    calendar: &mut Calendar<'db>,
    root_source: SourceFile,
    diagnostics: &mut Vec<Diagnostic>,
    _has_errors: &mut bool,
) {
    let uid_key = FieldName::new(db, "uid".to_string());
    let name_key = FieldName::new(db, "name".to_string());

    // Extract and parse the calendar uid as a UUID namespace.
    let namespace = match calendar.properties.get(&uid_key) {
        Some(blamed) => match &blamed.value {
            Value::String(s) => match Uuid::parse_str(s) {
                Ok(uuid) => uuid,
                Err(_) => {
                    diagnostics.push(Diagnostic {
                        source: root_source,
                        range: rowan::TextRange::default(),
                        severity: Severity::Warning,
                        message: format!(
                            "calendar uid \"{}\" is not a valid UUID; cannot derive entry UIDs",
                            s
                        ),
                    });
                    return;
                }
            },
            _ => return, // Non-string uid; shape-check will report this.
        },
        None => return, // Missing uid; shape-check will report this.
    };

    for entry in &mut calendar.entries {
        // Skip entries that already have a uid.
        if entry.value.get(&uid_key).is_some() {
            continue;
        }

        // Extract the name to use as the UUIDv5 key.
        let name_str = match entry.value.get(&name_key) {
            Some(blamed) => match &blamed.value {
                Value::Name(s) => s.clone(),
                _ => continue, // Non-name value; shape-check will report this.
            },
            None => continue, // Missing name; shape-check will report this.
        };

        let derived = Uuid::new_v5(&namespace, name_str.as_bytes());
        entry.value.insert(
            uid_key,
            Blamed {
                value: Value::String(derived.to_string()),
                blame: entry.blame.clone(),
            },
        );
    }
}

// r[impl model.name.unique]
fn check_name_collision<'db>(
    db: &'db dyn crate::Db,
    record: &super::types::Record<'db>,
    name_key: &FieldName<'db>,
    source: SourceFile,
    seen_names: &mut HashMap<String, SourceFile>,
    diagnostics: &mut Vec<Diagnostic>,
    has_errors: &mut bool,
) {
    if let Some(blamed_value) = record.get(name_key)
        && let Value::Name(name) = &blamed_value.value
    {
        if let Some(&first_source) = seen_names.get(name) {
            *has_errors = true;
            diagnostics.push(Diagnostic {
                source,
                range: rowan::TextRange::default(),
                severity: Severity::Error,
                message: format!(
                    "name @{} already defined in {}",
                    name,
                    first_source.path(db).display()
                ),
            });
        } else {
            seen_names.insert(name.clone(), source);
        }
    }
}

/// Flatten a Value into a list of (Record, Blame) pairs for validation.
/// A single record becomes a one-element list; a list is iterated.
fn flatten_to_records<'db>(
    db: &'db dyn crate::Db,
    source: SourceFile,
    value: Value<'db>,
) -> Vec<(Record<'db>, super::types::Blame<'db>)> {
    use super::interned::{DeclId, DeclKind, FieldPath};

    let default_blame = || super::types::Blame {
        decl: DeclId::new(db, source, 0, DeclKind::Calendar),
        path: FieldPath::root(),
    };

    match value {
        Value::Record(r) => {
            let blame =
                r.0.values()
                    .next()
                    .map(|b| b.blame.clone())
                    .unwrap_or_else(default_blame);
            vec![(r, blame)]
        }
        Value::List(items) => {
            let mut result = Vec::new();
            for item in items {
                match item.value {
                    Value::Record(r) => {
                        result.push((r, item.blame));
                    }
                    _ => {
                        // Non-record items in a list are skipped during validation
                    }
                }
            }
            result
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Database;
    use crate::eval::render::RenderWithDb;
    use expect_test::{Expect, expect};
    use std::path::PathBuf;

    fn make_source(db: &Database, path: &str, text: &str) -> SourceFile {
        SourceFile::new(db, PathBuf::from(path), text.into())
    }

    /// Evaluate a source file and validate the result as a calendar.
    fn check(db: &Database, source: SourceFile) -> CheckResult<'_> {
        let result = crate::evaluate(db, source);
        validate_calendar(db, source, result.value, result.diagnostics)
    }

    fn check_output(db: &Database, text: &str, expected: Expect) {
        let source = make_source(db, "test.gnomon", text);
        let result = check(db, source);
        assert!(!result.calendars.is_empty(), "no calendars found");
        let rendered = format!("{}", result.calendars[0].render(db));
        expected.assert_eq(&rendered);
    }

    fn check_diagnostics(db: &Database, text: &str) -> Vec<String> {
        let source = make_source(db, "test.gnomon", text);
        let result = check(db, source);
        result
            .diagnostics
            .iter()
            .map(|d| d.message.clone())
            .collect()
    }

    // r[verify model.calendar.entries]
    #[test]
    fn single_file_with_calendar_and_event() {
        let db = Database::default();
        check_output(
            &db,
            r#"
            calendar { uid: "test" }
            event @meeting 2026-03-01T14:30 1h "Standup"
            "#,
            expect![[r#"
                Calendar {
                    properties: {
                        type: "calendar",
                        uid: "test",
                    },
                    entries: [
                        {
                            duration: {
                                days: 0,
                                hours: 1,
                                minutes: 0,
                                seconds: 0,
                                weeks: 0,
                            },
                            name: @meeting,
                            start: {
                                date: {
                                    day: 1,
                                    month: 3,
                                    year: 2026,
                                },
                                time: {
                                    hour: 14,
                                    minute: 30,
                                    second: 0,
                                },
                            },
                            title: "Standup",
                            type: "event",
                        },
                    ],
                }"#]],
        );
    }

    // r[verify model.calendar.entries]
    #[test]
    fn calendar_with_multiple_entries() {
        let db = Database::default();
        check_output(
            &db,
            r#"
            calendar { uid: "cal" }
            event @a 2026-01-01T09:00 1h "A"
            event @b 2026-02-01T10:00 2h "B"
            "#,
            expect![[r#"
                Calendar {
                    properties: {
                        type: "calendar",
                        uid: "cal",
                    },
                    entries: [
                        {
                            duration: {
                                days: 0,
                                hours: 1,
                                minutes: 0,
                                seconds: 0,
                                weeks: 0,
                            },
                            name: @a,
                            start: {
                                date: {
                                    day: 1,
                                    month: 1,
                                    year: 2026,
                                },
                                time: {
                                    hour: 9,
                                    minute: 0,
                                    second: 0,
                                },
                            },
                            title: "A",
                            type: "event",
                        },
                        {
                            duration: {
                                days: 0,
                                hours: 2,
                                minutes: 0,
                                seconds: 0,
                                weeks: 0,
                            },
                            name: @b,
                            start: {
                                date: {
                                    day: 1,
                                    month: 2,
                                    year: 2026,
                                },
                                time: {
                                    hour: 10,
                                    minute: 0,
                                    second: 0,
                                },
                            },
                            title: "B",
                            type: "event",
                        },
                    ],
                }"#]],
        );
    }

    // r[verify model.calendar.singular+4]
    #[test]
    fn no_calendar_declaration_error() {
        let db = Database::default();
        let diags = check_diagnostics(&db, r#"event @a 2026-01-01T09:00 1h "A""#);
        assert!(diags.iter().any(|d| d.contains("no calendar record")));
    }

    // r[verify model.calendar.singular+4]
    #[test]
    fn multiple_calendars_accepted() {
        let db = Database::default();
        let source = make_source(
            &db,
            "a.gnomon",
            r#"
            calendar { uid: "a" }
            calendar { uid: "b" }
            "#,
        );
        let result = check(&db, source);
        assert_eq!(result.calendars.len(), 2);
    }

    // r[verify model.name.unique]
    #[test]
    fn name_collision() {
        let db = Database::default();
        let diags = check_diagnostics(
            &db,
            r#"
            calendar {}
            event @meeting 2026-01-01T09:00 1h "A"
            event @meeting 2026-02-01T10:00 1h "B"
            "#,
        );
        assert!(
            diags
                .iter()
                .any(|d| d.contains("name @meeting already defined"))
        );
    }

    // r[verify model.name.unique]
    #[test]
    fn name_collision_different_kinds() {
        let db = Database::default();
        let diags = check_diagnostics(
            &db,
            r#"
            calendar {}
            event @x 2026-01-01T09:00 1h "Event X"
            task @x "Task X"
            "#,
        );
        assert!(diags.iter().any(|d| d.contains("name @x already defined")));
    }

    #[test]
    fn empty_file_error() {
        let db = Database::default();
        let source = make_source(&db, "empty.gnomon", "");
        let result = check(&db, source);
        assert!(result.has_errors);
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.message.contains("no calendar record"))
        );
    }

    #[test]
    fn file_with_parse_errors_continues() {
        let db = Database::default();
        let diags = check_diagnostics(&db, r#"~~~ calendar { uid: "test" }"#);
        // Should have parse errors but validation continues.
        assert!(!diags.is_empty());
    }

    #[test]
    fn valid_check_has_no_errors() {
        let db = Database::default();
        let source = make_source(
            &db,
            "a.gnomon",
            r#"calendar { uid: "f47ac10b-58cc-4372-a567-0e02b2c3d479" }"#,
        );
        let result = check(&db, source);
        assert!(!result.has_errors);
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn tasks_and_events_together() {
        let db = Database::default();
        check_output(
            &db,
            r#"
            calendar {}
            task @review "Code review"
            task @deploy 2026-06-01T12:00 "Ship it"
            "#,
            expect![[r#"
                Calendar {
                    properties: {
                        type: "calendar",
                    },
                    entries: [
                        {
                            name: @review,
                            title: "Code review",
                            type: "task",
                        },
                        {
                            due: {
                                date: {
                                    day: 1,
                                    month: 6,
                                    year: 2026,
                                },
                                time: {
                                    hour: 12,
                                    minute: 0,
                                    second: 0,
                                },
                            },
                            name: @deploy,
                            title: "Ship it",
                            type: "task",
                        },
                    ],
                }"#]],
        );
    }

    #[test]
    fn mixed_decl_types() {
        let db = Database::default();
        check_output(
            &db,
            r#"
            calendar { uid: "main" }
            event @standup 2026-03-01T09:00 30m "Standup"
            task @review "Code review"
            "#,
            expect![[r#"
                Calendar {
                    properties: {
                        type: "calendar",
                        uid: "main",
                    },
                    entries: [
                        {
                            duration: {
                                days: 0,
                                hours: 0,
                                minutes: 30,
                                seconds: 0,
                                weeks: 0,
                            },
                            name: @standup,
                            start: {
                                date: {
                                    day: 1,
                                    month: 3,
                                    year: 2026,
                                },
                                time: {
                                    hour: 9,
                                    minute: 0,
                                    second: 0,
                                },
                            },
                            title: "Standup",
                            type: "event",
                        },
                        {
                            name: @review,
                            title: "Code review",
                            type: "task",
                        },
                    ],
                }"#]],
        );
    }

    #[test]
    fn multiple_calendars_preserve_order() {
        let db = Database::default();
        let source = make_source(
            &db,
            "a.gnomon",
            r#"
            calendar { uid: "f47ac10b-58cc-4372-a567-0e02b2c3d479" }
            calendar { uid: "a1b2c3d4-e5f6-7890-abcd-ef1234567890" }
            "#,
        );
        let result = check(&db, source);
        assert!(
            !result.has_errors,
            "unexpected errors: {:?}",
            result.diagnostics
        );
        let uid_key = crate::eval::interned::FieldName::new(&db, "uid".to_string());
        let uid0 = result.calendars[0].properties.get(&uid_key).unwrap();
        let uid1 = result.calendars[1].properties.get(&uid_key).unwrap();
        assert_eq!(
            uid0.value,
            Value::String("f47ac10b-58cc-4372-a567-0e02b2c3d479".into())
        );
        assert_eq!(
            uid1.value,
            Value::String("a1b2c3d4-e5f6-7890-abcd-ef1234567890".into())
        );
    }

    #[test]
    fn multiple_errors_all_reported() {
        let db = Database::default();
        let source = make_source(
            &db,
            "a.gnomon",
            r#"
            calendar {}
            calendar {}
            event @x 2026-01-01T09:00 1h "X"
            event @x 2026-02-01T10:00 1h "X again"
            "#,
        );
        let result = check(&db, source);
        let messages: Vec<&str> = result
            .diagnostics
            .iter()
            .map(|d| d.message.as_str())
            .collect();
        // With multiple calendars, loose events are ambiguous.
        assert!(
            messages.iter().any(|m| m.contains("ambiguous")),
            "missing ambiguous entries error in: {messages:?}"
        );
        assert!(
            messages.iter().any(|m| m.contains("name @x")),
            "missing name collision error in: {messages:?}"
        );
    }

    #[test]
    fn validation_errors_surface_through_check() {
        let db = Database::default();
        let source = make_source(
            &db,
            "a.gnomon",
            // Duplicate field "uid" triggers a validation error.
            r#"calendar { uid: "a", uid: "b" }"#,
        );
        let result = check(&db, source);
        assert!(result.has_errors);
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.message.contains("duplicate field"))
        );
    }

    #[test]
    fn calendar_only_no_events_or_tasks() {
        let db = Database::default();
        check_output(
            &db,
            r#"calendar { uid: "minimal" }"#,
            expect![[r#"
                Calendar {
                    properties: {
                        type: "calendar",
                        uid: "minimal",
                    },
                    entries: [],
                }"#]],
        );
    }

    #[test]
    fn distinct_names_no_collision() {
        let db = Database::default();
        let source = make_source(
            &db,
            "a.gnomon",
            r#"
            calendar { uid: "f47ac10b-58cc-4372-a567-0e02b2c3d479" }
            event @shared 2026-01-01T09:00 1h "Event"
            task @other "Task"
            "#,
        );
        let result = check(&db, source);
        assert!(!result.has_errors);
        assert_eq!(result.calendars[0].entries.len(), 2);
    }

    #[test]
    fn entries_preserve_declaration_order() {
        let db = Database::default();
        check_output(
            &db,
            r#"
            calendar {}
            event @second 2026-06-01T09:00 1h "Second"
            event @first 2026-01-01T09:00 1h "First"
            "#,
            expect![[r#"
                Calendar {
                    properties: {
                        type: "calendar",
                    },
                    entries: [
                        {
                            duration: {
                                days: 0,
                                hours: 1,
                                minutes: 0,
                                seconds: 0,
                                weeks: 0,
                            },
                            name: @second,
                            start: {
                                date: {
                                    day: 1,
                                    month: 6,
                                    year: 2026,
                                },
                                time: {
                                    hour: 9,
                                    minute: 0,
                                    second: 0,
                                },
                            },
                            title: "Second",
                            type: "event",
                        },
                        {
                            duration: {
                                days: 0,
                                hours: 1,
                                minutes: 0,
                                seconds: 0,
                                weeks: 0,
                            },
                            name: @first,
                            start: {
                                date: {
                                    day: 1,
                                    month: 1,
                                    year: 2026,
                                },
                                time: {
                                    hour: 9,
                                    minute: 0,
                                    second: 0,
                                },
                            },
                            title: "First",
                            type: "event",
                        },
                    ],
                }"#]],
        );
    }

    // ── UID derivation tests ─────────────────────────────────

    // r[verify model.calendar.uid+2]
    // r[verify model.calendar.uid.derivation]
    #[test]
    fn uid_derived_for_entry_without_uid() {
        let db = Database::default();
        let source = make_source(
            &db,
            "a.gnomon",
            r#"
            calendar { uid: "f47ac10b-58cc-4372-a567-0e02b2c3d479" }
            event @meeting 2026-03-01T14:30 1h "Standup"
            "#,
        );
        let result = check(&db, source);
        assert!(
            !result.has_errors,
            "unexpected errors: {:?}",
            result.diagnostics
        );
        let uid_key = FieldName::new(&db, "uid".to_string());
        let entry = &result.calendars[0].entries[0].value;
        let uid = entry.get(&uid_key).expect("entry should have derived uid");
        match &uid.value {
            Value::String(s) => {
                assert!(
                    uuid::Uuid::parse_str(s).is_ok(),
                    "derived uid is not a valid UUID: {s}"
                );
            }
            other => panic!("expected string uid, got: {other:?}"),
        }
    }

    // r[verify model.calendar.uid.derivation]
    #[test]
    fn uid_derivation_is_deterministic() {
        let db = Database::default();
        let text = r#"
            calendar { uid: "f47ac10b-58cc-4372-a567-0e02b2c3d479" }
            event @meeting 2026-03-01T14:30 1h "Standup"
        "#;
        let source1 = make_source(&db, "a.gnomon", text);
        let source2 = make_source(&db, "b.gnomon", text);
        let result1 = check(&db, source1);
        let result2 = check(&db, source2);
        let uid_key = FieldName::new(&db, "uid".to_string());
        let uid1 = &result1.calendars[0].entries[0]
            .value
            .get(&uid_key)
            .unwrap()
            .value;
        let uid2 = &result2.calendars[0].entries[0]
            .value
            .get(&uid_key)
            .unwrap()
            .value;
        assert_eq!(uid1, uid2);
    }

    // r[verify record.event.uid+2]
    #[test]
    fn uid_not_overwritten_when_explicit() {
        let db = Database::default();
        let source = make_source(
            &db,
            "a.gnomon",
            r#"
            calendar { uid: "f47ac10b-58cc-4372-a567-0e02b2c3d479" }
            event @meeting 2026-03-01T14:30 1h "Standup" { uid: "custom-uid" }
            "#,
        );
        let result = check(&db, source);
        let uid_key = FieldName::new(&db, "uid".to_string());
        let uid = &result.calendars[0].entries[0]
            .value
            .get(&uid_key)
            .unwrap()
            .value;
        assert_eq!(uid, &Value::String("custom-uid".into()));
    }

    // r[verify model.calendar.uid.derivation.non-uuid]
    #[test]
    fn uid_derivation_skipped_for_non_uuid_calendar_uid() {
        let db = Database::default();
        let source = make_source(
            &db,
            "a.gnomon",
            r#"
            calendar { uid: "not-a-uuid" }
            event @meeting 2026-03-01T14:30 1h "Standup" { uid: "explicit-uid" }
            "#,
        );
        let result = check(&db, source);
        // Non-UUID uid is a warning, not an error.
        assert!(!result.has_errors);
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.message.contains("not a valid UUID") && d.severity == Severity::Warning),
            "expected UUID warning, got: {:?}",
            result
                .diagnostics
                .iter()
                .map(|d| &d.message)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn three_calendars_all_accepted() {
        let db = Database::default();
        let source = make_source(
            &db,
            "a.gnomon",
            r#"
            calendar { uid: "a" }
            calendar { uid: "b" }
            calendar { uid: "c" }
            "#,
        );
        let result = check(&db, source);
        assert_eq!(result.calendars.len(), 3);
    }

    // ── Recurrence expansion tests ──────────────────────────

    // r[verify record.rrule.eval.expansion]
    // r[verify record.rrule.syntax]
    #[test]
    fn entry_with_valid_recur_validates_cleanly() {
        let db = Database::default();
        let source = make_source(
            &db,
            "a.gnomon",
            r#"
            calendar { uid: "f47ac10b-58cc-4372-a567-0e02b2c3d479" }
            event { name: @daily, start: 2026-01-01T00:00, recur: { frequency: #daily, termination: 2026-01-05T00:00 } }
            "#,
        );
        let result = check(&db, source);
        assert!(
            !result.has_errors,
            "unexpected errors: {:?}",
            result.diagnostics
        );
        // Validation should not inject an occurrences field.
        let occ_key = FieldName::new(&db, "occurrences".to_string());
        let entry = &result.calendars[0].entries[0].value;
        assert!(
            entry.get(&occ_key).is_none(),
            "should not have occurrences field"
        );
    }

    #[test]
    fn entry_without_recur_unchanged() {
        let db = Database::default();
        let source = make_source(
            &db,
            "a.gnomon",
            r#"
            calendar { uid: "f47ac10b-58cc-4372-a567-0e02b2c3d479" }
            event @meeting 2026-03-01T14:30 1h "Standup"
            "#,
        );
        let result = check(&db, source);
        let occ_key = FieldName::new(&db, "occurrences".to_string());
        let entry = &result.calendars[0].entries[0].value;
        assert!(entry.get(&occ_key).is_none(), "should not have occurrences");
    }

    // r[verify record.rrule.eval.start-required+2]
    #[test]
    fn entry_with_recur_but_no_start_produces_error() {
        let db = Database::default();
        let source = make_source(
            &db,
            "a.gnomon",
            r#"
            calendar { uid: "f47ac10b-58cc-4372-a567-0e02b2c3d479" }
            event { name: @nostart, recur: { frequency: #daily, termination: 5 } }
            "#,
        );
        let result = check(&db, source);
        assert!(result.has_errors);
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.message.contains("recurrence requires")),
            "expected recurrence-requires-start error, got: {:?}",
            result
                .diagnostics
                .iter()
                .map(|d| &d.message)
                .collect::<Vec<_>>()
        );
    }

    // r[verify record.rrule.eval.infinite]
    #[test]
    fn infinite_rule_validates_cleanly() {
        let db = Database::default();
        let source = make_source(
            &db,
            "a.gnomon",
            r#"
            calendar { uid: "f47ac10b-58cc-4372-a567-0e02b2c3d479" }
            event { name: @inf, start: 2026-01-01T00:00, recur: { frequency: #daily } }
            "#,
        );
        let result = check(&db, source);
        // Infinite rules are valid per r[record.rrule.eval.infinite] — no warnings or errors.
        assert!(
            !result.has_errors,
            "unexpected errors: {:?}",
            result.diagnostics
        );
        assert!(
            !result
                .diagnostics
                .iter()
                .any(|d| d.message.contains("infinite recurrence")),
            "should not warn about infinite recurrence"
        );
    }

    // r[verify record.rrule.n-day]
    #[test]
    fn weekly_recurrence_validates_cleanly() {
        let db = Database::default();
        let source = make_source(
            &db,
            "a.gnomon",
            r#"
            calendar { uid: "f47ac10b-58cc-4372-a567-0e02b2c3d479" }
            event { name: @weekly, start: 2026-01-05T09:00, recur: { frequency: #weekly, by_day: [{ day: #monday }], termination: 2026-02-01T00:00 } }
            "#,
        );
        let result = check(&db, source);
        assert!(
            !result.has_errors,
            "unexpected errors: {:?}",
            result.diagnostics
        );
        // Validation should not inject an occurrences field.
        let occ_key = FieldName::new(&db, "occurrences".to_string());
        let entry = &result.calendars[0].entries[0].value;
        assert!(
            entry.get(&occ_key).is_none(),
            "should not have occurrences field"
        );
    }
}
