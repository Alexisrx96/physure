use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Program {
    pub statements: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Statement {
    Import(ImportNode),
    Export(ExportNode),
    FunctionDef(FunctionDefNode),
    Assignment(AssignmentNode),
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportNode {
    pub path: String,
    pub specifier: ImportSpecifier,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ImportSpecifier {
    Wildcard,
    Symbols(Vec<ImportSymbol>),
    ModuleAlias(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportSymbol {
    pub name: String,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportNode {
    pub symbol: String,
    pub export_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionDefNode {
    pub name: String,
    pub params: Vec<String>,
    pub body: Expr,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssignmentNode {
    pub name: String,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    Quantity(QuantityNode),
    Identifier(String),
    BinaryOp {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    FunctionCall {
        name: String,
        args: Vec<Expr>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuantityNode {
    pub magnitude: f64,
    pub uncertainty: Option<f64>,
    pub unit: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_construct_import() {
        let node = ImportNode {
            path: "math".to_string(),
            specifier: ImportSpecifier::Wildcard,
        };
        let stmt = Statement::Import(node);
        assert!(false, "Failing unit test for Import");
    }

    #[test]
    fn test_construct_export() {
        let node = ExportNode {
            symbol: "pi".to_string(),
            export_name: "PI".to_string(),
        };
        let stmt = Statement::Export(node);
        assert!(false, "Failing unit test for Export");
    }

    #[test]
    fn test_construct_function_def() {
        let node = FunctionDefNode {
            name: "square".to_string(),
            params: vec!["x".to_string()],
            body: Expr::Identifier("x".to_string()),
        };
        let stmt = Statement::FunctionDef(node);
        assert!(false, "Failing unit test for FunctionDef");
    }

    #[test]
    fn test_construct_assignment() {
        let node = AssignmentNode {
            name: "x".to_string(),
            value: Expr::Identifier("y".to_string()),
        };
        let stmt = Statement::Assignment(node);
        assert!(false, "Failing unit test for Assignment");
    }

    #[test]
    fn test_construct_quantity() {
        let node = QuantityNode {
            magnitude: 1.0,
            uncertainty: None,
            unit: None,
        };
        let expr = Expr::Quantity(node);
        assert!(false, "Failing unit test for Quantity");
    }
}

pub fn unit_to_latex(_unit_str: &str) -> String {
    String::new()
}
