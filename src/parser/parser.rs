//! Parser for jq filter expressions
//!
//! Implements a Pratt parser (operator-precedence parsing) for jq expressions.

use super::ast::*;
use super::lexer::Lexer;
use super::token::{Token, TokenKind, Span};

/// Parse error
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "parse error at {}-{}: {}", self.span.start, self.span.end, self.message)
    }
}

impl std::error::Error for ParseError {}

/// Parser for jq expressions
pub struct Parser<'a> {
    lexer: Lexer<'a>,
    current: Token,
    previous: Token,
    errors: Vec<ParseError>,
}

impl<'a> Parser<'a> {
    /// Create a new parser
    pub fn new(input: &'a str) -> Self {
        let mut lexer = Lexer::new(input);
        let current = lexer.next_token();
        Parser {
            lexer,
            current,
            previous: Token::new(TokenKind::Eof, Span::default()),
            errors: Vec::new(),
        }
    }

    /// Parse the input and return an expression
    pub fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_query()
    }

    /// Parse a complete query (handles pipes, commas, and bindings)
    fn parse_query(&mut self) -> Result<Expr, ParseError> {
        // Check for function definition
        if self.check(&TokenKind::Def) {
            let def = self.parse_func_def()?;
            let body = self.parse_query()?;
            let span = def.span.merge(body.span);
            return Ok(Expr::new(
                ExprKind::LocalDef {
                    def,
                    body: Box::new(body),
                },
                span,
            ));
        }

        let mut expr = self.parse_comma_expr()?;

        // Handle "as" binding: expr as $var | body
        if self.check(&TokenKind::As) {
            self.advance();
            let pattern = self.parse_pattern()?;
            self.expect(&TokenKind::Pipe)?;
            let body = self.parse_query()?;
            let span = expr.span.merge(body.span);
            return Ok(Expr::new(
                ExprKind::Binding {
                    expr: Box::new(expr),
                    pattern,
                    body: Box::new(body),
                },
                span,
            ));
        }

        // Handle pipe: expr | expr
        while self.check(&TokenKind::Pipe) {
            self.advance();
            let right = self.parse_query()?;
            let span = expr.span.merge(right.span);
            expr = Expr::new(ExprKind::Pipe(Box::new(expr), Box::new(right)), span);
        }

        Ok(expr)
    }

    /// Parse a pattern (for bindings)
    /// Supports: $var, [$a, $b], {foo: $a, bar: $b}, {$a, $b}
    fn parse_pattern(&mut self) -> Result<Pattern, ParseError> {
        let span = self.current.span;

        // Simple binding: $var
        if let TokenKind::Binding(name) = &self.current.kind {
            let name = name.clone();
            self.advance();
            return Ok(Pattern {
                kind: PatternKind::Binding(name),
                span,
            });
        }

        // Array pattern: [$a, $b, ...]
        if self.check(&TokenKind::LBracket) {
            self.advance();
            let mut elements = Vec::new();

            if !self.check(&TokenKind::RBracket) {
                elements.push(self.parse_pattern()?);

                while self.check(&TokenKind::Comma) {
                    self.advance();
                    if self.check(&TokenKind::RBracket) {
                        break;
                    }
                    elements.push(self.parse_pattern()?);
                }
            }

            let end_span = self.current.span;
            self.expect(&TokenKind::RBracket)?;

            return Ok(Pattern {
                kind: PatternKind::Array(elements),
                span: span.merge(end_span),
            });
        }

        // Object pattern: {foo: $a, bar: $b} or {$a, $b}
        if self.check(&TokenKind::LBrace) {
            self.advance();
            let mut entries = Vec::new();

            if !self.check(&TokenKind::RBrace) {
                entries.push(self.parse_pattern_object_entry()?);

                while self.check(&TokenKind::Comma) {
                    self.advance();
                    if self.check(&TokenKind::RBrace) {
                        break;
                    }
                    entries.push(self.parse_pattern_object_entry()?);
                }
            }

            let end_span = self.current.span;
            self.expect(&TokenKind::RBrace)?;

            return Ok(Pattern {
                kind: PatternKind::Object(entries),
                span: span.merge(end_span),
            });
        }

        Err(self.error("expected binding pattern ($var), array pattern ([$a, $b]), or object pattern ({foo: $a})"))
    }

    /// Parse a single object pattern entry: foo: $a, "str": $b, or $a (shorthand)
    fn parse_pattern_object_entry(&mut self) -> Result<(ObjectKey, Pattern), ParseError> {
        // Shorthand binding: {$a} means {a: $a}
        if let TokenKind::Binding(name) = &self.current.kind {
            let name = name.clone();
            let span = self.current.span;
            self.advance();

            // Check if this is followed by a colon (explicit key: pattern) or standalone (shorthand)
            if self.check(&TokenKind::Colon) {
                // Explicit pattern after binding used as key: {$a: $b} means {a: $b}
                self.advance();
                let pattern = self.parse_pattern()?;
                return Ok((ObjectKey::Ident(name), pattern));
            }

            // Shorthand: {$a} means {a: $a}
            let pattern = Pattern {
                kind: PatternKind::Binding(name.clone()),
                span,
            };
            return Ok((ObjectKey::Ident(name), pattern));
        }

        // Key with explicit pattern
        let key = if let TokenKind::Ident(name) = &self.current.kind {
            let name = name.clone();
            self.advance();
            ObjectKey::Ident(name)
        } else if self.check(&TokenKind::StringStart) {
            // String key: {"foo": pattern}
            let s = self.parse_string_key()?;
            ObjectKey::String(s)
        } else if self.check(&TokenKind::LParen) {
            // Expression key: {(expr): pattern}
            self.advance();
            let expr = self.parse_query()?;
            self.expect(&TokenKind::RParen)?;
            ObjectKey::Expr(Box::new(expr))
        } else {
            return Err(self.error("expected pattern key (identifier, string, or expression)"));
        };

        // Colon is required after the key
        self.expect(&TokenKind::Colon)?;
        let pattern = self.parse_pattern()?;
        Ok((key, pattern))
    }

    /// Parse a string key (for object patterns) - simplified version
    fn parse_string_key(&mut self) -> Result<String, ParseError> {
        self.expect(&TokenKind::StringStart)?;

        let mut text = String::new();
        loop {
            match &self.current.kind {
                TokenKind::StringText(s) => {
                    text.push_str(s);
                    self.advance();
                }
                TokenKind::StringEnd => {
                    self.advance();
                    break;
                }
                TokenKind::StringInterpStart => {
                    return Err(self.error("interpolation not supported in pattern string keys"));
                }
                _ => {
                    return Err(self.error("unexpected token in string"));
                }
            }
        }

        Ok(text)
    }

    /// Parse comma expression
    fn parse_comma_expr(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_assign_expr()?;

        while self.check(&TokenKind::Comma) {
            self.advance();
            let right = self.parse_assign_expr()?;
            let span = expr.span.merge(right.span);
            expr = Expr::new(ExprKind::Comma(Box::new(expr), Box::new(right)), span);
        }

        Ok(expr)
    }

    /// Parse assignment expressions
    fn parse_assign_expr(&mut self) -> Result<Expr, ParseError> {
        let expr = self.parse_alternative_expr()?;

        // Check for assignment operators
        if self.check(&TokenKind::Eq) {
            self.advance();
            let value = self.parse_assign_expr()?;
            let span = expr.span.merge(value.span);
            return Ok(Expr::new(
                ExprKind::Assign {
                    target: Box::new(expr),
                    value: Box::new(value),
                },
                span,
            ));
        }

        if self.check(&TokenKind::PipeEq) {
            self.advance();
            let value = self.parse_assign_expr()?;
            let span = expr.span.merge(value.span);
            return Ok(Expr::new(
                ExprKind::Update {
                    target: Box::new(expr),
                    value: Box::new(value),
                },
                span,
            ));
        }

        // Handle +=, -=, etc.
        let update_op = match &self.current.kind {
            TokenKind::PlusEq => Some(BinaryOp::Add),
            TokenKind::MinusEq => Some(BinaryOp::Sub),
            TokenKind::StarEq => Some(BinaryOp::Mul),
            TokenKind::SlashEq => Some(BinaryOp::Div),
            TokenKind::PercentEq => Some(BinaryOp::Mod),
            _ => None,
        };

        if let Some(op) = update_op {
            self.advance();
            let value = self.parse_assign_expr()?;
            let span = expr.span.merge(value.span);
            return Ok(Expr::new(
                ExprKind::UpdateOp {
                    op,
                    target: Box::new(expr),
                    value: Box::new(value),
                },
                span,
            ));
        }

        Ok(expr)
    }

    /// Parse alternative expression (expr // expr)
    fn parse_alternative_expr(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_or_expr()?;

        while self.check(&TokenKind::DoubleSlash) {
            self.advance();
            let right = self.parse_or_expr()?;
            let span = expr.span.merge(right.span);
            expr = Expr::new(ExprKind::Alternative(Box::new(expr), Box::new(right)), span);
        }

        Ok(expr)
    }

    /// Parse or expression
    fn parse_or_expr(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_and_expr()?;

        while self.check(&TokenKind::Or) {
            self.advance();
            let right = self.parse_and_expr()?;
            let span = expr.span.merge(right.span);
            expr = Expr::new(
                ExprKind::BinaryOp {
                    op: BinaryOp::Or,
                    left: Box::new(expr),
                    right: Box::new(right),
                },
                span,
            );
        }

        Ok(expr)
    }

    /// Parse and expression
    fn parse_and_expr(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_comparison_expr()?;

        while self.check(&TokenKind::And) {
            self.advance();
            let right = self.parse_comparison_expr()?;
            let span = expr.span.merge(right.span);
            expr = Expr::new(
                ExprKind::BinaryOp {
                    op: BinaryOp::And,
                    left: Box::new(expr),
                    right: Box::new(right),
                },
                span,
            );
        }

        Ok(expr)
    }

    /// Parse comparison expression
    fn parse_comparison_expr(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_add_expr()?;

        loop {
            let op = match &self.current.kind {
                TokenKind::EqEq => BinaryOp::Eq,
                TokenKind::NotEq => BinaryOp::Ne,
                TokenKind::Lt => BinaryOp::Lt,
                TokenKind::LtEq => BinaryOp::Le,
                TokenKind::Gt => BinaryOp::Gt,
                TokenKind::GtEq => BinaryOp::Ge,
                _ => break,
            };
            self.advance();
            let right = self.parse_add_expr()?;
            let span = expr.span.merge(right.span);
            expr = Expr::new(
                ExprKind::BinaryOp {
                    op,
                    left: Box::new(expr),
                    right: Box::new(right),
                },
                span,
            );
        }

        Ok(expr)
    }

    /// Parse additive expression
    fn parse_add_expr(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_mul_expr()?;

        loop {
            let op = match &self.current.kind {
                TokenKind::Plus => BinaryOp::Add,
                TokenKind::Minus => BinaryOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_mul_expr()?;
            let span = expr.span.merge(right.span);
            expr = Expr::new(
                ExprKind::BinaryOp {
                    op,
                    left: Box::new(expr),
                    right: Box::new(right),
                },
                span,
            );
        }

        Ok(expr)
    }

    /// Parse multiplicative expression
    fn parse_mul_expr(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_unary_expr()?;

        loop {
            let op = match &self.current.kind {
                TokenKind::Star => BinaryOp::Mul,
                TokenKind::Slash => BinaryOp::Div,
                TokenKind::Percent => BinaryOp::Mod,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary_expr()?;
            let span = expr.span.merge(right.span);
            expr = Expr::new(
                ExprKind::BinaryOp {
                    op,
                    left: Box::new(expr),
                    right: Box::new(right),
                },
                span,
            );
        }

        Ok(expr)
    }

    /// Parse unary expression
    fn parse_unary_expr(&mut self) -> Result<Expr, ParseError> {
        // Negation
        if self.check(&TokenKind::Minus) {
            let start = self.current.span;
            self.advance();
            let expr = self.parse_unary_expr()?;
            let span = start.merge(expr.span);
            return Ok(Expr::new(ExprKind::Negate(Box::new(expr)), span));
        }

        self.parse_postfix_expr()
    }

    /// Parse postfix expression (including optional ? and indexing)
    fn parse_postfix_expr(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary()?;

        loop {
            if self.check(&TokenKind::Question) {
                let end = self.current.span;
                self.advance();
                let span = expr.span.merge(end);
                expr = Expr::new(ExprKind::Optional(Box::new(expr)), span);
            } else if self.check(&TokenKind::LBracket) {
                expr = self.parse_index_expr(expr)?;
            } else if let TokenKind::Field(name) = &self.current.kind {
                let name = name.clone();
                let end = self.current.span;
                self.advance();
                let span = expr.span.merge(end);

                // Check for optional
                let optional = self.check(&TokenKind::Question);
                if optional {
                    self.advance();
                }

                expr = Expr::new(
                    ExprKind::Index {
                        expr: Box::new(expr),
                        index: Box::new(Expr::new(
                            ExprKind::Literal(Literal::String(name)),
                            end,
                        )),
                        optional,
                    },
                    span,
                );
            } else if self.check(&TokenKind::Dot) {
                self.advance();
                if self.check(&TokenKind::LBracket) {
                    expr = self.parse_index_expr(expr)?;
                } else {
                    // This shouldn't normally happen, but handle it
                    break;
                }
            } else {
                break;
            }
        }

        Ok(expr)
    }

    /// Parse index expression: [expr], [], [start:end]
    fn parse_index_expr(&mut self, base: Expr) -> Result<Expr, ParseError> {
        let start_span = self.current.span;
        self.expect(&TokenKind::LBracket)?;

        // Empty brackets: .[]
        if self.check(&TokenKind::RBracket) {
            let end_span = self.current.span;
            self.advance();
            let optional = self.check(&TokenKind::Question);
            if optional {
                self.advance();
            }
            return Ok(Expr::new(
                ExprKind::Iterator {
                    expr: Box::new(base),
                    optional,
                },
                start_span.merge(end_span),
            ));
        }

        // Check for slice starting with :
        if self.check(&TokenKind::Colon) {
            self.advance();
            let end = if self.check(&TokenKind::RBracket) {
                None
            } else {
                Some(Box::new(self.parse_query()?))
            };
            let end_span = self.current.span;
            self.expect(&TokenKind::RBracket)?;
            let optional = self.check(&TokenKind::Question);
            if optional {
                self.advance();
            }
            return Ok(Expr::new(
                ExprKind::Slice {
                    expr: Box::new(base),
                    start: None,
                    end,
                    optional,
                },
                start_span.merge(end_span),
            ));
        }

        let index_expr = self.parse_query()?;

        // Check for slice: [start:end]
        if self.check(&TokenKind::Colon) {
            self.advance();
            let end = if self.check(&TokenKind::RBracket) {
                None
            } else {
                Some(Box::new(self.parse_query()?))
            };
            let end_span = self.current.span;
            self.expect(&TokenKind::RBracket)?;
            let optional = self.check(&TokenKind::Question);
            if optional {
                self.advance();
            }
            return Ok(Expr::new(
                ExprKind::Slice {
                    expr: Box::new(base),
                    start: Some(Box::new(index_expr)),
                    end,
                    optional,
                },
                start_span.merge(end_span),
            ));
        }

        let end_span = self.current.span;
        self.expect(&TokenKind::RBracket)?;
        let optional = self.check(&TokenKind::Question);
        if optional {
            self.advance();
        }

        Ok(Expr::new(
            ExprKind::Index {
                expr: Box::new(base),
                index: Box::new(index_expr),
                optional,
            },
            start_span.merge(end_span),
        ))
    }

    /// Parse primary expression
    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        let token = self.current.clone();

        match &token.kind {
            // Identity
            TokenKind::Dot => {
                self.advance();
                Ok(Expr::new(ExprKind::Identity, token.span))
            }

            // Recursive descent
            TokenKind::DotDot => {
                self.advance();
                Ok(Expr::new(ExprKind::RecursiveDescent, token.span))
            }

            // Field access starting with .
            TokenKind::Field(name) => {
                let name = name.clone();
                self.advance();
                let optional = self.check(&TokenKind::Question);
                if optional {
                    self.advance();
                }
                Ok(Expr::new(
                    ExprKind::Index {
                        expr: Box::new(Expr::new(ExprKind::Identity, token.span)),
                        index: Box::new(Expr::new(
                            ExprKind::Literal(Literal::String(name)),
                            token.span,
                        )),
                        optional,
                    },
                    token.span,
                ))
            }

            // Number literal
            TokenKind::Number(n) => {
                let n = *n;
                self.advance();
                Ok(Expr::new(ExprKind::Literal(Literal::Number(n)), token.span))
            }

            // String literal
            TokenKind::StringStart => self.parse_string(),

            // Identifiers (function calls or keywords like true/false/null)
            TokenKind::Ident(name) => {
                let name = name.clone();
                self.advance();

                // Check for true, false, null
                match name.as_str() {
                    "true" => Ok(Expr::new(ExprKind::Literal(Literal::Bool(true)), token.span)),
                    "false" => Ok(Expr::new(ExprKind::Literal(Literal::Bool(false)), token.span)),
                    "null" => Ok(Expr::new(ExprKind::Literal(Literal::Null), token.span)),
                    _ => {
                        // Function call
                        let args = if self.check(&TokenKind::LParen) {
                            self.parse_func_args()?
                        } else {
                            Vec::new()
                        };
                        Ok(Expr::new(
                            ExprKind::FunctionCall { name, args },
                            token.span,
                        ))
                    }
                }
            }

            // 'not' keyword as function call
            TokenKind::Not => {
                self.advance();
                Ok(Expr::new(
                    ExprKind::FunctionCall { name: "not".to_string(), args: Vec::new() },
                    token.span,
                ))
            }

            // Variable reference
            TokenKind::Binding(name) => {
                let name = name.clone();
                self.advance();
                Ok(Expr::new(ExprKind::Variable(name), token.span))
            }

            // $__loc__
            TokenKind::Loc => {
                self.advance();
                Ok(Expr::new(ExprKind::Loc, token.span))
            }

            // Format: @base64, etc.
            TokenKind::Format(fmt) => {
                let fmt = fmt.clone();
                self.advance();
                Ok(Expr::new(
                    ExprKind::Format {
                        format: fmt,
                        expr: None,
                    },
                    token.span,
                ))
            }

            // Parenthesized expression
            TokenKind::LParen => {
                let start = token.span;
                self.advance();
                let expr = self.parse_query()?;
                let end = self.current.span;
                self.expect(&TokenKind::RParen)?;
                Ok(Expr::new(ExprKind::Paren(Box::new(expr)), start.merge(end)))
            }

            // Array construction
            TokenKind::LBracket => self.parse_array(),

            // Object construction
            TokenKind::LBrace => self.parse_object(),

            // If expression
            TokenKind::If => self.parse_if(),

            // Try expression
            TokenKind::Try => self.parse_try(),

            // Reduce expression
            TokenKind::Reduce => self.parse_reduce(),

            // Foreach expression
            TokenKind::Foreach => self.parse_foreach(),

            // Break
            TokenKind::Break => {
                self.advance();
                if let TokenKind::Binding(name) = &self.current.kind {
                    let name = name.clone();
                    let end = self.current.span;
                    self.advance();
                    Ok(Expr::new(ExprKind::Break(name), token.span.merge(end)))
                } else {
                    Err(self.error("expected label name after break"))
                }
            }

            // Label
            TokenKind::Label => {
                self.advance();
                if let TokenKind::Binding(name) = &self.current.kind {
                    let name = name.clone();
                    self.advance();
                    self.expect(&TokenKind::Pipe)?;
                    let body = self.parse_query()?;
                    let span = token.span.merge(body.span);
                    Ok(Expr::new(
                        ExprKind::Label {
                            name,
                            body: Box::new(body),
                        },
                        span,
                    ))
                } else {
                    Err(self.error("expected label name"))
                }
            }

            _ => Err(self.error(&format!("unexpected token: {}", token.kind))),
        }
    }

    /// Parse a string (possibly with interpolation)
    fn parse_string(&mut self) -> Result<Expr, ParseError> {
        let start = self.current.span;
        self.expect(&TokenKind::StringStart)?;

        let mut parts: Vec<StringPart> = Vec::new();

        loop {
            match &self.current.kind {
                TokenKind::StringEnd => {
                    let end = self.current.span;
                    self.advance();
                    break Ok(if parts.len() == 1 {
                        if let StringPart::Text(s) = &parts[0] {
                            Expr::new(ExprKind::Literal(Literal::String(s.clone())), start.merge(end))
                        } else {
                            Expr::new(ExprKind::StringInterp(parts), start.merge(end))
                        }
                    } else if parts.is_empty() {
                        Expr::new(ExprKind::Literal(Literal::String(String::new())), start.merge(end))
                    } else {
                        Expr::new(ExprKind::StringInterp(parts), start.merge(end))
                    });
                }
                TokenKind::StringText(s) => {
                    parts.push(StringPart::Text(s.clone()));
                    self.advance();
                }
                TokenKind::StringInterpStart => {
                    self.advance();
                    let expr = self.parse_query()?;
                    parts.push(StringPart::Interp(Box::new(expr)));
                    if !self.check(&TokenKind::StringInterpEnd) && !self.check(&TokenKind::RParen) {
                        return Err(self.error("expected ) to end string interpolation"));
                    }
                    self.advance();
                }
                TokenKind::Error(e) => {
                    return Err(self.error(e));
                }
                _ => {
                    return Err(self.error("unexpected token in string"));
                }
            }
        }
    }

    /// Parse function arguments
    fn parse_func_args(&mut self) -> Result<Vec<Expr>, ParseError> {
        self.expect(&TokenKind::LParen)?;
        let mut args = Vec::new();

        if !self.check(&TokenKind::RParen) {
            args.push(self.parse_query()?);
            while self.check(&TokenKind::Semicolon) {
                self.advance();
                args.push(self.parse_query()?);
            }
        }

        self.expect(&TokenKind::RParen)?;
        Ok(args)
    }

    /// Parse array construction
    fn parse_array(&mut self) -> Result<Expr, ParseError> {
        let start = self.current.span;
        self.expect(&TokenKind::LBracket)?;

        if self.check(&TokenKind::RBracket) {
            let end = self.current.span;
            self.advance();
            return Ok(Expr::new(ExprKind::Array(None), start.merge(end)));
        }

        let expr = self.parse_query()?;
        let end = self.current.span;
        self.expect(&TokenKind::RBracket)?;

        Ok(Expr::new(ExprKind::Array(Some(Box::new(expr))), start.merge(end)))
    }

    /// Parse object construction
    fn parse_object(&mut self) -> Result<Expr, ParseError> {
        let start = self.current.span;
        self.expect(&TokenKind::LBrace)?;

        let mut entries = Vec::new();

        if !self.check(&TokenKind::RBrace) {
            entries.push(self.parse_object_entry()?);
            while self.check(&TokenKind::Comma) {
                self.advance();
                if self.check(&TokenKind::RBrace) {
                    break; // Trailing comma
                }
                entries.push(self.parse_object_entry()?);
            }
        }

        let end = self.current.span;
        self.expect(&TokenKind::RBrace)?;

        Ok(Expr::new(ExprKind::Object(entries), start.merge(end)))
    }

    /// Parse a single object entry
    fn parse_object_entry(&mut self) -> Result<ObjectEntry, ParseError> {
        let start = self.current.span;

        // Key
        let key = match &self.current.kind {
            TokenKind::Ident(name) => {
                let name = name.clone();
                self.advance();
                if self.check(&TokenKind::Colon) {
                    ObjectKey::Ident(name)
                } else {
                    // Shorthand: {foo} means {foo: .foo}
                    let value = Box::new(Expr::new(
                        ExprKind::Index {
                            expr: Box::new(Expr::new(ExprKind::Identity, start)),
                            index: Box::new(Expr::new(
                                ExprKind::Literal(Literal::String(name.clone())),
                                start,
                            )),
                            optional: false,
                        },
                        start,
                    ));
                    return Ok(ObjectEntry {
                        key: ObjectKey::Shorthand(name),
                        value,
                        span: start,
                    });
                }
            }
            TokenKind::StringStart => {
                let s = self.parse_string()?;
                if let ExprKind::Literal(Literal::String(ref key_str)) = s.kind {
                    // Check if this is shorthand (no colon follows)
                    if !self.check(&TokenKind::Colon) {
                        // Shorthand: {"foo"} means {"foo": .foo}
                        let name = key_str.clone();
                        let value = Box::new(Expr::new(
                            ExprKind::Index {
                                expr: Box::new(Expr::new(ExprKind::Identity, start)),
                                index: Box::new(Expr::new(
                                    ExprKind::Literal(Literal::String(name.clone())),
                                    start,
                                )),
                                optional: false,
                            },
                            start,
                        ));
                        return Ok(ObjectEntry {
                            key: ObjectKey::String(name),
                            value,
                            span: start,
                        });
                    }
                    ObjectKey::String(key_str.clone())
                } else {
                    // Interpolated string - check if shorthand
                    if !self.check(&TokenKind::Colon) {
                        // Shorthand: {"foo\(bar)"} means {("foo\(bar)"): .["foo\(bar)"]}
                        let key_expr = s.clone();
                        let value = Box::new(Expr::new(
                            ExprKind::Index {
                                expr: Box::new(Expr::new(ExprKind::Identity, start)),
                                index: Box::new(s),
                                optional: false,
                            },
                            start,
                        ));
                        return Ok(ObjectEntry {
                            key: ObjectKey::Expr(Box::new(key_expr)),
                            value,
                            span: start,
                        });
                    }
                    ObjectKey::Expr(Box::new(s))
                }
            }
            TokenKind::LParen => {
                self.advance();
                let expr = self.parse_query()?;
                self.expect(&TokenKind::RParen)?;
                ObjectKey::Expr(Box::new(expr))
            }
            TokenKind::Binding(name) => {
                // $var means {($var): $var}
                let name = name.clone();
                self.advance();
                let key_expr = Expr::new(ExprKind::Variable(name.clone()), start);
                let value = Box::new(Expr::new(ExprKind::Variable(name), start));
                return Ok(ObjectEntry {
                    key: ObjectKey::Expr(Box::new(key_expr)),
                    value,
                    span: start,
                });
            }
            _ => return Err(self.error("expected object key")),
        };

        self.expect(&TokenKind::Colon)?;
        let value = Box::new(self.parse_pipe_expr()?);
        let span = start.merge(value.span);

        Ok(ObjectEntry { key, value, span })
    }

    /// Parse pipe expression (without comma)
    fn parse_pipe_expr(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_assign_expr()?;

        // Handle pipe: expr | expr
        while self.check(&TokenKind::Pipe) {
            self.advance();
            let right = self.parse_assign_expr()?;
            let span = expr.span.merge(right.span);
            expr = Expr::new(ExprKind::Pipe(Box::new(expr), Box::new(right)), span);
        }

        Ok(expr)
    }

    /// Parse if expression
    fn parse_if(&mut self) -> Result<Expr, ParseError> {
        let start = self.current.span;
        self.expect(&TokenKind::If)?;

        let condition = Box::new(self.parse_query()?);
        self.expect(&TokenKind::Then)?;
        let then_branch = Box::new(self.parse_query()?);

        let else_branch = if self.check(&TokenKind::Elif) {
            // elif becomes nested if
            Some(Box::new(self.parse_if()?))
        } else if self.check(&TokenKind::Else) {
            self.advance();
            let branch = self.parse_query()?;
            self.expect(&TokenKind::End)?;
            Some(Box::new(branch))
        } else {
            self.expect(&TokenKind::End)?;
            None
        };

        let end = self.previous.span;
        Ok(Expr::new(
            ExprKind::Conditional {
                condition,
                then_branch,
                else_branch,
            },
            start.merge(end),
        ))
    }

    /// Parse try expression
    fn parse_try(&mut self) -> Result<Expr, ParseError> {
        let start = self.current.span;
        self.expect(&TokenKind::Try)?;

        let expr = Box::new(self.parse_postfix_expr()?);

        let catch = if self.check(&TokenKind::Catch) {
            self.advance();
            Some(Box::new(self.parse_postfix_expr()?))
        } else {
            None
        };

        let end = self.previous.span;
        Ok(Expr::new(ExprKind::TryCatch { expr, catch }, start.merge(end)))
    }

    /// Parse reduce expression
    fn parse_reduce(&mut self) -> Result<Expr, ParseError> {
        let start = self.current.span;
        self.expect(&TokenKind::Reduce)?;

        let expr = Box::new(self.parse_postfix_expr()?);
        self.expect(&TokenKind::As)?;

        let pattern = self.parse_pattern()?;

        self.expect(&TokenKind::LParen)?;
        let init = Box::new(self.parse_query()?);
        self.expect(&TokenKind::Semicolon)?;
        let update = Box::new(self.parse_query()?);
        let end = self.current.span;
        self.expect(&TokenKind::RParen)?;

        Ok(Expr::new(
            ExprKind::Reduce {
                expr,
                pattern,
                init,
                update,
            },
            start.merge(end),
        ))
    }

    /// Parse foreach expression
    fn parse_foreach(&mut self) -> Result<Expr, ParseError> {
        let start = self.current.span;
        self.expect(&TokenKind::Foreach)?;

        let expr = Box::new(self.parse_postfix_expr()?);
        self.expect(&TokenKind::As)?;

        let pattern = self.parse_pattern()?;

        self.expect(&TokenKind::LParen)?;
        let init = Box::new(self.parse_query()?);
        self.expect(&TokenKind::Semicolon)?;
        let update = Box::new(self.parse_query()?);

        let extract = if self.check(&TokenKind::Semicolon) {
            self.advance();
            Some(Box::new(self.parse_query()?))
        } else {
            None
        };

        let end = self.current.span;
        self.expect(&TokenKind::RParen)?;

        Ok(Expr::new(
            ExprKind::Foreach {
                expr,
                pattern,
                init,
                update,
                extract,
            },
            start.merge(end),
        ))
    }

    /// Parse function definition
    fn parse_func_def(&mut self) -> Result<FuncDef, ParseError> {
        let start = self.current.span;
        self.expect(&TokenKind::Def)?;

        let name = if let TokenKind::Ident(name) = &self.current.kind {
            let name = name.clone();
            self.advance();
            name
        } else {
            return Err(self.error("expected function name"));
        };

        let params = if self.check(&TokenKind::LParen) {
            self.advance();
            let mut params = Vec::new();
            if !self.check(&TokenKind::RParen) {
                params.push(self.parse_param()?);
                while self.check(&TokenKind::Semicolon) {
                    self.advance();
                    params.push(self.parse_param()?);
                }
            }
            self.expect(&TokenKind::RParen)?;
            params
        } else {
            Vec::new()
        };

        self.expect(&TokenKind::Colon)?;
        let body = Box::new(self.parse_query()?);
        let end = self.current.span;
        self.expect(&TokenKind::Semicolon)?;

        Ok(FuncDef {
            name,
            params,
            body,
            span: start.merge(end),
        })
    }

    /// Parse a function parameter
    fn parse_param(&mut self) -> Result<Param, ParseError> {
        let span = self.current.span;
        match &self.current.kind {
            TokenKind::Binding(name) => {
                let name = name.clone();
                self.advance();
                Ok(Param {
                    name,
                    is_binding: true,
                    span,
                })
            }
            TokenKind::Ident(name) => {
                let name = name.clone();
                self.advance();
                Ok(Param {
                    name,
                    is_binding: false,
                    span,
                })
            }
            _ => Err(self.error("expected parameter")),
        }
    }

    // Helper methods

    fn advance(&mut self) {
        self.previous = self.current.clone();
        self.current = self.lexer.next_token();
    }

    fn check(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(&self.current.kind) == std::mem::discriminant(kind)
    }

    fn expect(&mut self, kind: &TokenKind) -> Result<(), ParseError> {
        if self.check(kind) {
            self.advance();
            Ok(())
        } else {
            Err(self.error(&format!("expected {:?}, got {:?}", kind, self.current.kind)))
        }
    }

    fn error(&self, message: &str) -> ParseError {
        ParseError {
            message: message.to_string(),
            span: self.current.span,
        }
    }
}

