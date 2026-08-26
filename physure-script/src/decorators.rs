use crate::ast::{DecoratorNode, Expr, FunctionDefNode, QuantityNode, Statement};
use physure_core::error::{PhysureError, PhysureResult};

/// Every decorator name Track F's interpreter/validator understands. `@range` is
/// deliberately absent: it is desugared into `requires` by `lower_range` before this
/// registry is ever consulted, so nothing downstream needs to know it existed.
const KNOWN_DECORATORS: &[&str] = &["requires", "ensures", "stable", "experimental", "implicit_units", "precision"];

/// Expands `@range(var, min, max)` into two `@requires` decorators — `var >= min` and
/// `var <= max` — reusing `@requires`'s own runtime enforcement (Task 6) instead of
/// giving `@range` a code path of its own. Any other decorator passes through unchanged.
pub fn lower_range(raw: DecoratorNode) -> PhysureResult<Vec<DecoratorNode>> {
    if raw.name != "range" {
        return Ok(vec![raw]);
    }
    if raw.args.len() != 3 {
        return Err(PhysureError::Generic(format!(
            "@range expects 3 arguments (variable, min, max), got {}",
            raw.args.len()
        )));
    }
    let var_name = match &raw.args[0] {
        Expr::Identifier(name) => name.clone(),
        _ => {
            return Err(PhysureError::Generic(
                "@range's first argument must be a bare parameter name".to_string(),
            ))
        }
    };
    let var = raw.args[0].clone();
    let lo = raw.args[1].clone();
    let hi = raw.args[2].clone();

    let lower = DecoratorNode {
        name: "requires".to_string(),
        args: vec![
            Expr::FunctionCall { name: "op_>=".to_string(), args: vec![var.clone(), lo], kwargs: Vec::new() },
            Expr::Str(format!("{} must be >= the @range lower bound", var_name)),
        ],
    };
    let upper = DecoratorNode {
        name: "requires".to_string(),
        args: vec![
            Expr::FunctionCall { name: "op_<=".to_string(), args: vec![var, hi], kwargs: Vec::new() },
            Expr::Str(format!("{} must be <= the @range upper bound", var_name)),
        ],
    };
    Ok(vec![lower, upper])
}

/// Walks every statement (recursing into function bodies, so a decorated nested `fn`
/// is checked too) and validates its `decorators`. Mirrors `validate_unit_shadowing`
/// in `parser.rs`: called once, after the whole `Program` has been parsed.
pub fn validate_decorators(statements: &[Statement]) -> PhysureResult<()> {
    for stmt in statements {
        check_statement_decorators(stmt)?;
    }
    Ok(())
}

fn check_statement_decorators(stmt: &Statement) -> PhysureResult<()> {
    match stmt {
        Statement::FunctionDef(node) => {
            check_decorator_list(&node.decorators, Some(node))?;
            for body_stmt in &node.body_stmts {
                check_statement_decorators(body_stmt)?;
            }
        }
        Statement::Assignment(node) => {
            check_decorator_list(&node.decorators, None)?;
        }
        _ => {}
    }
    Ok(())
}

