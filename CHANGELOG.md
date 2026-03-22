# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2024-03-22

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

[Unreleased]: https://github.com/allxiao/jq-rust/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/allxiao/jq-rust/releases/tag/v0.1.0
