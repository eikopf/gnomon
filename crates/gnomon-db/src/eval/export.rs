//! Adapter that converts validated `Calendar<'db>` values into salsa-free
//! [`ImportValue`] trees for consumption by `gnomon-export`.

use gnomon_import::{ImportRecord, ImportValue};

use super::types::{Calendar, Record, Value};

/// Convert a validated `Calendar` into `ImportValue` representations suitable
/// for the export crate.
///
/// Returns `(calendar_properties, entries)` where `calendar_properties` is an
/// [`ImportRecord`] of calendar-level fields, and `entries` is a `Vec` of
/// [`ImportValue::Record`] values for each event/task.
pub fn calendar_to_import_values<'db>(
    db: &'db dyn crate::Db,
    calendar: &Calendar<'db>,
) -> (ImportRecord, Vec<ImportValue>) {
    let cal_record = record_to_import_record(db, &calendar.properties);
    let entries: Vec<ImportValue> = calendar
        .entries
        .iter()
        .map(|blamed| ImportValue::Record(record_to_import_record(db, &blamed.value)))
        .collect();
    (cal_record, entries)
}

/// Convert a `Record<'db>` to an [`ImportRecord`], stripping blame and
/// resolving interned field names.
fn record_to_import_record<'db>(db: &'db dyn crate::Db, record: &Record<'db>) -> ImportRecord {
    let mut result = ImportRecord::new();
    for (name, blamed) in &record.0 {
        let key = name.text(db).clone();
        let value = value_to_import_value(db, &blamed.value);
        result.insert(key, value);
    }
    result
}

/// Convert a `Value<'db>` to an [`ImportValue`], stripping blame.
fn value_to_import_value<'db>(db: &'db dyn crate::Db, value: &Value<'db>) -> ImportValue {
    match value {
        Value::String(s) => ImportValue::String(s.clone()),
        Value::Integer(n) => ImportValue::Integer(*n),
        Value::SignedInteger(n) => ImportValue::SignedInteger(*n),
        Value::Bool(b) => ImportValue::Bool(*b),
        Value::Undefined => ImportValue::Undefined,
        // Names are syntactic sugar for strings; in export they become plain strings.
        Value::Name(s) => ImportValue::String(s.clone()),
        Value::Record(r) => ImportValue::Record(record_to_import_record(db, r)),
        Value::List(items) => ImportValue::List(
            items
                .iter()
                .map(|b| value_to_import_value(db, &b.value))
                .collect(),
        ),
        // Paths are resolved during evaluation; in export they become strings.
        Value::Path(s) => ImportValue::String(s.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::interned::{DeclId, DeclKind, FieldName, FieldPath};
    use crate::eval::types::{Blame, Blamed};
    use crate::input::SourceFile;

    fn test_db() -> crate::Database {
        crate::Database::default()
    }

    fn test_blame(db: &dyn crate::Db) -> Blame<'_> {
        let source = SourceFile::new(db, "/test".into(), String::new());
        Blame {
            decl: DeclId::new(db, source, 0, DeclKind::Expr),
            path: FieldPath::root(),
        }
    }

    #[test]
    fn convert_simple_record() {
        let db = test_db();
        let blame = test_blame(&db);

        let mut record = Record::new();
        record.insert(
            FieldName::new(&db, "name".to_string()),
            Blamed {
                value: Value::String("hello".to_string()),
                blame: blame.clone(),
            },
        );
        record.insert(
            FieldName::new(&db, "count".to_string()),
            Blamed {
                value: Value::Integer(42),
                blame: blame.clone(),
            },
        );

        let import_record = record_to_import_record(&db, &record);
        assert_eq!(
            import_record.get("name"),
            Some(&ImportValue::String("hello".to_string()))
        );
        assert_eq!(import_record.get("count"), Some(&ImportValue::Integer(42)));
    }

    #[test]
    fn convert_name_to_string() {
        let db = test_db();
        let val = Value::Name("meeting".to_string());
        assert_eq!(
            value_to_import_value(&db, &val),
            ImportValue::String("meeting".to_string())
        );
    }

    #[test]
    fn convert_calendar_round_trip_structure() {
        let db = test_db();
        let blame = test_blame(&db);

        let mut props = Record::new();
        props.insert(
            FieldName::new(&db, "uid".to_string()),
            Blamed {
                value: Value::String("cal-1".to_string()),
                blame: blame.clone(),
            },
        );
        props.insert(
            FieldName::new(&db, "type".to_string()),
            Blamed {
                value: Value::String("calendar".to_string()),
                blame: blame.clone(),
            },
        );

        let mut entry = Record::new();
        entry.insert(
            FieldName::new(&db, "type".to_string()),
            Blamed {
                value: Value::String("event".to_string()),
                blame: blame.clone(),
            },
        );
        entry.insert(
            FieldName::new(&db, "title".to_string()),
            Blamed {
                value: Value::String("Standup".to_string()),
                blame: blame.clone(),
            },
        );

        let calendar = Calendar {
            properties: props,
            entries: vec![Blamed {
                value: entry,
                blame: blame.clone(),
            }],
        };

        let (cal_record, entries) = calendar_to_import_values(&db, &calendar);
        assert_eq!(
            cal_record.get("uid"),
            Some(&ImportValue::String("cal-1".to_string()))
        );
        assert_eq!(entries.len(), 1);
        if let ImportValue::Record(ref entry_record) = entries[0] {
            assert_eq!(
                entry_record.get("title"),
                Some(&ImportValue::String("Standup".to_string()))
            );
        } else {
            panic!("expected record entry");
        }
    }
}
