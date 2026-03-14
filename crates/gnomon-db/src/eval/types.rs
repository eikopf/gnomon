use super::interned::{DeclId, FieldName, FieldPath};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Blame<'db> {
    pub decl: DeclId<'db>,
    pub path: FieldPath<'db>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blamed<'db, T> {
    pub value: T,
    pub blame: Blame<'db>,
}

/// A record mapping field names to blamed values.
///
/// Fields are stored in a `Vec` sorted lexicographically by field-name text,
/// ensuring deterministic iteration order across runs regardless of salsa
/// interning order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Record<'db>(pub Vec<(FieldName<'db>, Blamed<'db, Value<'db>>)>);

impl<'db> Record<'db> {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Insert or replace a field, maintaining lexicographic order by text.
    pub fn insert(
        &mut self,
        db: &'db dyn crate::Db,
        name: FieldName<'db>,
        value: Blamed<'db, Value<'db>>,
    ) {
        let text = name.text(db);
        match self.0.binary_search_by(|(k, _)| k.text(db).cmp(text)) {
            Ok(idx) => self.0[idx] = (name, value),
            Err(idx) => self.0.insert(idx, (name, value)),
        }
    }

    /// Look up a field by name.
    pub fn get(
        &self,
        db: &'db dyn crate::Db,
        name: &FieldName<'db>,
    ) -> Option<&Blamed<'db, Value<'db>>> {
        let text = name.text(db);
        self.0
            .binary_search_by(|(k, _)| k.text(db).cmp(text))
            .ok()
            .map(|idx| &self.0[idx].1)
    }

    /// Iterate over `(FieldName, Blamed<Value>)` pairs in lexicographic order.
    pub fn iter(&self) -> impl Iterator<Item = (&FieldName<'db>, &Blamed<'db, Value<'db>>)> {
        self.0.iter().map(|(k, v)| (k, v))
    }

    /// Iterate over values only.
    pub fn values(&self) -> impl Iterator<Item = &Blamed<'db, Value<'db>>> {
        self.0.iter().map(|(_, v)| v)
    }

    /// Number of fields.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the record has no fields.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<'db> IntoIterator for Record<'db> {
    type Item = (FieldName<'db>, Blamed<'db, Value<'db>>);

    type IntoIter = std::vec::IntoIter<(FieldName<'db>, Blamed<'db, Value<'db>>)>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
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
            Value::Integer(_) => "unsigned integer",
            Value::SignedInteger(_) => "signed integer",
            Value::Bool(_) => "boolean",
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
    /// True when this calendar was produced by a foreign format import (iCalendar, JSCalendar).
    /// Foreign-import calendars may omit `uid`.
    pub foreign_import: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::interned::{DeclId, DeclKind, FieldPath};
    use crate::input::SourceFile;

    fn test_blame(db: &dyn crate::Db) -> Blame<'_> {
        let source = SourceFile::new(db, "/test".into(), String::new());
        Blame {
            decl: DeclId::new(db, source, 0, DeclKind::Expr),
            path: FieldPath::root(db),
        }
    }

    /// Record fields are iterated in lexicographic order by name,
    /// regardless of insertion order.
    #[test]
    fn record_iteration_is_alphabetical() {
        let db = crate::Database::default();
        let blame = test_blame(&db);

        let mut record = Record::new();
        // Insert in reverse alphabetical order.
        record.insert(
            &db,
            FieldName::new(&db, "zebra".to_string()),
            Blamed {
                value: Value::Integer(3),
                blame,
            },
        );
        record.insert(
            &db,
            FieldName::new(&db, "apple".to_string()),
            Blamed {
                value: Value::Integer(1),
                blame,
            },
        );
        record.insert(
            &db,
            FieldName::new(&db, "mango".to_string()),
            Blamed {
                value: Value::Integer(2),
                blame,
            },
        );

        let keys: Vec<&str> = record.iter().map(|(k, _)| k.text(&db).as_str()).collect();
        assert_eq!(keys, vec!["apple", "mango", "zebra"]);
    }

    /// Inserting a duplicate field name replaces the value.
    #[test]
    fn record_insert_replaces_duplicate() {
        let db = crate::Database::default();
        let blame = test_blame(&db);

        let mut record = Record::new();
        let name = FieldName::new(&db, "key".to_string());

        record.insert(
            &db,
            name,
            Blamed {
                value: Value::Integer(1),
                blame,
            },
        );
        record.insert(
            &db,
            name,
            Blamed {
                value: Value::Integer(2),
                blame,
            },
        );

        assert_eq!(record.len(), 1);
        assert_eq!(record.get(&db, &name).unwrap().value, Value::Integer(2));
    }
}
