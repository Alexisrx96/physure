use crate::ast::{
    Program, Statement, ImportNode, ImportSpecifier, ExportNode,
    FunctionDefNode, AssignmentNode, Expr, BinaryOp, QuantityNode
};
use super::{CodeGenerator, CodegenError};

pub struct PythonTranspiler;

impl CodeGenerator for PythonTranspiler {
    fn generate_program(&self, program: &Program) -> Result<String, CodegenError> {
        let mut out = String::new();
        
        for stmt in &program.statements {
            let stmt_str = self.generate_statement(stmt)?;
            out.push_str(&stmt_str);
            out.push('\n');
        }
        
        Ok(out)
    }
}

impl PythonTranspiler {
    fn generate_statement(&self, stmt: &Statement) -> Result<String, CodegenError> {
        match stmt {
            Statement::Import(node) => self.generate_import(node),
            Statement::Export(node) => self.generate_export(node),
            Statement::FunctionDef(node) => self.generate_function_def(node),
            Statement::Assignment(node) => self.generate_assignment(node),
            Statement::Expr(expr) => self.generate_expr(expr),
        }
    }
    
    fn generate_import(&self, node: &ImportNode) -> Result<String, CodegenError> {
        let py_path = node.path.replace("/", ".");
        let module = format!("physure.{}", py_path);
        
        match &node.specifier {
            ImportSpecifier::Wildcard => Ok(format!("from {} import *", module)),
            ImportSpecifier::Symbols(syms) => {
                let mut sym_strs = Vec::new();
                for sym in syms {
                    if let Some(alias) = &sym.alias {
                        sym_strs.push(format!("{} as {}", sym.name, alias));
                    } else {
                        sym_strs.push(sym.name.clone());
                    }
                }
                Ok(format!("from {} import {}", module, sym_strs.join(", ")))
            }
            ImportSpecifier::ModuleAlias(alias) => {
                Ok(format!("import {} as {}", module, alias))
            }
        }
    }
    
    fn generate_export(&self, node: &ExportNode) -> Result<String, CodegenError> {
        Ok(format!("__all_exports__[\"{}\"] = {}", node.export_name, node.symbol))
    }
    
    fn generate_function_def(&self, node: &FunctionDefNode) -> Result<String, CodegenError> {
        let params = node.params.join(", ");
        let body_str = self.generate_expr(&node.body)?;
        Ok(format!("def {}({}):\n    return {}", node.name, params, body_str))
    }
    
    fn generate_assignment(&self, node: &AssignmentNode) -> Result<String, CodegenError> {
        let val_str = self.generate_expr(&node.value)?;
        Ok(format!("{} = {}", node.name, val_str))
    }
    
    fn generate_expr(&self, expr: &Expr) -> Result<String, CodegenError> {
        match expr {
            Expr::Quantity(node) => self.generate_quantity(node),
            Expr::Identifier(name) => Ok(name.clone()),
            Expr::BinaryOp { op, left, right } => {
                let l_str = self.generate_expr(left)?;
                let r_str = self.generate_expr(right)?;
                let op_str = match op {
                    BinaryOp::Add => "+".to_string(),
                    BinaryOp::Sub => "-".to_string(),
                    BinaryOp::Mul => "*".to_string(),
                    BinaryOp::Div => "/".to_string(),
                    BinaryOp::Pow => "**".to_string(),
                    BinaryOp::Convert => format!("{}  # => {}", l_str, r_str),
                };
                Ok(format!("({} {} {})", l_str, op_str, r_str))
            }
            Expr::FunctionCall { name, args } => {
                let mut arg_strs = Vec::new();
                for arg in args {
                    arg_strs.push(self.generate_expr(arg)?);
                }
                Ok(format!("{}({})", name, arg_strs.join(", ")))
            }
        }
    }
    
    fn generate_quantity(&self, node: &QuantityNode) -> Result<String, CodegenError> {
        let unit_str = node.unit.as_deref().unwrap_or("");
        let unc_str = if let Some(unc) = node.uncertainty {
            format!(", uncertainty={:?}", unc)
        } else {
            String::new()
        };
        Ok(format!("Q_({:?}, \"{}\"{})", node.magnitude, unit_str, unc_str))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{ImportSymbol, Program, Statement, FunctionDefNode, Expr, QuantityNode, BinaryOp, ImportNode, ImportSpecifier};

    #[test]
    fn test_transpile_quantity_with_uncertainty() {
        let tp = PythonTranspiler;
        let q = QuantityNode {
            magnitude: 5.0,
            uncertainty: Some(0.1),
            unit: Some("m".to_string()),
        };
        let res = tp.generate_quantity(&q).unwrap();
        assert_eq!(res, "Q_(5.0, \"m\", uncertainty=0.1)");
    }

    #[test]
    fn test_transpile_function_def_and_import() {
        let tp = PythonTranspiler;
        
        let import_node = ImportNode {
            path: "physics/constants".to_string(),
            specifier: ImportSpecifier::Symbols(vec![
                ImportSymbol { name: "g".to_string(), alias: None }
            ]),
        };
        
        let fn_node = FunctionDefNode {
            name: "foo".to_string(),
            params: vec!["a".to_string(), "b".to_string()],
            body: Expr::BinaryOp {
                op: BinaryOp::Mul,
                left: Box::new(Expr::Identifier("a".to_string())),
                right: Box::new(Expr::Identifier("b".to_string())),
            },
        };

        let prog = Program {
            statements: vec![
                Statement::Import(import_node),
                Statement::FunctionDef(fn_node),
            ],
        };

        let res = tp.generate_program(&prog).unwrap();
        assert!(res.contains("from physure.physics.constants import g"));
        assert!(res.contains("def foo(a, b):\n    return (a * b)"));
    }
}
