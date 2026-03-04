#![no_main]
use libfuzzer_sys::fuzz_target;
use std::path::PathBuf;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // Split input into two "files" at first null byte (or midpoint)
        let (a, b) = match s.find('\0') {
            Some(i) => (&s[..i], &s[i + 1..]),
            None => {
                let mid = s.len() / 2;
                // Find the nearest char boundary at or after midpoint
                let mid = (mid..=s.len())
                    .find(|&i| s.is_char_boundary(i))
                    .unwrap_or(s.len());
                (&s[..mid], &s[mid..])
            }
        };

        let db = gnomon_db::Database::default();
        let src_a =
            gnomon_db::SourceFile::new(&db, PathBuf::from("a.gnomon"), a.to_string());
        let src_b =
            gnomon_db::SourceFile::new(&db, PathBuf::from("b.gnomon"), b.to_string());
        let _ = gnomon_db::merge(&db, &[src_a, src_b]);
    }
});
