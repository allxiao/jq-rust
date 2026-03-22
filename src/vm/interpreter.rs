//! AST-walking interpreter for jq expressions

use std::rc::Rc;
use std::cell::RefCell;

use crate::jv::{Jv, JvObject};
use crate::parser::{Expr, ExprKind, Literal, BinaryOp, ObjectKey, StringPart, FuncDef, Pattern, PatternKind};
use super::context::{Context, Binding};

/// Result of evaluating an expression - can produce multiple values
pub type EvalResult = Box<dyn Iterator<Item = Result<Jv, String>>>;

/// Prefix used to signal a break from a label
const BREAK_PREFIX: &str = "__BREAK__:";

/// Check if an error is a break signal for a given label
fn is_break_for(err: &str, label: &str) -> bool {
    err == format!("{}{}", BREAK_PREFIX, label)
}

/// Create a break signal error
fn make_break_signal(label: &str) -> String {
    format!("{}{}", BREAK_PREFIX, label)
}

/// Get a value at a given path in a JSON structure
fn get_value_at_path(input: &Jv, path: &[Jv]) -> Jv {
    let mut current = input.clone();
    for p in path {
        match (&current, p) {
            (Jv::Object(obj), Jv::String(key)) => {
                current = obj.get(key.as_str()).unwrap_or(Jv::Null);
            }
            (Jv::Array(arr), Jv::Number(n)) => {
                if let Some(idx) = n.as_i64() {
                    current = arr.get(idx).unwrap_or(Jv::Null);
                } else {
                    return Jv::Null;
                }
            }
            _ => return Jv::Null,
        }
    }
    current
}

/// Set a value at a given path in a JSON structure
fn set_value_at_path(input: Jv, path: &[Jv], value: Jv) -> Jv {
    if path.is_empty() {
        return value;
    }

    let first = &path[0];
    let rest = &path[1..];

    match (&input, first) {
        (Jv::Object(obj), Jv::String(key)) => {
            let mut new_obj = obj.clone();
            let child = obj.get(key.as_str()).unwrap_or(Jv::Null);
            let new_child = set_value_at_path(child, rest, value);
            new_obj.set(key.as_str(), new_child);
            Jv::Object(new_obj)
        }
        (Jv::Array(arr), Jv::Number(n)) => {
            if let Some(idx) = n.as_i64() {
                let mut new_arr = arr.clone();
                let len = new_arr.len();

                // Handle negative indices
                let actual_idx = if idx < 0 {
                    let adj = len as i64 + idx;
                    if adj < 0 { 0usize } else { adj as usize }
                } else {
                    idx as usize
                };

                // Extend array if needed
                while new_arr.len() <= actual_idx {
                    new_arr.push(Jv::Null);
                }

                let child = new_arr.get(actual_idx as i64).unwrap_or(Jv::Null);
                let new_child = set_value_at_path(child, rest, value);
                new_arr.set(actual_idx as i64, new_child).ok();
                Jv::Array(new_arr)
            } else {
                input
            }
        }
        (Jv::Null, Jv::String(key)) => {
            // Auto-vivification: create object
            let mut new_obj = JvObject::new();
            let new_child = set_value_at_path(Jv::Null, rest, value);
            new_obj.set(key.as_str(), new_child);
            Jv::Object(new_obj)
        }
        (Jv::Null, Jv::Number(n)) => {
            // Auto-vivification: create array
            if let Some(idx) = n.as_i64() {
                if idx >= 0 {
                    let mut new_arr = crate::jv::JvArray::new();
                    while new_arr.len() <= idx as usize {
                        new_arr.push(Jv::Null);
                    }
                    let new_child = set_value_at_path(Jv::Null, rest, value);
                    new_arr.set(idx, new_child).ok();
                    Jv::Array(new_arr)
                } else {
                    input
                }
            } else {
                input
            }
        }
        _ => input,
    }
}

/// The jq interpreter
pub struct Interpreter {
    ctx: Rc<RefCell<Context>>,
}

impl Interpreter {
    /// Create a new interpreter with default context
    pub fn new() -> Self {
        Interpreter {
            ctx: Rc::new(RefCell::new(Context::new())),
        }
    }

    /// Create an interpreter with a custom context
    pub fn with_context(ctx: Context) -> Self {
        Interpreter {
            ctx: Rc::new(RefCell::new(ctx)),
        }
    }

    /// Evaluate an expression with the given input
    pub fn eval(&mut self, expr: &Expr, input: Jv) -> EvalResult {
        self.eval_expr(expr, input, self.ctx.clone())
    }

    fn eval_expr(&mut self, expr: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        match &expr.kind {
            ExprKind::Identity => {
                Box::new(std::iter::once(Ok(input)))
            }

            ExprKind::RecursiveDescent => {
                // .. is equivalent to recurse
                self.recurse(input)
            }

            ExprKind::Literal(lit) => {
                let v = match lit {
                    Literal::Null => Jv::Null,
                    Literal::Bool(b) => Jv::Bool(*b),
                    Literal::Number(n) => Jv::from_f64(*n),
                    Literal::LiteralNumber(s) => Jv::literal_number(s),
                    Literal::String(s) => Jv::string(s),
                };
                Box::new(std::iter::once(Ok(v)))
            }

            ExprKind::Field(name) => {
                let result = input.get_field(name);
                if result.is_invalid() {
                    Box::new(std::iter::once(Err(format!(
                        "Cannot index {} with string \"{}\"",
                        input.type_name(),
                        name
                    ))))
                } else {
                    Box::new(std::iter::once(Ok(result)))
                }
            }

            ExprKind::Index { expr: base, index, optional } => {
                let optional = *optional;
                let index_expr = index.clone();
                let base_expr = base.clone();
                let ctx_clone = ctx.clone();
                let original_input = input.clone();

                let mut this = Interpreter { ctx: ctx.clone() };

                // Get base values
                let base_results = this.eval_expr(&base_expr, input, ctx_clone.clone());

                Box::new(base_results.flat_map(move |base_result| {
                    match base_result {
                        Err(e) => {
                            if optional {
                                Box::new(std::iter::empty()) as EvalResult
                            } else {
                                Box::new(std::iter::once(Err(e)))
                            }
                        }
                        Ok(base_val) => {
                            let mut inner = Interpreter { ctx: ctx_clone.clone() };
                            // Evaluate index expression with original input, not base_val
                            let index_results = inner.eval_expr(&index_expr, original_input.clone(), ctx_clone.clone());

                            let base_val_for_index = base_val;
                            let optional_inner = optional;

                            Box::new(index_results.filter_map(move |idx_result| {
                                match idx_result {
                                    Err(e) => {
                                        if optional_inner {
                                            None
                                        } else {
                                            Some(Err(e))
                                        }
                                    }
                                    Ok(idx_val) => {
                                        let result = base_val_for_index.index(&idx_val);
                                        if result.is_invalid() {
                                            if optional_inner {
                                                None
                                            } else {
                                                let idx_desc = match &idx_val {
                                                    Jv::String(s) => format!("string (\"{}\")", s.as_str()),
                                                    Jv::Number(n) => format!("number ({})", n),
                                                    _ => idx_val.type_name().to_string(),
                                                };
                                                Some(Err(format!(
                                                    "Cannot index {} with {}",
                                                    base_val_for_index.type_name(),
                                                    idx_desc
                                                )))
                                            }
                                        } else {
                                            Some(Ok(result))
                                        }
                                    }
                                }
                            })) as EvalResult
                        }
                    }
                }))
            }

            ExprKind::Slice { expr: base, start, end, optional } => {
                let optional = *optional;
                let base_expr = base.clone();
                let start_expr = start.clone();
                let end_expr = end.clone();
                let ctx_clone = ctx.clone();
                let original_input = input.clone();

                let mut this = Interpreter { ctx: ctx.clone() };
                let base_results = this.eval_expr(&base_expr, input, ctx_clone.clone());

                // Helper to convert a number to i64 using floor (for start index)
                fn number_to_start_index(v: &Jv) -> Option<i64> {
                    match v {
                        Jv::Number(n) => {
                            // NaN means "no start" - start from beginning (index 0)
                            if n.is_nan() {
                                None
                            } else if let Some(i) = n.as_i64() {
                                Some(i)
                            } else {
                                Some(n.as_f64().floor() as i64)
                            }
                        }
                        _ => None,
                    }
                }

                // Helper to convert a number to i64 using ceil (for end index)
                fn number_to_end_index(v: &Jv) -> Option<i64> {
                    match v {
                        Jv::Number(n) => {
                            // Use ceil for end index, but exact integers stay as is
                            // NaN means "no end" - slice to end of array
                            if n.is_nan() {
                                None
                            } else if let Some(i) = n.as_i64() {
                                Some(i)
                            } else {
                                Some(n.as_f64().ceil() as i64)
                            }
                        }
                        _ => None,
                    }
                }

                Box::new(base_results.flat_map(move |base_result| {
                    match base_result {
                        Err(e) if !optional => Box::new(std::iter::once(Err(e))) as EvalResult,
                        Err(_) => Box::new(std::iter::empty()),
                        Ok(base_val) => {
                            // Evaluate start index with original input
                            let start_val = if let Some(ref s) = start_expr {
                                let mut inner = Interpreter { ctx: ctx_clone.clone() };
                                let mut results = inner.eval_expr(s, original_input.clone(), ctx_clone.clone());
                                match results.next() {
                                    Some(Ok(v)) => number_to_start_index(&v),
                                    _ => None,
                                }
                            } else {
                                None
                            };

                            // Evaluate end index with original input
                            let end_val = if let Some(ref e) = end_expr {
                                let mut inner = Interpreter { ctx: ctx_clone.clone() };
                                let mut results = inner.eval_expr(e, original_input.clone(), ctx_clone.clone());
                                match results.next() {
                                    Some(Ok(v)) => number_to_end_index(&v),
                                    _ => None,
                                }
                            } else {
                                None
                            };

                            match &base_val {
                                Jv::Null => Box::new(std::iter::once(Ok(Jv::Null))) as EvalResult,
                                Jv::Array(arr) => {
                                    let result = arr.slice(start_val, end_val);
                                    Box::new(std::iter::once(Ok(Jv::Array(result)))) as EvalResult
                                }
                                Jv::String(s) => {
                                    let len = s.char_len() as i64;
                                    // Handle negative indices
                                    let start_idx = match start_val {
                                        Some(i) if i < 0 => (len + i).max(0) as usize,
                                        Some(i) => i as usize,
                                        None => 0,
                                    };
                                    let end_idx = match end_val {
                                        Some(i) if i < 0 => (len + i).max(0) as usize,
                                        Some(i) => i as usize,
                                        None => len as usize,
                                    };
                                    let result = s.slice(start_idx, end_idx);
                                    Box::new(std::iter::once(Ok(Jv::String(result)))) as EvalResult
                                }
                                _ if optional => Box::new(std::iter::empty()) as EvalResult,
                                _ => Box::new(std::iter::once(Err(format!(
                                    "Cannot slice {}",
                                    base_val.type_name()
                                )))) as EvalResult,
                            }
                        }
                    }
                }))
            }

            ExprKind::Iterator { expr: base, optional } => {
                let optional = *optional;
                let base_expr = base.clone();
                let ctx_clone = ctx.clone();

                let mut this = Interpreter { ctx: ctx.clone() };
                let base_results = this.eval_expr(&base_expr, input, ctx_clone);

                Box::new(base_results.flat_map(move |base_result| {
                    match base_result {
                        Err(e) if !optional => Box::new(std::iter::once(Err(e))) as EvalResult,
                        Err(_) => Box::new(std::iter::empty()),
                        Ok(base_val) => {
                            match &base_val {
                                Jv::Array(arr) => {
                                    let items: Vec<_> = arr.iter().collect();
                                    Box::new(items.into_iter().map(Ok)) as EvalResult
                                }
                                Jv::Object(obj) => {
                                    let items: Vec<_> = obj.values().collect();
                                    Box::new(items.into_iter().map(Ok)) as EvalResult
                                }
                                Jv::Null if optional => Box::new(std::iter::empty()) as EvalResult,
                                _ if optional => Box::new(std::iter::empty()) as EvalResult,
                                _ => Box::new(std::iter::once(Err(format!(
                                    "Cannot iterate over {} ({})",
                                    base_val.type_name(),
                                    base_val
                                )))) as EvalResult,
                            }
                        }
                    }
                }))
            }

            ExprKind::Pipe(left, right) => {
                let right_expr = right.clone();
                let ctx_clone = ctx.clone();

                let mut this = Interpreter { ctx: ctx.clone() };
                let left_results = this.eval_expr(left, input, ctx_clone.clone());

                Box::new(left_results.flat_map(move |left_result| {
                    match left_result {
                        Err(e) => Box::new(std::iter::once(Err(e))) as EvalResult,
                        Ok(left_val) => {
                            let mut inner = Interpreter { ctx: ctx_clone.clone() };
                            inner.eval_expr(&right_expr, left_val, ctx_clone.clone())
                        }
                    }
                }))
            }

            ExprKind::Comma(left, right) => {
                let input_clone = input.clone();
                let ctx_clone = ctx.clone();

                let mut this = Interpreter { ctx: ctx.clone() };
                let left_results = this.eval_expr(left, input, ctx_clone.clone());

                // In jq, when an error occurs in a comma expression, the error propagates
                // and the rest of the comma expression is not evaluated
                struct CommaIter {
                    left: Box<dyn Iterator<Item = Result<Jv, String>>>,
                    right: Option<Box<dyn Iterator<Item = Result<Jv, String>>>>,
                    right_expr: Box<Expr>,
                    input: Jv,
                    ctx: Rc<RefCell<Context>>,
                    errored: bool,
                }

                impl Iterator for CommaIter {
                    type Item = Result<Jv, String>;

                    fn next(&mut self) -> Option<Self::Item> {
                        // If we've seen an error, stop producing values
                        if self.errored {
                            return None;
                        }

                        // Try to get next from left
                        if let Some(result) = self.left.next() {
                            if result.is_err() {
                                self.errored = true;
                            }
                            return Some(result);
                        }

                        // Left exhausted, try right
                        if self.right.is_none() {
                            let mut inner = Interpreter { ctx: self.ctx.clone() };
                            self.right = Some(inner.eval_expr(&self.right_expr, self.input.clone(), self.ctx.clone()));
                        }

                        if let Some(ref mut right_iter) = self.right {
                            if let Some(result) = right_iter.next() {
                                if result.is_err() {
                                    self.errored = true;
                                }
                                return Some(result);
                            }
                        }

                        None
                    }
                }

                Box::new(CommaIter {
                    left: left_results,
                    right: None,
                    right_expr: right.clone(),
                    input: input_clone,
                    ctx: ctx_clone,
                    errored: false,
                })
            }

            ExprKind::Conditional { condition, then_branch, else_branch } => {
                let condition_expr = condition.clone();
                let then_expr = then_branch.clone();
                let else_expr = else_branch.clone();
                let ctx_clone = ctx.clone();

                let mut this = Interpreter { ctx: ctx.clone() };
                let cond_results: Vec<_> = this.eval_expr(&condition_expr, input.clone(), ctx_clone.clone()).collect();

                Box::new(cond_results.into_iter().flat_map(move |cond_result| {
                    match cond_result {
                        Err(e) => Box::new(std::iter::once(Err(e))) as EvalResult,
                        Ok(v) => {
                            let mut inner = Interpreter { ctx: ctx_clone.clone() };
                            if v.is_truthy() {
                                inner.eval_expr(&then_expr, input.clone(), ctx_clone.clone())
                            } else if let Some(ref else_e) = else_expr {
                                inner.eval_expr(else_e, input.clone(), ctx_clone.clone())
                            } else {
                                // No else branch and condition is falsy: return identity (input)
                                Box::new(std::iter::once(Ok(input.clone())))
                            }
                        }
                    }
                }))
            }

            ExprKind::TryCatch { expr: try_expr, catch } => {
                let catch_expr = catch.clone();
                let ctx_clone = ctx.clone();

                let mut this = Interpreter { ctx: ctx.clone() };
                let results_iter = this.eval_expr(try_expr, input, ctx_clone.clone());

                // For try without catch (e?), on first error we stop iteration entirely
                // For try-catch (try e catch handler), errors run the handler
                struct TryCatchIter {
                    inner: Box<dyn Iterator<Item = Result<Jv, String>>>,
                    catch_expr: Option<Box<Expr>>,
                    ctx: Rc<RefCell<Context>>,
                    done: bool,
                }

                impl Iterator for TryCatchIter {
                    type Item = Result<Jv, String>;

                    fn next(&mut self) -> Option<Self::Item> {
                        if self.done {
                            return None;
                        }

                        match self.inner.next() {
                            Some(Ok(v)) => Some(Ok(v)),
                            Some(Err(e)) => {
                                // Check if this is a break signal - don't catch those
                                if e.starts_with(BREAK_PREFIX) {
                                    return Some(Err(e));
                                }

                                if let Some(ref catch_e) = self.catch_expr {
                                    // Convert error string to Jv input for catch handler
                                    let err_input = if e.starts_with(crate::vm::context::JSON_ERROR_PREFIX) {
                                        let json_str = &e[crate::vm::context::JSON_ERROR_PREFIX.len()..];
                                        crate::jv::parse_json(json_str).unwrap_or_else(|_| Jv::string(&e))
                                    } else {
                                        Jv::string(&e)
                                    };
                                    let mut inner = Interpreter { ctx: self.ctx.clone() };
                                    // Evaluate catch handler and return first result
                                    // Note: catch handler may produce multiple values
                                    match inner.eval_expr(catch_e, err_input, self.ctx.clone()).next() {
                                        Some(result) => Some(result),
                                        None => self.next(), // catch produced empty, continue
                                    }
                                } else {
                                    // No catch - suppress error AND stop iteration entirely
                                    // This is jq's behavior: e? stops on first error
                                    self.done = true;
                                    None
                                }
                            }
                            None => None,
                        }
                    }
                }

                Box::new(TryCatchIter {
                    inner: results_iter,
                    catch_expr,
                    ctx: ctx_clone,
                    done: false,
                })
            }

            ExprKind::BinaryOp { op, left, right } => {
                let op = *op;
                let left_expr = left.clone();
                let right_expr = right.clone();
                let input_clone = input.clone();
                let ctx_clone = ctx.clone();

                // Special handling for short-circuit operators (and, or)
                match op {
                    BinaryOp::And => {
                        // left and right: if left is falsy, return false without evaluating right
                        let mut this = Interpreter { ctx: ctx.clone() };
                        return Box::new(this.eval_expr(&left_expr, input, ctx_clone.clone()).flat_map(move |left_result| {
                            match left_result {
                                Err(e) => Box::new(std::iter::once(Err(e))) as EvalResult,
                                Ok(left_val) => {
                                    if !left_val.is_truthy() {
                                        // Short-circuit: return false without evaluating right
                                        Box::new(std::iter::once(Ok(Jv::Bool(false)))) as EvalResult
                                    } else {
                                        // Evaluate right side
                                        let mut inner = Interpreter { ctx: ctx_clone.clone() };
                                        Box::new(inner.eval_expr(&right_expr, input_clone.clone(), ctx_clone.clone()).map(|right_result| {
                                            match right_result {
                                                Err(e) => Err(e),
                                                Ok(right_val) => Ok(Jv::Bool(right_val.is_truthy())),
                                            }
                                        })) as EvalResult
                                    }
                                }
                            }
                        }));
                    }
                    BinaryOp::Or => {
                        // left or right: if left is truthy, return true without evaluating right
                        let mut this = Interpreter { ctx: ctx.clone() };
                        return Box::new(this.eval_expr(&left_expr, input, ctx_clone.clone()).flat_map(move |left_result| {
                            match left_result {
                                Err(e) => Box::new(std::iter::once(Err(e))) as EvalResult,
                                Ok(left_val) => {
                                    if left_val.is_truthy() {
                                        // Short-circuit: return true without evaluating right
                                        Box::new(std::iter::once(Ok(Jv::Bool(true)))) as EvalResult
                                    } else {
                                        // Evaluate right side
                                        let mut inner = Interpreter { ctx: ctx_clone.clone() };
                                        Box::new(inner.eval_expr(&right_expr, input_clone.clone(), ctx_clone.clone()).map(|right_result| {
                                            match right_result {
                                                Err(e) => Err(e),
                                                Ok(right_val) => Ok(Jv::Bool(right_val.is_truthy())),
                                            }
                                        })) as EvalResult
                                    }
                                }
                            }
                        }));
                    }
                    _ => {}
                }

                // For other operators, evaluate both sides
                let mut this = Interpreter { ctx: ctx.clone() };
                let right_results = this.eval_expr(right, input, ctx_clone.clone());

                // jq semantics: for each right value, iterate over all left values
                Box::new(right_results.flat_map(move |right_result| {
                    match right_result {
                        Err(e) => Box::new(std::iter::once(Err(e))) as EvalResult,
                        Ok(right_val) => {
                            let mut inner = Interpreter { ctx: ctx_clone.clone() };
                            let left_results = inner.eval_expr(&left_expr, input_clone.clone(), ctx_clone.clone());

                            Box::new(left_results.map(move |left_result| {
                                match left_result {
                                    Err(e) => Err(e),
                                    Ok(left_val) => eval_binary_op(op, &left_val, &right_val),
                                }
                            })) as EvalResult
                        }
                    }
                }))
            }

            ExprKind::Negate(inner) => {
                let inner_expr = inner.clone();
                let ctx_clone = ctx.clone();

                let mut this = Interpreter { ctx: ctx.clone() };
                let results = this.eval_expr(&inner_expr, input, ctx_clone);

                Box::new(results.map(|result| {
                    match result {
                        Err(e) => Err(e),
                        Ok(Jv::Number(n)) => Ok(Jv::Number(n.neg())),
                        Ok(Jv::LiteralNumber(s)) => {
                            // Negate a literal number by toggling the sign
                            if s.starts_with('-') {
                                Ok(Jv::LiteralNumber(s[1..].to_string()))
                            } else {
                                Ok(Jv::LiteralNumber(format!("-{}", s)))
                            }
                        }
                        Ok(v) => Err(format!("{} cannot be negated", format_value_for_error(&v))),
                    }
                }))
            }

            ExprKind::Optional(inner) => {
                let inner_expr = inner.clone();
                let ctx_clone = ctx.clone();

                let mut this = Interpreter { ctx: ctx.clone() };
                let results = this.eval_expr(&inner_expr, input, ctx_clone);

                Box::new(results.filter_map(|result| {
                    match result {
                        Ok(v) => Some(Ok(v)),
                        Err(_) => None,
                    }
                }))
            }

            ExprKind::Array(inner) => {
                match inner {
                    None => Box::new(std::iter::once(Ok(Jv::array()))),
                    Some(inner_expr) => {
                        let ctx_clone = ctx.clone();
                        let mut this = Interpreter { ctx: ctx.clone() };
                        let results: Result<Vec<_>, _> = this.eval_expr(inner_expr, input, ctx_clone)
                            .collect();

                        match results {
                            Ok(values) => Box::new(std::iter::once(Ok(Jv::from_vec(values)))),
                            Err(e) => Box::new(std::iter::once(Err(e))),
                        }
                    }
                }
            }

            ExprKind::Object(entries) => {
                self.eval_object(entries, input, ctx)
            }

            ExprKind::FunctionCall { module, name, args } => {
                self.eval_function_call(module.as_deref(), name, args, input, ctx)
            }

            ExprKind::Variable(name) => {
                // Special case for $ENV
                if name == "ENV" {
                    return self.eval_env(input);
                }
                match ctx.borrow().lookup_value(name) {
                    Some(v) => Box::new(std::iter::once(Ok(v))),
                    None => Box::new(std::iter::once(Err(format!("variable ${} is not defined", name)))),
                }
            }

            ExprKind::Binding { expr: bind_expr, pattern, body } => {
                let body_expr = body.clone();
                let pattern = pattern.clone();
                let ctx_clone = ctx.clone();
                let input_clone = input.clone();

                let mut this = Interpreter { ctx: ctx.clone() };
                let bind_results = this.eval_expr(bind_expr, input, ctx_clone.clone());

                Box::new(bind_results.flat_map(move |bind_result| {
                    match bind_result {
                        Err(e) => Box::new(std::iter::once(Err(e))) as EvalResult,
                        Ok(bind_val) => {
                            // Create child context with bindings from pattern
                            let child_ctx = Rc::new(RefCell::new(Context::child(ctx_clone.clone())));

                            // Try to bind the pattern
                            let mut inner = Interpreter { ctx: child_ctx.clone() };
                            if let Err(e) = inner.bind_pattern(&pattern, &bind_val, &child_ctx) {
                                return Box::new(std::iter::once(Err(e))) as EvalResult;
                            }

                            let mut inner = Interpreter { ctx: child_ctx.clone() };
                            inner.eval_expr(&body_expr, input_clone.clone(), child_ctx)
                        }
                    }
                }))
            }

            ExprKind::Reduce { expr: iter_expr, pattern, init, update } => {
                self.eval_reduce(iter_expr, pattern, init, update, input, ctx)
            }

            ExprKind::Foreach { expr: iter_expr, pattern, init, update, extract } => {
                self.eval_foreach(iter_expr, pattern, init, update, extract.as_ref(), input, ctx)
            }

            ExprKind::Alternative(left, right) => {
                let right_expr = right.clone();
                let input_clone = input.clone();
                let ctx_clone = ctx.clone();

                let mut this = Interpreter { ctx: ctx.clone() };
                let left_results = this.eval_expr(left, input, ctx_clone.clone()).peekable();

                // Try to get results from left
                let mut got_result = false;
                let results: Vec<_> = left_results
                    .filter_map(|r| {
                        match r {
                            Ok(ref v) if !v.is_null() && !matches!(v, Jv::Bool(false)) => {
                                got_result = true;
                                Some(r)
                            }
                            Ok(_) => None,
                            Err(_) => None,
                        }
                    })
                    .collect();

                if got_result {
                    Box::new(results.into_iter())
                } else {
                    let mut inner = Interpreter { ctx: ctx_clone.clone() };
                    inner.eval_expr(&right_expr, input_clone, ctx_clone)
                }
            }

            ExprKind::StringInterp(parts) => {
                self.eval_string_interp(parts, input, ctx)
            }

            ExprKind::LocalDef { def, body } => {
                // Register function in context with closure capturing current context
                let child_ctx = Rc::new(RefCell::new(Context::child(ctx.clone())));
                // The closure captures the child context (which has parent = ctx)
                // so that the function can see bindings from when it was defined
                child_ctx.borrow_mut().bind_function(&def.name, Rc::new(def.clone()), child_ctx.clone());

                let mut inner = Interpreter { ctx: child_ctx.clone() };
                inner.eval_expr(body, input, child_ctx)
            }

            ExprKind::Paren(inner) => {
                let mut this = Interpreter { ctx: ctx.clone() };
                this.eval_expr(inner, input, ctx)
            }

            ExprKind::Loc => {
                // Return location object - simplified version
                let mut obj = JvObject::new();
                obj.set("file", Jv::string("<top-level>"));
                obj.set("line", Jv::from_i64(1));
                Box::new(std::iter::once(Ok(Jv::Object(obj))))
            }

            ExprKind::Format { format, expr } => {
                let format_name = format!("@{}", format);
                let ctx_clone = ctx.clone();

                // If expr is provided with a string template, format interpolations
                if let Some(inner_expr) = expr {
                    // Check if it's a string with interpolations that needs special handling
                    if let ExprKind::StringInterp(parts) = &inner_expr.kind {
                        return self.eval_format_template(format, parts, input, ctx);
                    }

                    // Otherwise evaluate the expression and format the result
                    let mut inner = Interpreter { ctx: ctx.clone() };
                    let results: Vec<_> = inner.eval_expr(inner_expr, input, ctx_clone.clone()).collect();

                    Box::new(results.into_iter().flat_map(move |result| {
                        match result {
                            Err(e) => Box::new(std::iter::once(Err(e))) as EvalResult,
                            Ok(val) => {
                                let ctx_mut = ctx_clone.borrow_mut();
                                if let Some(builtin) = ctx_mut.get_builtin(&format_name, 0) {
                                    let builtin_fn = *builtin;
                                    drop(ctx_mut);
                                    builtin_fn(&mut Context::new(), val, &[])
                                } else {
                                    Box::new(std::iter::once(Err(format!("unknown format: {}", format_name))))
                                }
                            }
                        }
                    }))
                } else {
                    // No expression - apply format to input
                    let ctx_mut = ctx_clone.borrow_mut();
                    if let Some(builtin) = ctx_mut.get_builtin(&format_name, 0) {
                        let builtin_fn = *builtin;
                        drop(ctx_mut);
                        builtin_fn(&mut Context::new(), input, &[])
                    } else {
                        Box::new(std::iter::once(Err(format!("unknown format: {}", format_name))))
                    }
                }
            }

            ExprKind::Assign { target, value } => {
                // Evaluate the value
                let value_expr = value.clone();
                let target_expr = target.clone();
                let ctx_clone = ctx.clone();

                let mut this = Interpreter { ctx: ctx.clone() };
                let value_results: Vec<_> = this.eval_expr(&value_expr, input.clone(), ctx_clone.clone()).collect();

                Box::new(value_results.into_iter().flat_map(move |value_result| {
                    match value_result {
                        Err(e) => Box::new(std::iter::once(Err(e))) as EvalResult,
                        Ok(new_value) => {
                            // Apply the assignment by computing the path and setting
                            let mut path_parts: Vec<Jv> = Vec::new();
                            let modified = Self::apply_assignment(
                                input.clone(),
                                &target_expr,
                                new_value,
                                &mut path_parts,
                                ctx_clone.clone(),
                            );
                            match modified {
                                Ok(v) => Box::new(std::iter::once(Ok(v))) as EvalResult,
                                Err(e) => Box::new(std::iter::once(Err(e))) as EvalResult,
                            }
                        }
                    }
                }))
            }

            ExprKind::Update { target, value } => {
                // expr |= f means: evaluate f with current value at target, then set result back
                let target_expr = target.clone();
                let value_expr = value.clone();
                let ctx_clone = ctx.clone();

                // Unwrap Paren if present
                let effective_target = if let ExprKind::Paren(inner) = &target_expr.kind {
                    inner.as_ref()
                } else {
                    &target_expr
                };

                // Special handling for comma targets: (.a, .b) |= f
                // For comma expressions, we apply the update once and use the same value for all paths
                if let ExprKind::Comma(left, right) = &effective_target.kind {
                    let left = left.clone();
                    let right = right.clone();
                    let ctx_for_closure = ctx_clone.clone();

                    return Box::new(std::iter::once((|| {
                        // Get the first value at the first path
                        let mut get_interp = Interpreter { ctx: ctx_for_closure.clone() };
                        let current_val = match get_interp.eval_expr(&left, input.clone(), ctx_for_closure.clone()).next() {
                            Some(Ok(v)) => v,
                            Some(Err(e)) => return Err(e),
                            None => Jv::Null,
                        };

                        // Apply the value expression to get the new value
                        let mut val_interp = Interpreter { ctx: ctx_for_closure.clone() };
                        let new_value = match val_interp.eval_expr(&value_expr, current_val, ctx_for_closure.clone()).next() {
                            Some(Ok(v)) => v,
                            Some(Err(e)) => return Err(e),
                            None => return Ok(input.clone()), // empty produces no output, return input unchanged
                        };

                        // Apply the value to both paths
                        let mut path_parts: Vec<Jv> = Vec::new();
                        let result = Self::apply_assignment(input.clone(), &left, new_value.clone(), &mut path_parts, ctx_for_closure.clone())?;
                        Self::apply_assignment(result, &right, new_value, &mut path_parts, ctx_for_closure)
                    })()));
                }

                // Special handling for iterator targets like .[] |= f
                if let ExprKind::Iterator { expr: iter_base, optional: _ } = &effective_target.kind {
                    return self.apply_update_to_iterator(
                        input,
                        iter_base,
                        &value_expr,
                        ctx_clone,
                    );
                }

                // Special handling for (.[] | filter) |= f
                // This is a path-based update that should update elements where the filter matches
                if let ExprKind::Pipe(left, right) = &effective_target.kind {
                    if let ExprKind::Iterator { expr: iter_base, optional: _ } = &left.kind {
                        // (.[] | filter) |= f - for each element, if filter passes, apply f
                        let iter_base_clone = iter_base.clone();
                        let filter_expr = right.clone();
                        let value_expr_clone = value_expr.clone();
                        let ctx_for_closure = ctx_clone.clone();

                        return Box::new(std::iter::once((|| {
                            // Get the container
                            let container = if let ExprKind::Identity = iter_base_clone.kind {
                                input.clone()
                            } else {
                                let mut interp = Interpreter { ctx: ctx_for_closure.clone() };
                                match interp.eval_expr(&iter_base_clone, input.clone(), ctx_for_closure.clone()).next() {
                                    Some(Ok(v)) => v,
                                    Some(Err(e)) => return Err(e),
                                    None => return Err("iterator base produced no value".to_string()),
                                }
                            };

                            match container {
                                Jv::Array(arr) => {
                                    // For each element, check if filter passes, if so apply the update
                                    let mut result = Vec::new();
                                    for elem in arr.iter() {
                                        // Check if element passes the filter
                                        let mut filter_interp = Interpreter { ctx: ctx_for_closure.clone() };
                                        let filter_result = filter_interp.eval_expr(&filter_expr, elem.clone(), ctx_for_closure.clone()).next();

                                        match filter_result {
                                            Some(Ok(_)) => {
                                                // Filter matched - apply the value expression
                                                let mut val_interp = Interpreter { ctx: ctx_for_closure.clone() };
                                                match val_interp.eval_expr(&value_expr_clone, elem.clone(), ctx_for_closure.clone()).next() {
                                                    Some(Ok(v)) => result.push(v),
                                                    Some(Err(e)) => return Err(e),
                                                    None => {
                                                        // Value expression returned empty - delete element
                                                    }
                                                }
                                            }
                                            Some(Err(e)) => return Err(e),
                                            None => {
                                                // Filter didn't match - keep element unchanged
                                                result.push(elem);
                                            }
                                        }
                                    }
                                    let new_container = Jv::from_vec(result);

                                    // If base is identity, return directly
                                    if let ExprKind::Identity = iter_base_clone.kind {
                                        Ok(new_container)
                                    } else {
                                        let mut path_parts: Vec<Jv> = Vec::new();
                                        Self::apply_assignment(input.clone(), &iter_base_clone, new_container, &mut path_parts, ctx_for_closure)
                                    }
                                }
                                _ => Err(format!("Cannot iterate over {}", container.type_name())),
                            }
                        })()));
                    }

                    // Also handle .foo[] |= f pattern (Pipe where right is Iterator)
                    if let ExprKind::Iterator { expr: iter_base, optional: _ } = &right.kind {
                        // .foo[] |= f - navigate to .foo, apply update to its iterator
                        let left_clone = left.clone();
                        let iter_base_clone = iter_base.clone();
                        let value_expr_clone = value_expr.clone();
                        let ctx_for_closure = ctx_clone.clone();

                        return Box::new(std::iter::once((|| {
                            let mut left_interp = Interpreter { ctx: ctx_for_closure.clone() };
                            let container = match left_interp.eval_expr(&left_clone, input.clone(), ctx_for_closure.clone()).next() {
                                Some(Ok(v)) => v,
                                Some(Err(e)) => return Err(e),
                                None => return Err("left side produced no value".to_string()),
                            };

                            // Apply update to iterator on the container
                            let updated_container = self.apply_update_to_iterator_sync(
                                container,
                                &iter_base_clone,
                                &value_expr_clone,
                                ctx_for_closure.clone(),
                            )?;

                            // Set the updated container back
                            let mut path_parts: Vec<Jv> = Vec::new();
                            Self::apply_assignment(input.clone(), &left_clone, updated_container, &mut path_parts, ctx_for_closure)
                        })()));
                    }
                }

                // Special handling for index expressions with generators like .foo[1,4,2,3] |= empty
                // When the value expression returns empty, we delete those indices
                if let ExprKind::Index { expr: base_expr, index: idx_expr, optional: _ } = &effective_target.kind {
                    // Check if index expression might be a generator (contains Comma)
                    if Self::contains_comma(&idx_expr.kind) {
                        return self.apply_update_to_indexed_generator(
                            input,
                            base_expr,
                            idx_expr,
                            &value_expr,
                            ctx_clone,
                        );
                    }
                }

                // Special handling for recursive descent: .. |= f
                if let ExprKind::RecursiveDescent = &effective_target.kind {
                    return self.apply_update_recursive_descent(
                        input,
                        &value_expr,
                        None,
                        None,
                        ctx_clone,
                    );
                }

                // Special handling for (.. | filter) |= f or (.. | filter | path) |= f
                // Check if the leftmost element of the pipe chain is RecursiveDescent
                if let ExprKind::Pipe(left, right) = &effective_target.kind {
                    if let ExprKind::RecursiveDescent = &left.kind {
                        // Check if right is also a pipe: .. | (filter | path)
                        if let ExprKind::Pipe(filter_expr, path_expr) = &right.kind {
                            // (.. | filter | path) |= f
                            return self.apply_update_recursive_descent(
                                input,
                                &value_expr,
                                Some(filter_expr.as_ref()),
                                Some(path_expr.as_ref()),
                                ctx_clone,
                            );
                        }
                        // Simple case: (.. | filter) |= f
                        return self.apply_update_recursive_descent(
                            input,
                            &value_expr,
                            Some(right.as_ref()),
                            None,
                            ctx_clone,
                        );
                    }
                    // Also check left-associative: (.. | filter) | path = Pipe(Pipe(.., filter), path)
                    if let ExprKind::Pipe(ll, lr) = &left.kind {
                        if let ExprKind::RecursiveDescent = &ll.kind {
                            // (.. | filter) | path |= f
                            return self.apply_update_recursive_descent(
                                input,
                                &value_expr,
                                Some(lr.as_ref()),
                                Some(right.as_ref()),
                                ctx_clone,
                            );
                        }
                    }
                }

                // Use path-based atomic update: collect all paths, then apply updates atomically
                // This is how jq's _modify function works: reduce path(target) as $p ...
                return self.apply_update_with_paths(input, &target_expr, &value_expr, ctx_clone);
            }

            ExprKind::UpdateOp { op, target, value } => {
                // expr += f means: evaluate f and apply arithmetic op to current value
                let op = *op;
                let target_expr = target.clone();
                let value_expr = value.clone();
                let ctx_clone = ctx.clone();

                // Special handling for iterator targets like .[] += 2
                if let ExprKind::Iterator { expr: iter_base, optional: _ } = &target_expr.kind {
                    return self.apply_updateop_to_iterator(
                        input,
                        iter_base,
                        &value_expr,
                        op,
                        ctx_clone,
                    );
                }

                // Get current value at target
                let mut get_interp = Interpreter { ctx: ctx.clone() };
                let current_results: Vec<_> = get_interp.eval_expr(&target_expr, input.clone(), ctx_clone.clone()).collect();

                Box::new(current_results.into_iter().flat_map(move |current_result| {
                    match current_result {
                        Err(e) => Box::new(std::iter::once(Err(e))) as EvalResult,
                        Ok(current_val) => {
                            // Evaluate the right-hand value
                            let mut val_interp = Interpreter { ctx: ctx_clone.clone() };
                            let right_val = match val_interp.eval_expr(&value_expr, input.clone(), ctx_clone.clone()).next() {
                                Some(Ok(v)) => v,
                                Some(Err(e)) => return Box::new(std::iter::once(Err(e))) as EvalResult,
                                None => return Box::new(std::iter::empty()) as EvalResult,
                            };

                            // Apply the operation
                            let new_value = match eval_binary_op(op, &current_val, &right_val) {
                                Ok(v) => v,
                                Err(e) => return Box::new(std::iter::once(Err(e))) as EvalResult,
                            };

                            let mut path_parts: Vec<Jv> = Vec::new();
                            let modified = Self::apply_assignment(
                                input.clone(),
                                &target_expr,
                                new_value,
                                &mut path_parts,
                                ctx_clone.clone(),
                            );
                            match modified {
                                Ok(v) => Box::new(std::iter::once(Ok(v))) as EvalResult,
                                Err(e) => Box::new(std::iter::once(Err(e))) as EvalResult,
                            }
                        }
                    }
                }))
            }

            ExprKind::Label { name, body } => {
                // Evaluate body, catching break signals for this label
                let label_name = name.clone();
                let body_expr = body.clone();
                let mut body_interp = Interpreter { ctx: ctx.clone() };
                let mut results = Vec::new();

                for result in body_interp.eval_expr(&body_expr, input, ctx) {
                    match result {
                        Ok(v) => results.push(Ok(v)),
                        Err(e) if is_break_for(&e, &label_name) => {
                            // Break caught - stop iteration but don't propagate error
                            break;
                        }
                        Err(e) => {
                            // Other error - propagate it
                            results.push(Err(e));
                            break;
                        }
                    }
                }

                Box::new(results.into_iter())
            }

            ExprKind::Break(label) => {
                // Signal a break to the corresponding label
                Box::new(std::iter::once(Err(make_break_signal(label))))
            }

            ExprKind::WithImports { imports, module_meta: _, body } => {
                // Process imports and add bindings to context
                // Create a module loader and process the imports
                let mut loader = crate::module::ModuleLoader::new();

                // Process each import
                if let Err(e) = loader.process_imports(imports, &ctx) {
                    return Box::new(std::iter::once(Err(e)));
                }

                // Evaluate the body with the updated context
                self.eval_expr(body, input, ctx)
            }
        }
    }

