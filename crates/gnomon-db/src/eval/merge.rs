use std::collections::HashMap;

use super::interned::FieldName;
use super::types::{Calendar, ReifiedDecl, Value};
use crate::input::SourceFile;
use crate::queries::{Diagnostic, Severity};

/// Result of merging multiple source files into a calendar.
pub struct MergeResult<'db> {
    pub calendar: Calendar<'db>,
    /// All diagnostics: parse, validation, lowering, and merge-level.
    pub diagnostics: Vec<Diagnostic>,
    pub has_errors: bool,
}

/// Merge multiple source files into a single calendar.
///
/// This function evaluates each source file and combines the results,
/// checking for uniqueness constraints (single calendar declaration,
/// unique names across events/tasks/groups, unique binding keys).
pub fn merge<'db>(db: &'db dyn crate::Db, sources: &[SourceFile]) -> MergeResult<'db> {
    let mut calendar = Calendar::default();
    let mut diagnostics = Vec::new();
    let mut has_errors = false;

    // Track calendar declarations for uniqueness.
    let mut calendar_sources: Vec<SourceFile> = Vec::new();

    // Track names for collision detection (global namespace across events/tasks/groups).
    let mut seen_names: HashMap<String, SourceFile> = HashMap::new();

    let name_key = FieldName::new(db, "name".to_string());

    for &source in sources {
        let result = crate::evaluate(db, source);

        // Collect parse + validation diagnostics from tracked queries.
        let check_diags = crate::check_syntax::accumulated::<Diagnostic>(db, source);
        for diag in check_diags {
            has_errors |= diag.severity == Severity::Error;
            diagnostics.push(diag.clone());
        }

        // Collect lowering diagnostics.
        for diag in result.diagnostics {
            has_errors |= diag.severity == Severity::Error;
            diagnostics.push(diag);
        }

        // Process declarations.
        for blamed_decl in &result.document.decls {
            match &blamed_decl.value {
                ReifiedDecl::Calendar(record) => {
                    calendar_sources.push(source);
                    if calendar_sources.len() == 1 {
                        calendar.properties = record.clone();
                    }
                }
                ReifiedDecl::Event(record) => {
                    check_name_collision(
                        db,
                        record,
                        &name_key,
                        source,
                        &mut seen_names,
                        &mut diagnostics,
                        &mut has_errors,
                    );
                    calendar.events.push(super::types::Blamed {
                        value: record.clone(),
                        blame: blamed_decl.blame.clone(),
                    });
                }
                ReifiedDecl::Task(record) => {
                    check_name_collision(
                        db,
                        record,
                        &name_key,
                        source,
                        &mut seen_names,
                        &mut diagnostics,
                        &mut has_errors,
                    );
                    calendar.tasks.push(super::types::Blamed {
                        value: record.clone(),
                        blame: blamed_decl.blame.clone(),
                    });
                }

                ReifiedDecl::Include { target, .. } => {
                    calendar.includes.push(super::types::Blamed {
                        value: target.clone(),
                        blame: blamed_decl.blame.clone(),
                    });
                }
            }
        }

        // Merge bindings, checking for collisions.
        for (name, blamed_uid) in &result.document.bindings {
            if let Some(existing) = calendar.bindings.get(name) {
                let first_source = existing.blame.decl.source(db);
                has_errors = true;
                diagnostics.push(Diagnostic {
                    source,
                    range: rowan::TextRange::default(),
                    severity: Severity::Error,
                    message: format!(
                        "binding @{} already defined in {}",
                        name,
                        first_source.path(db).display()
                    ),
                });
            } else {
                calendar.bindings.insert(name.clone(), blamed_uid.clone());
            }
        }
    }

    // Check calendar declaration uniqueness.
    if calendar_sources.is_empty() {
        has_errors = true;
        let source = sources
            .first()
            .copied()
            .unwrap_or_else(|| SourceFile::new(db, "".into(), String::new()));
        diagnostics.push(Diagnostic {
            source,
            range: rowan::TextRange::default(),
            severity: Severity::Error,
            message: "no calendar declaration found".into(),
        });
    } else if calendar_sources.len() > 1 {
        has_errors = true;
        for &extra_source in &calendar_sources[1..] {
            diagnostics.push(Diagnostic {
                source: extra_source,
                range: rowan::TextRange::default(),
                severity: Severity::Error,
                message: format!(
                    "duplicate calendar declaration (first defined in {})",
                    calendar_sources[0].path(db).display()
                ),
            });
        }
    }

    MergeResult {
        calendar,
        diagnostics,
        has_errors,
    }
}

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

    fn check_merge(db: &Database, files: &[(&str, &str)], expected: Expect) {
        let sources: Vec<SourceFile> = files
            .iter()
            .map(|(path, text)| make_source(db, path, text))
            .collect();
        let result = merge(db, &sources);
        let rendered = format!("{}", result.calendar.render(db));
        expected.assert_eq(&rendered);
    }

    fn merge_diagnostics(db: &Database, files: &[(&str, &str)]) -> Vec<String> {
        let sources: Vec<SourceFile> = files
            .iter()
            .map(|(path, text)| make_source(db, path, text))
            .collect();
        let result = merge(db, &sources);
        result.diagnostics.iter().map(|d| d.message.clone()).collect()
    }

    #[test]
    fn single_file_with_calendar_and_event() {
        let db = Database::default();
        check_merge(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "test" }
                event @meeting 2026-03-01T14:30 1h "Standup"
                "#,
            )],
            expect![[r#"
                Calendar {
                    properties: {
                        uid: "test",
                    },
                    events: [
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
                        },
                    ],
                    tasks: [],
                    includes: [],
                    bindings: {},
                }"#]],
        );
    }

    #[test]
    fn two_files_events_merged() {
        let db = Database::default();
        check_merge(
            &db,
            &[
                (
                    "a.gnomon",
                    r#"
                    calendar { uid: "cal" }
                    event @a 2026-01-01T09:00 1h "A"
                    "#,
                ),
                (
                    "b.gnomon",
                    r#"
                    event @b 2026-02-01T10:00 2h "B"
                    "#,
                ),
            ],
            expect![[r#"
                Calendar {
                    properties: {
                        uid: "cal",
                    },
                    events: [
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
                        },
                    ],
                    tasks: [],
                    includes: [],
                    bindings: {},
                }"#]],
        );
    }

    #[test]
    fn no_calendar_declaration_error() {
        let db = Database::default();
        let diags = merge_diagnostics(
            &db,
            &[("a.gnomon", r#"event @a 2026-01-01T09:00 1h "A""#)],
        );
        assert!(diags.iter().any(|d| d.contains("no calendar declaration")));
    }

    #[test]
    fn duplicate_calendar_error() {
        let db = Database::default();
        let diags = merge_diagnostics(
            &db,
            &[
                ("a.gnomon", r#"calendar { uid: "a" }"#),
                ("b.gnomon", r#"calendar { uid: "b" }"#),
            ],
        );
        assert!(diags
            .iter()
            .any(|d| d.contains("duplicate calendar declaration")));
    }

    #[test]
    fn name_collision_across_files() {
        let db = Database::default();
        let diags = merge_diagnostics(
            &db,
            &[
                (
                    "a.gnomon",
                    r#"
                    calendar {}
                    event @meeting 2026-01-01T09:00 1h "A"
                    "#,
                ),
                ("b.gnomon", r#"event @meeting 2026-02-01T10:00 1h "B""#),
            ],
        );
        assert!(diags
            .iter()
            .any(|d| d.contains("name @meeting already defined")));
    }

    #[test]
    fn name_collision_different_kinds() {
        let db = Database::default();
        let diags = merge_diagnostics(
            &db,
            &[
                (
                    "a.gnomon",
                    r#"
                    calendar {}
                    event @x 2026-01-01T09:00 1h "Event X"
                    "#,
                ),
                ("b.gnomon", r#"task @x "Task X""#),
            ],
        );
        assert!(diags
            .iter()
            .any(|d| d.contains("name @x already defined")));
    }

    #[test]
    fn binding_collision() {
        let db = Database::default();
        let diags = merge_diagnostics(
            &db,
            &[
                (
                    "a.gnomon",
                    r#"
                    calendar {}
                    bind @cal.holidays "uid-a"
                    "#,
                ),
                ("b.gnomon", r#"bind @cal.holidays "uid-b""#),
            ],
        );
        assert!(diags
            .iter()
            .any(|d| d.contains("binding @cal.holidays already defined")));
    }

    #[test]
    fn empty_sources_error() {
        let db = Database::default();
        let sources: Vec<SourceFile> = vec![];
        let result = merge(&db, &sources);
        assert!(result.has_errors);
        assert!(result
            .diagnostics
            .iter()
            .any(|d| d.message.contains("no calendar declaration")));
    }

    #[test]
    fn file_with_parse_errors_continues() {
        let db = Database::default();
        let diags = merge_diagnostics(
            &db,
            &[
                ("a.gnomon", r#"calendar { uid: "test" }"#),
                ("b.gnomon", r#"~~~ event @x 2026-01-01T09:00 1h "X""#),
            ],
        );
        // Should have parse errors but merge continues.
        assert!(!diags.is_empty());
    }

    #[test]
    fn includes_carried_through() {
        let db = Database::default();
        check_merge(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar {}
                include "holidays.ics"
                "#,
            )],
            expect![[r#"
                Calendar {
                    properties: {},
                    events: [],
                    tasks: [],
                    includes: [
                        "holidays.ics",
                    ],
                    bindings: {},
                }"#]],
        );
    }

    // ── Additional merge tests ──────────────────────────────────

    #[test]
    fn valid_merge_has_no_errors() {
        let db = Database::default();
        let sources = vec![make_source(
            &db,
            "a.gnomon",
            r#"calendar { uid: "ok" }"#,
        )];
        let result = merge(&db, &sources);
        assert!(!result.has_errors);
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn tasks_merged_across_files() {
        let db = Database::default();
        check_merge(
            &db,
            &[
                (
                    "a.gnomon",
                    r#"
                    calendar {}
                    task @review "Code review"
                    "#,
                ),
                ("b.gnomon", r#"task @deploy 2026-06-01T12:00 "Ship it""#),
            ],
            expect![[r#"
                Calendar {
                    properties: {},
                    events: [],
                    tasks: [
                        {
                            name: @review,
                            title: "Code review",
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
                        },
                    ],
                    includes: [],
                    bindings: {},
                }"#]],
        );
    }

    #[test]
    fn bindings_merged_across_files() {
        let db = Database::default();
        check_merge(
            &db,
            &[
                (
                    "a.gnomon",
                    r#"
                    calendar {}
                    bind @cal.work "work-uid"
                    "#,
                ),
                ("b.gnomon", r#"bind @cal.personal "personal-uid""#),
            ],
            expect![[r#"
                Calendar {
                    properties: {},
                    events: [],
                    tasks: [],
                    includes: [],
                    bindings: {
                        cal.personal: "personal-uid",
                        cal.work: "work-uid",
                    },
                }"#]],
        );
    }

    #[test]
    fn includes_merged_across_files() {
        let db = Database::default();
        check_merge(
            &db,
            &[
                (
                    "a.gnomon",
                    r#"
                    calendar {}
                    include "holidays.ics"
                    "#,
                ),
                ("b.gnomon", r#"include "https://example.com/feed.ics""#),
            ],
            expect![[r#"
                Calendar {
                    properties: {},
                    events: [],
                    tasks: [],
                    includes: [
                        "holidays.ics",
                        "https://example.com/feed.ics",
                    ],
                    bindings: {},
                }"#]],
        );
    }

    #[test]
    fn mixed_decl_types_across_three_files() {
        let db = Database::default();
        check_merge(
            &db,
            &[
                ("a.gnomon", r#"calendar { uid: "main" }"#),
                (
                    "b.gnomon",
                    r#"
                    event @standup 2026-03-01T09:00 30m "Standup"
                    bind @cal.work "work-uid"
                    "#,
                ),
                (
                    "c.gnomon",
                    r#"
                    task @review "Code review"
                    include "holidays.ics"
                    "#,
                ),
            ],
            expect![[r#"
                Calendar {
                    properties: {
                        uid: "main",
                    },
                    events: [
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
                        },
                    ],
                    tasks: [
                        {
                            name: @review,
                            title: "Code review",
                        },
                    ],
                    includes: [
                        "holidays.ics",
                    ],
                    bindings: {
                        cal.work: "work-uid",
                    },
                }"#]],
        );
    }

    #[test]
    fn name_collision_within_same_file() {
        let db = Database::default();
        let diags = merge_diagnostics(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar {}
                event @dup 2026-01-01T09:00 1h "First"
                event @dup 2026-02-01T10:00 1h "Second"
                "#,
            )],
        );
        assert!(diags
            .iter()
            .any(|d| d.contains("name @dup already defined")));
    }

    #[test]
    fn duplicate_calendar_within_same_file() {
        let db = Database::default();
        let diags = merge_diagnostics(
            &db,
            &[(
                "a.gnomon",
                r#"
                calendar { uid: "first" }
                calendar { uid: "second" }
                "#,
            )],
        );
        assert!(diags
            .iter()
            .any(|d| d.contains("duplicate calendar declaration")));
    }

    #[test]
    fn first_calendar_properties_win_on_duplicate() {
        let db = Database::default();
        let sources = vec![
            make_source(&db, "a.gnomon", r#"calendar { uid: "first" }"#),
            make_source(&db, "b.gnomon", r#"calendar { uid: "second" }"#),
        ];
        let result = merge(&db, &sources);
        // Should have an error, but properties come from the first calendar.
        assert!(result.has_errors);
        let uid_key = crate::eval::interned::FieldName::new(&db, "uid".to_string());
        let uid = result.calendar.properties.get(&uid_key).unwrap();
        assert_eq!(uid.value, Value::String("first".into()));
    }

    #[test]
    fn diagnostic_source_attribution_on_name_collision() {
        let db = Database::default();
        let sources = vec![
            make_source(&db, "first.gnomon", r#"calendar {}"#),
            make_source(
                &db,
                "events-a.gnomon",
                r#"event @dup 2026-01-01T09:00 1h "A""#,
            ),
            make_source(
                &db,
                "events-b.gnomon",
                r#"event @dup 2026-02-01T10:00 1h "B""#,
            ),
        ];
        let result = merge(&db, &sources);
        let collision_diag = result
            .diagnostics
            .iter()
            .find(|d| d.message.contains("name @dup"))
            .expect("should have a name collision diagnostic");
        // Error attributed to the second occurrence.
        assert_eq!(
            collision_diag.source.path(&db).to_str().unwrap(),
            "events-b.gnomon"
        );
        // Message names the first file.
        assert!(collision_diag.message.contains("events-a.gnomon"));
    }

    #[test]
    fn diagnostic_source_attribution_on_duplicate_calendar() {
        let db = Database::default();
        let sources = vec![
            make_source(&db, "main.gnomon", r#"calendar { uid: "a" }"#),
            make_source(&db, "extra.gnomon", r#"calendar { uid: "b" }"#),
        ];
        let result = merge(&db, &sources);
        let dup_diag = result
            .diagnostics
            .iter()
            .find(|d| d.message.contains("duplicate calendar"))
            .expect("should have a duplicate calendar diagnostic");
        // Error attributed to the second file.
        assert_eq!(
            dup_diag.source.path(&db).to_str().unwrap(),
            "extra.gnomon"
        );
        // Message names the first file.
        assert!(dup_diag.message.contains("main.gnomon"));
    }

    #[test]
    fn diagnostic_source_attribution_on_binding_collision() {
        let db = Database::default();
        let sources = vec![
            make_source(
                &db,
                "a.gnomon",
                r#"
                calendar {}
                bind @x "uid-a"
                "#,
            ),
            make_source(&db, "b.gnomon", r#"bind @x "uid-b""#),
        ];
        let result = merge(&db, &sources);
        let bind_diag = result
            .diagnostics
            .iter()
            .find(|d| d.message.contains("binding @x"))
            .expect("should have a binding collision diagnostic");
        assert_eq!(bind_diag.source.path(&db).to_str().unwrap(), "b.gnomon");
        assert!(bind_diag.message.contains("a.gnomon"));
    }

    #[test]
    fn multiple_errors_all_reported() {
        let db = Database::default();
        let sources = vec![
            make_source(
                &db,
                "a.gnomon",
                r#"
                calendar {}
                event @x 2026-01-01T09:00 1h "X"
                bind @b "uid-1"
                "#,
            ),
            make_source(
                &db,
                "b.gnomon",
                r#"
                calendar {}
                event @x 2026-02-01T10:00 1h "X again"
                bind @b "uid-2"
                "#,
            ),
        ];
        let result = merge(&db, &sources);
        let messages: Vec<&str> = result.diagnostics.iter().map(|d| d.message.as_str()).collect();
        assert!(
            messages.iter().any(|m| m.contains("duplicate calendar")),
            "missing duplicate calendar error in: {messages:?}"
        );
        assert!(
            messages.iter().any(|m| m.contains("name @x")),
            "missing name collision error in: {messages:?}"
        );
        assert!(
            messages.iter().any(|m| m.contains("binding @b")),
            "missing binding collision error in: {messages:?}"
        );
    }

    #[test]
    fn parse_errors_surface_but_valid_files_still_merge() {
        let db = Database::default();
        let sources = vec![
            make_source(&db, "good.gnomon", r#"calendar { uid: "ok" }"#),
            make_source(&db, "bad.gnomon", r#"~~~ not valid syntax"#),
            make_source(
                &db,
                "also-good.gnomon",
                r#"event @meeting 2026-03-01T09:00 1h "Hi""#,
            ),
        ];
        let result = merge(&db, &sources);
        assert!(result.has_errors);
        // Parse errors from bad.gnomon are present.
        assert!(result
            .diagnostics
            .iter()
            .any(|d| d.source.path(&db).to_str().unwrap() == "bad.gnomon"));
        // But the valid content still merged.
        assert_eq!(result.calendar.events.len(), 1);
        let uid_key = crate::eval::interned::FieldName::new(&db, "uid".to_string());
        assert_eq!(
            result.calendar.properties.get(&uid_key).unwrap().value,
            Value::String("ok".into())
        );
    }

    #[test]
    fn validation_errors_surface_through_merge() {
        let db = Database::default();
        let sources = vec![make_source(
            &db,
            "a.gnomon",
            // Duplicate field "uid" triggers a validation error.
            r#"calendar { uid: "a", uid: "b" }"#,
        )];
        let result = merge(&db, &sources);
        assert!(result.has_errors);
        assert!(result
            .diagnostics
            .iter()
            .any(|d| d.message.contains("duplicate field")));
    }

    #[test]
    fn calendar_only_no_events_or_tasks() {
        let db = Database::default();
        check_merge(
            &db,
            &[("a.gnomon", r#"calendar { uid: "minimal" }"#)],
            expect![[r#"
                Calendar {
                    properties: {
                        uid: "minimal",
                    },
                    events: [],
                    tasks: [],
                    includes: [],
                    bindings: {},
                }"#]],
        );
    }

    #[test]
    fn distinct_names_across_kinds_no_collision() {
        // event @a and task @b should not collide — only same names collide.
        let db = Database::default();
        let sources = vec![
            make_source(
                &db,
                "a.gnomon",
                r#"
                calendar {}
                event @shared 2026-01-01T09:00 1h "Event"
                "#,
            ),
            make_source(&db, "b.gnomon", r#"task @other "Task""#),
        ];
        let result = merge(&db, &sources);
        assert!(!result.has_errors);
        assert_eq!(result.calendar.events.len(), 1);
        assert_eq!(result.calendar.tasks.len(), 1);
    }

    #[test]
    fn events_preserve_source_order() {
        let db = Database::default();
        check_merge(
            &db,
            &[
                (
                    "a.gnomon",
                    r#"
                    calendar {}
                    event @second 2026-06-01T09:00 1h "Second"
                    "#,
                ),
                (
                    "b.gnomon",
                    r#"event @first 2026-01-01T09:00 1h "First""#,
                ),
            ],
            expect![[r#"
                Calendar {
                    properties: {},
                    events: [
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
                        },
                    ],
                    tasks: [],
                    includes: [],
                    bindings: {},
                }"#]],
        );
    }

    #[test]
    fn three_calendars_produce_two_errors() {
        let db = Database::default();
        let sources = vec![
            make_source(&db, "a.gnomon", "calendar {}"),
            make_source(&db, "b.gnomon", "calendar {}"),
            make_source(&db, "c.gnomon", "calendar {}"),
        ];
        let result = merge(&db, &sources);
        let dup_count = result
            .diagnostics
            .iter()
            .filter(|d| d.message.contains("duplicate calendar"))
            .count();
        assert_eq!(dup_count, 2);
    }
}
