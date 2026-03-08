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
use types::Value;

/// Result of evaluating a source file.
pub struct EvalResult<'db> {
    pub value: Value<'db>,
    /// Lowering diagnostics (parse + validation diagnostics are obtained separately
    /// via `check_syntax::accumulated::<Diagnostic>()`).
    pub diagnostics: Vec<Diagnostic>,
}

/// Evaluate a source file into a value.
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
    let value = ctx.lower_source_file(&tree);

    EvalResult {
        value,
        diagnostics: ctx.diagnostics,
    }
}

/// Internal: evaluate with an existing import stack for cycle detection.
pub(super) fn evaluate_with_import_stack<'db>(
    db: &'db dyn crate::Db,
    source: SourceFile,
    import_stack: Vec<std::path::PathBuf>,
) -> EvalResult<'db> {
    let _check = crate::check_syntax(db, source);
    let parse_result = crate::parse(db, source);
    let tree = parse_result.tree(db);

    let mut ctx = lower::LowerCtx::with_import_stack(db, source, import_stack);
    let value = ctx.lower_source_file(&tree);

    EvalResult {
        value,
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

    /// Extract the record from a single-decl file that produces a Value::Record.
    fn expect_record<'a, 'db>(result: &'a super::EvalResult<'db>) -> &'a Record<'db> {
        match &result.value {
            Value::Record(r) => r,
            other => panic!("expected Record, got: {other:?}"),
        }
    }

    /// Extract the record from a list item in a multi-decl file.
    fn expect_list_record<'a, 'db>(result: &'a super::EvalResult<'db>, index: usize) -> &'a Record<'db> {
        match &result.value {
            Value::List(items) => match &items[index].value {
                Value::Record(r) => r,
                other => panic!("expected Record at index {index}, got: {other:?}"),
            },
            other => panic!("expected List, got: {other:?}"),
        }
    }

    fn expect_list_len(result: &super::EvalResult<'_>) -> usize {
        match &result.value {
            Value::List(items) => items.len(),
            other => panic!("expected List, got: {other:?}"),
        }
    }

    // ── Calendar ─────────────────────────────────────────────────

    #[test]
    fn empty_calendar() {
        let db = Database::default();
        let result = eval(&db, "calendar {}");
        let r = expect_record(&result);
        assert!(r.0.is_empty());
    }

    #[test]
    fn calendar_with_string_field() {
        let db = Database::default();
        let result = eval(&db, r#"calendar { uid: "test-cal" }"#);
        let r = expect_record(&result);
        assert_eq!(get_field(r, &db, "uid"), Value::String("test-cal".into()));
    }

    // ── Event (prefix form) ──────────────────────────────────────

    #[test]
    fn event_prefix_form() {
        let db = Database::default();
        let result = eval(
            &db,
            r#"event { name: @standup, start: 2026-03-01T14:00, title: "Standup" }"#,
        );
        let r = expect_record(&result);
        assert_eq!(get_field(r, &db, "name"), Value::Name("standup".into()));
        assert_eq!(
            get_field(r, &db, "title"),
            Value::String("Standup".into())
        );
        // start is a desugared datetime record
        match get_field(r, &db, "start") {
            Value::Record(dt) => {
                assert!(has_field(&dt, &db, "date"));
                assert!(has_field(&dt, &db, "time"));
            }
            _ => panic!("expected Record for start"),
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
        let r = expect_record(&result);
        assert_eq!(get_field(r, &db, "name"), Value::Name("meeting".into()));
        assert_eq!(
            get_field(r, &db, "title"),
            Value::String("Standup".into())
        );
        assert!(matches!(get_field(r, &db, "start"), Value::Record(_)));
        assert!(matches!(get_field(r, &db, "duration"), Value::Record(_)));
    }

    #[test]
    fn event_short_form_with_body() {
        let db = Database::default();
        let result = eval(
            &db,
            r#"event @meeting 2026-03-01T14:30 1h "Standup" { priority: 5 }"#,
        );
        let r = expect_record(&result);
        assert_eq!(get_field(r, &db, "name"), Value::Name("meeting".into()));
        assert_eq!(get_field(r, &db, "priority"), Value::Integer(5));
    }

    #[test]
    fn event_short_form_date_plus_time() {
        let db = Database::default();
        let result = eval(
            &db,
            r#"event @meeting 2026-03-01 14:30 1h "Standup""#,
        );
        let r = expect_record(&result);
        match get_field(r, &db, "start") {
            Value::Record(dt) => {
                assert!(has_field(&dt, &db, "date"));
                assert!(has_field(&dt, &db, "time"));
            }
            _ => panic!("expected Record for start"),
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
        let r = expect_record(&result);
        assert_eq!(get_field(r, &db, "name"), Value::Name("review".into()));
    }

    #[test]
    fn task_short_form() {
        let db = Database::default();
        let result = eval(
            &db,
            r#"task @review 2026-03-15T17:00 "Code review""#,
        );
        let r = expect_record(&result);
        assert_eq!(get_field(r, &db, "name"), Value::Name("review".into()));
        assert_eq!(
            get_field(r, &db, "title"),
            Value::String("Code review".into())
        );
        assert!(matches!(get_field(r, &db, "due"), Value::Record(_)));
    }

    #[test]
    fn task_short_form_no_datetime() {
        let db = Database::default();
        let result = eval(&db, r#"task @todo "Do something""#);
        let r = expect_record(&result);
        assert_eq!(get_field(r, &db, "name"), Value::Name("todo".into()));
        assert!(!has_field(r, &db, "due"));
    }

    // ── Nested records and lists ─────────────────────────────────

    #[test]
    fn nested_record() {
        let db = Database::default();
        let result = eval(
            &db,
            r#"calendar { location: { name: "Office", coordinates: "geo:37,-122" } }"#,
        );
        let r = expect_record(&result);
        match get_field(r, &db, "location") {
            Value::Record(loc) => {
                assert_eq!(
                    get_field(&loc, &db, "name"),
                    Value::String("Office".into())
                );
            }
            _ => panic!("expected nested Record"),
        }
    }

    #[test]
    fn list_of_strings() {
        let db = Database::default();
        let result = eval(
            &db,
            r#"calendar { keywords: ["work", "meeting"] }"#,
        );
        let r = expect_record(&result);
        match get_field(r, &db, "keywords") {
            Value::List(items) => {
                assert_eq!(items.len(), 2);
                assert_eq!(items[0].value, Value::String("work".into()));
                assert_eq!(items[1].value, Value::String("meeting".into()));
            }
            _ => panic!("expected List"),
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
        let r = expect_record(&result);
        assert_eq!(get_field(r, &db, "show_without_time"), Value::Bool(true));
        assert_eq!(get_field(r, &db, "expect_reply"), Value::Bool(false));
    }

    #[test]
    fn undefined_literal() {
        let db = Database::default();
        let result = eval(&db, "calendar { x: undefined }");
        let r = expect_record(&result);
        assert_eq!(get_field(r, &db, "x"), Value::Undefined);
    }

    #[test]
    fn integer_and_signed_integer() {
        let db = Database::default();
        let result = eval(
            &db,
            "calendar { priority: 5, offset: -3 }",
        );
        let r = expect_record(&result);
        assert_eq!(get_field(r, &db, "priority"), Value::Integer(5));
        assert_eq!(get_field(r, &db, "offset"), Value::SignedInteger(-3));
    }

    #[test]
    fn uri_and_atom_literals() {
        let db = Database::default();
        let result = eval(
            &db,
            r#"calendar { href: <https://example.com>, status: #confirmed }"#,
        );
        let r = expect_record(&result);
        assert_eq!(
            get_field(r, &db, "href"),
            Value::String("https://example.com".into())
        );
        assert_eq!(
            get_field(r, &db, "status"),
            Value::String("confirmed".into())
        );
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
        assert_eq!(expect_list_len(&result), 3);
        assert!(matches!(expect_list_record(&result, 0), r if has_field(r, &db, "type")));
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
        match &result.value {
            Value::List(items) => {
                let idx0 = items[0].blame.decl.index(&db);
                let idx1 = items[1].blame.decl.index(&db);
                assert_eq!(idx0, 0);
                assert_eq!(idx1, 1);
            }
            _ => panic!("expected list for multiple decls"),
        }
    }

    #[test]
    fn blame_field_path_on_record_field() {
        let db = Database::default();
        let result = eval(&db, r#"calendar { uid: "test" }"#);
        let r = expect_record(&result);
        let uid_name = FieldName::new(&db, "uid".to_string());
        let blamed_value = r.get(&uid_name).unwrap();
        assert_eq!(blamed_value.blame.path.0.len(), 1);
    }

    // ── Every expression lowering ────────────────────────────────

    #[test]
    fn every_expression_in_event() {
        let db = Database::default();
        let result = eval(
            &db,
            "event { name: @standup, start: 2026-03-01T09:00, rrule: every day }",
        );
        let r = expect_record(&result);
        match get_field(r, &db, "rrule") {
            Value::Record(rrule) => {
                assert_eq!(
                    get_field(&rrule, &db, "frequency"),
                    Value::String("daily".into())
                );
            }
            _ => panic!("expected Record for rrule"),
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

    // ── New expression forms ─────────────────────────────────────

    #[test]
    fn let_expression() {
        let db = Database::default();
        let result = eval(&db, r#"let x = 42 in { count: x }"#);
        let r = expect_record(&result);
        assert_eq!(get_field(r, &db, "count"), Value::Integer(42));
    }

    #[test]
    fn identifier_expression() {
        let db = Database::default();
        let result = eval(&db, r#"let name = "hello" in name"#);
        assert_eq!(result.value, Value::String("hello".into()));
    }

    #[test]
    fn paren_expression() {
        let db = Database::default();
        let result = eval(&db, r#"(42)"#);
        assert_eq!(result.value, Value::Integer(42));
    }

    #[test]
    fn binary_concat_lists() {
        let db = Database::default();
        let result = eval(&db, r#"[1] ++ [2, 3]"#);
        match &result.value {
            Value::List(items) => {
                assert_eq!(items.len(), 3);
                assert_eq!(items[0].value, Value::Integer(1));
                assert_eq!(items[1].value, Value::Integer(2));
                assert_eq!(items[2].value, Value::Integer(3));
            }
            other => panic!("expected List, got: {other:?}"),
        }
    }

    #[test]
    fn binary_merge_records() {
        let db = Database::default();
        let result = eval(&db, r#"{ a: 1 } // { b: 2 }"#);
        let r = expect_record(&result);
        assert_eq!(get_field(r, &db, "a"), Value::Integer(1));
        assert_eq!(get_field(r, &db, "b"), Value::Integer(2));
    }

    #[test]
    fn binary_equality() {
        let db = Database::default();
        let result = eval(&db, "1 == 1");
        assert_eq!(result.value, Value::Bool(true));

        let result = eval(&db, "1 != 1");
        assert_eq!(result.value, Value::Bool(false));

        let result = eval(&db, "1 == 2");
        assert_eq!(result.value, Value::Bool(false));
    }

    #[test]
    fn field_access() {
        let db = Database::default();
        let result = eval(&db, r#"{ x: 42 }.x"#);
        assert_eq!(result.value, Value::Integer(42));
    }

    #[test]
    fn index_access() {
        let db = Database::default();
        let result = eval(&db, "[10, 20, 30][1]");
        assert_eq!(result.value, Value::Integer(20));
    }

    #[test]
    fn file_level_let_binding() {
        let db = Database::default();
        let result = eval(
            &db,
            r#"
            let base = { priority: 1 }
            event { name: @e, start: 2026-01-01T00:00 }
            "#,
        );
        // base is not used in decls, so it doesn't affect the result
        let r = expect_record(&result);
        assert_eq!(get_field(r, &db, "name"), Value::Name("e".into()));
    }

    #[test]
    fn file_level_let_used_in_expr_body() {
        let db = Database::default();
        let result = eval(
            &db,
            r#"
            let x = 42
            { count: x }
            "#,
        );
        let r = expect_record(&result);
        assert_eq!(get_field(r, &db, "count"), Value::Integer(42));
    }

    #[test]
    fn binary_left_associative() {
        let db = Database::default();
        // [1] ++ [2] ++ [3] should be ([1] ++ [2]) ++ [3] = [1, 2, 3]
        let result = eval(&db, "[1] ++ [2] ++ [3]");
        match &result.value {
            Value::List(items) => assert_eq!(items.len(), 3),
            other => panic!("expected List, got: {other:?}"),
        }
    }

    // ── Import expressions ──────────────────────────────────────

    mod import_tests {
        use super::*;
        use std::io::Write;

        fn eval_file<'db>(db: &'db Database, path: &std::path::Path) -> super::super::EvalResult<'db> {
            let text = std::fs::read_to_string(path).unwrap();
            let sf = SourceFile::new(db, path.to_path_buf(), text);
            super::super::evaluate(db, sf)
        }

        #[test]
        fn import_gnomon_file() {
            let dir = tempfile::tempdir().unwrap();

            // Create the imported file.
            let other_path = dir.path().join("other.gnomon");
            let mut f = std::fs::File::create(&other_path).unwrap();
            write!(f, r#"{{ x: 42 }}"#).unwrap();

            // Create the importing file.
            let main_path = dir.path().join("main.gnomon");
            let mut f = std::fs::File::create(&main_path).unwrap();
            write!(f, "import ./other.gnomon").unwrap();

            let db = Database::default();
            let result = eval_file(&db, &main_path);
            assert!(result.diagnostics.is_empty(), "diagnostics: {:?}", result.diagnostics);
            let r = expect_record(&result);
            assert_eq!(get_field(r, &db, "x"), Value::Integer(42));
        }

        #[test]
        fn import_circular_detected() {
            let dir = tempfile::tempdir().unwrap();

            // a.gnomon imports b.gnomon, b.gnomon imports a.gnomon
            let a_path = dir.path().join("a.gnomon");
            let b_path = dir.path().join("b.gnomon");
            std::fs::write(&a_path, "import ./b.gnomon").unwrap();
            std::fs::write(&b_path, "import ./a.gnomon").unwrap();

            let db = Database::default();
            let result = eval_file(&db, &a_path);
            assert!(
                result.diagnostics.iter().any(|d| d.message.contains("circular import")),
                "expected circular import error, got: {:?}",
                result.diagnostics,
            );
        }

        #[test]
        fn import_nonexistent_file() {
            let dir = tempfile::tempdir().unwrap();
            let main_path = dir.path().join("main.gnomon");
            std::fs::write(&main_path, "import ./missing.gnomon").unwrap();

            let db = Database::default();
            let result = eval_file(&db, &main_path);
            assert!(
                result.diagnostics.iter().any(|d| d.message.contains("cannot read import")),
                "expected file-not-found error, got: {:?}",
                result.diagnostics,
            );
        }

        #[test]
        fn import_with_let_binding() {
            let dir = tempfile::tempdir().unwrap();

            let lib_path = dir.path().join("lib.gnomon");
            std::fs::write(&lib_path, r#"{ base_priority: 5 }"#).unwrap();

            let main_path = dir.path().join("main.gnomon");
            std::fs::write(
                &main_path,
                r#"
                let defaults = import ./lib.gnomon
                defaults // { name: "custom" }
                "#,
            )
            .unwrap();

            let db = Database::default();
            let result = eval_file(&db, &main_path);
            assert!(result.diagnostics.is_empty(), "diagnostics: {:?}", result.diagnostics);
            let r = expect_record(&result);
            assert_eq!(get_field(r, &db, "base_priority"), Value::Integer(5));
            assert_eq!(get_field(r, &db, "name"), Value::String("custom".into()));
        }

        #[test]
        fn import_with_as_gnomon() {
            let dir = tempfile::tempdir().unwrap();

            let other_path = dir.path().join("other.gnomon");
            std::fs::write(&other_path, "{ val: 1 }").unwrap();

            let main_path = dir.path().join("main.gnomon");
            std::fs::write(&main_path, "import ./other.gnomon as gnomon").unwrap();

            let db = Database::default();
            let result = eval_file(&db, &main_path);
            assert!(result.diagnostics.is_empty(), "diagnostics: {:?}", result.diagnostics);
            let r = expect_record(&result);
            assert_eq!(get_field(r, &db, "val"), Value::Integer(1));
        }

        #[test]
        fn import_string_source() {
            let dir = tempfile::tempdir().unwrap();

            let other_path = dir.path().join("data.gnomon");
            std::fs::write(&other_path, "{ key: 99 }").unwrap();

            let main_path = dir.path().join("main.gnomon");
            std::fs::write(&main_path, r#"import "data.gnomon""#).unwrap();

            let db = Database::default();
            let result = eval_file(&db, &main_path);
            // String source treated as relative path — should find the file.
            assert!(result.diagnostics.is_empty(), "diagnostics: {:?}", result.diagnostics);
            let r = expect_record(&result);
            assert_eq!(get_field(r, &db, "key"), Value::Integer(99));
        }
    }
}
