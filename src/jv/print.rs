//! JSON printing/output
//!
//! Converts JV values to JSON text with various formatting options.

use std::fmt::Write;

use super::Jv;

/// Maximum nesting depth for printing (matches jq's MAX_PRINT_DEPTH)
const MAX_PRINT_DEPTH: usize = 10000;

/// Options for JSON output formatting
#[derive(Debug, Clone)]
pub struct JvPrintOptions {
    /// Pretty-print with indentation
    pub pretty: bool,
    /// Indentation string (default: "  ")
    pub indent: String,
    /// Sort object keys
    pub sort_keys: bool,
    /// Use tabs instead of spaces for indentation
    pub use_tabs: bool,
    /// Indent level (for internal use)
    pub indent_level: usize,
    /// Output ASCII only (escape non-ASCII chars)
    pub ascii_output: bool,
    /// Raw string output (no quotes for strings)
    pub raw_output: bool,
    /// Join output with newlines
    pub join_output: bool,
    /// Use colors for output (not implemented yet)
    pub color: bool,
    /// Current recursion depth (internal use)
    pub depth: usize,
}

impl Default for JvPrintOptions {
    fn default() -> Self {
        JvPrintOptions {
            pretty: false,
            indent: "  ".to_string(),
            sort_keys: false,
            use_tabs: false,
            indent_level: 0,
            ascii_output: false,
            raw_output: false,
            join_output: false,
            color: false,
            depth: 0,
        }
    }
}

impl JvPrintOptions {
    pub fn compact() -> Self {
        JvPrintOptions::default()
    }

    pub fn pretty() -> Self {
        JvPrintOptions {
            pretty: true,
            ..Default::default()
        }
    }

    pub fn with_indent(mut self, indent: &str) -> Self {
        self.indent = indent.to_string();
        self
    }

    pub fn with_sort_keys(mut self, sort: bool) -> Self {
        self.sort_keys = sort;
        self
    }

    fn current_indent(&self) -> String {
        if self.use_tabs {
            "\t".repeat(self.indent_level)
        } else {
            self.indent.repeat(self.indent_level)
        }
    }

    fn nested(&self) -> Self {
        JvPrintOptions {
            indent_level: self.indent_level + 1,
            depth: self.depth + 1,
            ..self.clone()
        }
    }
}

/// Print a JV value to a string with default options
pub fn print_jv(value: &Jv) -> String {
    print_jv_with_options(value, &JvPrintOptions::default())
}

/// Print a JV value to a string with custom options
pub fn print_jv_with_options(value: &Jv, options: &JvPrintOptions) -> String {
    let mut output = String::new();
    write_jv(&mut output, value, options).unwrap();
    output
}

/// Write a JV value to a writer
fn write_jv<W: Write>(w: &mut W, value: &Jv, options: &JvPrintOptions) -> std::fmt::Result {
    match value {
        Jv::Null => write!(w, "null"),
        Jv::Bool(true) => write!(w, "true"),
        Jv::Bool(false) => write!(w, "false"),
        Jv::Number(n) => write!(w, "{}", n),
        Jv::LiteralNumber(s) => write!(w, "{}", s), // Output the literal string as-is
        Jv::String(s) => {
            if options.raw_output {
                write!(w, "{}", s.as_str())
            } else {
                write_string(w, s.as_str(), options)
            }
        }
        Jv::Array(arr) => write_array(w, arr, options),
        Jv::Object(obj) => write_object(w, obj, options),
        Jv::Invalid(Some(e)) => write!(w, "<error: {}>", e),
        Jv::Invalid(None) => write!(w, "<invalid>"),
    }
}

/// Write a JSON string with proper escaping
fn write_string<W: Write>(w: &mut W, s: &str, options: &JvPrintOptions) -> std::fmt::Result {
    w.write_char('"')?;

    for ch in s.chars() {
        match ch {
            '"' => w.write_str("\\\"")?,
            '\\' => w.write_str("\\\\")?,
            '\x08' => w.write_str("\\b")?,
            '\x0c' => w.write_str("\\f")?,
            '\n' => w.write_str("\\n")?,
            '\r' => w.write_str("\\r")?,
            '\t' => w.write_str("\\t")?,
            c if c < '\x20' => {
                // Control characters
                write!(w, "\\u{:04x}", c as u32)?;
            }
            c if options.ascii_output && c > '\x7f' => {
                // Non-ASCII characters
                if c as u32 <= 0xFFFF {
                    write!(w, "\\u{:04x}", c as u32)?;
                } else {
                    // Surrogate pair for characters outside BMP
                    let code = c as u32 - 0x10000;
                    let high = 0xD800 + (code >> 10);
                    let low = 0xDC00 + (code & 0x3FF);
                    write!(w, "\\u{:04x}\\u{:04x}", high, low)?;
                }
            }
            c => w.write_char(c)?,
        }
    }

    w.write_char('"')
}

