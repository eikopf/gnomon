pub mod ast;
mod lexer;
mod parser;
mod preprocess;
mod syntax_kind;
pub mod validate;

pub use parser::ParseError;
pub use rowan::ast::AstNode;
pub use syntax_kind::{GnomonLanguage, SyntaxKind, SyntaxNode, SyntaxToken};
pub use validate::{SyntaxError, validate_syntax};

/// Result of parsing a Gnomon source string.
pub struct Parse {
    green_node: rowan::GreenNode,
    errors: Vec<ParseError>,
}

impl Parse {
    /// Get the root syntax node of the parse tree.
    pub fn syntax(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green_node.clone())
    }

    /// Get the parse errors, if any.
    pub fn errors(&self) -> &[ParseError] {
        &self.errors
    }

    /// Returns `true` if parsing produced no errors.
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }

    /// Debug-print the tree structure.
    pub fn debug_tree(&self) -> String {
        let syntax = self.syntax();
        format!("{syntax:#?}")
    }

    /// Get the green (immutable, interned) tree.
    pub fn green_node(&self) -> &rowan::GreenNode {
        &self.green_node
    }

    /// Get the typed AST root.
    pub fn tree(&self) -> ast::SourceFile {
        ast::SourceFile::cast(self.syntax()).unwrap()
    }
}

