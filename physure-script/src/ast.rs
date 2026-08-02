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
    Return(Expr),
    GuardReturn { cond: Expr, value: Expr },
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
    /// Optional declared unit constraint for each parameter, aligned by index with `params`.
    /// `None` (or a missing/short entry, for backward compatibility) means the parameter has
    /// no declared unit, so its argument is bound as-is with no conversion attempted.
    #[serde(default)]
    pub param_units: Vec<Option<String>>,
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
    /// A quoted string literal. Kept apart from `Identifier` so that a string whose text
    /// happens to name a variable stays the text the user wrote; `{name}` interpolation is
    /// the explicit way to fold a value into it.
    Str(String),
    BinaryOp {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    FunctionCall {
        name: String,
        args: Vec<Expr>,
        #[serde(default)]
        kwargs: Vec<(String, Expr)>,
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
    Range,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuantityNode {
    pub magnitude: f64,
    pub uncertainty: Option<f64>,
    /// The lower half-width of an asymmetric measurement, when one was written.
    ///
    /// `12.3 +/- (0.5, 0.4)` puts 0.5 in `uncertainty` and 0.4 here, so a symmetric quantity
    /// reads exactly as it did before. That is also the trap: anything that reads `uncertainty`
    /// alone silently reports an asymmetric measurement as symmetric, which is the one answer
    /// it must not give. Consumers either handle both halves or refuse.
    #[serde(default)]
    pub uncertainty_lower: Option<f64>,
    #[serde(default)]
    pub is_sigma: bool,
    pub unit: Option<String>,
}

impl QuantityNode {
    /// The reason this quantity cannot be used yet, if it is asymmetric.
    ///
    /// The grammar accepts `12.3 +/- (0.5, 0.4) pb` so the notation is settled and one place
    /// holds both halves. Nothing propagates a third moment yet, though, and every consumer
    /// downstream takes a single standard deviation, so an asymmetric measurement would come
    /// out looking symmetric with no sign that half of what was written had been dropped.
    pub fn asymmetric_refusal(&self) -> Option<String> {
        self.uncertainty_lower.map(|lower| {
            format!(
                "An asymmetric uncertainty (+{}, -{}) parses but cannot be evaluated yet: \
                 nothing propagates the third moment it needs, and using only the upper half \
                 would report the measurement as symmetric. Write a single value like \
                 `+/- {}` if that is what you meant.",
                self.uncertainty.unwrap_or(0.0),
                lower,
                self.uncertainty.unwrap_or(0.0),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_asymmetric_quantity_says_why_it_cannot_be_used() {
        let node = QuantityNode {
            magnitude: 12.3,
            uncertainty: Some(0.5),
            uncertainty_lower: Some(0.4),
            is_sigma: false,
            unit: Some("pb".to_string()),
        };
        assert!(node.asymmetric_refusal().unwrap().contains("cannot be evaluated yet"));

        let symmetric = QuantityNode { uncertainty_lower: None, ..node };
        assert!(symmetric.asymmetric_refusal().is_none());
    }

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
            param_units: vec![None],
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
            uncertainty_lower: None,
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