    fn eval_object(&mut self, entries: &[crate::parser::ObjectEntry], input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        // Object construction with generators: {x: (1,2)} produces {x:1}, {x:2}
        // For multiple entries with generators, we compute the cartesian product

        // Evaluate all entries and collect their results
        let mut entry_results: Vec<Vec<(String, Jv)>> = Vec::new();

        for entry in entries {
            // Evaluate key
            let key_strs: Vec<String> = match &entry.key {
                ObjectKey::Ident(s) | ObjectKey::String(s) | ObjectKey::Shorthand(s) => {
                    vec![s.clone()]
                }
                ObjectKey::Expr(key_expr) => {
                    let mut key_interp = Interpreter { ctx: ctx.clone() };
                    let mut keys = Vec::new();
                    for result in key_interp.eval_expr(key_expr, input.clone(), ctx.clone()) {
                        match result {
                            Ok(Jv::String(s)) => keys.push(s.as_str().to_string()),
                            Ok(v) => return Box::new(std::iter::once(Err(format!(
                                "cannot use {} as object key", v.type_name()
                            )))),
                            Err(e) => return Box::new(std::iter::once(Err(e))),
                        }
                    }
                    if keys.is_empty() {
                        continue;
                    }
                    keys
                }
            };

            // Evaluate value
            let mut val_interp = Interpreter { ctx: ctx.clone() };
            let mut values: Vec<Jv> = Vec::new();
            for result in val_interp.eval_expr(&entry.value, input.clone(), ctx.clone()) {
                match result {
                    Ok(v) => values.push(v),
                    Err(e) => return Box::new(std::iter::once(Err(e))),
                }
            }

            // Combine keys and values (cartesian product for this entry)
            let mut entry_combos: Vec<(String, Jv)> = Vec::new();
            for key in &key_strs {
                for val in &values {
                    entry_combos.push((key.clone(), val.clone()));
                }
            }

            if !entry_combos.is_empty() {
                entry_results.push(entry_combos);
            }
        }

        if entry_results.is_empty() {
            return Box::new(std::iter::once(Ok(Jv::Object(JvObject::new()))));
        }

        // Compute cartesian product of all entries
        fn cartesian_product(lists: &[Vec<(String, Jv)>]) -> Vec<Vec<(String, Jv)>> {
            if lists.is_empty() {
                return vec![vec![]];
            }
            let first = &lists[0];
            let rest = cartesian_product(&lists[1..]);
            let mut result = Vec::new();
            for item in first {
                for r in &rest {
                    let mut combo = vec![item.clone()];
                    combo.extend(r.iter().cloned());
                    result.push(combo);
                }
            }
            result
        }

        let combinations = cartesian_product(&entry_results);
        let results: Vec<_> = combinations.into_iter().map(|combo| {
            let mut obj = JvObject::new();
            for (key, val) in combo {
                obj.set(&key, val);
            }
            Ok(Jv::Object(obj))
        }).collect();

        Box::new(results.into_iter())
    }

    fn eval_function_call(&mut self, module: Option<&str>, name: &str, args: &[Expr], input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        let arity = args.len();

        // If module is specified, look up in that module's namespace
        if let Some(mod_name) = module {
            let maybe_func = ctx.borrow().lookup_module_function(mod_name, name, arity);
            if let Some((func_def, closure_ctx)) = maybe_func {
                return self.call_user_function(&func_def, args, input, ctx, closure_ctx);
            }
            // Also check for data variable: $mod::var
            let var_name = format!("{}::{}", mod_name, name);
            if let Some(v) = ctx.borrow().lookup_value(&var_name) {
                return Box::new(std::iter::once(Ok(v)));
            }
            return Box::new(std::iter::once(Err(format!(
                "{}::{}/{} is not defined", mod_name, name, arity
            ))));
        }

        // Check for special built-in higher-order functions
        match (name, arity) {
            ("map", 1) => return self.eval_map(&args[0], input, ctx),
            ("select", 1) => return self.eval_select(&args[0], input, ctx),
            ("recurse", 0) => return self.recurse(input),
            ("recurse", 1) => return self.eval_recurse_with(&args[0], input, ctx),
            ("recurse", 2) => return self.eval_recurse_with_cond(&args[0], &args[1], input, ctx),
            ("recurse_down", 0) => return self.recurse(input),
            ("range", 1) => return self.eval_range1(&args[0], input, ctx),
            ("range", 2) => return self.eval_range2(&args[0], &args[1], input, ctx),
            ("limit", 2) => return self.eval_limit(&args[0], &args[1], input, ctx),
            ("skip", 2) => return self.eval_skip(&args[0], &args[1], input, ctx),
            ("first", 1) => return self.eval_first_expr(&args[0], input, ctx),
            ("group_by", 1) => return self.eval_group_by(&args[0], input, ctx),
            ("sort_by", 1) => return self.eval_sort_by(&args[0], input, ctx),
            ("unique_by", 1) => return self.eval_unique_by(&args[0], input, ctx),
            ("max_by", 1) => return self.eval_max_by(&args[0], input, ctx),
            ("min_by", 1) => return self.eval_min_by(&args[0], input, ctx),
            ("any", 0) => return self.eval_any_simple(input),
            ("any", 1) => return self.eval_any_filter(&args[0], input, ctx),
            ("any", 2) => return self.eval_any_gen_filter(&args[0], &args[1], input, ctx),
            ("all", 0) => return self.eval_all_simple(input),
            ("all", 1) => return self.eval_all_filter(&args[0], input, ctx),
            ("all", 2) => return self.eval_all_gen_filter(&args[0], &args[1], input, ctx),
            ("IN", 1) => return self.eval_in_stream(&args[0], input, ctx),
            ("IN", 2) => return self.eval_in_stream2(&args[0], &args[1], input, ctx),
            ("del", 1) => return self.eval_del(&args[0], input, ctx),
            ("getpath", 1) => return self.eval_getpath(&args[0], input, ctx),
            ("isempty", 1) => return self.eval_isempty(&args[0], input, ctx),
            ("until", 2) => return self.eval_until(&args[0], &args[1], input, ctx),
            ("while", 2) => return self.eval_while(&args[0], &args[1], input, ctx),
            ("repeat", 1) => return self.eval_repeat(&args[0], input, ctx),
            ("range", 3) => return self.eval_range3(&args[0], &args[1], &args[2], input, ctx),
            ("walk", 1) => return self.eval_walk(&args[0], input, ctx),
            ("env", 0) => return self.eval_env(input),
            ("$ENV", 0) => return self.eval_env(input),
            ("splits", 1) => return self.eval_splits(&args[0], input, ctx),
            ("with_entries", 1) => return self.eval_with_entries(&args[0], input, ctx),
            ("map_values", 1) => return self.eval_map_values(&args[0], input, ctx),
            ("path", 1) => return self.eval_path(&args[0], input, ctx),
            ("add", 1) => return self.eval_add_gen(&args[0], input, ctx),
            ("paths", 1) => return self.eval_paths_filter(&args[0], input, ctx),
            ("pick", 1) => return self.eval_pick(&args[0], input, ctx),
            ("nth", 2) => return self.eval_nth(&args[0], &args[1], input, ctx),
            ("last", 1) => return self.eval_last_expr(&args[0], input, ctx),
            ("INDEX", 1) => return self.eval_index1(&args[0], input, ctx),
            ("INDEX", 2) => return self.eval_index2(&args[0], &args[1], input, ctx),
            ("JOIN", 2) => return self.eval_join2(&args[0], &args[1], input, ctx),
            ("JOIN", 3) => return self.eval_join3(&args[0], &args[1], &args[2], input, ctx),
            ("sub", 2) => return self.eval_sub(&args[0], &args[1], input, ctx, false),
            ("gsub", 2) => return self.eval_sub(&args[0], &args[1], input, ctx, true),
            ("sub", 3) => return self.eval_sub_flags(&args[0], &args[1], &args[2], input, ctx, false),
            ("gsub", 3) => return self.eval_sub_flags(&args[0], &args[1], &args[2], input, ctx, true),
            ("truncate_stream", 1) => return self.eval_truncate_stream(&args[0], input, ctx),
            ("fromstream", 1) => return self.eval_fromstream(&args[0], input, ctx),
            ("ascii_downcase", 0) | ("ascii_upcase", 0) => {
                // These are handled as regular builtins
            }
            _ => {}
        }

        // Check for user-defined function
        let maybe_func = ctx.borrow().lookup_function(name, arity);
        if let Some((func_def, closure_ctx)) = maybe_func {
            // Use closure_ctx (definition context) for function body evaluation,
            // but ctx (call-site context) for evaluating arguments
            return self.call_user_function(&func_def, args, input, ctx, closure_ctx);
        }

        // Check for builtin
        let has_builtin = ctx.borrow().has_builtin(name, arity);
        if has_builtin {
            // For builtins, we need to iterate over all combinations of argument values
            // e.g., index(",", "|") should call index(",") then index("|")

            if arity == 0 {
                let builtin = ctx.borrow().get_builtin(name, arity).copied();
                if let Some(func) = builtin {
                    return func(&mut ctx.borrow_mut(), input, &[]);
                }
            } else if arity == 1 {
                // Single argument - iterate over all values from the argument expression
                let mut arg_interp = Interpreter { ctx: ctx.clone() };
                let arg_results: Vec<_> = arg_interp.eval_expr(&args[0], input.clone(), ctx.clone()).collect();

                let builtin = ctx.borrow().get_builtin(name, arity).copied();
                if let Some(func) = builtin {
                    let ctx_for_builtin = ctx.clone();
                    return Box::new(arg_results.into_iter().flat_map(move |arg_result| {
                        match arg_result {
                            Ok(arg_val) => func(&mut ctx_for_builtin.borrow_mut(), input.clone(), &[arg_val]),
                            Err(e) => Box::new(std::iter::once(Err(e))) as EvalResult,
                        }
                    }));
                }
            } else {
                // Multiple arguments - compute cartesian product
                // Collect all values for each argument
                let mut arg_value_lists: Vec<Vec<Result<Jv, String>>> = Vec::new();
                for arg in args {
                    let mut arg_interp = Interpreter { ctx: ctx.clone() };
                    let arg_results: Vec<_> = arg_interp.eval_expr(arg, input.clone(), ctx.clone()).collect();
                    arg_value_lists.push(arg_results);
                }

                // Generate cartesian product of all arguments
                fn cartesian_product(lists: &[Vec<Result<Jv, String>>]) -> Vec<Vec<Result<Jv, String>>> {
                    if lists.is_empty() {
                        return vec![vec![]];
                    }
                    let first = &lists[0];
                    let rest = cartesian_product(&lists[1..]);
                    let mut result = Vec::new();
                    for item in first {
                        for r in &rest {
                            let mut combo = vec![item.clone()];
                            combo.extend(r.iter().cloned());
                            result.push(combo);
                        }
                    }
                    result
                }

                let combinations = cartesian_product(&arg_value_lists);
                let builtin = ctx.borrow().get_builtin(name, arity).copied();

                if let Some(func) = builtin {
                    let ctx_for_builtin = ctx.clone();
                    let mut all_results: Vec<Result<Jv, String>> = Vec::new();
                    for combo in combinations {
                        // Check for errors in this combination
                        let mut arg_vals = Vec::new();
                        let mut has_error = false;
                        for item in combo {
                            match item {
                                Ok(v) => arg_vals.push(v),
                                Err(e) => {
                                    all_results.push(Err(e));
                                    has_error = true;
                                    break;
                                }
                            }
                        }
                        if !has_error {
                            for result in func(&mut ctx_for_builtin.borrow_mut(), input.clone(), &arg_vals) {
                                all_results.push(result);
                            }
                        }
                    }
                    return Box::new(all_results.into_iter());
                }
            }
        }

        // Also check 0-arity builtin if called without args
        let has_zero_arity = arity == 0 && ctx.borrow().has_builtin(name, 0);
        if has_zero_arity {
            let builtin = ctx.borrow().get_builtin(name, 0).copied();
            if let Some(func) = builtin {
                return func(&mut ctx.borrow_mut(), input, &[]);
            }
        }

        // Check for expression binding (filter parameter)
        if arity == 0 {
            let maybe_expr_ctx = ctx.borrow().lookup_expr_with_context(name);
            if let Some((expr, eval_ctx)) = maybe_expr_ctx {
                // Evaluate the bound expression in its original context
                return self.eval_expr(&expr, input, eval_ctx);
            }
        }

        Box::new(std::iter::once(Err(format!("unknown function: {}/{}", name, arity))))
    }

