use gnomon_parser::AstNode;
use salsa::Accumulator;

use crate::input::SourceFile;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    Error,
    Warning,
}

#[salsa::accumulator]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Diagnostic {
    pub range: rowan::TextRange,
    pub severity: Severity,
    pub message: String,
}

#[salsa::tracked]
pub struct ParseResult<'db> {
    #[returns(ref)]
    pub green_node: rowan::GreenNode,
    pub has_errors: bool,
}

#[salsa::tracked]
pub fn parse(db: &dyn crate::Db, source: SourceFile) -> ParseResult<'_> {
    let result = gnomon_parser::parse(source.text(db));
    for error in result.errors() {
        Diagnostic {
            range: rowan::TextRange::new(
                rowan::TextSize::from(error.range.start as u32),
                rowan::TextSize::from(error.range.end as u32),
            ),
            severity: Severity::Error,
            message: error.message.clone(),
        }
        .accumulate(db);
    }
    ParseResult::new(db, result.green_node().clone(), !result.ok())
}

impl ParseResult<'_> {
    /// Reconstruct the rowan syntax node (cursor) on demand.
    pub fn syntax_node(&self, db: &dyn crate::Db) -> gnomon_parser::SyntaxNode {
        gnomon_parser::SyntaxNode::new_root(self.green_node(db).clone())
    }

    /// Get the typed AST root on demand.
    pub fn tree(&self, db: &dyn crate::Db) -> gnomon_parser::ast::SourceFile {
        gnomon_parser::ast::SourceFile::cast(self.syntax_node(db)).unwrap()
    }
}

#[salsa::tracked]
pub struct SyntaxCheckResult<'db> {
    pub parse_has_errors: bool,
}

#[salsa::tracked]
pub fn check_syntax(db: &dyn crate::Db, source: SourceFile) -> SyntaxCheckResult<'_> {
    let parse_result = parse(db, source);
    let root = parse_result.syntax_node(db);
    let errors = gnomon_parser::validate_syntax(&root);
    for err in errors {
        Diagnostic {
            range: err.range,
            severity: Severity::Error,
            message: err.message,
        }
        .accumulate(db);
    }
    SyntaxCheckResult::new(db, parse_result.has_errors(db))
}
