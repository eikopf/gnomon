use crate::syntax_kind::SyntaxKind;

use super::Parser;

impl Parser {
    /// Parse an expression: literal, record, list, or every expression.
    pub(super) fn parse_expr(&mut self) {
        match self.current() {
            SyntaxKind::L_BRACE => self.parse_record_expr(),
            SyntaxKind::L_BRACKET => self.parse_list_expr(),

            // Literal tokens
            SyntaxKind::INTEGER_LITERAL
            | SyntaxKind::SIGNED_INTEGER_LITERAL
            | SyntaxKind::STRING_LITERAL
            | SyntaxKind::DATE_LITERAL
            | SyntaxKind::MONTH_DAY_LITERAL
            | SyntaxKind::TIME_LITERAL
            | SyntaxKind::DATETIME_LITERAL
            | SyntaxKind::DURATION_LITERAL
            | SyntaxKind::TRUE_KW
            | SyntaxKind::FALSE_KW
            | SyntaxKind::NAME => {
                self.parse_literal_expr();
            }

            // `every` keyword (weak — lexed as IDENT)
            SyntaxKind::IDENT if self.current_text() == "every" => {
                self.parse_every_expr();
            }

            _ => {
                self.error_at_current("expected expression");
            }
        }
    }

    /// Parse a literal expression (wraps a single literal token in a LITERAL_EXPR node).
    fn parse_literal_expr(&mut self) {
        self.start_node(SyntaxKind::LITERAL_EXPR);
        self.bump();
        self.finish_node();
    }

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
}
