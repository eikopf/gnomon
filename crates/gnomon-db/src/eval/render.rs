use std::fmt;

use crate::Db;

use super::interned::{DeclId, DeclKind, FieldName, FieldPath, PathSegment};
use super::types::{Blame, Blamed, Calendar, Record, Value};

/// Format a value using the salsa database for name resolution.
pub trait RenderWithDb<'db> {
    fn render_fmt(&self, f: &mut fmt::Formatter<'_>, db: &'db dyn Db) -> fmt::Result;

    fn render<'a>(&'a self, db: &'db dyn Db) -> Rendered<'a, 'db, Self> {
        Rendered { value: self, db }
    }
}

/// Wrapper that bridges [`RenderWithDb`] into [`Display`](fmt::Display).
pub struct Rendered<'a, 'db, T: ?Sized> {
    value: &'a T,
    db: &'db dyn Db,
}

impl<'db, T: RenderWithDb<'db> + ?Sized> fmt::Display for Rendered<'_, 'db, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.value.render_fmt(f, self.db)
    }
}

// ── Indented-output helpers ────────────────────────────────────────

fn write_indent(w: &mut dyn fmt::Write, n: usize) -> fmt::Result {
    for _ in 0..n {
        w.write_char(' ')?;
    }
    Ok(())
}

// r[impl cli.subcommand.eval.output.string]
// r[impl cli.subcommand.eval.output.integer]
// r[impl cli.subcommand.eval.output.bool]
// r[impl cli.subcommand.eval.output.undefined]
// r[impl cli.subcommand.eval.output.name]
// r[impl cli.subcommand.eval.output.list]
fn write_value<'db>(
    w: &mut dyn fmt::Write,
    value: &Value<'db>,
    db: &'db dyn Db,
    indent: usize,
) -> fmt::Result {
    match value {
        Value::String(s) => write!(w, "{s:?}"),
        Value::Integer(n) => write!(w, "{n}"),
        Value::SignedInteger(n) => write!(w, "{n}"),
        Value::Bool(b) => write!(w, "{b}"),
        Value::Undefined => write!(w, "undefined"),
        Value::Name(n) => write!(w, "@{n}"),
        Value::Path(p) => write!(w, "{p}"),
        Value::Record(r) => write_record(w, r, db, indent),
        Value::List(items) => {
            write!(w, "[")?;
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    write!(w, ", ")?;
                }
                write_value(w, &item.value, db, indent)?;
            }
            write!(w, "]")
        }
    }
}

// r[impl cli.subcommand.eval.output.record]
fn write_record<'db>(
    w: &mut dyn fmt::Write,
    record: &Record<'db>,
    db: &'db dyn Db,
    indent: usize,
) -> fmt::Result {
    if record.is_empty() {
        return write!(w, "{{}}");
    }

    writeln!(w, "{{")?;
    for (name, blamed) in record.iter() {
        write_indent(w, indent + 4)?;
        write!(w, "{}: ", name.text(db))?;
        write_value(w, &blamed.value, db, indent + 4)?;
        writeln!(w, ",")?;
    }
    write_indent(w, indent)?;
    write!(w, "}}")
}

// ── Interned types ──────────────────────────────────────────────────

impl<'db> RenderWithDb<'db> for FieldName<'db> {
    fn render_fmt(&self, f: &mut fmt::Formatter<'_>, db: &'db dyn Db) -> fmt::Result {
        write!(f, "{}", self.text(db))
    }
}

impl<'db> RenderWithDb<'db> for DeclKind {
    fn render_fmt(&self, f: &mut fmt::Formatter<'_>, _db: &'db dyn Db) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl<'db> RenderWithDb<'db> for DeclId<'db> {
    fn render_fmt(&self, f: &mut fmt::Formatter<'_>, db: &'db dyn Db) -> fmt::Result {
        write!(f, "{}#{}", self.kind(db).render(db), self.index(db))
    }
}

impl<'db> RenderWithDb<'db> for PathSegment<'db> {
    fn render_fmt(&self, f: &mut fmt::Formatter<'_>, db: &'db dyn Db) -> fmt::Result {
        match self {
            PathSegment::Field(name) => name.render_fmt(f, db),
            PathSegment::Index(i) => write!(f, "[{i}]"),
        }
    }
}

impl<'db> RenderWithDb<'db> for FieldPath<'db> {
    fn render_fmt(&self, f: &mut fmt::Formatter<'_>, db: &'db dyn Db) -> fmt::Result {
        for (i, segment) in self.segments().iter().enumerate() {
            if i > 0 && matches!(segment, PathSegment::Field(_)) {
                write!(f, ".")?;
            }
            segment.render_fmt(f, db)?;
        }
        Ok(())
    }
}

// ── Value types ─────────────────────────────────────────────────────

impl<'db> RenderWithDb<'db> for Value<'db> {
    fn render_fmt(&self, f: &mut fmt::Formatter<'_>, db: &'db dyn Db) -> fmt::Result {
        write_value(f, self, db, 0)
    }
}

impl<'db> RenderWithDb<'db> for Record<'db> {
    fn render_fmt(&self, f: &mut fmt::Formatter<'_>, db: &'db dyn Db) -> fmt::Result {
        write_record(f, self, db, 0)
    }
}

