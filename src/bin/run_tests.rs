//! Test runner binary for jq compatibility tests
//!
//! Usage: cargo run --bin run-tests -- <test-file> [--verbose] [--fail-fast] [--filter <str>]

use std::env;
use std::fs;
use std::process;

use jqr::testing::{parse_test_file, run_test_case, TestCase, TestOutcome};

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut test_file = None;
    let mut verbose = false;
    let mut fail_fast = false;
    let mut filter_str = None;
    let mut show_errors = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--verbose" | "-v" => verbose = true,
            "--fail-fast" | "-f" => fail_fast = true,
            "--errors" | "-e" => show_errors = true,
            "--filter" => {
                i += 1;
                if i < args.len() {
                    filter_str = Some(args[i].clone());
                }
            }
            "--help" | "-h" => {
                eprintln!("Usage: run-tests <test-file> [--verbose] [--fail-fast] [--filter <str>] [--errors]");
                eprintln!();
                eprintln!("Options:");
                eprintln!("  --verbose, -v     Show all test results including passes");
                eprintln!("  --fail-fast, -f   Stop on first failure");
                eprintln!("  --filter <str>    Only run tests whose filter contains <str>");
                eprintln!("  --errors, -e      Show parse/runtime errors");
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

    let test_file = match test_file {
        Some(f) => f,
        None => {
            eprintln!("Usage: run-tests <test-file> [options]");
            process::exit(1);
        }
    };

    let content = match fs::read_to_string(&test_file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Cannot read {}: {}", test_file, e);
            process::exit(1);
        }
    };

    // Set module search path to the modules directory relative to test file
    use std::path::PathBuf;
    let test_path = PathBuf::from(&test_file);
    if let Some(parent) = test_path.parent() {
        let modules_dir = parent.join("modules");
        if modules_dir.exists() {
            jqr::set_module_search_path(Some(modules_dir));
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
        if let Some(ref f) = filter_str {
            if !filter.contains(f.as_str()) {
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
    println!("Test Results for: {}", test_file);
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

    // Exit with appropriate code
    if fail_count > 0 || error_count > 0 {
        process::exit(1);
    }
}
