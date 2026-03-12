use rowan::ast::AstNode;

use crate::{GnomonLanguage, SyntaxKind, SyntaxNode, SyntaxToken};

use super::support;

// ── Macros ──────────────────────────────────────────────────────────

macro_rules! ast_node {
    ($name:ident, $kind:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name {
            pub(crate) syntax: SyntaxNode,
        }

        impl AstNode for $name {
            type Language = GnomonLanguage;

            fn can_cast(kind: SyntaxKind) -> bool {
                kind == SyntaxKind::$kind
            }

            fn cast(node: SyntaxNode) -> Option<Self> {
                if Self::can_cast(node.kind()) {
                    Some(Self { syntax: node })
                } else {
                    None
                }
            }

            fn syntax(&self) -> &SyntaxNode {
                &self.syntax
            }
        }
    };
}

macro_rules! ast_enum {
    ($name:ident, $($variant:ident),+ $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub enum $name {
            $($variant($variant),)+
        }

        impl AstNode for $name {
            type Language = GnomonLanguage;

            fn can_cast(kind: SyntaxKind) -> bool {
                $(<$variant as AstNode>::can_cast(kind))||+
            }

            fn cast(node: SyntaxNode) -> Option<Self> {
                let kind = node.kind();
                $(if <$variant as AstNode>::can_cast(kind) {
                    return <$variant as AstNode>::cast(node).map(Self::$variant);
                })+
                None
            }

            fn syntax(&self) -> &SyntaxNode {
                match self {
                    $(Self::$variant(v) => v.syntax(),)+
                }
            }
        }
    };
}

// ── Node types ──────────────────────────────────────────────────────

ast_node!(SourceFile, SOURCE_FILE);
ast_node!(CalendarExpr, CALENDAR_EXPR);
ast_node!(EventExpr, EVENT_EXPR);
ast_node!(TaskExpr, TASK_EXPR);
ast_node!(LiteralExpr, LITERAL_EXPR);
ast_node!(RecordExpr, RECORD_EXPR);
ast_node!(ListExpr, LIST_EXPR);
ast_node!(EveryExpr, EVERY_EXPR);
ast_node!(ImportExpr, IMPORT_EXPR);
ast_node!(LetExpr, LET_EXPR);
ast_node!(LetBindingNode, LET_BINDING_NODE);
ast_node!(BinaryExpr, BINARY_EXPR);
ast_node!(FieldAccessExpr, FIELD_ACCESS_EXPR);
ast_node!(IndexExpr, INDEX_EXPR);
ast_node!(ParenExpr, PAREN_EXPR);
ast_node!(IdentExpr, IDENT_EXPR);
ast_node!(ShortSpan, SHORT_SPAN);
ast_node!(ShortDt, SHORT_DT);
ast_node!(Field, FIELD);

// ── Enum types ──────────────────────────────────────────────────────

ast_enum!(
    Expr,
    LiteralExpr,
    RecordExpr,
    ListExpr,
    EveryExpr,
    ImportExpr,
    LetExpr,
    BinaryExpr,
    FieldAccessExpr,
    IndexExpr,
    ParenExpr,
    IdentExpr,
    CalendarExpr,
    EventExpr,
    TaskExpr
);

// ── Accessor methods ────────────────────────────────────────────────

impl SourceFile {
    pub fn let_bindings(&self) -> impl Iterator<Item = LetBindingNode> {
        support::children(&self.syntax)
    }

    /// Body expressions (0 or more top-level expressions after let bindings).
    pub fn body_exprs(&self) -> impl Iterator<Item = Expr> {
        support::children(&self.syntax)
    }
}

impl CalendarExpr {
    pub fn body(&self) -> Option<RecordExpr> {
        support::child(&self.syntax)
    }
}

impl EventExpr {
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, SyntaxKind::NAME)
    }

    pub fn short_span(&self) -> Option<ShortSpan> {
        support::child(&self.syntax)
    }

    pub fn title(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|t| {
                matches!(
                    t.kind(),
                    SyntaxKind::STRING_LITERAL | SyntaxKind::TRIPLE_STRING_LITERAL
                )
            })
    }

    pub fn body(&self) -> Option<RecordExpr> {
        support::child(&self.syntax)
    }
}

impl TaskExpr {
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, SyntaxKind::NAME)
    }

    pub fn short_dt(&self) -> Option<ShortDt> {
        support::child(&self.syntax)
    }

    pub fn title(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|t| {
                matches!(
                    t.kind(),
                    SyntaxKind::STRING_LITERAL | SyntaxKind::TRIPLE_STRING_LITERAL
                )
            })
    }

    pub fn body(&self) -> Option<RecordExpr> {
        support::child(&self.syntax)
    }
}

