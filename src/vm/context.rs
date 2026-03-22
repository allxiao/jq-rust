//! Execution context for the interpreter

use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;

use crate::jv::Jv;
use crate::parser::FuncDef;

/// A binding in the current scope
#[derive(Debug, Clone)]
pub enum Binding {
    /// A value binding ($var)
    Value(Jv),
    /// A filter/function binding with closure (captures definition context)
    FilterClosure {
        def: Rc<FuncDef>,
        ctx: Rc<RefCell<Context>>,
    },
    /// An expression binding with its evaluation context (for filter parameters)
    ExprWithContext {
        expr: Rc<crate::parser::Expr>,
        ctx: Rc<RefCell<Context>>,
    },
}

/// Execution context containing variable bindings and function definitions
#[derive(Debug, Clone)]
pub struct Context {
    /// Variable and function bindings
    bindings: HashMap<String, Binding>,
    /// Parent context for lexical scoping
    parent: Option<Rc<RefCell<Context>>>,
    /// Built-in functions registry
    builtins: Rc<BuiltinRegistry>,
}

/// Registry of built-in functions
#[derive(Debug, Default)]
pub struct BuiltinRegistry {
    /// Map from (name, arity) to builtin function
    functions: HashMap<(String, usize), BuiltinFn>,
}

/// A built-in function
pub type BuiltinFn = fn(&mut Context, Jv, &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>>;

impl BuiltinRegistry {
    pub fn new() -> Self {
        let mut registry = BuiltinRegistry::default();
        registry.register_defaults();
        registry
    }

    fn register_defaults(&mut self) {
        // Core functions
        self.register("empty", 0, builtin_empty);
        self.register("null", 0, builtin_null);
        self.register("true", 0, builtin_true);
        self.register("false", 0, builtin_false);
        self.register("not", 0, builtin_not);
        self.register("type", 0, builtin_type);
        self.register("length", 0, builtin_length);
        self.register("keys", 0, builtin_keys);
        self.register("keys_unsorted", 0, builtin_keys_unsorted);
        self.register("values", 0, builtin_values);
        self.register("add", 0, builtin_add);
        self.register("reverse", 0, builtin_reverse);
        self.register("sort", 0, builtin_sort);
        self.register("unique", 0, builtin_unique);
        self.register("flatten", 0, builtin_flatten);
        self.register("flatten", 1, builtin_flatten_depth);
        self.register("transpose", 0, builtin_transpose);
        self.register("first", 0, builtin_first);
        self.register("last", 0, builtin_last);
        self.register("nth", 1, builtin_nth);
        self.register("error", 0, builtin_error);
        self.register("error", 1, builtin_error_msg);
        self.register("debug", 0, builtin_debug);
        self.register("debug", 1, builtin_debug_msg);
        self.register("input_line_number", 0, builtin_input_line_number);
        self.register("$__loc__", 0, builtin_loc);
        self.register("builtins", 0, builtin_builtins);
        self.register("now", 0, builtin_now);
        self.register("have_decnum", 0, builtin_have_decnum);
        self.register("have_literal_numbers", 0, builtin_have_literal_numbers);
        self.register("gmtime", 0, builtin_gmtime);
        self.register("mktime", 0, builtin_mktime);
        self.register("strftime", 1, builtin_strftime);
        self.register("strflocaltime", 1, builtin_strflocaltime);
        self.register("strptime", 1, builtin_strptime);
        self.register("modulemeta", 1, builtin_modulemeta);
        self.register("getpath", 1, builtin_getpath);
        self.register("delpaths", 1, builtin_delpaths);
        self.register("input", 0, builtin_input);
        self.register("inputs", 0, builtin_inputs);

        // Math functions
        self.register("floor", 0, builtin_floor);
        self.register("ceil", 0, builtin_ceil);
        self.register("round", 0, builtin_round);
        self.register("sqrt", 0, builtin_sqrt);
        self.register("fabs", 0, builtin_fabs);
        self.register("abs", 0, builtin_fabs); // abs is alias for fabs

        // String functions
        self.register("tostring", 0, builtin_tostring);
        self.register("tonumber", 0, builtin_tonumber);
        self.register("toboolean", 0, builtin_toboolean);
        self.register("ascii_downcase", 0, builtin_ascii_downcase);
        self.register("ascii_upcase", 0, builtin_ascii_upcase);
        self.register("ltrimstr", 1, builtin_ltrimstr);
        self.register("rtrimstr", 1, builtin_rtrimstr);
        self.register("trimstr", 1, builtin_trimstr);
        self.register("trim", 0, builtin_trim);
        self.register("ltrim", 0, builtin_ltrim);
        self.register("rtrim", 0, builtin_rtrim);
        self.register("startswith", 1, builtin_startswith);
        self.register("endswith", 1, builtin_endswith);
        self.register("split", 1, builtin_split);
        self.register("join", 1, builtin_join);
        self.register("implode", 0, builtin_implode);
        self.register("explode", 0, builtin_explode);
        self.register("tojson", 0, builtin_tojson);
        self.register("fromjson", 0, builtin_fromjson);

        // Array functions
        self.register("has", 1, builtin_has);
        self.register("in", 1, builtin_in);
        self.register("contains", 1, builtin_contains);
        self.register("inside", 1, builtin_inside);
        self.register("getpath", 1, builtin_getpath);
        self.register("setpath", 2, builtin_setpath);
        self.register("delpaths", 1, builtin_delpaths);
        self.register("leaf_paths", 0, builtin_leaf_paths);
        self.register("path", 1, builtin_path);
        self.register("min", 0, builtin_min);
        self.register("max", 0, builtin_max);
        self.register("indices", 1, builtin_indices);
        self.register("index", 1, builtin_index);
        self.register("rindex", 1, builtin_rindex);
        self.register("test", 1, builtin_test);
        self.register("match", 1, builtin_match);
        self.register("capture", 1, builtin_capture);
        self.register("splits", 1, builtin_splits);
        self.register("sub", 2, builtin_sub);
        self.register("gsub", 2, builtin_gsub);
        self.register("scan", 1, builtin_scan);
        self.register("bsearch", 1, builtin_bsearch);
        self.register("ascii", 0, builtin_ascii);
        self.register("utf8bytelength", 0, builtin_utf8bytelength);
        self.register("getpath", 1, builtin_getpath);
        self.register("group_by", 1, builtin_group_by);
        self.register("unique_by", 1, builtin_unique_by);
        self.register("sort_by", 1, builtin_sort_by);
        self.register("max_by", 1, builtin_max_by);
        self.register("min_by", 1, builtin_min_by);

        // Object functions
        self.register("to_entries", 0, builtin_to_entries);
        self.register("from_entries", 0, builtin_from_entries);
        self.register("with_entries", 1, builtin_with_entries);
        self.register("del", 1, builtin_del);
        self.register("paths", 0, builtin_paths);
        self.register("env", 0, builtin_env);

        // Type conversion
        self.register("type", 0, builtin_type);
        self.register("infinite", 0, builtin_infinite);
        self.register("nan", 0, builtin_nan);
        self.register("isinfinite", 0, builtin_isinfinite);
        self.register("isnan", 0, builtin_isnan);
        self.register("isnormal", 0, builtin_isnormal);
        self.register("isfinite", 0, builtin_isfinite);
        self.register("arrays", 0, builtin_arrays);
        self.register("objects", 0, builtin_objects);
        self.register("iterables", 0, builtin_iterables);
        self.register("booleans", 0, builtin_booleans);
        self.register("numbers", 0, builtin_numbers);
        self.register("strings", 0, builtin_strings);
        self.register("nulls", 0, builtin_nulls);
        self.register("scalars", 0, builtin_scalars);

        // More math
        self.register("log", 0, builtin_log);
        self.register("log10", 0, builtin_log10);
        self.register("log2", 0, builtin_log2);
        self.register("exp", 0, builtin_exp);
        self.register("exp10", 0, builtin_exp10);
        self.register("exp2", 0, builtin_exp2);
        self.register("pow", 1, builtin_pow);
        self.register("pow", 2, builtin_pow2);
        self.register("sin", 0, builtin_sin);
        self.register("cos", 0, builtin_cos);
        self.register("tan", 0, builtin_tan);
        self.register("asin", 0, builtin_asin);
        self.register("acos", 0, builtin_acos);
        self.register("atan", 0, builtin_atan);

        // Format functions
        self.register("@base64", 0, builtin_base64);
        self.register("@base64d", 0, builtin_base64d);
        self.register("@uri", 0, builtin_uri);
        self.register("@urid", 0, builtin_urid);
        self.register("@csv", 0, builtin_csv);
        self.register("@tsv", 0, builtin_tsv);
        self.register("@html", 0, builtin_html);
        self.register("@sh", 0, builtin_sh);
        self.register("@json", 0, builtin_json);
        self.register("@text", 0, builtin_text);

        // Higher-order functions (these are special - handled in interpreter)
        // map, select, etc. are implemented differently
    }

    pub fn register(&mut self, name: &str, arity: usize, func: BuiltinFn) {
        self.functions.insert((name.to_string(), arity), func);
    }

    pub fn get(&self, name: &str, arity: usize) -> Option<&BuiltinFn> {
        self.functions.get(&(name.to_string(), arity))
    }

    pub fn has(&self, name: &str, arity: usize) -> bool {
        self.functions.contains_key(&(name.to_string(), arity))
    }

    pub fn all_names(&self) -> Vec<String> {
        let mut names: Vec<_> = self.functions.keys()
            .map(|(name, arity)| format!("{}/{}", name, arity))
            .collect();
        names.sort();
        names
    }
}

impl Context {
    /// Create a new root context
    pub fn new() -> Self {
        Context {
            bindings: HashMap::new(),
            parent: None,
            builtins: Rc::new(BuiltinRegistry::new()),
        }
    }

    /// Create a child context with this as parent
    pub fn child(parent: Rc<RefCell<Context>>) -> Self {
        let builtins = parent.borrow().builtins.clone();
        Context {
            bindings: HashMap::new(),
            parent: Some(parent),
            builtins,
        }
    }

    /// Bind a value to a variable name (stored with $ prefix internally)
    pub fn bind_value(&mut self, name: &str, value: Jv) {
        // Value bindings use $ prefix to distinguish from filter params
        let key = format!("${}", name);
        self.bindings.insert(key, Binding::Value(value));
    }

    /// Bind a function definition with closure (keyed by name/arity)
    pub fn bind_function(&mut self, name: &str, def: Rc<FuncDef>, closure_ctx: Rc<RefCell<Context>>) {
        let arity = def.params.len();
        let key = format!("{}/{}", name, arity);
        self.bindings.insert(key, Binding::FilterClosure { def, ctx: closure_ctx });
    }

    /// Bind an expression with its context (for filter parameters)
    pub fn bind_expr_with_context(&mut self, name: &str, expr: Rc<crate::parser::Expr>, ctx: Rc<RefCell<Context>>) {
        self.bindings.insert(name.to_string(), Binding::ExprWithContext { expr, ctx });
    }

    /// Look up a binding by name
    pub fn lookup(&self, name: &str) -> Option<Binding> {
        if let Some(binding) = self.bindings.get(name) {
            return Some(binding.clone());
        }
        if let Some(ref parent) = self.parent {
            return parent.borrow().lookup(name);
        }
        None
    }

    /// Look up an expression binding with its context
    pub fn lookup_expr_with_context(&self, name: &str) -> Option<(Rc<crate::parser::Expr>, Rc<RefCell<Context>>)> {
        match self.lookup(name) {
            Some(Binding::ExprWithContext { expr, ctx }) => Some((expr, ctx)),
            _ => None,
        }
    }

    /// Look up a value binding (uses $ prefix internally)
    pub fn lookup_value(&self, name: &str) -> Option<Jv> {
        let key = format!("${}", name);
        match self.lookup(&key) {
            Some(Binding::Value(v)) => Some(v),
            _ => None,
        }
    }

    /// Look up a function binding by name and arity, returns def and closure context
    pub fn lookup_function(&self, name: &str, arity: usize) -> Option<(Rc<FuncDef>, Rc<RefCell<Context>>)> {
        let key = format!("{}/{}", name, arity);
        match self.lookup(&key) {
            Some(Binding::FilterClosure { def, ctx }) => Some((def, ctx)),
            _ => None,
        }
    }

    /// Get builtin function by name and arity
    pub fn get_builtin(&self, name: &str, arity: usize) -> Option<&BuiltinFn> {
        self.builtins.get(name, arity)
    }

    /// Check if a builtin exists
    pub fn has_builtin(&self, name: &str, arity: usize) -> bool {
        self.builtins.has(name, arity)
    }
}

impl Default for Context {
    fn default() -> Self {
        Context::new()
    }
}

// ============ Built-in function implementations ============

fn ok(v: Jv) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    Box::new(std::iter::once(Ok(v)))
}