/// Parse a Gnomon source string, returning a lossless concrete syntax tree.
///
/// The input is preprocessed (BOM removal, CRLF normalization, shebang removal)
/// before lexing.
pub fn parse(source: &str) -> Parse {
    let preprocessed = preprocess::preprocess(source);
    let tokens = lexer::lex(&preprocessed);
    let parser = parser::Parser::new(tokens);
    let (green_node, errors) = parser.parse();
    Parse { green_node, errors }
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::{Expect, expect};

    fn check(input: &str, expected_tree: Expect) {
        let parse = parse(input);
        let actual = parse.debug_tree();
        expected_tree.assert_eq(&actual);
    }

    fn check_no_errors(input: &str) {
        let parse = parse(input);
        assert!(parse.ok(), "expected no errors, got: {:?}", parse.errors());
    }

    // ── Round-trip (lossless) ────────────────────────────────────

    #[test]
    fn round_trip_lossless() {
        let source = r#"
; A calendar
calendar {
    uid: "my-cal",
}

event @meeting 2026-03-01T14:30 1h30m "Standup" {
    description: "Daily standup meeting",
}

task @cleanup "Clean up" {
    priority: 1,
}
"#;
        let preprocessed = preprocess::preprocess(source);
        let parse = parse(source);
        let text = parse.syntax().text().to_string();
        assert_eq!(text, preprocessed);
    }

    // ── Inclusion ────────────────────────────────────────────────

    // r[verify decl.syntax]
    #[test]
    fn parse_inclusion() {
        check(
            r#"include "holidays.gnomon""#,
            expect![[r#"
                SOURCE_FILE@0..25
                  INCLUSION_DECL@0..25
                    INCLUDE_KW@0..7 "include"
                    WHITESPACE@7..8 " "
                    STRING_LITERAL@8..25 "\"holidays.gnomon\""
            "#]],
        );
    }

    // r[verify decl.syntax]
    #[test]
    fn parse_inclusion_no_errors() {
        check_no_errors(r#"include "holidays.gnomon""#);
    }

    // ── Binding ──────────────────────────────────────────────────

    // r[verify decl.syntax]
    #[test]
    fn parse_binding() {
        check(
            r#"bind @cal.holidays "holidays.gnomon""#,
            expect![[r#"
                SOURCE_FILE@0..36
                  BINDING_DECL@0..36
                    BIND_KW@0..4 "bind"
                    WHITESPACE@4..5 " "
                    NAME@5..18 "@cal.holidays"
                    WHITESPACE@18..19 " "
                    STRING_LITERAL@19..36 "\"holidays.gnomon\""
            "#]],
        );
    }

    // r[verify decl.syntax]
    #[test]
    fn parse_binding_no_errors() {
        check_no_errors(r#"bind @cal.holidays "holidays.gnomon""#);
    }

    // ── Calendar ─────────────────────────────────────────────────

    // r[verify decl.syntax]
    #[test]
    fn parse_calendar() {
        check(
            r#"calendar { uid: "my-cal" }"#,
            expect![[r#"
                SOURCE_FILE@0..26
                  CALENDAR_DECL@0..26
                    CALENDAR_KW@0..8 "calendar"
                    WHITESPACE@8..9 " "
                    RECORD_EXPR@9..26
                      L_BRACE@9..10 "{"
                      WHITESPACE@10..11 " "
                      FIELD@11..24
                        IDENT@11..14 "uid"
                        COLON@14..15 ":"
                        WHITESPACE@15..16 " "
                        LITERAL_EXPR@16..24
                          STRING_LITERAL@16..24 "\"my-cal\""
                      WHITESPACE@24..25 " "
                      R_BRACE@25..26 "}"
            "#]],
        );
    }

    // r[verify decl.syntax]
    #[test]
    fn parse_calendar_no_errors() {
        check_no_errors(r#"calendar { uid: "my-cal" }"#);
    }

    // ── Event (short form) ───────────────────────────────────────

    // r[verify decl.syntax]
    #[test]
    fn parse_event_short_datetime() {
        check(
            r#"event @meeting 2026-03-01T14:30 1h30m "Standup""#,
            expect![[r#"
                SOURCE_FILE@0..47
                  EVENT_DECL@0..47
                    EVENT_KW@0..5 "event"
                    WHITESPACE@5..6 " "
                    NAME@6..14 "@meeting"
                    WHITESPACE@14..15 " "
                    SHORT_SPAN@15..37
                      SHORT_DT@15..31
                        DATETIME_LITERAL@15..31 "2026-03-01T14:30"
                      WHITESPACE@31..32 " "
                      DURATION_LITERAL@32..37 "1h30m"
                    WHITESPACE@37..38 " "
                    STRING_LITERAL@38..47 "\"Standup\""
            "#]],
        );
    }

    // r[verify decl.syntax]
    #[test]
    fn parse_event_short_no_errors() {
        check_no_errors(r#"event @meeting 2026-03-01T14:30 1h30m "Standup""#);
    }

    // r[verify decl.syntax]
    #[test]
    fn parse_event_short_date_time() {
        check(
            "event @lunch 2026-03-01 12:00 1h",
            expect![[r#"
                SOURCE_FILE@0..32
                  EVENT_DECL@0..32
                    EVENT_KW@0..5 "event"
                    WHITESPACE@5..6 " "
                    NAME@6..12 "@lunch"
                    WHITESPACE@12..13 " "
                    SHORT_SPAN@13..32
                      SHORT_DT@13..29
                        DATE_LITERAL@13..23 "2026-03-01"
                        WHITESPACE@23..24 " "
                        TIME_LITERAL@24..29 "12:00"
                      WHITESPACE@29..30 " "
                      DURATION_LITERAL@30..32 "1h"
            "#]],
        );
    }

    // ── Event (prefix form) ──────────────────────────────────────

    // r[verify decl.syntax]
    #[test]
    fn parse_event_prefix() {
        check(
            r#"event { name: @meeting, start: 2026-03-01T14:30 }"#,
            expect![[r#"
                SOURCE_FILE@0..49
                  EVENT_DECL@0..49
                    EVENT_KW@0..5 "event"
                    WHITESPACE@5..6 " "
                    RECORD_EXPR@6..49
                      L_BRACE@6..7 "{"
                      WHITESPACE@7..8 " "
                      FIELD@8..22
                        IDENT@8..12 "name"
                        COLON@12..13 ":"
                        WHITESPACE@13..14 " "
                        LITERAL_EXPR@14..22
                          NAME@14..22 "@meeting"
                      COMMA@22..23 ","
                      WHITESPACE@23..24 " "
                      FIELD@24..47
                        IDENT@24..29 "start"
                        COLON@29..30 ":"
                        WHITESPACE@30..31 " "
                        LITERAL_EXPR@31..47
                          DATETIME_LITERAL@31..47 "2026-03-01T14:30"
                      WHITESPACE@47..48 " "
                      R_BRACE@48..49 "}"
            "#]],
        );
    }

    // ── Task (short form) ────────────────────────────────────────

    // r[verify decl.syntax]
    #[test]
    fn parse_task_short() {
        check(
            r#"task @cleanup "Clean up""#,
            expect![[r#"
                SOURCE_FILE@0..24
                  TASK_DECL@0..24
                    TASK_KW@0..4 "task"
                    WHITESPACE@4..5 " "
                    NAME@5..13 "@cleanup"
                    WHITESPACE@13..14 " "
                    STRING_LITERAL@14..24 "\"Clean up\""
            "#]],
        );
    }

    // r[verify decl.syntax]
    #[test]
    fn parse_task_short_no_errors() {
        check_no_errors(r#"task @cleanup "Clean up""#);
    }

    // ── Task (prefix form) ───────────────────────────────────────

    // r[verify decl.syntax]
    #[test]
    fn parse_task_prefix() {
        check(
            r#"task { name: @cleanup }"#,
            expect![[r#"
                SOURCE_FILE@0..23
                  TASK_DECL@0..23
                    TASK_KW@0..4 "task"
                    WHITESPACE@4..5 " "
                    RECORD_EXPR@5..23
                      L_BRACE@5..6 "{"
                      WHITESPACE@6..7 " "
                      FIELD@7..21
                        IDENT@7..11 "name"
                        COLON@11..12 ":"
                        WHITESPACE@12..13 " "
                        LITERAL_EXPR@13..21
                          NAME@13..21 "@cleanup"
                      WHITESPACE@21..22 " "
                      R_BRACE@22..23 "}"
            "#]],
        );
    }

    // ── Record with nested record ────────────────────────────────

    // r[verify expr.record.syntax]
    #[test]
    fn parse_nested_record() {
        check(
            r#"calendar { uid: "test", description: { type: "text/html", content: "hello" } }"#,
            expect![[r#"
                SOURCE_FILE@0..78
                  CALENDAR_DECL@0..78
                    CALENDAR_KW@0..8 "calendar"
                    WHITESPACE@8..9 " "
                    RECORD_EXPR@9..78
                      L_BRACE@9..10 "{"
                      WHITESPACE@10..11 " "
                      FIELD@11..22
                        IDENT@11..14 "uid"
                        COLON@14..15 ":"
                        WHITESPACE@15..16 " "
                        LITERAL_EXPR@16..22
                          STRING_LITERAL@16..22 "\"test\""
                      COMMA@22..23 ","
                      WHITESPACE@23..24 " "
                      FIELD@24..76
                        IDENT@24..35 "description"
                        COLON@35..36 ":"
                        WHITESPACE@36..37 " "
                        RECORD_EXPR@37..76
                          L_BRACE@37..38 "{"
                          WHITESPACE@38..39 " "
                          FIELD@39..56
                            IDENT@39..43 "type"
                            COLON@43..44 ":"
                            WHITESPACE@44..45 " "
                            LITERAL_EXPR@45..56
                              STRING_LITERAL@45..56 "\"text/html\""
                          COMMA@56..57 ","
                          WHITESPACE@57..58 " "
                          FIELD@58..74
                            IDENT@58..65 "content"
                            COLON@65..66 ":"
                            WHITESPACE@66..67 " "
                            LITERAL_EXPR@67..74
                              STRING_LITERAL@67..74 "\"hello\""
                          WHITESPACE@74..75 " "
                          R_BRACE@75..76 "}"
                      WHITESPACE@76..77 " "
                      R_BRACE@77..78 "}"
            "#]],
        );
    }

    // ── List expression ──────────────────────────────────────────

    // r[verify expr.list.syntax]
    #[test]
    fn parse_list_in_field() {
        check(
            "calendar { tags: [1, 2, 3] }",
            expect![[r#"
                SOURCE_FILE@0..28
                  CALENDAR_DECL@0..28
                    CALENDAR_KW@0..8 "calendar"
                    WHITESPACE@8..9 " "
                    RECORD_EXPR@9..28
                      L_BRACE@9..10 "{"
                      WHITESPACE@10..11 " "
                      FIELD@11..26
                        IDENT@11..15 "tags"
                        COLON@15..16 ":"
                        WHITESPACE@16..17 " "
                        LIST_EXPR@17..26
                          L_BRACKET@17..18 "["
                          LITERAL_EXPR@18..19
                            INTEGER_LITERAL@18..19 "1"
                          COMMA@19..20 ","
                          WHITESPACE@20..21 " "
                          LITERAL_EXPR@21..22
                            INTEGER_LITERAL@21..22 "2"
                          COMMA@22..23 ","
                          WHITESPACE@23..24 " "
                          LITERAL_EXPR@24..25
                            INTEGER_LITERAL@24..25 "3"
                          R_BRACKET@25..26 "]"
                      WHITESPACE@26..27 " "
                      R_BRACE@27..28 "}"
            "#]],
        );
    }

    // ── Every expression ─────────────────────────────────────────

    // r[verify record.rrule.every+2]
    #[test]
    fn parse_every_day() {
        check(
            r#"event { name: @daily, recurrence: every day }"#,
            expect![[r#"
                SOURCE_FILE@0..45
                  EVENT_DECL@0..45
                    EVENT_KW@0..5 "event"
                    WHITESPACE@5..6 " "
                    RECORD_EXPR@6..45
                      L_BRACE@6..7 "{"
                      WHITESPACE@7..8 " "
                      FIELD@8..20
                        IDENT@8..12 "name"
                        COLON@12..13 ":"
                        WHITESPACE@13..14 " "
                        LITERAL_EXPR@14..20
                          NAME@14..20 "@daily"
                      COMMA@20..21 ","
                      WHITESPACE@21..22 " "
                      FIELD@22..43
                        IDENT@22..32 "recurrence"
                        COLON@32..33 ":"
                        WHITESPACE@33..34 " "
                        EVERY_EXPR@34..43
                          EVERY_KW@34..39 "every"
                          WHITESPACE@39..40 " "
                          DAY_KW@40..43 "day"
                      WHITESPACE@43..44 " "
                      R_BRACE@44..45 "}"
            "#]],
        );
    }

    // r[verify record.rrule.every+2]
    // r[verify record.rrule.weekday]
    #[test]
    fn parse_every_weekday() {
        check(
            "event { name: @weekly, recurrence: every monday }",
            expect![[r#"
                SOURCE_FILE@0..49
                  EVENT_DECL@0..49
                    EVENT_KW@0..5 "event"
                    WHITESPACE@5..6 " "
                    RECORD_EXPR@6..49
                      L_BRACE@6..7 "{"
                      WHITESPACE@7..8 " "
                      FIELD@8..21
                        IDENT@8..12 "name"
                        COLON@12..13 ":"
                        WHITESPACE@13..14 " "
                        LITERAL_EXPR@14..21
                          NAME@14..21 "@weekly"
                      COMMA@21..22 ","
                      WHITESPACE@22..23 " "
                      FIELD@23..47
                        IDENT@23..33 "recurrence"
                        COLON@33..34 ":"
                        WHITESPACE@34..35 " "
                        EVERY_EXPR@35..47
                          EVERY_KW@35..40 "every"
                          WHITESPACE@40..41 " "
                          MONDAY_KW@41..47 "monday"
                      WHITESPACE@47..48 " "
                      R_BRACE@48..49 "}"
            "#]],
        );
    }

    // r[verify record.rrule.every+2]
    #[test]
    fn parse_every_year_on() {
        check(
            "event { name: @birthday, recurrence: every year on 03-15 }",
            expect![[r#"
                SOURCE_FILE@0..58
                  EVENT_DECL@0..58
                    EVENT_KW@0..5 "event"
                    WHITESPACE@5..6 " "
                    RECORD_EXPR@6..58
                      L_BRACE@6..7 "{"
                      WHITESPACE@7..8 " "
                      FIELD@8..23
                        IDENT@8..12 "name"
                        COLON@12..13 ":"
                        WHITESPACE@13..14 " "
                        LITERAL_EXPR@14..23
                          NAME@14..23 "@birthday"
                      COMMA@23..24 ","
                      WHITESPACE@24..25 " "
                      FIELD@25..56
                        IDENT@25..35 "recurrence"
                        COLON@35..36 ":"
                        WHITESPACE@36..37 " "
                        EVERY_EXPR@37..56
                          EVERY_KW@37..42 "every"
                          WHITESPACE@42..43 " "
                          YEAR_KW@43..47 "year"
                          WHITESPACE@47..48 " "
                          ON_KW@48..50 "on"
                          WHITESPACE@50..51 " "
                          MONTH_DAY_LITERAL@51..56 "03-15"
                      WHITESPACE@56..57 " "
                      R_BRACE@57..58 "}"
            "#]],
        );
    }

    // r[verify record.rrule.every+2]
    #[test]
    fn parse_every_with_until() {
        check(
            "event { name: @daily, recurrence: every day until 2026-12-31T23:59 }",
            expect![[r#"
                SOURCE_FILE@0..68
                  EVENT_DECL@0..68
                    EVENT_KW@0..5 "event"
                    WHITESPACE@5..6 " "
                    RECORD_EXPR@6..68
                      L_BRACE@6..7 "{"
                      WHITESPACE@7..8 " "
                      FIELD@8..20
                        IDENT@8..12 "name"
                        COLON@12..13 ":"
                        WHITESPACE@13..14 " "
                        LITERAL_EXPR@14..20
                          NAME@14..20 "@daily"
                      COMMA@20..21 ","
                      WHITESPACE@21..22 " "
                      FIELD@22..66
                        IDENT@22..32 "recurrence"
                        COLON@32..33 ":"
                        WHITESPACE@33..34 " "
                        EVERY_EXPR@34..66
                          EVERY_KW@34..39 "every"
                          WHITESPACE@39..40 " "
                          DAY_KW@40..43 "day"
                          WHITESPACE@43..44 " "
                          UNTIL_KW@44..49 "until"
                          WHITESPACE@49..50 " "
                          DATETIME_LITERAL@50..66 "2026-12-31T23:59"
                      WHITESPACE@66..67 " "
                      R_BRACE@67..68 "}"
            "#]],
        );
    }

    // r[verify record.rrule.every+2]
    #[test]
    fn parse_every_n_times() {
        check(
            "event { name: @limited, recurrence: every day until 10 times }",
            expect![[r#"
                SOURCE_FILE@0..62
                  EVENT_DECL@0..62
                    EVENT_KW@0..5 "event"
                    WHITESPACE@5..6 " "
                    RECORD_EXPR@6..62
                      L_BRACE@6..7 "{"
                      WHITESPACE@7..8 " "
                      FIELD@8..22
                        IDENT@8..12 "name"
                        COLON@12..13 ":"
                        WHITESPACE@13..14 " "
                        LITERAL_EXPR@14..22
                          NAME@14..22 "@limited"
                      COMMA@22..23 ","
                      WHITESPACE@23..24 " "
                      FIELD@24..60
                        IDENT@24..34 "recurrence"
                        COLON@34..35 ":"
                        WHITESPACE@35..36 " "
                        EVERY_EXPR@36..60
                          EVERY_KW@36..41 "every"
                          WHITESPACE@41..42 " "
                          DAY_KW@42..45 "day"
                          WHITESPACE@45..46 " "
                          UNTIL_KW@46..51 "until"
                          WHITESPACE@51..52 " "
                          INTEGER_LITERAL@52..54 "10"
                          WHITESPACE@54..55 " "
                          TIMES_KW@55..60 "times"
                      WHITESPACE@60..61 " "
                      R_BRACE@61..62 "}"
            "#]],
        );
    }

    // ── Every with date literal terminator ─────────────────────

    // r[verify record.rrule.every+2]
    #[test]
    fn parse_every_until_date() {
        check(
            "event { name: @daily, recurrence: every day until 2026-12-31 }",
            expect![[r#"
                SOURCE_FILE@0..62
                  EVENT_DECL@0..62
                    EVENT_KW@0..5 "event"
                    WHITESPACE@5..6 " "
                    RECORD_EXPR@6..62
                      L_BRACE@6..7 "{"
                      WHITESPACE@7..8 " "
                      FIELD@8..20
                        IDENT@8..12 "name"
                        COLON@12..13 ":"
                        WHITESPACE@13..14 " "
                        LITERAL_EXPR@14..20
                          NAME@14..20 "@daily"
                      COMMA@20..21 ","
                      WHITESPACE@21..22 " "
                      FIELD@22..60
                        IDENT@22..32 "recurrence"
                        COLON@32..33 ":"
                        WHITESPACE@33..34 " "
                        EVERY_EXPR@34..60
                          EVERY_KW@34..39 "every"
                          WHITESPACE@39..40 " "
                          DAY_KW@40..43 "day"
                          WHITESPACE@43..44 " "
                          UNTIL_KW@44..49 "until"
                          WHITESPACE@49..50 " "
                          DATE_LITERAL@50..60 "2026-12-31"
                      WHITESPACE@60..61 " "
                      R_BRACE@61..62 "}"
            "#]],
        );
    }

    // r[verify record.rrule.every+2]
    #[test]
    fn parse_every_until_date_no_errors() {
        check_no_errors("event { name: @daily, recurrence: every day until 2026-12-31 }");
    }

    // ── Comments preserved ───────────────────────────────────────

    // r[verify lexer.comment]
    #[test]
    fn parse_with_comments() {
        check(
            "; A simple calendar\ncalendar { uid: \"test\" }",
            expect![[r#"
                SOURCE_FILE@0..44
                  COMMENT@0..19 "; A simple calendar"
                  WHITESPACE@19..20 "\n"
                  CALENDAR_DECL@20..44
                    CALENDAR_KW@20..28 "calendar"
                    WHITESPACE@28..29 " "
                    RECORD_EXPR@29..44
                      L_BRACE@29..30 "{"
                      WHITESPACE@30..31 " "
                      FIELD@31..42
                        IDENT@31..34 "uid"
                        COLON@34..35 ":"
                        WHITESPACE@35..36 " "
                        LITERAL_EXPR@36..42
                          STRING_LITERAL@36..42 "\"test\""
                      WHITESPACE@42..43 " "
                      R_BRACE@43..44 "}"
            "#]],
        );
    }

    // ── Error recovery ───────────────────────────────────────────

    #[test]
    fn error_recovery_bad_decl() {
        let parse = parse("~~~ calendar { uid: \"test\" }");
        assert!(!parse.ok());
        // Despite errors, the calendar declaration should still be parsed
        let tree = parse.debug_tree();
        assert!(tree.contains("ERROR_NODE"));
        assert!(tree.contains("CALENDAR_DECL"));
    }

    #[test]
    fn error_recovery_preserves_lossless() {
        let source = "~~~ calendar { uid: \"test\" }";
        let preprocessed = preprocess::preprocess(source);
        let parse = parse(source);
        let text = parse.syntax().text().to_string();
        assert_eq!(text, preprocessed);
    }

    // ── Multiple declarations ────────────────────────────────────

    // r[verify syntax.start]
    #[test]
    fn parse_multiple_decls() {
        let source = r#"include "base.gnomon"
calendar { uid: "cal" }
event @meeting 2026-03-01T14:30 1h "Standup"
task @cleanup "Clean""#;
        check_no_errors(source);
    }

    // ── Trailing comma in record ─────────────────────────────────

    // r[verify expr.record.syntax]
    #[test]
    fn parse_trailing_comma() {
        check_no_errors("calendar { uid: \"test\", }");
    }

    // ── Empty record ─────────────────────────────────────────────

    // r[verify expr.record.syntax]
    #[test]
    fn parse_empty_record() {
        check(
            "calendar {}",
            expect![[r#"
                SOURCE_FILE@0..11
                  CALENDAR_DECL@0..11
                    CALENDAR_KW@0..8 "calendar"
                    WHITESPACE@8..9 " "
                    RECORD_EXPR@9..11
                      L_BRACE@9..10 "{"
                      R_BRACE@10..11 "}"
            "#]],
        );
    }

    // ── Event with short form + record ───────────────────────────

    // r[verify decl.syntax]
    #[test]
    fn parse_event_short_with_record() {
        check_no_errors(
            r#"event @meeting 2026-03-01T14:30 1h30m "Standup" { description: "Daily standup" }"#,
        );
    }

    // ── Task with short_dt ───────────────────────────────────────

    // r[verify decl.syntax]
    #[test]
    fn parse_task_with_datetime() {
        check(
            r#"task @deadline 2026-06-01T17:00 "Submit report""#,
            expect![[r#"
                SOURCE_FILE@0..47
                  TASK_DECL@0..47
                    TASK_KW@0..4 "task"
                    WHITESPACE@4..5 " "
                    NAME@5..14 "@deadline"
                    WHITESPACE@14..15 " "
                    SHORT_DT@15..31
                      DATETIME_LITERAL@15..31 "2026-06-01T17:00"
                    WHITESPACE@31..32 " "
                    STRING_LITERAL@32..47 "\"Submit report\""
            "#]],
        );
    }

    // r[verify decl.syntax]
    #[test]
    fn parse_task_with_datetime_no_errors() {
        check_no_errors(r#"task @deadline 2026-06-01T17:00 "Submit report""#);
    }

    // ── Boolean and undefined literals ───────────────────────────

    // r[verify expr.literal.syntax+2]
    // r[verify lexer.keyword.strict]
    #[test]
    fn parse_boolean_fields() {
        check_no_errors("calendar { active: true, archived: false }");
    }

    // ── URI literal ──────────────────────────────────────────────

    // r[verify lexer.uri]
    #[test]
    fn parse_uri_in_field() {
        check(
            "event { name: @meeting, url: <https://meet.example.com/abc> }",
            expect![[r#"
                SOURCE_FILE@0..61
                  EVENT_DECL@0..61
                    EVENT_KW@0..5 "event"
                    WHITESPACE@5..6 " "
                    RECORD_EXPR@6..61
                      L_BRACE@6..7 "{"
                      WHITESPACE@7..8 " "
                      FIELD@8..22
                        IDENT@8..12 "name"
                        COLON@12..13 ":"
                        WHITESPACE@13..14 " "
                        LITERAL_EXPR@14..22
                          NAME@14..22 "@meeting"
                      COMMA@22..23 ","
                      WHITESPACE@23..24 " "
                      FIELD@24..59
                        IDENT@24..27 "url"
                        COLON@27..28 ":"
                        WHITESPACE@28..29 " "
                        LITERAL_EXPR@29..59
                          URI_LITERAL@29..59 "<https://meet.example ..."
                      WHITESPACE@59..60 " "
                      R_BRACE@60..61 "}"
            "#]],
        );
    }

    // r[verify lexer.uri]
    #[test]
    fn parse_uri_in_field_no_errors() {
        check_no_errors("event { name: @meeting, url: <https://meet.example.com/abc> }");
    }

    // r[verify lexer.uri]
    #[test]
    fn parse_uri_in_list() {
        check_no_errors("calendar { links: [<https://a.com>, <https://b.com>] }");
    }

    // ── Atom literal ─────────────────────────────────────────────

    // r[verify lexer.atom]
    #[test]
    fn parse_atom_in_field() {
        check(
            "event { name: @meeting, status: #confirmed }",
            expect![[r##"
                SOURCE_FILE@0..44
                  EVENT_DECL@0..44
                    EVENT_KW@0..5 "event"
                    WHITESPACE@5..6 " "
                    RECORD_EXPR@6..44
                      L_BRACE@6..7 "{"
                      WHITESPACE@7..8 " "
                      FIELD@8..22
                        IDENT@8..12 "name"
                        COLON@12..13 ":"
                        WHITESPACE@13..14 " "
                        LITERAL_EXPR@14..22
                          NAME@14..22 "@meeting"
                      COMMA@22..23 ","
                      WHITESPACE@23..24 " "
                      FIELD@24..42
                        IDENT@24..30 "status"
                        COLON@30..31 ":"
                        WHITESPACE@31..32 " "
                        LITERAL_EXPR@32..42
                          ATOM_LITERAL@32..42 "#confirmed"
                      WHITESPACE@42..43 " "
                      R_BRACE@43..44 "}"
            "##]],
        );
    }

    // r[verify lexer.atom]
    #[test]
    fn parse_atom_in_field_no_errors() {
        check_no_errors("event { name: @meeting, status: #confirmed }");
    }

    // r[verify lexer.atom]
    #[test]
    fn parse_atom_in_list() {
        check(
            "calendar { days: [#monday, #wednesday, #friday] }",
            expect![[r##"
                SOURCE_FILE@0..49
                  CALENDAR_DECL@0..49
                    CALENDAR_KW@0..8 "calendar"
                    WHITESPACE@8..9 " "
                    RECORD_EXPR@9..49
                      L_BRACE@9..10 "{"
                      WHITESPACE@10..11 " "
                      FIELD@11..47
                        IDENT@11..15 "days"
                        COLON@15..16 ":"
                        WHITESPACE@16..17 " "
                        LIST_EXPR@17..47
                          L_BRACKET@17..18 "["
                          LITERAL_EXPR@18..25
                            ATOM_LITERAL@18..25 "#monday"
                          COMMA@25..26 ","
                          WHITESPACE@26..27 " "
                          LITERAL_EXPR@27..37
                            ATOM_LITERAL@27..37 "#wednesday"
                          COMMA@37..38 ","
                          WHITESPACE@38..39 " "
                          LITERAL_EXPR@39..46
                            ATOM_LITERAL@39..46 "#friday"
                          R_BRACKET@46..47 "]"
                      WHITESPACE@47..48 " "
                      R_BRACE@48..49 "}"
            "##]],
        );
    }

    // ── Preprocessor integration ─────────────────────────────────

    // r[verify lexer.input-format.bom-removal]
    #[test]
    fn parse_with_bom() {
        let source = "\u{FEFF}calendar {}";
        let parse = parse(source);
        assert!(parse.ok());
    }

    // r[verify lexer.input-format.shebang-removal]
    #[test]
    fn parse_with_shebang() {
        let source = "#!/usr/bin/env gnomon\ncalendar {}";
        let parse = parse(source);
        assert!(parse.ok());
    }
}
