pub mod eval;
pub mod input;
pub mod queries;

pub use eval::interned::{DeclId, DeclKind, FieldName, FieldPath, PathSegment};
pub use eval::merge::{CheckResult, validate_calendar};
pub use eval::render::{RenderWithDb, Rendered};
pub use eval::types::{Blame, Blamed, Calendar, Record, Value};
pub use eval::{
    EvalOptions, EvalResult, ReplEvalResult, evaluate, evaluate_repl_input, evaluate_with_options,
};
pub use input::SourceFile;
pub use queries::{Diagnostic, ParseResult, Severity, SyntaxCheckResult, check_syntax, parse};
pub use rowan::TextRange;

#[salsa::db]
pub trait Db: salsa::Database {}

#[salsa::db]
#[derive(Default)]
pub struct Database {
    storage: salsa::Storage<Self>,
}

#[salsa::db]
impl Db for Database {}

impl salsa::Database for Database {}

#[cfg(test)]
mod tests {
    use super::*;
    use salsa::Setter;
    use std::path::PathBuf;

    #[test]
    fn basic_parse_query() {
        let db = Database::default();
        let source = SourceFile::new(
            &db,
            PathBuf::from("test.gnomon"),
            r#"calendar { uid: "test" }"#.into(),
        );
        let result = parse(&db, source);
        assert!(!result.has_errors(&db));
        let tree = result.tree(&db);
        assert_eq!(tree.body_exprs().count(), 1,);
    }

    #[test]
    fn errors_accumulated_as_diagnostics() {
        let db = Database::default();
        let source = SourceFile::new(&db, PathBuf::from("bad.gnomon"), "~~~ calendar {}".into());
        let result = parse(&db, source);
        assert!(result.has_errors(&db));
        let diagnostics = parse::accumulated::<Diagnostic>(&db, source);
        assert!(!diagnostics.is_empty());
    }

    #[test]
    fn incremental_same_text_reuses_cache() {
        let mut db = Database::default();
        let source = SourceFile::new(&db, PathBuf::from("test.gnomon"), "calendar {}".into());

        // First parse
        let result1 = parse(&db, source);
        let green1 = result1.green_node(&db).clone();

        // Set same text — salsa should detect no change
        source.set_text(&mut db).to("calendar {}".into());
        let result2 = parse(&db, source);
        let green2 = result2.green_node(&db).clone();

        assert_eq!(green1, green2);
    }

    #[test]
    fn check_syntax_accumulates_validation_errors() {
        let db = Database::default();
        let source = SourceFile::new(
            &db,
            PathBuf::from("dup.gnomon"),
            "calendar { uid: \"a\", uid: \"b\" }".into(),
        );
        let result = check_syntax(&db, source);
        assert!(!result.parse_has_errors(&db));
        let diagnostics = check_syntax::accumulated::<Diagnostic>(&db, source);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message.contains("duplicate field"))
        );
    }

    #[test]
    fn check_syntax_accumulates_parse_and_validation_errors() {
        let db = Database::default();
        // "~~~" causes parse errors; overflow causes a validation error
        // The error recovery wraps ~~~ as error nodes, then the parser picks up
        // the calendar declaration containing the overflow.
        let source = SourceFile::new(
            &db,
            PathBuf::from("both.gnomon"),
            "~~~ calendar { count: 99999999999999999999999 }".into(),
        );
        let result = check_syntax(&db, source);
        assert!(result.parse_has_errors(&db));
        let diagnostics = check_syntax::accumulated::<Diagnostic>(&db, source);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message.contains("expected") || d.message.contains("declaration")),
            "should have parse error, got: {diagnostics:?}"
        );
        assert!(
            diagnostics.iter().any(|d| d.message.contains("overflows")),
            "should have overflow error, got: {diagnostics:?}"
        );
    }

    #[test]
    fn incremental_different_text_reparses() {
        let mut db = Database::default();
        let source = SourceFile::new(&db, PathBuf::from("test.gnomon"), "calendar {}".into());

        let result1 = parse(&db, source);
        assert!(!result1.has_errors(&db));
        let green1 = result1.green_node(&db).clone();

        // Change source text
        source
            .set_text(&mut db)
            .to(r#"calendar { uid: "new" }"#.into());
        let result2 = parse(&db, source);
        assert!(!result2.has_errors(&db));
        let green2 = result2.green_node(&db).clone();

        // The green node should differ
        assert_ne!(green1, green2);
    }
}