impl<'db, T: RenderWithDb<'db>> RenderWithDb<'db> for Blamed<'db, T> {
    fn render_fmt(&self, f: &mut fmt::Formatter<'_>, db: &'db dyn Db) -> fmt::Result {
        self.value.render_fmt(f, db)
    }
}

impl<'db> RenderWithDb<'db> for Blame<'db> {
    fn render_fmt(&self, f: &mut fmt::Formatter<'_>, db: &'db dyn Db) -> fmt::Result {
        write!(f, "{}@", self.decl.render(db))?;
        self.path.render_fmt(f, db)
    }
}

impl<'db> RenderWithDb<'db> for String {
    fn render_fmt(&self, f: &mut fmt::Formatter<'_>, _db: &'db dyn Db) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

fn write_record_list<'db>(
    w: &mut dyn fmt::Write,
    items: &[Blamed<'db, Record<'db>>],
    db: &'db dyn Db,
    indent: usize,
) -> fmt::Result {
    if items.is_empty() {
        return write!(w, "[]");
    }
    writeln!(w, "[")?;
    for item in items {
        write_indent(w, indent + 4)?;
        write_record(w, &item.value, db, indent + 4)?;
        writeln!(w, ",")?;
    }
    write_indent(w, indent)?;
    write!(w, "]")
}

impl<'db> RenderWithDb<'db> for Calendar<'db> {
    fn render_fmt(&self, f: &mut fmt::Formatter<'_>, db: &'db dyn Db) -> fmt::Result {
        writeln!(f, "Calendar {{")?;

        // Properties
        write!(f, "    properties: ")?;
        write_record(f, &self.properties, db, 4)?;
        writeln!(f, ",")?;

        // Entries
        write!(f, "    entries: ")?;
        write_record_list(f, &self.entries, db, 4)?;
        writeln!(f, ",")?;

        write!(f, "}}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Database, SourceFile};
    use expect_test::{Expect, expect};
    use std::path::PathBuf;

    fn check(source: &str, expected: Expect) {
        let db = Database::default();
        let sf = SourceFile::new(&db, PathBuf::from("test.gnomon"), source.into());
        let result = crate::evaluate(&db, sf);
        let rendered = format!("{}", result.value.render(&db));
        expected.assert_eq(&rendered);
    }

    #[test]
    fn empty_calendar() {
        check(
            "calendar {}",
            expect![[r#"
            [{
                type: "calendar",
            }]"#]],
        );
    }

    // r[verify cli.subcommand.eval.output.string]
    // r[verify cli.subcommand.eval.output.record]
    #[test]
    fn calendar_with_fields() {
        check(
            r#"calendar { uid: "test-cal", name: "My Cal" }"#,
            expect![[r#"
                [{
                    name: "My Cal",
                    type: "calendar",
                    uid: "test-cal",
                }]"#]],
        );
    }

    // r[verify cli.subcommand.eval.output.name]
    // r[verify cli.subcommand.eval.output.integer]
    #[test]
    fn event_short_form() {
        check(
            r#"event @meeting 2026-03-01T14:30 1h30m "Standup""#,
            expect![[r#"
                [{
                    duration: {
                        days: 0,
                        hours: 1,
                        minutes: 30,
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
                }]"#]],
        );
    }

    // r[verify cli.subcommand.eval.output.list]
    #[test]
    fn list_values() {
        check(
            r#"calendar { keywords: ["work", "meeting"] }"#,
            expect![[r#"
                [{
                    keywords: ["work", "meeting"],
                    type: "calendar",
                }]"#]],
        );
    }

    #[test]
    fn task_short_form() {
        check(
            r#"task @review 2026-03-15T17:00 "Code review""#,
            expect![[r#"
                [{
                    due: {
                        date: {
                            day: 15,
                            month: 3,
                            year: 2026,
                        },
                        time: {
                            hour: 17,
                            minute: 0,
                            second: 0,
                        },
                    },
                    name: @review,
                    title: "Code review",
                    type: "task",
                }]"#]],
        );
    }

    // r[verify cli.subcommand.eval.output.bool]
    #[test]
    fn boolean_and_integer_values() {
        check(
            "calendar { show_without_time: true, priority: 5 }",
            expect![[r#"
                [{
                    priority: 5,
                    show_without_time: true,
                    type: "calendar",
                }]"#]],
        );
    }

    // r[verify cli.subcommand.eval.output.undefined]
    #[test]
    fn undefined_value() {
        check(
            "calendar { optional: undefined }",
            expect![[r#"
                [{
                    optional: undefined,
                    type: "calendar",
                }]"#]],
        );
    }

    /// Fields are rendered in lexicographic order regardless of source order.
    #[test]
    fn fields_in_alphabetical_order() {
        // Source has fields in reverse-alphabetical order (z, m, a),
        // but rendered output must sort them (a, m, type, z).
        check(
            r#"calendar { z_last: 3, m_middle: 2, a_first: 1 }"#,
            expect![[r#"
                [{
                    a_first: 1,
                    m_middle: 2,
                    type: "calendar",
                    z_last: 3,
                }]"#]],
        );
    }
}
