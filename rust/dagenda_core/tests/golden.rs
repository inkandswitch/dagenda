//! Goldens: each `golden/*.dagenda` must produce exactly what's in the files
//! beside it.
//!
//! | File          | Holds                                                |
//! |---------------|------------------------------------------------------|
//! | `*.plan.txt`  | everything the core derives from it (`common::report`) |
//! | `*.txt`       | `Plan::to_text`                                      |
//! | `*.svg`       | `Plan::to_svg`, the tech tree                        |
//!
//! They were reviewed by hand when blessed (the SVGs by eye). After an
//! intended change, check the diffs and re-bless with
//! `BLESS=1 cargo test --test golden`.

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
        let source = fs::read_to_string(&case)?;
        let plan = dagenda_core::parse(&source).ok();
        let outputs = [
            ("plan.txt", Some(common::report(&source, true))),
            ("txt", plan.as_ref().map(dagenda_core::plan::Plan::to_text)),
            ("svg", plan.as_ref().map(dagenda_core::plan::Plan::to_svg)),
        ];

        for (extension, output) in outputs {
            let Some(output) = output else {
                continue;
            };
            let expected_path = case.with_extension(extension);
            if bless {
                fs::write(&expected_path, &output)?;
                continue;
            }

            let expected = fs::read_to_string(&expected_path).unwrap_or_default();
            if output != expected {
                failures.push(format!(
                    "{}\n--- produced ---\n{output}\n--- expected ---\n{expected}",
                    expected_path.display()
                ));
            }
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}
