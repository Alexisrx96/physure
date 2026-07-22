use crate::ast::{BinaryOp, Expr, Program, Statement};
use crate::codegen::{CodeGenerator, CodegenError};

pub struct JavaTranspiler;

impl CodeGenerator for JavaTranspiler {
    fn generate_program(&self, program: &Program) -> Result<String, CodegenError> {
        let mut out = String::new();
        out.push_str("import com.physure.Quantity;\n\n");
        out.push_str("public class PhysureProgram {\n");

        for stmt in &program.statements {
            let stmt_code = self.generate_statement(stmt)?;
            if !stmt_code.is_empty() {
                out.push_str(&stmt_code);
                out.push('\n');
            }
        }

        out.push_str("}\n");
        Ok(out)
    }
}

fn snake_to_camel(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = false;
    for (i, c) in s.chars().enumerate() {
        if c == '_' {
            capitalize_next = true;
        } else {
            if capitalize_next && i > 0 {
                result.push(c.to_ascii_uppercase());
            } else {
                result.push(c);
            }
            capitalize_next = false;
        }
    }
    result
}

impl JavaTranspiler {
    fn generate_statement(&self, stmt: &Statement) -> Result<String, CodegenError> {
        match stmt {
            Statement::FunctionDef(f) => {
                let mut out = String::new();
                out.push_str(&format!("    public static Quantity {}(", snake_to_camel(&f.name)));
                let params: Vec<String> = f.params.iter().map(|p| format!("Quantity {}", snake_to_camel(p))).collect();
                out.push_str(&params.join(", "));
                out.push_str(") {\n");
                out.push_str(&format!("        return {};\n", self.generate_expr(&f.body)?));
                out.push_str("    }\n");
                Ok(out)
            }
            _ => Ok(String::new()),
        }
    }

    fn generate_expr(&self, expr: &Expr) -> Result<String, CodegenError> {
        match expr {
            Expr::Identifier(id) => Ok(snake_to_camel(id)),
            Expr::Quantity(q) => {
                let mut args = vec![format!("{:?}", q.magnitude)];
                
                let is_with_unc = q.uncertainty.is_some();
                if let Some(unc) = q.uncertainty {
                    args.push(format!("{:?}", unc));
                }
                if let Some(ref unit) = q.unit {
                    args.push(format!("\"{}\"", unit));
                }

                if is_with_unc {
                    Ok(format!("Quantity.withUncertainty({})", args.join(", ")))
                } else if q.unit.is_some() {
                    Ok(format!("Quantity.of({})", args.join(", ")))
                } else {
                    Ok(format!("Quantity.of({})", args.join(", ")))
                }
            }
            Expr::BinaryOp { op, left, right } => {
                let l = self.generate_expr(left)?;
                let r = self.generate_expr(right)?;
                let method = match op {
                    BinaryOp::Add => "add",
                    BinaryOp::Sub => "subtract",
                    BinaryOp::Mul => "multiply",
                    BinaryOp::Div => "divide",
                    BinaryOp::Pow => "pow",
                    BinaryOp::Convert => "convertTo",
                };
                Ok(format!("{}.{}({})", l, method, r))
            }
            Expr::FunctionCall { name, args } => {
                let mut arg_strs = Vec::new();
                for a in args {
                    arg_strs.push(self.generate_expr(a)?);
                }
                Ok(format!("{}({})", snake_to_camel(name), arg_strs.join(", ")))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{FunctionDefNode, QuantityNode};

    #[test]
    fn test_transpile_function_def() {
        let transpiler = JavaTranspiler;
        let func = Statement::FunctionDef(FunctionDefNode {
            name: "kinetic_energy".to_string(),
            params: vec!["m".to_string(), "v".to_string()],
            body: Expr::BinaryOp {
                op: BinaryOp::Mul,
                left: Box::new(Expr::Identifier("m".to_string())),
                right: Box::new(Expr::Identifier("v".to_string())),
            }
        });
        
        let result = transpiler.generate_statement(&func).unwrap();
        assert!(result.contains("public static Quantity kineticEnergy(Quantity m, Quantity v)"));
        assert!(result.contains("return m.multiply(v);"));
    }

    #[test]
    fn test_transpile_quantity_with_uncertainty() {
        let transpiler = JavaTranspiler;
        let q = Expr::Quantity(QuantityNode {
            magnitude: 75.0,
            uncertainty: Some(0.5),
            unit: Some("kg".to_string()),
        });
        let result = transpiler.generate_expr(&q).unwrap();
        assert_eq!(result, "Quantity.withUncertainty(75.0, 0.5, \"kg\")");
    }
}