fn err(msg: String) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    Box::new(std::iter::once(Err(msg)))
}

fn builtin_empty(_ctx: &mut Context, _input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    Box::new(std::iter::empty())
}

fn builtin_null(_ctx: &mut Context, _input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    ok(Jv::Null)
}

fn builtin_true(_ctx: &mut Context, _input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    ok(Jv::Bool(true))
}

fn builtin_false(_ctx: &mut Context, _input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    ok(Jv::Bool(false))
}

fn builtin_not(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    ok(Jv::Bool(!input.is_truthy()))
}

fn builtin_type(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    ok(Jv::string(input.type_name()))
}

fn builtin_length(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::Null => ok(Jv::from_i64(0)),
        Jv::String(s) => ok(Jv::from_i64(s.char_len() as i64)),
        Jv::Array(a) => ok(Jv::from_i64(a.len() as i64)),
        Jv::Object(o) => ok(Jv::from_i64(o.len() as i64)),
        Jv::Number(n) => ok(Jv::Number(n.abs())),
        _ => err(format!("{} has no length", input.type_name())),
    }
}

fn builtin_keys(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::Object(o) => {
            let mut keys: Vec<_> = o.keys();
            keys.sort();
            ok(Jv::from_vec(keys.into_iter().map(Jv::string).collect()))
        }
        Jv::Array(a) => {
            let keys: Vec<_> = (0..a.len() as i64).map(Jv::from_i64).collect();
            ok(Jv::from_vec(keys))
        }
        _ => err(format!("{} has no keys", input.type_name())),
    }
}

fn builtin_keys_unsorted(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::Object(o) => {
            let keys = o.keys();
            ok(Jv::from_vec(keys.into_iter().map(Jv::string).collect()))
        }
        Jv::Array(a) => {
            let keys: Vec<_> = (0..a.len() as i64).map(Jv::from_i64).collect();
            ok(Jv::from_vec(keys))
        }
        _ => err(format!("{} has no keys", input.type_name())),
    }
}

fn builtin_values(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    // values is a type selector that passes through everything except null
    match &input {
        Jv::Null => Box::new(std::iter::empty()),
        _ => ok(input),
    }
}

fn builtin_add(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::Array(arr) => {
            let items: Vec<Jv> = arr.iter().collect();
            if items.is_empty() {
                return ok(Jv::Null);
            }
            let mut result = items[0].clone();
            for item in &items[1..] {
                result = add_values(&result, item);
                if result.is_invalid() {
                    return err("cannot add values".to_string());
                }
            }
            ok(result)
        }
        _ => err(format!("cannot add {} values", input.type_name())),
    }
}

fn builtin_reverse(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::Array(a) => ok(Jv::Array(a.reverse())),
        _ => err(format!("{} cannot be reversed", input.type_name())),
    }
}

fn builtin_sort(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::Array(a) => ok(Jv::Array(a.sort())),
        _ => err(format!("{} cannot be sorted", input.type_name())),
    }
}

fn builtin_unique(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::Array(a) => ok(Jv::Array(a.sort().unique())),
        _ => err(format!("{} has no unique values", input.type_name())),
    }
}

fn builtin_flatten(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::Array(a) => ok(Jv::Array(a.flatten(None))),
        _ => err(format!("{} cannot be flattened", input.type_name())),
    }
}

fn builtin_flatten_depth(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    let depth = match args.first().and_then(|v| v.as_i64()) {
        Some(d) if d >= 0 => d as usize,
        _ => return err("flatten depth must not be negative".to_string()),
    };
    match &input {
        Jv::Array(a) => ok(Jv::Array(a.flatten(Some(depth)))),
        _ => err(format!("{} cannot be flattened", input.type_name())),
    }
}

fn builtin_transpose(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::Array(outer) => {
            if outer.is_empty() {
                return ok(Jv::from_vec(vec![]));
            }

            // Find the maximum length of inner arrays
            let mut max_len = 0;
            for item in outer.iter() {
                if let Jv::Array(inner) = item {
                    max_len = max_len.max(inner.len());
                }
            }

            if max_len == 0 {
                return ok(Jv::from_vec(vec![]));
            }

            // Build transposed result
            let mut result = Vec::with_capacity(max_len);
            for col in 0..max_len {
                let mut row = Vec::with_capacity(outer.len());
                for item in outer.iter() {
                    if let Jv::Array(inner) = item {
                        row.push(inner.get(col as i64).unwrap_or(Jv::Null));
                    } else {
                        row.push(Jv::Null);
                    }
                }
                result.push(Jv::from_vec(row));
            }

            ok(Jv::from_vec(result))
        }
        _ => err(format!("{} cannot be transposed", input.type_name())),
    }
}

fn builtin_first(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::Array(a) => {
            match a.get(0) {
                Some(v) => ok(v),
                None => err("first requires non-empty array".to_string()),
            }
        }
        _ => err(format!("{} has no first element", input.type_name())),
    }
}

