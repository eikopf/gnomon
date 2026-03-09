//! Thin adapter that delegates foreign-format translation to `gnomon_import`
//! and converts the salsa-free `ImportValue` into `Value<'db>`.

use gnomon_import::{ImportRecord, ImportValue};

use super::interned::FieldName;
use super::types::{Blame, Blamed, Record, Value};

/// Convert an `ImportValue` tree into a salsa-interned `Value<'db>`.
fn import_value_to_value<'db>(
    db: &'db dyn crate::Db,
    iv: ImportValue,
    blame: &Blame<'db>,
) -> Value<'db> {
    match iv {
        ImportValue::String(s) => Value::String(s),
        ImportValue::Integer(n) => Value::Integer(n),
        ImportValue::SignedInteger(n) => Value::SignedInteger(n),
        ImportValue::Bool(b) => Value::Bool(b),
        ImportValue::Undefined => Value::Undefined,
        ImportValue::Record(map) => Value::Record(import_record_to_record(db, map, blame)),
        ImportValue::List(items) => {
            let blamed_items = items
                .into_iter()
                .map(|v| Blamed {
                    value: import_value_to_value(db, v, blame),
                    blame: blame.clone(),
                })
                .collect();
            Value::List(blamed_items)
        }
    }
}

/// Convert an `ImportRecord` into a salsa-interned `Record<'db>`.
fn import_record_to_record<'db>(
    db: &'db dyn crate::Db,
    map: ImportRecord,
    blame: &Blame<'db>,
) -> Record<'db> {
    let mut record = Record::new();
    for (key, val) in map {
        let field_name = FieldName::new(db, key);
        record.insert(
            field_name,
            Blamed {
                value: import_value_to_value(db, val, blame),
                blame: blame.clone(),
            },
        );
    }
    record
}

/// Translate an iCalendar string into a Gnomon `Value::List` of records.
pub fn translate_icalendar<'db>(
    db: &'db dyn crate::Db,
    content: &str,
    blame: &Blame<'db>,
) -> Result<Value<'db>, String> {
    let iv = gnomon_import::translate_icalendar(content)?;
    Ok(import_value_to_value(db, iv, blame))
}

/// Translate a JSCalendar JSON string into a Gnomon value.
pub fn translate_jscalendar<'db>(
    db: &'db dyn crate::Db,
    content: &str,
    blame: &Blame<'db>,
) -> Result<Value<'db>, String> {
    let iv = gnomon_import::translate_jscalendar(content)?;
    Ok(import_value_to_value(db, iv, blame))
}
