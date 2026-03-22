//! JSON parsing
//!
//! Parses JSON text into JV values.

use super::{Jv, JvArray, JvObject, JvNumber, JvString};
use crate::error::JqError;

/// Maximum nesting depth for parsing (matches jq's MAX_PARSING_DEPTH)
const MAX_PARSING_DEPTH: usize = 10000;

/// JSON parser
pub struct JsonParser<'a> {
    input: &'a [u8],
    pos: usize,
    depth: usize,
}

impl<'a> JsonParser<'a> {
    pub fn new(input: &'a str) -> Self {
        JsonParser {
            input: input.as_bytes(),
            pos: 0,
            depth: 0,
        }
    }

    /// Parse a single JSON value
    pub fn parse(&mut self) -> Result<Jv, JqError> {
        self.skip_whitespace();
        let value = self.parse_value()?;
        self.skip_whitespace();
        Ok(value)
    }

    /// Check if there's more input after the current position
    pub fn has_more(&self) -> bool {
        self.pos < self.input.len()
    }

    /// Get current position
    pub fn position(&self) -> usize {
        self.pos
    }

    fn parse_value(&mut self) -> Result<Jv, JqError> {
        self.skip_whitespace();

        match self.peek() {
            Some(b'n') => {
                // Could be null or NaN
                if self.starts_with("nan") || self.starts_with("NaN") {
                    self.parse_nan()
                } else {
                    self.parse_null()
                }
            }
            Some(b'N') => self.parse_nan(),
            Some(b't') => self.parse_true(),
            Some(b'f') => self.parse_false(),
            Some(b'"') => self.parse_string(),
            Some(b'[') => self.parse_array(),
            Some(b'{') => self.parse_object(),
            Some(b'-') => {
                // Could be negative number, -Infinity, or -NaN
                if self.starts_with("-Infinity") {
                    self.parse_neg_infinity()
                } else if self.starts_with("-NaN") || self.starts_with("-nan") {
                    self.parse_neg_nan()
                } else {
                    self.parse_number()
                }
            }
            Some(b'0'..=b'9') => self.parse_number(),
            Some(b'I') => self.parse_infinity(),
            Some(c) => Err(JqError::Parse(format!(
                "unexpected character '{}' at position {}",
                *c as char, self.pos
            ))),
            None => Err(JqError::Parse("unexpected end of input".to_string())),
        }
    }

    fn starts_with(&self, s: &str) -> bool {
        let bytes = s.as_bytes();
        self.input.get(self.pos..self.pos + bytes.len()) == Some(bytes)
    }

    fn parse_infinity(&mut self) -> Result<Jv, JqError> {
        self.expect_literal("Infinity")?;
        // jq represents Infinity as f64::MAX
        Ok(Jv::Number(JvNumber::from_f64(f64::MAX)))
    }

    fn parse_neg_infinity(&mut self) -> Result<Jv, JqError> {
        self.expect_literal("-Infinity")?;
        // jq represents -Infinity as -f64::MAX
        Ok(Jv::Number(JvNumber::from_f64(-f64::MAX)))
    }

    fn parse_nan(&mut self) -> Result<Jv, JqError> {
        // Accept both "nan" and "NaN"
        if self.starts_with("NaN") {
            self.expect_literal("NaN")?;
        } else {
            self.expect_literal("nan")?;
        }
        // jq represents NaN as null
        Ok(Jv::Null)
    }

    fn parse_neg_nan(&mut self) -> Result<Jv, JqError> {
        if self.starts_with("-NaN") {
            self.expect_literal("-NaN")?;
        } else {
            self.expect_literal("-nan")?;
        }
        // jq represents -NaN as null too
        Ok(Jv::Null)
    }

    fn parse_null(&mut self) -> Result<Jv, JqError> {
        self.expect_literal("null")?;
        Ok(Jv::Null)
    }

    fn parse_true(&mut self) -> Result<Jv, JqError> {
        self.expect_literal("true")?;
        Ok(Jv::Bool(true))
    }

    fn parse_false(&mut self) -> Result<Jv, JqError> {
        self.expect_literal("false")?;
        Ok(Jv::Bool(false))
    }

    fn parse_string(&mut self) -> Result<Jv, JqError> {
        let s = self.parse_string_value()?;
        Ok(Jv::String(JvString::new(s)))
    }

