use std::collections::BTreeMap;

use super::interned::{DeclId, FieldName, FieldPath};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blame<'db> {
    pub decl: DeclId<'db>,
    pub path: FieldPath<'db>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blamed<'db, T> {
    pub value: T,
    pub blame: Blame<'db>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Record<'db>(pub BTreeMap<FieldName<'db>, Blamed<'db, Value<'db>>>);

impl<'db> Record<'db> {
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub fn insert(&mut self, name: FieldName<'db>, value: Blamed<'db, Value<'db>>) {
        self.0.insert(name, value);
    }

    pub fn get(&self, name: &FieldName<'db>) -> Option<&Blamed<'db, Value<'db>>> {
        self.0.get(name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value<'db> {
    String(String),
    Integer(u64),
    SignedInteger(i64),
    Bool(bool),
    Undefined,
    Name(String),
    Record(Record<'db>),
    List(Vec<Blamed<'db, Value<'db>>>),
    Path(String),
}

impl<'db> Value<'db> {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::String(_) => "string",
            Value::Integer(_) => "integer",
            Value::SignedInteger(_) => "signed integer",
            Value::Bool(_) => "bool",
            Value::Undefined => "undefined",
            Value::Name(_) => "name",
            Value::Record(_) => "record",
            Value::List(_) => "list",
            Value::Path(_) => "path",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Calendar<'db> {
    pub properties: Record<'db>,
    pub entries: Vec<Blamed<'db, Record<'db>>>,
}
