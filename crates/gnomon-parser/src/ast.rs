mod generated;
mod support;

pub use generated::*;

#[cfg(test)]
mod tests {
    use crate::{SyntaxKind, ast::*, parse};
    use rowan::ast::AstNode;

    #[test]
    fn source_file_cast() {
        let p = parse("calendar {}");
        let file = SourceFile::cast(p.syntax()).unwrap();
        assert_eq!(file.body_exprs().count(), 1);
    }

    #[test]
    fn parse_tree_convenience() {
        let p = parse("calendar {}");
        let file = p.tree();
        assert_eq!(file.syntax().kind(), SyntaxKind::SOURCE_FILE);
    }

    #[test]
    fn calendar_expr_accessors() {
        let p = parse(r#"calendar { uid: "test" }"#);
        let file = p.tree();
        let expr = file.body_exprs().next().unwrap();
        match expr {
            Expr::CalendarExpr(cal) => {
                let body = cal.body().unwrap();
                assert_eq!(body.fields().count(), 1);
            }
            _ => panic!("expected CalendarExpr"),
        }
    }

    #[test]
    fn event_expr_short_form() {
        let p = parse(r#"event @meeting 2026-03-01T14:30 1h30m "Standup""#);
        let file = p.tree();
        let expr = file.body_exprs().next().unwrap();
        match expr {
            Expr::EventExpr(ev) => {
                assert_eq!(ev.name().unwrap().text(), "@meeting");
                assert!(ev.short_span().is_some());
                assert_eq!(ev.title().unwrap().text(), "\"Standup\"");
            }
            _ => panic!("expected EventExpr"),
        }
    }

    #[test]
    fn event_expr_prefix_form() {
        let p = parse("event { name: @meeting }");
        let file = p.tree();
        let expr = file.body_exprs().next().unwrap();
        match expr {
            Expr::EventExpr(ev) => {
                // NAME is inside the record, not a direct child of EventExpr
                assert!(ev.name().is_none());
                assert!(ev.body().is_some());
            }
            _ => panic!("expected EventExpr"),
        }
    }

    #[test]
    fn task_expr_with_datetime() {
        let p = parse(r#"task @deadline 2026-06-01T17:00 "Submit report""#);
        let file = p.tree();
        let expr = file.body_exprs().next().unwrap();
        match expr {
            Expr::TaskExpr(task) => {
                assert_eq!(task.name().unwrap().text(), "@deadline");
                let dt = task.short_dt().unwrap();
                assert!(dt.datetime().is_some());
                assert_eq!(task.title().unwrap().text(), "\"Submit report\"");
            }
            _ => panic!("expected TaskExpr"),
        }
    }

    #[test]
    fn record_field_count_and_names() {
        let p = parse(r#"calendar { uid: "test", name: "cal" }"#);
        let file = p.tree();
        let cal = match file.body_exprs().next().unwrap() {
            Expr::CalendarExpr(c) => c,
            _ => panic!("expected CalendarExpr"),
        };
        let body = cal.body().unwrap();
        let fields: Vec<_> = body.fields().collect();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name().unwrap().text(), "uid");
        assert_eq!(fields[1].name().unwrap().text(), "name");
    }

    #[test]
    fn field_value_literal() {
        let p = parse(r#"calendar { uid: "test" }"#);
        let file = p.tree();
        let cal = match file.body_exprs().next().unwrap() {
            Expr::CalendarExpr(c) => c,
            _ => panic!("expected CalendarExpr"),
        };
        let field = cal.body().unwrap().fields().next().unwrap();
        match field.value().unwrap() {
            Expr::LiteralExpr(lit) => {
                assert_eq!(lit.literal_token().unwrap().text(), "\"test\"");
            }
            _ => panic!("expected LiteralExpr"),
        }
    }

    #[test]
    fn list_expr_elements() {
        let p = parse("calendar { tags: [1, 2, 3] }");
        let file = p.tree();
        let cal = match file.body_exprs().next().unwrap() {
            Expr::CalendarExpr(c) => c,
            _ => panic!("expected CalendarExpr"),
        };
        let field = cal.body().unwrap().fields().next().unwrap();
        match field.value().unwrap() {
            Expr::ListExpr(list) => {
                assert_eq!(list.elements().count(), 3);
            }
            _ => panic!("expected ListExpr"),
        }
    }

    #[test]
    fn every_expr_day() {
        let p = parse("event { name: @daily, recurrence: every day }");
        let file = p.tree();
        let ev = match file.body_exprs().next().unwrap() {
            Expr::EventExpr(e) => e,
            _ => panic!("expected EventExpr"),
        };
        let rec_field = ev.body().unwrap().fields().nth(1).unwrap();
        match rec_field.value().unwrap() {
            Expr::EveryExpr(every) => {
                assert!(every.day_kw().is_some());
                assert!(every.weekday().is_none());
                assert!(every.until_kw().is_none());
            }
            _ => panic!("expected EveryExpr"),
        }
    }

    #[test]
    fn every_expr_weekday() {
        let p = parse("event { name: @weekly, recurrence: every monday }");
        let file = p.tree();
        let ev = match file.body_exprs().next().unwrap() {
            Expr::EventExpr(e) => e,
            _ => panic!("expected EventExpr"),
        };
        let rec_field = ev.body().unwrap().fields().nth(1).unwrap();
        match rec_field.value().unwrap() {
            Expr::EveryExpr(every) => {
                assert!(every.weekday().is_some());
                assert_eq!(every.weekday().unwrap().text(), "monday");
            }
            _ => panic!("expected EveryExpr"),
        }
    }

    #[test]
    fn every_expr_year_on() {
        let p = parse("event { name: @bday, recurrence: every year on 03-15 }");
        let file = p.tree();
        let ev = match file.body_exprs().next().unwrap() {
            Expr::EventExpr(e) => e,
            _ => panic!("expected EventExpr"),
        };
        let rec_field = ev.body().unwrap().fields().nth(1).unwrap();
        match rec_field.value().unwrap() {
            Expr::EveryExpr(every) => {
                assert!(every.year_kw().is_some());
                assert!(every.on_kw().is_some());
                assert_eq!(every.month_day().unwrap().text(), "03-15");
            }
            _ => panic!("expected EveryExpr"),
        }
    }

    #[test]
    fn every_expr_until_datetime() {
        let p = parse("event { name: @daily, recurrence: every day until 2026-12-31T23:59 }");
        let file = p.tree();
        let ev = match file.body_exprs().next().unwrap() {
            Expr::EventExpr(e) => e,
            _ => panic!("expected EventExpr"),
        };
        let rec_field = ev.body().unwrap().fields().nth(1).unwrap();
        match rec_field.value().unwrap() {
            Expr::EveryExpr(every) => {
                assert!(every.until_kw().is_some());
                assert_eq!(every.until_datetime().unwrap().text(), "2026-12-31T23:59");
            }
            _ => panic!("expected EveryExpr"),
        }
    }

    #[test]
    fn every_expr_until_date() {
        let p = parse("event { name: @daily, recurrence: every day until 2026-12-31 }");
        let file = p.tree();
        let ev = match file.body_exprs().next().unwrap() {
            Expr::EventExpr(e) => e,
            _ => panic!("expected EventExpr"),
        };
        let rec_field = ev.body().unwrap().fields().nth(1).unwrap();
        match rec_field.value().unwrap() {
            Expr::EveryExpr(every) => {
                assert!(every.until_kw().is_some());
                assert_eq!(every.until_date().unwrap().text(), "2026-12-31");
                assert!(every.until_datetime().is_none());
            }
            _ => panic!("expected EveryExpr"),
        }
    }

    #[test]
    fn every_expr_until_count() {
        let p = parse("event { name: @limited, recurrence: every day until 10 times }");
        let file = p.tree();
        let ev = match file.body_exprs().next().unwrap() {
            Expr::EventExpr(e) => e,
            _ => panic!("expected EventExpr"),
        };
        let rec_field = ev.body().unwrap().fields().nth(1).unwrap();
        match rec_field.value().unwrap() {
            Expr::EveryExpr(every) => {
                assert!(every.until_kw().is_some());
                assert_eq!(every.until_count().unwrap().text(), "10");
                assert!(every.times_kw().is_some());
            }
            _ => panic!("expected EveryExpr"),
        }
    }

    #[test]
    fn short_span_accessors() {
        let p = parse(r#"event @meeting 2026-03-01T14:30 1h30m "Standup""#);
        let file = p.tree();
        let ev = match file.body_exprs().next().unwrap() {
            Expr::EventExpr(e) => e,
            _ => panic!("expected EventExpr"),
        };
        let span = ev.short_span().unwrap();
        assert!(span.start().is_some());
        assert_eq!(span.duration().unwrap().text(), "1h30m");
    }

    #[test]
    fn short_dt_date_time() {
        let p = parse("event @lunch 2026-03-01 12:00 1h");
        let file = p.tree();
        let ev = match file.body_exprs().next().unwrap() {
            Expr::EventExpr(e) => e,
            _ => panic!("expected EventExpr"),
        };
        let span = ev.short_span().unwrap();
        let dt = span.start().unwrap();
        assert!(dt.datetime().is_none());
        assert_eq!(dt.date().unwrap().text(), "2026-03-01");
        assert_eq!(dt.time().unwrap().text(), "12:00");
    }

    #[test]
    fn multiple_exprs_enum_dispatch() {
        let p = parse(
            r#"calendar { uid: "cal" }
event @meeting 2026-03-01T14:30 1h "Standup"
task @cleanup "Clean""#,
        );
        let file = p.tree();
        let exprs: Vec<_> = file.body_exprs().collect();
        assert_eq!(exprs.len(), 3);
        assert!(matches!(exprs[0], Expr::CalendarExpr(_)));
        assert!(matches!(exprs[1], Expr::EventExpr(_)));
        assert!(matches!(exprs[2], Expr::TaskExpr(_)));
    }
}
