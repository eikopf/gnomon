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

impl PartialOrd for FieldName<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FieldName<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let self_id: salsa::Id = salsa::plumbing::AsId::as_id(self);
        let other_id: salsa::Id = salsa::plumbing::AsId::as_id(other);
        self_id.cmp(&other_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PathSegment<'db> {
    Field(FieldName<'db>),
    Index(usize),
}

/// A path into a record structure, e.g. `[Field("alerts"), Index(0), Field("trigger")]`.
///
/// Not salsa-interned because it contains `FieldName<'db>` values that carry a
/// non-`'static` lifetime.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FieldPath<'db>(pub Vec<PathSegment<'db>>);

impl<'db> FieldPath<'db> {
    pub fn root() -> Self {
        Self(Vec::new())
    }

    pub fn push(&self, segment: PathSegment<'db>) -> Self {
        let mut segments = self.0.clone();
        segments.push(segment);
        Self(segments)
    }

    pub fn field(&self, name: FieldName<'db>) -> Self {
        self.push(PathSegment::Field(name))
    }

    pub fn index(&self, i: usize) -> Self {
        self.push(PathSegment::Index(i))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeclKind {
    Include,
    Bind,
    Calendar,
    Event,
    Task,
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
