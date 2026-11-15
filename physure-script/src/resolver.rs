use std::collections::HashMap;
use std::path::PathBuf;
use crate::ast::{Expr, FunctionDefNode, Statement};

#[derive(Debug, PartialEq, Clone)]
pub enum ModuleResolutionError {
    NotFound(String),
    ParseError(String),
    Collision(String),
}

pub trait ModuleResolver: Send + Sync {
    fn resolve(&self, path: &str) -> Result<ModuleExport, ModuleResolutionError>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModuleExport {
    /// The values the module defines, in the order it defines them: an importer evaluates them in
    /// that order, so a later one may use an earlier one (`q = 3 * p`).
    pub symbols: Vec<(String, Expr)>,
    pub functions: HashMap<String, FunctionDefNode>,
}

impl ModuleExport {
    /// The value the module defines under `name`.
    pub fn symbol(&self, name: &str) -> Option<&Expr> {
        self.symbols.iter().find(|(defined, _)| defined == name).map(|(_, expr)| expr)
    }

    /// Defines `name` as `expr`: a name defined again keeps its place and takes the new value.
    pub fn define(&mut self, name: String, expr: Expr) {
        match self.symbols.iter_mut().find(|(defined, _)| *defined == name) {
            Some((_, value)) => *value = expr,
            None => self.symbols.push((name, expr)),
        }
    }

    /// The names of the functions, sorted (so whichever of them an error names is always the same).
    pub fn function_names(&self) -> Vec<&String> {
        let mut names: Vec<&String> = self.functions.keys().collect();
        names.sort();
        names
    }
}

pub struct MemoryModuleResolver {
    pub modules: HashMap<String, ModuleExport>,
}

impl MemoryModuleResolver {
    pub fn new() -> Self {
        Self { modules: HashMap::new() }
    }
    
    pub fn add_module(&mut self, path: String, export: ModuleExport) {
        self.modules.insert(path, export);
    }
}

impl ModuleResolver for MemoryModuleResolver {
    fn resolve(&self, path: &str) -> Result<ModuleExport, ModuleResolutionError> {
        self.modules.get(path).cloned().ok_or_else(|| ModuleResolutionError::NotFound(path.to_string()))
    }
}

/// Resolves `import "path"` against `.phs` files on disk, relative to `base_dir`
/// (normally the directory of the script that's importing).
///
/// `path` segments are joined with `/` and suffixed with `.phs`, so `import a.b`
/// resolves to `<base_dir>/a/b.phs`. If the module contains one or more `export`
/// statements, only those symbols/functions are visible to importers; otherwise
/// every top-level assignment and function definition is exported.
pub struct FsModuleResolver {
    pub base_dir: PathBuf,
}

impl FsModuleResolver {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self { base_dir: base_dir.into() }
    }
}

impl Default for FsModuleResolver {
    fn default() -> Self {
        Self::new(".")
    }
}

