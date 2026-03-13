pub mod cache;
pub mod desugar;
pub mod export;
pub mod import;
pub mod interned;
pub mod literals;
pub mod lower;
pub mod merge;
pub mod render;
pub mod rrule;
pub mod shape;
pub mod types;

use std::path::PathBuf;

use crate::input::SourceFile;
use crate::queries::Diagnostic;
use types::Value;

/// Result of evaluating a source file.
pub struct EvalResult<'db> {
    pub value: Value<'db>,
    /// Lowering diagnostics (parse + validation diagnostics are obtained separately
    /// via `check_syntax::accumulated::<Diagnostic>()`).
    pub diagnostics: Vec<Diagnostic>,
    /// All transitively imported Gnomon file paths (canonical).
    pub imported_files: Vec<PathBuf>,
}

/// Options controlling evaluation behavior.
#[derive(Debug, Clone, Default)]
pub struct EvalOptions {
    /// If true, bypass the URI import cache and always re-fetch.
    pub force_refresh: bool,
}

/// Evaluate a source file into a value.
///
/// This function calls the tracked `check_syntax` query internally to ensure
/// parse and validation errors are accumulated. Lowering-specific diagnostics
/// are returned in `EvalResult::diagnostics`.
pub fn evaluate<'db>(db: &'db dyn crate::Db, source: SourceFile) -> EvalResult<'db> {
    evaluate_with_options(db, source, &EvalOptions::default())
}

/// Evaluate a source file with the given options.
pub fn evaluate_with_options<'db>(
    db: &'db dyn crate::Db,
    source: SourceFile,
    options: &EvalOptions,
) -> EvalResult<'db> {
    // Run parse + validation (tracked, memoized).
    let _check = crate::check_syntax(db, source);
    let parse_result = crate::parse(db, source);
    let tree = parse_result.tree(db);

    let mut ctx = lower::LowerCtx::new(db, source);
    ctx.force_refresh = options.force_refresh;
    let value = ctx.lower_source_file(&tree);

    EvalResult {
        value,
        diagnostics: ctx.diagnostics,
        imported_files: ctx.imported_files,
    }
}

/// Internal: evaluate with an existing import stack for cycle detection.
pub(super) fn evaluate_with_import_stack<'db>(
    db: &'db dyn crate::Db,
    source: SourceFile,
    import_stack: Vec<std::path::PathBuf>,
    force_refresh: bool,
) -> EvalResult<'db> {
    let _check = crate::check_syntax(db, source);
    let parse_result = crate::parse(db, source);
    let tree = parse_result.tree(db);

    let mut ctx = lower::LowerCtx::with_import_stack(db, source, import_stack);
    ctx.force_refresh = force_refresh;
    let value = ctx.lower_source_file(&tree);

    EvalResult {
        value,
        diagnostics: ctx.diagnostics,
        imported_files: ctx.imported_files,
    }
}

/// Result of evaluating a single REPL input.
pub struct ReplEvalResult<'db> {
    pub value: Value<'db>,
    pub diagnostics: Vec<Diagnostic>,
    /// New top-level let bindings introduced by this input.
    pub new_bindings: Vec<(String, Value<'db>)>,
}

