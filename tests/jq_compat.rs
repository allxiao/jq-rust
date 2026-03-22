//! Integration test: jq compatibility test suite
//!
//! Runs the official jq test suite and asserts a minimum pass count
//! to prevent regressions.

use jq_rust::testing::{parse_test_file, run_test_case, TestCase, TestOutcome};
use jq_rust::set_module_search_path;
use std::path::PathBuf;

/// Minimum number of tests that must pass (updated as we fix more)
const BASELINE_PASS_COUNT: usize = 336;

/// Stack size for the test thread (16 MB to handle deeply nested structures)
const TEST_STACK_SIZE: usize = 16 * 1024 * 1024;

#[test]
fn jq_test_suite_baseline() {
    // Run in a thread with larger stack to handle deeply nested JSON structures
    let result = std::thread::Builder::new()
        .stack_size(TEST_STACK_SIZE)
        .spawn(run_test_suite)
        .expect("Failed to spawn test thread")
        .join()
        .expect("Test thread panicked");

    assert!(
        result.0 >= BASELINE_PASS_COUNT,
        "Regression: only {} tests passed, expected at least {}",
        result.0,
        BASELINE_PASS_COUNT
    );
}

fn run_test_suite() -> (usize, usize, usize) {
    // Find the jq.test file - try relative paths from workspace root
    let test_paths = [
        "../../jq/tests/jq.test",
        "../jq/tests/jq.test",
        "jq/tests/jq.test",
    ];

    let (test_file_path, content) = test_paths
        .iter()
        .find_map(|p| {
            std::fs::read_to_string(p).ok().map(|c| (PathBuf::from(p), c))
        })
        .expect("Could not find jq/tests/jq.test");

    // Set module search path to the directory containing the test modules
    let modules_dir = test_file_path.parent().unwrap().join("modules");
    set_module_search_path(Some(modules_dir));

    let test_cases = parse_test_file(&content);

    let mut pass_count = 0;
    let mut fail_count = 0;
    let mut error_count = 0;
    let mut failures: Vec<(usize, String, String)> = Vec::new();
    let mut errors: Vec<(usize, String, String)> = Vec::new();

    for tc in test_cases.iter() {
        let (filter, line_number) = match tc {
            TestCase::Normal { filter, line_number, .. } => (filter.clone(), *line_number),
            TestCase::ShouldFail { filter, line_number, .. } => (filter.clone(), *line_number),
        };

        match run_test_case(tc) {
            TestOutcome::Pass => pass_count += 1,
            TestOutcome::Fail { reason, expected, actual } => {
                fail_count += 1;
                failures.push((line_number, filter.clone(), format!("{}: expected {:?}, got {:?}", reason, expected, actual)));
            }
            TestOutcome::Error { reason, .. } => {
                error_count += 1;
                errors.push((line_number, filter.clone(), reason.clone()));
            }
        }
    }

    let total = pass_count + fail_count + error_count;

    // Print summary of errors (parse/compile failures)
    if !errors.is_empty() {
        eprintln!("\n=== ERRORS (parse/compile failures) ===");
        for (line, filter, reason) in errors.iter().take(20) {
            eprintln!("  Line {}: {} -> {}", line, &filter[..filter.len().min(40)], reason);
        }
        if errors.len() > 20 {
            eprintln!("  ... and {} more errors", errors.len() - 20);
        }
    }

    // Print summary of failures (wrong output)
    if !failures.is_empty() {
        eprintln!("\n=== FAILURES (wrong output) ===");
        for (line, filter, reason) in failures.iter().take(20) {
            eprintln!("  Line {}: {} -> {}", line, &filter[..filter.len().min(40)], &reason[..reason.len().min(100)]);
        }
        if failures.len() > 20 {
            eprintln!("  ... and {} more failures", failures.len() - 20);
        }
    }

    eprintln!(
        "\njq.test: {}/{} passed ({} failed, {} errors)",
        pass_count, total, fail_count, error_count
    );

    (pass_count, fail_count, error_count)
}
