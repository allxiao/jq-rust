# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-03-24

### Added
- Rich error messages with source context, matching jq's error format
  - Line and column numbers for all errors
  - Source line display with caret pointer (`^`) indicating error location
  - Human-readable token names (e.g., `'='` instead of `Eq`, `'}'` instead of `RBrace`)
- Pre-evaluation validation for undefined functions and variables
  - Errors reported in source order (left-to-right)
  - All errors collected and reported together, not just the first one
- New `SourceInfo` type for tracking source context through evaluation
- `interpret_with_source()` function for rich error messages in library usage

### Changed
- Error messages now align closely with jq's format for better familiarity
- Binary name in error output changed from `jq_rust` to `jqr`

### Example

Before (v0.1.0):
```
jqr: error: parse error at 2-3: expected RBrace, got Eq
```

After (v0.2.0):
```
jqr: error: syntax error, unexpected '=', expecting '}' at <top-level>, line 1, column 3:
    {a=1, b=2}
      ^
jqr: 1 compile error
```

## [0.1.0] - 2026-03-24

### Added
- Initial release of jq-rust (binary: `jqr`)
- Full jq compatibility (855/855 tests passing)
- All jq operators and built-in functions
- User-defined functions with lexical scoping
- Module system (import/include/modulemeta)
- Regular expression support including lookahead/lookbehind
- All format strings (@base64, @uri, @csv, @tsv, @html, @sh, @json)
- Streaming JSON input
- Command-line interface compatible with jq

### Performance
- 10-17% faster than jq C in most operations
- Optimized object construction for common cases
- Efficient reference counting with copy-on-write semantics

### Changed
- Project will not be published to crates.io (binary releases only)

[Unreleased]: https://github.com/allxiao/jq-rust/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/allxiao/jq-rust/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/allxiao/jq-rust/releases/tag/v0.1.0
