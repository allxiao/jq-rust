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
    /// A filter/function binding (func arg)
    Filter(Rc<FuncDef>),
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
        self.register("modulemeta", 1, builtin_modulemeta);
        self.register("getpath", 1, builtin_getpath);
        self.register("delpaths", 1, builtin_delpaths);

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
        self.register("ascii_downcase", 0, builtin_ascii_downcase);
        self.register("ascii_upcase", 0, builtin_ascii_upcase);
        self.register("ltrimstr", 1, builtin_ltrimstr);
        self.register("rtrimstr", 1, builtin_rtrimstr);
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

    /// Bind a value to a variable name
    pub fn bind_value(&mut self, name: &str, value: Jv) {
        self.bindings.insert(name.to_string(), Binding::Value(value));
    }

    /// Bind a function definition
    pub fn bind_function(&mut self, name: &str, def: Rc<FuncDef>) {
        self.bindings.insert(name.to_string(), Binding::Filter(def));
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

    /// Look up a value binding
    pub fn lookup_value(&self, name: &str) -> Option<Jv> {
        match self.lookup(name) {
            Some(Binding::Value(v)) => Some(v),
            _ => None,
        }
    }

    /// Look up a function binding
    pub fn lookup_function(&self, name: &str) -> Option<Rc<FuncDef>> {
        match self.lookup(name) {
            Some(Binding::Filter(f)) => Some(f),
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
    match &input {
        Jv::Object(o) => {
            let vals: Vec<_> = o.values().collect();
            Box::new(vals.into_iter().map(Ok))
        }
        Jv::Array(a) => {
            let vals: Vec<_> = a.iter().collect();
            Box::new(vals.into_iter().map(Ok))
        }
        _ => err(format!("{} has no values", input.type_name())),
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
        _ => return err("flatten depth must be a non-negative integer".to_string()),
    };
    match &input {
        Jv::Array(a) => ok(Jv::Array(a.flatten(Some(depth)))),
        _ => err(format!("{} cannot be flattened", input.type_name())),
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

fn builtin_error(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match input.as_str() {
        Some(s) => err(s.to_string()),
        None => err(format!("{}", input)),
    }
}

fn builtin_error_msg(_ctx: &mut Context, _input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match args.first().and_then(|v| v.as_str()) {
        Some(s) => err(s.to_string()),
        None => err(args.first().map(|v| format!("{}", v)).unwrap_or_default()),
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

fn builtin_builtins(_ctx: &mut Context, _input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    // Return list of builtin function names - simplified version
    let builtins = vec![
        "empty", "null", "true", "false", "not", "type", "length", "keys", "values",
        "add", "reverse", "sort", "unique", "flatten", "first", "last", "nth",
        "floor", "ceil", "round", "sqrt", "abs", "min", "max",
        "map", "select", "recurse", "range", "limit", "group_by", "sort_by", "unique_by",
        "tostring", "tonumber", "split", "join", "test", "match", "sub", "gsub",
        "has", "in", "contains", "inside", "getpath", "setpath", "delpaths", "del",
        "to_entries", "from_entries", "keys_unsorted", "error", "debug",
        "@base64", "@base64d", "@uri", "@csv", "@tsv", "@html", "@sh", "@json", "@text",
    ];
    let arr: Vec<Jv> = builtins.iter().map(|s| Jv::string(*s)).collect();
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
    match input.as_number() {
        Some(n) => ok(Jv::Number(n.abs())),
        None => err(format!("{} has no absolute value", input.type_name())),
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
            match s.as_str().parse::<f64>() {
                Ok(n) => ok(Jv::from_f64(n)),
                Err(_) => err(format!("cannot parse '{}' as number", s.as_str())),
            }
        }
        _ => err(format!("{} cannot be parsed as number", input.type_name())),
    }
}

fn builtin_ascii_downcase(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::String(s) => ok(Jv::String(s.to_lowercase())),
        _ => err(format!("{} has no ascii_downcase", input.type_name())),
    }
}

fn builtin_ascii_upcase(_ctx: &mut Context, input: Jv, _args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match &input {
        Jv::String(s) => ok(Jv::String(s.to_uppercase())),
        _ => err(format!("{} has no ascii_upcase", input.type_name())),
    }
}

fn builtin_ltrimstr(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match (&input, args.first()) {
        (Jv::String(s), Some(Jv::String(prefix))) => {
            ok(Jv::String(s.ltrimstr(prefix.as_str())))
        }
        _ => err("ltrimstr requires string arguments".to_string()),
    }
}

fn builtin_rtrimstr(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match (&input, args.first()) {
        (Jv::String(s), Some(Jv::String(suffix))) => {
            ok(Jv::String(s.rtrimstr(suffix.as_str())))
        }
        _ => err("rtrimstr requires string arguments".to_string()),
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
            let parts: Vec<Jv> = s.split(sep.as_str()).into_iter().map(|p| Jv::String(p)).collect();
            ok(Jv::from_vec(parts))
        }
        _ => err("split requires string arguments".to_string()),
    }
}

fn builtin_join(_ctx: &mut Context, input: Jv, args: &[Jv]) -> Box<dyn Iterator<Item = Result<Jv, String>>> {
    match (&input, args.first()) {
        (Jv::Array(arr), Some(Jv::String(sep))) => {
            let strings: Result<Vec<String>, _> = arr.iter()
                .map(|v| match v {
                    Jv::String(s) => Ok(s.as_str().to_string()),
                    Jv::Null => Ok(String::new()),
                    _ => Err(format!("cannot join {}", v.type_name())),
                })
                .collect();
            match strings {
                Ok(ss) => ok(Jv::string(ss.join(sep.as_str()))),
                Err(e) => err(e),
            }
        }
        _ => err("join requires array and string separator".to_string()),
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
                err("array index must be integer".to_string())
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
                        _ => return Err("cannot index non-array with number".to_string()),
                    };
                    let normalized_idx = if idx < 0 { arr.len() as i64 + idx } else { idx };
                    let child = arr.get(normalized_idx).unwrap_or(Jv::Null);
                    let new_child = set_at_path(child, rest, value)?;
                    arr.set(normalized_idx, new_child);
                    Ok(Jv::Array(arr))
                } else {
                    Err("array index must be integer".to_string())
                }
            }
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
        _ => return err("delpaths requires array of paths".to_string()),
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
                        a.set(idx, new_child);
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
                return err("min requires non-empty array".to_string());
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
                return err("max requires non-empty array".to_string());
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
            let mut indices = Vec::new();
            let mut start = 0;
            while let Some(pos) = haystack[start..].find(needle) {
                indices.push(Jv::from_i64((start + pos) as i64));
                start = start + pos + 1;
            }
            ok(Jv::from_vec(indices))
        }
        (Jv::Array(a), _) => {
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
            match s.as_str().find(sub.as_str()) {
                Some(pos) => ok(Jv::from_i64(pos as i64)),
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
            match s.as_str().rfind(sub.as_str()) {
                Some(pos) => ok(Jv::from_i64(pos as i64)),
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
                    // Support both {key, value} and {name, value} and {k, v}
                    let key = e.get("key")
                        .or_else(|| e.get("name"))
                        .or_else(|| e.get("k"));
                    let value = e.get("value")
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
        _ => err("bsearch requires array input".to_string()),
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
    match &input {
        Jv::Array(a) => {
            let mut result = String::new();
            for item in a.iter() {
                if let Some(n) = item.as_i64() {
                    if let Some(c) = char::from_u32(n as u32) {
                        result.push(c);
                    } else {
                        return err(format!("invalid codepoint: {}", n));
                    }
                } else {
                    return err("implode requires array of integers".to_string());
                }
            }
            ok(Jv::string(result))
        }
        _ => err("implode requires array input".to_string()),
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
        _ => err("utf8bytelength requires string input".to_string()),
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
            match parse_json(s.as_str()) {
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
