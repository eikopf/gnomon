use crate::syntax_kind::SyntaxKind;

use super::Parser;

// r[impl record.rrule.weekday]
const WEEKDAYS: &[(&str, SyntaxKind)] = &[
    ("monday", SyntaxKind::MONDAY_KW),
    ("tuesday", SyntaxKind::TUESDAY_KW),
    ("wednesday", SyntaxKind::WEDNESDAY_KW),
    ("thursday", SyntaxKind::THURSDAY_KW),
    ("friday", SyntaxKind::FRIDAY_KW),
    ("saturday", SyntaxKind::SATURDAY_KW),
    ("sunday", SyntaxKind::SUNDAY_KW),
];

impl Parser {
    // r[impl record.rrule.every+2]
    /// Parse an `every` expression:
    /// ```ebnf
    /// every expr = "every", every_subject, [ "until", every_terminator ] ;
    /// ```
    pub(super) fn parse_every_expr(&mut self) {
        self.start_node(SyntaxKind::EVERY_EXPR);

        // "every"
        self.bump_remap(SyntaxKind::EVERY_KW);

        // Subject
        self.parse_every_subject();

        // Optional "until" clause
        if self.at_keyword("until") {
            self.bump_remap(SyntaxKind::UNTIL_KW);
            self.parse_every_terminator();
        }

        self.finish_node();
    }

    // r[impl record.rrule.every+2]
    /// ```ebnf
    /// every_subject = "day"
    ///               | "year", "on", month_day_literal
    ///               | weekday
    ///               ;
    /// ```
    fn parse_every_subject(&mut self) {
        if self.at_keyword("day") {
            self.bump_remap(SyntaxKind::DAY_KW);
        } else if self.at_keyword("year") {
            self.bump_remap(SyntaxKind::YEAR_KW);
            self.expect_keyword("on", SyntaxKind::ON_KW);
            self.expect(SyntaxKind::MONTH_DAY_LITERAL);
        } else if let Some(kind) = self.at_weekday() {
            self.bump_remap(kind);
        } else {
            self.error_at_current("expected `day`, `year`, or a weekday");
        }
    }

    // r[impl record.rrule.every+2]
    /// ```ebnf
    /// every_terminator = datetime_literal
    ///                  | date_literal
    ///                  | integer_literal, "times"
    ///                  ;
    /// ```
    fn parse_every_terminator(&mut self) {
        match self.current() {
            SyntaxKind::DATETIME_LITERAL | SyntaxKind::DATE_LITERAL => {
                self.bump();
            }
            SyntaxKind::INTEGER_LITERAL => {
                self.bump();
                self.expect_keyword("times", SyntaxKind::TIMES_KW);
            }
            _ => {
                self.error_at_current("expected datetime, date, or integer (followed by `times`)");
            }
        }
    }

    /// If the current token is an IDENT matching a weekday, return the keyword kind.
    fn at_weekday(&self) -> Option<SyntaxKind> {
        if self.current() != SyntaxKind::IDENT {
            return None;
        }
        let text = self.current_text();
        WEEKDAYS
            .iter()
            .find(|(name, _)| *name == text)
            .map(|(_, kind)| *kind)
    }
}