impl ModuleResolver for FsModuleResolver {
    fn resolve(&self, path: &str) -> Result<ModuleExport, ModuleResolutionError> {
        let rel_path = path.replace('.', "/") + ".phs";
        let file_path = self.base_dir.join(rel_path);
        let source = std::fs::read_to_string(&file_path)
            .map_err(|_| ModuleResolutionError::NotFound(path.to_string()))?;
        let program = crate::parser::parse_phs(&source)
            .map_err(|e| ModuleResolutionError::ParseError(format!("{:?}", e)))?;

        let mut module = ModuleExport { symbols: Vec::new(), functions: HashMap::new() };
        let mut exports = Vec::new();
        for stmt in program.statements {
            match stmt {
                Statement::Assignment(node) => module.define(node.name, node.value),
                Statement::FunctionDef(node) => { module.functions.insert(node.name.clone(), node); }
                Statement::Export(node) => exports.push(node),
                _ => {}
            }
        }

        if exports.is_empty() {
            return Ok(module);
        }

        let mut export = ModuleExport { symbols: Vec::new(), functions: HashMap::new() };
        for node in exports {
            if let Some(expr) = module.symbol(&node.symbol) {
                export.define(node.export_name, expr.clone());
            } else if let Some(func) = module.functions.get(&node.symbol) {
                export.functions.insert(node.export_name, func.clone());
            } else {
                return Err(ModuleResolutionError::NotFound(node.symbol));
            }
        }
        Ok(export)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SymbolInfo {
    Variable(Expr),
    Function(FunctionDefNode),
}

pub struct SymbolTable {
    pub scopes: Vec<HashMap<String, SymbolInfo>>,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self { scopes: vec![HashMap::new()] }
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    pub fn insert(&mut self, name: String, info: SymbolInfo) -> Result<(), ModuleResolutionError> {
        let current_scope = self.scopes.last_mut().unwrap();
        if current_scope.contains_key(&name) {
            return Err(ModuleResolutionError::Collision(name));
        }
        current_scope.insert(name, info);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&SymbolInfo> {
        for scope in self.scopes.iter().rev() {
            if let Some(info) = scope.get(name) {
                return Some(info);
            }
        }
        None
    }
    
    pub fn import_wildcard(&mut self, export: ModuleExport) -> Result<(), ModuleResolutionError> {
        for (name, expr) in &export.symbols {
            self.insert(name.clone(), SymbolInfo::Variable(expr.clone()))?;
        }
        for name in export.function_names() {
            self.insert(name.clone(), SymbolInfo::Function(export.functions[name].clone()))?;
        }
        Ok(())
    }

    pub fn import_symbol(&mut self, export: &ModuleExport, name: &str, alias: Option<&str>) -> Result<(), ModuleResolutionError> {
        let target_name = alias.unwrap_or(name).to_string();
        if let Some(expr) = export.symbol(name) {
            self.insert(target_name, SymbolInfo::Variable(expr.clone()))?;
            return Ok(());
        }
        if let Some(func) = export.functions.get(name) {
            self.insert(target_name, SymbolInfo::Function(func.clone()))?;
            return Ok(());
        }
        Err(ModuleResolutionError::NotFound(name.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Expr;
    use crate::ast::QuantityNode;

    fn get_dummy_expr() -> Expr {
        Expr::Quantity(QuantityNode {
            magnitude: 1.0,
            uncertainty: None,
            uncertainty_lower: None,
            is_sigma: false,
            dof: None,
            unit: None,
        })
    }

    #[test]
    fn test_memory_resolver() {
        let mut resolver = MemoryModuleResolver::new();
        let mut export = ModuleExport {
            symbols: Vec::new(),
            functions: HashMap::new(),
        };
        export.define("pi".to_string(), get_dummy_expr());
        
        resolver.add_module("math".to_string(), export.clone());

        assert_eq!(resolver.resolve("math"), Ok(export));
        assert_eq!(resolver.resolve("physics"), Err(ModuleResolutionError::NotFound("physics".to_string())));
    }

    #[test]
    fn test_symbol_table_collision() {
        let mut table = SymbolTable::new();
        table.insert("x".to_string(), SymbolInfo::Variable(get_dummy_expr())).unwrap();
        
        let res = table.insert("x".to_string(), SymbolInfo::Variable(get_dummy_expr()));
        assert_eq!(res, Err(ModuleResolutionError::Collision("x".to_string())));
    }

    #[test]
    fn test_symbol_table_wildcard_and_alias() {
        let mut table = SymbolTable::new();
        let mut export = ModuleExport {
            symbols: Vec::new(),
            functions: HashMap::new(),
        };
        export.define("h".to_string(), get_dummy_expr());
        export.define("c".to_string(), get_dummy_expr());

        // Wildcard
        table.import_wildcard(export.clone()).unwrap();
        assert!(table.get("h").is_some());
        assert!(table.get("c").is_some());
        
        // Alias
        let mut table2 = SymbolTable::new();
        table2.import_symbol(&export, "h", Some("planck")).unwrap();
        assert!(table2.get("h").is_none());
        assert!(table2.get("planck").is_some());
    }
}

#[cfg(test)]
mod fs_resolver_tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_fs_resolver_wildcard_import_implicit_export() {
        let dir = std::env::temp_dir().join(format!("phs_resolver_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut f = std::fs::File::create(dir.join("constants.phs")).unwrap();
        writeln!(f, "G = 6.674e-11").unwrap();
        drop(f);

        let resolver = FsModuleResolver::new(&dir);
        let export = resolver.resolve("constants").unwrap();
        assert!(export.symbol("G").is_some());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_fs_resolver_explicit_export_hides_others() {
        let dir = std::env::temp_dir().join(format!("phs_resolver_test_export_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut f = std::fs::File::create(dir.join("mod.phs")).unwrap();
        writeln!(f, "internal = 1\npublic = 2\nexport public as PUBLIC").unwrap();
        drop(f);

        let resolver = FsModuleResolver::new(&dir);
        let export = resolver.resolve("mod").unwrap();
        assert!(export.symbol("PUBLIC").is_some());
        assert!(export.symbol("internal").is_none());
        assert!(export.symbol("public").is_none());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_fs_resolver_missing_file() {
        let resolver = FsModuleResolver::new("/nonexistent/phs/dir/xyz");
        assert!(matches!(resolver.resolve("nope"), Err(ModuleResolutionError::NotFound(_))));
    }

    #[test]
    fn an_imported_module_binds_its_names_in_the_order_it_defines_them() {
        // `q` uses `p` and `r` both: imported in the order a map iterated in, `r` was 8 m one time in
        // seven and otherwise an unknown unit `p` or `q`, or a type error.
        let dir = std::env::temp_dir().join(format!("phs_resolver_test_order_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("lib.phs"), "p = 2 m\nq = 3 * p\nr = q + p\n").unwrap();
        std::fs::write(dir.join("hidden.phs"), "p = 2 m\nq = 3 * p\nexport p as P\nexport q as Q\n").unwrap();

        let resolver = FsModuleResolver::new(&dir);
        let names = |export: ModuleExport| export.symbols.into_iter().map(|(name, _)| name).collect::<Vec<_>>();
        assert_eq!(names(resolver.resolve("lib").unwrap()), ["p", "q", "r"]);
        assert_eq!(names(resolver.resolve("hidden").unwrap()), ["P", "Q"]);
        for _ in 0..30 {
            let mut interp = crate::interpreter::PhsInterpreter::with_base_dir(&dir);
            let results = interp.eval_str("import \"lib\"\nr").unwrap();
            let crate::value::PhsValue::Quantity(r) = &results[1] else { panic!("{results:?}") };
            assert_eq!(r.value.mean(), 8.0);
        }

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