    fn call_user_function(&mut self, func: &FuncDef, args: &[Expr], input: Jv, call_ctx: Rc<RefCell<Context>>, closure_ctx: Rc<RefCell<Context>>) -> EvalResult {
        // Separate parameters into value params ($x) and filter params (x)
        let mut value_param_indices = Vec::new();
        let mut filter_param_indices = Vec::new();

        for (i, param) in func.params.iter().enumerate() {
            if param.is_binding {
                value_param_indices.push(i);
            } else {
                filter_param_indices.push(i);
            }
        }

        // Evaluate all value parameters and collect all their values
        // Arguments are evaluated in the call-site context
        let mut value_param_values: Vec<Vec<Jv>> = Vec::new();
        for &idx in &value_param_indices {
            let arg = &args[idx];
            let mut arg_interp = Interpreter { ctx: call_ctx.clone() };
            let values: Vec<_> = arg_interp.eval_expr(arg, input.clone(), call_ctx.clone())
                .filter_map(|r| r.ok())
                .collect();
            if values.is_empty() {
                return Box::new(std::iter::empty());
            }
            value_param_values.push(values);
        }

        // Compute cartesian product of value parameters
        fn cartesian_product(lists: &[Vec<Jv>]) -> Vec<Vec<Jv>> {
            if lists.is_empty() {
                return vec![vec![]];
            }
            let first = &lists[0];
            let rest = cartesian_product(&lists[1..]);
            let mut result = Vec::new();
            for item in first {
                for r in &rest {
                    let mut combo = vec![item.clone()];
                    combo.extend(r.iter().cloned());
                    result.push(combo);
                }
            }
            result
        }

        let value_combinations = cartesian_product(&value_param_values);

        // For each combination, create a context and evaluate the body
        let func_body = func.body.clone();
        let func_params = func.params.clone();
        let args_clone: Vec<_> = args.iter().cloned().collect();
        let call_ctx_clone = call_ctx.clone();
        let closure_ctx_clone = closure_ctx.clone();
        let input_clone = input.clone();
        let filter_params = filter_param_indices.clone();
        let value_params = value_param_indices.clone();

        Box::new(value_combinations.into_iter().flat_map(move |combo| {
            // Create child context with closure_ctx as parent (for lexical scoping)
            let child_ctx = Rc::new(RefCell::new(Context::child(closure_ctx_clone.clone())));

            // Bind value parameters from this combination
            for (combo_idx, &param_idx) in value_params.iter().enumerate() {
                let param = &func_params[param_idx];
                child_ctx.borrow_mut().bind_value(&param.name, combo[combo_idx].clone());
            }

            // Bind filter parameters as expressions with call-site context
            // (so they can see bindings from the call site)
            for &param_idx in &filter_params {
                let param = &func_params[param_idx];
                child_ctx.borrow_mut().bind_expr_with_context(
                    &param.name,
                    Rc::new(args_clone[param_idx].clone()),
                    call_ctx_clone.clone(),
                );
            }

            let mut inner = Interpreter { ctx: child_ctx.clone() };
            let results: Vec<_> = inner.eval_expr(&func_body, input_clone.clone(), child_ctx).collect();
            results.into_iter()
        }))
    }

    // Higher-order function implementations

    fn eval_map(&mut self, filter: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        match &input {
            Jv::Array(arr) => {
                let filter_expr = filter.clone();
                let ctx_clone = ctx.clone();
                let items: Vec<Jv> = arr.iter().collect();

                let mut results = Vec::new();
                for item in items {
                    let mut inner = Interpreter { ctx: ctx_clone.clone() };
                    for result in inner.eval_expr(&filter_expr, item, ctx_clone.clone()) {
                        match result {
                            Ok(v) => results.push(v),
                            Err(e) => return Box::new(std::iter::once(Err(e))),
                        }
                    }
                }
                Box::new(std::iter::once(Ok(Jv::from_vec(results))))
            }
            Jv::Null => Box::new(std::iter::once(Ok(Jv::Null))),
            _ => Box::new(std::iter::once(Err(format!("Cannot iterate over {} ({})", input.type_name(), input)))),
        }
    }

    fn eval_select(&mut self, predicate: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        let pred_expr = predicate.clone();
        let ctx_clone = ctx.clone();

        let mut inner = Interpreter { ctx: ctx_clone.clone() };
        match inner.eval_expr(&pred_expr, input.clone(), ctx_clone).next() {
            Some(Ok(v)) if v.is_truthy() => Box::new(std::iter::once(Ok(input))),
            Some(Err(e)) => Box::new(std::iter::once(Err(e))),
            _ => Box::new(std::iter::empty()),
        }
    }

    fn recurse(&mut self, input: Jv) -> EvalResult {
        const MAX_RECURSE: usize = 100000;
        let mut results = Vec::new();

        fn recurse_impl(value: Jv, results: &mut Vec<Jv>, max: usize) -> Result<(), String> {
            if results.len() > max {
                return Err("recurse: too many results".to_string());
            }
            results.push(value.clone());
            match &value {
                Jv::Array(arr) => {
                    for item in arr.iter() {
                        recurse_impl(item, results, max)?;
                    }
                }
                Jv::Object(obj) => {
                    for v in obj.values() {
                        recurse_impl(v, results, max)?;
                    }
                }
                _ => {}
            }
            Ok(())
        }

        match recurse_impl(input, &mut results, MAX_RECURSE) {
            Ok(()) => Box::new(results.into_iter().map(Ok)),
            Err(e) => Box::new(std::iter::once(Err(e))),
        }
    }

    fn eval_recurse_with(&mut self, filter: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        const MAX_RECURSE: usize = 100000;
        let filter_expr = filter.clone();
        let ctx_clone = ctx.clone();

        let mut results = vec![input.clone()];
        let mut queue = vec![input];

        while let Some(current) = queue.pop() {
            if results.len() > MAX_RECURSE {
                return Box::new(std::iter::once(Err("recurse: too many results".to_string())));
            }
            let mut inner = Interpreter { ctx: ctx_clone.clone() };
            for result in inner.eval_expr(&filter_expr, current, ctx_clone.clone()) {
                match result {
                    Ok(v) => {
                        results.push(v.clone());
                        queue.push(v);
                    }
                    Err(_) => {} // Stop recursion on error
                }
            }
        }

        Box::new(results.into_iter().map(Ok))
    }

    fn eval_recurse_with_cond(&mut self, filter: &Expr, cond: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        // recurse(f; cond) - apply f while condition is true
        // def recurse(f; cond): def r: ., (f | select(cond) | r); r;
        const MAX_RECURSE: usize = 100000;
        let filter_expr = filter.clone();
        let cond_expr = cond.clone();
        let ctx_clone = ctx.clone();

        let mut results = vec![input.clone()];
        let mut queue = vec![input];

        while let Some(current) = queue.pop() {
            if results.len() > MAX_RECURSE {
                return Box::new(std::iter::once(Err("recurse: too many results".to_string())));
            }
            let mut inner = Interpreter { ctx: ctx_clone.clone() };
            for result in inner.eval_expr(&filter_expr, current, ctx_clone.clone()) {
                match result {
                    Ok(v) => {
                        // Check if condition is satisfied
                        let mut cond_interp = Interpreter { ctx: ctx_clone.clone() };
                        match cond_interp.eval_expr(&cond_expr, v.clone(), ctx_clone.clone()).next() {
                            Some(Ok(cond_val)) if cond_val.is_truthy() => {
                                // Condition true - include and continue recursion
                                results.push(v.clone());
                                queue.push(v);
                            }
                            _ => {
                                // Condition false or error - stop recursion on this branch
                            }
                        }
                    }
                    Err(_) => {} // Stop recursion on error
                }
            }
        }

        Box::new(results.into_iter().map(Ok))
    }

    fn eval_range1(&mut self, end_expr: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        let ctx_clone = ctx.clone();
        let mut inner = Interpreter { ctx: ctx_clone.clone() };

        // Iterate over all end values (supports generators like range(3,5))
        let end_results: Vec<_> = inner.eval_expr(end_expr, input, ctx_clone).collect();

        Box::new(end_results.into_iter().flat_map(|end_result| {
            match end_result {
                Ok(Jv::Number(n)) => {
                    if let Some(end) = n.as_i64() {
                        let values: Vec<Jv> = (0..end).map(Jv::from_i64).collect();
                        Box::new(values.into_iter().map(Ok)) as EvalResult
                    } else {
                        Box::new(std::iter::once(Err("range requires integer".to_string())))
                    }
                }
                Err(e) => Box::new(std::iter::once(Err(e))),
                _ => Box::new(std::iter::once(Err("range requires number".to_string()))),
            }
        }))
    }

    fn eval_range2(&mut self, start_expr: &Expr, end_expr: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        let ctx_clone = ctx.clone();
        let input_clone = input.clone();
        let end_expr = end_expr.clone();

        // Collect all start values
        let mut inner1 = Interpreter { ctx: ctx_clone.clone() };
        let start_results: Vec<_> = inner1.eval_expr(start_expr, input, ctx_clone.clone()).collect();

        let ctx_for_end = ctx_clone;
        Box::new(start_results.into_iter().flat_map(move |start_result| {
            let end_expr = end_expr.clone();
            let input_clone = input_clone.clone();
            let ctx_for_end = ctx_for_end.clone();

            match start_result {
                Ok(Jv::Number(start_num)) => {
                    if let Some(s) = start_num.as_i64() {
                        let mut inner2 = Interpreter { ctx: ctx_for_end.clone() };
                        let end_results: Vec<_> = inner2.eval_expr(&end_expr, input_clone, ctx_for_end).collect();

                        Box::new(end_results.into_iter().flat_map(move |end_result| {
                            match end_result {
                                Ok(Jv::Number(end_num)) => {
                                    if let Some(e) = end_num.as_i64() {
                                        let values: Vec<Jv> = (s..e).map(Jv::from_i64).collect();
                                        Box::new(values.into_iter().map(Ok)) as EvalResult
                                    } else {
                                        Box::new(std::iter::once(Err("range requires integers".to_string())))
                                    }
                                }
                                Err(e) => Box::new(std::iter::once(Err(e))),
                                _ => Box::new(std::iter::once(Err("range requires integers".to_string()))),
                            }
                        })) as EvalResult
                    } else {
                        Box::new(std::iter::once(Err("range requires integers".to_string())))
                    }
                }
                Err(e) => Box::new(std::iter::once(Err(e))),
                _ => Box::new(std::iter::once(Err("range requires integers".to_string()))),
            }
        }))
    }

    fn eval_limit(&mut self, n_expr: &Expr, iter_expr: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        let ctx_clone = ctx.clone();
        let iter_expr = iter_expr.clone();
        let input_clone = input.clone();

        // Iterate over all n values (supports generators like limit(5,7; expr))
        let mut n_inner = Interpreter { ctx: ctx_clone.clone() };
        let n_results: Vec<_> = n_inner.eval_expr(n_expr, input, ctx_clone.clone()).collect();

        let ctx_for_iter = ctx_clone;
        Box::new(n_results.into_iter().flat_map(move |n_result| {
            match n_result {
                Ok(Jv::Number(num)) => {
                    match num.as_i64() {
                        Some(i) if i < 0 => Box::new(std::iter::once(Err("limit doesn't support negative count".to_string()))) as EvalResult,
                        Some(i) => {
                            let n = i as usize;
                            let mut iter_inner = Interpreter { ctx: ctx_for_iter.clone() };
                            let results: Vec<_> = iter_inner.eval_expr(&iter_expr, input_clone.clone(), ctx_for_iter.clone()).take(n).collect();
                            Box::new(results.into_iter())
                        }
                        None => Box::new(std::iter::once(Err("limit requires integer".to_string()))),
                    }
                }
                Err(e) => Box::new(std::iter::once(Err(e))),
                _ => Box::new(std::iter::once(Err("limit requires number".to_string()))),
            }
        }))
    }

    fn eval_skip(&mut self, n_expr: &Expr, iter_expr: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        let ctx_clone = ctx.clone();
        let iter_expr = iter_expr.clone();
        let input_clone = input.clone();

        // Iterate over all n values (supports generators like skip(0,2,3; expr))
        let mut n_inner = Interpreter { ctx: ctx_clone.clone() };
        let n_results: Vec<_> = n_inner.eval_expr(n_expr, input, ctx_clone.clone()).collect();

        let ctx_for_iter = ctx_clone;
        Box::new(n_results.into_iter().flat_map(move |n_result| {
            match n_result {
                Ok(Jv::Number(num)) => {
                    match num.as_i64() {
                        Some(i) if i < 0 => Box::new(std::iter::once(Err("skip doesn't support negative count".to_string()))) as EvalResult,
                        Some(i) => {
                            let n = i as usize;
                            let mut iter_inner = Interpreter { ctx: ctx_for_iter.clone() };
                            let results: Vec<_> = iter_inner.eval_expr(&iter_expr, input_clone.clone(), ctx_for_iter.clone()).skip(n).collect();
                            Box::new(results.into_iter()) as EvalResult
                        }
                        None => Box::new(std::iter::once(Err("skip requires integer".to_string()))) as EvalResult,
                    }
                }
                Err(e) => Box::new(std::iter::once(Err(e))),
                _ => Box::new(std::iter::once(Err("skip requires number".to_string()))),
            }
        }))
    }

    fn eval_first_expr(&mut self, iter_expr: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        let ctx_clone = ctx.clone();
        let mut inner = Interpreter { ctx: ctx_clone.clone() };

        match inner.eval_expr(iter_expr, input, ctx_clone).next() {
            Some(result) => Box::new(std::iter::once(result)),
            None => Box::new(std::iter::empty()),
        }
    }

    fn eval_last_expr(&mut self, iter_expr: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        let ctx_clone = ctx.clone();
        let mut inner = Interpreter { ctx: ctx_clone.clone() };

        let mut last_result = None;
        for result in inner.eval_expr(iter_expr, input, ctx_clone) {
            last_result = Some(result);
        }

        match last_result {
            Some(result) => Box::new(std::iter::once(result)),
            None => Box::new(std::iter::empty()),
        }
    }

    fn eval_nth(&mut self, n_expr: &Expr, iter_expr: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        let ctx_clone = ctx.clone();
        let iter_expr = iter_expr.clone();
        let input_clone = input.clone();

        // Iterate over all n values (supports generators like nth(0,5,9; expr))
        let mut n_inner = Interpreter { ctx: ctx_clone.clone() };
        let n_results: Vec<_> = n_inner.eval_expr(n_expr, input, ctx_clone.clone()).collect();

        let ctx_for_iter = ctx_clone;
        Box::new(n_results.into_iter().flat_map(move |n_result| {
            match n_result {
                Ok(Jv::Number(num)) => {
                    match num.as_i64() {
                        Some(i) if i < 0 => Box::new(std::iter::once(Err("nth doesn't support negative indices".to_string()))) as EvalResult,
                        Some(i) => {
                            let n = i as usize;
                            let mut iter_inner = Interpreter { ctx: ctx_for_iter.clone() };
                            match iter_inner.eval_expr(&iter_expr, input_clone.clone(), ctx_for_iter.clone()).nth(n) {
                                Some(result) => Box::new(std::iter::once(result)) as EvalResult,
                                None => Box::new(std::iter::empty()) as EvalResult,
                            }
                        }
                        None => Box::new(std::iter::once(Err("nth requires integer".to_string()))) as EvalResult,
                    }
                }
                Err(e) => Box::new(std::iter::once(Err(e))) as EvalResult,
                _ => Box::new(std::iter::once(Err("nth requires integer".to_string()))) as EvalResult,
            }
        }))
    }

    fn eval_reduce(&mut self, iter_expr: &Expr, pattern: &Pattern, init_expr: &Expr, update_expr: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        let ctx_clone = ctx.clone();

        // Evaluate initial value
        let mut init_inner = Interpreter { ctx: ctx_clone.clone() };
        let mut acc = match init_inner.eval_expr(init_expr, input.clone(), ctx_clone.clone()).next() {
            Some(Ok(v)) => v,
            Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
            None => return Box::new(std::iter::empty()),
        };

        // Iterate over values
        let mut iter_inner = Interpreter { ctx: ctx_clone.clone() };
        for result in iter_inner.eval_expr(iter_expr, input.clone(), ctx_clone.clone()) {
            match result {
                Ok(item) => {
                    // Create context with binding
                    let child_ctx = Rc::new(RefCell::new(Context::child(ctx_clone.clone())));
                    let mut bind_inner = Interpreter { ctx: child_ctx.clone() };
                    if let Err(e) = bind_inner.bind_pattern(pattern, &item, &child_ctx) {
                        return Box::new(std::iter::once(Err(e)));
                    }

                    // Evaluate update with acc as input
                    let mut update_inner = Interpreter { ctx: child_ctx.clone() };
                    match update_inner.eval_expr(update_expr, acc.clone(), child_ctx).next() {
                        Some(Ok(v)) => acc = v,
                        Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
                        None => {}
                    }
                }
                Err(e) => return Box::new(std::iter::once(Err(e))),
            }
        }

        Box::new(std::iter::once(Ok(acc)))
    }

    fn eval_foreach(&mut self, iter_expr: &Expr, pattern: &Pattern, init_expr: &Expr, update_expr: &Expr, extract_expr: Option<&Box<Expr>>, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        let ctx_clone = ctx.clone();

        // Evaluate initial values - foreach iterates over ALL init values
        let mut init_inner = Interpreter { ctx: ctx_clone.clone() };
        let init_values: Vec<Jv> = init_inner.eval_expr(init_expr, input.clone(), ctx_clone.clone())
            .filter_map(|r| r.ok())
            .collect();

        if init_values.is_empty() {
            return Box::new(std::iter::empty());
        }

        let mut all_results = Vec::new();

        // For each initial state
        for init_state in init_values {
            let mut state = init_state;
            let mut results = Vec::new();

            // Iterate over values
            let mut iter_inner = Interpreter { ctx: ctx_clone.clone() };
            let iter_values: Vec<Result<Jv, String>> = iter_inner.eval_expr(iter_expr, input.clone(), ctx_clone.clone()).collect();

            for result in iter_values {
                match result {
                    Ok(item) => {
                        // Create context with binding
                        let child_ctx = Rc::new(RefCell::new(Context::child(ctx_clone.clone())));
                        let mut bind_inner = Interpreter { ctx: child_ctx.clone() };
                        if let Err(e) = bind_inner.bind_pattern(pattern, &item, &child_ctx) {
                            return Box::new(std::iter::once(Err(e)));
                        }

                        // Evaluate update with state as input
                        let mut update_inner = Interpreter { ctx: child_ctx.clone() };
                        match update_inner.eval_expr(update_expr, state.clone(), child_ctx.clone()).next() {
                            Some(Ok(v)) => state = v,
                            Some(Err(e)) => {
                                // Check if it's a break signal - if so, propagate it
                                // The label handler will catch it
                                if e.starts_with(BREAK_PREFIX) {
                                    results.push(Err(e));
                                    all_results.extend(results);
                                    return Box::new(all_results.into_iter());
                                }
                                return Box::new(std::iter::once(Err(e)));
                            }
                            None => {}
                        }

                        // Extract output if provided
                        if let Some(ext_expr) = extract_expr {
                            let mut ext_inner = Interpreter { ctx: child_ctx.clone() };
                            for ext_result in ext_inner.eval_expr(ext_expr, state.clone(), child_ctx) {
                                match ext_result {
                                    Ok(v) => results.push(Ok(v)),
                                    Err(e) => {
                                        if e.starts_with(BREAK_PREFIX) {
                                            results.push(Err(e));
                                            all_results.extend(results);
                                            return Box::new(all_results.into_iter());
                                        }
                                        results.push(Err(e));
                                    }
                                }
                            }
                        } else {
                            results.push(Ok(state.clone()));
                        }
                    }
                    Err(e) => return Box::new(std::iter::once(Err(e))),
                }
            }

            all_results.extend(results);
        }

        Box::new(all_results.into_iter())
    }

    fn eval_group_by(&mut self, key_expr: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        match &input {
            Jv::Array(arr) => {
                // Collect items with their keys
                let mut items_with_keys: Vec<(Vec<Jv>, Jv)> = Vec::new();

                for item in arr.iter() {
                    let mut inner = Interpreter { ctx: ctx.clone() };
                    // Collect all key values for tuple comparison
                    let keys: Vec<Jv> = inner.eval_expr(key_expr, item.clone(), ctx.clone())
                        .filter_map(|r| r.ok())
                        .collect();

                    if keys.is_empty() {
                        items_with_keys.push((vec![Jv::Null], item));
                    } else {
                        items_with_keys.push((keys, item));
                    }
                }

                // Sort first by keys (this is how jq group_by works)
                items_with_keys.sort_by(|a, b| {
                    for (k1, k2) in a.0.iter().zip(b.0.iter()) {
                        match k1.cmp(k2) {
                            std::cmp::Ordering::Equal => continue,
                            other => return other,
                        }
                    }
                    a.0.len().cmp(&b.0.len())
                });

                // Group consecutive items with equal keys
                let mut groups: Vec<Vec<Jv>> = Vec::new();
                let mut current_group: Vec<Jv> = Vec::new();
                let mut current_keys: Option<Vec<Jv>> = None;

                for (keys, item) in items_with_keys {
                    let same = match &current_keys {
                        None => false,
                        Some(k) => k == &keys,
                    };
                    if same {
                        current_group.push(item);
                    } else {
                        if !current_group.is_empty() {
                            groups.push(std::mem::take(&mut current_group));
                        }
                        current_group.push(item);
                        current_keys = Some(keys);
                    }
                }
                if !current_group.is_empty() {
                    groups.push(current_group);
                }

                let result: Vec<Jv> = groups.into_iter()
                    .map(Jv::from_vec)
                    .collect();
                Box::new(std::iter::once(Ok(Jv::from_vec(result))))
            }
            _ => Box::new(std::iter::once(Err("group_by requires array".to_string()))),
        }
    }

    fn eval_sort_by(&mut self, key_expr: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        match &input {
            Jv::Array(arr) => {
                let mut items_with_keys: Vec<(Vec<Jv>, Jv)> = Vec::new();

                for item in arr.iter() {
                    let mut inner = Interpreter { ctx: ctx.clone() };
                    // Collect all key values for tuple comparison
                    let keys: Vec<Jv> = inner.eval_expr(key_expr, item.clone(), ctx.clone())
                        .filter_map(|r| r.ok())
                        .collect();

                    if keys.is_empty() {
                        items_with_keys.push((vec![Jv::Null], item));
                    } else {
                        items_with_keys.push((keys, item));
                    }
                }

                // Sort by comparing key vectors lexicographically
                items_with_keys.sort_by(|a, b| {
                    for (k1, k2) in a.0.iter().zip(b.0.iter()) {
                        match k1.cmp(k2) {
                            std::cmp::Ordering::Equal => continue,
                            other => return other,
                        }
                    }
                    // If all compared keys are equal, shorter key vector comes first
                    a.0.len().cmp(&b.0.len())
                });
                let result: Vec<Jv> = items_with_keys.into_iter().map(|(_, v)| v).collect();
                Box::new(std::iter::once(Ok(Jv::from_vec(result))))
            }
            _ => Box::new(std::iter::once(Err("sort_by requires array".to_string()))),
        }
    }

    fn eval_unique_by(&mut self, key_expr: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        match &input {
            Jv::Array(arr) => {
                // Collect (key, item) pairs
                let mut pairs: Vec<(Jv, Jv)> = Vec::new();
                for item in arr.iter() {
                    let mut inner = Interpreter { ctx: ctx.clone() };
                    match inner.eval_expr(key_expr, item.clone(), ctx.clone()).next() {
                        Some(Ok(key)) => {
                            pairs.push((key, item));
                        }
                        Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
                        None => {}
                    }
                }

                // Sort by key
                pairs.sort_by(|(k1, _), (k2, _)| k1.partial_cmp(k2).unwrap_or(std::cmp::Ordering::Equal));

                // Keep first occurrence of each unique key
                use std::collections::HashSet;
                let mut seen: HashSet<String> = HashSet::new();
                let mut result = Vec::new();

                for (key, item) in pairs {
                    let key_str = format!("{}", key);
                    if seen.insert(key_str) {
                        result.push(item);
                    }
                }

                Box::new(std::iter::once(Ok(Jv::from_vec(result))))
            }
            _ => Box::new(std::iter::once(Err("unique_by requires array".to_string()))),
        }
    }

