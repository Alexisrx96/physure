use crate::ast::*;
use crate::codegen::{CodeGenerator, CodegenError};

pub struct RustTranspiler;

impl CodeGenerator for RustTranspiler {
    fn generate_program(&self, program: &Program) -> Result<String, CodegenError> {
        let mut code = String::from("use physure::measurement::Quantity;\n\n");
        let mut statements = Vec::new();
        for stmt in &program.statements {
            let stmt_code = self.generate_statement(stmt)?;
            if !stmt_code.is_empty() {
                statements.push(stmt_code);
            }
        }
        code.push_str(&statements.join("\n"));
        Ok(code)
    }
}

impl RustTranspiler {
    fn generate_statement(&self, stmt: &Statement) -> Result<String, CodegenError> {
        match stmt {
            Statement::Import(_) => Ok(String::new()),
            Statement::Export(_) => Ok(String::new()),
            Statement::FunctionDef(node) => self.generate_function_def(node),
            Statement::Assignment(node) => self.generate_assignment(node),
            Statement::Expr(expr) => self.generate_expr(expr),
        }
    }

    fn generate_function_def(&self, node: &FunctionDefNode) -> Result<String, CodegenError> {
        let mut params = Vec::new();
        for param in &node.params {
            params.push(format!("{}: Quantity", param));
        }
        let body = self.generate_expr(&node.body)?;
        Ok(format!(
            "pub fn {}({}) -> Quantity {{\n    {}\n}}",
            node.name,
            params.join(", "),
            body
        ))
    }

    fn generate_assignment(&self, node: &AssignmentNode) -> Result<String, CodegenError> {
        let value = self.generate_expr(&node.value)?;
        Ok(format!("let {} = {};", node.name, value))
    }

    fn generate_expr(&self, expr: &Expr) -> Result<String, CodegenError> {
        match expr {
            Expr::Quantity(node) => {
                let unit = match &node.unit {
                    Some(u) => format!("\"{}\"", u),
                    None => "\"\"".to_string(),
                };
                if let Some(uncertainty) = node.uncertainty {
                    Ok(format!(
                        "Quantity::with_uncertainty({:?}, {:?}, {})",
                        node.magnitude, uncertainty, unit
                    ))
                } else {
                    Ok(format!(
                        "Quantity::new({:?}, {})",
                        node.magnitude, unit
                    ))
                }
            }
            Expr::Identifier(name) => Ok(name.clone()),
            Expr::BinaryOp { op, left, right } => {
                let left_code = self.generate_expr(left)?;
                let right_code = self.generate_expr(right)?;
                match op {
                    BinaryOp::Add => Ok(format!("{} + {}", left_code, right_code)),
                    BinaryOp::Sub => Ok(format!("{} - {}", left_code, right_code)),
                    BinaryOp::Mul => Ok(format!("{} * {}", left_code, right_code)),
                    BinaryOp::Div => Ok(format!("{} / {}", left_code, right_code)),
                    BinaryOp::Pow => {
                        if let Expr::Quantity(q) = &**right {
                            if q.magnitude.fract() == 0.0 {
                                return Ok(format!("{}.powi({})", left_code, q.magnitude as i32));
                            }
                        }
                        Ok(format!("{}.powf({})", left_code, right_code))
                    }
                }
            }
            Expr::FunctionCall { name, args } => {
                let mut arg_codes = Vec::new();
                for arg in args {
                    arg_codes.push(self.generate_expr(arg)?);
                }
                Ok(format!("{}({})", name, arg_codes.join(", ")))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transpile_function_def() {
        let transpiler = RustTranspiler;
        let ast = Program {
            statements: vec![Statement::FunctionDef(FunctionDefNode {
                name: "kinetic_energy".to_string(),
                params: vec!["m".to_string(), "v".to_string()],
                body: Expr::BinaryOp {
                    op: BinaryOp::Mul,
                    left: Box::new(Expr::BinaryOp {
                        op: BinaryOp::Mul,
                        left: Box::new(Expr::Identifier("m".to_string())),
                        right: Box::new(Expr::BinaryOp {
                            op: BinaryOp::Pow,
                            left: Box::new(Expr::Identifier("v".to_string())),
                            right: Box::new(Expr::Quantity(QuantityNode {
                                magnitude: 2.0,
                                uncertainty: None,
                                unit: None,
                            })),
                        }),
                    }),
                    right: Box::new(Expr::Quantity(QuantityNode {
                        magnitude: 0.5,
                        uncertainty: None,
                        unit: None,
                    })),
                },
            })],
        };
        let code = transpiler.generate_program(&ast).unwrap();
        assert!(code.contains("pub fn kinetic_energy(m: Quantity, v: Quantity) -> Quantity"));
        assert!(code.contains("v.powi(2)"));
    }

    #[test]
    fn test_transpile_quantity_with_uncertainty() {
        let transpiler = RustTranspiler;
        let ast = Program {
            statements: vec![Statement::Expr(Expr::Quantity(QuantityNode {
                magnitude: 75.0,
                uncertainty: Some(0.5),
                unit: Some("kg".to_string()),
            }))],
        };
        let code = transpiler.generate_program(&ast).unwrap();
        assert!(code.contains("Quantity::with_uncertainty(75.0, 0.5, \"kg\")"));
    }
}
