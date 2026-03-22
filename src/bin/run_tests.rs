//! Test runner binary for jq compatibility tests
//!
//! Usage: cargo run --bin run-tests -- [test-file] [--verbose] [--fail-fast] [--filter <str>]
//!
//! If no test file is specified, runs all bundled test files.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;

use jq_rust::testing::{parse_test_file, run_test_case, TestCase, TestOutcome};

/// Get the path to bundled test data
fn get_test_data_dir() -> PathBuf {
    // When run via cargo, CARGO_MANIFEST_DIR points to the crate root
    if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
        return PathBuf::from(manifest_dir).join("tests").join("data");
    }
    // Fallback: try relative to current dir
    PathBuf::from("tests").join("data")
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut test_file = None;
    let mut verbose = false;
    let mut fail_fast = false;
    let mut filter_str = None;
    let mut show_errors = false;
    let mut run_all = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--verbose" | "-v" => verbose = true,
            "--fail-fast" | "-f" => fail_fast = true,
            "--errors" | "-e" => show_errors = true,
            "--all" | "-a" => run_all = true,
            "--filter" => {
                i += 1;
                if i < args.len() {
                    filter_str = Some(args[i].clone());
                }
            }
            "--help" | "-h" => {
                eprintln!("Usage: run-tests [test-file] [options]");
                eprintln!();
                eprintln!("If no test file is specified, runs the bundled jq.test file.");
                eprintln!();
                eprintln!("Options:");
                eprintln!("  --all, -a         Run all bundled test files");
                eprintln!("  --verbose, -v     Show all test results including passes");
                eprintln!("  --fail-fast, -f   Stop on first failure");
                eprintln!("  --filter <str>    Only run tests whose filter contains <str>");
                eprintln!("  --errors, -e      Show parse/runtime errors");
                eprintln!();
                eprintln!("Bundled test files:");
                let test_dir = get_test_data_dir();
                if test_dir.exists() {
                    for entry in fs::read_dir(&test_dir).unwrap().flatten() {
                        if entry.path().extension().map_or(false, |e| e == "test") {
                            eprintln!("  {}", entry.path().display());
                        }
                    }
                }
                process::exit(0);
            }
            s if test_file.is_none() && !s.starts_with('-') => {
                test_file = Some(s.to_string());
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                process::exit(1);
            }
        }
        i += 1;
    }

    // Determine which test files to run
    let test_files: Vec<PathBuf> = if run_all {
        // Run all bundled test files
        let test_dir = get_test_data_dir();
        let mut files: Vec<PathBuf> = fs::read_dir(&test_dir)
            .unwrap()
            .flatten()
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "test"))
            .map(|e| e.path())
            .collect();
        files.sort();
        files
    } else if let Some(f) = test_file {
        vec![PathBuf::from(f)]
    } else {
        // Default: run jq.test from bundled data
        vec![get_test_data_dir().join("jq.test")]
    };

    let mut total_pass = 0;
    let mut total_fail = 0;
    let mut total_error = 0;

    for test_file in &test_files {
        let (pass, fail, error) = run_test_file(
            test_file,
            verbose,
            fail_fast,
            show_errors,
            filter_str.as_deref(),
        );
        total_pass += pass;
        total_fail += fail;
        total_error += error;

        if fail_fast && (fail > 0 || error > 0) {
            break;
        }
    }

    if test_files.len() > 1 {
        println!();
        println!("========================================");
        println!("Total Results");
        println!("========================================");
        println!(
            "Pass: {}  Fail: {}  Error: {}",
            total_pass, total_fail, total_error
        );
    }

    // Exit with appropriate code
    if total_fail > 0 || total_error > 0 {
        process::exit(1);
    }
}

fn run_test_file(
    test_file: &PathBuf,
    verbose: bool,
    fail_fast: bool,
    show_errors: bool,
    filter_str: Option<&str>,
) -> (usize, usize, usize) {
    let content = match fs::read_to_string(test_file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Cannot read {}: {}", test_file.display(), e);
            return (0, 0, 1);
        }
    };

    // Set module search path to the modules directory relative to test file
    if let Some(parent) = test_file.parent() {
        let modules_dir = parent.join("modules");
        if modules_dir.exists() {
            jq_rust::set_module_search_path(Some(modules_dir));
        }
    }

    let test_cases = parse_test_file(&content);

    let mut pass_count = 0;
    let mut fail_count = 0;
    let mut error_count = 0;
    let mut skip_count = 0;
    let total = test_cases.len();

    let mut failures: Vec<(usize, String, String, Vec<String>, Vec<String>)> = Vec::new();
    let mut errors: Vec<(usize, String, String)> = Vec::new();

    for tc in &test_cases {
        let (filter, line_num) = match tc {
            TestCase::Normal {
                filter,
                line_number,
                ..
            } => (filter.clone(), *line_number),
            TestCase::ShouldFail {
                filter,
                line_number,
                ..
            } => (filter.clone(), *line_number),
        };

        // Apply filter
        if let Some(f) = filter_str {
            if !filter.contains(f) {
                skip_count += 1;
                continue;
            }
        }

        let outcome = run_test_case(tc);

        match outcome {
            TestOutcome::Pass => {
                pass_count += 1;
                if verbose {
                    println!("  PASS [line {}]: {}", line_num, filter);
                }
            }
            TestOutcome::Fail {
                reason,
                expected,
                actual,
            } => {
                fail_count += 1;
                failures.push((line_num, filter.clone(), reason.clone(), expected, actual));
                if verbose || fail_fast {
                    println!("  FAIL [line {}]: {}", line_num, filter);
                    println!("    Reason: {}", reason);
                }
                if fail_fast {
                    break;
                }
            }
            TestOutcome::Error { reason } => {
                error_count += 1;
                errors.push((line_num, filter.clone(), reason.clone()));
                if verbose || show_errors {
                    println!("  ERROR [line {}]: {}", line_num, filter);
                    println!("    {}", reason);
                }
                if fail_fast {
                    break;
                }
            }
        }
    }

    // Summary
    println!();
    println!("========================================");
    println!("Test Results for: {}", test_file.display());
    println!("========================================");
    println!(
        "Total: {}  Pass: {}  Fail: {}  Error: {}  Skip: {}",
        total, pass_count, fail_count, error_count, skip_count
    );

    let run_count = pass_count + fail_count + error_count;
    if run_count > 0 {
        let pass_rate = (pass_count as f64 / run_count as f64) * 100.0;
        println!("Pass rate: {:.1}%", pass_rate);
    }

    // Show failure summary
    if !failures.is_empty() && !verbose {
        println!();
        println!("Failures (first 30):");
        for (line_num, filter, reason, expected, actual) in failures.iter().take(30) {
            println!("  [line {}] {} -- {}", line_num, filter, reason);
            if expected.len() <= 3 && actual.len() <= 3 {
                println!("    expected: {:?}", expected);
                println!("    actual:   {:?}", actual);
            }
        }
    }

    if !errors.is_empty() && !verbose && show_errors {
        println!();
        println!("Errors (first 20):");
        for (line_num, filter, reason) in errors.iter().take(20) {
            println!("  [line {}] {} -- {}", line_num, filter, reason);
        }
    }

    (pass_count, fail_count, error_count)
}
