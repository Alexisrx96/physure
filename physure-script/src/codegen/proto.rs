use crate::ast::{FunctionDefNode, Program, Statement};
use crate::codegen::{CodeGenerator, CodegenError};

pub struct ProtoGenerator;

impl CodeGenerator for ProtoGenerator {
    fn generate_program(&self, program: &Program) -> Result<String, CodegenError> {
        let node = program
            .statements
            .iter()
            .find_map(|s| match s {
                Statement::FunctionDef(f) => Some(f),
                _ => None,
            })
            .ok_or_else(|| {
                CodegenError::Generic("no function definition found to generate a .proto contract for".to_string())
            })?;
        Ok(self.generate_function(node))
    }
}

impl ProtoGenerator {
    fn generate_function(&self, node: &FunctionDefNode) -> String {
        let pascal = super::to_pascal_case(&node.name);
        let has_contract = node.decorators.iter().any(|d| d.name == "requires" || d.name == "ensures");

        let mut out = String::from("syntax = \"proto3\";\n\n");

        out.push_str(&format!("message {}Request {{\n", pascal));
        for (i, param) in node.params.iter().enumerate() {
            out.push_str(&format!("  double {} = {};\n", param, i + 1));
        }
        out.push_str("}\n\n");

        out.push_str(&format!("message {}Response {{\n", pascal));
        out.push_str("  double value = 1;\n");
        if has_contract {
            out.push_str("  bool ok = 2;      // present only if the function has >=1 @requires/@ensures\n");
            out.push_str("  string error = 3; // present only if the function has >=1 @requires/@ensures\n");
        }
        out.push_str("}\n\n");

        out.push_str(&format!("service {}Service {{\n", pascal));
        out.push_str(&format!("  rpc Compute({}Request) returns ({}Response);\n", pascal, pascal));
        out.push_str("}\n");

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::DecoratorNode;
    use crate::ast::Expr;

    fn program_with(node: FunctionDefNode) -> Program {
        Program { statements: vec![Statement::FunctionDef(node)], lines: vec![] }
    }

    fn base_node() -> FunctionDefNode {
        FunctionDefNode {
            name: "kinetic_energy".to_string(),
            params: vec!["m".to_string(), "v".to_string()],
            param_units: vec![None, Some("m/s".to_string())],
            body_stmts: vec![],
            body_lines: vec![],
            decorators: vec![],
            doc: None,
        }
    }

    #[test]
    fn generates_request_response_service_without_contract_fields() {
        let out = ProtoGenerator.generate_program(&program_with(base_node())).unwrap();
        assert!(out.contains("message KineticEnergyRequest {"));
        assert!(out.contains("double m = 1;"));
        assert!(out.contains("double v = 2;"));
        assert!(out.contains("message KineticEnergyResponse {"));
        assert!(out.contains("double value = 1;"));
        assert!(!out.contains("bool ok"));
        assert!(out.contains("service KineticEnergyService {"));
        assert!(out.contains("rpc Compute(KineticEnergyRequest) returns (KineticEnergyResponse);"));
    }

    #[test]
    fn adds_ok_and_error_fields_when_function_has_contracts() {
        let mut node = base_node();
        node.decorators = vec![DecoratorNode {
            name: "requires".to_string(),
            args: vec![Expr::Identifier("m".to_string()), Expr::Str("m must be positive".to_string())],
        }];
        let out = ProtoGenerator.generate_program(&program_with(node)).unwrap();
        assert!(out.contains("bool ok = 2;"));
        assert!(out.contains("string error = 3;"));
    }

    #[test]
    fn errors_on_empty_program() {
        let out = ProtoGenerator.generate_program(&Program { statements: vec![], lines: vec![] });
        assert!(out.is_err());
    }
}
