//! Token definitions for jq lexer

use std::fmt;

/// Position in source code
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Span { start, end }
    }

    pub fn merge(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// A token with its span
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Token { kind, span }
    }
}

/// Token types for jq language
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Literals
    /// Number literal (integer or float)
    Number(f64),
    /// Literal number that can't be represented as f64 (extreme exponents)
    /// Stores the normalized string representation
    LiteralNumber(String),
    /// Start of a string (opening quote)
    StringStart,
    /// String text content
    StringText(String),
    /// Start of string interpolation \(
    StringInterpStart,
    /// End of string interpolation )
    StringInterpEnd,
    /// End of a string (closing quote)
    StringEnd,

    // Identifiers and special names
    /// Identifier (function name, keyword, etc.)
    Ident(String),
    /// Field accessor (.field)
    Field(String),
    /// Variable binding ($name)
    Binding(String),
    /// Format string (@base64, @uri, etc.)
    Format(String),

    // Single-character operators
    /// . (identity or field start)
    Dot,
    /// |
    Pipe,
    /// ,
    Comma,
    /// :
    Colon,
    /// ::
    DoubleColon,
    /// ;
    Semicolon,
    /// (
    LParen,
    /// )
    RParen,
    /// [
    LBracket,
    /// ]
    RBracket,
    /// {
    LBrace,
    /// }
    RBrace,
    /// ?
    Question,
    /// =
    Eq,
    /// +
    Plus,
    /// -
    Minus,
    /// *
    Star,
    /// /
    Slash,
    /// %
    Percent,
    /// <
    Lt,
    /// >
    Gt,
    /// $
    Dollar,

    // Multi-character operators
    /// ..
    DotDot,
    /// ==
    EqEq,
    /// !=
    NotEq,
    /// <=
    LtEq,
    /// >=
    GtEq,
    /// //
    DoubleSlash,
    /// |=
    PipeEq,
    /// +=
    PlusEq,
    /// -=
    MinusEq,
    /// *=
    StarEq,
    /// /=
    SlashEq,
    /// %=
    PercentEq,
    /// //=
    DoubleSlashEq,
    /// ?//
    QuestionDoubleSlash,

    // Keywords
    /// if
    If,
    /// then
    Then,
    /// else
    Else,
    /// elif
    Elif,
    /// end
    End,
    /// as
    As,
    /// def
    Def,
    /// reduce
    Reduce,
    /// foreach
    Foreach,
    /// try
    Try,
    /// catch
    Catch,
    /// and
    And,
    /// or
    Or,
    /// not (builtin, but often used)
    Not,
    /// import
    Import,
    /// include
    Include,
    /// module
    Module,
    /// label
    Label,
    /// break
    Break,
    /// $__loc__
    Loc,

    // Special
    /// End of file
    Eof,
    /// Invalid character
    Invalid(char),
    /// Error during lexing
    Error(String),
}

impl TokenKind {
    /// Check if this is an end-of-file token
    pub fn is_eof(&self) -> bool {
        matches!(self, TokenKind::Eof)
    }

