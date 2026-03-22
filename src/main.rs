//! jqr: A Rust implementation of jq
//!
//! jq is a lightweight and flexible command-line JSON processor.

use std::io::{self, BufRead, Write};
use std::process;

use clap::Parser;
use jq_rust::jv::{parse_json_stream, print_jv_with_options, JvPrintOptions};
use jq_rust::{interpret, parse, Jv};

/// jqr - commandline JSON processor (Rust implementation of jq)
#[derive(Parser, Debug)]
#[command(name = "jqr")]
#[command(version = "0.1.0")]
#[command(
    about = "A Rust implementation of jq - a lightweight and flexible command-line JSON processor"
)]
struct Args {
    /// The jq filter to apply
    #[arg(default_value = ".")]
    filter: String,

    /// Input files (use '-' for stdin)
    #[arg(default_values_t = vec!["-".to_string()])]
    files: Vec<String>,

    /// Compact output (no pretty printing)
    #[arg(short = 'c', long = "compact-output")]
    compact: bool,

    /// Raw output (don't quote strings)
    #[arg(short = 'r', long = "raw-output")]
    raw_output: bool,

    /// Sort object keys
    #[arg(short = 'S', long = "sort-keys")]
    sort_keys: bool,

    /// Use tabs for indentation
    #[arg(long = "tab")]
    tab: bool,

    /// Read entire input as single string
    #[arg(short = 'R', long = "raw-input")]
    raw_input: bool,

    /// Read entire input as single JSON value
    #[arg(short = 's', long = "slurp")]
    slurp: bool,

    /// Don't read any input
    #[arg(short = 'n', long = "null-input")]
    null_input: bool,

    /// Exit with error if output is false or null
    #[arg(short = 'e', long = "exit-status")]
    exit_status: bool,

    /// ASCII output (escape non-ASCII)
    #[arg(short = 'a', long = "ascii-output")]
    ascii_output: bool,

    /// Join output without newlines
    #[arg(short = 'j', long = "join-output")]
    join_output: bool,

    /// Use color output
    #[arg(short = 'C', long = "color-output")]
    color: bool,

    /// Monochrome output
    #[arg(short = 'M', long = "monochrome-output")]
    monochrome: bool,
}

fn main() {
    let args = Args::parse();

    // Build print options
    let print_options = JvPrintOptions {
        pretty: !args.compact,
        sort_keys: args.sort_keys,
        use_tabs: args.tab,
        ascii_output: args.ascii_output,
        raw_output: args.raw_output,
        join_output: args.join_output,
        color: args.color && !args.monochrome,
        ..Default::default()
    };

    // Parse the filter expression
    let expr = match parse(&args.filter) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("jq-rust: compile error: {}", e);
            process::exit(3);
        }
    };

    let mut exit_code = 0;

    let result = if args.null_input {
        // No input, run filter on null
        run_filter(&expr, Jv::Null, &print_options, &args, &mut exit_code)
    } else {
        process_input(&args, &print_options, &expr, &mut exit_code)
    };

    if let Err(e) = result {
        eprintln!("jq-rust: {}", e);
        process::exit(5);
    }

    if args.exit_status {
        process::exit(exit_code);
    }
}

fn run_filter(
    expr: &jq_rust::Expr,
    input: Jv,
    print_options: &JvPrintOptions,
    args: &Args,
    exit_code: &mut i32,
) -> Result<(), String> {
    let results = interpret(expr, input);

    for result in results {
        match result {
            Ok(value) => {
                // Track exit status
                if args.exit_status {
                    match &value {
                        Jv::Bool(false) | Jv::Null => *exit_code = 1,
                        _ => {}
                    }
                }

                let output = print_jv_with_options(&value, print_options);

                if args.join_output {
                    print!("{}", output);
                } else {
                    println!("{}", output);
                }
            }
            Err(e) => {
                eprintln!("jq-rust: error: {}", e);
                *exit_code = 5;
            }
        }
    }

    if args.join_output {
        io::stdout().flush().ok();
    }

    Ok(())
}

fn process_input(
    args: &Args,
    print_options: &JvPrintOptions,
    expr: &jq_rust::Expr,
    exit_code: &mut i32,
) -> Result<(), String> {
    let stdin = io::stdin();

    for file in &args.files {
        let reader: Box<dyn BufRead> = if file == "-" {
            Box::new(stdin.lock())
        } else {
            let f =
                std::fs::File::open(file).map_err(|e| format!("cannot open '{}': {}", file, e))?;
            Box::new(io::BufReader::new(f))
        };

        if args.raw_input {
            // Read as raw string
            let mut content = String::new();
            let mut reader = reader;
            reader
                .read_to_string(&mut content)
                .map_err(|e| format!("read error: {}", e))?;

            if args.slurp {
                // Single string input
                run_filter(expr, Jv::string(&content), print_options, args, exit_code)?;
            } else {
                // Line by line
                for line in content.lines() {
                    run_filter(expr, Jv::string(line), print_options, args, exit_code)?;
                }
            }
        } else if args.slurp {
            // Slurp mode: read all JSON values into an array
            let mut content = String::new();
            let mut reader = reader;
            reader
                .read_to_string(&mut content)
                .map_err(|e| format!("read error: {}", e))?;

            let mut values = Vec::new();
            for result in parse_json_stream(&content) {
                let value = result.map_err(|e| format!("{}", e))?;
                values.push(value);
            }

            let arr = Jv::from_vec(values);
            run_filter(expr, arr, print_options, args, exit_code)?;
        } else {
            // Normal mode: process each JSON value
            let mut content = String::new();
            let mut reader = reader;
            reader
                .read_to_string(&mut content)
                .map_err(|e| format!("read error: {}", e))?;

            for result in parse_json_stream(&content) {
                let value = result.map_err(|e| format!("{}", e))?;
                run_filter(expr, value, print_options, args, exit_code)?;
            }
        }
    }

    Ok(())
}

trait ReadToString {
    fn read_to_string(&mut self, buf: &mut String) -> io::Result<usize>;
}

impl<R: BufRead> ReadToString for R {
    fn read_to_string(&mut self, buf: &mut String) -> io::Result<usize> {
        io::Read::read_to_string(self, buf)
    }
}