/// Parse a jq filter expression
pub fn parse(input: &str) -> Result<Expr, ParseError> {
    let mut parser = Parser::new(input);
    let expr = parser.parse_expr()?;

    if !parser.current.kind.is_eof() {
        return Err(parser.error("unexpected token after expression"));
    }

    Ok(expr)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(input: &str) -> Expr {
        parse(input).unwrap_or_else(|e| panic!("Failed to parse '{}': {}", input, e))
    }

    #[test]
    fn test_identity() {
        let expr = parse_ok(".");
        assert!(matches!(expr.kind, ExprKind::Identity));
    }

    #[test]
    fn test_field() {
        let expr = parse_ok(".foo");
        assert!(matches!(expr.kind, ExprKind::Index { .. }));
    }

    #[test]
    fn test_pipe() {
        let expr = parse_ok(". | .foo");
        assert!(matches!(expr.kind, ExprKind::Pipe(_, _)));
    }

    #[test]
    fn test_literals() {
        assert!(matches!(
            parse_ok("null").kind,
            ExprKind::Literal(Literal::Null)
        ));
        assert!(matches!(
            parse_ok("true").kind,
            ExprKind::Literal(Literal::Bool(true))
        ));
        assert!(matches!(
            parse_ok("false").kind,
            ExprKind::Literal(Literal::Bool(false))
        ));
        assert!(matches!(
            parse_ok("42").kind,
            ExprKind::Literal(Literal::Number(_))
        ));
    }

    #[test]
    fn test_string() {
        let expr = parse_ok(r#""hello""#);
        assert!(matches!(
            expr.kind,
            ExprKind::Literal(Literal::String(_))
        ));
    }

    #[test]
    fn test_array() {
        let expr = parse_ok("[1, 2, 3]");
        assert!(matches!(expr.kind, ExprKind::Array(_)));
    }

    #[test]
    fn test_object() {
        let expr = parse_ok("{foo: 1, bar: 2}");
        assert!(matches!(expr.kind, ExprKind::Object(_)));
    }

    #[test]
    fn test_function_call() {
        let expr = parse_ok("map(. + 1)");
        assert!(matches!(
            expr.kind,
            ExprKind::FunctionCall { name, args } if name == "map" && args.len() == 1
        ));
    }

    #[test]
    fn test_if_then_else() {
        let expr = parse_ok("if . > 0 then 1 else 0 end");
        assert!(matches!(expr.kind, ExprKind::Conditional { .. }));
    }

    #[test]
    fn test_binary_ops() {
        let expr = parse_ok("1 + 2 * 3");
        // Should parse as 1 + (2 * 3) due to precedence
        assert!(matches!(expr.kind, ExprKind::BinaryOp { op: BinaryOp::Add, .. }));
    }

    #[test]
    fn test_iterator() {
        let expr = parse_ok(".[]");
        assert!(matches!(
            expr.kind,
            ExprKind::Iterator { optional: false, .. }
        ));
    }

    #[test]
    fn test_optional() {
        let expr = parse_ok(".foo?");
        assert!(matches!(expr.kind, ExprKind::Index { optional: true, .. }));
    }

    #[test]
    fn test_complex_expression() {
        parse_ok(".foo | map(select(. > 0)) | add");
        parse_ok("if .x then .y else .z end");
        parse_ok("[.[] | . + 1]");
        parse_ok("{a: .b, c: .d}");
    }
}