    /// Returns a human-readable name for the token, suitable for error messages.
    /// Uses the same style as jq: symbols are quoted like `'='`, keywords are quoted.
    pub fn display_name(&self) -> String {
        match self {
            // Literals
            TokenKind::Number(_) | TokenKind::LiteralNumber(_) => "number".to_string(),
            TokenKind::StringStart | TokenKind::StringEnd => "string".to_string(),
            TokenKind::StringText(_) => "string content".to_string(),
            TokenKind::StringInterpStart => "'\\('".to_string(),
            TokenKind::StringInterpEnd => "')'".to_string(),

            // Identifiers and special names
            TokenKind::Ident(s) => format!("'{}'", s),
            TokenKind::Field(s) => format!("'.{}'", s),
            TokenKind::Binding(s) => format!("'${}'", s),
            TokenKind::Format(s) => format!("'@{}'", s),

            // Single-character operators
            TokenKind::Dot => "'.'".to_string(),
            TokenKind::Pipe => "'|'".to_string(),
            TokenKind::Comma => "','".to_string(),
            TokenKind::Colon => "':'".to_string(),
            TokenKind::DoubleColon => "'::'".to_string(),
            TokenKind::Semicolon => "';'".to_string(),
            TokenKind::LParen => "'('".to_string(),
            TokenKind::RParen => "')'".to_string(),
            TokenKind::LBracket => "'['".to_string(),
            TokenKind::RBracket => "']'".to_string(),
            TokenKind::LBrace => "'{'".to_string(),
            TokenKind::RBrace => "'}'".to_string(),
            TokenKind::Question => "'?'".to_string(),
            TokenKind::Eq => "'='".to_string(),
            TokenKind::Plus => "'+'".to_string(),
            TokenKind::Minus => "'-'".to_string(),
            TokenKind::Star => "'*'".to_string(),
            TokenKind::Slash => "'/'".to_string(),
            TokenKind::Percent => "'%'".to_string(),
            TokenKind::Lt => "'<'".to_string(),
            TokenKind::Gt => "'>'".to_string(),
            TokenKind::Dollar => "'$'".to_string(),

            // Multi-character operators
            TokenKind::DotDot => "'..'".to_string(),
            TokenKind::EqEq => "'=='".to_string(),
            TokenKind::NotEq => "'!='".to_string(),
            TokenKind::LtEq => "'<='".to_string(),
            TokenKind::GtEq => "'>='".to_string(),
            TokenKind::DoubleSlash => "'//'".to_string(),
            TokenKind::PipeEq => "'|='".to_string(),
            TokenKind::PlusEq => "'+='".to_string(),
            TokenKind::MinusEq => "'-='".to_string(),
            TokenKind::StarEq => "'*='".to_string(),
            TokenKind::SlashEq => "'/='".to_string(),
            TokenKind::PercentEq => "'%='".to_string(),
            TokenKind::DoubleSlashEq => "'//='".to_string(),
            TokenKind::QuestionDoubleSlash => "'?//'".to_string(),

            // Keywords
            TokenKind::If => "'if'".to_string(),
            TokenKind::Then => "'then'".to_string(),
            TokenKind::Else => "'else'".to_string(),
            TokenKind::Elif => "'elif'".to_string(),
            TokenKind::End => "'end'".to_string(),
            TokenKind::As => "'as'".to_string(),
            TokenKind::Def => "'def'".to_string(),
            TokenKind::Reduce => "'reduce'".to_string(),
            TokenKind::Foreach => "'foreach'".to_string(),
            TokenKind::Try => "'try'".to_string(),
            TokenKind::Catch => "'catch'".to_string(),
            TokenKind::And => "'and'".to_string(),
            TokenKind::Or => "'or'".to_string(),
            TokenKind::Not => "'not'".to_string(),
            TokenKind::Import => "'import'".to_string(),
            TokenKind::Include => "'include'".to_string(),
            TokenKind::Module => "'module'".to_string(),
            TokenKind::Label => "'label'".to_string(),
            TokenKind::Break => "'break'".to_string(),
            TokenKind::Loc => "'$__loc__'".to_string(),

            // Special
            TokenKind::Eof => "end of input".to_string(),
            TokenKind::Invalid(c) => format!("'{}'", c),
            TokenKind::Error(s) => s.clone(),
        }
    }

    /// Check if this is a keyword
    pub fn is_keyword(&self) -> bool {
        matches!(
            self,
            TokenKind::If
                | TokenKind::Then
                | TokenKind::Else
                | TokenKind::Elif
                | TokenKind::End
                | TokenKind::As
                | TokenKind::Def
                | TokenKind::Reduce
                | TokenKind::Foreach
                | TokenKind::Try
                | TokenKind::Catch
                | TokenKind::And
                | TokenKind::Or
                | TokenKind::Import
                | TokenKind::Include
                | TokenKind::Module
                | TokenKind::Label
                | TokenKind::Break
        )
    }

    /// Get keyword from identifier if applicable
    pub fn keyword_from_ident(s: &str) -> Option<TokenKind> {
        match s {
            "if" => Some(TokenKind::If),
            "then" => Some(TokenKind::Then),
            "else" => Some(TokenKind::Else),
            "elif" => Some(TokenKind::Elif),
            "end" => Some(TokenKind::End),
            "as" => Some(TokenKind::As),
            "def" => Some(TokenKind::Def),
            "reduce" => Some(TokenKind::Reduce),
            "foreach" => Some(TokenKind::Foreach),
            "try" => Some(TokenKind::Try),
            "catch" => Some(TokenKind::Catch),
            "and" => Some(TokenKind::And),
            "or" => Some(TokenKind::Or),
            "not" => Some(TokenKind::Not),
            "import" => Some(TokenKind::Import),
            "include" => Some(TokenKind::Include),
            "module" => Some(TokenKind::Module),
            "label" => Some(TokenKind::Label),
            "break" => Some(TokenKind::Break),
            _ => None,
        }
    }