    fn eval_max_by(&mut self, key_expr: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        match &input {
            Jv::Array(arr) if arr.len() > 0 => {
                let mut max_item: Option<(Vec<Jv>, Jv)> = None;

                for item in arr.iter() {
                    let mut inner = Interpreter { ctx: ctx.clone() };
                    // Collect all key values for tuple comparison
                    let keys: Vec<Jv> = inner.eval_expr(key_expr, item.clone(), ctx.clone())
                        .filter_map(|r| r.ok())
                        .collect();

                    if keys.is_empty() {
                        continue;
                    }

                    if let Some((ref max_keys, _)) = max_item {
                        // Compare key vectors lexicographically
                        // Use >= so that equal keys update to the last item
                        let mut is_greater_or_equal = true;
                        for (k1, k2) in keys.iter().zip(max_keys.iter()) {
                            match k1.cmp(k2) {
                                std::cmp::Ordering::Greater => break, // is_greater_or_equal stays true
                                std::cmp::Ordering::Less => {
                                    is_greater_or_equal = false;
                                    break;
                                }
                                std::cmp::Ordering::Equal => continue,
                            }
                        }
                        if is_greater_or_equal {
                            max_item = Some((keys, item));
                        }
                    } else {
                        max_item = Some((keys, item));
                    }
                }

                match max_item {
                    Some((_, v)) => Box::new(std::iter::once(Ok(v))),
                    None => Box::new(std::iter::once(Ok(Jv::Null))),
                }
            }
            Jv::Array(_) => Box::new(std::iter::once(Ok(Jv::Null))),
            _ => Box::new(std::iter::once(Err("max_by requires array".to_string()))),
        }
    }

    fn eval_min_by(&mut self, key_expr: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        match &input {
            Jv::Array(arr) if arr.len() > 0 => {
                let mut min_item: Option<(Vec<Jv>, Jv)> = None;

                for item in arr.iter() {
                    let mut inner = Interpreter { ctx: ctx.clone() };
                    // Collect all key values for tuple comparison
                    let keys: Vec<Jv> = inner.eval_expr(key_expr, item.clone(), ctx.clone())
                        .filter_map(|r| r.ok())
                        .collect();

                    if keys.is_empty() {
                        continue;
                    }

                    if let Some((ref min_keys, _)) = min_item {
                        // Compare key vectors lexicographically
                        let mut is_less = false;
                        for (k1, k2) in keys.iter().zip(min_keys.iter()) {
                            match k1.cmp(k2) {
                                std::cmp::Ordering::Less => {
                                    is_less = true;
                                    break;
                                }
                                std::cmp::Ordering::Greater => break,
                                std::cmp::Ordering::Equal => continue,
                            }
                        }
                        if is_less {
                            min_item = Some((keys, item));
                        }
                    } else {
                        min_item = Some((keys, item));
                    }
                }

                match min_item {
                    Some((_, v)) => Box::new(std::iter::once(Ok(v))),
                    None => Box::new(std::iter::once(Ok(Jv::Null))),
                }
            }
            Jv::Array(_) => Box::new(std::iter::once(Ok(Jv::Null))),
            _ => Box::new(std::iter::once(Err("min_by requires array".to_string()))),
        }
    }

    fn eval_any_simple(&mut self, input: Jv) -> EvalResult {
        match &input {
            Jv::Array(arr) => {
                for item in arr.iter() {
                    if item.is_truthy() {
                        return Box::new(std::iter::once(Ok(Jv::Bool(true))));
                    }
                }
                Box::new(std::iter::once(Ok(Jv::Bool(false))))
            }
            _ => Box::new(std::iter::once(Err("any requires array".to_string()))),
        }
    }

    fn eval_all_simple(&mut self, input: Jv) -> EvalResult {
        match &input {
            Jv::Array(arr) => {
                for item in arr.iter() {
                    if !item.is_truthy() {
                        return Box::new(std::iter::once(Ok(Jv::Bool(false))));
                    }
                }
                Box::new(std::iter::once(Ok(Jv::Bool(true))))
            }
            _ => Box::new(std::iter::once(Err("all requires array".to_string()))),
        }
    }

    fn eval_any_filter(&mut self, filter: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        // any(f) = map(f) | any
        match &input {
            Jv::Array(arr) => {
                for item in arr.iter() {
                    let mut inner = Interpreter { ctx: ctx.clone() };
                    match inner.eval_expr(filter, item, ctx.clone()).next() {
                        Some(Ok(v)) if v.is_truthy() => {
                            return Box::new(std::iter::once(Ok(Jv::Bool(true))));
                        }
                        Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
                        _ => {}
                    }
                }
                Box::new(std::iter::once(Ok(Jv::Bool(false))))
            }
            _ => Box::new(std::iter::once(Err("any requires array".to_string()))),
        }
    }

    fn eval_all_filter(&mut self, filter: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        // all(f) = map(f) | all
        match &input {
            Jv::Array(arr) => {
                for item in arr.iter() {
                    let mut inner = Interpreter { ctx: ctx.clone() };
                    match inner.eval_expr(filter, item, ctx.clone()).next() {
                        Some(Ok(v)) if !v.is_truthy() => {
                            return Box::new(std::iter::once(Ok(Jv::Bool(false))));
                        }
                        Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
                        _ => {}
                    }
                }
                Box::new(std::iter::once(Ok(Jv::Bool(true))))
            }
            _ => Box::new(std::iter::once(Err("all requires array".to_string()))),
        }
    }

    fn eval_any_gen_filter(&mut self, gen: &Expr, filter: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        // any(gen; filter) = first(gen | select(filter)) | true
        let mut inner_gen = Interpreter { ctx: ctx.clone() };
        for result in inner_gen.eval_expr(gen, input.clone(), ctx.clone()) {
            match result {
                Ok(item) => {
                    let mut inner_filter = Interpreter { ctx: ctx.clone() };
                    match inner_filter.eval_expr(filter, item, ctx.clone()).next() {
                        Some(Ok(v)) if v.is_truthy() => {
                            return Box::new(std::iter::once(Ok(Jv::Bool(true))));
                        }
                        Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
                        _ => {}
                    }
                }
                Err(e) => return Box::new(std::iter::once(Err(e))),
            }
        }
        Box::new(std::iter::once(Ok(Jv::Bool(false))))
    }

    fn eval_all_gen_filter(&mut self, gen: &Expr, filter: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        // all(gen; filter) = first(gen | if filter then empty else . end) | false | not
        let mut inner_gen = Interpreter { ctx: ctx.clone() };
        for result in inner_gen.eval_expr(gen, input.clone(), ctx.clone()) {
            match result {
                Ok(item) => {
                    let mut inner_filter = Interpreter { ctx: ctx.clone() };
                    match inner_filter.eval_expr(filter, item, ctx.clone()).next() {
                        Some(Ok(v)) if !v.is_truthy() => {
                            return Box::new(std::iter::once(Ok(Jv::Bool(false))));
                        }
                        Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
                        _ => {}
                    }
                }
                Err(e) => return Box::new(std::iter::once(Err(e))),
            }
        }
        Box::new(std::iter::once(Ok(Jv::Bool(true))))
    }

    fn eval_in_stream(&mut self, stream: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        // IN(s) = any(s == .; .)
        // Check if input equals any value produced by the stream
        let mut inner = Interpreter { ctx: ctx.clone() };
        for result in inner.eval_expr(stream, Jv::Null, ctx.clone()) {
            match result {
                Ok(item) => {
                    if item == input {
                        return Box::new(std::iter::once(Ok(Jv::Bool(true))));
                    }
                }
                Err(e) => return Box::new(std::iter::once(Err(e))),
            }
        }
        Box::new(std::iter::once(Ok(Jv::Bool(false))))
    }

    fn eval_in_stream2(&mut self, src: &Expr, stream: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        // IN(src; s) = any(src == s; .)
        // Check if any value from src equals any value from stream
        let mut inner_src = Interpreter { ctx: ctx.clone() };
        let src_values: Vec<_> = inner_src.eval_expr(src, input.clone(), ctx.clone())
            .filter_map(|r| r.ok())
            .collect();

        let mut inner_s = Interpreter { ctx: ctx.clone() };
        let stream_values: Vec<_> = inner_s.eval_expr(stream, Jv::Null, ctx.clone())
            .filter_map(|r| r.ok())
            .collect();

        for s_val in src_values {
            if stream_values.iter().any(|v| *v == s_val) {
                return Box::new(std::iter::once(Ok(Jv::Bool(true))));
            }
        }
        Box::new(std::iter::once(Ok(Jv::Bool(false))))
    }

    // INDEX(stream; idx_expr) - creates object mapping idx_expr to stream elements
    // def INDEX(stream; idx_expr): reduce stream as $row ({}; .[$row|idx_expr|tostring] = $row)
    fn eval_index2(&mut self, stream: &Expr, idx_expr: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        use crate::jv::JvObject;

        let mut result = JvObject::new();
        let mut inner = Interpreter { ctx: ctx.clone() };

        for row_result in inner.eval_expr(stream, input.clone(), ctx.clone()) {
            match row_result {
                Ok(row) => {
                    // Evaluate idx_expr on row
                    let mut idx_interp = Interpreter { ctx: ctx.clone() };
                    if let Some(idx_result) = idx_interp.eval_expr(idx_expr, row.clone(), ctx.clone()).next() {
                        match idx_result {
                            Ok(idx_val) => {
                                // Convert to string for object key
                                let key = match &idx_val {
                                    Jv::String(s) => s.as_str().to_string(),
                                    Jv::Number(n) => format!("{}", n),
                                    _ => format!("{}", idx_val),
                                };
                                result.set(&key, row);
                            }
                            Err(e) => return Box::new(std::iter::once(Err(e))),
                        }
                    }
                }
                Err(e) => return Box::new(std::iter::once(Err(e))),
            }
        }

        Box::new(std::iter::once(Ok(Jv::Object(result))))
    }

    // INDEX(idx_expr) - same as INDEX(.[]; idx_expr)
    fn eval_index1(&mut self, idx_expr: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        use crate::jv::JvObject;

        // input should be an array, iterate over it
        let arr = match &input {
            Jv::Array(a) => a,
            _ => return Box::new(std::iter::once(Err("INDEX requires array input".to_string()))),
        };

        let mut result = JvObject::new();

        for row in arr.iter() {
            // Evaluate idx_expr on row
            let mut idx_interp = Interpreter { ctx: ctx.clone() };
            if let Some(idx_result) = idx_interp.eval_expr(idx_expr, row.clone(), ctx.clone()).next() {
                match idx_result {
                    Ok(idx_val) => {
                        // Convert to string for object key
                        let key = match &idx_val {
                            Jv::String(s) => s.as_str().to_string(),
                            Jv::Number(n) => format!("{}", n),
                            _ => format!("{}", idx_val),
                        };
                        result.set(&key, row);
                    }
                    Err(e) => return Box::new(std::iter::once(Err(e))),
                }
            }
        }

        Box::new(std::iter::once(Ok(Jv::Object(result))))
    }

    // JOIN($idx; idx_expr) - [.[] | [., $idx[idx_expr]]]
    fn eval_join2(&mut self, idx_expr: &Expr, key_expr: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        // idx_expr is the index object ($idx in jq definition)
        // key_expr is the expression to compute the key

        // First evaluate idx_expr to get the index object
        let mut idx_interp = Interpreter { ctx: ctx.clone() };
        let idx_obj = match idx_interp.eval_expr(idx_expr, input.clone(), ctx.clone()).next() {
            Some(Ok(v)) => v,
            Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
            None => return Box::new(std::iter::once(Err("INDEX object evaluation produced no value".to_string()))),
        };

        // Input should be an array
        let arr = match &input {
            Jv::Array(a) => a,
            _ => return Box::new(std::iter::once(Err("JOIN requires array input".to_string()))),
        };

        let mut results = Vec::new();

        for item in arr.iter() {
            // Compute the key for this item
            let mut key_interp = Interpreter { ctx: ctx.clone() };
            let key_val = match key_interp.eval_expr(key_expr, item.clone(), ctx.clone()).next() {
                Some(Ok(v)) => v,
                Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
                None => continue,
            };

            // Convert key to string
            let key = match &key_val {
                Jv::String(s) => s.as_str().to_string(),
                Jv::Number(n) => format!("{}", n),
                _ => format!("{}", key_val),
            };

            // Look up in the index object
            let lookup = match &idx_obj {
                Jv::Object(obj) => obj.get(&key).unwrap_or(Jv::Null),
                _ => Jv::Null,
            };

            // Create [item, lookup] pair
            results.push(Jv::from_vec(vec![item.clone(), lookup]));
        }

        Box::new(std::iter::once(Ok(Jv::from_vec(results))))
    }

    // JOIN($idx; stream; idx_expr) - stream | [., $idx[idx_expr]]
    fn eval_join3(&mut self, idx_expr: &Expr, stream: &Expr, key_expr: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        // First evaluate idx_expr to get the index object
        let mut idx_interp = Interpreter { ctx: ctx.clone() };
        let idx_obj = match idx_interp.eval_expr(idx_expr, input.clone(), ctx.clone()).next() {
            Some(Ok(v)) => v,
            Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
            None => return Box::new(std::iter::once(Err("INDEX object evaluation produced no value".to_string()))),
        };

        let idx_obj_clone = idx_obj;
        let key_expr = key_expr.clone();
        let ctx_clone = ctx.clone();

        // Evaluate stream and for each item, create [item, $idx[key]]
        let mut stream_interp = Interpreter { ctx: ctx.clone() };

        Box::new(stream_interp.eval_expr(stream, input, ctx).map(move |item_result| {
            match item_result {
                Ok(item) => {
                    // Compute the key for this item
                    let mut key_interp = Interpreter { ctx: ctx_clone.clone() };
                    let key_val = match key_interp.eval_expr(&key_expr, item.clone(), ctx_clone.clone()).next() {
                        Some(Ok(v)) => v,
                        Some(Err(e)) => return Err(e),
                        None => return Ok(Jv::from_vec(vec![item, Jv::Null])),
                    };

                    // Convert key to string
                    let key = match &key_val {
                        Jv::String(s) => s.as_str().to_string(),
                        Jv::Number(n) => format!("{}", n),
                        _ => format!("{}", key_val),
                    };

                    // Look up in the index object
                    let lookup = match &idx_obj_clone {
                        Jv::Object(obj) => obj.get(&key).unwrap_or(Jv::Null),
                        _ => Jv::Null,
                    };

                    Ok(Jv::from_vec(vec![item, lookup]))
                }
                Err(e) => Err(e),
            }
        }))
    }

    fn eval_del(&mut self, path_expr: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        // del(path) deletes the element at path
        let result = Self::apply_deletion(input, path_expr, ctx);
        match result {
            Ok(v) => Box::new(std::iter::once(Ok(v))),
            Err(e) => Box::new(std::iter::once(Err(e))),
        }
    }

