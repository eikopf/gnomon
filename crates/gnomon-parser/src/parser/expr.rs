use crate::syntax_kind::SyntaxKind;

use super::Parser;

impl Parser {
    // r[impl expr.syntax+2]
    /// Parse an expression using a Pratt parser for operator precedence.
    pub(super) fn parse_expr(&mut self) {
        self.parse_expr_bp(0);
    }

    /// Pratt parser core: parse an expression with minimum binding power `min_bp`.
    fn parse_expr_bp(&mut self, min_bp: u8) {
        let cp = self.checkpoint();
        self.parse_primary_expr();

        loop {
            match self.current() {
                // Postfix: .field (binding power 90)
                SyntaxKind::DOT => {
                    if 90 < min_bp {
                        break;
                    }
                    self.start_node_at(cp, SyntaxKind::FIELD_ACCESS_EXPR);
                    self.bump(); // .
                    self.expect(SyntaxKind::IDENT);
                    self.finish_node();
                }
                // Postfix: [expr] (binding power 90)
                SyntaxKind::L_BRACKET => {
                    if 90 < min_bp {
                        break;
                    }
                    self.start_node_at(cp, SyntaxKind::INDEX_EXPR);
                    self.bump(); // [
                    self.parse_expr();
                    self.expect(SyntaxKind::R_BRACKET);
                    self.finish_node();
                }
                // r[impl expr.op.assoc.concat-merge]
                // Right-associative: ++, // (binding power 50)
                SyntaxKind::PLUS_PLUS | SyntaxKind::SLASH_SLASH => {
                    if 50 < min_bp {
                        break;
                    }
                    self.start_node_at(cp, SyntaxKind::BINARY_EXPR);
                    self.bump(); // operator
                    self.parse_expr_bp(50); // right-associative: same min_bp
                    self.finish_node();
                }
                // r[impl expr.op.assoc.comparison]
                // Non-associative: ==, != (binding power 30)
                SyntaxKind::EQ_EQ | SyntaxKind::BANG_EQ => {
                    if 30 < min_bp {
                        break;
                    }
                    self.start_node_at(cp, SyntaxKind::BINARY_EXPR);
                    self.bump(); // operator
                    self.parse_expr_bp(31); // higher min_bp prevents right nesting
                    self.finish_node();
                    // Non-associative: error if another comparison follows
                    if matches!(self.current(), SyntaxKind::EQ_EQ | SyntaxKind::BANG_EQ) {
                        self.error_at_current(
                            "comparison operators cannot be chained; use parentheses",
                        );
                    }
                    break; // prevent left-nesting by exiting the loop
                }
                _ => break,
            }
        }
    }

    /// Parse a primary (atomic) expression.
    fn parse_primary_expr(&mut self) {
        match self.current() {
            SyntaxKind::L_BRACE => self.parse_record_expr(),
            SyntaxKind::L_BRACKET => self.parse_list_expr(),
            SyntaxKind::L_PAREN => self.parse_paren_expr(),

            SyntaxKind::IDENT if self.current_text() == "calendar" => self.parse_calendar_expr(),
            SyntaxKind::IDENT if self.current_text() == "event" => self.parse_event_expr(),
            SyntaxKind::IDENT if self.current_text() == "task" => self.parse_task_expr(),
            SyntaxKind::IDENT if self.current_text() == "import" => self.parse_import_expr(),
            SyntaxKind::IDENT if self.current_text() == "let" => self.parse_let_expr(),
            SyntaxKind::IDENT if self.current_text() == "every" => self.parse_every_expr(),
            SyntaxKind::IDENT => self.parse_ident_expr(),

            // Literal tokens
            SyntaxKind::INTEGER_LITERAL
            | SyntaxKind::SIGNED_INTEGER_LITERAL
            | SyntaxKind::STRING_LITERAL
            | SyntaxKind::TRIPLE_STRING_LITERAL
            | SyntaxKind::DATE_LITERAL
            | SyntaxKind::MONTH_DAY_LITERAL
            | SyntaxKind::TIME_LITERAL
            | SyntaxKind::DATETIME_LITERAL
            | SyntaxKind::DURATION_LITERAL
            | SyntaxKind::URI_LITERAL
            | SyntaxKind::ATOM_LITERAL
            | SyntaxKind::PATH_LITERAL
            | SyntaxKind::TRUE_KW
            | SyntaxKind::FALSE_KW
            | SyntaxKind::UNDEFINED_KW
            | SyntaxKind::NAME => {
                self.parse_literal_expr();
            }

            _ => {
                self.error_at_current("expected expression");
            }
        }
    }

