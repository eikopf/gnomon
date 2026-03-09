use std::collections::HashMap;

use uuid::Uuid;

use super::interned::FieldName;
use super::types::{Blamed, Calendar, Record, Value};
use crate::input::SourceFile;
use crate::queries::{Diagnostic, Severity};

/// Result of validating a calendar.
pub struct CheckResult<'db> {
    pub calendar: Calendar<'db>,
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
pub fn validate_calendar<'db>(
    db: &'db dyn crate::Db,
    root_source: SourceFile,
    value: Value<'db>,
    eval_diagnostics: Vec<Diagnostic>,
) -> CheckResult<'db> {
    let mut calendar = Calendar::default();
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

    // r[impl model.calendar.singular]
    // Track calendar declarations for uniqueness.
    let mut calendar_count = 0usize;
    let mut first_calendar_source: Option<SourceFile> = None;

    // Track names for collision detection (global namespace across events/tasks).
    let mut seen_names: HashMap<String, SourceFile> = HashMap::new();

    let name_key = FieldName::new(db, "name".to_string());
    let type_key = FieldName::new(db, "type".to_string());

    // r[impl model.calendar.entries]
    // Flatten value into records.
    let records = flatten_to_records(db, root_source, value);

    for (record, blame) in records {
        // Determine if this is an entry (has type: "event"|"task") or calendar.
        let is_entry = record.get(&type_key).is_some_and(|v| {
            matches!(&v.value, Value::String(s) if s == "event" || s == "task")
        });

        let source = blame.decl.source(db);

        if is_entry {
            check_name_collision(
                db,
                &record,
                &name_key,
                source,
                &mut seen_names,
                &mut diagnostics,
                &mut has_errors,
            );
            calendar.entries.push(Blamed {
                value: record,
                blame,
            });
        } else {
            // Calendar
            calendar_count += 1;
            if calendar_count == 1 {
                first_calendar_source = Some(source);
                calendar.properties = record;
            } else {
                has_errors = true;
                diagnostics.push(Diagnostic {
                    source,
                    range: rowan::TextRange::default(),
                    severity: Severity::Error,
                    message: format!(
                        "duplicate calendar declaration (first defined in {})",
                        first_calendar_source
                            .unwrap_or(root_source)
                            .path(db)
                            .display()
                    ),
                });
            }
        }
    }

    // Check calendar declaration uniqueness.
    if calendar_count == 0 {
        has_errors = true;
        diagnostics.push(Diagnostic {
            source: root_source,
            range: rowan::TextRange::default(),
            severity: Severity::Error,
            message: "no calendar declaration found".into(),
        });
    }

    // r[impl model.calendar.uid.derivation]
    // Derive UUIDv5 UIDs for entries that omit an explicit uid.
    derive_uids(db, &mut calendar, root_source, &mut diagnostics, &mut has_errors);

    // Shape-check the merged calendar.
    let shape_diags = super::shape::check_calendar_shape(db, &calendar, root_source);
    for diag in shape_diags {
        has_errors |= diag.severity == Severity::Error;
        diagnostics.push(diag);
    }

    // Expand recurrence rules into materialized occurrences.
    super::rrule::expand_entry_recurrences(db, &mut calendar, &mut diagnostics, &mut has_errors);

    CheckResult {
        calendar,
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
    has_errors: &mut bool,
) {
    let uid_key = FieldName::new(db, "uid".to_string());
    let name_key = FieldName::new(db, "name".to_string());

    // Extract and parse the calendar uid as a UUID namespace.
    let namespace = match calendar.properties.get(&uid_key) {
        Some(blamed) => match &blamed.value {
            Value::String(s) => match Uuid::parse_str(s) {
                Ok(uuid) => uuid,
                Err(_) => {
                    *has_errors = true;
                    diagnostics.push(Diagnostic {
                        source: root_source,
                        range: rowan::TextRange::default(),
                        severity: Severity::Error,
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
            uid_key.clone(),
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
    if let Some(blamed_value) = record.get(name_key) {
        if let Value::Name(name) = &blamed_value.value {
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
            let blame = r
                .0
                .values()
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
    use crate::eval::render::RenderWithDb;
    use crate::Database;
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
        let rendered = format!("{}", result.calendar.render(db));
        expected.assert_eq(&rendered);
    }

    fn check_diagnostics(db: &Database, text: &str) -> Vec<String> {
        let source = make_source(db, "test.gnomon", text);
        let result = check(db, source);
        result.diagnostics.iter().map(|d| d.message.clone()).collect()
    }

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

    #[test]
    fn no_calendar_declaration_error() {
        let db = Database::default();
        let diags = check_diagnostics(
            &db,
            r#"event @a 2026-01-01T09:00 1h "A""#,
        );
        assert!(diags.iter().any(|d| d.contains("no calendar declaration")));
    }

    #[test]
    fn duplicate_calendar_error() {
        let db = Database::default();
        let diags = check_diagnostics(
            &db,
            r#"
            calendar { uid: "a" }
            calendar { uid: "b" }
            "#,
        );
        assert!(diags
            .iter()
            .any(|d| d.contains("duplicate calendar declaration")));
    }

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
        assert!(diags
            .iter()
            .any(|d| d.contains("name @meeting already defined")));
    }

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
        assert!(diags
            .iter()
            .any(|d| d.contains("name @x already defined")));
    }

    #[test]
    fn empty_file_error() {
        let db = Database::default();
        let source = make_source(&db, "empty.gnomon", "");
        let result = check(&db, source);
        assert!(result.has_errors);
        assert!(result
            .diagnostics
            .iter()
            .any(|d| d.message.contains("no calendar declaration")));
    }

    #[test]
    fn file_with_parse_errors_continues() {
        let db = Database::default();
        let diags = check_diagnostics(
            &db,
            r#"~~~ calendar { uid: "test" }"#,
        );
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
                    properties: {},
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
    fn first_calendar_properties_win_on_duplicate() {
        let db = Database::default();
        let source = make_source(
            &db,
            "a.gnomon",
            r#"
            calendar { uid: "first" }
            calendar { uid: "second" }
            "#,
        );
        let result = check(&db, source);
        assert!(result.has_errors);
        let uid_key = crate::eval::interned::FieldName::new(&db, "uid".to_string());
        let uid = result.calendar.properties.get(&uid_key).unwrap();
        assert_eq!(uid.value, Value::String("first".into()));
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
        let messages: Vec<&str> = result.diagnostics.iter().map(|d| d.message.as_str()).collect();
        assert!(
            messages.iter().any(|m| m.contains("duplicate calendar")),
            "missing duplicate calendar error in: {messages:?}"
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
        assert!(result
            .diagnostics
            .iter()
            .any(|d| d.message.contains("duplicate field")));
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
        assert_eq!(result.calendar.entries.len(), 2);
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
                    properties: {},
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
        assert!(!result.has_errors, "unexpected errors: {:?}", result.diagnostics);
        let uid_key = FieldName::new(&db, "uid".to_string());
        let entry = &result.calendar.entries[0].value;
        let uid = entry.get(&uid_key).expect("entry should have derived uid");
        match &uid.value {
            Value::String(s) => {
                assert!(uuid::Uuid::parse_str(s).is_ok(), "derived uid is not a valid UUID: {s}");
            }
            other => panic!("expected string uid, got: {other:?}"),
        }
    }

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
        let uid1 = &result1.calendar.entries[0].value.get(&uid_key).unwrap().value;
        let uid2 = &result2.calendar.entries[0].value.get(&uid_key).unwrap().value;
        assert_eq!(uid1, uid2);
    }

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
        let uid = &result.calendar.entries[0].value.get(&uid_key).unwrap().value;
        assert_eq!(uid, &Value::String("custom-uid".into()));
    }

    #[test]
    fn uid_derivation_skipped_for_non_uuid_calendar_uid() {
        let db = Database::default();
        let source = make_source(
            &db,
            "a.gnomon",
            r#"
            calendar { uid: "not-a-uuid" }
            event @meeting 2026-03-01T14:30 1h "Standup"
            "#,
        );
        let result = check(&db, source);
        assert!(result.has_errors);
        assert!(
            result.diagnostics.iter().any(|d| d.message.contains("not a valid UUID")),
            "expected UUID error, got: {:?}",
            result.diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn three_calendars_produce_two_errors() {
        let db = Database::default();
        let source = make_source(
            &db,
            "a.gnomon",
            r#"
            calendar {}
            calendar {}
            calendar {}
            "#,
        );
        let result = check(&db, source);
        let dup_count = result
            .diagnostics
            .iter()
            .filter(|d| d.message.contains("duplicate calendar"))
            .count();
        assert_eq!(dup_count, 2);
    }

    // ── Recurrence expansion tests ──────────────────────────

    #[test]
    fn entry_with_recur_gets_occurrences() {
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
        assert!(!result.has_errors, "unexpected errors: {:?}", result.diagnostics);
        let occ_key = FieldName::new(&db, "occurrences".to_string());
        let entry = &result.calendar.entries[0].value;
        let occ = entry.get(&occ_key).expect("should have occurrences field");
        match &occ.value {
            Value::List(items) => {
                assert_eq!(items.len(), 5, "expected 5 daily occurrences Jan 1-5");
            }
            other => panic!("expected List for occurrences, got: {other:?}"),
        }
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
        let entry = &result.calendar.entries[0].value;
        assert!(entry.get(&occ_key).is_none(), "should not have occurrences");
    }

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
            result.diagnostics.iter().any(|d| d.message.contains("recurrence requires")),
            "expected recurrence-requires-start error, got: {:?}",
            result.diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn infinite_rule_capped_with_warning() {
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
        assert!(
            result.diagnostics.iter().any(|d| d.severity == Severity::Warning && d.message.contains("infinite recurrence")),
            "expected infinite recurrence warning, got: {:?}",
            result.diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
        let occ_key = FieldName::new(&db, "occurrences".to_string());
        let entry = &result.calendar.entries[0].value;
        match &entry.get(&occ_key).unwrap().value {
            Value::List(items) => {
                assert_eq!(items.len(), 1000, "infinite rule should be capped at 1000");
            }
            other => panic!("expected List, got: {other:?}"),
        }
    }

    #[test]
    fn weekly_recurrence_expanded() {
        let db = Database::default();
        let source = make_source(
            &db,
            "a.gnomon",
            // 2026-01-05 is a Monday. Until 2026-02-01 should give 4 Mondays: Jan 5, 12, 19, 26.
            r#"
            calendar { uid: "f47ac10b-58cc-4372-a567-0e02b2c3d479" }
            event { name: @weekly, start: 2026-01-05T09:00, recur: { frequency: #weekly, by_day: [{ day: #monday }], termination: 2026-02-01T00:00 } }
            "#,
        );
        let result = check(&db, source);
        assert!(!result.has_errors, "unexpected errors: {:?}", result.diagnostics);
        let occ_key = FieldName::new(&db, "occurrences".to_string());
        let entry = &result.calendar.entries[0].value;
        match &entry.get(&occ_key).unwrap().value {
            Value::List(items) => {
                assert_eq!(items.len(), 4, "expected 4 Mondays: Jan 5, 12, 19, 26");
            }
            other => panic!("expected List, got: {other:?}"),
        }
    }
}