impl ShortSpan {
    pub fn start(&self) -> Option<ShortDt> {
        support::child(&self.syntax)
    }

    pub fn duration(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, SyntaxKind::DURATION_LITERAL)
    }
}

impl ShortDt {
    pub fn datetime(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, SyntaxKind::DATETIME_LITERAL)
    }

    pub fn date(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, SyntaxKind::DATE_LITERAL)
    }

    pub fn time(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, SyntaxKind::TIME_LITERAL)
    }
}

impl LiteralExpr {
    pub fn literal_token(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|t| !t.kind().is_trivia())
    }
}

impl RecordExpr {
    pub fn fields(&self) -> impl Iterator<Item = Field> {
        support::children(&self.syntax)
    }
}

impl ListExpr {
    pub fn elements(&self) -> impl Iterator<Item = Expr> {
        support::children(&self.syntax)
    }
}

impl EveryExpr {
    pub fn day_kw(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, SyntaxKind::DAY_KW)
    }

    pub fn year_kw(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, SyntaxKind::YEAR_KW)
    }

    pub fn on_kw(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, SyntaxKind::ON_KW)
    }

    pub fn month_day(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, SyntaxKind::MONTH_DAY_LITERAL)
    }

    pub fn weekday(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|t| {
                matches!(
                    t.kind(),
                    SyntaxKind::MONDAY_KW
                        | SyntaxKind::TUESDAY_KW
                        | SyntaxKind::WEDNESDAY_KW
                        | SyntaxKind::THURSDAY_KW
                        | SyntaxKind::FRIDAY_KW
                        | SyntaxKind::SATURDAY_KW
                        | SyntaxKind::SUNDAY_KW
                )
            })
    }

    pub fn until_kw(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, SyntaxKind::UNTIL_KW)
    }

    pub fn until_datetime(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, SyntaxKind::DATETIME_LITERAL)
    }

    pub fn until_date(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, SyntaxKind::DATE_LITERAL)
    }

    pub fn until_count(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, SyntaxKind::INTEGER_LITERAL)
    }

    pub fn times_kw(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, SyntaxKind::TIMES_KW)
    }
}

impl ImportExpr {
    /// The source token (PATH_LITERAL, URI_LITERAL, or STRING_LITERAL).
    pub fn source(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|t| {
                matches!(
                    t.kind(),
                    SyntaxKind::PATH_LITERAL | SyntaxKind::URI_LITERAL | SyntaxKind::STRING_LITERAL
                )
            })
    }

    /// The format keyword (GNOMON_KW, ICALENDAR_KW, or JSCALENDAR_KW), if present.
    pub fn format(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|t| {
                matches!(
                    t.kind(),
                    SyntaxKind::GNOMON_KW | SyntaxKind::ICALENDAR_KW | SyntaxKind::JSCALENDAR_KW
                )
            })
    }
}

impl LetExpr {
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, SyntaxKind::IDENT)
    }

    /// The bound expression (first child Expr).
    pub fn bound_expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }

    /// The body expression (second child Expr).
    pub fn body_expr(&self) -> Option<Expr> {
        support::children::<Expr>(&self.syntax).nth(1)
    }
}

impl LetBindingNode {
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, SyntaxKind::IDENT)
    }

    pub fn value_expr(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
}

impl BinaryExpr {
    /// The left-hand side expression (first child Expr).
    pub fn lhs(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }

    /// The operator token (PLUS_PLUS, SLASH_SLASH, EQ_EQ, or BANG_EQ).
    pub fn op(&self) -> Option<SyntaxToken> {
        self.syntax
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|t| {
                matches!(
                    t.kind(),
                    SyntaxKind::PLUS_PLUS
                        | SyntaxKind::SLASH_SLASH
                        | SyntaxKind::EQ_EQ
                        | SyntaxKind::BANG_EQ
                )
            })
    }

    /// The right-hand side expression (second child Expr).
    pub fn rhs(&self) -> Option<Expr> {
        support::children::<Expr>(&self.syntax).nth(1)
    }
}

impl FieldAccessExpr {
    /// The target expression being accessed.
    pub fn target(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }

    /// The field name identifier.
    pub fn field_name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, SyntaxKind::IDENT)
    }
}

impl IndexExpr {
    /// The target expression being indexed.
    pub fn target(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }

    /// The index expression inside brackets.
    pub fn index_expr(&self) -> Option<Expr> {
        support::children::<Expr>(&self.syntax).nth(1)
    }
}

impl ParenExpr {
    /// The inner expression.
    pub fn inner(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
}

impl IdentExpr {
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, SyntaxKind::IDENT)
    }
}

impl Field {
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, SyntaxKind::IDENT)
    }

    pub fn value(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
}
