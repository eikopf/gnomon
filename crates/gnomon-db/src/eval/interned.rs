use std::fmt;
use std::rc::Rc;

use crate::input::SourceFile;

#[salsa::interned]
pub struct FieldName<'db> {
    #[returns(ref)]
    pub text: String,
}

impl fmt::Debug for FieldName<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let id: salsa::Id = salsa::plumbing::AsId::as_id(self);
        f.debug_tuple("FieldName").field(&id).finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PathSegment<'db> {
    Field(FieldName<'db>),
    Index(usize),
}

/// A path into a record structure, e.g. `[Field("alerts"), Index(0), Field("trigger")]`.
///
/// Uses `Rc` for O(1) cloning. Paths are frequently cloned when stored in
/// `Blame` values, so cheap clones avoid the O(n) copy that a bare `Vec`
/// would require on every clone.
///
/// Not salsa-interned because it contains `FieldName<'db>` values that carry a
/// non-`'static` lifetime.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FieldPath<'db>(Rc<Vec<PathSegment<'db>>>);

impl<'db> FieldPath<'db> {
    pub fn root() -> Self {
        Self(Rc::new(Vec::new()))
    }

    pub fn push(&self, segment: PathSegment<'db>) -> Self {
        let mut segments = Vec::with_capacity(self.0.len() + 1);
        segments.extend_from_slice(&self.0);
        segments.push(segment);
        Self(Rc::new(segments))
    }

    pub fn field(&self, name: FieldName<'db>) -> Self {
        self.push(PathSegment::Field(name))
    }

    pub fn index(&self, i: usize) -> Self {
        self.push(PathSegment::Index(i))
    }

    /// Returns a reference to the path segments.
    pub fn segments(&self) -> &[PathSegment<'db>] {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeclKind {
    Calendar,
    Event,
    Task,
    /// A file-level expression (no declaration keyword).
    Expr,
}

#[salsa::interned]
pub struct DeclId<'db> {
    pub source: SourceFile,
    pub index: usize,
    pub kind: DeclKind,
}

impl fmt::Debug for DeclId<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let id: salsa::Id = salsa::plumbing::AsId::as_id(self);
        f.debug_tuple("DeclId").field(&id).finish()
    }
}
