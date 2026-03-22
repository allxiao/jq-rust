//! AST-walking interpreter for jq expressions

use std::rc::Rc;
use std::cell::RefCell;

use crate::jv::{Jv, JvObject};
use crate::parser::{Expr, ExprKind, Literal, BinaryOp, ObjectKey, StringPart, FuncDef, Pattern, PatternKind};
use super::context::Context;

/// Result of evaluating an expression - can produce multiple values
pub type EvalResult = Box<dyn Iterator<Item = Result<Jv, String>>>;

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
                            let index_results = inner.eval_expr(&index_expr, base_val.clone(), ctx_clone.clone());

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
                                                Some(Err(format!(
                                                    "Cannot index {} with {}",
                                                    base_val_for_index.type_name(),
                                                    idx_val.type_name()
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

                let mut this = Interpreter { ctx: ctx.clone() };
                let base_results = this.eval_expr(&base_expr, input, ctx_clone.clone());

                Box::new(base_results.flat_map(move |base_result| {
                    match base_result {
                        Err(e) if !optional => Box::new(std::iter::once(Err(e))) as EvalResult,
                        Err(_) => Box::new(std::iter::empty()),
                        Ok(base_val) => {
                            // Evaluate start index
                            let start_val = if let Some(ref s) = start_expr {
                                let mut inner = Interpreter { ctx: ctx_clone.clone() };
                                let mut results = inner.eval_expr(s, base_val.clone(), ctx_clone.clone());
                                match results.next() {
                                    Some(Ok(v)) => v.as_i64(),
                                    _ => None,
                                }
                            } else {
                                None
                            };

                            // Evaluate end index
                            let end_val = if let Some(ref e) = end_expr {
                                let mut inner = Interpreter { ctx: ctx_clone.clone() };
                                let mut results = inner.eval_expr(e, base_val.clone(), ctx_clone.clone());
                                match results.next() {
                                    Some(Ok(v)) => v.as_i64(),
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
                                    "Cannot iterate over {}",
                                    base_val.type_name()
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
                let right_expr = right.clone();
                let input_clone = input.clone();
                let ctx_clone = ctx.clone();

                let mut this = Interpreter { ctx: ctx.clone() };
                let left_results = this.eval_expr(left, input, ctx_clone.clone());

                Box::new(left_results.chain(std::iter::from_fn({
                    let mut done = false;
                    move || {
                        if done {
                            return None;
                        }
                        done = true;
                        let mut inner = Interpreter { ctx: ctx_clone.clone() };
                        Some(inner.eval_expr(&right_expr, input_clone.clone(), ctx_clone.clone()))
                    }
                }).flatten()))
            }

            ExprKind::Conditional { condition, then_branch, else_branch } => {
                let then_expr = then_branch.clone();
                let else_expr = else_branch.clone();
                let input_clone = input.clone();
                let ctx_clone = ctx.clone();

                let mut this = Interpreter { ctx: ctx.clone() };
                let cond_result = this.eval_expr(condition, input, ctx_clone.clone()).next();

                match cond_result {
                    Some(Err(e)) => Box::new(std::iter::once(Err(e))),
                    Some(Ok(v)) => {
                        let mut inner = Interpreter { ctx: ctx_clone.clone() };
                        if v.is_truthy() {
                            inner.eval_expr(&then_expr, input_clone, ctx_clone)
                        } else if let Some(ref else_e) = else_expr {
                            inner.eval_expr(else_e, input_clone, ctx_clone)
                        } else {
                            Box::new(std::iter::once(Ok(Jv::Null)))
                        }
                    }
                    None => Box::new(std::iter::empty()),
                }
            }

            ExprKind::TryCatch { expr: try_expr, catch } => {
                let catch_expr = catch.clone();
                let input_clone = input.clone();
                let ctx_clone = ctx.clone();

                let mut this = Interpreter { ctx: ctx.clone() };
                let results: Vec<_> = this.eval_expr(try_expr, input, ctx_clone.clone()).collect();

                Box::new(results.into_iter().flat_map(move |result| {
                    match result {
                        Ok(v) => Box::new(std::iter::once(Ok(v))) as EvalResult,
                        Err(e) => {
                            if let Some(ref catch_e) = catch_expr {
                                // Set up error as input to catch
                                let mut inner = Interpreter { ctx: ctx_clone.clone() };
                                inner.eval_expr(catch_e, Jv::string(&e), ctx_clone.clone())
                            } else {
                                // No catch - suppress error
                                Box::new(std::iter::empty())
                            }
                        }
                    }
                }))
            }

            ExprKind::BinaryOp { op, left, right } => {
                let op = *op;
                let right_expr = right.clone();
                let input_clone = input.clone();
                let ctx_clone = ctx.clone();

                let mut this = Interpreter { ctx: ctx.clone() };
                let left_results = this.eval_expr(left, input, ctx_clone.clone());

                Box::new(left_results.flat_map(move |left_result| {
                    match left_result {
                        Err(e) => Box::new(std::iter::once(Err(e))) as EvalResult,
                        Ok(left_val) => {
                            let mut inner = Interpreter { ctx: ctx_clone.clone() };
                            let right_results = inner.eval_expr(&right_expr, input_clone.clone(), ctx_clone.clone());

                            Box::new(right_results.map(move |right_result| {
                                match right_result {
                                    Err(e) => Err(e),
                                    Ok(right_val) => eval_binary_op(op, &left_val, &right_val),
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
                        Ok(v) => Err(format!("{} cannot be negated", v.type_name())),
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

            ExprKind::FunctionCall { name, args } => {
                self.eval_function_call(name, args, input, ctx)
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
                            if let Err(e) = Self::bind_pattern(&pattern, &bind_val, &child_ctx) {
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
                let mut left_results = this.eval_expr(left, input, ctx_clone.clone()).peekable();

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
                // Register function in context
                let child_ctx = Rc::new(RefCell::new(Context::child(ctx.clone())));
                child_ctx.borrow_mut().bind_function(&def.name, Rc::new(def.clone()));

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
                obj.set("file", Jv::string("<input>"));
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
                                let mut ctx_mut = ctx_clone.borrow_mut();
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
                    let mut ctx_mut = ctx_clone.borrow_mut();
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

                // Get current value at target
                let mut get_interp = Interpreter { ctx: ctx.clone() };
                let current_results: Vec<_> = get_interp.eval_expr(&target_expr, input.clone(), ctx_clone.clone()).collect();

                Box::new(current_results.into_iter().flat_map(move |current_result| {
                    match current_result {
                        Err(e) => Box::new(std::iter::once(Err(e))) as EvalResult,
                        Ok(current_val) => {
                            // Pipe current value through the filter
                            let mut val_interp = Interpreter { ctx: ctx_clone.clone() };
                            let new_value = match val_interp.eval_expr(&value_expr, current_val, ctx_clone.clone()).next() {
                                Some(Ok(v)) => v,
                                Some(Err(e)) => return Box::new(std::iter::once(Err(e))) as EvalResult,
                                None => return Box::new(std::iter::empty()) as EvalResult,
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

            ExprKind::UpdateOp { op, target, value } => {
                // expr += f means: evaluate f and apply arithmetic op to current value
                let op = *op;
                let target_expr = target.clone();
                let value_expr = value.clone();
                let ctx_clone = ctx.clone();

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

            _ => {
                Box::new(std::iter::once(Err(format!("expression type not yet implemented: {:?}", expr.kind))))
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

    fn eval_function_call(&mut self, name: &str, args: &[Expr], input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        let arity = args.len();

        // Check for special built-in higher-order functions
        match (name, arity) {
            ("map", 1) => return self.eval_map(&args[0], input, ctx),
            ("select", 1) => return self.eval_select(&args[0], input, ctx),
            ("recurse", 0) => return self.recurse(input),
            ("recurse", 1) => return self.eval_recurse_with(&args[0], input, ctx),
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
            ("ascii_downcase", 0) | ("ascii_upcase", 0) => {
                // These are handled as regular builtins
            }
            _ => {}
        }

        // Check for user-defined function
        let maybe_func = ctx.borrow().lookup_function(name);
        if let Some(func_def) = maybe_func {
            return self.call_user_function(&func_def, args, input, ctx);
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

        Box::new(std::iter::once(Err(format!("unknown function: {}/{}", name, arity))))
    }

    fn call_user_function(&mut self, func: &FuncDef, args: &[Expr], input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        // Create child context with parameter bindings
        let child_ctx = Rc::new(RefCell::new(Context::child(ctx.clone())));

        // Bind parameters
        for (param, arg) in func.params.iter().zip(args.iter()) {
            if param.is_binding {
                // Value parameter ($var) - evaluate and bind
                let mut arg_interp = Interpreter { ctx: ctx.clone() };
                match arg_interp.eval_expr(arg, input.clone(), ctx.clone()).next() {
                    Some(Ok(v)) => {
                        child_ctx.borrow_mut().bind_value(&param.name, v);
                    }
                    Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
                    None => return Box::new(std::iter::empty()),
                }
            } else {
                // Filter parameter (non-$) - for now, evaluate and bind as value
                // TODO: Full filter parameter support requires storing both expression and context
                let mut arg_interp = Interpreter { ctx: ctx.clone() };
                match arg_interp.eval_expr(arg, input.clone(), ctx.clone()).next() {
                    Some(Ok(v)) => {
                        child_ctx.borrow_mut().bind_value(&param.name, v);
                    }
                    Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
                    None => return Box::new(std::iter::empty()),
                }
            }
        }

        let mut inner = Interpreter { ctx: child_ctx.clone() };
        inner.eval_expr(&func.body, input, child_ctx)
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
            _ => Box::new(std::iter::once(Err(format!("Cannot iterate over {}", input.type_name())))),
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
                    if let Err(e) = Self::bind_pattern(pattern, &item, &child_ctx) {
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

        // Evaluate initial value
        let mut init_inner = Interpreter { ctx: ctx_clone.clone() };
        let mut state = match init_inner.eval_expr(init_expr, input.clone(), ctx_clone.clone()).next() {
            Some(Ok(v)) => v,
            Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
            None => return Box::new(std::iter::empty()),
        };

        let mut results = Vec::new();

        // Iterate over values
        let mut iter_inner = Interpreter { ctx: ctx_clone.clone() };
        for result in iter_inner.eval_expr(iter_expr, input.clone(), ctx_clone.clone()) {
            match result {
                Ok(item) => {
                    // Create context with binding
                    let child_ctx = Rc::new(RefCell::new(Context::child(ctx_clone.clone())));
                    if let Err(e) = Self::bind_pattern(pattern, &item, &child_ctx) {
                        return Box::new(std::iter::once(Err(e)));
                    }

                    // Evaluate update with state as input
                    let mut update_inner = Interpreter { ctx: child_ctx.clone() };
                    match update_inner.eval_expr(update_expr, state.clone(), child_ctx.clone()).next() {
                        Some(Ok(v)) => state = v,
                        Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
                        None => {}
                    }

                    // Extract output if provided
                    if let Some(ext_expr) = extract_expr {
                        let mut ext_inner = Interpreter { ctx: child_ctx.clone() };
                        for ext_result in ext_inner.eval_expr(ext_expr, state.clone(), child_ctx) {
                            match ext_result {
                                Ok(v) => results.push(Ok(v)),
                                Err(e) => results.push(Err(e)),
                            }
                        }
                    } else {
                        results.push(Ok(state.clone()));
                    }
                }
                Err(e) => return Box::new(std::iter::once(Err(e))),
            }
        }

        Box::new(results.into_iter())
    }

    fn eval_group_by(&mut self, key_expr: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        match &input {
            Jv::Array(arr) => {
                use std::collections::BTreeMap;
                let mut groups: BTreeMap<String, Vec<Jv>> = BTreeMap::new();

                for item in arr.iter() {
                    let mut inner = Interpreter { ctx: ctx.clone() };
                    match inner.eval_expr(key_expr, item.clone(), ctx.clone()).next() {
                        Some(Ok(key)) => {
                            let key_str = format!("{}", key);
                            groups.entry(key_str).or_default().push(item);
                        }
                        Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
                        None => {}
                    }
                }

                let result: Vec<Jv> = groups.into_values()
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
                let mut items_with_keys: Vec<(Jv, Jv)> = Vec::new();

                for item in arr.iter() {
                    let mut inner = Interpreter { ctx: ctx.clone() };
                    match inner.eval_expr(key_expr, item.clone(), ctx.clone()).next() {
                        Some(Ok(key)) => items_with_keys.push((key, item)),
                        Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
                        None => items_with_keys.push((Jv::Null, item)),
                    }
                }

                items_with_keys.sort_by(|a, b| a.0.cmp(&b.0));
                let result: Vec<Jv> = items_with_keys.into_iter().map(|(_, v)| v).collect();
                Box::new(std::iter::once(Ok(Jv::from_vec(result))))
            }
            _ => Box::new(std::iter::once(Err("sort_by requires array".to_string()))),
        }
    }

    fn eval_unique_by(&mut self, key_expr: &Expr, input: Jv, ctx: Rc<RefCell<Context>>) -> EvalResult {
        match &input {
            Jv::Array(arr) => {
                use std::collections::HashSet;
                let mut seen: HashSet<String> = HashSet::new();
                let mut result = Vec::new();

                for item in arr.iter() {
                    let mut inner = Interpreter { ctx: ctx.clone() };
                    match inner.eval_expr(key_expr, item.clone(), ctx.clone()).next() {
                        Some(Ok(key)) => {
                            let key_str = format!("{}", key);
                            if seen.insert(key_str) {
                                result.push(item);
                            }
                        }
                        Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
                        None => {}
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
                let mut max_item: Option<(Jv, Jv)> = None;

                for item in arr.iter() {
                    let mut inner = Interpreter { ctx: ctx.clone() };
                    match inner.eval_expr(key_expr, item.clone(), ctx.clone()).next() {
                        Some(Ok(key)) => {
                            if let Some((ref max_key, _)) = max_item {
                                if key > *max_key {
                                    max_item = Some((key, item));
                                }
                            } else {
                                max_item = Some((key, item));
                            }
                        }
                        Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
                        None => {}
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
                let mut min_item: Option<(Jv, Jv)> = None;

                for item in arr.iter() {
                    let mut inner = Interpreter { ctx: ctx.clone() };
                    match inner.eval_expr(key_expr, item.clone(), ctx.clone()).next() {
                        Some(Ok(key)) => {
                            if let Some((ref min_key, _)) = min_item {
                                if key < *min_key {
                                    min_item = Some((key, item));
                                }
                            } else {
                                min_item = Some((key, item));
                            }
                        }
                        Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
                        None => {}
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
                let mut idx_interp = Interpreter { ctx: ctx.clone() };
                let idx_val = match idx_interp.eval_expr(index, current.clone(), ctx.clone()).next() {
                    Some(Ok(v)) => v,
                    Some(Err(e)) => return Err(e),
                    None => return Ok(current),
                };

                match &base.kind {
                    ExprKind::Identity => {
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
                // del(a, b) - delete from both paths, left-to-right
                // jq applies deletions left-to-right and recalculates indices after each
                let result = Self::apply_deletion(current, left, ctx.clone())?;
                Self::apply_deletion(result, right, ctx)
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
        // repeat(f) - repeatedly apply f, yielding each result
        let expr_clone = expr.clone();
        let ctx_clone = ctx.clone();
        let mut current = input;

        // Use an iterator that repeatedly applies expr
        struct RepeatIter {
            expr: Expr,
            ctx: Rc<RefCell<Context>>,
            current: Jv,
            count: usize,
        }

        impl Iterator for RepeatIter {
            type Item = Result<Jv, String>;

            fn next(&mut self) -> Option<Self::Item> {
                if self.count > 10000 {
                    return Some(Err("repeat: too many iterations".to_string()));
                }
                self.count += 1;

                let result = self.current.clone();

                // Apply expression to get next value
                let mut interp = Interpreter { ctx: self.ctx.clone() };
                match interp.eval_expr(&self.expr, self.current.clone(), self.ctx.clone()).next() {
                    Some(Ok(v)) => {
                        self.current = v;
                        Some(Ok(result))
                    }
                    Some(Err(e)) => Some(Err(e)),
                    None => None,
                }
            }
        }

        Box::new(RepeatIter {
            expr: expr_clone,
            ctx: ctx_clone,
            current,
            count: 0,
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
        fn walk_value(interp: &mut Interpreter, filter: &Expr, value: Jv, ctx: Rc<RefCell<Context>>) -> Result<Jv, String> {
            // First, recursively walk children
            let walked = match &value {
                Jv::Array(arr) => {
                    let mut new_arr = Vec::new();
                    for item in arr.iter() {
                        new_arr.push(walk_value(interp, filter, item, ctx.clone())?);
                    }
                    Jv::from_vec(new_arr)
                }
                Jv::Object(obj) => {
                    let mut new_obj = crate::jv::JvObject::new();
                    for (k, v) in obj.iter() {
                        let walked_v = walk_value(interp, filter, v, ctx.clone())?;
                        new_obj.set(&k, walked_v);
                    }
                    Jv::Object(new_obj)
                }
                _ => value.clone(),
            };

            // Then apply filter to the walked value
            let mut filter_interp = Interpreter { ctx: ctx.clone() };
            match filter_interp.eval_expr(filter, walked, ctx).next() {
                Some(Ok(v)) => Ok(v),
                Some(Err(e)) => Err(e),
                None => Ok(Jv::Null),
            }
        }

        match walk_value(self, filter, input, ctx) {
            Ok(v) => Box::new(std::iter::once(Ok(v))),
            Err(e) => Box::new(std::iter::once(Err(e))),
        }
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
                        // Evaluate the index
                        let mut interp = Interpreter { ctx: ctx.clone() };
                        if let Some(Ok(idx)) = interp.eval_expr(index, input.clone(), ctx.clone()).next() {
                            let mut new_path = base_path;
                            new_path.push(idx);
                            paths.push(new_path);
                        }
                    }
                }
                ExprKind::Pipe(left, right) => {
                    // For pipes, we need to traverse left first, then right
                    let left_paths = collect_paths(left, input, ctx.clone(), current_path);
                    for path in left_paths {
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
                ExprKind::Comma(left, right) => {
                    // For comma, collect paths from both sides
                    paths.extend(collect_paths(left, input, ctx.clone(), current_path.clone()));
                    paths.extend(collect_paths(right, input, ctx, current_path));
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
            .map(|p| Ok(Jv::from_vec(p)))
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

            match key {
                Jv::String(k) => {
                    let k = k.as_str();
                    if let Jv::Object(obj) = target {
                        if rest.is_empty() {
                            obj.set(k, value);
                        } else {
                            let existing = obj.get(k).unwrap_or(Jv::Object(crate::jv::JvObject::new()));
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
                            let existing = arr.get(idx).unwrap_or(Jv::Object(crate::jv::JvObject::new()));
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

        // Start with an empty object (pick typically returns an object)
        let mut result = Jv::Object(crate::jv::JvObject::new());

        for path in paths {
            if let Some(value) = get_at_path(&input, &path) {
                set_at_path(&mut result, &path, value);
            }
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
                            let mut ctx_mut = ctx.borrow_mut();
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

                // Evaluate the index
                let mut idx_interp = Interpreter { ctx: ctx.clone() };
                let idx_val = match idx_interp.eval_expr(index, current.clone(), ctx.clone()).next() {
                    Some(Ok(v)) => v,
                    Some(Err(e)) => return Err(e),
                    None => return Err("index evaluation produced no value".to_string()),
                };

                match &base.kind {
                    ExprKind::Identity => {
                        // Direct index on input: .[idx] = value
                        match &idx_val {
                            Jv::String(s) => {
                                match current {
                                    Jv::Object(mut obj) => {
                                        obj.set(s.as_str(), value);
                                        Ok(Jv::Object(obj))
                                    }
                                    Jv::Null => {
                                        let mut obj = JvObject::new();
                                        obj.set(s.as_str(), value);
                                        Ok(Jv::Object(obj))
                                    }
                                    _ => Err(format!("Cannot index {} with string", current.type_name())),
                                }
                            }
                            Jv::Number(n) => {
                                if let Some(idx) = n.as_i64() {
                                    match current {
                                        Jv::Array(mut arr) => {
                                            let len = arr.len() as i64;
                                            let actual_idx = if idx < 0 { len + idx } else { idx };
                                            if actual_idx < 0 {
                                                return Err("Out of bounds negative array index".to_string());
                                            }
                                            arr.set(actual_idx, value)?;
                                            Ok(Jv::Array(arr))
                                        }
                                        Jv::Null => {
                                            if idx < 0 {
                                                return Err("Out of bounds negative array index".to_string());
                                            }
                                            let mut arr = JvArray::new();
                                            arr.set(idx, value)?;
                                            Ok(Jv::Array(arr))
                                        }
                                        _ => Err(format!("Cannot index {} with number", current.type_name())),
                                    }
                                } else {
                                    Err("Array index must be integer".to_string())
                                }
                            }
                            _ => Err(format!("Cannot use {} as index", idx_val.type_name())),
                        }
                    }
                    _ => {
                        // Nested: get base value, apply assignment, set back
                        let mut base_interp = Interpreter { ctx: ctx.clone() };
                        let base_val = match base_interp.eval_expr(base, current.clone(), ctx.clone()).next() {
                            Some(Ok(v)) => v,
                            Some(Err(e)) => return Err(e),
                            None => Jv::Null,
                        };

                        // Apply inner assignment
                        let inner_target = Expr::new(
                            ExprKind::Index {
                                expr: Box::new(Expr::new(ExprKind::Identity, target.span)),
                                index: index.clone(),
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

                // Evaluate start and end indices
                let start_val = if let Some(start_expr) = start {
                    match interp.eval_expr(start_expr, current.clone(), ctx.clone()).next() {
                        Some(Ok(Jv::Number(n))) => n.as_i64().unwrap_or(0),
                        Some(Err(e)) => return Err(e),
                        _ => 0,
                    }
                } else {
                    0
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
                            Jv::String(s) => {
                                // String slice assignment
                                let chars: Vec<char> = s.as_str().chars().collect();
                                let len = chars.len();
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

                                let replacement = match &value {
                                    Jv::String(s) => s.as_str().to_string(),
                                    _ => return Err("Cannot assign non-string to string slice".to_string()),
                                };

                                let mut result: String = chars[..start_idx.min(len)].iter().collect();
                                result.push_str(&replacement);
                                result.extend(chars[end_idx.min(len)..].iter());
                                Ok(Jv::string(result))
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
            _ => Err(format!("Cannot assign to expression: {:?}", target.kind)),
        }
    }

    /// Bind values to a pattern, returning error if pattern doesn't match
    fn bind_pattern(pattern: &Pattern, value: &Jv, ctx: &Rc<RefCell<Context>>) -> Result<(), String> {
        match &pattern.kind {
            PatternKind::Binding(name) => {
                ctx.borrow_mut().bind_value(name, value.clone());
                Ok(())
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
                    Self::bind_pattern(pat, &elem, ctx)?;
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
                        ObjectKey::Expr(_) => {
                            // Expression keys in patterns are not commonly used
                            return Err("expression keys in patterns not yet supported".to_string());
                        }
                    };

                    let elem = obj.get(&key_str).unwrap_or(Jv::Null);
                    Self::bind_pattern(pat, &elem, ctx)?;
                }
                Ok(())
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
    }
}

fn add_values(a: &Jv, b: &Jv) -> Result<Jv, String> {
    match (a, b) {
        (Jv::Null, v) | (v, Jv::Null) => Ok(v.clone()),
        (Jv::Number(n1), Jv::Number(n2)) => Ok(Jv::Number(n1.add(n2))),
        (Jv::String(s1), Jv::String(s2)) => Ok(Jv::String(s1.concat(s2))),
        (Jv::Array(a1), Jv::Array(a2)) => Ok(Jv::Array(a1.concat(a2))),
        (Jv::Object(o1), Jv::Object(o2)) => Ok(Jv::Object(o1.merge(o2))),
        _ => Err(format!("{} and {} cannot be added", a.type_name(), b.type_name())),
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
        _ => Err(format!("{} and {} cannot be subtracted", a.type_name(), b.type_name())),
    }
}

/// Maximum string length for repetition (10MB to match reasonable jq limits)
const MAX_STRING_REPEAT_SIZE: usize = 10_000_000;

fn mul_values(a: &Jv, b: &Jv) -> Result<Jv, String> {
    match (a, b) {
        (Jv::Number(n1), Jv::Number(n2)) => Ok(Jv::Number(n1.mul(n2))),
        (Jv::String(s), Jv::Number(n)) | (Jv::Number(n), Jv::String(s)) => {
            if let Some(count) = n.as_i64() {
                if count <= 0 {
                    Ok(Jv::Null)
                } else {
                    let result_len = s.len().saturating_mul(count as usize);
                    if result_len > MAX_STRING_REPEAT_SIZE {
                        return Err("Repeat string result too long".to_string());
                    }
                    Ok(Jv::string(s.as_str().repeat(count as usize)))
                }
            } else {
                Err("string multiplication requires integer".to_string())
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
                Err("division by zero".to_string())
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
                Err("modulo by zero".to_string())
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
