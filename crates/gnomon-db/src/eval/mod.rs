pub mod desugar;
pub mod interned;
pub mod literals;
pub mod lower;
pub mod merge;
pub mod render;
pub mod shape;
pub mod types;

use crate::input::SourceFile;
use crate::queries::Diagnostic;
use types::Document;

/// Result of evaluating a source file.
pub struct EvalResult<'db> {
    pub document: Document<'db>,
    /// Lowering diagnostics (parse + validation diagnostics are obtained separately
    /// via `check_syntax::accumulated::<Diagnostic>()`).
    pub diagnostics: Vec<Diagnostic>,
}

/// Evaluate a source file into a reified document.
///
/// This function calls the tracked `check_syntax` query internally to ensure
/// parse and validation errors are accumulated. Lowering-specific diagnostics
/// are returned in `EvalResult::diagnostics`.
pub fn evaluate<'db>(db: &'db dyn crate::Db, source: SourceFile) -> EvalResult<'db> {
    // Run parse + validation (tracked, memoized).
    let _check = crate::check_syntax(db, source);
    let parse_result = crate::parse(db, source);
    let tree = parse_result.tree(db);

    let mut ctx = lower::LowerCtx::new(db, source);
    let document = ctx.lower_document(&tree);

    EvalResult {
        document,
        diagnostics: ctx.diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::interned::FieldName;
    use super::types::*;
    use crate::{Database, SourceFile};
    use std::path::PathBuf;

    fn eval<'db>(db: &'db Database, source: &str) -> super::EvalResult<'db> {
        let sf = SourceFile::new(db, PathBuf::from("test.gnomon"), source.into());
        super::evaluate(db, sf)
    }

    fn get_field<'db>(record: &Record<'db>, db: &'db Database, name: &str) -> Value<'db> {
        let field_name = FieldName::new(db, name.to_string());
        record.get(&field_name).unwrap().value.clone()
    }

    fn has_field<'db>(record: &Record<'db>, db: &'db Database, name: &str) -> bool {
        let field_name = FieldName::new(db, name.to_string());
        record.get(&field_name).is_some()
    }

    // ── Calendar ─────────────────────────────────────────────────

    #[test]
    fn empty_calendar() {
        let db = Database::default();
        let result = eval(&db, "calendar {}");
        assert_eq!(result.document.decls.len(), 1);
        match &result.document.decls[0].value {
            ReifiedDecl::Calendar(r) => assert!(r.0.is_empty()),
            _ => panic!("expected Calendar"),
        }
    }

    #[test]
    fn calendar_with_string_field() {
        let db = Database::default();
        let result = eval(&db, r#"calendar { uid: "test-cal" }"#);
        match &result.document.decls[0].value {
            ReifiedDecl::Calendar(r) => {
                assert_eq!(get_field(&r, &db, "uid"), Value::String("test-cal".into()));
            }
            _ => panic!("expected Calendar"),
        }
    }

    // ── Event (prefix form) ──────────────────────────────────────

    #[test]
    fn event_prefix_form() {
        let db = Database::default();
        let result = eval(
            &db,
            r#"event { name: @standup, start: 2026-03-01T14:00, title: "Standup" }"#,
        );
        assert_eq!(result.document.decls.len(), 1);
        match &result.document.decls[0].value {
            ReifiedDecl::Entry(r) => {
                assert_eq!(get_field(&r, &db, "name"), Value::Name("standup".into()));
                assert_eq!(
                    get_field(&r, &db, "title"),
                    Value::String("Standup".into())
                );
                // start is a desugared datetime record
                match get_field(&r, &db, "start") {
                    Value::Record(dt) => {
                        assert!(has_field(&dt, &db, "date"));
                        assert!(has_field(&dt, &db, "time"));
                    }
                    _ => panic!("expected Record for start"),
                }
            }
            _ => panic!("expected Entry"),
        }
    }

    // ── Event (short form) ───────────────────────────────────────

    #[test]
    fn event_short_form() {
        let db = Database::default();
        let result = eval(
            &db,
            r#"event @meeting 2026-03-01T14:30 1h30m "Standup""#,
        );
        assert_eq!(result.document.decls.len(), 1);
        match &result.document.decls[0].value {
            ReifiedDecl::Entry(r) => {
                assert_eq!(get_field(&r, &db, "name"), Value::Name("meeting".into()));
                assert_eq!(
                    get_field(&r, &db, "title"),
                    Value::String("Standup".into())
                );
                // start is desugared datetime
                assert!(matches!(get_field(&r, &db, "start"), Value::Record(_)));
                // duration is desugared
                assert!(matches!(get_field(&r, &db, "duration"), Value::Record(_)));
            }
            _ => panic!("expected Entry"),
        }
    }

    #[test]
    fn event_short_form_with_body() {
        let db = Database::default();
        let result = eval(
            &db,
            r#"event @meeting 2026-03-01T14:30 1h "Standup" { priority: 5 }"#,
        );
        match &result.document.decls[0].value {
            ReifiedDecl::Entry(r) => {
                assert_eq!(get_field(&r, &db, "name"), Value::Name("meeting".into()));
                assert_eq!(get_field(&r, &db, "priority"), Value::Integer(5));
            }
            _ => panic!("expected Entry"),
        }
    }

    #[test]
    fn event_short_form_date_plus_time() {
        let db = Database::default();
        let result = eval(
            &db,
            r#"event @meeting 2026-03-01 14:30 1h "Standup""#,
        );
        match &result.document.decls[0].value {
            ReifiedDecl::Entry(r) => {
                match get_field(&r, &db, "start") {
                    Value::Record(dt) => {
                        assert!(has_field(&dt, &db, "date"));
                        assert!(has_field(&dt, &db, "time"));
                    }
                    _ => panic!("expected Record for start"),
                }
            }
            _ => panic!("expected Entry"),
        }
    }

    // ── Task ─────────────────────────────────────────────────────

    #[test]
    fn task_prefix_form() {
        let db = Database::default();
        let result = eval(
            &db,
            r#"task { name: @review, title: "Code review" }"#,
        );
        match &result.document.decls[0].value {
            ReifiedDecl::Entry(r) => {
                assert_eq!(get_field(&r, &db, "name"), Value::Name("review".into()));
            }
            _ => panic!("expected Entry"),
        }
    }

    #[test]
    fn task_short_form() {
        let db = Database::default();
        let result = eval(
            &db,
            r#"task @review 2026-03-15T17:00 "Code review""#,
        );
        match &result.document.decls[0].value {
            ReifiedDecl::Entry(r) => {
                assert_eq!(get_field(&r, &db, "name"), Value::Name("review".into()));
                assert_eq!(
                    get_field(&r, &db, "title"),
                    Value::String("Code review".into())
                );
                // due is desugared datetime
                assert!(matches!(get_field(&r, &db, "due"), Value::Record(_)));
            }
            _ => panic!("expected Entry"),
        }
    }

    #[test]
    fn task_short_form_no_datetime() {
        let db = Database::default();
        let result = eval(&db, r#"task @todo "Do something""#);
        match &result.document.decls[0].value {
            ReifiedDecl::Entry(r) => {
                assert_eq!(get_field(&r, &db, "name"), Value::Name("todo".into()));
                assert!(!has_field(&r, &db, "due"));
            }
            _ => panic!("expected Entry"),
        }
    }

    // ── Include ──────────────────────────────────────────────────

    #[test]
    fn include_local_path() {
        let db = Database::default();
        let result = eval(&db, r#"include "holidays.ics""#);
        assert_eq!(result.document.decls.len(), 1);
        match &result.document.decls[0].value {
            ReifiedDecl::Include { target, content } => {
                assert_eq!(*target, IncludeRef::Path("holidays.ics".into()));
                assert!(content.is_empty());
            }
            _ => panic!("expected Include"),
        }
    }

    #[test]
    fn include_uri() {
        let db = Database::default();
        let result = eval(&db, r#"include "https://example.com/cal.ics""#);
        match &result.document.decls[0].value {
            ReifiedDecl::Include { target, .. } => {
                assert_eq!(
                    *target,
                    IncludeRef::Uri("https://example.com/cal.ics".into())
                );
            }
            _ => panic!("expected Include"),
        }
    }

    // ── Bind ─────────────────────────────────────────────────────

    #[test]
    fn binding() {
        let db = Database::default();
        let result = eval(&db, r#"bind @cal.holidays "holidays-uid""#);
        assert!(result.document.decls.is_empty());
        assert_eq!(result.document.bindings.len(), 1);
        let blamed_uid = result.document.bindings.get("cal.holidays").unwrap();
        assert_eq!(blamed_uid.value, "holidays-uid");
    }

    // ── Nested records and lists ─────────────────────────────────

    #[test]
    fn nested_record() {
        let db = Database::default();
        let result = eval(
            &db,
            r#"calendar { location: { name: "Office", coordinates: "geo:37,-122" } }"#,
        );
        match &result.document.decls[0].value {
            ReifiedDecl::Calendar(r) => {
                match get_field(&r, &db, "location") {
                    Value::Record(loc) => {
                        assert_eq!(
                            get_field(&loc, &db, "name"),
                            Value::String("Office".into())
                        );
                    }
                    _ => panic!("expected nested Record"),
                }
            }
            _ => panic!("expected Calendar"),
        }
    }

    #[test]
    fn list_of_strings() {
        let db = Database::default();
        let result = eval(
            &db,
            r#"calendar { keywords: ["work", "meeting"] }"#,
        );
        match &result.document.decls[0].value {
            ReifiedDecl::Calendar(r) => {
                match get_field(&r, &db, "keywords") {
                    Value::List(items) => {
                        assert_eq!(items.len(), 2);
                        assert_eq!(items[0].value, Value::String("work".into()));
                        assert_eq!(items[1].value, Value::String("meeting".into()));
                    }
                    _ => panic!("expected List"),
                }
            }
            _ => panic!("expected Calendar"),
        }
    }

    // ── Literal types ────────────────────────────────────────────

    #[test]
    fn boolean_literals() {
        let db = Database::default();
        let result = eval(
            &db,
            "calendar { show_without_time: true, expect_reply: false }",
        );
        match &result.document.decls[0].value {
            ReifiedDecl::Calendar(r) => {
                assert_eq!(get_field(&r, &db, "show_without_time"), Value::Bool(true));
                assert_eq!(get_field(&r, &db, "expect_reply"), Value::Bool(false));
            }
            _ => panic!("expected Calendar"),
        }
    }

    #[test]
    fn undefined_literal() {
        let db = Database::default();
        let result = eval(&db, "calendar { x: undefined }");
        match &result.document.decls[0].value {
            ReifiedDecl::Calendar(r) => {
                assert_eq!(get_field(&r, &db, "x"), Value::Undefined);
            }
            _ => panic!("expected Calendar"),
        }
    }

    #[test]
    fn integer_and_signed_integer() {
        let db = Database::default();
        let result = eval(
            &db,
            "calendar { priority: 5, offset: -3 }",
        );
        match &result.document.decls[0].value {
            ReifiedDecl::Calendar(r) => {
                assert_eq!(get_field(&r, &db, "priority"), Value::Integer(5));
                assert_eq!(get_field(&r, &db, "offset"), Value::SignedInteger(-3));
            }
            _ => panic!("expected Calendar"),
        }
    }

    #[test]
    fn uri_and_atom_literals() {
        let db = Database::default();
        let result = eval(
            &db,
            r#"calendar { href: <https://example.com>, status: #confirmed }"#,
        );
        match &result.document.decls[0].value {
            ReifiedDecl::Calendar(r) => {
                assert_eq!(
                    get_field(&r, &db, "href"),
                    Value::String("https://example.com".into())
                );
                assert_eq!(
                    get_field(&r, &db, "status"),
                    Value::String("confirmed".into())
                );
            }
            _ => panic!("expected Calendar"),
        }
    }

    // ── Declaration order preserved ──────────────────────────────

    #[test]
    fn declaration_order_preserved() {
        let db = Database::default();
        let result = eval(
            &db,
            r#"
            event @a 2026-01-01T09:00 1h "A"
            calendar { uid: "cal" }
            task @b "B"
            "#,
        );
        assert_eq!(result.document.decls.len(), 3);
        assert!(matches!(
            result.document.decls[0].value,
            ReifiedDecl::Entry(_)
        ));
        assert!(matches!(
            result.document.decls[1].value,
            ReifiedDecl::Calendar(_)
        ));
        assert!(matches!(
            result.document.decls[2].value,
            ReifiedDecl::Entry(_)
        ));
    }

    // ── Blame tracking ───────────────────────────────────────────

    #[test]
    fn blame_tracks_declaration_index() {
        let db = Database::default();
        let result = eval(
            &db,
            r#"
            calendar { uid: "a" }
            calendar { uid: "b" }
            "#,
        );
        let idx0 = result.document.decls[0].blame.decl.index(&db);
        let idx1 = result.document.decls[1].blame.decl.index(&db);
        assert_eq!(idx0, 0);
        assert_eq!(idx1, 1);
    }

    #[test]
    fn blame_field_path_on_record_field() {
        let db = Database::default();
        let result = eval(&db, r#"calendar { uid: "test" }"#);
        match &result.document.decls[0].value {
            ReifiedDecl::Calendar(r) => {
                let uid_name = FieldName::new(&db, "uid".to_string());
                let blamed_value = r.get(&uid_name).unwrap();
                // The field path should include the "uid" field segment.
                assert_eq!(blamed_value.blame.path.0.len(), 1);
            }
            _ => panic!("expected Calendar"),
        }
    }

    // ── Every expression lowering ────────────────────────────────

    #[test]
    fn every_expression_in_event() {
        let db = Database::default();
        let result = eval(
            &db,
            "event { name: @standup, start: 2026-03-01T09:00, rrule: every day }",
        );
        match &result.document.decls[0].value {
            ReifiedDecl::Entry(r) => {
                match get_field(&r, &db, "rrule") {
                    Value::Record(rrule) => {
                        assert_eq!(
                            get_field(&rrule, &db, "frequency"),
                            Value::String("daily".into())
                        );
                    }
                    _ => panic!("expected Record for rrule"),
                }
            }
            _ => panic!("expected Entry"),
        }
    }

    // ── No diagnostics on valid input ────────────────────────────

    #[test]
    fn no_lowering_diagnostics_on_valid_input() {
        let db = Database::default();
        let result = eval(
            &db,
            r#"event @meeting 2026-03-01T14:30 1h "Standup" { priority: 5 }"#,
        );
        assert!(result.diagnostics.is_empty());
    }
}