fn builtin_last(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::Array(a) => {
            match a.get(-1) {
                Some(v) => ok(v),
                None => err("last requires non-empty array".to_string()),
            }
        }
        _ => err(format!("{} has no last element", input.type_name())),
    }
}

fn builtin_nth(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    let idx = match args.first().and_then(|v| v.as_i64()) {
        Some(i) => i,
        _ => return err("nth requires integer index".to_string()),
    };
    match &input {
        Jv::Array(a) => {
            match a.get(idx) {
                Some(v) => ok(v),
                None => ok(Jv::Null),
            }
        }
        _ => err(format!("{} cannot be indexed", input.type_name())),
    }
}

/// Marker prefix for errors that carry a JSON value instead of a simple string
pub const JSON_ERROR_PREFIX: &str = "__JSON_ERROR__:";

fn builtin_error(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match input.as_str() {
        Some(s) => err(s.to_string()),
        None => err(format!("{}{}", JSON_ERROR_PREFIX, input)),
    }
}

fn builtin_error_msg(_ctx: &mut Context, _input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match args.first().and_then(|v| v.as_str()) {
        Some(s) => err(s.to_string()),
        None => {
            if let Some(v) = args.first() {
                err(format!("{}{}", JSON_ERROR_PREFIX, v))
            } else {
                err(String::new())
            }
        }
    }
}

fn builtin_debug(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    // debug outputs to stderr and returns input unchanged
    use crate::jv::print_jv;
    eprintln!("[\"DEBUG:\",{}]", print_jv(&input));
    ok(input)
}

fn builtin_debug_msg(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    use crate::jv::print_jv;
    let msg = args.first().map(|v| print_jv(v)).unwrap_or_else(|| "DEBUG".to_string());
    eprintln!("[{},{}]", msg, print_jv(&input));
    ok(input)
}

fn builtin_input_line_number(_ctx: &mut Context, _input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    // For now, return a placeholder - proper implementation needs runtime state
    ok(Jv::from_i64(1))
}

fn builtin_loc(_ctx: &mut Context, _input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    let mut obj = crate::jv::JvObject::new();
    obj.set("file", Jv::string("<stdin>"));
    obj.set("line", Jv::from_i64(1));
    ok(Jv::Object(obj))
}

fn builtin_builtins(ctx: &mut Context, _input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    // Return list of builtin function names with arities in format "name/arity"
    let names = ctx.builtins.all_names();
    let arr: Vec<Jv> = names.iter().map(|s| Jv::string(s.clone())).collect();
    ok(Jv::from_vec(arr))
}

fn builtin_now(_ctx: &mut Context, _input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs_f64();
    ok(Jv::from_f64(secs))
}

fn builtin_have_decnum(_ctx: &mut Context, _input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    // We use f64, so we don't have decimal (arbitrary precision) number support
    ok(Jv::Bool(false))
}

fn builtin_have_literal_numbers(_ctx: &mut Context, _input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    // We don't have literal number preservation
    ok(Jv::Bool(false))
}

fn builtin_input(_ctx: &mut Context, _input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    // input reads the next input from stdin
    // When there's no more input, jq returns an error with "break"
    // For testing purposes, we always return "break" since we don't have input management
    err("break".to_string())
}

fn builtin_inputs(_ctx: &mut Context, _input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    // inputs returns all remaining inputs
    // When there's no input, it returns nothing (empty)
    Box::new(std::iter::empty())
}

fn builtin_gmtime(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::Number(n) => {
            let timestamp_f64 = n.as_f64();
            let timestamp = timestamp_f64 as i64;
            let frac_secs = timestamp_f64 - timestamp as f64; // fractional seconds
            use chrono::{TimeZone, Datelike, Timelike};
            match chrono::Utc.timestamp_opt(timestamp, 0) {
                chrono::LocalResult::Single(dt) => {
                    // jq format: [year, month (0-11), day (1-31), hour, minute, second, weekday (0=Sun), yearday (0-365)]
                    // Second field includes fractional part
                    let secs_with_frac = dt.second() as f64 + frac_secs;
                    let arr = vec![
                        Jv::from_i64(dt.year() as i64),
                        Jv::from_i64(dt.month0() as i64),  // 0-indexed month
                        Jv::from_i64(dt.day() as i64),
                        Jv::from_i64(dt.hour() as i64),
                        Jv::from_i64(dt.minute() as i64),
                        Jv::from_f64(secs_with_frac),  // Include fractional seconds
                        Jv::from_i64(dt.weekday().num_days_from_sunday() as i64),
                        Jv::from_i64(dt.ordinal0() as i64),  // 0-indexed day of year
                    ];
                    ok(Jv::from_vec(arr))
                }
                _ => err("invalid timestamp".to_string()),
            }
        }
        _ => err("gmtime requires number".to_string()),
    }
}

fn builtin_mktime(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::Array(arr) => {
            if arr.len() < 3 {
                return err("mktime requires at least [year, month, day]".to_string());
            }
            // Validate that year is a number
            let year = match arr.get(0) {
                Some(Jv::Number(n)) => n.as_i64().unwrap_or(1970) as i32,
                _ => return err("mktime requires parsed datetime inputs".to_string()),
            };
            let month = arr.get(1).and_then(|v| v.as_i64()).unwrap_or(0) as u32 + 1; // convert from 0-indexed
            let day = arr.get(2).and_then(|v| v.as_i64()).unwrap_or(1) as u32;
            let hour = arr.get(3).and_then(|v| v.as_i64()).unwrap_or(0) as u32;
            let minute = arr.get(4).and_then(|v| v.as_i64()).unwrap_or(0) as u32;
            let second = arr.get(5).and_then(|v| v.as_i64()).unwrap_or(0) as u32;

            use chrono::{TimeZone, NaiveDate};
            match NaiveDate::from_ymd_opt(year, month, day)
                .and_then(|d| d.and_hms_opt(hour, minute, second))
            {
                Some(naive_dt) => {
                    let dt = chrono::Utc.from_utc_datetime(&naive_dt);
                    ok(Jv::from_i64(dt.timestamp()))
                }
                None => err("invalid date/time components".to_string()),
            }
        }
        _ => err("mktime requires array".to_string()),
    }
}

fn builtin_strftime(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    let format = match args.first() {
        Some(Jv::String(s)) => s.as_str().to_string(),
        _ => return err("strftime/1 requires a string format".to_string()),
    };

    use chrono::{TimeZone, Datelike, Timelike, NaiveDate};

    match &input {
        Jv::Number(n) => {
            // Input is Unix timestamp
            let timestamp = n.as_f64() as i64;
            match chrono::Utc.timestamp_opt(timestamp, 0) {
                chrono::LocalResult::Single(dt) => {
                    ok(Jv::string(dt.format(&format).to_string()))
                }
                _ => err("invalid timestamp".to_string()),
            }
        }
        Jv::Array(arr) => {
            // Input is [year, month (0-11), day, hour, minute, second, ...]
            // Validate that year is a number
            let year = match arr.get(0) {
                Some(Jv::Number(n)) => n.as_i64().unwrap_or(1970) as i32,
                _ => return err("strftime/1 requires parsed datetime inputs".to_string()),
            };
            let month = arr.get(1).and_then(|v| v.as_i64()).unwrap_or(0) as u32 + 1; // convert from 0-indexed
            let day = arr.get(2).and_then(|v| v.as_i64()).unwrap_or(1) as u32;
            let hour = arr.get(3).and_then(|v| v.as_i64()).unwrap_or(0) as u32;
            let minute = arr.get(4).and_then(|v| v.as_i64()).unwrap_or(0) as u32;
            let second = arr.get(5).and_then(|v| v.as_i64()).unwrap_or(0) as u32;

            match NaiveDate::from_ymd_opt(year, month, day)
                .and_then(|d| d.and_hms_opt(hour, minute, second))
            {
                Some(naive_dt) => {
                    let dt = chrono::Utc.from_utc_datetime(&naive_dt);
                    ok(Jv::string(dt.format(&format).to_string()))
                }
                None => err("invalid date/time components".to_string()),
            }
        }
        _ => err("strftime requires number or array".to_string()),
    }
}

