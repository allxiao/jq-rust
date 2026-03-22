//! Lexer for jq filter language

use super::token::{Span, Token, TokenKind};

/// Lexer state for handling strings with interpolation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LexerState {
    /// Normal parsing state
    Normal,
    /// Inside a string literal
    InString,
    /// Inside string interpolation (after \( )
    InStringInterp,
}

/// Lexer for jq filter expressions
pub struct Lexer<'a> {
    input: &'a [u8],
    pos: usize,
    /// Stack of states for nested string interpolation
    state_stack: Vec<LexerState>,
    /// Stack of parenthesis counts for each interpolation level
    paren_stack: Vec<usize>,
}

impl<'a> Lexer<'a> {
    /// Create a new lexer for the given input
    pub fn new(input: &'a str) -> Self {
        Lexer {
            input: input.as_bytes(),
            pos: 0,
            state_stack: vec![LexerState::Normal],
            paren_stack: vec![],
        }
    }

    /// Get current state
    fn state(&self) -> LexerState {
        *self.state_stack.last().unwrap_or(&LexerState::Normal)
    }

    /// Push a new state
    fn push_state(&mut self, state: LexerState) {
        self.state_stack.push(state);
    }

    /// Pop the current state
    fn pop_state(&mut self) {
        if self.state_stack.len() > 1 {
            self.state_stack.pop();
        }
    }

    /// Peek at the current character
    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    /// Peek at the next character
    fn peek_next(&self) -> Option<u8> {
        self.input.get(self.pos + 1).copied()
    }

    /// Advance and return the current character
    fn advance(&mut self) -> Option<u8> {
        let c = self.peek()?;
        self.pos += 1;
        Some(c)
    }

    /// Check if at end of input
    fn is_at_end(&self) -> bool {
        self.pos >= self.input.len()
    }

