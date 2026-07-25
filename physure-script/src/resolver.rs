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
    pub symbols: HashMap<String, Expr>,
    pub functions: HashMap<String, FunctionDefNode>,
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

        let mut symbols = HashMap::new();
        let mut functions = HashMap::new();
        let mut exports = Vec::new();
        for stmt in program.statements {
            match stmt {
                Statement::Assignment(node) => { symbols.insert(node.name, node.value); }
                Statement::FunctionDef(node) => { functions.insert(node.name.clone(), node); }
                Statement::Export(node) => exports.push(node),
                _ => {}
            }
        }

        if exports.is_empty() {
            return Ok(ModuleExport { symbols, functions });
        }

        let mut export = ModuleExport { symbols: HashMap::new(), functions: HashMap::new() };
        for node in exports {
            if let Some(expr) = symbols.get(&node.symbol) {
                export.symbols.insert(node.export_name, expr.clone());
            } else if let Some(func) = functions.get(&node.symbol) {
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
        for (name, expr) in export.symbols {
            self.insert(name, SymbolInfo::Variable(expr))?;
        }
        for (name, func) in export.functions {
            self.insert(name, SymbolInfo::Function(func))?;
        }
        Ok(())
    }

    pub fn import_symbol(&mut self, export: &ModuleExport, name: &str, alias: Option<&str>) -> Result<(), ModuleResolutionError> {
        let target_name = alias.unwrap_or(name).to_string();
        if let Some(expr) = export.symbols.get(name) {
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
            is_sigma: false,
            unit: None,
        })
    }

    #[test]
    fn test_memory_resolver() {
        let mut resolver = MemoryModuleResolver::new();
        let mut export = ModuleExport {
            symbols: HashMap::new(),
            functions: HashMap::new(),
        };
        export.symbols.insert("pi".to_string(), get_dummy_expr());
        
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
            symbols: HashMap::new(),
            functions: HashMap::new(),
        };
        export.symbols.insert("h".to_string(), get_dummy_expr());
        export.symbols.insert("c".to_string(), get_dummy_expr());

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
        assert!(export.symbols.contains_key("G"));

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
        assert!(export.symbols.contains_key("PUBLIC"));
        assert!(!export.symbols.contains_key("internal"));
        assert!(!export.symbols.contains_key("public"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_fs_resolver_missing_file() {
        let resolver = FsModuleResolver::new("/nonexistent/phs/dir/xyz");
        assert!(matches!(resolver.resolve("nope"), Err(ModuleResolutionError::NotFound(_))));
    }
}