// strflocaltime is the same as strftime for now (we don't have proper localtime support)
fn builtin_strflocaltime(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    let format = match args.first() {
        Some(Jv::String(s)) => s.as_str().to_string(),
        _ => return err("strflocaltime/1 requires a string format".to_string()),
    };

    use chrono::{TimeZone, Datelike, Timelike, NaiveDate};

    match &input {
        Jv::Number(n) => {
            let timestamp = n.as_f64() as i64;
            match chrono::Utc.timestamp_opt(timestamp, 0) {
                chrono::LocalResult::Single(dt) => {
                    ok(Jv::string(dt.format(&format).to_string()))
                }
                _ => err("invalid timestamp".to_string()),
            }
        }
        Jv::Array(arr) => {
            // Validate that year is a number
            let year = match arr.get(0) {
                Some(Jv::Number(n)) => n.as_i64().unwrap_or(1970) as i32,
                _ => return err("strflocaltime/1 requires parsed datetime inputs".to_string()),
            };
            let month = arr.get(1).and_then(|v| v.as_i64()).unwrap_or(0) as u32 + 1;
            let day = arr.get(2).and_then(|v| v.as_i64()).unwrap_or(1) as u32;
            let hour = arr.get(3).and_then(|v| v.as_i64()).unwrap_or(0) as u32;
            let minute = arr.get(4).and_then(|v| v.as_i64()).unwrap_or(0) as u32;
            let second = arr.get(5).and_then(|v| v.as_i64()).unwrap_or(0) as u32;

            match NaiveDate::from_ymd_opt(year, month, day)
                .and_then(|d| d.and_hms_opt(hour, minute, second))
            {
                Some(naive_dt) => {
                    let dt = chrono::Utc.from_utc_datetime(&naive_dt);
                    ok(Jv::string(dt.format(&format).to_string()))
                }
                None => err("invalid date/time components".to_string()),
            }
        }
        _ => err("strflocaltime requires number or array".to_string()),
    }
}

fn builtin_strptime(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    let format = match args.first() {
        Some(Jv::String(s)) => s.as_str().to_string(),
        _ => return err("strptime requires format string".to_string()),
    };

    match &input {
        Jv::String(s) => {
            use chrono::{NaiveDateTime, Datelike, Timelike};
            match NaiveDateTime::parse_from_str(s.as_str(), &format) {
                Ok(dt) => {
                    let arr = vec![
                        Jv::from_i64(dt.year() as i64),
                        Jv::from_i64(dt.month0() as i64),  // 0-indexed month
                        Jv::from_i64(dt.day() as i64),
                        Jv::from_i64(dt.hour() as i64),
                        Jv::from_i64(dt.minute() as i64),
                        Jv::from_i64(dt.second() as i64),
                        Jv::from_i64(dt.weekday().num_days_from_sunday() as i64),
                        Jv::from_i64(dt.ordinal0() as i64),
                    ];
                    ok(Jv::from_vec(arr))
                }
                Err(_) => err(format!("cannot parse '{}' with format '{}'", s.as_str(), format)),
            }
        }
        _ => err("strptime requires string".to_string()),
    }
}

fn builtin_modulemeta(_ctx: &mut Context, _input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    // Return empty object for now - module system not implemented
    ok(Jv::Object(crate::jv::JvObject::new()))
}

fn builtin_floor(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match input.as_number() {
        Some(n) => ok(Jv::Number(n.floor())),
        None => err(format!("{} cannot be floored", input.type_name())),
    }
}

fn builtin_ceil(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match input.as_number() {
        Some(n) => ok(Jv::Number(n.ceil())),
        None => err(format!("{} cannot be ceiled", input.type_name())),
    }
}

fn builtin_round(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match input.as_number() {
        Some(n) => ok(Jv::Number(n.round())),
        None => err(format!("{} cannot be rounded", input.type_name())),
    }
}

fn builtin_sqrt(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match input.as_number() {
        Some(n) => ok(Jv::Number(n.sqrt())),
        None => err(format!("{} has no sqrt", input.type_name())),
    }
}

fn builtin_fabs(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::Number(n) => ok(Jv::Number(n.abs())),
        // jq returns strings unchanged for abs
        Jv::String(_) => ok(input),
        _ => err(format!("{} has no absolute value", input.type_name())),
    }
}

fn builtin_tostring(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::String(_) => ok(input),
        _ => {
            use crate::jv::print_jv;
            ok(Jv::string(print_jv(&input)))
        }
    }
}

fn builtin_tonumber(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::Number(_) => ok(input),
        Jv::String(s) => {
            let str_val = s.as_str();
            // Check for null byte - jq versions before 1.7 error on this
            if str_val.contains('\0') {
                // Format the string with \u0000 style escaping to match jq
                let escaped: String = str_val.chars().map(|c| {
                    if c == '\0' { "\\u0000".to_string() }
                    else { c.to_string() }
                }).collect();
                return err(format!("string (\"{}\") cannot be parsed as a number", escaped));
            }
            // jq does NOT trim whitespace - reject strings with leading/trailing space
            match str_val.parse::<f64>() {
                Ok(n) if !n.is_nan() || str_val == "nan" => ok(Jv::from_f64(n)),
                _ => {
                    // Format error message like jq: string ("...") cannot be parsed as a number
                    let display_str = if str_val.len() > 15 {
                        format!("{}...", &str_val[..15])
                    } else {
                        str_val.to_string()
                    };
                    err(format!("string ({}) cannot be parsed as a number",
                        Jv::string(display_str.to_string())))
                }
            }
        }
        _ => err(format!("{} cannot be parsed as number", input.type_name())),
    }
}

fn builtin_toboolean(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    use crate::jv::print_jv;
    match &input {
        Jv::Bool(_) => ok(input),
        Jv::String(s) => {
            match s.as_str() {
                "true" => ok(Jv::Bool(true)),
                "false" => ok(Jv::Bool(false)),
                _ => err(format!("string ({}) cannot be parsed as a boolean", print_jv(&input))),
            }
        }
        _ => err(format!("{} ({}) cannot be parsed as a boolean", input.type_name(), print_jv(&input))),
    }
}

fn builtin_ascii_downcase(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::String(s) => {
            // Only convert ASCII characters to lowercase
            let result: String = s.as_str().chars().map(|c| {
                if c.is_ascii() { c.to_ascii_lowercase() } else { c }
            }).collect();
            ok(Jv::string(result))
        }
        _ => err(format!("{} has no ascii_downcase", input.type_name())),
    }
}

fn builtin_ascii_upcase(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::String(s) => {
            // Only convert ASCII characters to uppercase
            let result: String = s.as_str().chars().map(|c| {
                if c.is_ascii() { c.to_ascii_uppercase() } else { c }
            }).collect();
            ok(Jv::string(result))
        }
        _ => err(format!("{} has no ascii_upcase", input.type_name())),
    }
}

fn builtin_ltrimstr(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match (&input, args.first()) {
        (Jv::String(s), Some(Jv::String(prefix))) => {
            ok(Jv::String(s.ltrimstr(prefix.as_str())))
        }
        // jq reports this as startswith error since ltrimstr uses it internally
        _ => err("startswith() requires string inputs".to_string()),
    }
}

fn builtin_rtrimstr(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match (&input, args.first()) {
        (Jv::String(s), Some(Jv::String(suffix))) => {
            ok(Jv::String(s.rtrimstr(suffix.as_str())))
        }
        // jq reports this as endswith error since rtrimstr uses it internally
        _ => err("endswith() requires string inputs".to_string()),
    }
}

fn builtin_trimstr(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match (&input, args.first()) {
        (Jv::String(s), Some(Jv::String(t))) => {
            // trimstr removes from both ends
            let result = s.ltrimstr(t.as_str());
            ok(Jv::String(result.rtrimstr(t.as_str())))
        }
        _ => err("trimstr requires string arguments".to_string()),
    }
}

// Unicode whitespace characters that jq considers as whitespace for trim
fn is_jq_whitespace(c: char) -> bool {
    matches!(c,
        '\t' | '\n' | '\x0b' | '\x0c' | '\r' | ' ' | // ASCII whitespace
        '\u{0085}' | // NEXT LINE
        '\u{00A0}' | // NO-BREAK SPACE
        '\u{1680}' | // OGHAM SPACE MARK
        '\u{2000}' | '\u{2001}' | '\u{2002}' | '\u{2003}' | '\u{2004}' |
        '\u{2005}' | '\u{2006}' | '\u{2007}' | '\u{2008}' | '\u{2009}' |
        '\u{200A}' | // various width spaces
        '\u{2028}' | // LINE SEPARATOR
        '\u{2029}' | // PARAGRAPH SEPARATOR
        '\u{202F}' | // NARROW NO-BREAK SPACE
        '\u{205F}' | // MEDIUM MATHEMATICAL SPACE
        '\u{3000}'   // IDEOGRAPHIC SPACE
    )
}

fn builtin_trim(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::String(s) => {
            let trimmed = s.as_str().trim_matches(is_jq_whitespace);
            ok(Jv::string(trimmed.to_string()))
        }
        _ => err("trim input must be a string".to_string()),
    }
}

fn builtin_ltrim(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::String(s) => {
            let trimmed = s.as_str().trim_start_matches(is_jq_whitespace);
            ok(Jv::string(trimmed.to_string()))
        }
        _ => err("trim input must be a string".to_string()),
    }
}

fn builtin_rtrim(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::String(s) => {
            let trimmed = s.as_str().trim_end_matches(is_jq_whitespace);
            ok(Jv::string(trimmed.to_string()))
        }
        _ => err("trim input must be a string".to_string()),
    }
}