    fn parse_string_value(&mut self) -> Result<String, JqError> {
        self.expect(b'"')?;

        let mut s = String::new();

        loop {
            match self.next() {
                Some(b'"') => break,
                Some(b'\\') => {
                    let escaped = self.parse_escape()?;
                    s.push(escaped);
                }
                Some(c) if c < 0x20 => {
                    return Err(JqError::Parse(format!(
                        "control character in string at position {}",
                        self.pos
                    )));
                }
                Some(c) => {
                    // Handle UTF-8
                    if c < 0x80 {
                        s.push(c as char);
                    } else {
                        // Multi-byte UTF-8
                        self.pos -= 1;
                        let ch = self.parse_utf8_char()?;
                        s.push(ch);
                    }
                }
                None => {
                    return Err(JqError::Parse("unterminated string".to_string()));
                }
            }
        }

        Ok(s)
    }

    fn parse_escape(&mut self) -> Result<char, JqError> {
        match self.next() {
            Some(b'"') => Ok('"'),
            Some(b'\\') => Ok('\\'),
            Some(b'/') => Ok('/'),
            Some(b'b') => Ok('\x08'),
            Some(b'f') => Ok('\x0c'),
            Some(b'n') => Ok('\n'),
            Some(b'r') => Ok('\r'),
            Some(b't') => Ok('\t'),
            Some(b'u') => self.parse_unicode_escape(),
            Some(c) => Err(JqError::Parse(format!(
                "invalid escape sequence '\\{}' at position {}",
                c as char, self.pos
            ))),
            None => Err(JqError::Parse("unexpected end of escape sequence".to_string())),
        }
    }

    fn parse_unicode_escape(&mut self) -> Result<char, JqError> {
        let mut code_point = 0u32;

        for _ in 0..4 {
            let digit = self.next().ok_or_else(|| {
                JqError::Parse("incomplete unicode escape".to_string())
            })?;

            let value = match digit {
                b'0'..=b'9' => digit - b'0',
                b'a'..=b'f' => digit - b'a' + 10,
                b'A'..=b'F' => digit - b'A' + 10,
                _ => {
                    return Err(JqError::Parse(format!(
                        "invalid unicode escape digit '{}' at position {}",
                        digit as char, self.pos
                    )));
                }
            };

            code_point = code_point * 16 + value as u32;
        }

        // Handle surrogate pairs
        if (0xD800..=0xDBFF).contains(&code_point) {
            // High surrogate, expect low surrogate
            self.expect(b'\\')?;
            self.expect(b'u')?;

            let mut low = 0u32;
            for _ in 0..4 {
                let digit = self.next().ok_or_else(|| {
                    JqError::Parse("incomplete unicode escape".to_string())
                })?;

                let value = match digit {
                    b'0'..=b'9' => digit - b'0',
                    b'a'..=b'f' => digit - b'a' + 10,
                    b'A'..=b'F' => digit - b'A' + 10,
                    _ => {
                        return Err(JqError::Parse(format!(
                            "invalid unicode escape digit at position {}",
                            self.pos
                        )));
                    }
                };

                low = low * 16 + value as u32;
            }

            if !(0xDC00..=0xDFFF).contains(&low) {
                return Err(JqError::Parse("invalid surrogate pair".to_string()));
            }

            code_point = 0x10000 + ((code_point - 0xD800) << 10) + (low - 0xDC00);
        }

        char::from_u32(code_point).ok_or_else(|| {
            JqError::Parse(format!("invalid unicode code point U+{:04X}", code_point))
        })
    }

    fn parse_utf8_char(&mut self) -> Result<char, JqError> {
        let first = self.next().ok_or_else(|| {
            JqError::Parse("unexpected end of UTF-8 sequence".to_string())
        })?;

        let width = if first & 0x80 == 0 {
            1
        } else if first & 0xE0 == 0xC0 {
            2
        } else if first & 0xF0 == 0xE0 {
            3
        } else if first & 0xF8 == 0xF0 {
            4
        } else {
            return Err(JqError::Parse("invalid UTF-8 byte".to_string()));
        };

        let mut bytes = vec![first];
        for _ in 1..width {
            let b = self.next().ok_or_else(|| {
                JqError::Parse("incomplete UTF-8 sequence".to_string())
            })?;
            if b & 0xC0 != 0x80 {
                return Err(JqError::Parse("invalid UTF-8 continuation byte".to_string()));
            }
            bytes.push(b);
        }

        std::str::from_utf8(&bytes)
            .map_err(|_| JqError::Parse("invalid UTF-8".to_string()))?
            .chars()
            .next()
            .ok_or_else(|| JqError::Parse("empty UTF-8 sequence".to_string()))
    }

