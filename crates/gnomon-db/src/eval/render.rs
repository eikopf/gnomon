use std::fmt;

use crate::Db;

use super::interned::{DeclId, DeclKind, FieldName, FieldPath, PathSegment};
use super::types::{Blame, Blamed, Document, IncludeRef, Record, ReifiedDecl, Value};

/// Format a value using the salsa database for name resolution.
///
/// Salsa-interned types like [`FieldName`] and [`DeclId`] require database
/// access to resolve to human-readable text. This trait threads the `&dyn Db`
/// through the formatting pipeline.
pub trait RenderWithDb<'db> {
    fn render_fmt(&self, f: &mut fmt::Formatter<'_>, db: &'db dyn Db) -> fmt::Result;

    /// Return a thin wrapper that implements [`Display`](fmt::Display).
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

fn write_record<'db>(
    w: &mut dyn fmt::Write,
    record: &Record<'db>,
    db: &'db dyn Db,
    indent: usize,
) -> fmt::Result {
    let mut entries: Vec<_> = record.0.iter().collect();
    entries.sort_by(|(a, _), (b, _)| a.text(db).cmp(b.text(db)));

    if entries.is_empty() {
        return write!(w, "{{}}");
    }

    writeln!(w, "{{")?;
    for (name, blamed) in &entries {
        write_indent(w, indent + 4)?;
        write!(w, "{}: ", name.text(db))?;
        write_value(w, &blamed.value, db, indent + 4)?;
        writeln!(w, ",")?;
    }
    write_indent(w, indent)?;
    write!(w, "}}")
}