fn builtin_startswith(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match (&input, args.first()) {
        (Jv::String(s), Some(Jv::String(prefix))) => {
            ok(Jv::Bool(s.starts_with(prefix.as_str())))
        }
        _ => err("startswith requires string arguments".to_string()),
    }
}

fn builtin_endswith(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match (&input, args.first()) {
        (Jv::String(s), Some(Jv::String(suffix))) => {
            ok(Jv::Bool(s.ends_with(suffix.as_str())))
        }
        _ => err("endswith requires string arguments".to_string()),
    }
}

fn builtin_split(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match (&input, args.first()) {
        (Jv::String(s), Some(Jv::String(sep))) => {
            let sep_str = sep.as_str();
            let parts: Vec<Jv> = if sep_str.is_empty() {
                // jq's split("") splits into individual characters without empty strings at edges
                s.as_str().chars().map(|c| Jv::string(c.to_string())).collect()
            } else {
                s.split(sep_str).into_iter().map(|p| Jv::String(p)).collect()
            };
            ok(Jv::from_vec(parts))
        }
        _ => err("split requires string arguments".to_string()),
    }
}

fn builtin_join(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match (&input, args.first()) {
        (Jv::Array(arr), Some(Jv::String(sep))) => {
            use crate::jv::{JvPrintOptions, print_jv_with_options};

            let mut result = String::new();
            let mut first = true;
            for v in arr.iter() {
                if !first {
                    result.push_str(sep.as_str());
                }
                first = false;
                match v {
                    Jv::String(s) => result.push_str(s.as_str()),
                    Jv::Null => {} // null adds nothing
                    _ => {
                        // Convert to string representation (tostring semantics)
                        let s = match &v {
                            Jv::Bool(b) => if *b { "true".to_string() } else { "false".to_string() },
                            Jv::Number(n) => format!("{}", n),
                            Jv::Array(_) | Jv::Object(_) => {
                                return err(format!("{} and {} cannot be added",
                                    format!("string (\"{}\")", result.replace('"', "\\\"")),
                                    format_value_for_join(&v)));
                            }
                            _ => {
                                let opts = JvPrintOptions::compact();
                                print_jv_with_options(&v, &opts)
                            }
                        };
                        result.push_str(&s);
                    }
                }
            }
            ok(Jv::string(result))
        }
        _ => err("join requires array and string separator".to_string()),
    }
}

fn format_value_for_join(v: &Jv) -> String {
    use crate::jv::{JvPrintOptions, print_jv_with_options};
    let opts = JvPrintOptions::compact();
    match v {
        Jv::Object(_) => format!("object ({})", print_jv_with_options(v, &opts)),
        Jv::Array(_) => format!("array ({})", print_jv_with_options(v, &opts)),
        _ => v.type_name().to_string(),
    }
}

fn builtin_has(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match (&input, args.first()) {
        (Jv::Object(o), Some(Jv::String(key))) => {
            ok(Jv::Bool(o.has(key.as_str())))
        }
        (Jv::Array(a), Some(Jv::Number(n))) => {
            if let Some(idx) = n.as_i64() {
                let len = a.len() as i64;
                let idx = if idx < 0 { len + idx } else { idx };
                ok(Jv::Bool(idx >= 0 && idx < len))
            } else {
                // nan, inf, or non-integer returns false
                ok(Jv::Bool(false))
            }
        }
        _ => err("has requires object/string or array/number".to_string()),
    }
}

fn builtin_in(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match (args.first(), &input) {
        (Some(Jv::Object(o)), Jv::String(key)) => {
            ok(Jv::Bool(o.has(key.as_str())))
        }
        (Some(Jv::Array(a)), Jv::Number(n)) => {
            if let Some(idx) = n.as_i64() {
                let len = a.len() as i64;
                let idx = if idx < 0 { len + idx } else { idx };
                ok(Jv::Bool(idx >= 0 && idx < len))
            } else {
                err("array index must be integer".to_string())
            }
        }
        _ => err("in requires appropriate arguments".to_string()),
    }
}

fn builtin_contains(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match args.first() {
        Some(b) => ok(Jv::Bool(jv_contains(&input, b))),
        None => err("contains requires an argument".to_string()),
    }
}

fn builtin_inside(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match args.first() {
        Some(b) => ok(Jv::Bool(jv_contains(b, &input))),
        None => err("inside requires an argument".to_string()),
    }
}

fn builtin_getpath(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match args.first() {
        Some(Jv::Array(path)) => {
            let mut current = input;
            for key in path.iter() {
                current = current.index(&key);
                if current.is_invalid() {
                    return ok(Jv::Null);
                }
            }
            ok(current)
        }
        _ => err("getpath requires array path".to_string()),
    }
}

// Helper functions

fn add_values(a: &Jv, b: &Jv) -> Jv {
    match (a, b) {
        (Jv::Null, _) => b.clone(),
        (_, Jv::Null) => a.clone(),
        (Jv::Number(n1), Jv::Number(n2)) => Jv::Number(n1.add(n2)),
        (Jv::String(s1), Jv::String(s2)) => Jv::String(s1.concat(s2)),
        (Jv::Array(a1), Jv::Array(a2)) => Jv::Array(a1.concat(a2)),
        (Jv::Object(o1), Jv::Object(o2)) => Jv::Object(o1.merge(o2)),
        _ => Jv::invalid(),
    }
}

fn jv_contains(a: &Jv, b: &Jv) -> bool {
    match (a, b) {
        (_, Jv::Null) => true,
        (Jv::Array(arr), Jv::Array(sub)) => {
            sub.iter().all(|item| arr.iter().any(|x| jv_contains(&x, &item)))
        }
        (Jv::Object(obj), Jv::Object(sub)) => {
            sub.iter().all(|(k, v)| {
                obj.get(&k).map_or(false, |ov| jv_contains(&ov, &v))
            })
        }
        (Jv::String(s), Jv::String(sub)) => s.contains(sub.as_str()),
        _ => a == b,
    }
}

// ============ Path functions ============

fn builtin_setpath(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    let (path, value) = match (args.get(0), args.get(1)) {
        (Some(Jv::Array(p)), Some(v)) => (p, v.clone()),
        _ => return err("setpath requires path array and value".to_string()),
    };

    fn set_at_path(current: Jv, path: &[Jv], value: Jv) -> Result<Jv, String> {
        if path.is_empty() {
            return Ok(value);
        }
        let key = &path[0];
        let rest = &path[1..];

        match key {
            Jv::String(s) => {
                let mut obj = match current {
                    Jv::Object(o) => o,
                    Jv::Null => crate::jv::JvObject::new(),
                    _ => return Err("cannot index non-object with string".to_string()),
                };
                let child = obj.get(s.as_str()).unwrap_or(Jv::Null);
                let new_child = set_at_path(child, rest, value)?;
                obj.set(s.as_str(), new_child);
                Ok(Jv::Object(obj))
            }
            Jv::Number(n) => {
                if let Some(idx) = n.as_i64() {
                    let mut arr = match current {
                        Jv::Array(a) => a,
                        Jv::Null => crate::jv::JvArray::new(),
                        Jv::Object(_) => return Err(format!("Cannot index object with number ({})", idx)),
                        _ => return Err(format!("Cannot index {} with number ({})", current.type_name(), idx)),
                    };
                    let normalized_idx = if idx < 0 { arr.len() as i64 + idx } else { idx };
                    let child = arr.get(normalized_idx).unwrap_or(Jv::Null);
                    let new_child = set_at_path(child, rest, value)?;
                    arr.set(normalized_idx, new_child)?;
                    Ok(Jv::Array(arr))
                } else {
                    Err("array index must be integer".to_string())
                }
            }
            Jv::Array(_) => Err("Cannot update field at array index of array".to_string()),
            _ => Err("path element must be string or number".to_string()),
        }
    }

    let path_vec: Vec<Jv> = path.iter().collect();
    match set_at_path(input, &path_vec, value) {
        Ok(v) => ok(v),
        Err(e) => err(e),
    }
}

fn builtin_delpaths(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    let paths = match args.first() {
        Some(Jv::Array(p)) => p,
        _ => return err("Paths must be specified as an array".to_string()),
    };

    // Collect all paths, sort by length (longest first) to delete leaf paths first
    let mut path_list: Vec<Vec<Jv>> = paths.iter()
        .filter_map(|p| match p {
            Jv::Array(arr) => Some(arr.iter().collect()),
            _ => None,
        })
        .collect();
    path_list.sort_by(|a, b| b.len().cmp(&a.len()));

    fn del_at_path(current: Jv, path: &[Jv]) -> Jv {
        if path.is_empty() {
            return Jv::Null;
        }
        if path.len() == 1 {
            match &path[0] {
                Jv::String(s) => {
                    if let Jv::Object(mut o) = current {
                        o.delete(s.as_str());
                        return Jv::Object(o);
                    }
                }
                Jv::Number(n) => {
                    if let (Jv::Array(mut a), Some(idx)) = (current.clone(), n.as_i64()) {
                        a.delete(idx);
                        return Jv::Array(a);
                    }
                }
                _ => {}
            }
            return current;
        }

        let key = &path[0];
        let rest = &path[1..];

        match key {
            Jv::String(s) => {
                if let Jv::Object(mut o) = current.clone() {
                    if let Some(child) = o.get(s.as_str()) {
                        let new_child = del_at_path(child, rest);
                        o.set(s.as_str(), new_child);
                        return Jv::Object(o);
                    }
                }
            }
            Jv::Number(n) => {
                if let (Jv::Array(mut a), Some(idx)) = (current.clone(), n.as_i64()) {
                    if let Some(child) = a.get(idx) {
                        let new_child = del_at_path(child, rest);
                        let _ = a.set(idx, new_child);
                        return Jv::Array(a);
                    }
                }
            }
            _ => {}
        }
        current
    }

    let mut result = input;
    for path in path_list {
        result = del_at_path(result, &path);
    }
    ok(result)
}