    fn apply_deletion(
        current: Jv,
        target: &Expr,
        ctx: Rc<RefCell<Context>>,
    ) -> Result<Jv, String> {
        match &target.kind {
            ExprKind::Identity => {
                // del(.) - return null
                Ok(Jv::Null)
            }
            ExprKind::Field(name) => {
                // del(.foo)
                match current {
                    Jv::Object(mut obj) => {
                        obj.delete(name);
                        Ok(Jv::Object(obj))
                    }
                    _ => Ok(current), // No-op if not object
                }
            }
            ExprKind::Index { expr: base, index, optional: _ } => {
                // Check if index produces multiple values (comma expression)
                let mut idx_interp = Interpreter { ctx: ctx.clone() };
                let idx_results: Vec<_> = idx_interp.eval_expr(index, current.clone(), ctx.clone()).collect();

                if idx_results.is_empty() {
                    return Ok(current);
                }

                match &base.kind {
                    ExprKind::Identity => {
                        // If there are multiple indices, collect them all and delete from original
                        if idx_results.len() > 1 {
                            match &current {
                                Jv::Array(arr) => {
                                    let len = arr.len() as i64;
                                    let mut to_delete: std::collections::HashSet<usize> = std::collections::HashSet::new();

                                    for result in idx_results {
                                        if let Ok(Jv::Number(n)) = result {
                                            if let Some(idx) = n.as_i64() {
                                                let abs_idx = if idx < 0 {
                                                    (len + idx).max(0) as usize
                                                } else {
                                                    idx as usize
                                                };
                                                if abs_idx < arr.len() {
                                                    to_delete.insert(abs_idx);
                                                }
                                            }
                                        }
                                    }

                                    let mut result = Vec::new();
                                    for (i, elem) in arr.iter().enumerate() {
                                        if !to_delete.contains(&i) {
                                            result.push(elem);
                                        }
                                    }
                                    return Ok(Jv::from_vec(result));
                                }
                                Jv::Object(obj) => {
                                    let mut new_obj = obj.clone();
                                    for result in idx_results {
                                        if let Ok(Jv::String(s)) = result {
                                            new_obj.delete(s.as_str());
                                        }
                                    }
                                    return Ok(Jv::Object(new_obj));
                                }
                                _ => return Ok(current),
                            }
                        }

                        // Single index case
                        let idx_val = match idx_results.into_iter().next() {
                            Some(Ok(v)) => v,
                            Some(Err(e)) => return Err(e),
                            None => return Ok(current),
                        };

                        match &idx_val {
                            Jv::String(s) => {
                                match current {
                                    Jv::Object(mut obj) => {
                                        obj.delete(s.as_str());
                                        Ok(Jv::Object(obj))
                                    }
                                    _ => Ok(current),
                                }
                            }
                            Jv::Number(n) => {
                                if let Some(idx) = n.as_i64() {
                                    match current {
                                        Jv::Array(mut arr) => {
                                            arr.delete(idx);
                                            Ok(Jv::Array(arr))
                                        }
                                        _ => Ok(current),
                                    }
                                } else {
                                    Ok(current)
                                }
                            }
                            _ => Ok(current),
                        }
                    }
                    _ => {
                        // Nested deletion
                        let mut base_interp = Interpreter { ctx: ctx.clone() };
                        let base_val = match base_interp.eval_expr(base, current.clone(), ctx.clone()).next() {
                            Some(Ok(v)) => v,
                            Some(Err(e)) => return Err(e),
                            None => return Ok(current),
                        };

                        // If base path doesn't exist (returns null for object/array access),
                        // there's nothing to delete, so return input unchanged
                        if base_val == Jv::Null {
                            // Check if the base expression is accessing a non-existent path
                            // by checking if it's a field/index access that returned null
                            if Self::is_path_access(&base.kind) {
                                return Ok(current);
                            }
                        }

                        let inner_target = Expr::new(
                            ExprKind::Index {
                                expr: Box::new(Expr::new(ExprKind::Identity, target.span)),
                                index: index.clone(),
                                optional: false,
                            },
                            target.span,
                        );
                        let modified_base = Self::apply_deletion(base_val, &inner_target, ctx.clone())?;
                        Self::apply_assignment(current, base, modified_base, &mut Vec::new(), ctx)
                    }
                }
            }
            ExprKind::Slice { expr: base, start, end, optional: _ } => {
                // del(.[start:end])
                let mut interp = Interpreter { ctx: ctx.clone() };

                // Evaluate start and end indices
                let start_val = if let Some(start_expr) = start {
                    match interp.eval_expr(start_expr, current.clone(), ctx.clone()).next() {
                        Some(Ok(Jv::Number(n))) => n.as_i64(),
                        Some(Err(e)) => return Err(e),
                        _ => None,
                    }
                } else {
                    None
                };

                let end_val = if let Some(end_expr) = end {
                    match interp.eval_expr(end_expr, current.clone(), ctx.clone()).next() {
                        Some(Ok(Jv::Number(n))) => n.as_i64(),
                        Some(Err(e)) => return Err(e),
                        _ => None,
                    }
                } else {
                    None
                };

                match &base.kind {
                    ExprKind::Identity => {
                        // Direct slice deletion
                        match current {
                            Jv::Array(arr) => {
                                let len = arr.len();
                                let start_idx = match start_val {
                                    Some(i) if i < 0 => (len as i64 + i).max(0) as usize,
                                    Some(i) => (i as usize).min(len),
                                    None => 0,
                                };
                                let end_idx = match end_val {
                                    Some(i) if i < 0 => (len as i64 + i).max(0) as usize,
                                    Some(i) => (i as usize).min(len),
                                    None => len,
                                };

                                // Build new array without the slice range
                                let mut result = Vec::new();
                                for i in 0..start_idx.min(len) {
                                    result.push(arr.get(i as i64).unwrap_or(Jv::Null));
                                }
                                for i in end_idx..len {
                                    result.push(arr.get(i as i64).unwrap_or(Jv::Null));
                                }
                                Ok(Jv::from_vec(result))
                            }
                            _ => Ok(current),
                        }
                    }
                    _ => {
                        // Nested slice deletion
                        let mut base_interp = Interpreter { ctx: ctx.clone() };
                        let base_val = match base_interp.eval_expr(base, current.clone(), ctx.clone()).next() {
                            Some(Ok(v)) => v,
                            Some(Err(e)) => return Err(e),
                            None => return Ok(current),
                        };

                        let inner_target = Expr {
                            kind: ExprKind::Slice {
                                expr: Box::new(Expr::new(ExprKind::Identity, target.span)),
                                start: start.clone(),
                                end: end.clone(),
                                optional: false,
                            },
                            span: target.span,
                        };
                        let modified_base = Self::apply_deletion(base_val, &inner_target, ctx.clone())?;
                        Self::apply_assignment(current, base, modified_base, &mut Vec::new(), ctx)
                    }
                }
            }
            ExprKind::Comma(left, right) => {
                // del(a, b) - collect all paths and delete them all from the original array
                // jq evaluates all paths on the original input, then deletes them all at once
                // This is different from del(a) | del(b) which applies sequentially

                // For arrays, we need to collect all indices to delete from the original
                // For simplicity, handle the common case of arrays with numeric indices
                #[derive(Clone)]
                enum DeleteTarget {
                    Single(i64),
                    Slice { start: i64, end: Option<i64> }, // end=None means to the end
                }

                fn collect_indices_to_delete(
                    expr: &Expr,
                    input: &Jv,
                    ctx: Rc<RefCell<Context>>,
                    indices: &mut Vec<DeleteTarget>,
                ) {
                    match &expr.kind {
                        ExprKind::Index { expr: base, index, .. } => {
                            if let ExprKind::Identity = base.kind {
                                let mut interp = Interpreter { ctx: ctx.clone() };
                                if let Some(Ok(Jv::Number(n))) = interp.eval_expr(index, input.clone(), ctx.clone()).next() {
                                    if let Some(idx) = n.as_i64() {
                                        indices.push(DeleteTarget::Single(idx));
                                    }
                                }
                            }
                        }
                        ExprKind::Slice { expr: base, start, end, .. } => {
                            if let ExprKind::Identity = base.kind {
                                let mut interp = Interpreter { ctx: ctx.clone() };
                                let start_val = if let Some(start_expr) = start {
                                    match interp.eval_expr(start_expr, input.clone(), ctx.clone()).next() {
                                        Some(Ok(Jv::Number(n))) => n.as_i64().unwrap_or(0),
                                        _ => 0,
                                    }
                                } else {
                                    0
                                };

                                let end_val = if let Some(end_expr) = end {
                                    match interp.eval_expr(end_expr, input.clone(), ctx.clone()).next() {
                                        Some(Ok(Jv::Number(n))) => n.as_i64(),
                                        _ => None,
                                    }
                                } else {
                                    None // None means to the end of array
                                };

                                indices.push(DeleteTarget::Slice { start: start_val, end: end_val });
                            }
                        }
                        ExprKind::Comma(l, r) => {
                            collect_indices_to_delete(l, input, ctx.clone(), indices);
                            collect_indices_to_delete(r, input, ctx, indices);
                        }
                        _ => {}
                    }
                }

                // Check if this is a simple array deletion case
                if let Jv::Array(arr) = &current {
                    let len = arr.len() as i64;
                    let mut indices = Vec::new();
                    collect_indices_to_delete(target, &current, ctx.clone(), &mut indices);

                    // Convert to absolute indices and flatten slices
                    let mut to_delete: std::collections::HashSet<usize> = std::collections::HashSet::new();
                    for target in indices {
                        match target {
                            DeleteTarget::Single(idx) => {
                                let abs_idx = if idx < 0 { (len + idx).max(0) as usize } else { idx as usize };
                                if abs_idx < arr.len() {
                                    to_delete.insert(abs_idx);
                                }
                            }
                            DeleteTarget::Slice { start, end } => {
                                let start_idx = if start < 0 { (len + start).max(0) as usize } else { (start as usize).min(arr.len()) };
                                let end_idx = match end {
                                    Some(e) if e < 0 => (len + e).max(0) as usize,
                                    Some(e) => (e as usize).min(arr.len()),
                                    None => arr.len(), // None means to the end
                                };
                                for i in start_idx..end_idx {
                                    to_delete.insert(i);
                                }
                            }
                        }
                    }

                    // Build result array without deleted indices
                    let mut result = Vec::new();
                    for (i, elem) in arr.iter().enumerate() {
                        if !to_delete.contains(&i) {
                            result.push(elem);
                        }
                    }
                    return Ok(Jv::from_vec(result));
                }

                // Fallback to sequential for non-array cases
                let result = Self::apply_deletion(current, left, ctx.clone())?;
                Self::apply_deletion(result, right, ctx)
            }
            ExprKind::Pipe(left, right) => {
                // del(left | right) means:
                // For each path generated by left, delete what right specifies
                // E.g., del(.foo | .[0]) means delete .foo[0]
                // E.g., del((.a, .b) | .[0]) means delete .a[0] and .b[0]

                // We need to handle this by:
                // 1. Collecting paths from left
                // 2. For each path, applying the right deletion
                // 3. Combining the results

                // Handle the case where left is a Comma (multiple paths)
                match &left.kind {
                    ExprKind::Paren(inner) => {
                        // del((a, b, c) | right) - handle parenthesized comma
                        match &inner.kind {
                            ExprKind::Comma(_, _) => {
                                // Collect all field expressions from the comma
                                fn collect_path_exprs<'a>(expr: &'a Expr, exprs: &mut Vec<&'a Expr>) {
                                    match &expr.kind {
                                        ExprKind::Comma(l, r) => {
                                            collect_path_exprs(l, exprs);
                                            collect_path_exprs(r, exprs);
                                        }
                                        _ => exprs.push(expr),
                                    }
                                }

                                let mut path_exprs = Vec::new();
                                collect_path_exprs(inner, &mut path_exprs);

                                // Apply deletion for each path
                                let mut result = current.clone();
                                for path_expr in path_exprs {
                                    // Check if this path actually exists in the object
                                    // For field access, check if the field exists
                                    let path_exists = match &path_expr.kind {
                                        ExprKind::Field(name) => {
                                            match &result {
                                                Jv::Object(obj) => obj.get(name).is_some(),
                                                _ => false,
                                            }
                                        }
                                        ExprKind::Index { expr: base, index, .. } => {
                                            if matches!(base.kind, ExprKind::Identity) {
                                                // Check if it's a field index on an object
                                                let mut interp = Interpreter { ctx: ctx.clone() };
                                                match interp.eval_expr(index, result.clone(), ctx.clone()).next() {
                                                    Some(Ok(Jv::String(s))) => {
                                                        match &result {
                                                            Jv::Object(obj) => obj.get(s.as_str()).is_some(),
                                                            _ => true,
                                                        }
                                                    }
                                                    _ => true, // For non-string indices, assume path exists
                                                }
                                            } else {
                                                true // For nested paths, assume exists and let it fail naturally
                                            }
                                        }
                                        _ => true, // For other expressions, assume path exists
                                    };

                                    if !path_exists {
                                        continue; // Skip paths that don't exist
                                    }

                                    // Get the value at this path
                                    let mut interp = Interpreter { ctx: ctx.clone() };
                                    match interp.eval_expr(path_expr, result.clone(), ctx.clone()).next() {
                                        Some(Ok(sub_value)) => {
                                            // Apply right deletion to this sub-value
                                            let modified = Self::apply_deletion(sub_value, right, ctx.clone())?;
                                            // Write it back using assignment
                                            result = Self::apply_assignment(result, path_expr, modified, &mut Vec::new(), ctx.clone())?;
                                        }
                                        Some(Err(e)) => return Err(e),
                                        None => {} // No value at this path, skip
                                    }
                                }
                                Ok(result)
                            }
                            _ => {
                                // Single parenthesized expression - treat like direct pipe
                                let mut interp = Interpreter { ctx: ctx.clone() };
                                match interp.eval_expr(left, current.clone(), ctx.clone()).next() {
                                    Some(Ok(sub_value)) => {
                                        let modified = Self::apply_deletion(sub_value, right, ctx.clone())?;
                                        Self::apply_assignment(current, left, modified, &mut Vec::new(), ctx)
                                    }
                                    Some(Err(e)) => Err(e),
                                    None => Ok(current),
                                }
                            }
                        }
                    }
                    _ => {
                        // Simple pipe: del(.foo | .[0])
                        let mut interp = Interpreter { ctx: ctx.clone() };
                        match interp.eval_expr(left, current.clone(), ctx.clone()).next() {
                            Some(Ok(sub_value)) => {
                                let modified = Self::apply_deletion(sub_value, right, ctx.clone())?;
                                Self::apply_assignment(current, left, modified, &mut Vec::new(), ctx)
                            }
                            Some(Err(e)) => Err(e),
                            None => Ok(current),
                        }
                    }
                }
            }
            ExprKind::FunctionCall { name, args, .. } => {
                // For function calls like empty, we evaluate and if it produces no results,
                // return input unchanged. This handles del(empty) = identity
                if name == "empty" && args.is_empty() {
                    return Ok(current);
                }
                // For other function calls, we can't delete from them
                Err(format!("Cannot delete from expression: {:?}", target.kind))
            }
            _ => Err(format!("Cannot delete from expression: {:?}", target.kind)),
        }
    }

    fn eval_getpath(&mut self, path_expr: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        // getpath(path_array) - evaluate the path expression to get array, then traverse
        let path_results: Vec<_> = self.eval_expr(path_expr, input.clone(), ctx.clone()).collect();

        Box::new(path_results.into_iter().flat_map(move |path_result| {
            match path_result {
                Err(e) => Box::new(std::iter::once(Err(e))) as EvalResult,
                Ok(Jv::Array(path)) => {
                    let mut current = input.clone();
                    for key in path.iter() {
                        current = current.index(&key);
                        if current.is_invalid() {
                            return Box::new(std::iter::once(Ok(Jv::Null))) as EvalResult;
                        }
                    }
                    Box::new(std::iter::once(Ok(current))) as EvalResult
                }
                Ok(v) => Box::new(std::iter::once(Err(format!("getpath requires array, got {}", v.type_name())))) as EvalResult,
            }
        }))
    }

    fn eval_isempty(&mut self, filter: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        // isempty(expr) returns true if expr produces no output
        let mut inner = Interpreter { ctx: ctx.clone() };
        let has_output = inner.eval_expr(filter, input, ctx).next().is_some();
        Box::new(std::iter::once(Ok(Jv::Bool(!has_output))))
    }

    fn eval_add_gen(&mut self, gen: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        // add(expr) - collect all values from generator and add them
        // equivalent to [expr] | add
        let mut inner = Interpreter { ctx: ctx.clone() };
        let values: Vec<_> = inner.eval_expr(gen, input, ctx).collect();

        if values.is_empty() {
            return Box::new(std::iter::once(Ok(Jv::Null)));
        }

        // Collect values into an array, then call add
        let mut items = Vec::new();
        for result in values {
            match result {
                Ok(v) => items.push(v),
                Err(e) => return Box::new(std::iter::once(Err(e))),
            }
        }

        if items.is_empty() {
            return Box::new(std::iter::once(Ok(Jv::Null)));
        }

        // Add all items together
        let mut acc = items[0].clone();
        for item in &items[1..] {
            acc = match (&acc, item) {
                (Jv::Null, other) => other.clone(),
                (other, Jv::Null) => other.clone(),
                (Jv::Number(a), Jv::Number(b)) => Jv::from_f64(a.as_f64() + b.as_f64()),
                (Jv::String(a), Jv::String(b)) => Jv::String(a.concat(b)),
                (Jv::Array(a), Jv::Array(b)) => {
                    let mut result = Vec::new();
                    result.extend(a.iter());
                    result.extend(b.iter());
                    Jv::from_vec(result)
                }
                (Jv::Object(a), Jv::Object(b)) => {
                    let mut result = a.clone();
                    for (k, v) in b.iter() {
                        result.set(&k, v);
                    }
                    Jv::Object(result)
                }
                _ => return Box::new(std::iter::once(Err(format!(
                    "Cannot add {} and {}",
                    acc.type_name(),
                    item.type_name()
                )))),
            };
        }

        Box::new(std::iter::once(Ok(acc)))
    }

    fn eval_until(&mut self, cond: &Expr, update: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        // until(cond; update) - apply update until cond is true
        let mut current = input;
        let max_iterations = 10000; // Safety limit

        for _ in 0..max_iterations {
            // Check condition
            let mut cond_interp = Interpreter { ctx: ctx.clone() };
            match cond_interp.eval_expr(cond, current.clone(), ctx.clone()).next() {
                Some(Ok(v)) if v.is_truthy() => {
                    return Box::new(std::iter::once(Ok(current)));
                }
                Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
                _ => {}
            }

            // Apply update
            let mut update_interp = Interpreter { ctx: ctx.clone() };
            match update_interp.eval_expr(update, current.clone(), ctx.clone()).next() {
                Some(Ok(v)) => current = v,
                Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
                None => return Box::new(std::iter::empty()),
            }
        }

        Box::new(std::iter::once(Err("until: too many iterations".to_string())))
    }

    fn eval_while(&mut self, cond: &Expr, update: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        // while(cond; update) - output each value while cond is true
        let mut current = input;
        let mut results = Vec::new();
        let max_iterations = 10000;

        for _ in 0..max_iterations {
            // Check condition
            let mut cond_interp = Interpreter { ctx: ctx.clone() };
            match cond_interp.eval_expr(cond, current.clone(), ctx.clone()).next() {
                Some(Ok(v)) if !v.is_truthy() => break,
                Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
                _ => {}
            }

            results.push(Ok(current.clone()));

            // Apply update
            let mut update_interp = Interpreter { ctx: ctx.clone() };
            match update_interp.eval_expr(update, current.clone(), ctx.clone()).next() {
                Some(Ok(v)) => current = v,
                Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
                None => break,
            }
        }

        Box::new(results.into_iter())
    }

    fn eval_repeat(&mut self, expr: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        // repeat(f) - repeatedly apply f to the same input, yielding each result
        // def repeat(f): f, repeat(f);
        // It applies f to the original input repeatedly
        // When f produces an error, repeat terminates (propagates the error)
        let expr_clone = expr.clone();
        let ctx_clone = ctx.clone();

        struct RepeatIter {
            expr: Expr,
            ctx: Rc<RefCell<Context>>,
            input: Jv,
            current_iter: Option<Box<dyn Iterator<Item = Result<Jv, String>>>>,
            count: usize,
            done: bool,
        }

        impl Iterator for RepeatIter {
            type Item = Result<Jv, String>;

            fn next(&mut self) -> Option<Self::Item> {
                if self.done {
                    return None;
                }
                if self.count > 100000 {
                    self.done = true;
                    return Some(Err("repeat: too many iterations".to_string()));
                }

                loop {
                    // If we have a current iterator, try to get next from it
                    if let Some(ref mut iter) = self.current_iter {
                        if let Some(result) = iter.next() {
                            self.count += 1;
                            // If we get an error, propagate it and stop repeat
                            if result.is_err() {
                                self.done = true;
                            }
                            return Some(result);
                        }
                    }

                    // Start a new iteration of the expression
                    let mut interp = Interpreter { ctx: self.ctx.clone() };
                    self.current_iter = Some(interp.eval_expr(&self.expr, self.input.clone(), self.ctx.clone()));
                }
            }
        }

        Box::new(RepeatIter {
            expr: expr_clone,
            ctx: ctx_clone,
            input,
            current_iter: None,
            count: 0,
            done: false,
        })
    }

    fn eval_range3(&mut self, start_expr: &Expr, end_expr: &Expr, step_expr: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        // range(start; end; step) - iterate over all combinations of start, end, step values
        let input_clone = input.clone();
        let input_clone2 = input.clone();
        let end_expr = end_expr.clone();
        let step_expr = step_expr.clone();
        let ctx_clone = ctx.clone();

        // Collect all start values
        let mut interp = Interpreter { ctx: ctx.clone() };
        let start_results: Vec<_> = interp.eval_expr(start_expr, input, ctx.clone()).collect();

        Box::new(start_results.into_iter().flat_map(move |start_result| {
            let end_expr = end_expr.clone();
            let step_expr = step_expr.clone();
            let input_clone = input_clone.clone();
            let input_clone2 = input_clone2.clone();
            let ctx_clone = ctx_clone.clone();

            match start_result {
                Ok(start_val) => {
                    let start = start_val.as_f64().unwrap_or(0.0);

                    let mut interp2 = Interpreter { ctx: ctx_clone.clone() };
                    let end_results: Vec<_> = interp2.eval_expr(&end_expr, input_clone, ctx_clone.clone()).collect();

                    Box::new(end_results.into_iter().flat_map(move |end_result| {
                        let step_expr = step_expr.clone();
                        let input_clone2 = input_clone2.clone();
                        let ctx_clone = ctx_clone.clone();

                        match end_result {
                            Ok(end_val) => {
                                let end = end_val.as_f64().unwrap_or(0.0);

                                let mut interp3 = Interpreter { ctx: ctx_clone.clone() };
                                let step_results: Vec<_> = interp3.eval_expr(&step_expr, input_clone2, ctx_clone).collect();

                                Box::new(step_results.into_iter().flat_map(move |step_result| {
                                    match step_result {
                                        Ok(step_val) => {
                                            let step = step_val.as_f64().unwrap_or(1.0);

                                            if step == 0.0 {
                                                return Box::new(std::iter::empty()) as EvalResult;
                                            }

                                            let mut results = Vec::new();
                                            let mut current = start;

                                            if step > 0.0 {
                                                while current < end {
                                                    results.push(Ok(Jv::from_f64(current)));
                                                    current += step;
                                                }
                                            } else {
                                                while current > end {
                                                    results.push(Ok(Jv::from_f64(current)));
                                                    current += step;
                                                }
                                            }

                                            Box::new(results.into_iter()) as EvalResult
                                        }
                                        Err(e) => Box::new(std::iter::once(Err(e))) as EvalResult,
                                    }
                                })) as EvalResult
                            }
                            Err(e) => Box::new(std::iter::once(Err(e))),
                        }
                    })) as EvalResult
                }
                Err(e) => Box::new(std::iter::once(Err(e))),
            }
        }))
    }

    fn eval_walk(&mut self, filter: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        // walk(f) - recursively apply f to all values (depth-first, bottom-up)
        // f can be a generator producing multiple outputs
        // If f produces no output for a value, that value is omitted
        fn walk_value(interp: &mut Interpreter, filter: &Expr, value: Jv, ctx: Rc<RefCell<Context>>) -> Vec<Result<Jv, String>> {
            // First, recursively walk children
            let walked = match &value {
                Jv::Array(arr) => {
                    let mut new_arr = Vec::new();
                    for item in arr.iter() {
                        // For arrays, collect all outputs from walking each item
                        let results = walk_value(interp, filter, item, ctx.clone());
                        for result in results {
                            match result {
                                Ok(v) => new_arr.push(v),
                                Err(e) => return vec![Err(e)],
                            }
                        }
                    }
                    Jv::from_vec(new_arr)
                }
                Jv::Object(obj) => {
                    let mut new_obj = crate::jv::JvObject::new();
                    for (k, v) in obj.iter() {
                        let results = walk_value(interp, filter, v, ctx.clone());
                        // Only include first result for objects (maintaining single value per key)
                        // If no results, omit the key
                        if let Some(first) = results.into_iter().next() {
                            match first {
                                Ok(walked_v) => new_obj.set(&k, walked_v),
                                Err(e) => return vec![Err(e)],
                            }
                        }
                        // If empty results, key is omitted
                    }
                    Jv::Object(new_obj)
                }
                _ => value.clone(),
            };

            // Then apply filter to the walked value - collect ALL outputs from the generator
            let mut filter_interp = Interpreter { ctx: ctx.clone() };
            filter_interp.eval_expr(filter, walked, ctx).collect()
        }

        // Collect all results from walk_value
        let results = walk_value(self, filter, input, ctx);
        Box::new(results.into_iter())
    }

    fn eval_env(&mut self, _input: Jv) -> EvalResult {
        // Return environment variables as object
        let mut obj = crate::jv::JvObject::new();
        for (key, value) in std::env::vars() {
            obj.set(&key, Jv::string(value));
        }
        Box::new(std::iter::once(Ok(Jv::Object(obj))))
    }

    fn eval_splits(&mut self, sep_expr: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        // splits(sep) - stream version of split using regex
        let sep = match self.eval_expr(sep_expr, input.clone(), ctx.clone()).next() {
            Some(Ok(Jv::String(s))) => s.as_str().to_string(),
            Some(Ok(v)) => return Box::new(std::iter::once(Err(format!("splits requires string separator, got {}", v.type_name())))),
            Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
            None => return Box::new(std::iter::empty()),
        };

        match &input {
            Jv::String(s) => {
                match regex::Regex::new(&sep) {
                    Ok(re) => {
                        let parts: Vec<Jv> = re.split(s.as_str())
                            .map(|p| Jv::string(p))
                            .collect();
                        Box::new(parts.into_iter().map(Ok))
                    }
                    Err(e) => Box::new(std::iter::once(Err(format!("invalid regex: {}", e)))),
                }
            }
            _ => Box::new(std::iter::once(Err(format!("splits requires string input, got {}", input.type_name())))),
        }
    }

    /// Evaluate sub/gsub with proper interpolation support
    fn eval_sub(&mut self, pattern_expr: &Expr, replacement_expr: &Expr, input: Jv, ctx: Rc<RefCell<Context>>, global: bool) -> EvalResult {
        self.eval_sub_impl(pattern_expr, replacement_expr, None, input, ctx, global)
    }

    fn eval_sub_flags(&mut self, pattern_expr: &Expr, replacement_expr: &Expr, flags_expr: &Expr, input: Jv, ctx: Rc<RefCell<Context>>, global: bool) -> EvalResult {
        self.eval_sub_impl(pattern_expr, replacement_expr, Some(flags_expr), input, ctx, global)
    }

    fn eval_sub_impl(&mut self, pattern_expr: &Expr, replacement_expr: &Expr, flags_expr: Option<&Expr>, input: Jv, ctx: Rc<RefCell<Context>>, global: bool) -> EvalResult {
        // Evaluate pattern
        let pattern = match self.eval_expr(pattern_expr, input.clone(), ctx.clone()).next() {
            Some(Ok(Jv::String(s))) => s.as_str().to_string(),
            Some(Ok(v)) => return Box::new(std::iter::once(Err(format!("sub/gsub pattern must be string, got {}", v.type_name())))),
            Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
            None => return Box::new(std::iter::empty()),
        };

        // Evaluate flags if present
        let flags = if let Some(flags_e) = flags_expr {
            match self.eval_expr(flags_e, input.clone(), ctx.clone()).next() {
                Some(Ok(Jv::String(s))) => s.as_str().to_string(),
                Some(Ok(v)) => return Box::new(std::iter::once(Err(format!("sub/gsub flags must be string, got {}", v.type_name())))),
                Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
                None => String::new(),
            }
        } else {
            String::new()
        };

        // 'g' flag in flags string makes sub behave like gsub
        let is_global = global || flags.contains('g');

        // Build regex with flags
        let mut regex_pattern = String::new();
        if flags.contains('i') {
            regex_pattern.push_str("(?i)");
        }
        if flags.contains('x') {
            regex_pattern.push_str("(?x)");
        }
        if flags.contains('s') {
            regex_pattern.push_str("(?s)");
        }
        if flags.contains('m') {
            regex_pattern.push_str("(?m)");
        }
        regex_pattern.push_str(&pattern);

        let s = match &input {
            Jv::String(s) => s.as_str().to_string(),
            _ => return Box::new(std::iter::once(Err(format!("sub/gsub requires string input, got {}", input.type_name())))),
        };

        let re = match regex::Regex::new(&regex_pattern) {
            Ok(r) => r,
            Err(e) => return Box::new(std::iter::once(Err(format!("invalid regex: {}", e)))),
        };

        // Get named capture groups
        let capture_names: Vec<Option<&str>> = re.capture_names().collect();

        let replacement_expr = replacement_expr.clone();
        let ctx_clone = ctx.clone();

        // Process replacements
        if is_global {
            // gsub - replace all matches
            // Collect all captures first
            let mut all_caps: Vec<_> = re.captures_iter(&s).collect();

            // Check if we should add an empty match at the very end
            // jq/oniguruma includes an empty match at position len if the pattern can match there
            // but Rust regex only includes it if the previous match didn't end there
            let last_end = all_caps.last().map(|c| c.get(0).unwrap().end()).unwrap_or(0);
            if last_end == s.len() && !all_caps.is_empty() {
                // Previous match ended exactly at the end of string
                // Only add empty match if the pattern can match empty AND last match was non-empty
                let last_was_nonempty = all_caps.last()
                    .map(|c| c.get(0).unwrap().end() > c.get(0).unwrap().start())
                    .unwrap_or(false);
                if last_was_nonempty && re.is_match("") {
                    if let Some(caps) = re.captures_at(&s, s.len()) {
                        if caps.get(0).unwrap().start() == s.len() && caps.get(0).unwrap().end() == s.len() {
                            all_caps.push(caps);
                        }
                    }
                }
            }

            if all_caps.is_empty() {
                // No match, return input unchanged
                return Box::new(std::iter::once(Ok(input)));
            }

            // For each capture, collect all replacement values
            let mut caps_with_replacements: Vec<(regex::Captures, Vec<String>)> = Vec::new();
            for caps in all_caps {
                let capture_obj = self.build_capture_object(&caps, &capture_names);

                // Collect ALL replacement values from evaluating the expression
                let mut inner = Interpreter { ctx: ctx_clone.clone() };
                let mut replacements = Vec::new();
                for result in inner.eval_expr(&replacement_expr, capture_obj, ctx_clone.clone()) {
                    match result {
                        Ok(Jv::String(repl)) => replacements.push(repl.as_str().to_string()),
                        Ok(v) => return Box::new(std::iter::once(Err(format!("sub/gsub replacement must produce string, got {}", v.type_name())))),
                        Err(e) => return Box::new(std::iter::once(Err(e))),
                    }
                }
                if replacements.is_empty() {
                    replacements.push(String::new()); // Empty replacement
                }
                caps_with_replacements.push((caps, replacements));
            }

            // Generate cartesian product of all replacement combinations
            // For simplicity, if there are multiple matches and multiple replacements,
            // we use the same replacement index for all matches
            let max_replacements = caps_with_replacements.iter().map(|(_, r)| r.len()).max().unwrap_or(1);

            let mut results = Vec::new();
            for repl_idx in 0..max_replacements {
                let mut result = String::new();
                let mut last_end = 0;

                for (caps, replacements) in &caps_with_replacements {
                    let m = caps.get(0).unwrap();
                    result.push_str(&s[last_end..m.start()]);

                    // Use repl_idx if available, otherwise use the last replacement
                    let repl = if repl_idx < replacements.len() {
                        &replacements[repl_idx]
                    } else {
                        replacements.last().unwrap()
                    };
                    result.push_str(repl);

                    last_end = m.end();
                }

                result.push_str(&s[last_end..]);
                results.push(Ok(Jv::string(result)));
            }

            Box::new(results.into_iter())
        } else {
            // sub - replace first match only
            if let Some(caps) = re.captures(&s) {
                let m = caps.get(0).unwrap();

                // Build capture object
                let capture_obj = self.build_capture_object(&caps, &capture_names);

                // Collect ALL replacement values from evaluating the expression
                let mut inner = Interpreter { ctx: ctx_clone.clone() };
                let mut results = Vec::new();

                for result in inner.eval_expr(&replacement_expr, capture_obj, ctx_clone.clone()) {
                    match result {
                        Ok(Jv::String(repl)) => {
                            let mut res = String::new();
                            res.push_str(&s[..m.start()]);
                            res.push_str(repl.as_str());
                            res.push_str(&s[m.end()..]);
                            results.push(Ok(Jv::string(res)));
                        }
                        Ok(v) => return Box::new(std::iter::once(Err(format!("sub/gsub replacement must produce string, got {}", v.type_name())))),
                        Err(e) => return Box::new(std::iter::once(Err(e))),
                    }
                }

                if results.is_empty() {
                    // No replacement values, return unchanged
                    Box::new(std::iter::once(Ok(input)))
                } else {
                    Box::new(results.into_iter())
                }
            } else {
                // No match, return input unchanged
                Box::new(std::iter::once(Ok(input)))
            }
        }
    }

    /// Build a capture object from regex captures for sub/gsub replacement
    fn build_capture_object(&self, caps: &regex::Captures, capture_names: &[Option<&str>]) -> Jv {
        let mut obj = crate::jv::JvObject::new();

        // Add named captures
        for (i, name_opt) in capture_names.iter().enumerate() {
            if let Some(name) = name_opt {
                if let Some(m) = caps.get(i) {
                    obj.set(name, Jv::string(m.as_str()));
                } else {
                    obj.set(name, Jv::Null);
                }
            }
        }

        Jv::Object(obj)
    }

    fn eval_with_entries(&mut self, filter: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        // with_entries(f) = to_entries | map(f) | from_entries
        match &input {
            Jv::Object(obj) => {
                // Convert to entries array
                let mut entries = Vec::new();
                for (k, v) in obj.iter() {
                    let mut entry = crate::jv::JvObject::new();
                    entry.set("key", Jv::string(k));
                    entry.set("value", v);
                    entries.push(Jv::Object(entry));
                }

                // Apply filter to each entry
                let mut new_entries = Vec::new();
                for entry in entries {
                    let mut inner = Interpreter { ctx: ctx.clone() };
                    for result in inner.eval_expr(filter, entry, ctx.clone()) {
                        match result {
                            Ok(v) => new_entries.push(v),
                            Err(e) => return Box::new(std::iter::once(Err(e))),
                        }
                    }
                }

                // Convert back from entries
                let mut result_obj = crate::jv::JvObject::new();
                for entry in new_entries {
                    if let Jv::Object(e) = entry {
                        if let (Some(Jv::String(key)), Some(value)) = (e.get("key"), e.get("value")) {
                            result_obj.set(key.as_str(), value);
                        }
                    }
                }
                Box::new(std::iter::once(Ok(Jv::Object(result_obj))))
            }
            _ => Box::new(std::iter::once(Err(format!("with_entries requires object, got {}", input.type_name())))),
        }
    }

    fn eval_map_values(&mut self, filter: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        // map_values(f) applies f to each value in an object or array
        match &input {
            Jv::Object(obj) => {
                let mut result_obj = crate::jv::JvObject::new();
                for (k, v) in obj.iter() {
                    let mut inner = Interpreter { ctx: ctx.clone() };
                    match inner.eval_expr(filter, v, ctx.clone()).next() {
                        Some(Ok(new_v)) => result_obj.set(&k, new_v),
                        Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
                        None => {} // Skip if filter produces no output
                    }
                }
                Box::new(std::iter::once(Ok(Jv::Object(result_obj))))
            }
            Jv::Array(arr) => {
                let mut result = Vec::new();
                for item in arr.iter() {
                    let mut inner = Interpreter { ctx: ctx.clone() };
                    match inner.eval_expr(filter, item, ctx.clone()).next() {
                        Some(Ok(v)) => result.push(v),
                        Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
                        None => {} // Skip
                    }
                }
                Box::new(std::iter::once(Ok(Jv::from_vec(result))))
            }
            Jv::Null => Box::new(std::iter::once(Ok(Jv::Null))),
            _ => Box::new(std::iter::once(Err(format!("map_values requires object or array, got {}", input.type_name())))),
        }
    }

    fn eval_path(&mut self, filter: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        // path(expr) returns the path(s) to the value(s) selected by expr
        fn collect_paths(expr: &Expr, input: &Jv, ctx: Rc<RefCell<Context>>, current_path: Vec<Jv>) -> Vec<Vec<Jv>> {
            let mut paths = Vec::new();

            match &expr.kind {
                ExprKind::Identity => {
                    paths.push(current_path);
                }
                ExprKind::Field(name) => {
                    let mut new_path = current_path;
                    new_path.push(Jv::string(name.clone()));
                    paths.push(new_path);
                }
                ExprKind::Index { expr: base_expr, index, .. } => {
                    // First collect paths from base expression
                    let base_paths = collect_paths(base_expr, input, ctx.clone(), current_path);
                    for base_path in base_paths {
                        // Then add the index to each base path
                        // Evaluate the index - it may produce multiple values (e.g., 0,1)
                        let mut interp = Interpreter { ctx: ctx.clone() };
                        for idx_result in interp.eval_expr(index, input.clone(), ctx.clone()) {
                            if let Ok(idx) = idx_result {
                                let mut new_path = base_path.clone();
                                new_path.push(idx);
                                paths.push(new_path);
                            }
                        }
                    }
                }
                ExprKind::Pipe(left, right) => {
                    // For pipes, we need to traverse left first, then right
                    let left_paths = collect_paths(left, input, ctx.clone(), current_path);
                    for path in left_paths {
                        // Check if this path contains an invalid marker
                        if path.len() >= 2 {
                            if let Some(Jv::String(marker)) = path.get(path.len() - 2) {
                                if marker.as_str() == "__INVALID_PATH_MARKER__" {
                                    // Found invalid path marker
                                    let result_value = path.last().unwrap();
                                    use crate::jv::print_jv;
                                    let formatted = print_jv(result_value);

                                    // Check what kind of access is being attempted on the right side
                                    let error_msg = match &right.kind {
                                        ExprKind::Index { index, .. } => {
                                            // Evaluate index to get the access key
                                            let mut interp = Interpreter { ctx: ctx.clone() };
                                            match interp.eval_expr(index, result_value.clone(), ctx.clone()).next() {
                                                Some(Ok(Jv::Number(n))) => {
                                                    format!("Invalid path expression near attempt to access element {} of {}", n, formatted)
                                                }
                                                Some(Ok(Jv::String(s))) => {
                                                    format!("Invalid path expression near attempt to access element \"{}\" of {}", s.as_str(), formatted)
                                                }
                                                _ => format!("Invalid path expression with result {}", formatted)
                                            }
                                        }
                                        ExprKind::Iterator { .. } => {
                                            format!("Invalid path expression near attempt to iterate through {}", formatted)
                                        }
                                        ExprKind::Field(name) => {
                                            format!("Invalid path expression near attempt to access element \"{}\" of {}", name, formatted)
                                        }
                                        _ => format!("Invalid path expression with result {}", formatted)
                                    };

                                    // Create error marker with the specific message
                                    let mut error_path = Vec::new();
                                    error_path.push(Jv::string("__INVALID_PATH_MARKER__"));
                                    error_path.push(Jv::string(error_msg));
                                    paths.push(error_path);
                                    continue;
                                }
                            }
                        }

                        // Navigate to the value at this path, then continue with right
                        let value_at_path = get_value_at_path(input, &path);
                        let right_paths = collect_paths(right, &value_at_path, ctx.clone(), path);
                        paths.extend(right_paths);
                    }
                }
                ExprKind::Iterator { expr: base_expr, .. } => {
                    // First get the base value for the iterator
                    let base_value = if let ExprKind::Identity = base_expr.kind {
                        input.clone()
                    } else {
                        // For complex base expressions, we'd need to navigate
                        input.clone()
                    };
                    // For .[], enumerate all paths
                    match &base_value {
                        Jv::Array(arr) => {
                            for i in 0..arr.len() {
                                let mut new_path = current_path.clone();
                                new_path.push(Jv::from_i64(i as i64));
                                paths.push(new_path);
                            }
                        }
                        Jv::Object(obj) => {
                            for (k, _) in obj.iter() {
                                let mut new_path = current_path.clone();
                                new_path.push(Jv::string(k));
                                paths.push(new_path);
                            }
                        }
                        _ => {}
                    }
                }
                ExprKind::Optional(inner) => {
                    // Try to get paths from inner expression
                    paths.extend(collect_paths(inner, input, ctx, current_path));
                }
                ExprKind::RecursiveDescent => {
                    // .. returns all paths recursively
                    fn collect_recursive(value: &Jv, base_path: Vec<Jv>, paths: &mut Vec<Vec<Jv>>) {
                        // First add current path
                        paths.push(base_path.clone());
                        // Then recurse into children
                        match value {
                            Jv::Object(obj) => {
                                for (k, v) in obj.iter() {
                                    let mut child_path = base_path.clone();
                                    child_path.push(Jv::string(k));
                                    collect_recursive(&v, child_path, paths);
                                }
                            }
                            Jv::Array(arr) => {
                                for (i, v) in arr.iter().enumerate() {
                                    let mut child_path = base_path.clone();
                                    child_path.push(Jv::from_i64(i as i64));
                                    collect_recursive(&v, child_path, paths);
                                }
                            }
                            _ => {}
                        }
                    }
                    // Get value at current path
                    let value_at_path = get_value_at_path(input, &current_path);
                    collect_recursive(&value_at_path, current_path, &mut paths);
                }
                ExprKind::Comma(left, right) => {
                    // For comma, collect paths from both sides
                    paths.extend(collect_paths(left, input, ctx.clone(), current_path.clone()));
                    paths.extend(collect_paths(right, input, ctx, current_path));
                }
                ExprKind::FunctionCall { name, args, .. } => {
                    // For function calls like select(...), we need to evaluate and filter
                    if name == "select" && args.len() == 1 {
                        // select(cond): only returns current path if cond is true
                        let mut interp = Interpreter { ctx: ctx.clone() };
                        match interp.eval_expr(&args[0], input.clone(), ctx).next() {
                            Some(Ok(Jv::Bool(true))) => {
                                // Condition is true, keep this path
                                paths.push(current_path);
                            }
                            _ => {
                                // Condition is false or error, don't include this path
                            }
                        }
                    } else if name == "first" && args.is_empty() {
                        // first is equivalent to .[0]
                        let mut new_path = current_path;
                        new_path.push(Jv::from_i64(0));
                        paths.push(new_path);
                    } else if name == "last" && args.is_empty() {
                        // last is equivalent to .[-1]
                        let mut new_path = current_path;
                        new_path.push(Jv::from_i64(-1));
                        paths.push(new_path);
                    } else {
                        // For other function calls (map, sort, etc.), they transform values
                        // and are not valid path expressions.
                        // However, we need to return an error with the result value
                        // to match jq's error message format.
                        // For now, we evaluate to get the result and store it as a special marker.
                        let mut interp = Interpreter { ctx: ctx.clone() };
                        match interp.eval_expr(expr, input.clone(), ctx).next() {
                            Some(Ok(result)) => {
                                // Mark this as an invalid path by returning a special sentinel
                                // We'll detect this later and generate the appropriate error
                                let mut marker_path = current_path;
                                marker_path.push(Jv::string("__INVALID_PATH_MARKER__"));
                                marker_path.push(result);
                                paths.push(marker_path);
                            }
                            _ => {}
                        }
                    }
                }
                _ => {
                    // For other expressions, just return the current path
                    paths.push(current_path);
                }
            }

            paths
        }

        fn get_value_at_path(input: &Jv, path: &[Jv]) -> Jv {
            let mut current = input.clone();
            for p in path {
                match (&current, p) {
                    (Jv::Object(obj), Jv::String(key)) => {
                        current = obj.get(key.as_str()).unwrap_or(Jv::Null);
                    }
                    (Jv::Array(arr), Jv::Number(n)) => {
                        if let Some(idx) = n.as_i64() {
                            current = arr.get(idx).unwrap_or(Jv::Null);
                        } else {
                            return Jv::Null;
                        }
                    }
                    _ => return Jv::Null,
                }
            }
            current
        }

        let all_paths = collect_paths(filter, &input, ctx, Vec::new());
        let results: Vec<_> = all_paths.into_iter()
            .map(|p| {
                // Check for invalid path marker
                if p.len() >= 2 {
                    if let Some(Jv::String(marker)) = p.first() {
                        if marker.as_str() == "__INVALID_PATH_MARKER__" {
                            // New format: marker followed by error message string
                            if let Some(Jv::String(error_msg)) = p.get(1) {
                                return Err(error_msg.as_str().to_string());
                            }
                        }
                    }
                    if let Some(Jv::String(marker)) = p.get(p.len() - 2) {
                        if marker.as_str() == "__INVALID_PATH_MARKER__" {
                            // Old format: path followed by marker and result
                            let result_value = p.last().unwrap();
                            use crate::jv::print_jv;
                            let formatted = print_jv(result_value);
                            return Err(format!("Invalid path expression with result {}", formatted));
                        }
                    }
                }
                Ok(Jv::from_vec(p))
            })
            .collect();
        Box::new(results.into_iter())
    }

    fn eval_paths_filter(&mut self, filter: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        // paths(f) - returns paths to values where f is true
        fn collect_all_paths(value: &Jv, current_path: Vec<Jv>, results: &mut Vec<(Vec<Jv>, Jv)>) {
            results.push((current_path.clone(), value.clone()));

            match value {
                Jv::Array(arr) => {
                    for (i, item) in arr.iter().enumerate() {
                        let mut new_path = current_path.clone();
                        new_path.push(Jv::from_i64(i as i64));
                        collect_all_paths(&item, new_path, results);
                    }
                }
                Jv::Object(obj) => {
                    for (k, v) in obj.iter() {
                        let mut new_path = current_path.clone();
                        new_path.push(Jv::string(k));
                        collect_all_paths(&v, new_path, results);
                    }
                }
                _ => {}
            }
        }

        let mut all_values = Vec::new();
        collect_all_paths(&input, Vec::new(), &mut all_values);

        let mut matching_paths = Vec::new();
        for (path, value) in all_values {
            if path.is_empty() {
                continue; // Skip root
            }
            let mut inner = Interpreter { ctx: ctx.clone() };
            match inner.eval_expr(filter, value, ctx.clone()).next() {
                Some(Ok(v)) if v.is_truthy() => {
                    matching_paths.push(Ok(Jv::from_vec(path)));
                }
                Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
                _ => {}
            }
        }

        Box::new(matching_paths.into_iter())
    }

    fn eval_pick(&mut self, filter: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        // pick(path_exprs) - returns object with only the specified paths
        // First, get all the paths from the filter expression
        let path_results = self.eval_path(filter, input.clone(), ctx.clone());

        // Collect all paths
        let mut paths = Vec::new();
        for result in path_results {
            match result {
                Ok(path_arr) => {
                    if let Jv::Array(arr) = path_arr {
                        let path: Vec<Jv> = arr.iter().collect();
                        paths.push(path);
                    }
                }
                Err(e) => return Box::new(std::iter::once(Err(e))),
            }
        }

        // Build a new object/value with only the picked paths
        fn set_at_path(target: &mut Jv, path: &[Jv], value: Jv) {
            if path.is_empty() {
                *target = value;
                return;
            }

            let key = &path[0];
            let rest = &path[1..];

            // Helper to determine what container type to create for a path
            fn make_container_for_path(path: &[Jv]) -> Jv {
                if path.first().map(|k| matches!(k, Jv::Number(_))).unwrap_or(false) {
                    Jv::Array(crate::jv::JvArray::new())
                } else {
                    Jv::Object(crate::jv::JvObject::new())
                }
            }

            match key {
                Jv::String(k) => {
                    let k = k.as_str();
                    if let Jv::Object(obj) = target {
                        if rest.is_empty() {
                            obj.set(k, value);
                        } else {
                            let existing = obj.get(k).unwrap_or_else(|| make_container_for_path(rest));
                            let mut nested = existing;
                            set_at_path(&mut nested, rest, value);
                            obj.set(k, nested);
                        }
                    }
                }
                Jv::Number(n) => {
                    if let (Some(idx), Jv::Array(arr)) = (n.as_i64(), target) {
                        // Use arr.set which handles bounds checking
                        if rest.is_empty() {
                            let _ = arr.set(idx, value);
                        } else {
                            let existing = arr.get(idx).unwrap_or_else(|| make_container_for_path(rest));
                            let mut nested = existing;
                            set_at_path(&mut nested, rest, value);
                            let _ = arr.set(idx, nested);
                        }
                    }
                }
                _ => {}
            }
        }

        fn get_at_path(source: &Jv, path: &[Jv]) -> Option<Jv> {
            if path.is_empty() {
                return Some(source.clone());
            }

            let key = &path[0];
            let rest = &path[1..];

            match (source, key) {
                (Jv::Object(obj), Jv::String(k)) => {
                    obj.get(k.as_str()).and_then(|v| get_at_path(&v, rest))
                }
                (Jv::Array(arr), Jv::Number(n)) => {
                    n.as_i64().and_then(|idx| arr.get(idx)).and_then(|v| get_at_path(&v, rest))
                }
                _ => None,
            }
        }

        // Determine the result type based on the input type and the paths
        // If all paths start with a numeric index, we need an array; otherwise object
        let mut result = if paths.iter().all(|p| p.first().map(|k| matches!(k, Jv::Number(_))).unwrap_or(false)) {
            match &input {
                Jv::Array(_) => Jv::Array(crate::jv::JvArray::new()),
                _ => Jv::Object(crate::jv::JvObject::new()),
            }
        } else {
            Jv::Object(crate::jv::JvObject::new())
        };

        for path in paths {
            // Check for negative indices which are not supported in pick
            for key in &path {
                if let Jv::Number(n) = key {
                    if let Some(idx) = n.as_i64() {
                        if idx < 0 {
                            return Box::new(std::iter::once(Err("Out of bounds negative array index".to_string())));
                        }
                    }
                }
            }
            // Get value at path, defaulting to null if path doesn't exist
            let value = get_at_path(&input, &path).unwrap_or(Jv::Null);
            set_at_path(&mut result, &path, value);
        }

        Box::new(std::iter::once(Ok(result)))
    }

    fn eval_string_interp(&mut self, parts: &[StringPart], input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        let mut result = String::new();

        for part in parts {
            match part {
                StringPart::Text(s) => result.push_str(s),
                StringPart::Interp(expr) => {
                    let mut inner = Interpreter { ctx: ctx.clone() };
                    match inner.eval_expr(expr, input.clone(), ctx.clone()).next() {
                        Some(Ok(v)) => {
                            match &v {
                                Jv::String(s) => result.push_str(s.as_str()),
                                _ => {
                                    use crate::jv::print_jv;
                                    result.push_str(&print_jv(&v));
                                }
                            }
                        }
                        Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
                        None => {}
                    }
                }
            }
        }

        Box::new(std::iter::once(Ok(Jv::string(result))))
    }

    /// Evaluate a format with a string template (e.g., @html "<b>\(.)</b>")
    /// The interpolated values are formatted, but literal parts are not.
    fn eval_format_template(&mut self, format: &str, parts: &[StringPart], input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        let format_name = format!("@{}", format);
        let mut result = String::new();

        for part in parts {
            match part {
                StringPart::Text(s) => result.push_str(s),
                StringPart::Interp(expr) => {
                    let mut inner = Interpreter { ctx: ctx.clone() };
                    match inner.eval_expr(expr, input.clone(), ctx.clone()).next() {
                        Some(Ok(v)) => {
                            // Convert value to string first
                            let str_val = match &v {
                                Jv::String(s) => s.as_str().to_string(),
                                _ => {
                                    use crate::jv::print_jv;
                                    print_jv(&v)
                                }
                            };

                            // Apply format to the string value
                            let ctx_mut = ctx.borrow_mut();
                            if let Some(builtin) = ctx_mut.get_builtin(&format_name, 0) {
                                let builtin_fn = *builtin;
                                drop(ctx_mut);
                                match builtin_fn(&mut Context::new(), Jv::string(str_val), &[]).next() {
                                    Some(Ok(Jv::String(formatted))) => result.push_str(formatted.as_str()),
                                    Some(Ok(other)) => {
                                        use crate::jv::print_jv;
                                        result.push_str(&print_jv(&other));
                                    }
                                    Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
                                    None => {}
                                }
                            } else {
                                return Box::new(std::iter::once(Err(format!("unknown format: {}", format_name))));
                            }
                        }
                        Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
                        None => {}
                    }
                }
            }
        }

        Box::new(std::iter::once(Ok(Jv::string(result))))
    }

    /// Apply an update using path-based approach (like jq's _modify function)
    ///
    /// This implements the jq pattern:
    ///   reduce path(target) as $p (.; . | setpath($p; (getpath($p) | update)))
    ///
    /// This ensures all paths are updated atomically on a single result.
    fn apply_update_with_paths(
        &mut self,
        input: Jv,
        target: &Expr,
        update: &Expr,
        ctx: Rc<RefCell<Context>>,
    ) -> EvalResult {
        // First, collect all paths from the target expression
        let paths = self.collect_paths_for_update(target, &input, ctx.clone());

        if paths.is_empty() {
            // No paths selected, return input unchanged
            return Box::new(std::iter::once(Ok(input)));
        }

        // Apply updates to each path, accumulating the result
        let mut result = input.clone();
        for path in paths {
            // Check for error markers in path
            if !path.is_empty() {
                if let Jv::String(s) = &path[0] {
                    if s.as_str() == "__INVALID_PATH_MARKER__" {
                        // This is an error path, skip it
                        continue;
                    }
                }
            }

            // Get current value at this path
            let current_value = get_value_at_path(&result, &path);

            // Apply the update expression to the current value
            let mut update_interp = Interpreter { ctx: ctx.clone() };
            let new_value = match update_interp.eval_expr(update, current_value, ctx.clone()).next() {
                Some(Ok(v)) => v,
                Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
                None => {
                    // Update returned empty, this path should be deleted
                    // For now, just skip (TODO: implement deletion)
                    continue;
                }
            };

            // Set the new value at this path
            result = set_value_at_path(result, &path, new_value);
        }

        Box::new(std::iter::once(Ok(result)))
    }

    /// Collect all paths from a target expression for update operations
    fn collect_paths_for_update(&mut self, expr: &Expr, input: &Jv, ctx: Rc<RefCell<Context>>) -> Vec<Vec<Jv>> {
        let mut paths = Vec::new();
        self.collect_paths_recursive(expr, input, ctx, Vec::new(), &mut paths);
        paths
    }

    /// Recursively collect paths from an expression
    fn collect_paths_recursive(
        &mut self,
        expr: &Expr,
        input: &Jv,
        ctx: Rc<RefCell<Context>>,
        current_path: Vec<Jv>,
        paths: &mut Vec<Vec<Jv>>,
    ) {
        match &expr.kind {
            ExprKind::Identity => {
                paths.push(current_path);
            }

            ExprKind::Field(name) => {
                let mut new_path = current_path;
                new_path.push(Jv::string(name.clone()));
                paths.push(new_path);
            }

            ExprKind::Index { expr: base_expr, index, .. } => {
                // First collect paths from base expression
                let mut base_paths = Vec::new();
                self.collect_paths_recursive(base_expr, input, ctx.clone(), current_path, &mut base_paths);

                for base_path in base_paths {
                    // Get value at base path to evaluate index against
                    let base_value = get_value_at_path(input, &base_path);

                    // Evaluate the index expression
                    let mut interp = Interpreter { ctx: ctx.clone() };
                    for idx_result in interp.eval_expr(index, base_value.clone(), ctx.clone()) {
                        if let Ok(idx) = idx_result {
                            let mut new_path = base_path.clone();
                            new_path.push(idx);
                            paths.push(new_path);
                        }
                    }
                }
            }

            ExprKind::Iterator { expr: base_expr, .. } => {
                // First collect paths from base expression
                let mut base_paths = Vec::new();
                self.collect_paths_recursive(base_expr, input, ctx.clone(), current_path, &mut base_paths);

                for base_path in base_paths {
                    // Get value at base path
                    let base_value = get_value_at_path(input, &base_path);

                    // Iterate over all elements
                    match &base_value {
                        Jv::Array(arr) => {
                            for i in 0..arr.len() {
                                let mut new_path = base_path.clone();
                                new_path.push(Jv::from_i64(i as i64));
                                paths.push(new_path);
                            }
                        }
                        Jv::Object(obj) => {
                            for (k, _) in obj.iter() {
                                let mut new_path = base_path.clone();
                                new_path.push(Jv::string(k));
                                paths.push(new_path);
                            }
                        }
                        _ => {}
                    }
                }
            }

            ExprKind::Pipe(left, right) => {
                // For pipes, collect paths from left, then continue with right
                let mut left_paths = Vec::new();
                self.collect_paths_recursive(left, input, ctx.clone(), current_path, &mut left_paths);

                for left_path in left_paths {
                    // Navigate to the value at left path
                    let left_value = get_value_at_path(input, &left_path);
                    // Continue collecting from right with the left path as base
                    self.collect_paths_recursive(right, &left_value, ctx.clone(), left_path, paths);
                }
            }

            ExprKind::Comma(left, right) => {
                // For comma, collect paths from both sides
                self.collect_paths_recursive(left, input, ctx.clone(), current_path.clone(), paths);
                self.collect_paths_recursive(right, input, ctx, current_path, paths);
            }

            ExprKind::Optional(inner) => {
                // For optional, try to collect paths from inner
                self.collect_paths_recursive(inner, input, ctx, current_path, paths);
            }

            ExprKind::Paren(inner) => {
                self.collect_paths_recursive(inner, input, ctx, current_path, paths);
            }

            ExprKind::FunctionCall { module: None, name, args } if args.is_empty() => {
                // Zero-argument function call might be a bound expression (parameter reference)
                if let Some((bound_expr, bound_ctx)) = ctx.borrow().lookup_expr_with_context(name) {
                    // This is a bound expression - recursively collect paths from it
                    self.collect_paths_recursive(&bound_expr, input, bound_ctx, current_path, paths);
                    return;
                }

                // Otherwise, treat as a function call that produces a value
                // For functions like select(), we need to evaluate and filter
                if name == "select" {
                    // select always returns current path if condition passes
                    paths.push(current_path);
                } else {
                    // Generic function - just use current path
                    paths.push(current_path);
                }
            }

            ExprKind::FunctionCall { module: None, name, args } if args.len() == 1 && name == "select" => {
                // select(cond): returns current path if condition is true
                let mut interp = Interpreter { ctx: ctx.clone() };
                let value_at_path = get_value_at_path(input, &current_path);
                if let Some(Ok(Jv::Bool(true))) = interp.eval_expr(&args[0], value_at_path, ctx).next() {
                    paths.push(current_path);
                }
                // If condition is false or error, don't include this path
            }

            ExprKind::FunctionCall { module: None, name, args } if args.len() == 1 && name == "getpath" => {
                // getpath(path_expr) - evaluate path_expr and use result as path
                let mut interp = Interpreter { ctx: ctx.clone() };
                let value_at_path = get_value_at_path(input, &current_path);

                for path_result in interp.eval_expr(&args[0], value_at_path, ctx.clone()) {
                    if let Ok(Jv::Array(path_arr)) = path_result {
                        // Build full path: current_path + path from getpath argument
                        let mut full_path = current_path.clone();
                        for elem in path_arr.iter() {
                            full_path.push(elem);
                        }
                        paths.push(full_path);
                    }
                }
            }

            _ => {
                // For other expressions, just use current path
                // This handles things like literals, etc.
                paths.push(current_path);
            }
        }
    }

    /// Apply an update to each element via an iterator (e.g., .[] |= f)
    /// This handles arrays and objects, applying f to each element/value
    /// If f returns empty for an element, that element is removed
    fn apply_update_to_iterator(
        &mut self,
        input: Jv,
        iter_base: &Expr,
        filter: &Expr,
        ctx: Rc<RefCell<Context>>,
    ) -> EvalResult {
        match self.apply_update_to_iterator_sync(input, iter_base, filter, ctx) {
            Ok(v) => Box::new(std::iter::once(Ok(v))),
            Err(e) => Box::new(std::iter::once(Err(e))),
        }
    }

    /// Synchronous version of apply_update_to_iterator
    fn apply_update_to_iterator_sync(
        &mut self,
        input: Jv,
        iter_base: &Expr,
        filter: &Expr,
        ctx: Rc<RefCell<Context>>,
    ) -> Result<Jv, String> {
        // Get the container to iterate over
        let container = if let ExprKind::Identity = iter_base.kind {
            input.clone()
        } else {
            match self.eval_expr(iter_base, input.clone(), ctx.clone()).next() {
                Some(Ok(v)) => v,
                Some(Err(e)) => return Err(e),
                None => return Err("iterator base produced no value".to_string()),
            }
        };

        match container {
            Jv::Array(arr) => {
                // Apply filter to each element, keeping only non-empty results
                let mut result = Vec::new();
                for elem in arr.iter() {
                    let mut filter_interp = Interpreter { ctx: ctx.clone() };
                    let filter_result = filter_interp.eval_expr(filter, elem, ctx.clone()).next();
                    match filter_result {
                        Some(Ok(v)) => result.push(v),
                        Some(Err(e)) => return Err(e),
                        None => {
                            // Filter returned empty, skip this element
                        }
                    }
                }
                let new_container = Jv::from_vec(result);

                // If base is identity, return the new container directly
                if let ExprKind::Identity = iter_base.kind {
                    Ok(new_container)
                } else {
                    // Set the new container back at the base path
                    let mut path_parts: Vec<Jv> = Vec::new();
                    Self::apply_assignment(input, iter_base, new_container, &mut path_parts, ctx)
                }
            }
            Jv::Object(obj) => {
                // Apply filter to each value
                let mut result = JvObject::new();
                for (key, val) in obj.iter() {
                    let mut filter_interp = Interpreter { ctx: ctx.clone() };
                    let filter_result = filter_interp.eval_expr(filter, val, ctx.clone()).next();
                    match filter_result {
                        Some(Ok(v)) => result.set(&key, v),
                        Some(Err(e)) => return Err(e),
                        None => {
                            // Filter returned empty, skip this key
                        }
                    }
                }
                let new_container = Jv::Object(result);

                // If base is identity, return the new container directly
                if let ExprKind::Identity = iter_base.kind {
                    Ok(new_container)
                } else {
                    // Set the new container back at the base path
                    let mut path_parts: Vec<Jv> = Vec::new();
                    Self::apply_assignment(input, iter_base, new_container, &mut path_parts, ctx)
                }
            }
            _ => Err(format!("Cannot iterate over {}", container.type_name())),
        }
    }

    /// Check if an expression kind contains a Comma (generator)
    fn contains_comma(kind: &ExprKind) -> bool {
        match kind {
            ExprKind::Comma(_, _) => true,
            ExprKind::Paren(inner) => Self::contains_comma(&inner.kind),
            _ => false,
        }
    }

    /// Check if an expression is a path access (field or index)
    /// Used to determine if a null result means "path doesn't exist"
    fn is_path_access(kind: &ExprKind) -> bool {
        match kind {
            ExprKind::Field(_) => true,
            ExprKind::Index { .. } => true,
            ExprKind::Slice { .. } => true,
            _ => false,
        }
    }

    /// Apply update to recursive descent: .. |= f or (.. | filter) |= f or (.. | filter | path) |= f
    fn apply_update_recursive_descent(
        &mut self,
        input: Jv,
        value_expr: &Expr,
        filter: Option<&Expr>,
        path_expr: Option<&Expr>,
        ctx: Rc<RefCell<Context>>,
    ) -> EvalResult {
        // Collect all paths using path(..)
        let mut all_paths: Vec<Vec<Jv>> = Vec::new();
        self.collect_recursive_paths(&input, &mut all_paths, Vec::new());

        // Sort paths by length descending (deepest first)
        // This ensures we update children before parents
        all_paths.sort_by(|a, b| b.len().cmp(&a.len()));

        // Apply updates
        let mut result = input;
        for path in all_paths {
            // Get value at this path
            let value_at_path = {
                let mut current = result.clone();
                let mut valid = true;
                for key in &path {
                    current = current.index(key);
                    if matches!(current, Jv::Invalid(_)) {
                        valid = false;
                        break;
                    }
                }
                if !valid {
                    continue; // Path no longer valid after earlier update
                }
                current
            };

            // If there's a filter, check if value passes
            if let Some(filter_expr) = filter {
                let mut filter_interp = Interpreter { ctx: ctx.clone() };
                let filter_result = filter_interp.eval_expr(filter_expr, value_at_path.clone(), ctx.clone()).next();

                match filter_result {
                    Some(Ok(_)) => {
                        // Filter passed - continue to path expression or update
                    }
                    Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
                    None => {
                        // Filter produced no output - skip this path
                        continue;
                    }
                }
            }

            // If there's a path expression (like .b), evaluate it and extend the path
            let (update_path, update_value) = if let Some(pe) = path_expr {
                // Get the path components from evaluating path(path_expr)
                let mut path_interp = Interpreter { ctx: ctx.clone() };

                // First evaluate path_expr to get the value we're updating
                let value_to_update = match path_interp.eval_expr(pe, value_at_path.clone(), ctx.clone()).next() {
                    Some(Ok(v)) => v,
                    Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
                    None => continue, // Path produced no output
                };

                // Now we need to figure out what path components the path_expr adds
                // For simple cases like .b, we can extract the field name
                let extended_path = match &pe.kind {
                    ExprKind::Field(name) => {
                        let mut p = path.clone();
                        p.push(Jv::string(name));
                        p
                    }
                    ExprKind::Index { expr: base, index, .. } if matches!(base.kind, ExprKind::Identity) => {
                        // .[index] - evaluate index and add to path
                        let mut idx_interp = Interpreter { ctx: ctx.clone() };
                        match idx_interp.eval_expr(index, value_at_path.clone(), ctx.clone()).next() {
                            Some(Ok(idx_val)) => {
                                let mut p = path.clone();
                                p.push(idx_val);
                                p
                            }
                            _ => continue,
                        }
                    }
                    _ => {
                        // For more complex expressions, we can't easily determine the path
                        // Fall back to not extending
                        continue;
                    }
                };

                (extended_path, value_to_update)
            } else {
                (path.clone(), value_at_path)
            };

            // Apply value expression
            let mut val_interp = Interpreter { ctx: ctx.clone() };
            let new_value = match val_interp.eval_expr(value_expr, update_value, ctx.clone()).next() {
                Some(Ok(v)) => v,
                Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
                None => continue, // Empty update - skip
            };

            // Set the new value at path
            let path_vec: Vec<_> = update_path.iter().collect();
            result = match Self::setpath_at(&result, &path_vec, new_value) {
                Ok(v) => v,
                Err(_) => continue, // Ignore errors (structure may have changed)
            };
        }

        Box::new(std::iter::once(Ok(result)))
    }

    /// Helper to set a value at a path
    fn setpath_at(current: &Jv, path: &[&Jv], value: Jv) -> Result<Jv, String> {
        if path.is_empty() {
            return Ok(value);
        }
        let key = path[0];
        let rest = &path[1..];

        match key {
            Jv::String(s) => {
                let mut obj = match current {
                    Jv::Object(o) => o.clone(),
                    Jv::Null => crate::jv::JvObject::new(),
                    _ => return Err("cannot index non-object with string".to_string()),
                };
                let child = obj.get(s.as_str()).unwrap_or(Jv::Null);
                let new_child = Self::setpath_at(&child, rest, value)?;
                obj.set(s.as_str(), new_child);
                Ok(Jv::Object(obj))
            }
            Jv::Number(n) => {
                if let Some(idx) = n.as_i64() {
                    let mut arr = match current {
                        Jv::Array(a) => a.clone(),
                        Jv::Null => crate::jv::JvArray::new(),
                        _ => return Err(format!("Cannot index {} with number ({})", current.type_name(), idx)),
                    };
                    let normalized_idx = if idx < 0 { arr.len() as i64 + idx } else { idx };
                    let child = arr.get(normalized_idx).unwrap_or(Jv::Null);
                    let new_child = Self::setpath_at(&child, rest, value)?;
                    arr.set(normalized_idx, new_child)?;
                    Ok(Jv::Array(arr))
                } else {
                    Err("array index must be integer".to_string())
                }
            }
            _ => Err("path element must be string or number".to_string()),
        }
    }

    /// Evaluate truncate_stream(stream_generator)
    fn eval_truncate_stream(&mut self, stream_expr: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        // input is the depth
        let depth = match &input {
            Jv::Number(n) => n.as_i64().unwrap_or(0) as usize,
            _ => return Box::new(std::iter::once(Err(format!("truncate_stream depth must be number, got {}", input.type_name())))),
        };

        // Evaluate the stream expression to get stream items
        let stream_items: Vec<_> = self.eval_expr(stream_expr, Jv::Null, ctx.clone()).collect();

        let results: Vec<_> = stream_items.into_iter().filter_map(|item| {
            match item {
                Ok(Jv::Array(arr)) if !arr.is_empty() => {
                    // Get the path (first element)
                    let path = match arr.get(0) {
                        Some(Jv::Array(p)) => p,
                        _ => return None,
                    };

                    // Skip if path is too short to truncate
                    if path.len() <= depth {
                        return None;
                    }

                    // Truncate the path
                    let truncated_path: Vec<Jv> = path.iter().skip(depth).collect();

                    if arr.len() == 1 {
                        // End marker
                        Some(Ok(Jv::from_vec(vec![Jv::from_vec(truncated_path)])))
                    } else {
                        // Value entry
                        let value = arr.get(1).unwrap_or(Jv::Null);
                        Some(Ok(Jv::from_vec(vec![Jv::from_vec(truncated_path), value])))
                    }
                }
                Err(e) => Some(Err(e)),
                _ => None,
            }
        }).collect();

        Box::new(results.into_iter())
    }

    /// Evaluate fromstream(stream_generator)
    fn eval_fromstream(&mut self, stream_expr: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        // Evaluate the stream expression to get stream items
        let stream_items: Vec<_> = self.eval_expr(stream_expr, input, ctx.clone()).collect();

        let mut result = Jv::Null;

        for item in stream_items {
            match item {
                Ok(Jv::Array(arr)) if arr.len() >= 2 => {
                    // [path, value] - set the value at path
                    let path = match arr.get(0) {
                        Some(Jv::Array(p)) => p.iter().collect::<Vec<_>>(),
                        _ => continue,
                    };
                    let value = arr.get(1).unwrap_or(Jv::Null);

                    // Set value at path
                    result = Self::setpath_at(&result, &path.iter().collect::<Vec<_>>(), value).unwrap_or(result);
                }
                Ok(Jv::Array(_)) => {
                    // [path] - end marker, ignore
                }
                Err(e) => return Box::new(std::iter::once(Err(e))),
                _ => {}
            }
        }

        Box::new(std::iter::once(Ok(result)))
    }

    /// Recursively collect all paths in a value
    fn collect_recursive_paths(&self, value: &Jv, paths: &mut Vec<Vec<Jv>>, current_path: Vec<Jv>) {
        // Add current path (including empty path for root)
        paths.push(current_path.clone());

        match value {
            Jv::Array(arr) => {
                for (i, elem) in arr.iter().enumerate() {
                    let mut child_path = current_path.clone();
                    child_path.push(Jv::from_i64(i as i64));
                    self.collect_recursive_paths(&elem, paths, child_path);
                }
            }
            Jv::Object(obj) => {
                for (key, val) in obj.iter() {
                    let mut child_path = current_path.clone();
                    child_path.push(Jv::string(key));
                    self.collect_recursive_paths(&val, paths, child_path);
                }
            }
            _ => {
                // Scalars have no children
            }
        }
    }

    /// Apply update to indexed generator like .foo[1,4,2,3] |= empty
    fn apply_update_to_indexed_generator(
        &mut self,
        input: Jv,
        base_expr: &Expr,
        idx_expr: &Expr,
        value_expr: &Expr,
        ctx: Rc<RefCell<Context>>,
    ) -> EvalResult {
        // Get the base container
        let container = match self.eval_expr(base_expr, input.clone(), ctx.clone()).next() {
            Some(Ok(v)) => v,
            Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
            None => return Box::new(std::iter::once(Err("base expression produced no value".to_string()))),
        };

        // Evaluate all indices from the generator
        let indices: Vec<i64> = self.eval_expr(idx_expr, input.clone(), ctx.clone())
            .filter_map(|r| r.ok())
            .filter_map(|v| v.as_i64())
            .collect();

        match container {
            Jv::Array(arr) => {
                let len = arr.len() as i64;
                let mut result = Vec::with_capacity(arr.len());
                let mut indices_to_delete = std::collections::HashSet::new();

                // Normalize indices and check which ones should be deleted
                for idx in indices {
                    let normalized = if idx < 0 { len + idx } else { idx };
                    if normalized >= 0 && normalized < len {
                        let normalized_usize = normalized as usize;
                        let elem = arr.get(idx).unwrap_or(Jv::Null);

                        // Apply the value expression to the element
                        let mut val_interp = Interpreter { ctx: ctx.clone() };
                        let update_result = val_interp.eval_expr(value_expr, elem, ctx.clone()).next();

                        match update_result {
                            Some(Ok(_)) => {
                                // Value expression returned something - keep with update
                                // But for now, we're only handling deletion (empty)
                            }
                            Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
                            None => {
                                // Value expression returned empty - mark for deletion
                                indices_to_delete.insert(normalized_usize);
                            }
                        }
                    }
                }

                // Build result array, skipping deleted indices
                for (i, elem) in arr.iter().enumerate() {
                    if !indices_to_delete.contains(&i) {
                        result.push(elem);
                    }
                }

                let new_container = Jv::from_vec(result);

                // Set the result back at the base path
                let mut path_parts: Vec<Jv> = Vec::new();
                match Self::apply_assignment(input, base_expr, new_container, &mut path_parts, ctx) {
                    Ok(v) => Box::new(std::iter::once(Ok(v))),
                    Err(e) => Box::new(std::iter::once(Err(e))),
                }
            }
            _ => Box::new(std::iter::once(Err(format!("Cannot index {} with generators", container.type_name())))),
        }
    }

    /// Apply an update operator (+= etc) to each element via an iterator (e.g., .[] += 2)
    fn apply_updateop_to_iterator(
        &mut self,
        input: Jv,
        iter_base: &Expr,
        value_expr: &Expr,
        op: BinaryOp,
        ctx: Rc<RefCell<Context>>,
    ) -> EvalResult {
        // Get the container to iterate over
        let container = if let ExprKind::Identity = iter_base.kind {
            input.clone()
        } else {
            match self.eval_expr(iter_base, input.clone(), ctx.clone()).next() {
                Some(Ok(v)) => v,
                Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
                None => return Box::new(std::iter::once(Err("iterator base produced no value".to_string()))),
            }
        };

        // Evaluate the right-hand value once
        let right_val = match self.eval_expr(value_expr, input.clone(), ctx.clone()).next() {
            Some(Ok(v)) => v,
            Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
            None => return Box::new(std::iter::empty()),
        };

        match container {
            Jv::Array(arr) => {
                // Apply operation to each element
                let mut result = Vec::new();
                for elem in arr.iter() {
                    match eval_binary_op(op, &elem, &right_val) {
                        Ok(v) => result.push(v),
                        Err(e) => return Box::new(std::iter::once(Err(e))),
                    }
                }
                let new_container = Jv::from_vec(result);

                // If base is identity, return the new container directly
                if let ExprKind::Identity = iter_base.kind {
                    Box::new(std::iter::once(Ok(new_container)))
                } else {
                    // Set the new container back at the base path
                    let mut path_parts: Vec<Jv> = Vec::new();
                    match Self::apply_assignment(input, iter_base, new_container, &mut path_parts, ctx) {
                        Ok(v) => Box::new(std::iter::once(Ok(v))),
                        Err(e) => Box::new(std::iter::once(Err(e))),
                    }
                }
            }
            Jv::Object(obj) => {
                // Apply operation to each value
                let mut result = JvObject::new();
                for (key, val) in obj.iter() {
                    match eval_binary_op(op, &val, &right_val) {
                        Ok(v) => result.set(&key, v),
                        Err(e) => return Box::new(std::iter::once(Err(e))),
                    }
                }
                let new_container = Jv::Object(result);

                // If base is identity, return the new container directly
                if let ExprKind::Identity = iter_base.kind {
                    Box::new(std::iter::once(Ok(new_container)))
                } else {
                    // Set the new container back at the base path
                    let mut path_parts: Vec<Jv> = Vec::new();
                    match Self::apply_assignment(input, iter_base, new_container, &mut path_parts, ctx) {
                        Ok(v) => Box::new(std::iter::once(Ok(v))),
                        Err(e) => Box::new(std::iter::once(Err(e))),
                    }
                }
            }
            _ => Box::new(std::iter::once(Err(format!("Cannot iterate over {}", container.type_name())))),
        }
    }

    /// Apply an assignment by traversing the target path and setting the value
    fn apply_assignment(
        current: Jv,
        target: &Expr,
        value: Jv,
        _path: &mut Vec<Jv>,
        ctx: Rc<RefCell<Context>>,
    ) -> Result<Jv, String> {
        use crate::jv::JvArray;

        match &target.kind {
            ExprKind::Identity => {
                // Direct assignment to input
                Ok(value)
            }
            ExprKind::Field(name) => {
                // .foo = value
                match current {
                    Jv::Object(mut obj) => {
                        obj.set(name, value);
                        Ok(Jv::Object(obj))
                    }
                    Jv::Null => {
                        let mut obj = JvObject::new();
                        obj.set(name, value);
                        Ok(Jv::Object(obj))
                    }
                    _ => Err(format!("Cannot index {} with string \"{}\"", current.type_name(), name)),
                }
            }
            ExprKind::Index { expr: base, index, optional: _ } => {
                // For nested assignments like .foo.bar = value or .foo[0] = value
                // We need to:
                // 1. Get the current value at base
                // 2. Modify it with the assignment
                // 3. Set the modified value back

                // Evaluate the index - collect all values for comma expressions
                let mut idx_interp = Interpreter { ctx: ctx.clone() };
                let idx_results: Vec<_> = idx_interp.eval_expr(index, current.clone(), ctx.clone()).collect();

                if idx_results.is_empty() {
                    return Err("index evaluation produced no value".to_string());
                }

                match &base.kind {
                    ExprKind::Identity => {
                        // Direct index on input: .[idx] = value
                        // If multiple indices (comma expression), apply assignment to each
                        let mut result = current.clone();
                        for idx_result in idx_results {
                            let idx_val = match idx_result {
                                Ok(v) => v,
                                Err(e) => return Err(e),
                            };

                            match &idx_val {
                                Jv::String(s) => {
                                    match result {
                                        Jv::Object(mut obj) => {
                                            obj.set(s.as_str(), value.clone());
                                            result = Jv::Object(obj);
                                        }
                                        Jv::Null => {
                                            let mut obj = JvObject::new();
                                            obj.set(s.as_str(), value.clone());
                                            result = Jv::Object(obj);
                                        }
                                        _ => return Err(format!("Cannot index {} with string", result.type_name())),
                                    }
                                }
                                Jv::Number(n) => {
                                    // NaN index in assignment is an error
                                    if n.is_nan() {
                                        return Err("Cannot set array element at NaN index".to_string());
                                    }
                                    // jq truncates float indices using floor
                                    let idx = if let Some(i) = n.as_i64() {
                                        i
                                    } else {
                                        n.as_f64().floor() as i64
                                    };
                                    match result {
                                        Jv::Array(mut arr) => {
                                            let len = arr.len() as i64;
                                            let actual_idx = if idx < 0 { len + idx } else { idx };
                                            if actual_idx < 0 {
                                                return Err("Out of bounds negative array index".to_string());
                                            }
                                            arr.set(actual_idx, value.clone())?;
                                            result = Jv::Array(arr);
                                        }
                                        Jv::Null => {
                                            if idx < 0 {
                                                return Err("Out of bounds negative array index".to_string());
                                            }
                                            let mut arr = JvArray::new();
                                            arr.set(idx, value.clone())?;
                                            result = Jv::Array(arr);
                                        }
                                        _ => return Err(format!("Cannot index {} with number ({})", result.type_name(), idx)),
                                    }
                                }
                                _ => return Err(format!("Cannot use {} as index", idx_val.type_name())),
                            }
                        }
                        Ok(result)
                    }
                    _ => {
                        // Nested: get base value, apply assignment, set back
                        // For comma expressions, apply all assignments sequentially
                        let mut result = current.clone();
                        for idx_result in idx_results {
                            let idx_val = match idx_result {
                                Ok(v) => v,
                                Err(e) => return Err(e),
                            };

                            let mut base_interp = Interpreter { ctx: ctx.clone() };
                            let base_val = match base_interp.eval_expr(base, result.clone(), ctx.clone()).next() {
                                Some(Ok(v)) => v,
                                Some(Err(e)) => return Err(e),
                                None => Jv::Null,
                            };

                            // Create a literal index expression for this specific index
                            let idx_literal = match &idx_val {
                                Jv::Number(n) => Expr::new(ExprKind::Literal(Literal::Number(n.as_f64())), target.span),
                                Jv::String(s) => Expr::new(ExprKind::Literal(Literal::String(s.as_str().to_string())), target.span),
                                _ => return Err(format!("Cannot use {} as index", idx_val.type_name())),
                            };

                            // Apply inner assignment
                            let inner_target = Expr::new(
                                ExprKind::Index {
                                    expr: Box::new(Expr::new(ExprKind::Identity, target.span)),
                                    index: Box::new(idx_literal),
                                    optional: false,
                                },
                                target.span,
                            );
                            let modified_base = Self::apply_assignment(base_val, &inner_target, value.clone(), _path, ctx.clone())?;

                            // Now set modified base back to parent
                            result = Self::apply_assignment(result, base, modified_base, _path, ctx.clone())?;
                        }
                        Ok(result)
                    }
                }
            }
            ExprKind::Iterator { expr: base, optional: _ } => {
                // .[] = value - set all elements to value
                match &base.kind {
                    ExprKind::Identity => {
                        // Direct iterator on input: .[] = value
                        match current {
                            Jv::Array(arr) => {
                                // Replace all elements with value
                                let new_arr: Vec<Jv> = (0..arr.len()).map(|_| value.clone()).collect();
                                Ok(Jv::from_vec(new_arr))
                            }
                            Jv::Object(obj) => {
                                // Replace all values with value
                                let mut new_obj = JvObject::new();
                                for (k, _) in obj.iter() {
                                    new_obj.set(&k, value.clone());
                                }
                                Ok(Jv::Object(new_obj))
                            }
                            Jv::Null => {
                                // .[] = v on null is null (no elements to update)
                                Ok(Jv::Null)
                            }
                            _ => Err(format!("Cannot iterate over {}", current.type_name())),
                        }
                    }
                    _ => {
                        // Check if base is a non-path function call
                        if let ExprKind::FunctionCall { name, .. } = &base.kind {
                            if !matches!(name.as_str(), "getpath" | "first" | "last") {
                                // Evaluate to get the result value for the error message
                                let mut base_interp = Interpreter { ctx: ctx.clone() };
                                if let Some(Ok(result)) = base_interp.eval_expr(base, current.clone(), ctx.clone()).next() {
                                    use crate::jv::print_jv;
                                    let formatted = print_jv(&result);
                                    return Err(format!("Invalid path expression near attempt to iterate through {}", formatted));
                                }
                            }
                        }

                        // Nested: get base value, apply iterator assignment, set back
                        let mut base_interp = Interpreter { ctx: ctx.clone() };
                        let base_val = match base_interp.eval_expr(base, current.clone(), ctx.clone()).next() {
                            Some(Ok(v)) => v,
                            Some(Err(e)) => return Err(e),
                            None => Jv::Null,
                        };

                        // Apply inner assignment
                        let inner_target = Expr::new(
                            ExprKind::Iterator {
                                expr: Box::new(Expr::new(ExprKind::Identity, target.span)),
                                optional: false,
                            },
                            target.span,
                        );
                        let modified_base = Self::apply_assignment(base_val, &inner_target, value, _path, ctx.clone())?;

                        // Now set modified base back to parent
                        Self::apply_assignment(current, base, modified_base, _path, ctx)
                    }
                }
            }
            ExprKind::Slice { expr: base, start, end, optional: _ } => {
                // .[start:end] = value - slice assignment
                let mut interp = Interpreter { ctx: ctx.clone() };

                // Helper to convert number to i64, truncating floats (floor for start, ceil for end)
                fn number_to_start(n: &crate::jv::JvNumber) -> i64 {
                    if let Some(i) = n.as_i64() {
                        i
                    } else {
                        n.as_f64().floor() as i64
                    }
                }
                fn number_to_end(n: &crate::jv::JvNumber) -> Option<i64> {
                    if n.is_nan() {
                        None
                    } else if let Some(i) = n.as_i64() {
                        Some(i)
                    } else {
                        Some(n.as_f64().ceil() as i64)
                    }
                }

                // Evaluate start and end indices
                let start_val = if let Some(start_expr) = start {
                    match interp.eval_expr(start_expr, current.clone(), ctx.clone()).next() {
                        Some(Ok(Jv::Number(n))) => number_to_start(&n),
                        Some(Err(e)) => return Err(e),
                        _ => 0,
                    }
                } else {
                    0
                };

                let end_val = if let Some(end_expr) = end {
                    match interp.eval_expr(end_expr, current.clone(), ctx.clone()).next() {
                        Some(Ok(Jv::Number(n))) => number_to_end(&n),
                        Some(Err(e)) => return Err(e),
                        _ => None,
                    }
                } else {
                    None
                };

                match &base.kind {
                    ExprKind::Identity => {
                        // Direct slice on input: .[start:end] = value
                        match current {
                            Jv::Array(arr) => {
                                let len = arr.len();
                                // Handle negative indices
                                let start_idx = if start_val < 0 {
                                    (len as i64 + start_val).max(0) as usize
                                } else {
                                    (start_val as usize).min(len)
                                };
                                let end_idx = match end_val {
                                    Some(e) if e < 0 => (len as i64 + e).max(0) as usize,
                                    Some(e) => (e as usize).min(len),
                                    None => len,
                                };

                                // Build new array: elements before slice + value elements + elements after slice
                                let mut result = Vec::new();
                                for i in 0..start_idx.min(len) {
                                    result.push(arr.get(i as i64).unwrap_or(Jv::Null));
                                }

                                // Insert value elements (if value is an array) or value itself
                                match &value {
                                    Jv::Array(val_arr) => {
                                        for item in val_arr.iter() {
                                            result.push(item);
                                        }
                                    }
                                    _ => result.push(value),
                                }

                                // Add elements after the slice
                                for i in end_idx..len {
                                    result.push(arr.get(i as i64).unwrap_or(Jv::Null));
                                }

                                Ok(Jv::from_vec(result))
                            }
                            Jv::String(_) => {
                                // jq does not support string slice assignment
                                Err("Cannot update string slices".to_string())
                            }
                            _ => Err(format!("Cannot slice {}", current.type_name())),
                        }
                    }
                    _ => {
                        // Nested slice: get base value, modify, set back
                        let mut base_interp = Interpreter { ctx: ctx.clone() };
                        let base_val = match base_interp.eval_expr(base, current.clone(), ctx.clone()).next() {
                            Some(Ok(v)) => v,
                            Some(Err(e)) => return Err(e),
                            None => return Err("base expression produced no value".to_string()),
                        };

                        // Create a slice target for the inner assignment
                        let inner_target = Expr {
                            kind: ExprKind::Slice {
                                expr: Box::new(Expr {
                                    kind: ExprKind::Identity,
                                    span: target.span,
                                }),
                                start: start.clone(),
                                end: end.clone(),
                                optional: false,
                            },
                            span: target.span,
                        };
                        let modified_base = Self::apply_assignment(base_val, &inner_target, value, _path, ctx.clone())?;
                        Self::apply_assignment(current, base, modified_base, _path, ctx)
                    }
                }
            }
            ExprKind::Pipe(left, right) => {
                // For piped paths like .foo | .bar = value
                let mut left_interp = Interpreter { ctx: ctx.clone() };
                let left_val = match left_interp.eval_expr(left, current.clone(), ctx.clone()).next() {
                    Some(Ok(v)) => v,
                    Some(Err(e)) => return Err(e),
                    None => return Err("pipe left side produced no value".to_string()),
                };

                let modified = Self::apply_assignment(left_val, right, value, _path, ctx.clone())?;
                Self::apply_assignment(current, left, modified, _path, ctx)
            }
            ExprKind::Paren(inner) => {
                // Unwrap parentheses
                Self::apply_assignment(current, inner, value, _path, ctx)
            }
            ExprKind::Binding { expr: bind_expr, pattern, body } => {
                // For (.a as $x | .b) = value
                // 1. Evaluate bind_expr to get binding value
                // 2. Create context with binding
                // 3. Apply assignment to body
                let mut bind_interp = Interpreter { ctx: ctx.clone() };
                let bind_val = match bind_interp.eval_expr(bind_expr, current.clone(), ctx.clone()).next() {
                    Some(Ok(v)) => v,
                    Some(Err(e)) => return Err(e),
                    None => return Err("binding expression produced no value".to_string()),
                };

                // Create child context with binding
                let child_ctx = Rc::new(RefCell::new(Context::child(ctx.clone())));
                let mut bind_inner = Interpreter { ctx: child_ctx.clone() };
                bind_inner.bind_pattern(pattern, &bind_val, &child_ctx)?;

                // Apply assignment to body in child context
                Self::apply_assignment(current, body, value, _path, child_ctx)
            }
            ExprKind::FunctionCall { name, args, .. } if name == "getpath" && args.len() == 1 => {
                // getpath(path) = value is equivalent to setpath(path; value)
                let mut interp = Interpreter { ctx: ctx.clone() };
                let path_val = match interp.eval_expr(&args[0], current.clone(), ctx.clone()).next() {
                    Some(Ok(v)) => v,
                    Some(Err(e)) => return Err(e),
                    None => return Err("getpath argument produced no value".to_string()),
                };

                // path_val should be an array of path components
                let path_arr = match &path_val {
                    Jv::Array(arr) => arr,
                    _ => return Err("Path must be specified as an array".to_string()),
                };

                // Build the result by setting the path
                let mut result = current;
                let components: Vec<Jv> = path_arr.iter().collect();

                fn set_path(current: Jv, path: &[Jv], value: Jv) -> Result<Jv, String> {
                    use crate::jv::{JvObject, JvArray};

                    if path.is_empty() {
                        return Ok(value);
                    }

                    let key = &path[0];
                    let rest = &path[1..];

                    match key {
                        Jv::String(s) => {
                            // Object key
                            let mut obj = match current {
                                Jv::Object(o) => o,
                                Jv::Null => JvObject::new(),
                                _ => return Err(format!("Cannot index {} with string \"{}\"", current.type_name(), s.as_str())),
                            };
                            let child = obj.get(s.as_str()).unwrap_or(Jv::Null);
                            let new_child = set_path(child, rest, value)?;
                            obj.set(s.as_str(), new_child);
                            Ok(Jv::Object(obj))
                        }
                        Jv::Number(n) => {
                            // Array index - jq truncates float indices using floor
                            let idx = if let Some(i) = n.as_i64() {
                                i
                            } else {
                                n.as_f64().floor() as i64
                            };
                            let mut arr = match current {
                                Jv::Array(a) => a,
                                Jv::Null => JvArray::new(),
                                _ => return Err(format!("Cannot index {} with number ({})", current.type_name(), idx)),
                            };
                            let child = arr.get(idx).unwrap_or(Jv::Null);
                            let new_child = set_path(child, rest, value)?;
                            arr.set(idx, new_child).map_err(|e| e)?;
                            Ok(Jv::Array(arr))
                        }
                        _ => Err(format!("Cannot index with {}", key.type_name())),
                    }
                }

                result = set_path(result, &components, value)?;
                Ok(result)
            }
            ExprKind::FunctionCall { name, args, .. } => {
                // Check if this is a call to a 0-arity function that's actually an expression binding
                // This handles cases like: def x: .[0]; x = 10
                // or filter parameters like: def inc(x): x |= .+1; inc(.foo)
                if args.is_empty() {
                    // Look up the function - it might be a 0-arity user-defined function
                    // that's really just an expression binding (filter parameter)
                    let func_key = format!("{}/0", name);
                    // Clone the binding data to avoid borrow issues during recursion
                    let func_binding = {
                        let borrowed = ctx.borrow();
                        borrowed.lookup(&func_key)
                    };
                    if let Some(Binding::FilterClosure { def, ctx: closure_ctx }) = func_binding {
                        // This is a user-defined function - evaluate its body as the target
                        // The body expression becomes the path we're assigning to
                        return Self::apply_assignment(current, &def.body, value, _path, closure_ctx);
                    }
                    // Also check for direct expression bindings (filter parameters)
                    let expr_binding = {
                        let borrowed = ctx.borrow();
                        borrowed.lookup_expr_with_context(name)
                    };
                    if let Some((expr, expr_ctx)) = expr_binding {
                        return Self::apply_assignment(current, &expr, value, _path, expr_ctx);
                    }
                }

                // For any other function call, evaluate it and produce an error with the result
                // This handles both built-in functions like reverse, sort, etc.
                // and user-defined functions that don't produce valid paths
                if !matches!(name.as_str(), "getpath" | "first" | "last") {
                    let mut interp = Interpreter { ctx: ctx.clone() };
                    if let Some(Ok(result)) = interp.eval_expr(target, current.clone(), ctx.clone()).next() {
                        use crate::jv::print_jv;
                        let formatted = print_jv(&result);
                        return Err(format!("Invalid path expression with result {}", formatted));
                    }
                }
                Err(format!("Cannot assign to expression: {:?}", target.kind))
            }
            ExprKind::Comma(left, right) => {
                // (.a, .b) = value assigns value to both paths
                let result = Self::apply_assignment(current, left, value.clone(), _path, ctx.clone())?;
                Self::apply_assignment(result, right, value, _path, ctx)
            }
            _ => Err(format!("Cannot assign to expression: {:?}", target.kind)),
        }
    }

    /// Bind values to a pattern, returning error if pattern doesn't match
    fn bind_pattern(&mut self, pattern: &Pattern, value: &Jv, ctx: &Rc<RefCell<Context>>) -> Result<(), String> {
        match &pattern.kind {
            PatternKind::Binding(name) => {
                ctx.borrow_mut().bind_value(name, value.clone());
                Ok(())
            }
            PatternKind::BoundPattern { name, pattern: sub_pattern } => {
                // Bind the name to the value
                ctx.borrow_mut().bind_value(name, value.clone());
                // Also apply the sub-pattern
                self.bind_pattern(sub_pattern, value, ctx)
            }
            PatternKind::Array(patterns) => {
                // Value must be an array
                let arr = match value {
                    Jv::Array(a) => a,
                    _ => return Err(format!("Cannot bind {} to array pattern", value.type_name())),
                };

                // Bind each element to corresponding pattern
                for (i, pat) in patterns.iter().enumerate() {
                    let elem = arr.get(i as i64).unwrap_or(Jv::Null);
                    self.bind_pattern(pat, &elem, ctx)?;
                }
                Ok(())
            }
            PatternKind::Object(entries) => {
                // Value must be an object
                let obj = match value {
                    Jv::Object(o) => o,
                    _ => return Err(format!("Cannot bind {} to object pattern", value.type_name())),
                };

                // Bind each key's value to corresponding pattern
                for (key, pat) in entries {
                    let key_str = match key {
                        ObjectKey::Ident(s) | ObjectKey::String(s) => s.clone(),
                        ObjectKey::Shorthand(s) => s.clone(),
                        ObjectKey::Expr(key_expr) => {
                            // Evaluate expression key with value as input
                            let mut results = self.eval_expr(key_expr, value.clone(), ctx.clone());
                            match results.next() {
                                Some(Ok(Jv::String(s))) => s.as_str().to_string(),
                                Some(Ok(other)) => {
                                    return Err(format!("Cannot use {} as object key", other.type_name()));
                                }
                                Some(Err(e)) => return Err(e),
                                None => return Err("Expression key produced no value".to_string()),
                            }
                        }
                    };

                    let elem = obj.get(&key_str).unwrap_or(Jv::Null);
                    self.bind_pattern(pat, &elem, ctx)?;
                }
                Ok(())
            }
            PatternKind::Alternative(first, second) => {
                // Collect all variable names from both patterns
                let mut all_vars = std::collections::HashSet::new();
                Self::collect_pattern_vars(first, &mut all_vars);
                Self::collect_pattern_vars(second, &mut all_vars);

                // Pre-bind all variables to null
                for var in &all_vars {
                    ctx.borrow_mut().bind_value(var, Jv::Null);
                }

                // Try first pattern - collect bindings without committing
                let bindings = self.try_bind_pattern(first, value, ctx);
                if let Ok(binds) = bindings {
                    // Success - commit bindings (overwrite the nulls)
                    for (name, val) in binds {
                        ctx.borrow_mut().bind_value(&name, val);
                    }
                    Ok(())
                } else {
                    // First failed, try second - this will overwrite the nulls
                    self.bind_pattern(second, value, ctx)
                }
            }
        }
    }

    /// Try to bind a pattern, collecting bindings without modifying context
    /// Returns collected bindings on success, error on failure
    fn try_bind_pattern(&mut self, pattern: &Pattern, value: &Jv, ctx: &Rc<RefCell<Context>>) -> Result<Vec<(String, Jv)>, String> {
        let mut bindings = Vec::new();
        self.collect_bindings(pattern, value, ctx, &mut bindings)?;
        Ok(bindings)
    }

    /// Recursively collect bindings from a pattern without modifying context
    fn collect_bindings(&mut self, pattern: &Pattern, value: &Jv, ctx: &Rc<RefCell<Context>>, bindings: &mut Vec<(String, Jv)>) -> Result<(), String> {
        match &pattern.kind {
            PatternKind::Binding(name) => {
                bindings.push((name.clone(), value.clone()));
                Ok(())
            }
            PatternKind::BoundPattern { name, pattern: sub_pattern } => {
                bindings.push((name.clone(), value.clone()));
                self.collect_bindings(sub_pattern, value, ctx, bindings)
            }
            PatternKind::Array(patterns) => {
                let arr = match value {
                    Jv::Array(a) => a,
                    _ => return Err(format!("Cannot bind {} to array pattern", value.type_name())),
                };
                for (i, pat) in patterns.iter().enumerate() {
                    let elem = arr.get(i as i64).unwrap_or(Jv::Null);
                    self.collect_bindings(pat, &elem, ctx, bindings)?;
                }
                Ok(())
            }
            PatternKind::Object(entries) => {
                let obj = match value {
                    Jv::Object(o) => o,
                    _ => return Err(format!("Cannot bind {} to object pattern", value.type_name())),
                };
                for (key, pat) in entries {
                    let key_str = match key {
                        ObjectKey::Ident(s) | ObjectKey::String(s) => s.clone(),
                        ObjectKey::Shorthand(s) => s.clone(),
                        ObjectKey::Expr(key_expr) => {
                            let mut results = self.eval_expr(key_expr, value.clone(), ctx.clone());
                            match results.next() {
                                Some(Ok(Jv::String(s))) => s.as_str().to_string(),
                                Some(Ok(other)) => {
                                    return Err(format!("Cannot use {} as object key", other.type_name()));
                                }
                                Some(Err(e)) => return Err(e),
                                None => return Err("Expression key produced no value".to_string()),
                            }
                        }
                    };
                    let elem = obj.get(&key_str).unwrap_or(Jv::Null);
                    self.collect_bindings(pat, &elem, ctx, bindings)?;
                }
                Ok(())
            }
            PatternKind::Alternative(first, second) => {
                // Try first pattern
                let first_result = self.try_bind_pattern(first, value, ctx);
                if let Ok(binds) = first_result {
                    bindings.extend(binds);
                    Ok(())
                } else {
                    // Try second
                    self.collect_bindings(second, value, ctx, bindings)
                }
            }
        }
    }

    /// Collect all variable names from a pattern (for pre-binding to null)
    fn collect_pattern_vars(pattern: &Pattern, vars: &mut std::collections::HashSet<String>) {
        match &pattern.kind {
            PatternKind::Binding(name) => {
                vars.insert(name.clone());
            }
            PatternKind::BoundPattern { name, pattern: sub_pattern } => {
                vars.insert(name.clone());
                Self::collect_pattern_vars(sub_pattern, vars);
            }
            PatternKind::Array(patterns) => {
                for pat in patterns {
                    Self::collect_pattern_vars(pat, vars);
                }
            }
            PatternKind::Object(entries) => {
                for (_, pat) in entries {
                    Self::collect_pattern_vars(pat, vars);
                }
            }
            PatternKind::Alternative(first, second) => {
                Self::collect_pattern_vars(first, vars);
                Self::collect_pattern_vars(second, vars);
            }
        }
    }
}

