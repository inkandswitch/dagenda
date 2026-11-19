//! Goldens: each `golden/*.dagenda` must derive the report in its `.txt`.
//!
//! The reports were reviewed by hand when blessed. After an intended change,
//! check the diffs and re-bless with `BLESS=1 cargo test --test golden`.

mod common;

use std::{fs, path::Path};
use testresult::TestResult;

#[test]
fn goldens() -> TestResult {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let bless = std::env::var_os("BLESS").is_some();

    let mut cases: Vec<_> = fs::read_dir(&dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "dagenda"))
        .collect();
    cases.sort();
    assert!(!cases.is_empty(), "no goldens in {}", dir.display());

    let mut failures = Vec::new();
    for case in cases {
        let report = common::report(&fs::read_to_string(&case)?, true);
        let expected_path = case.with_extension("txt");

        if bless {
            fs::write(&expected_path, &report)?;
            continue;
        }

        let expected = fs::read_to_string(&expected_path).unwrap_or_default();
        if report != expected {
            failures.push(format!(
                "{}\n--- derived ---\n{report}--- expected ---\n{expected}",
                case.display()
            ));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}
