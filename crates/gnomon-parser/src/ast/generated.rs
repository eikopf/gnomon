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
ast_node!(InclusionDecl, INCLUSION_DECL);
ast_node!(BindingDecl, BINDING_DECL);
ast_node!(CalendarDecl, CALENDAR_DECL);
ast_node!(EventDecl, EVENT_DECL);
ast_node!(TaskDecl, TASK_DECL);
ast_node!(LiteralExpr, LITERAL_EXPR);
ast_node!(RecordExpr, RECORD_EXPR);
ast_node!(ListExpr, LIST_EXPR);
ast_node!(EveryExpr, EVERY_EXPR);
ast_node!(ShortSpan, SHORT_SPAN);
ast_node!(ShortDt, SHORT_DT);
ast_node!(Field, FIELD);

// ── Enum types ──────────────────────────────────────────────────────

ast_enum!(Decl, InclusionDecl, BindingDecl, CalendarDecl, EventDecl, TaskDecl);
ast_enum!(Expr, LiteralExpr, RecordExpr, ListExpr, EveryExpr);

// ── Accessor methods ────────────────────────────────────────────────

impl SourceFile {
    pub fn decls(&self) -> impl Iterator<Item = Decl> {
        support::children(&self.syntax)
    }
}

impl InclusionDecl {
    pub fn path(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, SyntaxKind::STRING_LITERAL)
    }
}

impl BindingDecl {
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, SyntaxKind::NAME)
    }

    pub fn path(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, SyntaxKind::STRING_LITERAL)
    }
}

impl CalendarDecl {
    pub fn body(&self) -> Option<RecordExpr> {
        support::child(&self.syntax)
    }
}

impl EventDecl {
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, SyntaxKind::NAME)
    }

    pub fn short_span(&self) -> Option<ShortSpan> {
        support::child(&self.syntax)
    }

    pub fn title(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, SyntaxKind::STRING_LITERAL)
    }

    pub fn body(&self) -> Option<RecordExpr> {
        support::child(&self.syntax)
    }
}

impl TaskDecl {
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, SyntaxKind::NAME)
    }

    pub fn short_dt(&self) -> Option<ShortDt> {
        support::child(&self.syntax)
    }

    pub fn title(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, SyntaxKind::STRING_LITERAL)
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

impl Field {
    pub fn name(&self) -> Option<SyntaxToken> {
        support::token(&self.syntax, SyntaxKind::IDENT)
    }

    pub fn value(&self) -> Option<Expr> {
        support::child(&self.syntax)
    }
}
