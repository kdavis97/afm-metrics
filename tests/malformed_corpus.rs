//! Runs every `.afm` file under `tests/corpus` through the parser and checks
//! that it fails at the line and column recorded in the matching
//! `.expected` file. Each fixture is a small, realistic hand-broken file
//! rather than a unit-level probe of one parsing function, so a change that
//! shifts an error location shows up here even if it happens to not be
//! covered by the in-module tests.

use std::fs;
use std::path::Path;

#[test]
fn malformed_corpus_reports_expected_location() {
    let corpus_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let mut checked = 0;

    for entry in fs::read_dir(&corpus_dir).expect("tests/corpus should exist") {
        let path = entry.expect("directory entry should be readable").path();
        if path.extension().and_then(|e| e.to_str()) != Some("afm") {
            continue;
        }

        let contents = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e));

        let expected_path = path.with_extension("expected");
        let expected = fs::read_to_string(&expected_path)
            .unwrap_or_else(|e| panic!("failed to read {}: {}", expected_path.display(), e));
        let (want_line, want_column) = expected
            .trim()
            .split_once(':')
            .unwrap_or_else(|| panic!("{} should contain 'line:column'", expected_path.display()));
        let want_line: usize = want_line
            .parse()
            .unwrap_or_else(|_| panic!("{} has a non-numeric line", expected_path.display()));
        let want_column: usize = want_column
            .parse()
            .unwrap_or_else(|_| panic!("{} has a non-numeric column", expected_path.display()));

        let err = afm_metrics::parse(&contents)
            .err()
            .unwrap_or_else(|| panic!("{} should fail to parse but didn't", path.display()));
        assert_eq!(err.line, want_line, "{}: wrong line", path.display());
        assert_eq!(err.column, want_column, "{}: wrong column", path.display());
        checked += 1;
    }

    assert!(checked > 0, "tests/corpus should contain at least one .afm fixture");
}
