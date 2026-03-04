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
                ReifiedDecl::Group(record) => {
                    check_name_collision(
                        db,
                        record,
                        &name_key,
                        source,
                        &mut seen_names,
                        &mut diagnostics,
                        &mut has_errors,
                    );
                    calendar.groups.push(super::types::Blamed {
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
                    groups: [],
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
                    groups: [],
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
                    groups: [],
                    includes: [
                        "holidays.ics",
                    ],
                    bindings: {},
                }"#]],
        );
    }
}