fn builtin_leaf_paths(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    fn collect_leaf_paths(v: &Jv, current_path: Vec<Jv>, paths: &mut Vec<Vec<Jv>>) {
        match v {
            Jv::Object(o) => {
                for (k, child) in o.iter() {
                    let mut new_path = current_path.clone();
                    new_path.push(Jv::string(k.clone()));
                    collect_leaf_paths(&child, new_path, paths);
                }
            }
            Jv::Array(a) => {
                for (i, child) in a.iter().enumerate() {
                    let mut new_path = current_path.clone();
                    new_path.push(Jv::from_i64(i as i64));
                    collect_leaf_paths(&child, new_path, paths);
                }
            }
            _ => {
                paths.push(current_path);
            }
        }
    }

    let mut paths = Vec::new();
    collect_leaf_paths(&input, Vec::new(), &mut paths);
    let jv_paths: Vec<Jv> = paths.into_iter().map(Jv::from_vec).collect();
    ok(Jv::from_vec(jv_paths))
}

fn builtin_path(_ctx: &mut Context, _input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    // path(expr) is special - it needs to be handled in the interpreter
    // This is a placeholder for the 1-arity version
    err("path(expr) must be handled by the interpreter".to_string())
}

fn builtin_paths(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    fn collect_paths(v: &Jv, current_path: Vec<Jv>, paths: &mut Vec<Vec<Jv>>) {
        match v {
            Jv::Object(o) => {
                for (k, child) in o.iter() {
                    let mut new_path = current_path.clone();
                    new_path.push(Jv::string(k.clone()));
                    paths.push(new_path.clone());
                    collect_paths(&child, new_path, paths);
                }
            }
            Jv::Array(a) => {
                for (i, child) in a.iter().enumerate() {
                    let mut new_path = current_path.clone();
                    new_path.push(Jv::from_i64(i as i64));
                    paths.push(new_path.clone());
                    collect_paths(&child, new_path, paths);
                }
            }
            _ => {}
        }
    }

    let mut paths = Vec::new();
    collect_paths(&input, Vec::new(), &mut paths);
    let jv_paths: Vec<Jv> = paths.into_iter().map(Jv::from_vec).collect();
    Box::new(jv_paths.into_iter().map(Ok))
}

// ============ Min/Max functions ============

fn builtin_min(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::Array(a) => {
            if a.is_empty() {
                return ok(Jv::Null);
            }
            let mut min = a.get(0).unwrap();
            for item in a.iter().skip(1) {
                if item < min {
                    min = item;
                }
            }
            ok(min)
        }
        _ => err(format!("{} has no min", input.type_name())),
    }
}

fn builtin_max(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::Array(a) => {
            if a.is_empty() {
                return ok(Jv::Null);
            }
            let mut max = a.get(0).unwrap();
            for item in a.iter().skip(1) {
                if item > max {
                    max = item;
                }
            }
            ok(max)
        }
        _ => err(format!("{} has no max", input.type_name())),
    }
}

// ============ Index functions ============

fn builtin_indices(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    let target = match args.first() {
        Some(t) => t,
        None => return err("indices requires an argument".to_string()),
    };

    match (&input, target) {
        (Jv::String(s), Jv::String(sub)) => {
            let haystack = s.as_str();
            let needle = sub.as_str();
            // jq returns empty array for empty needle
            if needle.is_empty() {
                return ok(Jv::from_vec(Vec::new()));
            }
            let mut indices = Vec::new();
            let mut byte_start = 0;
            while let Some(byte_pos) = haystack[byte_start..].find(needle) {
                // Convert byte position to character position
                let abs_byte_pos = byte_start + byte_pos;
                let char_pos = haystack[..abs_byte_pos].chars().count();
                indices.push(Jv::from_i64(char_pos as i64));
                // Move past the match (at least 1 byte for next search)
                byte_start = abs_byte_pos + 1;
                // Skip past full UTF-8 char boundaries if needed
                while byte_start < haystack.len() && !haystack.is_char_boundary(byte_start) {
                    byte_start += 1;
                }
            }
            ok(Jv::from_vec(indices))
        }
        (Jv::Array(a), Jv::Array(pattern)) => {
            // Search for subarray pattern
            let mut indices = Vec::new();
            let pattern_len = pattern.len();
            if pattern_len == 0 {
                return ok(Jv::from_vec(indices));
            }
            let arr_len = a.len();
            for i in 0..=arr_len.saturating_sub(pattern_len) {
                let mut matches = true;
                for j in 0..pattern_len {
                    if a.get((i + j) as i64) != pattern.get(j as i64) {
                        matches = false;
                        break;
                    }
                }
                if matches {
                    indices.push(Jv::from_i64(i as i64));
                }
            }
            ok(Jv::from_vec(indices))
        }
        (Jv::Array(a), _) => {
            // Search for single element
            let mut indices = Vec::new();
            for (i, item) in a.iter().enumerate() {
                if &item == target {
                    indices.push(Jv::from_i64(i as i64));
                }
            }
            ok(Jv::from_vec(indices))
        }
        _ => err("indices requires string or array".to_string()),
    }
}

fn builtin_index(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    let target = match args.first() {
        Some(t) => t,
        None => return err("index requires an argument".to_string()),
    };

    match (&input, target) {
        (Jv::String(s), Jv::String(sub)) => {
            let haystack = s.as_str();
            let needle = sub.as_str();
            // jq returns null for empty needle
            if needle.is_empty() {
                return ok(Jv::Null);
            }
            match haystack.find(needle) {
                Some(byte_pos) => {
                    // Convert byte position to character position
                    let char_pos = haystack[..byte_pos].chars().count();
                    ok(Jv::from_i64(char_pos as i64))
                }
                None => ok(Jv::Null),
            }
        }
        (Jv::Array(a), _) => {
            for (i, item) in a.iter().enumerate() {
                if &item == target {
                    return ok(Jv::from_i64(i as i64));
                }
            }
            ok(Jv::Null)
        }
        _ => err("index requires string or array".to_string()),
    }
}

fn builtin_rindex(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    let target = match args.first() {
        Some(t) => t,
        None => return err("rindex requires an argument".to_string()),
    };

    match (&input, target) {
        (Jv::String(s), Jv::String(sub)) => {
            let haystack = s.as_str();
            let needle = sub.as_str();
            // jq returns null for empty needle
            if needle.is_empty() {
                return ok(Jv::Null);
            }
            match haystack.rfind(needle) {
                Some(byte_pos) => {
                    // Convert byte position to character position
                    let char_pos = haystack[..byte_pos].chars().count();
                    ok(Jv::from_i64(char_pos as i64))
                }
                None => ok(Jv::Null),
            }
        }
        (Jv::Array(a), _) => {
            // Collect into a vec first to support reverse iteration
            let items: Vec<(usize, Jv)> = a.iter().enumerate().collect();
            for (i, item) in items.into_iter().rev() {
                if &item == target {
                    return ok(Jv::from_i64(i as i64));
                }
            }
            ok(Jv::Null)
        }
        _ => err("rindex requires string or array".to_string()),
    }
}

// ============ Object functions ============

fn builtin_to_entries(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::Object(o) => {
            let entries: Vec<Jv> = o.iter().map(|(k, v)| {
                let mut entry = crate::jv::JvObject::new();
                entry.set("key", Jv::string(k));
                entry.set("value", v);
                Jv::Object(entry)
            }).collect();
            ok(Jv::from_vec(entries))
        }
        _ => err(format!("{} cannot be converted to entries", input.type_name())),
    }
}

fn builtin_from_entries(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::Array(a) => {
            let mut obj = crate::jv::JvObject::new();
            for entry in a.iter() {
                if let Jv::Object(e) = entry {
                    // Support both {key, value}, {Key, Value}, {name, value}, {Name, Value}, {k, v}
                    let key = e.get("key")
                        .or_else(|| e.get("Key"))
                        .or_else(|| e.get("name"))
                        .or_else(|| e.get("Name"))
                        .or_else(|| e.get("k"));
                    let value = e.get("value")
                        .or_else(|| e.get("Value"))
                        .or_else(|| e.get("v"))
                        .unwrap_or(Jv::Null);

                    if let Some(Jv::String(k)) = key {
                        obj.set(k.as_str(), value);
                    } else if let Some(Jv::Number(n)) = key {
                        obj.set(&format!("{}", n), value);
                    }
                }
            }
            ok(Jv::Object(obj))
        }
        _ => err(format!("{} cannot be converted from entries", input.type_name())),
    }
}