    /// Get identifier string for keywords (for use as object keys in patterns)
    pub fn as_ident_string(&self) -> Option<&'static str> {
        match self {
            TokenKind::If => Some("if"),
            TokenKind::Then => Some("then"),
            TokenKind::Else => Some("else"),
            TokenKind::Elif => Some("elif"),
            TokenKind::End => Some("end"),
            TokenKind::As => Some("as"),
            TokenKind::Def => Some("def"),
            TokenKind::Reduce => Some("reduce"),
            TokenKind::Foreach => Some("foreach"),
            TokenKind::Try => Some("try"),
            TokenKind::Catch => Some("catch"),
            TokenKind::And => Some("and"),
            TokenKind::Or => Some("or"),
            TokenKind::Not => Some("not"),
            TokenKind::Import => Some("import"),
            TokenKind::Include => Some("include"),
            TokenKind::Module => Some("module"),
            TokenKind::Label => Some("label"),
            TokenKind::Break => Some("break"),
            _ => None,
        }
    }
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenKind::Number(n) => write!(f, "{}", n),
            TokenKind::LiteralNumber(s) => write!(f, "{}", s),
            TokenKind::StringStart => write!(f, "\""),
            TokenKind::StringText(s) => write!(f, "{}", s),
            TokenKind::StringInterpStart => write!(f, "\\("),
            TokenKind::StringInterpEnd => write!(f, ")"),
            TokenKind::StringEnd => write!(f, "\""),
            TokenKind::Ident(s) => write!(f, "{}", s),
            TokenKind::Field(s) => write!(f, ".{}", s),
            TokenKind::Binding(s) => write!(f, "${}", s),
            TokenKind::Format(s) => write!(f, "@{}", s),
            TokenKind::Dot => write!(f, "."),
            TokenKind::Pipe => write!(f, "|"),
            TokenKind::Comma => write!(f, ","),
            TokenKind::Colon => write!(f, ":"),
            TokenKind::DoubleColon => write!(f, "::"),
            TokenKind::Semicolon => write!(f, ";"),
            TokenKind::LParen => write!(f, "("),
            TokenKind::RParen => write!(f, ")"),
            TokenKind::LBracket => write!(f, "["),
            TokenKind::RBracket => write!(f, "]"),
            TokenKind::LBrace => write!(f, "{{"),
            TokenKind::RBrace => write!(f, "}}"),
            TokenKind::Question => write!(f, "?"),
            TokenKind::Eq => write!(f, "="),
            TokenKind::Plus => write!(f, "+"),
            TokenKind::Minus => write!(f, "-"),
            TokenKind::Star => write!(f, "*"),
            TokenKind::Slash => write!(f, "/"),
            TokenKind::Percent => write!(f, "%"),
            TokenKind::Lt => write!(f, "<"),
            TokenKind::Gt => write!(f, ">"),
            TokenKind::Dollar => write!(f, "$"),
            TokenKind::DotDot => write!(f, ".."),
            TokenKind::EqEq => write!(f, "=="),
            TokenKind::NotEq => write!(f, "!="),
            TokenKind::LtEq => write!(f, "<="),
            TokenKind::GtEq => write!(f, ">="),
            TokenKind::DoubleSlash => write!(f, "//"),
            TokenKind::PipeEq => write!(f, "|="),
            TokenKind::PlusEq => write!(f, "+="),
            TokenKind::MinusEq => write!(f, "-="),
            TokenKind::StarEq => write!(f, "*="),
            TokenKind::SlashEq => write!(f, "/="),
            TokenKind::PercentEq => write!(f, "%="),
            TokenKind::DoubleSlashEq => write!(f, "//="),
            TokenKind::QuestionDoubleSlash => write!(f, "?//"),
            TokenKind::If => write!(f, "if"),
            TokenKind::Then => write!(f, "then"),
            TokenKind::Else => write!(f, "else"),
            TokenKind::Elif => write!(f, "elif"),
            TokenKind::End => write!(f, "end"),
            TokenKind::As => write!(f, "as"),
            TokenKind::Def => write!(f, "def"),
            TokenKind::Reduce => write!(f, "reduce"),
            TokenKind::Foreach => write!(f, "foreach"),
            TokenKind::Try => write!(f, "try"),
            TokenKind::Catch => write!(f, "catch"),
            TokenKind::And => write!(f, "and"),
            TokenKind::Or => write!(f, "or"),
            TokenKind::Not => write!(f, "not"),
            TokenKind::Import => write!(f, "import"),
            TokenKind::Include => write!(f, "include"),
            TokenKind::Module => write!(f, "module"),
            TokenKind::Label => write!(f, "label"),
            TokenKind::Break => write!(f, "break"),
            TokenKind::Loc => write!(f, "$__loc__"),
            TokenKind::Eof => write!(f, "<EOF>"),
            TokenKind::Invalid(c) => write!(f, "<invalid: {}>", c),
            TokenKind::Error(s) => write!(f, "<error: {}>", s),
        }
    }
}