    /// Skip whitespace and comments
    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            match c {
                b' ' | b'\t' | b'\n' | b'\r' => {
                    self.advance();
                }
                b'#' => {
                    // Comment until end of line
                    while let Some(c) = self.peek() {
                        if c == b'\n' {
                            break;
                        }
                        self.advance();
                    }
                }
                _ => break,
            }
        }
    }

    /// Scan the next token
    pub fn next_token(&mut self) -> Token {
        // Handle string state specially
        if self.state() == LexerState::InString {
            return self.scan_string_content();
        }

        self.skip_whitespace();

        let start = self.pos;

        if self.is_at_end() {
            return Token::new(TokenKind::Eof, Span::new(start, start));
        }

        let c = self.advance().unwrap();

        let kind = match c {
            // Single-character tokens (that can't start multi-char tokens)
            b',' => TokenKind::Comma,
            b';' => TokenKind::Semicolon,
            b'(' => {
                if self.state() == LexerState::InStringInterp {
                    if let Some(count) = self.paren_stack.last_mut() {
                        *count += 1;
                    }
                }
                TokenKind::LParen
            }
            b')' => {
                // Check if this closes a string interpolation
                if self.state() == LexerState::InStringInterp {
                    if let Some(count) = self.paren_stack.last_mut() {
                        if *count == 0 {
                            // This closes the interpolation
                            self.paren_stack.pop();
                            self.pop_state();
                            return Token::new(
                                TokenKind::StringInterpEnd,
                                Span::new(start, self.pos),
                            );
                        } else {
                            *count -= 1;
                        }
                    }
                }
                TokenKind::RParen
            }
            b'[' => TokenKind::LBracket,
            b']' => TokenKind::RBracket,
            b'{' => TokenKind::LBrace,
            b'}' => TokenKind::RBrace,

            // Operators that might be multi-character
            b'.' => {
                if self.peek() == Some(b'.') {
                    self.advance();
                    TokenKind::DotDot
                } else if self.peek().map_or(false, is_ident_start) {
                    // Field access: .fieldname
                    let field_start = self.pos;
                    while self.peek().map_or(false, is_ident_continue) {
                        self.advance();
                    }
                    let name =
                        String::from_utf8_lossy(&self.input[field_start..self.pos]).to_string();
                    TokenKind::Field(name)
                } else {
                    TokenKind::Dot
                }
            }
            b'|' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    TokenKind::PipeEq
                } else {
                    TokenKind::Pipe
                }
            }
            b':' => {
                if self.peek() == Some(b':') {
                    self.advance();
                    TokenKind::DoubleColon
                } else {
                    TokenKind::Colon
                }
            }
            b'?' => {
                if self.peek() == Some(b'/') && self.peek_next() == Some(b'/') {
                    self.advance();
                    self.advance();
                    TokenKind::QuestionDoubleSlash
                } else {
                    TokenKind::Question
                }
            }
            b'=' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    TokenKind::EqEq
                } else {
                    TokenKind::Eq
                }
            }
            b'+' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    TokenKind::PlusEq
                } else {
                    TokenKind::Plus
                }
            }
            b'-' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    TokenKind::MinusEq
                } else {
                    TokenKind::Minus
                }
            }
            b'*' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    TokenKind::StarEq
                } else {
                    TokenKind::Star
                }
            }
            b'/' => {
                if self.peek() == Some(b'/') {
                    self.advance();
                    if self.peek() == Some(b'=') {
                        self.advance();
                        TokenKind::DoubleSlashEq
                    } else {
                        TokenKind::DoubleSlash
                    }
                } else if self.peek() == Some(b'=') {
                    self.advance();
                    TokenKind::SlashEq
                } else {
                    TokenKind::Slash
                }
            }
            b'%' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    TokenKind::PercentEq
                } else {
                    TokenKind::Percent
                }
            }
            b'<' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    TokenKind::LtEq
                } else {
                    TokenKind::Lt
                }
            }
            b'>' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    TokenKind::GtEq
                } else {
                    TokenKind::Gt
                }
            }
            b'!' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    TokenKind::NotEq
                } else {
                    TokenKind::Invalid('!')
                }
            }

            // String start
            b'"' => {
                self.push_state(LexerState::InString);
                TokenKind::StringStart
            }

            // Variable binding: $name or $__loc__
            b'$' => {
                if self.input[self.pos..].starts_with(b"__loc__") {
                    self.pos += 7;
                    TokenKind::Loc
                } else if self.peek().map_or(false, is_ident_start) {
                    let name_start = self.pos;
                    while self.peek().map_or(false, is_ident_continue) {
                        self.advance();
                    }
                    let name =
                        String::from_utf8_lossy(&self.input[name_start..self.pos]).to_string();
                    TokenKind::Binding(name)
                } else {
                    TokenKind::Dollar
                }
            }

            // Format: @name
            b'@' => {
                if self.peek().map_or(false, is_ident_start) {
                    let name_start = self.pos;
                    while self
                        .peek()
                        .map_or(false, |c| is_ident_continue(c) || c == b'-')
                    {
                        self.advance();
                    }
                    let name =
                        String::from_utf8_lossy(&self.input[name_start..self.pos]).to_string();
                    TokenKind::Format(name)
                } else {
                    TokenKind::Invalid('@')
                }
            }

            // Numbers
            c if c.is_ascii_digit() => {
                self.pos -= 1; // Back up to re-read the first digit
                self.scan_number()
            }

            // Identifiers and keywords
            c if is_ident_start(c) => {
                let ident_start = start;
                while self.peek().map_or(false, is_ident_continue) {
                    self.advance();
                }
                // Handle namespaced identifiers (foo::bar)
                while self.peek() == Some(b':') && self.peek_next() == Some(b':') {
                    self.advance(); // skip first :
                    self.advance(); // skip second :
                    while self.peek().map_or(false, is_ident_continue) {
                        self.advance();
                    }
                }
                let name = String::from_utf8_lossy(&self.input[ident_start..self.pos]).to_string();

                // Check for keywords
                TokenKind::keyword_from_ident(&name).unwrap_or(TokenKind::Ident(name))
            }

            // Unknown character
            _ => TokenKind::Invalid(c as char),
        };

        Token::new(kind, Span::new(start, self.pos))
    }

    /// Scan string content (after the opening quote)
    fn scan_string_content(&mut self) -> Token {
        let start = self.pos;

        match self.peek() {
            None => Token::new(
                TokenKind::Error("unterminated string".to_string()),
                Span::new(start, self.pos),
            ),
            Some(b'"') => {
                self.advance();
                self.pop_state();
                Token::new(TokenKind::StringEnd, Span::new(start, self.pos))
            }
            Some(b'\\') if self.peek_next() == Some(b'(') => {
                // String interpolation start
                self.advance(); // consume \
                self.advance(); // consume (
                self.push_state(LexerState::InStringInterp);
                self.paren_stack.push(0);
                Token::new(TokenKind::StringInterpStart, Span::new(start, self.pos))
            }
            _ => {
                // Collect all text (including escapes) until " or \(
                let mut text = String::new();

                loop {
                    match self.peek() {
                        None | Some(b'"') => break,
                        Some(b'\\') if self.peek_next() == Some(b'(') => break,
                        Some(b'\\') => {
                            self.advance();
                            match self.peek() {
                                Some(b'n') => {
                                    self.advance();
                                    text.push('\n');
                                }
                                Some(b'r') => {
                                    self.advance();
                                    text.push('\r');
                                }
                                Some(b't') => {
                                    self.advance();
                                    text.push('\t');
                                }
                                Some(b'\\') => {
                                    self.advance();
                                    text.push('\\');
                                }
                                Some(b'"') => {
                                    self.advance();
                                    text.push('"');
                                }
                                Some(b'/') => {
                                    self.advance();
                                    text.push('/');
                                }
                                Some(b'b') => {
                                    self.advance();
                                    text.push('\x08');
                                }
                                Some(b'f') => {
                                    self.advance();
                                    text.push('\x0c');
                                }
                                Some(b'u') => {
                                    self.advance();
                                    // Unicode escape
                                    if let Some(ch) = self.scan_unicode_escape() {
                                        text.push(ch);
                                    } else {
                                        return Token::new(
                                            TokenKind::Error("invalid unicode escape".to_string()),
                                            Span::new(start, self.pos),
                                        );
                                    }
                                }
                                Some(c) => {
                                    return Token::new(
                                        TokenKind::Error(format!("invalid escape \\{}", c as char)),
                                        Span::new(start, self.pos),
                                    );
                                }
                                None => {
                                    return Token::new(
                                        TokenKind::Error("unterminated escape".to_string()),
                                        Span::new(start, self.pos),
                                    );
                                }
                            }
                        }
                        Some(c) if c < 0x80 => {
                            self.advance();
                            text.push(c as char);
                        }
                        Some(_) => {
                            // Multi-byte UTF-8
                            let remaining = &self.input[self.pos..];
                            if let Ok(s) = std::str::from_utf8(remaining) {
                                if let Some(ch) = s.chars().next() {
                                    text.push(ch);
                                    self.pos += ch.len_utf8();
                                } else {
                                    break;
                                }
                            } else {
                                self.advance();
                            }
                        }
                    }
                }

                Token::new(TokenKind::StringText(text), Span::new(start, self.pos))
            }
        }
    }

    /// Scan a unicode escape sequence (\uXXXX)
    fn scan_unicode_escape(&mut self) -> Option<char> {
        let mut value = 0u32;
        for _ in 0..4 {
            let digit = self.advance()?;
            let hex = match digit {
                b'0'..=b'9' => digit - b'0',
                b'a'..=b'f' => digit - b'a' + 10,
                b'A'..=b'F' => digit - b'A' + 10,
                _ => return None,
            };
            value = value * 16 + hex as u32;
        }

        // Handle surrogate pairs
        if (0xD800..=0xDBFF).contains(&value) {
            // High surrogate - expect \uXXXX for low surrogate
            if self.peek() != Some(b'\\') || self.input.get(self.pos + 1) != Some(&b'u') {
                return None;
            }
            self.advance(); // skip \
            self.advance(); // skip u

            let mut low = 0u32;
            for _ in 0..4 {
                let digit = self.advance()?;
                let hex = match digit {
                    b'0'..=b'9' => digit - b'0',
                    b'a'..=b'f' => digit - b'a' + 10,
                    b'A'..=b'F' => digit - b'A' + 10,
                    _ => return None,
                };
                low = low * 16 + hex as u32;
            }

            if !(0xDC00..=0xDFFF).contains(&low) {
                return None;
            }

            value = 0x10000 + ((value - 0xD800) << 10) + (low - 0xDC00);
        }

        char::from_u32(value)
    }

    /// Scan a number literal
    fn scan_number(&mut self) -> TokenKind {
        let start = self.pos;

        // Integer part
        while self.peek().map_or(false, |c| c.is_ascii_digit()) {
            self.advance();
        }

        // Check for decimal point
        if self.peek() == Some(b'.') && self.peek_next().map_or(false, |c| c.is_ascii_digit()) {
            self.advance(); // consume .
            while self.peek().map_or(false, |c| c.is_ascii_digit()) {
                self.advance();
            }
        }

        // Check for exponent
        let has_exponent = matches!(self.peek(), Some(b'e') | Some(b'E'));
        if has_exponent {
            self.advance();
            if let Some(b'+') | Some(b'-') = self.peek() {
                self.advance();
            }
            while self.peek().map_or(false, |c| c.is_ascii_digit()) {
                self.advance();
            }
        }

        let num_str = std::str::from_utf8(&self.input[start..self.pos]).unwrap_or("0");

        // Check if this has an extreme exponent that would overflow/underflow f64
        // f64 exponent range is roughly -308 to +308
        if has_exponent {
            if let Some(exp) = extract_exponent(num_str) {
                if exp > 308 || exp < -308 {
                    // Return as literal number - normalize to jq's format
                    return TokenKind::LiteralNumber(normalize_literal_number(num_str));
                }
            }
        }

        match num_str.parse::<f64>() {
            Ok(n) => {
                // For extreme exponents that overflow to infinity or underflow to zero,
                // we keep them as-is (infinity/zero) since we don't have decnum support.
                TokenKind::Number(n)
            }
            Err(_) => TokenKind::Error(format!("invalid number: {}", num_str)),
        }
    }

    /// Tokenize the entire input
    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token();
            let is_eof = token.kind.is_eof();
            tokens.push(token);
            if is_eof {
                break;
            }
        }
        tokens
    }
}

