mod decl;
mod every;
mod expr;

use rowan::GreenNodeBuilder;

use crate::lexer::Token;
use crate::syntax_kind::SyntaxKind;

/// A parse error with a message and byte range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub range: std::ops::Range<usize>,
}

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    builder: GreenNodeBuilder<'static>,
    errors: Vec<ParseError>,
    /// Cumulative byte offset up to (but not including) `tokens[pos]`.
    offset: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            builder: GreenNodeBuilder::new(),
            errors: Vec::new(),
            offset: 0,
        }
    }

    // ── Accessors ────────────────────────────────────────────────

    fn current(&self) -> SyntaxKind {
        self.nth(0)
    }

    fn nth(&self, n: usize) -> SyntaxKind {
        self.nth_non_trivia(n)
    }

    /// Look ahead `n` non-trivia tokens and return the kind.
    fn nth_non_trivia(&self, n: usize) -> SyntaxKind {
        let mut pos = self.pos;
        let mut remaining = n;
        loop {
            if pos >= self.tokens.len() {
                return SyntaxKind::ERROR; // EOF sentinel
            }
            if self.tokens[pos].kind.is_trivia() {
                pos += 1;
                continue;
            }
            if remaining == 0 {
                return self.tokens[pos].kind;
            }
            remaining -= 1;
            pos += 1;
        }
    }

    fn current_text(&self) -> &str {
        self.nth_text(0)
    }

    fn nth_text(&self, n: usize) -> &str {
        let mut pos = self.pos;
        let mut remaining = n;
        loop {
            if pos >= self.tokens.len() {
                return "";
            }
            if self.tokens[pos].kind.is_trivia() {
                pos += 1;
                continue;
            }
            if remaining == 0 {
                return &self.tokens[pos].text;
            }
            remaining -= 1;
            pos += 1;
        }
    }

    fn at_eof(&self) -> bool {
        // true if all remaining tokens are trivia
        self.tokens[self.pos..].iter().all(|t| t.kind.is_trivia())
    }

    fn at(&self, kind: SyntaxKind) -> bool {
        self.current() == kind
    }

    /// Check if the current non-trivia token is an IDENT with the given text.
    fn at_keyword(&self, kw: &str) -> bool {
        self.current() == SyntaxKind::IDENT && self.current_text() == kw
    }

    // ── Token consumption ────────────────────────────────────────

    /// Eat all trivia (whitespace, comments) at the current position,
    /// adding them as leaf tokens to the green tree.
    fn skip_trivia(&mut self) {
        while self.pos < self.tokens.len() && self.tokens[self.pos].kind.is_trivia() {
            let tok = &self.tokens[self.pos];
            self.builder
                .token(tok.kind.into(), &tok.text);
            self.offset += tok.text.len();
            self.pos += 1;
        }
    }

    /// Consume the current token and add it to the green tree.
    fn bump(&mut self) {
        self.skip_trivia();
        if self.pos < self.tokens.len() {
            let tok = &self.tokens[self.pos];
            self.builder
                .token(tok.kind.into(), &tok.text);
            self.offset += tok.text.len();
            self.pos += 1;
        }
    }

    /// Consume the current token but emit it under a different `SyntaxKind`.
    /// Used for weak keyword promotion: the lexer emits `IDENT`, the parser
    /// re-tags it as the appropriate keyword kind.
    fn bump_remap(&mut self, kind: SyntaxKind) {
        self.skip_trivia();
        if self.pos < self.tokens.len() {
            let tok = &self.tokens[self.pos];
            self.builder
                .token(kind.into(), &tok.text);
            self.offset += tok.text.len();
            self.pos += 1;
        }
    }

    /// Consume the current token if it matches `kind`, returning `true`.
    /// Otherwise emit an error and return `false`.
    fn expect(&mut self, kind: SyntaxKind) -> bool {
        if self.current() == kind {
            self.bump();
            true
        } else {
            self.error_at_current(&format!("expected {kind}"));
            false
        }
    }

    /// Consume the current IDENT if its text matches `kw`, re-tagging it
    /// as `remap_kind`. Returns `true` on success.
    fn expect_keyword(&mut self, kw: &str, remap_kind: SyntaxKind) -> bool {
        if self.at_keyword(kw) {
            self.bump_remap(remap_kind);
            true
        } else {
            self.error_at_current(&format!("expected `{kw}`"));
            false
        }
    }

    // ── Errors ───────────────────────────────────────────────────

    fn error_at_current(&mut self, msg: &str) {
        let range = self.current_range();
        self.errors.push(ParseError {
            message: msg.to_string(),
            range,
        });
    }

    fn current_range(&self) -> std::ops::Range<usize> {
        // Skip trivia to find the actual next token range
        let mut pos = self.pos;
        let mut off = self.offset;
        while pos < self.tokens.len() && self.tokens[pos].kind.is_trivia() {
            off += self.tokens[pos].text.len();
            pos += 1;
        }
        if pos < self.tokens.len() {
            let len = self.tokens[pos].text.len();
            off..off + len
        } else {
            off..off
        }
    }

    /// Wrap a single token in an ERROR_NODE and advance.
    fn error_recover(&mut self) {
        self.builder
            .start_node(SyntaxKind::ERROR_NODE.into());
        self.bump();
        self.builder.finish_node();
    }

    // ── Node construction ────────────────────────────────────────

    fn start_node(&mut self, kind: SyntaxKind) {
        self.skip_trivia();
        self.builder.start_node(kind.into());
    }

    fn start_node_before_trivia(&mut self, kind: SyntaxKind) {
        self.builder.start_node(kind.into());
    }

    fn finish_node(&mut self) {
        self.builder.finish_node();
    }

    // ── Entry point ──────────────────────────────────────────────

    pub fn parse(mut self) -> (rowan::GreenNode, Vec<ParseError>) {
        self.parse_source_file();
        (self.builder.finish(), self.errors)
    }

    // r[impl syntax.start]
    fn parse_source_file(&mut self) {
        self.start_node_before_trivia(SyntaxKind::SOURCE_FILE);
        while !self.at_eof() {
            if self.at_decl_start() {
                self.parse_decl();
            } else {
                self.error_at_current("expected declaration");
                self.error_recover();
            }
        }
        // Consume any trailing trivia
        self.skip_trivia();
        self.finish_node();
    }
}