impl Default for Interpreter {
    fn default() -> Self {
        Interpreter::new()
    }
}

fn eval_binary_op(op: BinaryOp, left: &Jv, right: &Jv) -> Result<Jv, String> {
    match op {
        BinaryOp::Add => add_values(left, right),
        BinaryOp::Sub => sub_values(left, right),
        BinaryOp::Mul => mul_values(left, right),
        BinaryOp::Div => div_values(left, right),
        BinaryOp::Mod => mod_values(left, right),
        BinaryOp::Eq => Ok(Jv::Bool(left == right)),
        BinaryOp::Ne => Ok(Jv::Bool(left != right)),
        BinaryOp::Lt => Ok(Jv::Bool(left < right)),
        BinaryOp::Le => Ok(Jv::Bool(left <= right)),
        BinaryOp::Gt => Ok(Jv::Bool(left > right)),
        BinaryOp::Ge => Ok(Jv::Bool(left >= right)),
        BinaryOp::And => Ok(Jv::Bool(left.is_truthy() && right.is_truthy())),
        BinaryOp::Or => Ok(Jv::Bool(left.is_truthy() || right.is_truthy())),
        // Alternative: return left if truthy, otherwise right
        BinaryOp::Alternative => Ok(if left.is_truthy() { left.clone() } else { right.clone() }),
    }
}

