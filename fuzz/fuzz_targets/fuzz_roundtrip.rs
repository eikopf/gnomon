#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let parse = gnomon_parser::parse(s);
        let reconstructed = parse.syntax().to_string();
        let preprocessed = gnomon_parser::preprocess(s);
        assert_eq!(
            reconstructed, preprocessed,
            "lossless roundtrip violated"
        );
    }
});
