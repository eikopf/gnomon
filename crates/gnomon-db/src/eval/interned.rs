use std::fmt;

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

/// A segment of a [`FieldPath`]. Uses [`salsa::Id`] instead of [`FieldName`]
/// to keep the type `'static`, allowing `FieldPath` to be salsa-interned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PathSegment {
    Field(salsa::Id),
    Index(usize),
}

/// A path into a record structure, e.g. `[Field("alerts"), Index(0), Field("trigger")]`.
///
/// Salsa-interned for O(1) cloning — paths are stored in every [`super::types::Blame`]
/// value, so cheap clones matter.
#[salsa::interned]
pub struct FieldPath<'db> {
    #[returns(ref)]
    pub segments: Vec<PathSegment>,
}

impl fmt::Debug for FieldPath<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let id: salsa::Id = salsa::plumbing::AsId::as_id(self);
        f.debug_tuple("FieldPath").field(&id).finish()
    }
}

impl<'db> FieldPath<'db> {
    pub fn root(db: &'db dyn crate::Db) -> Self {
        FieldPath::new(db, Vec::new())
    }

    pub fn field(&self, db: &'db dyn crate::Db, name: FieldName<'db>) -> Self {
        self.push(db, PathSegment::Field(salsa::plumbing::AsId::as_id(&name)))
    }

    pub fn index(&self, db: &'db dyn crate::Db, i: usize) -> Self {
        self.push(db, PathSegment::Index(i))
    }

    fn push(&self, db: &'db dyn crate::Db, segment: PathSegment) -> Self {
        let segs = self.segments(db);
        let mut new = Vec::with_capacity(segs.len() + 1);
        new.extend_from_slice(segs);
        new.push(segment);
        FieldPath::new(db, new)
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