/// Write a JSON array
fn write_array<W: Write>(
    w: &mut W,
    arr: &super::JvArray,
    options: &JvPrintOptions,
) -> std::fmt::Result {
    // Check depth limit - jq uses > (not >=) so depth 10000 is allowed
    if options.depth > MAX_PRINT_DEPTH {
        return w.write_str("<skipped: too deep>");
    }

    w.write_char('[')?;

    let items: Vec<Jv> = arr.iter().collect();
    let nested_options = options.nested();

    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            w.write_char(',')?;
        }

        if options.pretty {
            w.write_char('\n')?;
            w.write_str(&nested_options.current_indent())?;
        }

        write_jv(w, item, &nested_options)?;
    }

    if options.pretty && !items.is_empty() {
        w.write_char('\n')?;
        w.write_str(&options.current_indent())?;
    }

    w.write_char(']')
}

/// Write a JSON object
fn write_object<W: Write>(
    w: &mut W,
    obj: &super::JvObject,
    options: &JvPrintOptions,
) -> std::fmt::Result {
    // Check depth limit - jq uses > (not >=) so depth 10000 is allowed
    if options.depth > MAX_PRINT_DEPTH {
        return w.write_str("<skipped: too deep>");
    }

    w.write_char('{')?;

    let mut entries: Vec<(String, Jv)> = obj.iter().collect();

    if options.sort_keys {
        entries.sort_by(|a, b| a.0.cmp(&b.0));
    }

    let nested_options = options.nested();

    for (i, (key, value)) in entries.iter().enumerate() {
        if i > 0 {
            w.write_char(',')?;
        }

        if options.pretty {
            w.write_char('\n')?;
            w.write_str(&nested_options.current_indent())?;
        }

        write_string(w, key, options)?;
        w.write_char(':')?;

        if options.pretty {
            w.write_char(' ')?;
        }

        write_jv(w, value, &nested_options)?;
    }

    if options.pretty && !entries.is_empty() {
        w.write_char('\n')?;
        w.write_str(&options.current_indent())?;
    }

    w.write_char('}')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jv::{JvArray, JvObject};

    #[test]
    fn test_print_null() {
        assert_eq!(print_jv(&Jv::Null), "null");
    }

    #[test]
    fn test_print_bool() {
        assert_eq!(print_jv(&Jv::Bool(true)), "true");
        assert_eq!(print_jv(&Jv::Bool(false)), "false");
    }

    #[test]
    fn test_print_number() {
        assert_eq!(print_jv(&Jv::from_i64(42)), "42");
        assert_eq!(print_jv(&Jv::from_f64(3.14)), "3.14");
    }

    #[test]
    fn test_print_string() {
        assert_eq!(print_jv(&Jv::string("hello")), "\"hello\"");
        assert_eq!(print_jv(&Jv::string("hello\nworld")), "\"hello\\nworld\"");
        assert_eq!(print_jv(&Jv::string("quote: \"")), "\"quote: \\\"\"");
    }

    #[test]
    fn test_print_array() {
        let arr = JvArray::from_vec(vec![
            Jv::from_i64(1),
            Jv::from_i64(2),
            Jv::from_i64(3),
        ]);
        assert_eq!(print_jv(&Jv::Array(arr)), "[1,2,3]");
    }

    #[test]
    fn test_print_object() {
        let mut obj = JvObject::new();
        obj.set("a", Jv::from_i64(1));
        // Note: BTreeMap keeps keys sorted, so output is deterministic
        assert!(print_jv(&Jv::Object(obj)).contains("\"a\":1"));
    }

    #[test]
    fn test_print_pretty() {
        let arr = JvArray::from_vec(vec![
            Jv::from_i64(1),
            Jv::from_i64(2),
        ]);
        let output = print_jv_with_options(&Jv::Array(arr), &JvPrintOptions::pretty());
        assert!(output.contains('\n'));
        assert!(output.contains("  ")); // Default indent
    }

    #[test]
    fn test_print_nested() {
        let inner = JvArray::from_vec(vec![Jv::from_i64(1), Jv::from_i64(2)]);
        let mut obj = JvObject::new();
        obj.set("arr", Jv::Array(inner));
        obj.set("val", Jv::Bool(true));

        let json = print_jv(&Jv::Object(obj));
        assert!(json.contains("[1,2]"));
    }

    #[test]
    fn test_print_escape() {
        let options = JvPrintOptions {
            ascii_output: true,
            ..Default::default()
        };
        let output = print_jv_with_options(&Jv::string("日本語"), &options);
        assert!(output.contains("\\u"));
    }

    #[test]
    fn test_print_raw() {
        let options = JvPrintOptions {
            raw_output: true,
            ..Default::default()
        };
        assert_eq!(
            print_jv_with_options(&Jv::string("hello"), &options),
            "hello"
        );
    }
}
