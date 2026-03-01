use rowan::ast::AstNode;

use crate::{GnomonLanguage, SyntaxKind, SyntaxNode, SyntaxToken};

pub(crate) fn child<N: AstNode<Language = GnomonLanguage>>(parent: &SyntaxNode) -> Option<N> {
    parent.children().find_map(N::cast)
}

pub(crate) fn children<N: AstNode<Language = GnomonLanguage>>(
    parent: &SyntaxNode,
) -> impl Iterator<Item = N> {
    parent.children().filter_map(N::cast)
}

pub(crate) fn token(parent: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    parent
        .children_with_tokens()
        .filter_map(|it| it.into_token())
        .find(|it| it.kind() == kind)
}