/// Extract the exponent from a number string like "9E999999999" or "1.5e-10"
fn extract_exponent(s: &str) -> Option<i64> {
    let s = s.to_uppercase();
    if let Some(idx) = s.find('E') {
        s[idx + 1..].parse::<i64>().ok()
    } else {
        None
    }
}

/// Normalize a literal number to jq's canonical format.
/// Input: "9999999999E999999990" or "0.000000001E-999999990"
/// Output: "9.999999999E+999999999" or "1E-999999999"
fn normalize_literal_number(s: &str) -> String {
    let s = s.to_uppercase();
    let negative = s.starts_with('-');
    let s = s.trim_start_matches('-');

    // Split mantissa and exponent
    let (mantissa_str, exp_str) = if let Some(idx) = s.find('E') {
        (&s[..idx], &s[idx + 1..])
    } else {
        return s.to_string(); // No exponent, return as-is
    };

    let mut exponent: i64 = exp_str.parse().unwrap_or(0);

    // Parse mantissa into digits and decimal position
    // "9999999999" -> digits: [9,9,9,9,9,9,9,9,9,9], decimal_pos: 10 (after all digits)
    // "0.000000001" -> digits: [1], decimal_pos: -8 (one place before the 1)
    // "123.456" -> digits: [1,2,3,4,5,6], decimal_pos: 3 (after 3rd digit)

    let mut digits: Vec<char> = Vec::new();
    let mut decimal_pos: i64 = 0;
    let mut found_decimal = false;

    for c in mantissa_str.chars() {
        if c == '.' {
            found_decimal = true;
            decimal_pos = digits.len() as i64;
        } else if c.is_ascii_digit() {
            if digits.is_empty() && c == '0' && !found_decimal {
                // Leading zeros before decimal point - skip
            } else if digits.is_empty() && c == '0' && found_decimal {
                // Leading zeros after decimal point, adjust exponent
                exponent -= 1;
            } else {
                digits.push(c);
            }
        }
    }

    // If no decimal point was found, it's at the end
    if !found_decimal {
        decimal_pos = digits.len() as i64;
    }

    // If no significant digits, return "0"
    if digits.is_empty() {
        return "0".to_string();
    }

    // Trim trailing zeros from digits
    while digits.len() > 1 && digits.last() == Some(&'0') {
        digits.pop();
    }

    // Normalize: move decimal point to after first digit
    // Current: digits with decimal at decimal_pos
    // Target: d.ddddd with decimal at position 1
    // Adjustment to exponent: decimal_pos - 1 (if decimal was at pos 3, we shift by 2)
    let shift = decimal_pos - 1;
    exponent += shift;

    // Build normalized mantissa: first digit, then decimal (if more digits), then rest
    let mut result = String::new();
    if negative {
        result.push('-');
    }

    result.push(digits[0]);
    if digits.len() > 1 {
        result.push('.');
        for &d in &digits[1..] {
            result.push(d);
        }
    }

    // Add exponent with explicit sign
    result.push('E');
    if exponent >= 0 {
        result.push('+');
    }
    result.push_str(&exponent.to_string());

    result
}

