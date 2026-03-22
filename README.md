# jq-rust

[![CI](https://github.com/allxiao/jq-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/allxiao/jq-rust/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/jq-rust.svg)](https://crates.io/crates/jq-rust)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

**jq-rust** is a complete Rust implementation of [jq](https://jqlang.org/), the lightweight and flexible command-line JSON processor. The binary is named `jqr` for a shorter command.

## Features

- 🚀 **Fast** - Often 10-17% faster than the original jq implementation
- ✅ **Compatible** - 100% test compatibility with jq (855/855 tests passing)
- 🦀 **Pure Rust** - No C dependencies, easy to install and deploy
- 📦 **Single Binary** - Statically linked, works anywhere

## Installation

### From Releases

Download the latest binary for your platform from the [releases page](https://github.com/allxiao/jq-rust/releases).

### From Source

```bash
cargo install jq-rust
```

Or build from source:

```bash
git clone https://github.com/allxiao/jq-rust.git
cd jq-rust
cargo build --release
# Binary will be at ./target/release/jqr
```

## Usage

jqr is a drop-in replacement for jq. All jq filters and options work the same way:

```bash
# Basic usage
echo '{"name": "John", "age": 30}' | jqr '.name'
# Output: "John"

# Pretty print JSON
echo '{"a":1,"b":2}' | jqr '.'

# Compact output
echo '{"a": 1}' | jqr -c '.'

# Raw string output
echo '{"name": "John"}' | jqr -r '.name'
# Output: John

# Process files
jqr '.[] | select(.active)' data.json

# Multiple filters
echo '[1,2,3,4,5]' | jqr 'map(. * 2) | add'
# Output: 30
```

## Command Line Options

```
jqr [OPTIONS] [FILTER] [FILES]...

Arguments:
  [FILTER]    The jq filter to apply [default: .]
  [FILES]...  Input files (use '-' for stdin) [default: -]

Options:
  -c, --compact-output    Compact output
  -r, --raw-output        Raw output (strings without quotes)
  -s, --slurp             Read entire input into array
  -n, --null-input        Don't read any input
  -e, --exit-status       Set exit status based on output
  -S, --sort-keys         Sort object keys
      --tab               Use tabs for indentation
  -C, --color-output      Colorize output (default when stdout is a tty)
  -M, --monochrome-output Disable colored output
  -a, --ascii-output      ASCII output (escape non-ASCII characters)
  -R, --raw-input         Read input as raw strings
  -h, --help              Print help
  -V, --version           Print version
```

## Performance

jqr is faster than the original C implementation in most benchmarks:

| Operation | jqr vs jq C |
|-----------|-------------|
| Array Iteration | 17% faster |
| Sort | 17% faster |
| Identity | 16% faster |
| Arithmetic | 15% faster |
| Map | 13% faster |
| Object Construction | 10% faster |

See [PERFORMANCE.md](PERFORMANCE.md) for detailed benchmarks.

## Compatibility

jqr passes 100% of jq's official test suite (855 tests). It supports:

- All jq operators and filters
- User-defined functions
- Module system (import/include)
- Regular expressions (including lookahead/lookbehind)
- Streaming JSON input
- All format strings (@base64, @uri, @csv, @html, etc.)

## Library Usage

jq-rust can also be used as a Rust library:

```rust
use jq_rust::{parse, interpret, Jv};
use jq_rust::jv::parse_json;

fn main() {
    // Parse a jq filter
    let filter = parse(".name").unwrap();

    // Parse JSON input
    let input = parse_json(r#"{"name": "John", "age": 30}"#).unwrap();

    // Run the filter
    for result in interpret(&filter, input) {
        match result {
            Ok(value) => println!("{}", value),
            Err(e) => eprintln!("Error: {}", e),
        }
    }
}
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- The [jq project](https://github.com/jqlang/jq) for the original implementation and test suite
- The Rust community for excellent crates like `regex`, `clap`, and `chrono`