    // r[impl expr.literal.syntax+4]
    /// Parse a literal expression (wraps a single literal token in a LITERAL_EXPR node).
    fn parse_literal_expr(&mut self) {
        self.start_node(SyntaxKind::LITERAL_EXPR);
        self.bump();
        self.finish_node();
    }

    // r[impl expr.record.syntax]
    /// Parse a record expression: `{ field, field, ... }`
    pub(super) fn parse_record_expr(&mut self) {
        self.start_node(SyntaxKind::RECORD_EXPR);
        self.expect(SyntaxKind::L_BRACE);

        // Fields, separated by commas
        while !self.at(SyntaxKind::R_BRACE) && !self.at_eof() {
            if self.current() == SyntaxKind::IDENT {
                self.parse_field();
            } else {
                self.error_at_current("expected field name");
                self.error_recover();
                continue;
            }

            if self.at(SyntaxKind::COMMA) {
                self.bump(); // eat comma
            } else if !self.at(SyntaxKind::R_BRACE) {
                self.error_at_current("expected `,` or `}`");
                // Don't consume — let the loop try again
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.finish_node();
    }

    /// Parse a single field: `ident : expr`
    fn parse_field(&mut self) {
        self.start_node(SyntaxKind::FIELD);
        self.bump(); // IDENT (field name)
        self.expect(SyntaxKind::COLON);
        self.parse_expr();
        self.finish_node();
    }

    // r[impl expr.list.syntax]
    /// Parse a list expression: `[ expr, expr, ... ]`
    pub(super) fn parse_list_expr(&mut self) {
        self.start_node(SyntaxKind::LIST_EXPR);
        self.expect(SyntaxKind::L_BRACKET);

        while !self.at(SyntaxKind::R_BRACKET) && !self.at_eof() {
            self.parse_expr();

            if self.at(SyntaxKind::COMMA) {
                self.bump();
            } else if !self.at(SyntaxKind::R_BRACKET) {
                self.error_at_current("expected `,` or `]`");
                break;
            }
        }

        self.expect(SyntaxKind::R_BRACKET);
        self.finish_node();
    }

    /// Parse a parenthesized expression: `( expr )`
    fn parse_paren_expr(&mut self) {
        self.start_node(SyntaxKind::PAREN_EXPR);
        self.bump(); // (
        self.parse_expr();
        self.expect(SyntaxKind::R_PAREN);
        self.finish_node();
    }

    /// Parse an import expression: `import source [as format]`
    // r[impl expr.import.syntax+2]
    fn parse_import_expr(&mut self) {
        self.start_node(SyntaxKind::IMPORT_EXPR);
        self.bump_remap(SyntaxKind::IMPORT_KW);
        match self.current() {
            SyntaxKind::PATH_LITERAL | SyntaxKind::URI_LITERAL | SyntaxKind::STRING_LITERAL => {
                self.bump()
            }
            _ => self.error_at_current("expected path, URI, or string"),
        }
        if self.at_keyword("as") {
            self.bump_remap(SyntaxKind::AS_KW);
            match self.current_text() {
                "gnomon" => self.bump_remap(SyntaxKind::GNOMON_KW),
                "icalendar" => self.bump_remap(SyntaxKind::ICALENDAR_KW),
                "jscalendar" => self.bump_remap(SyntaxKind::JSCALENDAR_KW),
                _ => self.error_at_current("expected format: gnomon, icalendar, or jscalendar"),
            }
        }
        self.finish_node();
    }

    /// Parse a let expression: `let ident = expr in expr`
    // r[impl expr.let.syntax]
    fn parse_let_expr(&mut self) {
        self.start_node(SyntaxKind::LET_EXPR);
        self.bump_remap(SyntaxKind::LET_KW);
        self.expect(SyntaxKind::IDENT);
        self.expect(SyntaxKind::EQUALS);
        self.parse_expr();
        self.expect_keyword("in", SyntaxKind::IN_KW);
        self.parse_expr();
        self.finish_node();
    }

    /// Parse an identifier expression (bare variable reference).
    fn parse_ident_expr(&mut self) {
        self.start_node(SyntaxKind::IDENT_EXPR);
        self.bump(); // IDENT
        self.finish_node();
    }
}
