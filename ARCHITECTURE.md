# jq-rust Architecture

## Overview

This document describes the architecture for converting jq from C to Rust. The architecture closely follows the original C implementation while leveraging Rust's safety features and idioms.

## Module Structure

```
jq-rust/
├── src/
│   ├── main.rs              # CLI entry point
│   ├── lib.rs               # Library root, public API
│   │
│   ├── jv/                  # JSON Value module (corresponds to jv.c, jv.h)
│   │   ├── mod.rs           # Module exports
│   │   ├── value.rs         # JV enum and core types
│   │   ├── number.rs        # Number handling
│   │   ├── string.rs        # String operations
│   │   ├── array.rs         # Array operations
│   │   ├── object.rs        # Object operations
│   │   └── iter.rs          # Iterator implementations
│   │
│   ├── parser/              # Lexer and Parser (corresponds to lexer.l, parser.y)
│   │   ├── mod.rs
│   │   ├── lexer.rs         # Tokenizer
│   │   ├── token.rs         # Token definitions
│   │   ├── parser.rs        # Parser (recursive descent or parser combinator)
│   │   └── ast.rs           # Abstract Syntax Tree
│   │
│   ├── compiler/            # Compiler (corresponds to compile.c)
│   │   ├── mod.rs
│   │   ├── compiler.rs      # AST to bytecode compiler
│   │   ├── bytecode.rs      # Bytecode definitions
│   │   └── opcode.rs        # Opcode definitions
│   │
│   ├── vm/                  # Virtual Machine (corresponds to execute.c)
│   │   ├── mod.rs
│   │   ├── executor.rs      # Bytecode interpreter
│   │   ├── stack.rs         # Value stack
│   │   └── frame.rs         # Call frames
│   │
│   ├── builtins/            # Built-in functions (corresponds to builtin.c)
│   │   ├── mod.rs
│   │   ├── core.rs          # Core functions (type, empty, error, etc.)
│   │   ├── math.rs          # Math functions
│   │   ├── string.rs        # String functions
│   │   ├── array.rs         # Array functions
│   │   ├── object.rs        # Object functions
│   │   ├── datetime.rs      # Date/time functions
│   │   └── format.rs        # Format functions (@base64, @uri, etc.)
│   │
│   ├── io/                  # Input/Output handling
│   │   ├── mod.rs
│   │   ├── json_parse.rs    # JSON parsing (corresponds to jv_parse.c)
│   │   ├── json_print.rs    # JSON printing (corresponds to jv_print.c)
│   │   └── stream.rs        # Streaming JSON parser
│   │
│   └── util/                # Utilities
│       ├── mod.rs
│       ├── unicode.rs       # Unicode/UTF-8 handling
│       └── error.rs         # Error types
│
├── tests/                   # Integration tests
│   ├── jq_tests.rs          # Ported from jq.test
│   └── ...
│
└── benches/                 # Benchmarks
    └── ...
```

## Core Types

### JV (JSON Value)

The central data type, equivalent to `jv` in C:

```rust
pub enum Jv {
    Null,
    Bool(bool),
    Number(JvNumber),
    String(JvString),
    Array(JvArray),
    Object(JvObject),
    Invalid(Option<Box<JvError>>),
}
```

Key differences from C:
- Use Rust enums instead of tagged unions
- Use `Rc<RefCell<>>` or `Arc<>` for reference counting instead of manual refcounting
- Leverage Rust's ownership system for memory safety

### Token

```rust
pub enum Token {
    // Literals
    Null,
    True,
    False,
    Number(f64),
    String(String),

    // Operators
    Pipe,           // |
    Dot,            // .
    DotDot,         // ..
    Comma,          // ,
    Colon,          // :
    Semicolon,      // ;
    // ... more operators

    // Keywords
    If, Then, Else, End,
    As, Def, Reduce, Foreach,
    Try, Catch,
    Import, Include,
    // ... more keywords

    // Identifiers
    Ident(String),
    Field(String),
    Format(String),

    // Special
    Eof,
}
```

### AST

```rust
pub enum Expr {
    Identity,                           // .
    RecursiveDescent,                   // ..
    Literal(Jv),                        // null, true, false, numbers, strings
    Field(String),                      // .foo
    Index(Box<Expr>),                   // .[expr]
    Slice(Option<Box<Expr>>, Option<Box<Expr>>),  // .[start:end]
    Iterator,                           // .[]
    Pipe(Box<Expr>, Box<Expr>),         // expr | expr
    Comma(Box<Expr>, Box<Expr>),        // expr, expr
    Conditional(Box<Expr>, Box<Expr>, Option<Box<Expr>>),  // if-then-else
    TryCatch(Box<Expr>, Option<Box<Expr>>),  // try-catch
    Reduce { ... },
    Foreach { ... },
    FunctionDef { ... },
    FunctionCall { ... },
    // ... more variants
}
```

### Bytecode/Opcode

```rust
pub enum Opcode {
    // Stack operations
    LoadK(u16),     // Load constant
    Dup,
    Pop,

    // Variable operations
    LoadV(u16),
    StoreV(u16),

    // Path operations
    Index,
    Each,
    Path,

    // Control flow
    Jump(i16),
    JumpF(i16),     // Jump if false
    Fork(i16),

    // Function calls
    CallBuiltin(u16, u8),  // builtin_id, arity
    CallJq(u16),           // function_id
    Ret,

    // Special
    TryBegin(i16),
    TryEnd,
    // ... more opcodes
}
```

## Execution Model

1. **Parse**: jq filter string → Token stream → AST
2. **Compile**: AST → Bytecode
3. **Execute**: Bytecode + Input JSON → Output JSON(s)

The executor is a stack-based virtual machine that processes bytecode instructions.

## Dependencies

### Required
- `serde` + `serde_json` - JSON parsing/serialization (or custom implementation)
- `clap` - CLI argument parsing
- `thiserror` - Error handling

### Optional
- `regex` or `oniguruma` - Regular expression support
- `chrono` - Date/time handling
- `num-bigint` - Arbitrary precision numbers (for `--enable-decnum`)

## Compatibility Goals

1. **Language compatibility**: Support all jq language features
2. **CLI compatibility**: Match jq's command-line interface
3. **Behavior compatibility**: Match jq's output for all inputs
4. **Test compatibility**: Pass all tests from jq test suite

## Implementation Strategy

### Phase 1: Foundation
- Implement JV type with basic operations
- JSON parsing and printing
- Basic CLI structure

### Phase 2: Lexer & Parser
- Tokenizer for jq syntax
- Parser producing AST
- Support for basic expressions

### Phase 3: Compiler
- Define bytecode format
- Compile AST to bytecode
- Handle variable scoping

### Phase 4: Executor
- Stack-based VM
- Basic operations (identity, field access, pipes)
- Iteration support

### Phase 5: Built-in Functions
- Implement built-ins incrementally
- Start with most-used functions
- Add tests for each

### Phase 6: Advanced Features
- User-defined functions
- Module system
- Try-catch error handling
- Streaming parser

### Phase 7: Polish
- Full CLI compatibility
- Performance optimization
- Documentation

## Testing Strategy

1. **Unit tests**: Test individual components
2. **Integration tests**: Port tests from jq.test
3. **Compatibility tests**: Compare output with C jq
4. **Fuzzing**: Use fuzzing to find edge cases

## Performance Considerations

- Use string interning for common strings
- Consider arena allocation for JV values
- Profile and optimize hot paths
- Consider SIMD for JSON parsing