    fn parse_number(&mut self) -> Result<Jv, JqError> {
        let start = self.pos;

        // Optional minus
        if self.peek() == Some(&b'-') {
            self.pos += 1;
        }

        // Integer part
        match self.peek() {
            Some(b'0') => {
                self.pos += 1;
            }
            Some(b'1'..=b'9') => {
                self.pos += 1;
                while let Some(b'0'..=b'9') = self.peek() {
                    self.pos += 1;
                }
            }
            _ => {
                return Err(JqError::Parse(format!(
                    "invalid number at position {}",
                    self.pos
                )));
            }
        }

        // Fractional part
        if self.peek() == Some(&b'.') {
            self.pos += 1;
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(JqError::Parse(format!(
                    "expected digit after decimal point at position {}",
                    self.pos
                )));
            }
            while let Some(b'0'..=b'9') = self.peek() {
                self.pos += 1;
            }
        }

        // Exponent part
        if let Some(b'e') | Some(b'E') = self.peek() {
            self.pos += 1;
            if let Some(b'+') | Some(b'-') = self.peek() {
                self.pos += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(JqError::Parse(format!(
                    "expected digit in exponent at position {}",
                    self.pos
                )));
            }
            while let Some(b'0'..=b'9') = self.peek() {
                self.pos += 1;
            }
        }

        let num_str = std::str::from_utf8(&self.input[start..self.pos])
            .map_err(|_| JqError::Parse("invalid number encoding".to_string()))?;

        // Try parsing as integer first for precision
        if !num_str.contains('.') && !num_str.contains('e') && !num_str.contains('E') {
            if let Ok(i) = num_str.parse::<i64>() {
                return Ok(Jv::Number(JvNumber::from_i64(i)));
            }
        }

        // Parse as float
        let f: f64 = num_str.parse().map_err(|_| {
            JqError::Parse(format!("invalid number '{}'", num_str))
        })?;

        Ok(Jv::Number(JvNumber::from_f64(f)))
    }

    fn parse_array(&mut self) -> Result<Jv, JqError> {
        // Check depth limit
        if self.depth >= MAX_PARSING_DEPTH {
            return Err(JqError::Parse("Exceeds depth limit for parsing".to_string()));
        }
        self.depth += 1;

        self.expect(b'[')?;
        self.skip_whitespace();

        let mut arr = JvArray::new();

        if self.peek() == Some(&b']') {
            self.pos += 1;
            self.depth -= 1;
            return Ok(Jv::Array(arr));
        }

        loop {
            let value = self.parse_value()?;
            arr.push(value);

            self.skip_whitespace();

            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                    self.skip_whitespace();
                }
                Some(b']') => {
                    self.pos += 1;
                    break;
                }
                Some(c) => {
                    return Err(JqError::Parse(format!(
                        "expected ',' or ']' but found '{}' at position {}",
                        *c as char, self.pos
                    )));
                }
                None => {
                    return Err(JqError::Parse("unterminated array".to_string()));
                }
            }
        }

        self.depth -= 1;
        Ok(Jv::Array(arr))
    }

    fn parse_object(&mut self) -> Result<Jv, JqError> {
        // Check depth limit
        if self.depth >= MAX_PARSING_DEPTH {
            return Err(JqError::Parse("Exceeds depth limit for parsing".to_string()));
        }
        self.depth += 1;

        self.expect(b'{')?;
        self.skip_whitespace();

        let mut obj = JvObject::new();

        if self.peek() == Some(&b'}') {
            self.pos += 1;
            self.depth -= 1;
            return Ok(Jv::Object(obj));
        }

        loop {
            self.skip_whitespace();

            // Parse key
            if self.peek() != Some(&b'"') {
                return Err(JqError::Parse(format!(
                    "expected string key at position {}",
                    self.pos
                )));
            }
            let key = self.parse_string_value()?;

            self.skip_whitespace();
            self.expect(b':')?;

            // Parse value
            let value = self.parse_value()?;
            obj.set(&key, value);

            self.skip_whitespace();

            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                    self.skip_whitespace();
                }
                Some(b'}') => {
                    self.pos += 1;
                    break;
                }
                Some(c) => {
                    return Err(JqError::Parse(format!(
                        "expected ',' or '}}' but found '{}' at position {}",
                        *c as char, self.pos
                    )));
                }
                None => {
                    return Err(JqError::Parse("unterminated object".to_string()));
                }
            }
        }

        self.depth -= 1;

        Ok(Jv::Object(obj))
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            if matches!(c, b' ' | b'\t' | b'\n' | b'\r') {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<&u8> {
        self.input.get(self.pos)
    }

    fn next(&mut self) -> Option<u8> {
        if self.pos < self.input.len() {
            let c = self.input[self.pos];
            self.pos += 1;
            Some(c)
        } else {
            None
        }
    }

    fn expect(&mut self, expected: u8) -> Result<(), JqError> {
        match self.next() {
            Some(c) if c == expected => Ok(()),
            Some(c) => Err(JqError::Parse(format!(
                "expected '{}' but found '{}' at position {}",
                expected as char, c as char, self.pos - 1
            ))),
            None => Err(JqError::Parse(format!(
                "expected '{}' but found end of input",
                expected as char
            ))),
        }
    }

    fn expect_literal(&mut self, literal: &str) -> Result<(), JqError> {
        for expected in literal.bytes() {
            self.expect(expected)?;
        }
        Ok(())
    }
}