fn check_decorator_list(decorators: &[DecoratorNode], func: Option<&FunctionDefNode>) -> PhysureResult<()> {
    if decorators.is_empty() {
        return Ok(());
    }
    let mut has_stable = false;
    let mut has_experimental = false;
    let mut has_ensures = false;

    for dec in decorators {
        if !KNOWN_DECORATORS.contains(&dec.name.as_str()) {
            return Err(PhysureError::Generic(format!("Unknown decorator '@{}'", dec.name)));
        }
        match dec.name.as_str() {
            "requires" | "ensures" => {
                if func.is_none() {
                    return Err(PhysureError::Generic(format!(
                        "@{} (or @range, which desugars into @requires) is only valid on a function definition, not a variable assignment",
                        dec.name
                    )));
                }
                if dec.args.len() != 2 {
                    return Err(PhysureError::Generic(format!(
                        "@{} expects 2 arguments (condition, message), got {}",
                        dec.name,
                        dec.args.len()
                    )));
                }
                if dec.name == "ensures" {
                    has_ensures = true;
                }
            }
            "stable" => {
                if !dec.args.is_empty() {
                    return Err(PhysureError::Generic("@stable takes no arguments".to_string()));
                }
                has_stable = true;
            }
            "experimental" => {
                if !dec.args.is_empty() {
                    return Err(PhysureError::Generic("@experimental takes no arguments".to_string()));
                }
                has_experimental = true;
            }
            "implicit_units" => {
                if func.is_none() {
                    return Err(PhysureError::Generic(
                        "@implicit_units is only valid on a function definition, not a variable assignment".to_string(),
                    ));
                }
                if !dec.args.is_empty() {
                    return Err(PhysureError::Generic("@implicit_units takes no arguments".to_string()));
                }
            }
            "precision" => {
                if func.is_some() {
                    return Err(PhysureError::Generic(
                        "@precision is only valid on a variable assignment, not a function definition".to_string(),
                    ));
                }
                if dec.args.len() != 1 {
                    return Err(PhysureError::Generic(format!(
                        "@precision expects exactly 1 argument (a positive integer), got {}",
                        dec.args.len()
                    )));
                }
                let is_valid_count = matches!(
                    &dec.args[0],
                    Expr::Quantity(QuantityNode { magnitude, .. }) if *magnitude > 0.0 && magnitude.fract() == 0.0
                );
                if !is_valid_count {
                    return Err(PhysureError::Generic("@precision's argument must be a positive whole number".to_string()));
                }
            }
            _ => unreachable!("checked against KNOWN_DECORATORS above"),
        }
    }

    if has_stable && has_experimental {
        return Err(PhysureError::Generic(
            "A function cannot be both @stable and @experimental".to_string(),
        ));
    }
    if has_ensures {
        if let Some(f) = func {
            if f.params.iter().any(|p| p == "result") {
                return Err(PhysureError::Generic(format!(
                    "function '{}' cannot use @ensures because it has a parameter literally named \
                     'result', which the postcondition needs to refer to the return value",
                    f.name
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{AssignmentNode, QuantityNode};

    fn quantity(magnitude: f64) -> Expr {
        Expr::Quantity(QuantityNode {
            magnitude,
            uncertainty: None,
            uncertainty_lower: None,
            is_sigma: false,
            unit: None,
        })
    }

    #[test]
    fn lower_range_expands_into_two_requires() {
        let raw = DecoratorNode {
            name: "range".to_string(),
            args: vec![Expr::Identifier("v".to_string()), quantity(0.0), quantity(10.0)],
        };
        let lowered = lower_range(raw).unwrap();
        assert_eq!(lowered.len(), 2);
        assert!(lowered.iter().all(|d| d.name == "requires"));
    }

    #[test]
    fn lower_range_rejects_wrong_arity() {
        let raw = DecoratorNode {
            name: "range".to_string(),
            args: vec![Expr::Identifier("v".to_string()), quantity(0.0)],
        };
        assert!(lower_range(raw).is_err());
    }

    #[test]
    fn lower_range_passes_through_non_range_decorators() {
        let raw = DecoratorNode { name: "stable".to_string(), args: vec![] };
        let lowered = lower_range(raw.clone()).unwrap();
        assert_eq!(lowered, vec![raw]);
    }

    fn function_with_decorators(decorators: Vec<DecoratorNode>) -> Statement {
        Statement::FunctionDef(FunctionDefNode {
            name: "f".to_string(),
            params: vec!["x".to_string()],
            param_units: vec![None],
            body_stmts: vec![Statement::Expr(Expr::Identifier("x".to_string()))],
            body_lines: vec![],
            decorators,
            doc: None,
        })
    }

    #[test]
    fn validate_decorators_rejects_unknown_name() {
        let stmt = function_with_decorators(vec![DecoratorNode { name: "bogus".to_string(), args: vec![] }]);
        assert!(validate_decorators(&[stmt]).is_err());
    }

    #[test]
    fn validate_decorators_rejects_stable_and_experimental_together() {
        let stmt = function_with_decorators(vec![
            DecoratorNode { name: "stable".to_string(), args: vec![] },
            DecoratorNode { name: "experimental".to_string(), args: vec![] },
        ]);
        assert!(validate_decorators(&[stmt]).is_err());
    }

    #[test]
    fn validate_decorators_accepts_bare_implicit_units() {
        let stmt = function_with_decorators(vec![DecoratorNode { name: "implicit_units".to_string(), args: vec![] }]);
        assert!(validate_decorators(&[stmt]).is_ok());
    }

    #[test]
    fn validate_decorators_rejects_implicit_units_with_arguments() {
        let stmt = function_with_decorators(vec![DecoratorNode { name: "implicit_units".to_string(), args: vec![quantity(1.0)] }]);
        assert!(validate_decorators(&[stmt]).is_err());
    }

    #[test]
    fn validate_decorators_rejects_implicit_units_on_a_plain_assignment() {
        let stmt = Statement::Assignment(AssignmentNode {
            name: "x".to_string(),
            value: quantity(1.0),
            decorators: vec![DecoratorNode { name: "implicit_units".to_string(), args: vec![] }],
        });
        assert!(validate_decorators(&[stmt]).is_err());
    }

    #[test]
    fn validate_decorators_accepts_precision_with_a_positive_integer_on_an_assignment() {
        let stmt = Statement::Assignment(AssignmentNode {
            name: "x".to_string(),
            value: quantity(1.0),
            decorators: vec![DecoratorNode { name: "precision".to_string(), args: vec![quantity(2.0)] }],
        });
        assert!(validate_decorators(&[stmt]).is_ok());
    }

    #[test]
    fn validate_decorators_rejects_precision_on_a_function_definition() {
        let stmt = function_with_decorators(vec![DecoratorNode { name: "precision".to_string(), args: vec![quantity(2.0)] }]);
        assert!(validate_decorators(&[stmt]).is_err());
    }

    #[test]
    fn validate_decorators_rejects_precision_with_wrong_arity() {
        let stmt = Statement::Assignment(AssignmentNode {
            name: "x".to_string(),
            value: quantity(1.0),
            decorators: vec![DecoratorNode { name: "precision".to_string(), args: vec![] }],
        });
        assert!(validate_decorators(&[stmt]).is_err());
    }

    #[test]
    fn validate_decorators_rejects_precision_with_a_non_positive_or_fractional_argument() {
        let non_integer = Statement::Assignment(AssignmentNode {
            name: "x".to_string(),
            value: quantity(1.0),
            decorators: vec![DecoratorNode { name: "precision".to_string(), args: vec![quantity(1.5)] }],
        });
        assert!(validate_decorators(&[non_integer]).is_err());

        let zero = Statement::Assignment(AssignmentNode {
            name: "x".to_string(),
            value: quantity(1.0),
            decorators: vec![DecoratorNode { name: "precision".to_string(), args: vec![quantity(0.0)] }],
        });
        assert!(validate_decorators(&[zero]).is_err());
    }

    #[test]
    fn validate_decorators_rejects_ensures_on_param_named_result() {
        let stmt = Statement::FunctionDef(FunctionDefNode {
            name: "f".to_string(),
            params: vec!["result".to_string()],
            param_units: vec![None],
            body_stmts: vec![Statement::Expr(Expr::Identifier("result".to_string()))],
            body_lines: vec![],
            decorators: vec![DecoratorNode {
                name: "ensures".to_string(),
                args: vec![Expr::Identifier("result".to_string()), Expr::Str("must hold".to_string())],
            }],
            doc: None,
        });
        assert!(validate_decorators(&[stmt]).is_err());
    }

    #[test]
    fn validate_decorators_accepts_requires_with_two_args() {
        let stmt = function_with_decorators(vec![DecoratorNode {
            name: "requires".to_string(),
            args: vec![Expr::Identifier("x".to_string()), Expr::Str("x must be positive".to_string())],
        }]);
        assert!(validate_decorators(&[stmt]).is_ok());
    }

    #[test]
    fn lower_range_rejects_non_identifier_first_arg() {
        let raw = DecoratorNode {
            name: "range".to_string(),
            args: vec![quantity(1.0), quantity(0.0), quantity(10.0)],
        };
        assert!(lower_range(raw).is_err());
    }

    #[test]
    fn validate_decorators_rejects_requires_wrong_arity() {
        let stmt = function_with_decorators(vec![DecoratorNode {
            name: "requires".to_string(),
            args: vec![Expr::Identifier("x".to_string())],
        }]);
        assert!(validate_decorators(&[stmt]).is_err());
    }

    #[test]
    fn validate_decorators_rejects_requires_on_assignment() {
        let stmt = Statement::Assignment(crate::ast::AssignmentNode {
            name: "x".to_string(),
            value: quantity(1.0),
            decorators: vec![DecoratorNode {
                name: "requires".to_string(),
                args: vec![Expr::Identifier("x".to_string()), Expr::Str("must hold".to_string())],
            }],
        });
        assert!(validate_decorators(&[stmt]).is_err());
    }

    #[test]
    fn validate_decorators_rejects_ensures_wrong_arity() {
        let stmt = function_with_decorators(vec![DecoratorNode {
            name: "ensures".to_string(),
            args: vec![Expr::Identifier("result".to_string())],
        }]);
        assert!(validate_decorators(&[stmt]).is_err());
    }

    #[test]
    fn validate_decorators_rejects_ensures_on_assignment() {
        let stmt = Statement::Assignment(crate::ast::AssignmentNode {
            name: "x".to_string(),
            value: quantity(1.0),
            decorators: vec![DecoratorNode {
                name: "ensures".to_string(),
                args: vec![Expr::Identifier("result".to_string()), Expr::Str("must hold".to_string())],
            }],
        });
        assert!(validate_decorators(&[stmt]).is_err());
    }

    #[test]
    fn validate_decorators_rejects_stable_with_args() {
        let stmt = function_with_decorators(vec![DecoratorNode {
            name: "stable".to_string(),
            args: vec![Expr::Identifier("x".to_string())],
        }]);
        assert!(validate_decorators(&[stmt]).is_err());
    }

    #[test]
    fn validate_decorators_rejects_experimental_with_args() {
        let stmt = function_with_decorators(vec![DecoratorNode {
            name: "experimental".to_string(),
            args: vec![Expr::Identifier("x".to_string())],
        }]);
        assert!(validate_decorators(&[stmt]).is_err());
    }

    #[test]
    fn validate_decorators_range_on_assignment_error_mentions_range() {
        let stmt = Statement::Assignment(crate::ast::AssignmentNode {
            name: "v".to_string(),
            value: quantity(1.0),
            decorators: vec![DecoratorNode {
                name: "requires".to_string(),
                args: vec![
                    Expr::Identifier("v".to_string()),
                    Expr::Str("v must be >= the @range lower bound".to_string()),
                ],
            }],
        });
        let err = validate_decorators(&[stmt]).unwrap_err();
        assert!(err.to_string().contains("@range"));
    }
}