/// Evaluate a single REPL input with a pre-existing environment.
///
/// `env` contains let bindings accumulated from prior inputs.
/// Returns the evaluated value plus any new bindings introduced.
pub fn evaluate_repl_input<'db>(
    db: &'db dyn crate::Db,
    source: SourceFile,
    env: &[(String, Value<'db>)],
) -> ReplEvalResult<'db> {
    let _check = crate::check_syntax(db, source);
    let parse_result = crate::parse(db, source);
    let tree = parse_result.tree(db);

    let mut ctx = lower::LowerCtx::new(db, source);
    ctx.seed_env(env);

    let env_len_before = ctx.env_len();
    let value = ctx.lower_source_file(&tree);
    let new_bindings = ctx.env_slice_from(env_len_before);

    ReplEvalResult {
        value,
        diagnostics: ctx.diagnostics,
        new_bindings,
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

    /// Extract a record from an expression-mode file (not declaration mode).
    fn expect_record<'a, 'db>(result: &'a super::EvalResult<'db>) -> &'a Record<'db> {
        match &result.value {
            Value::Record(r) => r,
            other => panic!("expected Record, got: {other:?}"),
        }
    }

    /// Extract the record from a list item (declaration mode).
    fn expect_list_record<'a, 'db>(
        result: &'a super::EvalResult<'db>,
        index: usize,
    ) -> &'a Record<'db> {
        match &result.value {
            Value::List(items) => match &items[index].value {
                Value::Record(r) => r,
                other => panic!("expected Record at index {index}, got: {other:?}"),
            },
            other => panic!("expected List, got: {other:?}"),
        }
    }

    /// Extract the single record from a declaration-mode file with one declaration.
    fn expect_single_decl<'a, 'db>(result: &'a super::EvalResult<'db>) -> &'a Record<'db> {
        expect_list_record(result, 0)
    }

    /// Unwrap a singleton list containing a calendar record (from iCalendar import).
    fn unwrap_singleton_calendar<'a, 'db>(
        value: &'a Value<'db>,
        db: &'db Database,
    ) -> &'a Record<'db> {
        match value {
            Value::List(items) => {
                assert_eq!(items.len(), 1, "expected singleton calendar list");
                match &items[0].value {
                    Value::Record(r) => {
                        assert_eq!(get_field(r, db, "type"), Value::String("calendar".into()));
                        r
                    }
                    other => panic!("expected calendar record, got: {other:?}"),
                }
            }
            other => panic!("expected list, got: {other:?}"),
        }
    }

    fn expect_list_len(result: &super::EvalResult<'_>) -> usize {
        match &result.value {
            Value::List(items) => items.len(),
            other => panic!("expected List, got: {other:?}"),
        }
    }

    // ── Calendar ─────────────────────────────────────────────────

    // r[verify decl.calendar.desugar+2]
    #[test]
    fn empty_calendar() {
        let db = Database::default();
        let result = eval(&db, "calendar {}");
        let r = expect_single_decl(&result);
        // Calendar now has type: "calendar"
        assert_eq!(get_field(r, &db, "type"), Value::String("calendar".into()));
    }

    // r[verify decl.calendar.desugar+2]
    #[test]
    fn calendar_with_string_field() {
        let db = Database::default();
        let result = eval(&db, r#"calendar { uid: "test-cal" }"#);
        let r = expect_single_decl(&result);
        assert_eq!(get_field(r, &db, "uid"), Value::String("test-cal".into()));
        assert_eq!(get_field(r, &db, "type"), Value::String("calendar".into()));
    }

    // ── Event (prefix form) ──────────────────────────────────────

    // r[verify decl.event.desugar+2]
    // r[verify model.entry.type.infer+2]
    #[test]
    fn event_prefix_form() {
        let db = Database::default();
        let result = eval(
            &db,
            r#"event { name: @standup, start: 2026-03-01T14:00, title: "Standup" }"#,
        );
        let r = expect_single_decl(&result);
        assert_eq!(get_field(r, &db, "name"), Value::Name("standup".into()));
        assert_eq!(get_field(r, &db, "title"), Value::String("Standup".into()));
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

    // r[verify decl.short-event.desugar+2]
    // r[verify record.event.duration]
    #[test]
    fn event_short_form() {
        let db = Database::default();
        let result = eval(&db, r#"event @meeting 2026-03-01T14:30 1h30m "Standup""#);
        let r = expect_single_decl(&result);
        assert_eq!(get_field(r, &db, "name"), Value::Name("meeting".into()));
        assert_eq!(get_field(r, &db, "title"), Value::String("Standup".into()));
        assert!(matches!(get_field(r, &db, "start"), Value::Record(_)));
        assert!(matches!(get_field(r, &db, "duration"), Value::Record(_)));
    }

    // r[verify decl.short-event.desugar+2]
    #[test]
    fn event_short_form_with_body() {
        let db = Database::default();
        let result = eval(
            &db,
            r#"event @meeting 2026-03-01T14:30 1h "Standup" { priority: 5 }"#,
        );
        let r = expect_single_decl(&result);
        assert_eq!(get_field(r, &db, "name"), Value::Name("meeting".into()));
        assert_eq!(get_field(r, &db, "priority"), Value::Integer(5));
    }

    // r[verify decl.short-event.desugar+2]
    // r[verify record.event.start]
    #[test]
    fn event_short_form_date_plus_time() {
        let db = Database::default();
        let result = eval(&db, r#"event @meeting 2026-03-01 14:30 1h "Standup""#);
        let r = expect_single_decl(&result);
        match get_field(r, &db, "start") {
            Value::Record(dt) => {
                assert!(has_field(&dt, &db, "date"));
                assert!(has_field(&dt, &db, "time"));
            }
            _ => panic!("expected Record for start"),
        }
    }

    // ── Task ─────────────────────────────────────────────────────

    // r[verify decl.task.desugar+2]
    // r[verify model.entry.type.infer+2]
    #[test]
    fn task_prefix_form() {
        let db = Database::default();
        let result = eval(&db, r#"task { name: @review, title: "Code review" }"#);
        let r = expect_single_decl(&result);
        assert_eq!(get_field(r, &db, "name"), Value::Name("review".into()));
    }

    // r[verify decl.short-task.desugar+2]
    // r[verify record.task.due]
    #[test]
    fn task_short_form() {
        let db = Database::default();
        let result = eval(&db, r#"task @review 2026-03-15T17:00 "Code review""#);
        let r = expect_single_decl(&result);
        assert_eq!(get_field(r, &db, "name"), Value::Name("review".into()));
        assert_eq!(
            get_field(r, &db, "title"),
            Value::String("Code review".into())
        );
        assert!(matches!(get_field(r, &db, "due"), Value::Record(_)));
    }

    // r[verify decl.short-task.desugar+2]
    #[test]
    fn task_short_form_no_datetime() {
        let db = Database::default();
        let result = eval(&db, r#"task @todo "Do something""#);
        let r = expect_single_decl(&result);
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
        let r = expect_single_decl(&result);
        match get_field(r, &db, "location") {
            Value::Record(loc) => {
                assert_eq!(get_field(&loc, &db, "name"), Value::String("Office".into()));
            }
            _ => panic!("expected nested Record"),
        }
    }

    #[test]
    fn list_of_strings() {
        let db = Database::default();
        let result = eval(&db, r#"calendar { keywords: ["work", "meeting"] }"#);
        let r = expect_single_decl(&result);
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
        let r = expect_single_decl(&result);
        assert_eq!(get_field(r, &db, "show_without_time"), Value::Bool(true));
        assert_eq!(get_field(r, &db, "expect_reply"), Value::Bool(false));
    }

    #[test]
    fn undefined_literal() {
        let db = Database::default();
        let result = eval(&db, "calendar { x: undefined }");
        let r = expect_single_decl(&result);
        assert_eq!(get_field(r, &db, "x"), Value::Undefined);
    }

    #[test]
    fn integer_and_signed_integer() {
        let db = Database::default();
        let result = eval(&db, "calendar { priority: 5, offset: -3 }");
        let r = expect_single_decl(&result);
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
        let r = expect_single_decl(&result);
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
        let r = expect_single_decl(&result);
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
        let r = expect_single_decl(&result);
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

    // r[verify expr.let.scope+2]
    // r[verify expr.let.syntax+2]
    #[test]
    fn let_expression() {
        let db = Database::default();
        let result = eval(&db, r#"let x = 42 in { count: x }"#);
        let r = expect_record(&result);
        assert_eq!(get_field(r, &db, "count"), Value::Integer(42));
    }

    // r[verify expr.let.syntax+2]
    // r[verify expr.let.scope+2]
    #[test]
    fn multi_binding_let_expression() {
        let db = Database::default();
        let result = eval(&db, r#"let x = 1 let y = 2 in { a: x, b: y }"#);
        let r = expect_record(&result);
        assert_eq!(get_field(r, &db, "a"), Value::Integer(1));
        assert_eq!(get_field(r, &db, "b"), Value::Integer(2));
    }

    // r[verify expr.let.sequential]
    // r[verify expr.let.scope+2]
    #[test]
    fn multi_binding_let_sequential() {
        let db = Database::default();
        // Each binding can reference earlier bindings.
        let result = eval(&db, r#"let x = 1 let y = x let z = y in z"#);
        assert_eq!(result.value, Value::Integer(1));
    }

    // r[verify expr.literal.identifier]
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

    // r[verify expr.op.concat]
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

    // r[verify expr.op.merge]
    #[test]
    fn binary_merge_records() {
        let db = Database::default();
        let result = eval(&db, r#"{ a: 1 } // { b: 2 }"#);
        let r = expect_record(&result);
        assert_eq!(get_field(r, &db, "a"), Value::Integer(1));
        assert_eq!(get_field(r, &db, "b"), Value::Integer(2));
    }

    // r[verify expr.op.eq]
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

    // r[verify expr.op.field]
    #[test]
    fn field_access() {
        let db = Database::default();
        let result = eval(&db, r#"{ x: 42 }.x"#);
        assert_eq!(result.value, Value::Integer(42));
    }

    // r[verify expr.op.index]
    #[test]
    fn index_access() {
        let db = Database::default();
        let result = eval(&db, "[10, 20, 30][1]");
        assert_eq!(result.value, Value::Integer(20));
    }

    // r[verify syntax.file.let]
    // r[verify syntax.file.body+2]
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
        // Declaration mode: produces a list
        let r = expect_single_decl(&result);
        assert_eq!(get_field(r, &db, "name"), Value::Name("e".into()));
    }

    // r[verify syntax.file.let]
    // r[verify syntax.file.body+2]
    // r[verify expr.let.sequential]
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

    // r[verify expr.op.assoc.concat-merge]
    #[test]
    fn binary_right_associative() {
        let db = Database::default();
        // [1] ++ [2] ++ [3] should be [1] ++ ([2] ++ [3]) = [1, 2, 3] (right-associative)
        let result = eval(&db, "[1] ++ [2] ++ [3]");
        match &result.value {
            Value::List(items) => assert_eq!(items.len(), 3),
            other => panic!("expected List, got: {other:?}"),
        }
    }

    // r[verify expr.op.assoc.concat-merge]
    #[test]
    fn merge_right_associative() {
        let db = Database::default();
        // { a: 1 } // { a: 2 } // { a: 3 } should yield { a: 3 }
        let result = eval(&db, "{ a: 1 } // { a: 2 } // { a: 3 }");
        let r = expect_record(&result);
        assert_eq!(get_field(r, &db, "a"), Value::Integer(3));
    }

    // r[verify expr.op.precedence]
    #[test]
    fn concat_binds_tighter_than_comparison() {
        let db = Database::default();
        // [1] ++ [2] == [1, 2] should parse as ([1] ++ [2]) == [1, 2] → true
        let result = eval(&db, "[1] ++ [2] == [1, 2]");
        match &result.value {
            Value::Bool(b) => assert!(*b),
            other => panic!("expected Bool(true), got: {other:?}"),
        }
    }

    // r[verify lexer.triple-string.desugar]
    #[test]
    fn triple_string_in_record() {
        let db = Database::default();
        let input = "{ desc: \"\"\"hello world\"\"\" }";
        let result = eval(&db, input);
        let r = expect_record(&result);
        assert_eq!(
            get_field(r, &db, "desc"),
            Value::String("hello world".into())
        );
    }

    // r[verify lexer.triple-string.dedent]
    #[test]
    fn triple_string_dedent_in_record() {
        let db = Database::default();
        let input = "{\n    desc: \"\"\"\n        hello\n        world\n        \"\"\",\n}";
        let result = eval(&db, input);
        let r = expect_record(&result);
        assert_eq!(
            get_field(r, &db, "desc"),
            Value::String("hello\nworld".into())
        );
    }

    // ── Import expressions ──────────────────────────────────────

    mod import_tests {
        use super::*;
        use std::io::Write;

        fn eval_file<'db>(
            db: &'db Database,
            path: &std::path::Path,
        ) -> super::super::EvalResult<'db> {
            let text = std::fs::read_to_string(path).unwrap();
            let sf = SourceFile::new(db, path.to_path_buf(), text);
            super::super::evaluate(db, sf)
        }

        // r[verify expr.import.eval]
        // r[verify expr.import.eager]
        // r[verify expr.import.syntax+2]
        // r[verify model.import.resolution]
        // r[verify model.import.transparent]
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
            assert!(
                result.diagnostics.is_empty(),
                "diagnostics: {:?}",
                result.diagnostics
            );
            let r = expect_record(&result);
            assert_eq!(get_field(r, &db, "x"), Value::Integer(42));
        }

        // r[verify expr.import.cycle]
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
                result
                    .diagnostics
                    .iter()
                    .any(|d| d.message.contains("circular import")),
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
                result
                    .diagnostics
                    .iter()
                    .any(|d| d.message.contains("cannot read import")),
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
            assert!(
                result.diagnostics.is_empty(),
                "diagnostics: {:?}",
                result.diagnostics
            );
            let r = expect_record(&result);
            assert_eq!(get_field(r, &db, "base_priority"), Value::Integer(5));
            assert_eq!(get_field(r, &db, "name"), Value::String("custom".into()));
        }

        // r[verify expr.import.format+2]
        #[test]
        fn import_with_as_gnomon() {
            let dir = tempfile::tempdir().unwrap();

            let other_path = dir.path().join("other.gnomon");
            std::fs::write(&other_path, "{ val: 1 }").unwrap();

            let main_path = dir.path().join("main.gnomon");
            std::fs::write(&main_path, "import ./other.gnomon as gnomon").unwrap();

            let db = Database::default();
            let result = eval_file(&db, &main_path);
            assert!(
                result.diagnostics.is_empty(),
                "diagnostics: {:?}",
                result.diagnostics
            );
            let r = expect_record(&result);
            assert_eq!(get_field(r, &db, "val"), Value::Integer(1));
        }

        // ── iCalendar import ────────────────────────────────────

        // r[verify expr.import.format+2]
        #[test]
        fn import_icalendar_explicit_format() {
            let dir = tempfile::tempdir().unwrap();

            let ics_path = dir.path().join("cal.ics");
            std::fs::write(
                &ics_path,
                "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//EN\r\n\
                 BEGIN:VEVENT\r\nUID:ev1\r\nSUMMARY:Lunch\r\n\
                 DTSTART:20260315T120000\r\nDURATION:PT1H\r\n\
                 END:VEVENT\r\nEND:VCALENDAR\r\n",
            )
            .unwrap();

            let main_path = dir.path().join("main.gnomon");
            std::fs::write(&main_path, "import ./cal.ics as icalendar").unwrap();

            let db = Database::default();
            let result = eval_file(&db, &main_path);
            assert!(
                result.diagnostics.is_empty(),
                "diagnostics: {:?}",
                result.diagnostics
            );
            let cal = unwrap_singleton_calendar(&result.value, &db);
            match get_field(cal, &db, "entries") {
                Value::List(items) => {
                    assert_eq!(items.len(), 1);
                    match &items[0].value {
                        Value::Record(e) => {
                            assert_eq!(get_field(e, &db, "type"), Value::String("event".into()));
                            assert_eq!(get_field(e, &db, "uid"), Value::String("ev1".into()));
                            assert_eq!(get_field(e, &db, "title"), Value::String("Lunch".into()));
                        }
                        _ => panic!("expected event record"),
                    }
                }
                _ => panic!("expected entries list"),
            }
        }

        // r[verify expr.import.format+2]
        #[test]
        fn import_icalendar_inferred_from_extension() {
            let dir = tempfile::tempdir().unwrap();

            let ics_path = dir.path().join("cal.ics");
            std::fs::write(
                &ics_path,
                "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//EN\r\n\
                 BEGIN:VEVENT\r\nUID:ev-infer\r\nSUMMARY:Inferred\r\n\
                 DTSTART:20260101T090000\r\nDURATION:PT30M\r\n\
                 END:VEVENT\r\nEND:VCALENDAR\r\n",
            )
            .unwrap();

            let main_path = dir.path().join("main.gnomon");
            // No `as icalendar` — should infer from .ics extension.
            std::fs::write(&main_path, "import ./cal.ics").unwrap();

            let db = Database::default();
            let result = eval_file(&db, &main_path);
            assert!(
                result.diagnostics.is_empty(),
                "diagnostics: {:?}",
                result.diagnostics
            );
            let cal = unwrap_singleton_calendar(&result.value, &db);
            match get_field(cal, &db, "entries") {
                Value::List(items) => {
                    assert_eq!(items.len(), 1);
                    match &items[0].value {
                        Value::Record(e) => {
                            assert_eq!(get_field(e, &db, "uid"), Value::String("ev-infer".into()));
                        }
                        _ => panic!("expected event record"),
                    }
                }
                _ => panic!("expected entries list"),
            }
        }

        // ── JSCalendar import ───────────────────────────────────

        // r[verify expr.import.format+2]
        #[test]
        fn import_jscalendar_explicit_format() {
            let dir = tempfile::tempdir().unwrap();

            let json_path = dir.path().join("event.json");
            std::fs::write(
                &json_path,
                r#"{ "@type": "Event", "uid": "js1", "title": "JS Event", "start": "2026-03-15T10:00:00", "duration": "PT2H" }"#,
            )
            .unwrap();

            let main_path = dir.path().join("main.gnomon");
            std::fs::write(&main_path, "import ./event.json as jscalendar").unwrap();

            let db = Database::default();
            let result = eval_file(&db, &main_path);
            assert!(
                result.diagnostics.is_empty(),
                "diagnostics: {:?}",
                result.diagnostics
            );
            let r = expect_record(&result);
            assert_eq!(get_field(r, &db, "type"), Value::String("event".into()));
            assert_eq!(get_field(r, &db, "uid"), Value::String("js1".into()));
            assert_eq!(get_field(r, &db, "title"), Value::String("JS Event".into()));
            assert!(has_field(r, &db, "start"));
            assert!(has_field(r, &db, "duration"));
        }

        // r[verify expr.import.format+2]
        #[test]
        fn import_jscalendar_inferred_from_extension() {
            let dir = tempfile::tempdir().unwrap();

            let json_path = dir.path().join("task.json");
            std::fs::write(
                &json_path,
                r#"{ "@type": "Task", "uid": "t-infer", "title": "Inferred Task" }"#,
            )
            .unwrap();

            let main_path = dir.path().join("main.gnomon");
            // No `as jscalendar` — should infer from .json extension.
            std::fs::write(&main_path, "import ./task.json").unwrap();

            let db = Database::default();
            let result = eval_file(&db, &main_path);
            assert!(
                result.diagnostics.is_empty(),
                "diagnostics: {:?}",
                result.diagnostics
            );
            let r = expect_record(&result);
            assert_eq!(get_field(r, &db, "type"), Value::String("task".into()));
            assert_eq!(get_field(r, &db, "uid"), Value::String("t-infer".into()));
        }

        #[test]
        fn import_icalendar_malformed() {
            let dir = tempfile::tempdir().unwrap();

            let ics_path = dir.path().join("bad.ics");
            std::fs::write(&ics_path, "this is not valid icalendar").unwrap();

            let main_path = dir.path().join("main.gnomon");
            std::fs::write(&main_path, "import ./bad.ics as icalendar").unwrap();

            let db = Database::default();
            let result = eval_file(&db, &main_path);
            assert!(
                result
                    .diagnostics
                    .iter()
                    .any(|d| d.message.contains("iCalendar parse error")),
                "expected parse error, got: {:?}",
                result.diagnostics,
            );
        }

        #[test]
        fn import_jscalendar_malformed() {
            let dir = tempfile::tempdir().unwrap();

            let json_path = dir.path().join("bad.json");
            std::fs::write(&json_path, "not json{").unwrap();

            let main_path = dir.path().join("main.gnomon");
            std::fs::write(&main_path, "import ./bad.json as jscalendar").unwrap();

            let db = Database::default();
            let result = eval_file(&db, &main_path);
            assert!(
                result
                    .diagnostics
                    .iter()
                    .any(|d| d.message.contains("JSCalendar JSON parse error")),
                "expected parse error, got: {:?}",
                result.diagnostics,
            );
        }

        // ── URI imports ───────────────────────────────────────────

        /// Spin up a tiny HTTP server on localhost that serves `body` with the
        /// given `content_type` at any path, returning the socket address.
        fn serve_once(body: &str, content_type: &str) -> std::net::SocketAddr {
            use std::net::TcpListener;

            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body,
            );
            std::thread::spawn(move || {
                // Accept one connection, write the response, close.
                let (mut stream, _) = listener.accept().unwrap();
                std::io::Write::write_all(&mut stream, response.as_bytes()).unwrap();
            });
            addr
        }

        // r[verify expr.import.format.uri]
        #[test]
        fn import_uri_icalendar() {
            let ics = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//EN\r\n\
                        BEGIN:VEVENT\r\nUID:uri-ev1\r\nSUMMARY:URI Event\r\n\
                        DTSTART:20260315T120000\r\nDURATION:PT1H\r\n\
                        END:VEVENT\r\nEND:VCALENDAR\r\n";
            let addr = serve_once(ics, "text/calendar");

            let dir = tempfile::tempdir().unwrap();
            let main_path = dir.path().join("main.gnomon");
            std::fs::write(
                &main_path,
                format!("import <http://{addr}/cal.ics> as icalendar"),
            )
            .unwrap();

            let db = Database::default();
            let result = eval_file(&db, &main_path);
            assert!(
                result.diagnostics.is_empty(),
                "diagnostics: {:?}",
                result.diagnostics
            );
            let cal = unwrap_singleton_calendar(&result.value, &db);
            match get_field(cal, &db, "entries") {
                Value::List(items) => {
                    assert_eq!(items.len(), 1);
                    match &items[0].value {
                        Value::Record(e) => {
                            assert_eq!(get_field(e, &db, "uid"), Value::String("uri-ev1".into()));
                            assert_eq!(
                                get_field(e, &db, "title"),
                                Value::String("URI Event".into())
                            );
                        }
                        _ => panic!("expected event record"),
                    }
                }
                _ => panic!("expected entries list"),
            }
        }

        #[test]
        fn import_uri_jscalendar() {
            let json = r#"{ "@type": "Event", "uid": "uri-js1", "title": "URI JS", "start": "2026-03-15T10:00:00", "duration": "PT2H" }"#;
            let addr = serve_once(json, "application/json");

            let dir = tempfile::tempdir().unwrap();
            let main_path = dir.path().join("main.gnomon");
            std::fs::write(
                &main_path,
                format!("import <http://{addr}/event.json> as jscalendar"),
            )
            .unwrap();

            let db = Database::default();
            let result = eval_file(&db, &main_path);
            assert!(
                result.diagnostics.is_empty(),
                "diagnostics: {:?}",
                result.diagnostics
            );
            let r = expect_record(&result);
            assert_eq!(get_field(r, &db, "uid"), Value::String("uri-js1".into()));
            assert_eq!(get_field(r, &db, "title"), Value::String("URI JS".into()));
        }

        #[test]
        fn import_uri_gnomon() {
            let gnomon_src = "{ x: 42 }";
            let addr = serve_once(gnomon_src, "text/plain");

            let dir = tempfile::tempdir().unwrap();
            let main_path = dir.path().join("main.gnomon");
            std::fs::write(
                &main_path,
                format!("import <http://{addr}/data.gn> as gnomon"),
            )
            .unwrap();

            let db = Database::default();
            let result = eval_file(&db, &main_path);
            assert!(
                result.diagnostics.is_empty(),
                "diagnostics: {:?}",
                result.diagnostics
            );
            let r = expect_record(&result);
            assert_eq!(get_field(r, &db, "x"), Value::Integer(42));
        }

        #[test]
        fn import_uri_inferred_from_extension() {
            let ics = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//EN\r\n\
                        BEGIN:VEVENT\r\nUID:ext-infer\r\nSUMMARY:Ext Infer\r\n\
                        DTSTART:20260101T090000\r\nDURATION:PT30M\r\n\
                        END:VEVENT\r\nEND:VCALENDAR\r\n";
            let addr = serve_once(ics, "application/octet-stream");

            let dir = tempfile::tempdir().unwrap();
            let main_path = dir.path().join("main.gnomon");
            // URL path ends in .ics — format should be inferred.
            std::fs::write(&main_path, format!("import <http://{addr}/cal.ics>")).unwrap();

            let db = Database::default();
            let result = eval_file(&db, &main_path);
            assert!(
                result.diagnostics.is_empty(),
                "diagnostics: {:?}",
                result.diagnostics
            );
            let cal = unwrap_singleton_calendar(&result.value, &db);
            match get_field(cal, &db, "entries") {
                Value::List(items) => {
                    assert_eq!(items.len(), 1);
                    match &items[0].value {
                        Value::Record(e) => {
                            assert_eq!(get_field(e, &db, "uid"), Value::String("ext-infer".into()));
                        }
                        _ => panic!("expected event record"),
                    }
                }
                _ => panic!("expected entries list"),
            }
        }

        #[test]
        fn import_uri_network_error() {
            let dir = tempfile::tempdir().unwrap();
            let main_path = dir.path().join("main.gnomon");
            // Use a port that is (almost certainly) not listening.
            std::fs::write(&main_path, "import <http://127.0.0.1:1/>").unwrap();

            let db = Database::default();
            let result = eval_file(&db, &main_path);
            assert!(
                result
                    .diagnostics
                    .iter()
                    .any(|d| d.message.contains("URI import failed")),
                "expected network error, got: {:?}",
                result.diagnostics,
            );
        }

        #[test]
        fn import_uri_inferred_from_content_type() {
            let ics = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//EN\r\n\
                        BEGIN:VEVENT\r\nUID:ct-infer\r\nSUMMARY:CT Infer\r\n\
                        DTSTART:20260101T090000\r\nDURATION:PT30M\r\n\
                        END:VEVENT\r\nEND:VCALENDAR\r\n";
            // URL has no recognized extension; Content-Type is text/calendar.
            let addr = serve_once(ics, "text/calendar");

            let dir = tempfile::tempdir().unwrap();
            let main_path = dir.path().join("main.gnomon");
            std::fs::write(&main_path, format!("import <http://{addr}/feed>")).unwrap();

            let db = Database::default();
            let result = eval_file(&db, &main_path);
            assert!(
                result.diagnostics.is_empty(),
                "diagnostics: {:?}",
                result.diagnostics
            );
            let cal = unwrap_singleton_calendar(&result.value, &db);
            match get_field(cal, &db, "entries") {
                Value::List(items) => {
                    assert_eq!(items.len(), 1);
                    match &items[0].value {
                        Value::Record(entry) => {
                            assert_eq!(
                                get_field(entry, &db, "uid"),
                                Value::String("ct-infer".into())
                            );
                        }
                        _ => panic!("expected record"),
                    }
                }
                _ => panic!("expected entries list"),
            }
        }

        // ── Imported files tracking ────────────────────────────

        #[test]
        fn imported_files_tracked() {
            let dir = tempfile::tempdir().unwrap();

            let other_path = dir.path().join("other.gnomon");
            std::fs::write(&other_path, "{ x: 42 }").unwrap();

            let main_path = dir.path().join("main.gnomon");
            std::fs::write(&main_path, "import ./other.gnomon").unwrap();

            let db = Database::default();
            let result = eval_file(&db, &main_path);
            assert!(
                result.diagnostics.is_empty(),
                "diagnostics: {:?}",
                result.diagnostics
            );

            let canon = other_path.canonicalize().unwrap();
            assert!(
                result.imported_files.contains(&canon),
                "expected imported_files to contain {:?}, got: {:?}",
                canon,
                result.imported_files,
            );
        }

        #[test]
        fn imported_files_transitive() {
            let dir = tempfile::tempdir().unwrap();

            let c_path = dir.path().join("c.gnomon");
            std::fs::write(&c_path, "{ z: 1 }").unwrap();

            let b_path = dir.path().join("b.gnomon");
            std::fs::write(&b_path, "import ./c.gnomon").unwrap();

            let a_path = dir.path().join("a.gnomon");
            std::fs::write(&a_path, "import ./b.gnomon").unwrap();

            let db = Database::default();
            let result = eval_file(&db, &a_path);
            assert!(
                result.diagnostics.is_empty(),
                "diagnostics: {:?}",
                result.diagnostics
            );

            let b_canon = b_path.canonicalize().unwrap();
            let c_canon = c_path.canonicalize().unwrap();
            assert!(
                result.imported_files.contains(&b_canon),
                "expected imported_files to contain b: {:?}, got: {:?}",
                b_canon,
                result.imported_files,
            );
            assert!(
                result.imported_files.contains(&c_canon),
                "expected imported_files to contain c: {:?}, got: {:?}",
                c_canon,
                result.imported_files,
            );
        }

        #[test]
        fn imported_files_excludes_non_gnomon() {
            let dir = tempfile::tempdir().unwrap();

            let ics_path = dir.path().join("cal.ics");
            std::fs::write(
                &ics_path,
                "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//EN\r\n\
                 BEGIN:VEVENT\r\nUID:ev1\r\nSUMMARY:Lunch\r\n\
                 DTSTART:20260315T120000\r\nDURATION:PT1H\r\n\
                 END:VEVENT\r\nEND:VCALENDAR\r\n",
            )
            .unwrap();

            let main_path = dir.path().join("main.gnomon");
            std::fs::write(&main_path, "import ./cal.ics").unwrap();

            let db = Database::default();
            let result = eval_file(&db, &main_path);
            assert!(
                result.diagnostics.is_empty(),
                "diagnostics: {:?}",
                result.diagnostics
            );
            assert!(
                result.imported_files.is_empty(),
                "non-Gnomon imports should not appear in imported_files, got: {:?}",
                result.imported_files,
            );
        }
    }
}