fn builtin_with_entries(_ctx: &mut Context, _input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    // with_entries(f) is defined as to_entries | map(f) | from_entries
    // This needs special handling in the interpreter
    err("with_entries must be handled by the interpreter".to_string())
}

fn builtin_del(_ctx: &mut Context, _input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    // del(path) is special - needs interpreter support
    err("del(path) must be handled by the interpreter".to_string())
}

fn builtin_env(_ctx: &mut Context, _input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    let mut obj = crate::jv::JvObject::new();
    for (key, value) in std::env::vars() {
        obj.set(&key, Jv::string(value));
    }
    ok(Jv::Object(obj))
}

// ============ Type checking functions ============

fn builtin_infinite(_ctx: &mut Context, _input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    ok(Jv::from_f64(f64::INFINITY))
}

fn builtin_nan(_ctx: &mut Context, _input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    ok(Jv::from_f64(f64::NAN))
}

fn builtin_isinfinite(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match input.as_f64() {
        Some(n) => ok(Jv::Bool(n.is_infinite())),
        None => ok(Jv::Bool(false)),
    }
}

fn builtin_isnan(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match input.as_f64() {
        Some(n) => ok(Jv::Bool(n.is_nan())),
        None => ok(Jv::Bool(false)),
    }
}

fn builtin_isnormal(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match input.as_f64() {
        Some(n) => ok(Jv::Bool(n.is_normal())),
        None => ok(Jv::Bool(false)),
    }
}

fn builtin_isfinite(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match input.as_f64() {
        Some(n) => ok(Jv::Bool(n.is_finite())),
        None => ok(Jv::Bool(false)),
    }
}

// Type selectors - return input if type matches, otherwise empty
fn builtin_arrays(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::Array(_) => ok(input),
        _ => Box::new(std::iter::empty()),
    }
}

fn builtin_objects(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::Object(_) => ok(input),
        _ => Box::new(std::iter::empty()),
    }
}

fn builtin_iterables(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::Array(_) | Jv::Object(_) => ok(input),
        _ => Box::new(std::iter::empty()),
    }
}

fn builtin_booleans(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::Bool(_) => ok(input),
        _ => Box::new(std::iter::empty()),
    }
}

fn builtin_numbers(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::Number(_) => ok(input),
        _ => Box::new(std::iter::empty()),
    }
}

fn builtin_strings(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::String(_) => ok(input),
        _ => Box::new(std::iter::empty()),
    }
}

fn builtin_nulls(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::Null => ok(input),
        _ => Box::new(std::iter::empty()),
    }
}

fn builtin_scalars(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::Array(_) | Jv::Object(_) => Box::new(std::iter::empty()),
        _ => ok(input),
    }
}

// ============ More math functions ============

fn builtin_log(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match input.as_f64() {
        Some(n) => ok(Jv::from_f64(n.ln())),
        None => err(format!("{} has no log", input.type_name())),
    }
}

fn builtin_log10(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match input.as_f64() {
        Some(n) => ok(Jv::from_f64(n.log10())),
        None => err(format!("{} has no log10", input.type_name())),
    }
}

fn builtin_log2(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match input.as_f64() {
        Some(n) => ok(Jv::from_f64(n.log2())),
        None => err(format!("{} has no log2", input.type_name())),
    }
}

fn builtin_exp(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match input.as_f64() {
        Some(n) => ok(Jv::from_f64(n.exp())),
        None => err(format!("{} has no exp", input.type_name())),
    }
}

fn builtin_exp10(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match input.as_f64() {
        Some(n) => ok(Jv::from_f64(10.0_f64.powf(n))),
        None => err(format!("{} has no exp10", input.type_name())),
    }
}

fn builtin_exp2(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match input.as_f64() {
        Some(n) => ok(Jv::from_f64(n.exp2())),
        None => err(format!("{} has no exp2", input.type_name())),
    }
}

fn builtin_pow(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match (input.as_f64(), args.first().and_then(|v| v.as_f64())) {
        (Some(base), Some(exp)) => ok(Jv::from_f64(base.powf(exp))),
        _ => err("pow requires number arguments".to_string()),
    }
}

fn builtin_pow2(_ctx: &mut Context, _input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    // pow(base; exp) - two argument form
    match (args.first().and_then(|v| v.as_f64()), args.get(1).and_then(|v| v.as_f64())) {
        (Some(base), Some(exp)) => ok(Jv::from_f64(base.powf(exp))),
        _ => err("pow requires number arguments".to_string()),
    }
}

fn builtin_sin(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match input.as_f64() {
        Some(n) => ok(Jv::from_f64(n.sin())),
        None => err(format!("{} has no sin", input.type_name())),
    }
}

fn builtin_cos(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match input.as_f64() {
        Some(n) => ok(Jv::from_f64(n.cos())),
        None => err(format!("{} has no cos", input.type_name())),
    }
}

fn builtin_tan(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match input.as_f64() {
        Some(n) => ok(Jv::from_f64(n.tan())),
        None => err(format!("{} has no tan", input.type_name())),
    }
}

fn builtin_asin(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match input.as_f64() {
        Some(n) => ok(Jv::from_f64(n.asin())),
        None => err(format!("{} has no asin", input.type_name())),
    }
}

fn builtin_acos(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match input.as_f64() {
        Some(n) => ok(Jv::from_f64(n.acos())),
        None => err(format!("{} has no acos", input.type_name())),
    }
}

fn builtin_atan(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match input.as_f64() {
        Some(n) => ok(Jv::from_f64(n.atan())),
        None => err(format!("{} has no atan", input.type_name())),
    }
}

// ============ Regex functions (basic implementations) ============

fn builtin_test(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    let pattern = match args.first() {
        Some(Jv::String(s)) => s.as_str(),
        _ => return err("test requires string pattern".to_string()),
    };

    match &input {
        Jv::String(s) => {
            match regex::Regex::new(pattern) {
                Ok(re) => ok(Jv::Bool(re.is_match(s.as_str()))),
                Err(e) => err(format!("invalid regex: {}", e)),
            }
        }
        _ => err("test requires string input".to_string()),
    }
}

fn builtin_match(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    let pattern = match args.first() {
        Some(Jv::String(s)) => s.as_str(),
        _ => return err("match requires string pattern".to_string()),
    };

    match &input {
        Jv::String(s) => {
            match regex::Regex::new(pattern) {
                Ok(re) => {
                    if let Some(m) = re.find(s.as_str()) {
                        let mut obj = crate::jv::JvObject::new();
                        obj.set("offset", Jv::from_i64(m.start() as i64));
                        obj.set("length", Jv::from_i64(m.len() as i64));
                        obj.set("string", Jv::string(m.as_str()));

                        // Capture groups
                        if let Some(caps) = re.captures(s.as_str()) {
                            let captures: Vec<Jv> = caps.iter().skip(1)
                                .map(|c| match c {
                                    Some(m) => {
                                        let mut g = crate::jv::JvObject::new();
                                        g.set("offset", Jv::from_i64(m.start() as i64));
                                        g.set("length", Jv::from_i64(m.len() as i64));
                                        g.set("string", Jv::string(m.as_str()));
                                        g.set("name", Jv::Null);
                                        Jv::Object(g)
                                    }
                                    None => Jv::Null,
                                })
                                .collect();
                            obj.set("captures", Jv::from_vec(captures));
                        } else {
                            obj.set("captures", Jv::from_vec(vec![]));
                        }

                        ok(Jv::Object(obj))
                    } else {
                        ok(Jv::Null)
                    }
                }
                Err(e) => err(format!("invalid regex: {}", e)),
            }
        }
        _ => err("match requires string input".to_string()),
    }
}

fn builtin_capture(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    let pattern = match args.first() {
        Some(Jv::String(s)) => s.as_str(),
        _ => return err("capture requires string pattern".to_string()),
    };

    match &input {
        Jv::String(s) => {
            match regex::Regex::new(pattern) {
                Ok(re) => {
                    if let Some(caps) = re.captures(s.as_str()) {
                        let mut obj = crate::jv::JvObject::new();
                        for name in re.capture_names().flatten() {
                            if let Some(m) = caps.name(name) {
                                obj.set(name, Jv::string(m.as_str()));
                            }
                        }
                        ok(Jv::Object(obj))
                    } else {
                        ok(Jv::Object(crate::jv::JvObject::new()))
                    }
                }
                Err(e) => err(format!("invalid regex: {}", e)),
            }
        }
        _ => err("capture requires string input".to_string()),
    }
}

