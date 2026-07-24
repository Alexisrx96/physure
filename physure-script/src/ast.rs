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
    pub body_stmts: Vec<Statement>,
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
    Convert,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuantityNode {
    pub magnitude: f64,
    pub uncertainty: Option<f64>,
    #[serde(default)]
    pub is_sigma: bool,
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
        assert!(matches!(stmt, Statement::Import(_)));
    }

    #[test]
    fn test_construct_export() {
        let node = ExportNode {
            symbol: "pi".to_string(),
            export_name: "PI".to_string(),
        };
        let stmt = Statement::Export(node);
        assert!(matches!(stmt, Statement::Export(_)));
    }

    #[test]
    fn test_construct_function_def() {
        let node = FunctionDefNode {
            name: "square".to_string(),
            params: vec!["x".to_string()],
            body_stmts: vec![Statement::Expr(Expr::Identifier("x".to_string()))],
        };
        let stmt = Statement::FunctionDef(node);
        assert!(matches!(stmt, Statement::FunctionDef(_)));
    }

    #[test]
    fn test_construct_assignment() {
        let node = AssignmentNode {
            name: "x".to_string(),
            value: Expr::Identifier("y".to_string()),
        };
        let stmt = Statement::Assignment(node);
        assert!(matches!(stmt, Statement::Assignment(_)));
    }

    #[test]
    fn test_construct_quantity() {
        let node = QuantityNode {
            magnitude: 1.0,
            uncertainty: None,
            is_sigma: false,
            unit: None,
        };
        let expr = Expr::Quantity(node);
        assert!(matches!(expr, Expr::Quantity(_)));
    }
}

pub fn unit_to_latex(unit_str: &str) -> String {
    let u = unit_str.trim();
    if u.is_empty() || u == "1" || u == "Dimensionless" {
        return String::new();
    }

    fn format_part(part: &str) -> String {
        let terms: Vec<&str> = part.split('*').collect();
        let mut formatted_terms = Vec::new();
        for t in terms {
            let clean = t.trim();
            if clean.is_empty() { continue; }
            if let Some((base, exp)) = clean.split_once('^') {
                formatted_terms.push(format!("\\text{{{}}}^{{{}}}", base.trim(), exp.trim()));
            } else {
                formatted_terms.push(format!("\\text{{{}}}", clean));
            }
        }
        formatted_terms.join(" \\cdot ")
    }

    if let Some((num, den)) = u.split_once('/') {
        let num_latex = format_part(num);
        let den_latex = format_part(den);
        if num_latex.is_empty() {
            format!("\\frac{{1}}{{{}}}", den_latex)
        } else {
            format!("\\frac{{{}}}{{{}}}", num_latex, den_latex)
        }
    } else {
        format_part(u)
    }
}
