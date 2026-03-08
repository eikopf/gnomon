use std::fmt;

/// All syntax kinds in the Gnomon language.
///
/// Token kinds (leaves) and node kinds (internal) share the same enum so that
/// `rowan` can use a single `u16` discriminant for both.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[allow(non_camel_case_types)]
#[repr(u16)]
pub enum SyntaxKind {
    // ── Trivia ───────────────────────────────────────────────────────
    WHITESPACE = 0,
    COMMENT,

    // ── Punctuation ──────────────────────────────────────────────────
    L_BRACE,      // {
    R_BRACE,      // }
    L_BRACKET,    // [
    R_BRACKET,    // ]
    L_PAREN,      // (
    R_PAREN,      // )
    COLON,        // :
    COMMA,        // ,
    EQUALS,       // =
    EQ_EQ,        // ==
    BANG,         // !
    BANG_EQ,      // !=
    DOT,          // .
    HYPHEN,       // -
    PLUS,         // +
    PLUS_PLUS,    // ++
    SLASH,        // /
    SLASH_SLASH,  // //
    AT,           // @

    // ── Literals ─────────────────────────────────────────────────────
    INTEGER_LITERAL,
    SIGNED_INTEGER_LITERAL,
    STRING_LITERAL,
    DATE_LITERAL,
    MONTH_DAY_LITERAL,
    TIME_LITERAL,
    DATETIME_LITERAL,
    DURATION_LITERAL,
    URI_LITERAL,
    ATOM_LITERAL,
    PATH_LITERAL,

    // ── Identifiers / names ──────────────────────────────────────────
    IDENT,
    NAME,

    // ── Strict keywords ──────────────────────────────────────────────
    TRUE_KW,
    FALSE_KW,
    UNDEFINED_KW,

    // ── Weak keywords (parser-promoted from IDENT) ───────────────────
    CALENDAR_KW,
    EVENT_KW,
    TASK_KW,
    EVERY_KW,
    IMPORT_KW,
    AS_KW,
    LET_KW,
    IN_KW,
    GNOMON_KW,
    ICALENDAR_KW,
    JSCALENDAR_KW,
    DAY_KW,
    YEAR_KW,
    ON_KW,
    UNTIL_KW,
    TIMES_KW,
    OMIT_KW,
    FORWARD_KW,
    BACKWARD_KW,
    MONDAY_KW,
    TUESDAY_KW,
    WEDNESDAY_KW,
    THURSDAY_KW,
    FRIDAY_KW,
    SATURDAY_KW,
    SUNDAY_KW,
    LOCAL_KW,

    // ── Error token ──────────────────────────────────────────────────
    ERROR,

    // ── Node kinds (internal / composite) ────────────────────────────
    SOURCE_FILE,
    CALENDAR_DECL,
    EVENT_DECL,
    TASK_DECL,
    SHORT_SPAN,
    SHORT_DT,
    LITERAL_EXPR,
    RECORD_EXPR,
    LIST_EXPR,
    FIELD,
    EVERY_EXPR,
    IMPORT_EXPR,
    LET_EXPR,
    LET_BINDING_NODE,
    BINARY_EXPR,
    FIELD_ACCESS_EXPR,
    INDEX_EXPR,
    PAREN_EXPR,
    IDENT_EXPR,
    ERROR_NODE,
}

impl SyntaxKind {
    pub fn is_trivia(self) -> bool {
        matches!(self, SyntaxKind::WHITESPACE | SyntaxKind::COMMENT)
    }
}

impl fmt::Display for SyntaxKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        Self(kind as u16)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GnomonLanguage {}

impl rowan::Language for GnomonLanguage {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        assert!(raw.0 <= SyntaxKind::ERROR_NODE as u16, "SyntaxKind out of range: {}", raw.0);
        // SAFETY: SyntaxKind is repr(u16) and we checked the range.
        unsafe { std::mem::transmute::<u16, SyntaxKind>(raw.0) }
    }

    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        kind.into()
    }
}

pub type SyntaxNode = rowan::SyntaxNode<GnomonLanguage>;
pub type SyntaxToken = rowan::SyntaxToken<GnomonLanguage>;
