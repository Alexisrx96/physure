use crate::ast::{Expr, FunctionDefNode, Program, Statement};
use crate::codegen::{CodeGenerator, CodegenError};

pub struct MdGenerator;

impl CodeGenerator for MdGenerator {
    fn generate_program(&self, program: &Program) -> Result<String, CodegenError> {
        let node = program
            .statements
            .iter()
            .find_map(|s| match s {
                Statement::FunctionDef(f) => Some(f),
                _ => None,
            })
            .ok_or_else(|| CodegenError::Generic("no function definition found to document".to_string()))?;
        Ok(self.generate_function(node))
    }
}

impl MdGenerator {
    fn generate_function(&self, node: &FunctionDefNode) -> String {
        let mut out = format!("# {}\n\n", node.name);

        if let Some(doc) = &node.doc {
            out.push_str(doc);
            out.push_str("\n\n");
        }

        out.push_str("## Signature\n\n");
        out.push_str(&format!("`{}({}) -> Quantity`\n\n", node.name, node.params.join(", ")));
        out.push_str("| Parameter | Unit |\n| :-- | :-- |\n");
        for (i, param) in node.params.iter().enumerate() {
            let unit = match node.param_units.get(i).and_then(|u| u.as_deref()) {
                Some(u) if !u.is_empty() => u.to_string(),
                _ => "*(none declared)*".to_string(),
            };
            out.push_str(&format!("| `{}` | {} |\n", param, unit));
        }
        out.push('\n');

        if node.decorators.iter().any(|d| d.name == "stable") {
            out.push_str("## Stability\n\n`@stable`\n\n");
        } else if node.decorators.iter().any(|d| d.name == "experimental") {
            out.push_str("## Stability\n\n`@experimental`\n\n");
        }

        let requires: Vec<String> = node
            .decorators
            .iter()
            .filter(|d| d.name == "requires")
            .map(|d| message_text(&d.args[1]))
            .collect();
        if !requires.is_empty() {
            out.push_str("## Preconditions\n\n");
            for msg in &requires {
                out.push_str(&format!("- `{}`\n", msg));
            }
            out.push('\n');
        }

        let ensures: Vec<String> = node
            .decorators
            .iter()
            .filter(|d| d.name == "ensures")
            .map(|d| message_text(&d.args[1]))
            .collect();
        if !ensures.is_empty() {
            out.push_str("## Postconditions\n\n");
            for msg in &ensures {
                out.push_str(&format!("- `{}`\n", msg));
            }
            out.push('\n');
        }

        out.trim_end().to_string() + "\n"
    }
}

fn message_text(expr: &Expr) -> String {
    match expr {
        Expr::Str(s) => s.clone(),
        other => format!("{:?}", other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::DecoratorNode;

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
    fn bare_function_has_signature_table_only() {
        let out = MdGenerator.generate_program(&program_with(base_node())).unwrap();
        assert!(out.contains("# kinetic_energy"));
        assert!(out.contains("`kinetic_energy(m, v) -> Quantity`"));
        assert!(out.contains("| `m` | *(none declared)* |"));
        assert!(out.contains("| `v` | m/s |"));
        assert!(!out.contains("## Stability"));
        assert!(!out.contains("## Preconditions"));
        assert!(!out.contains("## Postconditions"));
    }

    #[test]
    fn doc_only_adds_description_section() {
        let mut node = base_node();
        node.doc = Some("Computes kinetic energy.".to_string());
        let out = MdGenerator.generate_program(&program_with(node)).unwrap();
        assert!(out.contains("# kinetic_energy\n\nComputes kinetic energy.\n\n## Signature"));
    }

    #[test]
    fn decorators_only_add_stability_and_condition_sections() {
        let mut node = base_node();
        node.decorators = vec![
            DecoratorNode { name: "stable".to_string(), args: vec![] },
            DecoratorNode {
                name: "requires".to_string(),
                args: vec![Expr::Identifier("v".to_string()), Expr::Str("v must be positive".to_string())],
            },
            DecoratorNode {
                name: "ensures".to_string(),
                args: vec![Expr::Identifier("result".to_string()), Expr::Str("result must be positive".to_string())],
            },
        ];
        let out = MdGenerator.generate_program(&program_with(node)).unwrap();
        assert!(out.contains("## Stability\n\n`@stable`"));
        assert!(out.contains("## Preconditions\n\n- `v must be positive`"));
        assert!(out.contains("## Postconditions\n\n- `result must be positive`"));
    }

    #[test]
    fn doc_and_decorators_together_render_all_sections() {
        let mut node = base_node();
        node.doc = Some("Computes kinetic energy.".to_string());
        node.decorators = vec![DecoratorNode { name: "experimental".to_string(), args: vec![] }];
        let out = MdGenerator.generate_program(&program_with(node)).unwrap();
        assert!(out.contains("Computes kinetic energy."));
        assert!(out.contains("`@experimental`"));
    }

    #[test]
    fn range_lowered_requires_render_as_separate_bullets() {
        let mut node = base_node();
        node.decorators = vec![
            DecoratorNode {
                name: "requires".to_string(),
                args: vec![Expr::Identifier("v".to_string()), Expr::Str("v must be >= the @range lower bound".to_string())],
            },
            DecoratorNode {
                name: "requires".to_string(),
                args: vec![Expr::Identifier("v".to_string()), Expr::Str("v must be <= the @range upper bound".to_string())],
            },
        ];
        let out = MdGenerator.generate_program(&program_with(node)).unwrap();
        assert!(out.contains("- `v must be >= the @range lower bound`"));
        assert!(out.contains("- `v must be <= the @range upper bound`"));
    }
}
