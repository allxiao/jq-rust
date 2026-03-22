//! Module loading for jq
//!
//! Handles import/include statements and module resolution.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::jv::{parse_json_stream, Jv};
use crate::parser::{parse_program_full, Expr, ExprKind, FuncDef, Import, Literal, ObjectKey};
use crate::vm::Context;

/// Module loader and cache
pub struct ModuleLoader {
    /// Search paths for modules
    search_paths: Vec<PathBuf>,
    /// Cache of loaded module contents (path -> source code)
    module_cache: HashMap<String, LoadedModule>,
}

// Thread-local module search path override
thread_local! {
    static MODULE_SEARCH_PATH: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// Set the module search path for the current thread
pub fn set_module_search_path(path: Option<PathBuf>) {
    MODULE_SEARCH_PATH.with(|p| {
        *p.borrow_mut() = path;
    });
}

/// Get the module search path for the current thread
pub fn get_module_search_path() -> Option<PathBuf> {
    MODULE_SEARCH_PATH.with(|p| p.borrow().clone())
}

/// A loaded module
#[derive(Debug, Clone)]
pub struct LoadedModule {
    /// Module path
    pub path: String,
    /// Module metadata (from `module { ... }`)
    pub metadata: Option<Expr>,
    /// Function definitions
    pub defs: Vec<FuncDef>,
    /// Imports this module depends on
    pub imports: Vec<Import>,
}

impl ModuleLoader {
    /// Create a new module loader with default search paths
    pub fn new() -> Self {
        let mut search_paths = Vec::new();

        // Check for thread-local override first
        if let Some(path) = get_module_search_path() {
            search_paths.push(path);
        }

        // Add current directory
        search_paths.push(PathBuf::from("."));

        // Add home directory .jq if it exists
        if let Some(home) = dirs::home_dir() {
            search_paths.push(home.join(".jq"));
        }

        ModuleLoader {
            search_paths,
            module_cache: HashMap::new(),
        }
    }

    /// Add a search path
    pub fn add_search_path(&mut self, path: PathBuf) {
        self.search_paths.insert(0, path);
    }

    /// Set search paths from a directory (for testing in a specific location)
    pub fn set_base_path(&mut self, base: PathBuf) {
        self.search_paths.insert(0, base);
    }

    /// Find a module file given a relative path
    fn find_module(&self, rel_path: &str, suffix: &str) -> Option<PathBuf> {
        for search_dir in &self.search_paths {
            // Try {search_dir}/{rel_path}.jq
            let p1 = search_dir.join(format!("{}{}", rel_path, suffix));
            if p1.exists() && p1.is_file() {
                return Some(p1);
            }

            // Try {search_dir}/{rel_path}/jq/main.jq
            let p2 = search_dir
                .join(rel_path)
                .join("jq")
                .join(format!("main{}", suffix));
            if p2.exists() && p2.is_file() {
                return Some(p2);
            }

            // Try {search_dir}/{rel_path}/{basename}.jq
            if let Some(basename) = Path::new(rel_path).file_name() {
                let p3 = search_dir.join(rel_path).join(format!(
                    "{}{}",
                    basename.to_string_lossy(),
                    suffix
                ));
                if p3.exists() && p3.is_file() {
                    return Some(p3);
                }
            }
        }
        None
    }

    /// Load a code module (.jq file)
    pub fn load_code_module(&mut self, rel_path: &str) -> Result<LoadedModule, String> {
        // Check cache first
        if let Some(module) = self.module_cache.get(rel_path) {
            return Ok(module.clone());
        }

        // Find the module file
        let file_path = self
            .find_module(rel_path, ".jq")
            .ok_or_else(|| format!("module not found: {}", rel_path))?;

        // Read and parse
        let source = fs::read_to_string(&file_path)
            .map_err(|e| format!("error reading module {}: {}", rel_path, e))?;

        let program = parse_program_full(&source)
            .map_err(|e| format!("error parsing module {}: {}", rel_path, e))?;

        let loaded = LoadedModule {
            path: rel_path.to_string(),
            metadata: program.module,
            defs: program.defs,
            imports: program.imports,
        };

        // Cache it
        self.module_cache
            .insert(rel_path.to_string(), loaded.clone());

        Ok(loaded)
    }

    /// Load a data module (.json file)
    pub fn load_data_module(&mut self, rel_path: &str) -> Result<Jv, String> {
        // Find the module file
        let file_path = self
            .find_module(rel_path, ".json")
            .ok_or_else(|| format!("data module not found: {}", rel_path))?;

        // Read and parse JSON
        let source = fs::read_to_string(&file_path)
            .map_err(|e| format!("error reading data module {}: {}", rel_path, e))?;

        // Parse JSON - jq always wraps data in an array
        let values: Vec<Jv> = parse_json_stream(&source)
            .map(|r| r.map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?;

        // jq always wraps data imports in an array, even for a single value
        Ok(Jv::from_vec(values))
    }

    /// Process imports and bind them to the context
    pub fn process_imports(
        &mut self,
        imports: &[Import],
        ctx: &Rc<RefCell<Context>>,
    ) -> Result<(), String> {
        self.process_imports_with_origin(imports, ctx, None)
    }

    /// Process imports with an origin directory for relative path resolution
    fn process_imports_with_origin(
        &mut self,
        imports: &[Import],
        ctx: &Rc<RefCell<Context>>,
        origin_dir: Option<&Path>,
    ) -> Result<(), String> {
        for import in imports {
            // Check if import has metadata with search path
            let extra_search_path: Option<PathBuf> = if let Some(ref metadata) = import.metadata {
                // Try to extract {search: "path"} from metadata
                self.extract_search_path(metadata, origin_dir)
            } else {
                None
            };

            // Temporarily add the extra search path if present
            if let Some(ref path) = extra_search_path {
                self.search_paths.insert(0, path.clone());
            }

            let result = if import.is_data {
                // Data import: import "path" as $var
                let data = self.load_data_module(&import.path)?;
                let var_name = import
                    .alias
                    .as_ref()
                    .ok_or_else(|| "data import requires alias".to_string())?;

                // Bind as $var and $var::var
                ctx.borrow_mut().bind_value(var_name, data.clone());
                ctx.borrow_mut().bind_module_data(var_name, var_name, data);
                Ok(())
            } else if import.is_include {
                // Include: include "path"
                self.load_and_bind_module(&import.path, None, ctx, origin_dir)
            } else {
                // Import: import "path" as name
                let alias = import
                    .alias
                    .as_ref()
                    .ok_or_else(|| "import requires alias".to_string())?;
                self.load_and_bind_module(&import.path, Some(alias), ctx, origin_dir)
            };

            // Remove the temporary search path
            if extra_search_path.is_some() {
                self.search_paths.remove(0);
            }

            result?;
        }
        Ok(())
    }

    /// Load a module and bind its definitions
    fn load_and_bind_module(
        &mut self,
        path: &str,
        alias: Option<&str>,
        ctx: &Rc<RefCell<Context>>,
        _origin_dir: Option<&Path>,
    ) -> Result<(), String> {
        // Find the module file to get its directory for nested imports
        let module_file = self
            .find_module(path, ".jq")
            .ok_or_else(|| format!("module not found: {}", path))?;
        let module_dir = module_file.parent().map(|p| p.to_path_buf());

        let module = self.load_code_module(path)?;

        // Create a child context for this module's imports
        // This context will be used as the closure context for the module's functions
        let module_ctx = Rc::new(RefCell::new(Context::child(ctx.clone())));

        // Process the module's own imports (recursively) into the module's context
        self.process_imports_with_origin(&module.imports, &module_ctx, module_dir.as_deref())?;

        // Bind all functions with the module context as their closure
        for def in &module.defs {
            let def_rc = Rc::new(def.clone());
            if let Some(alias_name) = alias {
                ctx.borrow_mut().bind_module_function(
                    alias_name,
                    &def.name,
                    def_rc,
                    module_ctx.clone(),
                );
            } else {
                // include - bind directly without namespace
                ctx.borrow_mut()
                    .bind_function(&def.name, def_rc, module_ctx.clone());
            }
        }
        Ok(())
    }

    /// Extract search path from import metadata
    fn extract_search_path(&self, metadata: &Expr, origin_dir: Option<&Path>) -> Option<PathBuf> {
        // metadata should be an object expression like {search: "./"}
        if let ExprKind::Object(entries) = &metadata.kind {
            for entry in entries {
                let key_name = match &entry.key {
                    ObjectKey::Ident(s) | ObjectKey::String(s) | ObjectKey::Shorthand(s) => {
                        s.clone()
                    }
                    _ => continue,
                };

                if key_name == "search" {
                    // Get the value
                    if let ExprKind::Literal(Literal::String(search_path)) = &entry.value.kind {
                        // Resolve relative to origin directory
                        if let Some(origin) = origin_dir {
                            return Some(origin.join(search_path));
                        } else {
                            return Some(PathBuf::from(search_path));
                        }
                    }
                }
            }
        }
        None
    }

    /// Get module metadata for modulemeta builtin
    pub fn get_module_meta(&mut self, module_path: &str) -> Result<Jv, String> {
        let module = self.load_code_module(module_path)?;

        // Start with module's own metadata if present
        let mut meta = if let Some(ref module_meta_expr) = module.metadata {
            // Try to evaluate the module metadata expression to get object values
            self.eval_const_object(module_meta_expr)
                .unwrap_or_else(|| crate::jv::JvObject::new())
        } else {
            crate::jv::JvObject::new()
        };

        // Build deps array
        let mut deps = Vec::new();
        for import in &module.imports {
            let mut dep = crate::jv::JvObject::new();
            if let Some(alias) = &import.alias {
                dep.set("as", Jv::string(alias));
            }
            dep.set("is_data", Jv::Bool(import.is_data));
            dep.set("relpath", Jv::string(&import.path));

            // Add search metadata if present
            if let Some(ref metadata_expr) = import.metadata {
                if let Some(search_str) = self.extract_search_string(metadata_expr) {
                    dep.set("search", Jv::string(&search_str));
                }
            }
            deps.push(Jv::Object(dep));
        }
        meta.set("deps", Jv::from_vec(deps));

        // Build defs array
        let mut defs = Vec::new();
        for def in &module.defs {
            defs.push(Jv::string(&format!("{}/{}", def.name, def.params.len())));
        }
        meta.set("defs", Jv::from_vec(defs));

        Ok(Jv::Object(meta))
    }

    /// Try to evaluate a constant object expression to get JvObject
    fn eval_const_object(&self, expr: &Expr) -> Option<crate::jv::JvObject> {
        if let ExprKind::Object(entries) = &expr.kind {
            let mut obj = crate::jv::JvObject::new();
            for entry in entries {
                let key = match &entry.key {
                    ObjectKey::Ident(s) | ObjectKey::String(s) | ObjectKey::Shorthand(s) => {
                        s.clone()
                    }
                    _ => continue,
                };
                // Try to get a constant value
                if let Some(value) = self.eval_const_value(&entry.value) {
                    obj.set(&key, value);
                }
            }
            Some(obj)
        } else {
            None
        }
    }

    /// Try to evaluate a constant expression
    fn eval_const_value(&self, expr: &Expr) -> Option<Jv> {
        match &expr.kind {
            ExprKind::Literal(Literal::Null) => Some(Jv::Null),
            ExprKind::Literal(Literal::Bool(b)) => Some(Jv::Bool(*b)),
            ExprKind::Literal(Literal::Number(n)) => Some(Jv::from_f64(*n)),
            ExprKind::Literal(Literal::String(s)) => Some(Jv::string(s)),
            ExprKind::Object(_entries) => self.eval_const_object(expr).map(Jv::Object),
            ExprKind::Array(Some(inner)) => {
                // Try to evaluate array with single constant
                self.eval_const_value(inner).map(|v| Jv::from_vec(vec![v]))
            }
            ExprKind::Array(None) => Some(Jv::from_vec(vec![])),
            _ => None,
        }
    }

    /// Extract search string from metadata expression
    fn extract_search_string(&self, metadata: &Expr) -> Option<String> {
        if let ExprKind::Object(entries) = &metadata.kind {
            for entry in entries {
                let key_name = match &entry.key {
                    ObjectKey::Ident(s) | ObjectKey::String(s) | ObjectKey::Shorthand(s) => {
                        s.clone()
                    }
                    _ => continue,
                };
                if key_name == "search" {
                    if let ExprKind::Literal(Literal::String(s)) = &entry.value.kind {
                        return Some(s.clone());
                    }
                }
            }
        }
        None
    }
}

impl Default for ModuleLoader {
    fn default() -> Self {
        Self::new()
    }
}