fn builtin_splits(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    let pattern = match args.first() {
        Some(Jv::String(s)) => s.as_str().to_string(),
        _ => return err("splits requires string pattern".to_string()),
    };

    match &input {
        Jv::String(s) => {
            match regex::Regex::new(&pattern) {
                Ok(re) => {
                    let parts: Vec<Jv> = re.split(s.as_str())
                        .map(|p| Jv::string(p))
                        .collect();
                    Box::new(parts.into_iter().map(Ok))
                }
                Err(e) => err(format!("invalid regex: {}", e)),
            }
        }
        _ => err("splits requires string input".to_string()),
    }
}

fn builtin_sub(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    let (pattern, replacement) = match (args.get(0), args.get(1)) {
        (Some(Jv::String(p)), Some(Jv::String(r))) => (p.as_str(), r.as_str()),
        _ => return err("sub requires pattern and replacement strings".to_string()),
    };

    match &input {
        Jv::String(s) => {
            match regex::Regex::new(pattern) {
                Ok(re) => {
                    let result = re.replace(s.as_str(), replacement);
                    ok(Jv::string(result.into_owned()))
                }
                Err(e) => err(format!("invalid regex: {}", e)),
            }
        }
        _ => err("sub requires string input".to_string()),
    }
}

fn builtin_gsub(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    let (pattern, replacement) = match (args.get(0), args.get(1)) {
        (Some(Jv::String(p)), Some(Jv::String(r))) => (p.as_str(), r.as_str()),
        _ => return err("gsub requires pattern and replacement strings".to_string()),
    };

    match &input {
        Jv::String(s) => {
            match regex::Regex::new(pattern) {
                Ok(re) => {
                    let result = re.replace_all(s.as_str(), replacement);
                    ok(Jv::string(result.into_owned()))
                }
                Err(e) => err(format!("invalid regex: {}", e)),
            }
        }
        _ => err("gsub requires string input".to_string()),
    }
}

fn builtin_scan(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    let pattern = match args.first() {
        Some(Jv::String(p)) => p.as_str().to_string(),
        _ => return err("scan requires pattern string".to_string()),
    };

    match &input {
        Jv::String(s) => {
            let s_str = s.as_str().to_string();
            match regex::Regex::new(&pattern) {
                Ok(re) => {
                    let matches: Vec<_> = re.find_iter(&s_str)
                        .map(|m| Ok(Jv::string(m.as_str())))
                        .collect();
                    Box::new(matches.into_iter())
                }
                Err(e) => err(format!("invalid regex: {}", e)),
            }
        }
        _ => err("scan requires string input".to_string()),
    }
}

// ============ Additional array/string functions ============

fn builtin_bsearch(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    let target = match args.first() {
        Some(t) => t,
        None => return err("bsearch requires an argument".to_string()),
    };

    match &input {
        Jv::Array(a) => {
            // Binary search - assumes sorted array
            let items: Vec<Jv> = a.iter().collect();
            let mut lo = 0i64;
            let mut hi = items.len() as i64;

            while lo < hi {
                let mid = lo + (hi - lo) / 2;
                if &items[mid as usize] < target {
                    lo = mid + 1;
                } else {
                    hi = mid;
                }
            }

            // Check if found
            if (lo as usize) < items.len() && &items[lo as usize] == target {
                ok(Jv::from_i64(lo))
            } else {
                // Return negative insertion point minus 1
                ok(Jv::from_i64(-lo - 1))
            }
        }
        _ => {
            use crate::jv::print_jv;
            err(format!("{} ({}) cannot be searched from", input.type_name(), print_jv(&input)))
        }
    }
}

fn builtin_explode(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::String(s) => {
            let codepoints: Vec<Jv> = s.as_str().chars()
                .map(|c| Jv::from_i64(c as i64))
                .collect();
            ok(Jv::from_vec(codepoints))
        }
        _ => err("explode requires string input".to_string()),
    }
}

fn builtin_implode(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    const REPLACEMENT_CHAR: char = '\u{FFFD}'; // Unicode replacement character

    match &input {
        Jv::Array(a) => {
            let mut result = String::new();
            for item in a.iter() {
                if let Some(n_f64) = item.as_f64() {
                    // jq truncates floats to integers (floor)
                    let n = n_f64.floor() as i64;

                    // Check for valid Unicode codepoint
                    // Invalid if: negative, > 0x10FFFF, or in surrogate pair range 0xD800-0xDFFF
                    let cp = n as u32;
                    if n < 0 || n > 0x10FFFF || (0xD800..=0xDFFF).contains(&cp) {
                        result.push(REPLACEMENT_CHAR);
                    } else if let Some(c) = char::from_u32(cp) {
                        result.push(c);
                    } else {
                        result.push(REPLACEMENT_CHAR);
                    }
                } else {
                    return err("implode requires array of integers".to_string());
                }
            }
            ok(Jv::string(result))
        }
        _ => err("implode input must be an array".to_string()),
    }
}

fn builtin_ascii(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::String(s) => {
            let s = s.as_str();
            if s.is_empty() {
                return ok(Jv::Null);
            }
            ok(Jv::from_i64(s.chars().next().unwrap() as i64))
        }
        _ => err("ascii requires string input".to_string()),
    }
}

fn builtin_utf8bytelength(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::String(s) => {
            ok(Jv::from_i64(s.as_str().len() as i64))
        }
        _ => {
            // Format error message to match jq's format
            use crate::jv::print_jv;
            err(format!("{} ({}) only strings have UTF-8 byte length", input.type_name(), print_jv(&input)))
        }
    }
}

fn builtin_tojson(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    use crate::jv::print_jv;
    ok(Jv::string(print_jv(&input)))
}

fn builtin_fromjson(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::String(s) => {
            use crate::jv::parse_json;
            let str_val = s.as_str();
            // jq accepts "nan", "NaN", "-NaN" as special values for NaN
            if str_val == "nan" || str_val == "NaN" || str_val == "-NaN" {
                return ok(Jv::from_f64(f64::NAN));
            }
            match parse_json(str_val) {
                Ok(v) => ok(v),
                Err(e) => err(format!("invalid JSON: {}", e)),
            }
        }
        _ => err("fromjson requires string input".to_string()),
    }
}

// Higher-order functions that need special handling but have arity 1
fn builtin_group_by(_ctx: &mut Context, _input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    err("group_by must be handled by the interpreter".to_string())
}

fn builtin_unique_by(_ctx: &mut Context, _input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    err("unique_by must be handled by the interpreter".to_string())
}

fn builtin_sort_by(_ctx: &mut Context, _input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    err("sort_by must be handled by the interpreter".to_string())
}

fn builtin_max_by(_ctx: &mut Context, _input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    err("max_by must be handled by the interpreter".to_string())
}

fn builtin_min_by(_ctx: &mut Context, _input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    err("min_by must be handled by the interpreter".to_string())
}

// ============ Format functions ============

fn builtin_base64(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::String(s) => {
            ok(Jv::string(crate::builtins::format::base64_encode(s.as_str())))
        }
        _ => err("@base64 requires string input".to_string()),
    }
}

fn builtin_base64d(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::String(s) => {
            match crate::builtins::format::base64_decode(s.as_str()) {
                Ok(decoded) => ok(Jv::string(decoded)),
                Err(e) => err(e),
            }
        }
        _ => err("@base64d requires string input".to_string()),
    }
}

fn builtin_uri(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::String(s) => {
            ok(Jv::string(crate::builtins::format::uri_encode(s.as_str())))
        }
        _ => err("@uri requires string input".to_string()),
    }
}

fn builtin_urid(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::String(s) => {
            match crate::builtins::format::uri_decode(s.as_str()) {
                Ok(decoded) => ok(Jv::string(decoded)),
                Err(e) => err(e),
            }
        }
        _ => err("@urid requires string input".to_string()),
    }
}

fn builtin_csv(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::Array(a) => {
            let items: Vec<Jv> = a.iter().collect();
            ok(Jv::string(crate::builtins::format::to_csv(&items)))
        }
        _ => err("@csv requires array input".to_string()),
    }
}

fn builtin_tsv(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::Array(a) => {
            let items: Vec<Jv> = a.iter().collect();
            ok(Jv::string(crate::builtins::format::to_tsv(&items)))
        }
        _ => err("@tsv requires array input".to_string()),
    }
}

fn builtin_html(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::String(s) => {
            ok(Jv::string(crate::builtins::format::html_escape(s.as_str())))
        }
        _ => err("@html requires string input".to_string()),
    }
}

fn builtin_sh(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::String(s) => {
            ok(Jv::string(crate::builtins::format::sh_escape(s.as_str())))
        }
        _ => err("@sh requires string input".to_string()),
    }
}

fn builtin_json(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    ok(Jv::string(crate::builtins::format::to_json(&input)))
}

fn builtin_text(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    ok(Jv::string(crate::builtins::format::to_text(&input)))
}