/// Check if a byte can start an identifier
fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

/// Check if a byte can continue an identifier
fn is_ident_continue(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokenize(input: &str) -> Vec<TokenKind> {
        Lexer::new(input)
            .tokenize()
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    #[test]
    fn test_identity() {
        assert_eq!(tokenize("."), vec![TokenKind::Dot, TokenKind::Eof]);
    }

    #[test]
    fn test_field_access() {
        assert_eq!(
            tokenize(".foo"),
            vec![TokenKind::Field("foo".to_string()), TokenKind::Eof]
        );
        assert_eq!(
            tokenize(".foo.bar"),
            vec![
                TokenKind::Field("foo".to_string()),
                TokenKind::Field("bar".to_string()),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn test_pipe() {
        assert_eq!(
            tokenize(". | .foo"),
            vec![
                TokenKind::Dot,
                TokenKind::Pipe,
                TokenKind::Field("foo".to_string()),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn test_operators() {
        assert_eq!(
            tokenize("+ - * / %"),
            vec![
                TokenKind::Plus,
                TokenKind::Minus,
                TokenKind::Star,
                TokenKind::Slash,
                TokenKind::Percent,
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn test_comparison() {
        assert_eq!(
            tokenize("== != < > <= >="),
            vec![
                TokenKind::EqEq,
                TokenKind::NotEq,
                TokenKind::Lt,
                TokenKind::Gt,
                TokenKind::LtEq,
                TokenKind::GtEq,
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn test_keywords() {
        assert_eq!(
            tokenize("if then else end"),
            vec![
                TokenKind::If,
                TokenKind::Then,
                TokenKind::Else,
                TokenKind::End,
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn test_numbers() {
        let tokens = tokenize("42 3.14 1e10");
        assert_eq!(tokens.len(), 4);
        assert!(matches!(tokens[0], TokenKind::Number(n) if n == 42.0));
        assert!(matches!(tokens[1], TokenKind::Number(n) if (n - 3.14).abs() < 0.001));
        assert!(matches!(tokens[2], TokenKind::Number(n) if n == 1e10));
    }

    #[test]
    fn test_string() {
        let tokens = tokenize("\"hello\"");
        assert_eq!(
            tokens,
            vec![
                TokenKind::StringStart,
                TokenKind::StringText("hello".to_string()),
                TokenKind::StringEnd,
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn test_string_escape() {
        let tokens = tokenize(r#""hello\nworld""#);
        assert_eq!(
            tokens,
            vec![
                TokenKind::StringStart,
                TokenKind::StringText("hello\nworld".to_string()),
                TokenKind::StringEnd,
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn test_binding() {
        assert_eq!(
            tokenize("$foo"),
            vec![TokenKind::Binding("foo".to_string()), TokenKind::Eof]
        );
    }

    #[test]
    fn test_format() {
        assert_eq!(
            tokenize("@base64"),
            vec![TokenKind::Format("base64".to_string()), TokenKind::Eof]
        );
    }

    #[test]
    fn test_brackets() {
        assert_eq!(
            tokenize("[]{}()"),
            vec![
                TokenKind::LBracket,
                TokenKind::RBracket,
                TokenKind::LBrace,
                TokenKind::RBrace,
                TokenKind::LParen,
                TokenKind::RParen,
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn test_comment() {
        assert_eq!(
            tokenize(". # this is a comment\n.foo"),
            vec![
                TokenKind::Dot,
                TokenKind::Field("foo".to_string()),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn test_complex_expression() {
        let tokens = tokenize(".foo | map(. + 1) | select(. > 10)");
        assert!(tokens.len() > 5);
        assert_eq!(tokens.last(), Some(&TokenKind::Eof));
    }
}