/// Parse a JSON string into a JV value
pub fn parse_json(input: &str) -> Result<Jv, JqError> {
    let mut parser = JsonParser::new(input);
    let value = parser.parse()?;

    // Check for trailing content
    if parser.has_more() {
        return Err(JqError::Parse(format!(
            "unexpected trailing content at position {}",
            parser.position()
        )));
    }

    Ok(value)
}

/// Parse multiple JSON values from input (for streaming)
pub fn parse_json_stream(input: &str) -> impl Iterator<Item = Result<Jv, JqError>> + '_ {
    let mut parser = JsonParser::new(input);

    std::iter::from_fn(move || {
        parser.skip_whitespace_pub();
        if !parser.has_more() {
            return None;
        }
        Some(parser.parse())
    })
}

impl JsonParser<'_> {
    fn skip_whitespace_pub(&mut self) {
        self.skip_whitespace();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_null() {
        assert_eq!(parse_json("null").unwrap(), Jv::Null);
    }

    #[test]
    fn test_parse_bool() {
        assert_eq!(parse_json("true").unwrap(), Jv::Bool(true));
        assert_eq!(parse_json("false").unwrap(), Jv::Bool(false));
    }

    #[test]
    fn test_parse_number() {
        assert_eq!(parse_json("42").unwrap(), Jv::from_i64(42));
        assert_eq!(parse_json("-42").unwrap(), Jv::from_i64(-42));
        assert_eq!(parse_json("3.14").unwrap(), Jv::from_f64(3.14));
        assert_eq!(parse_json("1e10").unwrap(), Jv::from_f64(1e10));
        assert_eq!(parse_json("1.5e-3").unwrap(), Jv::from_f64(1.5e-3));
    }

    #[test]
    fn test_parse_string() {
        assert_eq!(parse_json(r#""hello""#).unwrap(), Jv::string("hello"));
        assert_eq!(parse_json(r#""hello\nworld""#).unwrap(), Jv::string("hello\nworld"));
        assert_eq!(parse_json(r#""hello\\world""#).unwrap(), Jv::string("hello\\world"));
        assert_eq!(parse_json(r#""\u0041""#).unwrap(), Jv::string("A"));
    }

    #[test]
    fn test_parse_array() {
        let arr = parse_json("[1, 2, 3]").unwrap();
        assert!(arr.is_array());
        assert_eq!(arr.len(), Some(3));
    }

    #[test]
    fn test_parse_object() {
        let obj = parse_json(r#"{"a": 1, "b": 2}"#).unwrap();
        assert!(obj.is_object());
        assert_eq!(obj.get_field("a"), Jv::from_i64(1));
        assert_eq!(obj.get_field("b"), Jv::from_i64(2));
    }

    #[test]
    fn test_parse_nested() {
        let json = r#"{"array": [1, {"nested": true}], "value": null}"#;
        let v = parse_json(json).unwrap();
        assert!(v.is_object());
    }

    #[test]
    fn test_parse_whitespace() {
        assert_eq!(parse_json("  null  ").unwrap(), Jv::Null);
        assert_eq!(parse_json("[\n  1,\n  2\n]").unwrap().len(), Some(2));
    }

    #[test]
    fn test_parse_error() {
        assert!(parse_json("").is_err());
        assert!(parse_json("[1, 2").is_err());
        assert!(parse_json("{invalid}").is_err());
    }
}
