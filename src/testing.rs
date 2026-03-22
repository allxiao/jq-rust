//! Test suite parsing and execution for jq compatibility tests
//!
//! Parses the jq test file format (jq.test, base64.test, etc.)
//! and runs tests against our interpreter.

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::jv::{parse_json, print_jv_with_options, JvPrintOptions};
use crate::{interpret, parse, Jv};

/// Timeout for a single test case (in seconds)
const TEST_TIMEOUT_SECS: u64 = 2;

/// A single test case parsed from a .test file
#[derive(Debug, Clone)]
pub enum TestCase {
    /// A normal test: filter + input -> expected outputs
    Normal {
        filter: String,
        input: String,
        expected_outputs: Vec<String>,
        line_number: usize,
    },
    /// A test that should fail to compile
    ShouldFail {
        filter: String,
        error_lines: Vec<String>,
        check_msg: bool,
        line_number: usize,
    },
}

/// Result of running a single test case
#[derive(Debug)]
pub enum TestOutcome {
    Pass,
    Fail {
        reason: String,
        expected: Vec<String>,
        actual: Vec<String>,
    },
    Error {
        reason: String,
    },
}

/// Parse a jq test file into a list of test cases
pub fn parse_test_file(content: &str) -> Vec<TestCase> {
    let lines: Vec<&str> = content.lines().collect();
    let mut cases = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // Skip blank lines and comments
        if line.is_empty() || line.starts_with('#') {
            i += 1;
            continue;
        }

        // Check for %%FAIL
        if line.starts_with("%%FAIL") {
            let check_msg = !line.contains("IGNORE MSG");
            let fail_line = i + 1; // 1-indexed line for reporting
            i += 1;

            if i >= lines.len() {
                break;
            }

            // Next line is the filter that should fail
            let filter = lines[i].to_string();
            i += 1;

            // Remaining lines until blank are error message lines
            let mut error_lines = Vec::new();
            while i < lines.len() && !lines[i].is_empty() {
                error_lines.push(lines[i].to_string());
                i += 1;
            }

            cases.push(TestCase::ShouldFail {
                filter,
                error_lines,
                check_msg,
                line_number: fail_line,
            });
        } else {
            // Normal test case
            let test_line = i + 1; // 1-indexed
            let filter = line.to_string();
            i += 1;

            if i >= lines.len() {
                break;
            }

            // Strip BOM from input line
            let input_raw = lines[i];
            let input = input_raw.trim_start_matches('\u{feff}').to_string();
            i += 1;

            // Read expected outputs until blank line or EOF or next comment
            let mut expected_outputs = Vec::new();
            while i < lines.len() && !lines[i].is_empty() && !lines[i].starts_with('#') {
                expected_outputs.push(lines[i].to_string());
                i += 1;
            }

            if !expected_outputs.is_empty() {
                cases.push(TestCase::Normal {
                    filter,
                    input,
                    expected_outputs,
                    line_number: test_line,
                });
            }
        }
    }

    cases
}

/// Run a single test case and return the outcome
pub fn run_test_case(tc: &TestCase) -> TestOutcome {
    match tc {
        TestCase::ShouldFail {
            filter, check_msg: _, ..
        } => {
            // Try to compile the filter - it should fail
            match parse(filter) {
                Err(_e) => TestOutcome::Pass,
                Ok(_expr) => {
                    // Parser succeeded when it shouldn't have - but maybe execution fails?
                    // Some %%FAIL tests fail at runtime, not parse time
                    // For now, count as fail
                    TestOutcome::Fail {
                        reason: format!("Expected compilation failure for: {}", filter),
                        expected: vec!["<compile error>".to_string()],
                        actual: vec!["<compiled successfully>".to_string()],
                    }
                }
            }
        }
        TestCase::Normal {
            filter,
            input,
            expected_outputs,
            ..
        } => {
            // Parse the filter
            let expr = match parse(filter) {
                Ok(e) => e,
                Err(e) => {
                    return TestOutcome::Error {
                        reason: format!("Parse error: {}", e),
                    };
                }
            };

            // Parse the input JSON
            let input_jv = match parse_json(input) {
                Ok(v) => v,
                Err(e) => {
                    return TestOutcome::Error {
                        reason: format!("Input parse error: {}", e),
                    };
                }
            };

            // Run the interpreter
            let results = interpret(&expr, input_jv);

            // Collect outputs in compact JSON format
            // Limit to prevent infinite iterators from hanging
            const MAX_OUTPUTS: usize = 10000;
            let compact_opts = JvPrintOptions::compact();
            let mut actual_outputs = Vec::new();

            for result in results {
                if actual_outputs.len() >= MAX_OUTPUTS {
                    return TestOutcome::Error {
                        reason: format!("Too many outputs (>{}) - possible infinite loop", MAX_OUTPUTS),
                    };
                }
                match result {
                    Ok(value) => {
                        let formatted = format_output(&value, &compact_opts);
                        actual_outputs.push(formatted);
                    }
                    Err(e) => {
                        // Some tests expect error outputs - represent as error strings
                        actual_outputs.push(format!("<error: {}>", e));
                    }
                }
            }

            // Compare outputs - normalize both to handle whitespace differences
            if actual_outputs.len() != expected_outputs.len() {
                return TestOutcome::Fail {
                    reason: format!(
                        "Output count mismatch: expected {}, got {}",
                        expected_outputs.len(),
                        actual_outputs.len()
                    ),
                    expected: expected_outputs.clone(),
                    actual: actual_outputs,
                };
            }

            for (i, (expected, actual)) in
                expected_outputs.iter().zip(actual_outputs.iter()).enumerate()
            {
                // Normalize both by parsing and re-formatting, to handle whitespace differences
                let normalized_expected = normalize_json(expected);
                let normalized_actual = normalize_json(actual);

                if normalized_expected != normalized_actual {
                    return TestOutcome::Fail {
                        reason: format!("Output {} mismatch", i + 1),
                        expected: expected_outputs.clone(),
                        actual: actual_outputs,
                    };
                }
            }

            TestOutcome::Pass
        }
    }
}

/// Normalize a JSON string by parsing and re-formatting in compact form
fn normalize_json(s: &str) -> String {
    match parse_json(s) {
        Ok(jv) => {
            let opts = JvPrintOptions::compact();
            print_jv_with_options(&jv, &opts)
        }
        Err(_) => s.to_string(), // If not valid JSON, return as-is
    }
}

/// Format a Jv value as compact JSON string, matching jq output
fn format_output(value: &Jv, opts: &JvPrintOptions) -> String {
    print_jv_with_options(value, opts)
}
