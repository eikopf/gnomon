use crate::syntax_kind::SyntaxKind;

use super::Parser;

impl Parser {
    /// Returns true if the current position looks like the start of a declaration.
    pub(super) fn at_decl_start(&self) -> bool {
        match self.current() {
            SyntaxKind::IDENT => matches!(
                self.current_text(),
                "include" | "bind" | "calendar" | "event" | "task"
            ),
            _ => false,
        }
    }

    // r[impl decl.syntax+2]
    /// Parse a single declaration.
    pub(super) fn parse_decl(&mut self) {
        match self.current_text() {
            "include" => self.parse_inclusion(),
            "bind" => self.parse_binding(),
            "calendar" => self.parse_calendar_decl(),
            "event" => self.parse_event_decl(),
            "task" => self.parse_task_decl(),
            _ => {
                self.error_at_current("expected declaration keyword");
                self.error_recover();
            }
        }
    }

    // r[impl decl.syntax+2]
    /// `include string-literal`
    fn parse_inclusion(&mut self) {
        self.start_node(SyntaxKind::INCLUSION_DECL);
        self.bump_remap(SyntaxKind::INCLUDE_KW);
        self.expect(SyntaxKind::STRING_LITERAL);
        self.finish_node();
    }

    // r[impl decl.syntax+2]
    /// `bind name string-literal`
    fn parse_binding(&mut self) {
        self.start_node(SyntaxKind::BINDING_DECL);
        self.bump_remap(SyntaxKind::BIND_KW);
        self.expect(SyntaxKind::NAME);
        self.expect(SyntaxKind::STRING_LITERAL);
        self.finish_node();
    }

    // r[impl decl.syntax+2]
    /// `calendar record`
    fn parse_calendar_decl(&mut self) {
        self.start_node(SyntaxKind::CALENDAR_DECL);
        self.bump_remap(SyntaxKind::CALENDAR_KW);
        self.parse_record_expr();
        self.finish_node();
    }

    // r[impl decl.syntax+2]
    /// `event { ... }` (prefix form) or `event @name short_span [title] [record]` (short form)
    fn parse_event_decl(&mut self) {
        self.start_node(SyntaxKind::EVENT_DECL);
        self.bump_remap(SyntaxKind::EVENT_KW);

        match self.current() {
            SyntaxKind::L_BRACE => {
                // Prefix form: event { ... }
                self.parse_record_expr();
            }
            SyntaxKind::NAME => {
                // Short form: event @name short_span [title] [record]
                self.bump(); // NAME
                self.parse_short_span();
                // Optional title (string literal)
                if self.at(SyntaxKind::STRING_LITERAL) {
                    self.bump();
                }
                // Optional record
                if self.at(SyntaxKind::L_BRACE) {
                    self.parse_record_expr();
                }
            }
            _ => {
                self.error_at_current("expected `{` or name after `event`");
            }
        }

        self.finish_node();
    }

    // r[impl decl.syntax+2]
    /// `task { ... }` (prefix form) or `task @name [short_dt] [title] [record]` (short form)
    fn parse_task_decl(&mut self) {
        self.start_node(SyntaxKind::TASK_DECL);
        self.bump_remap(SyntaxKind::TASK_KW);

        match self.current() {
            SyntaxKind::L_BRACE => {
                // Prefix form: task { ... }
                self.parse_record_expr();
            }
            SyntaxKind::NAME => {
                // Short form: task @name [short_dt] [title] [record]
                self.bump(); // NAME

                // Optional short_dt
                if self.at_short_dt_start() {
                    self.parse_short_dt();
                }
                // Optional title
                if self.at(SyntaxKind::STRING_LITERAL) {
                    self.bump();
                }
                // Optional record
                if self.at(SyntaxKind::L_BRACE) {
                    self.parse_record_expr();
                }
            }
            _ => {
                self.error_at_current("expected `{` or name after `task`");
            }
        }

        self.finish_node();
    }

    // r[impl decl.syntax+2]
    /// `short_span = short_dt [duration]`
    fn parse_short_span(&mut self) {
        self.start_node(SyntaxKind::SHORT_SPAN);
        self.parse_short_dt();
        // Optional duration
        if self.at(SyntaxKind::DURATION_LITERAL) {
            self.bump();
        }
        self.finish_node();
    }

    // r[impl decl.syntax+2]
    /// `short_dt = date time | datetime`
    pub(super) fn parse_short_dt(&mut self) {
        self.start_node(SyntaxKind::SHORT_DT);
        match self.current() {
            SyntaxKind::DATETIME_LITERAL => {
                self.bump();
            }
            SyntaxKind::DATE_LITERAL => {
                self.bump(); // date
                self.expect(SyntaxKind::TIME_LITERAL);
            }
            _ => {
                self.error_at_current("expected datetime or date");
            }
        }
        self.finish_node();
    }

    /// Check if the current token could start a short_dt.
    fn at_short_dt_start(&self) -> bool {
        matches!(
            self.current(),
            SyntaxKind::DATETIME_LITERAL | SyntaxKind::DATE_LITERAL
        )
    }
}