fn add_values(a: &Jv, b: &Jv) -> Result<Jv, String> {
    match (a, b) {
        (Jv::Null, v) | (v, Jv::Null) => Ok(v.clone()),
        (Jv::Number(n1), Jv::Number(n2)) => Ok(Jv::Number(n1.add(n2))),
        (Jv::String(s1), Jv::String(s2)) => Ok(Jv::String(s1.concat(s2))),
        (Jv::Array(a1), Jv::Array(a2)) => Ok(Jv::Array(a1.concat(a2))),
        (Jv::Object(o1), Jv::Object(o2)) => Ok(Jv::Object(o1.merge(o2))),
        _ => Err(format!("{} and {} cannot be added", format_value_for_error(a), format_value_for_error(b))),
    }
}

/// Format a value for error messages, truncating long values like jq.
/// jq uses a 30-byte buffer for truncation.
fn format_value_for_error(v: &Jv) -> String {
    use crate::jv::{JvPrintOptions, print_jv_with_options};

    // Buffer size matching jq's errbuf[30]
    const BUFSIZE: usize = 30;

    // First, dump the value to JSON string representation
    let opts = JvPrintOptions::compact();
    let dumped = print_jv_with_options(v, &opts);

    // Get the type name
    let kind = v.type_name();

    // Truncate the dumped value if needed
    let truncated = jv_dump_string_trunc(&dumped, BUFSIZE);

    format!("{} ({})", kind, truncated)
}