fn write_reified_decl<'db>(
    w: &mut dyn fmt::Write,
    decl: &ReifiedDecl<'db>,
    db: &'db dyn Db,
    indent: usize,
) -> fmt::Result {
    match decl {
        ReifiedDecl::Include { target, content } => {
            writeln!(w, "Include {{")?;
            write_indent(w, indent + 4)?;
            writeln!(w, "target: {},", target.render(db))?;
            if !content.is_empty() {
                write_indent(w, indent + 4)?;
                writeln!(w, "content: [")?;
                for item in content {
                    write_indent(w, indent + 8)?;
                    write_record(w, &item.value, db, indent + 8)?;
                    writeln!(w, ",")?;
                }
                write_indent(w, indent + 4)?;
                writeln!(w, "],")?;
            }
            write_indent(w, indent)?;
            write!(w, "}}")
        }
        ReifiedDecl::Calendar(r) => {
            write!(w, "Calendar ")?;
            write_record(w, r, db, indent)
        }
        ReifiedDecl::Event(r) => {
            write!(w, "Event ")?;
            write_record(w, r, db, indent)
        }
        ReifiedDecl::Task(r) => {
            write!(w, "Task ")?;
            write_record(w, r, db, indent)
        }
        ReifiedDecl::Group(r) => {
            write!(w, "Group ")?;
            write_record(w, r, db, indent)
        }
    }
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
        for (i, segment) in self.0.iter().enumerate() {
            if i > 0 {
                if matches!(segment, PathSegment::Field(_)) {
                    write!(f, ".")?;
                }
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

impl<'db> RenderWithDb<'db> for IncludeRef {
    fn render_fmt(&self, f: &mut fmt::Formatter<'_>, _db: &'db dyn Db) -> fmt::Result {
        match self {
            IncludeRef::Path(p) => write!(f, "\"{}\"", p.display()),
            IncludeRef::Uri(u) => write!(f, "{u:?}"),
        }
    }
}

impl<'db> RenderWithDb<'db> for ReifiedDecl<'db> {
    fn render_fmt(&self, f: &mut fmt::Formatter<'_>, db: &'db dyn Db) -> fmt::Result {
        write_reified_decl(f, self, db, 0)
    }
}

impl<'db> RenderWithDb<'db> for String {
    fn render_fmt(&self, f: &mut fmt::Formatter<'_>, _db: &'db dyn Db) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl<'db> RenderWithDb<'db> for Document<'db> {
    fn render_fmt(&self, f: &mut fmt::Formatter<'_>, db: &'db dyn Db) -> fmt::Result {
        writeln!(f, "Document {{")?;

        // Bindings
        write!(f, "    bindings: ")?;
        if self.bindings.is_empty() {
            writeln!(f, "{{}},")?;
        } else {
            writeln!(f, "{{")?;
            for (name, blamed_uid) in &self.bindings {
                writeln!(f, "        {name}: {:?},", blamed_uid.value)?;
            }
            writeln!(f, "    }},")?;
        }

        // Decls
        write!(f, "    decls: ")?;
        if self.decls.is_empty() {
            writeln!(f, "[],")?;
        } else {
            writeln!(f, "[")?;
            for blamed_decl in &self.decls {
                write!(f, "        ")?;
                write_reified_decl(f, &blamed_decl.value, db, 8)?;
                writeln!(f, ",")?;
            }
            writeln!(f, "    ],")?;
        }

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
        let rendered = format!("{}", result.document.render(&db));
        expected.assert_eq(&rendered);
    }

    #[test]
    fn empty_calendar() {
        check(
            "calendar {}",
            expect![[r#"
                Document {
                    bindings: {},
                    decls: [
                        Calendar {},
                    ],
                }"#]],
        );
    }

    #[test]
    fn calendar_with_fields() {
        check(
            r#"calendar { uid: "test-cal", name: "My Cal" }"#,
            expect![[r#"
                Document {
                    bindings: {},
                    decls: [
                        Calendar {
                            name: "My Cal",
                            uid: "test-cal",
                        },
                    ],
                }"#]],
        );
    }

    #[test]
    fn event_short_form() {
        check(
            r#"event @meeting 2026-03-01T14:30 1h30m "Standup""#,
            expect![[r#"
                Document {
                    bindings: {},
                    decls: [
                        Event {
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
                        },
                    ],
                }"#]],
        );
    }

    #[test]
    fn binding() {
        check(
            r#"bind @cal.holidays "holidays-uid""#,
            expect![[r#"
                Document {
                    bindings: {
                        cal.holidays: "holidays-uid",
                    },
                    decls: [],
                }"#]],
        );
    }

    #[test]
    fn list_values() {
        check(
            r#"calendar { keywords: ["work", "meeting"] }"#,
            expect![[r#"
                Document {
                    bindings: {},
                    decls: [
                        Calendar {
                            keywords: ["work", "meeting"],
                        },
                    ],
                }"#]],
        );
    }

    #[test]
    fn include_path() {
        check(
            r#"include "holidays.ics""#,
            expect![[r#"
                Document {
                    bindings: {},
                    decls: [
                        Include {
                            target: "holidays.ics",
                        },
                    ],
                }"#]],
        );
    }

    #[test]
    fn task_short_form() {
        check(
            r#"task @review 2026-03-15T17:00 "Code review""#,
            expect![[r#"
                Document {
                    bindings: {},
                    decls: [
                        Task {
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
                        },
                    ],
                }"#]],
        );
    }

    #[test]
    fn mixed_declarations() {
        check(
            r#"
            event @a 2026-01-01T09:00 1h "A"
            calendar { uid: "cal" }
            task @b "B"
            "#,
            expect![[r#"
                Document {
                    bindings: {},
                    decls: [
                        Event {
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
                        Calendar {
                            uid: "cal",
                        },
                        Task {
                            name: @b,
                            title: "B",
                        },
                    ],
                }"#]],
        );
    }

    #[test]
    fn boolean_and_integer_values() {
        check(
            "calendar { show_without_time: true, priority: 5 }",
            expect![[r#"
                Document {
                    bindings: {},
                    decls: [
                        Calendar {
                            priority: 5,
                            show_without_time: true,
                        },
                    ],
                }"#]],
        );
    }
}
