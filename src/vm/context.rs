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

        // Math functions
        self.register("floor", 0, builtin_floor);
        self.register("ceil", 0, builtin_ceil);
        self.register("round", 0, builtin_round);
        self.register("sqrt", 0, builtin_sqrt);
        self.register("fabs", 0, builtin_fabs);

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

        // Array functions
        self.register("has", 1, builtin_has);
        self.register("in", 1, builtin_in);
        self.register("contains", 1, builtin_contains);
        self.register("inside", 1, builtin_inside);
        self.register("getpath", 1, builtin_getpath);

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