/// Truncate a JSON-dumped string like jq's jv_dump_string_trunc.
/// Uses a buffer of `bufsize` bytes, truncating with "..." and preserving delimiters.
fn jv_dump_string_trunc(s: &str, bufsize: usize) -> String {
    let len = s.len();
    if len > bufsize - 1 && bufsize >= 8 {
        // Determine the closing delimiter based on first character
        let delim = match s.chars().next() {
            Some('"') => Some('"'),
            Some('[') => Some(']'),
            Some('{') => Some('}'),
            _ => None,
        };

        // Calculate truncation point: bufsize - 5 for "...", delim (if any), and null
        // But we don't have null terminator in Rust, so it's bufsize - 4 with delim
        let l = bufsize - if delim.is_some() { 5 } else { 4 };

        // Find UTF-8 boundary at or before position l
        let mut truncate_at = l;
        while truncate_at > 0 && !s.is_char_boundary(truncate_at) {
            truncate_at -= 1;
        }

        let truncated = &s[..truncate_at];
        if let Some(d) = delim {
            format!("{}...{}", truncated, d)
        } else {
            format!("{}...", truncated)
        }
    } else {
        s.to_string()
    }
}

fn sub_values(a: &Jv, b: &Jv) -> Result<Jv, String> {
    match (a, b) {
        (Jv::Number(n1), Jv::Number(n2)) => Ok(Jv::Number(n1.sub(n2))),
        (Jv::Array(arr), Jv::Array(sub)) => {
            let sub_items: Vec<_> = sub.iter().collect();
            let result: Vec<_> = arr.iter().filter(|x| !sub_items.contains(x)).collect();
            Ok(Jv::from_vec(result))
        }
        _ => Err(format!("{} and {} cannot be subtracted", format_value_for_error(a), format_value_for_error(b))),
    }
}

/// Maximum string length for repetition (10MB to match reasonable jq limits)
const MAX_STRING_REPEAT_SIZE: usize = 10_000_000;

fn mul_values(a: &Jv, b: &Jv) -> Result<Jv, String> {
    match (a, b) {
        (Jv::Number(n1), Jv::Number(n2)) => Ok(Jv::Number(n1.mul(n2))),
        (Jv::String(s), Jv::Number(n)) | (Jv::Number(n), Jv::String(s)) => {
            let f = n.as_f64();
            // Handle nan and infinity as returning null
            if f.is_nan() || f.is_infinite() {
                return Ok(Jv::Null);
            }
            // Use floor (toward negative infinity) like jq does
            let count = f.floor() as i64;
            if count < 0 {
                Ok(Jv::Null)
            } else if count == 0 {
                Ok(Jv::string("".to_string()))
            } else {
                let result_len = s.len().saturating_mul(count as usize);
                if result_len > MAX_STRING_REPEAT_SIZE {
                    return Err("Repeat string result too long".to_string());
                }
                Ok(Jv::string(s.as_str().repeat(count as usize)))
            }
        }
        (Jv::Object(o1), Jv::Object(o2)) => {
            // Recursive merge
            Ok(Jv::Object(recursive_merge(o1, o2)))
        }
        _ => Err(format!("{} and {} cannot be multiplied", a.type_name(), b.type_name())),
    }
}

fn div_values(a: &Jv, b: &Jv) -> Result<Jv, String> {
    match (a, b) {
        (Jv::Number(n1), Jv::Number(n2)) => {
            if n2.as_f64() == 0.0 {
                Err(format!("{} and {} cannot be divided because the divisor is zero",
                    format_value_for_error(a), format_value_for_error(b)))
            } else {
                Ok(Jv::Number(n1.div(n2)))
            }
        }
        (Jv::String(s), Jv::String(sep)) => {
            let parts: Vec<Jv> = s.split(sep.as_str()).into_iter().map(|p| Jv::String(p)).collect();
            Ok(Jv::from_vec(parts))
        }
        _ => Err(format!("{} and {} cannot be divided", a.type_name(), b.type_name())),
    }
}

fn mod_values(a: &Jv, b: &Jv) -> Result<Jv, String> {
    match (a, b) {
        (Jv::Number(n1), Jv::Number(n2)) => {
            if n2.as_f64() == 0.0 {
                Err(format!("{} and {} cannot be divided (remainder) because the divisor is zero",
                    format_value_for_error(a), format_value_for_error(b)))
            } else {
                Ok(Jv::Number(n1.modulo(n2)))
            }
        }
        _ => Err(format!("{} and {} cannot use modulo", a.type_name(), b.type_name())),
    }
}

fn recursive_merge(o1: &JvObject, o2: &JvObject) -> JvObject {
    let mut result = o1.clone();
    for (k, v2) in o2.iter() {
        let merged = if let Some(v1) = result.get(&k) {
            match (&v1, &v2) {
                (Jv::Object(o1_inner), Jv::Object(o2_inner)) => {
                    Jv::Object(recursive_merge(o1_inner, o2_inner))
                }
                _ => v2,
            }
        } else {
            v2
        };
        result.set(&k, merged);
    }
    result
}

/// Convenience function to interpret an expression
pub fn interpret(expr: &Expr, input: Jv) -> EvalResult {
    let mut interp = Interpreter::new();
    interp.eval(expr, input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;
    use crate::jv::parse_json;

    fn eval(filter: &str, input: &str) -> Vec<Result<Jv, String>> {
        let expr = parse(filter).expect("parse failed");
        let input = parse_json(input).expect("json parse failed");
        interpret(&expr, input).collect()
    }

    fn eval_ok(filter: &str, input: &str) -> Vec<Jv> {
        eval(filter, input).into_iter().map(|r| r.expect("eval error")).collect()
    }

    #[test]
    fn test_identity() {
        let results = eval_ok(".", r#"{"a": 1}"#);
        assert_eq!(results.len(), 1);
        assert!(results[0].is_object());
    }

    #[test]
    fn test_field() {
        let results = eval_ok(".a", r#"{"a": 1, "b": 2}"#);
        assert_eq!(results, vec![Jv::from_i64(1)]);
    }

    #[test]
    fn test_nested_field() {
        let results = eval_ok(".a.b", r#"{"a": {"b": 42}}"#);
        assert_eq!(results, vec![Jv::from_i64(42)]);
    }

    #[test]
    fn test_pipe() {
        let results = eval_ok(".a | . + 1", r#"{"a": 5}"#);
        assert_eq!(results, vec![Jv::from_i64(6)]);
    }

    #[test]
    fn test_iterator() {
        let results = eval_ok(".[]", "[1, 2, 3]");
        assert_eq!(results, vec![Jv::from_i64(1), Jv::from_i64(2), Jv::from_i64(3)]);
    }

    #[test]
    fn test_map() {
        let results = eval_ok("map(. + 1)", "[1, 2, 3]");
        assert_eq!(results.len(), 1);
        let arr = results[0].as_array().unwrap();
        assert_eq!(arr.len(), 3);
    }

    #[test]
    fn test_select() {
        let results = eval_ok(".[] | select(. > 2)", "[1, 2, 3, 4]");
        assert_eq!(results, vec![Jv::from_i64(3), Jv::from_i64(4)]);
    }

    #[test]
    fn test_arithmetic() {
        assert_eq!(eval_ok("1 + 2", "null"), vec![Jv::from_i64(3)]);
        assert_eq!(eval_ok("5 - 3", "null"), vec![Jv::from_i64(2)]);
        assert_eq!(eval_ok("2 * 3", "null"), vec![Jv::from_i64(6)]);
        assert_eq!(eval_ok("10 / 2", "null"), vec![Jv::from_f64(5.0)]);
    }

    #[test]
    fn test_comparison() {
        assert_eq!(eval_ok("1 == 1", "null"), vec![Jv::Bool(true)]);
        assert_eq!(eval_ok("1 != 2", "null"), vec![Jv::Bool(true)]);
        assert_eq!(eval_ok("1 < 2", "null"), vec![Jv::Bool(true)]);
        assert_eq!(eval_ok("2 > 1", "null"), vec![Jv::Bool(true)]);
    }

    #[test]
    fn test_if_then_else() {
        assert_eq!(eval_ok("if . > 0 then 1 else 0 end", "5"), vec![Jv::from_i64(1)]);
        assert_eq!(eval_ok("if . > 0 then 1 else 0 end", "-5"), vec![Jv::from_i64(0)]);
    }

    #[test]
    fn test_array_construction() {
        let results = eval_ok("[.a, .b]", r#"{"a": 1, "b": 2}"#);
        assert_eq!(results.len(), 1);
        let arr = results[0].as_array().unwrap();
        assert_eq!(arr.len(), 2);
    }

    #[test]
    fn test_object_construction() {
        let results = eval_ok("{x: .a, y: .b}", r#"{"a": 1, "b": 2}"#);
        assert_eq!(results.len(), 1);
        assert!(results[0].is_object());
    }

    #[test]
    fn test_reduce() {
        let results = eval_ok("reduce .[] as $x (0; . + $x)", "[1, 2, 3, 4]");
        assert_eq!(results, vec![Jv::from_i64(10)]);
    }

    #[test]
    fn test_try_catch() {
        let results = eval_ok("try .a.b catch \"error\"", r#"{"a": 1}"#);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_alternative() {
        assert_eq!(eval_ok(".a // \"default\"", r#"{"b": 1}"#), vec![Jv::string("default")]);
        assert_eq!(eval_ok(".a // \"default\"", r#"{"a": 1}"#), vec![Jv::from_i64(1)]);
    }

    #[test]
    fn test_string_interp() {
        let results = eval_ok(r#""value: \(.a)""#, r#"{"a": 42}"#);
        assert_eq!(results, vec![Jv::string("value: 42")]);
    }
}
