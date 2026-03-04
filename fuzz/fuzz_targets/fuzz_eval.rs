#![no_main]
use libfuzzer_sys::fuzz_target;
use std::path::PathBuf;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let db = gnomon_db::Database::default();
        let source = gnomon_db::SourceFile::new(&db, PathBuf::from("fuzz.gnomon"), s.to_string());
        let _ = gnomon_db::evaluate(&db, source);
    }
});
