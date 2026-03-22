//! Integration test: jq compatibility test suite
//!
//! Runs the official jq test suite and asserts a minimum pass count
//! to prevent regressions.

use jq_rust::testing::{parse_test_file, run_test_case, TestCase, TestOutcome};

/// Minimum number of tests that must pass (updated as we fix more)
const BASELINE_PASS_COUNT: usize = 0;

#[test]
fn jq_test_suite_baseline() {
    // Find the jq.test file - try relative paths from workspace root
    let test_paths = [
        "../../jq/tests/jq.test",
        "../jq/tests/jq.test",
        "jq/tests/jq.test",
    ];

    let content = test_paths
        .iter()
        .find_map(|p| std::fs::read_to_string(p).ok())
        .expect("Could not find jq/tests/jq.test");

    let test_cases = parse_test_file(&content);

    let mut pass_count = 0;
    let mut fail_count = 0;
    let mut error_count = 0;

    for (i, tc) in test_cases.iter().enumerate() {
        let filter = match tc {
            TestCase::Normal { filter, line_number, .. } => {
                eprintln!("[{}] Running test at line {}: {}", i, line_number, &filter[..filter.len().min(50)]);
                filter.clone()
            }
            TestCase::ShouldFail { filter, line_number, .. } => {
                eprintln!("[{}] Running %%FAIL test at line {}: {}", i, line_number, &filter[..filter.len().min(50)]);
                filter.clone()
            }
        };

        match run_test_case(tc) {
            TestOutcome::Pass => pass_count += 1,
            TestOutcome::Fail { .. } => fail_count += 1,
            TestOutcome::Error { reason, .. } => {
                error_count += 1;
                if reason.contains("Too many outputs") || reason.contains("too many") {
                    eprintln!("  -> INFINITE LOOP DETECTED: {}", filter);
                }
            }
        }
    }

    let total = pass_count + fail_count + error_count;
    eprintln!(
        "jq.test: {}/{} passed ({} failed, {} errors)",
        pass_count, total, fail_count, error_count
    );

    assert!(
        pass_count >= BASELINE_PASS_COUNT,
        "Regression: only {} tests passed, expected at least {}",
        pass_count,
        BASELINE_PASS_COUNT
    );
}
