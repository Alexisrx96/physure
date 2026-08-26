use physure_core::error::{PhysureError, PhysureResult};
use crate::ast::*;
use super::Rule;
use super::quantities::parse_quantity;

pub(crate) fn parse_expr(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut inner = pair.into_inner();
    let first = match inner.next() {
        Some(f) => f,
        None => return Ok(Expr::Quantity(QuantityNode { magnitude: 0.0, uncertainty: None, uncertainty_lower: None, is_sigma: false, unit: None })),
    };
    let mut result = match first.as_rule() {
        Rule::if_expr => parse_if_expr(first)?,
        Rule::for_expr => parse_for_expr(first)?,
        Rule::base_expr => parse_base_expr(first)?,
        Rule::conv_expr => parse_conv_expr(first)?,
        _ => parse_comp_expr(first)?,
    };
    // A ternary tail and a `where` clause can both follow, in that order.
    for tail in inner {
        result = match tail.as_rule() {
            Rule::ternary_op => {
                // `ternary_op` is a rule of its own, so the two branches arrive nested
                // inside it rather than as further children of `expr`.
                let mut branches = tail.into_inner();
                let then_pair = branches
                    .next()
                    .ok_or_else(|| PhysureError::Generic("Ternary is missing its 'then' branch".into()))?;
                let else_pair = branches
                    .next()
                    .ok_or_else(|| PhysureError::Generic("Ternary is missing its 'else' branch".into()))?;
                let then_expr = parse_base_expr(then_pair)?;
                let else_expr = parse_base_expr(else_pair)?;
                Expr::FunctionCall {
                    name: "ternary".to_string(),
                    args: vec![result, then_expr, else_expr],
                    kwargs: Vec::new(),
                }
            }
            Rule::where_expr => parse_where_expr(tail, result)?,
            other => {
                return Err(PhysureError::Generic(format!("Unexpected rule after an expression: {:?}", other)))
            }
        };
    }
    Ok(result)
}

/// The loosest tier: the one optional `..` and the format spec that closes the expression.
fn parse_base_expr(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut inner = pair.into_inner();
    let first = inner.next().unwrap();
    let mut left = parse_conv_expr(first)?;

    while let Some(op_pair) = inner.next() {
        // The spec closes the expression — `a + b: .2f` formats the sum, not `b`.
        if op_pair.as_rule() == Rule::op_format {
            let spec = op_pair.into_inner().next().map(|p| p.as_str().to_string()).unwrap_or_default();
            left = Expr::FunctionCall {
                name: "format".to_string(),
                args: vec![left, Expr::Identifier(spec)],
                kwargs: Vec::new(),
            };
            continue;
        }
        if op_pair.as_rule() != Rule::op_range {
            return Err(PhysureError::Generic(format!("Unexpected op in base_expr: {:?}", op_pair.as_rule())));
        }
        // A missing endpoint is a grammar error, so `inner.next()` is the endpoint the
        // grammar already required rather than something that might not be there.
        let right = parse_conv_expr(inner.next().unwrap())?;
        left = Expr::BinaryOp {
            op: BinaryOp::Range,
            left: Box::new(left),
            right: Box::new(right),
        };
    }

    Ok(left)
}

fn parse_conv_expr(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut inner = pair.into_inner();
    let first = inner.next().unwrap();
    let mut left = parse_comp_expr(first)?;

    while let Some(op_pair) = inner.next() {
        let op = match op_pair.as_rule() {
            Rule::op_add => BinaryOp::Add,
            Rule::op_sub => BinaryOp::Sub,
            Rule::op_convert => BinaryOp::Convert,
            _ => return Err(PhysureError::Generic(format!("Unexpected op in conv_expr: {:?}", op_pair.as_rule()))),
        };
        let right_pair = inner.next().unwrap();
        // `=>`'s target is a `unit_expr`, not a `comp_expr` (see phs.pest's `conv_expr`) --
        // every consumer of this side (the interpreter's `BinaryOp::Convert` arm, every
        // codegen target) reads it back through `expr_to_unit_string`, which flattens an
        // `Expr` to unit-chain text and only special-cases `Identifier`/`Quantity`/`BinaryOp`.
        // Reusing the matched text as one `Identifier` is exactly what that flattening would
        // produce for a real unit chain anyway, and it sidesteps rebuilding `unit_expr`'s
        // internal `*`/`/` structure into an equivalent `Expr` tree for no benefit -- nothing
        // ever inspects this side's shape, only its flattened text.
        let right = if op == BinaryOp::Convert {
            Expr::Identifier(right_pair.as_str().trim().to_string())
        } else {
            parse_comp_expr(right_pair)?
        };
        left = Expr::BinaryOp {
            op,
            left: Box::new(left),
            right: Box::new(right),
        };
    }

    Ok(left)
}

fn parse_comp_expr(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut inner = pair.into_inner();
    let first = inner.next().unwrap();
    let mut left = parse_term(first)?;
    
    while let Some(op_pair) = inner.next() {
        if op_pair.as_rule() == Rule::op_compare {
            let right_pair = inner.next().unwrap();
            let right = parse_term(right_pair)?;
            let cmp_op = op_pair.as_str().to_string();
            left = Expr::FunctionCall {
                name: format!("op_{}", cmp_op),
                args: vec![left, right],
                kwargs: Vec::new(),
            };
        } else {
            let right = parse_term(op_pair)?;
            left = Expr::BinaryOp {
                op: BinaryOp::Mul,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
    }
    
    Ok(left)
}

/// `Some(name)` when `expr` is nothing but a bare name — used by `parse_term` to track
/// whether the most recently produced factor is a bare identifier, regardless of which
/// operator produced it.
fn as_bare_identifier(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Identifier(name) => Some(name.clone()),
        _ => None,
    }
}

fn parse_term(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut inner = pair.into_inner();
    let first = inner.next().unwrap(); // factor
    let mut left = parse_factor(first)?;
    // The name of the *rightmost* factor combined into `left` so far, when it happens to be a
    // bare identifier — carried across every arm below (explicit or implicit), because what
    // matters is whether the *next* join is implicit, not how the previous one was spelled.
    // Only used to catch two bare identifiers landing next to each other via implicit
    // multiplication (`x y`, or `y z` in `x * y z`); see the `Rule::factor` arm below.
    let mut prev_bare_identifier = as_bare_identifier(&left);

    while let Some(next_pair) = inner.next() {
        match next_pair.as_rule() {
            Rule::op_mul => {
                let right_pair = inner.next().unwrap();
                let right = parse_factor(right_pair)?;
                prev_bare_identifier = as_bare_identifier(&right);
                left = Expr::BinaryOp {
                    op: BinaryOp::Mul,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            }
            Rule::op_div => {
                let right_pair = inner.next().unwrap();
                let right = parse_factor(right_pair)?;
                prev_bare_identifier = as_bare_identifier(&right);
                left = Expr::BinaryOp {
                    op: BinaryOp::Div,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            }
            Rule::factor => {
                // Implicit multiplication — except between two bare identifiers with nothing
                // else in play (`x y`). That reads exactly like a forgotten operator, and it
                // is never how the language's own examples write a product of two symbols: a
                // coefficient/quantity always sits on one side (`1/2 m`, `(2 m) v^2`). Nothing
                // in the repo's docs or tests ever spells two bare names side by side on
                // purpose, so `x y` used to silently return `x * y` with a plausible-looking
                // unit and no error at all.
                let (line, col) = next_pair.line_col();
                let right = parse_factor(next_pair)?;
                let right_bare_identifier = as_bare_identifier(&right);
                if let (Some(l_name), Some(r_name)) = (&prev_bare_identifier, &right_bare_identifier) {
                    return Err(PhysureError::Generic(format!(
                        "--> {line}:{col}\nMissing operator between '{l_name}' and '{r_name}': PhysureScript does \
not read two bare names side by side as a product. Write `{l_name} * {r_name}` if you meant to \
multiply them, or add whatever operator belongs between them."
                    )));
                }
                prev_bare_identifier = right_bare_identifier;
                left = Expr::BinaryOp {
                    op: BinaryOp::Mul,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            }
            _ => return Err(PhysureError::Generic(format!("Unexpected rule in term: {:?}", next_pair.as_rule()))),
        }
    }
    Ok(left)
}

fn parse_factor(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut inner = pair.into_inner();
    let mut primary_pair = inner.next().unwrap();
    let negate = primary_pair.as_rule() == Rule::op_sub;
    if negate {
        primary_pair = inner.next().unwrap();
    }

    let left = match primary_pair.as_rule() {
        Rule::quantity => parse_quantity(primary_pair)?,
        Rule::number => parse_number_quantity(primary_pair)?,
        Rule::method_call => parse_method_call(primary_pair)?,
        Rule::function_call => parse_function_call(primary_pair)?,
        Rule::identifier => Expr::Identifier(primary_pair.as_str().to_string()),
        Rule::string_lit => Expr::Str(primary_pair.as_str().trim_matches('"').to_string()),
        Rule::if_expr => parse_if_expr(primary_pair)?,
        Rule::for_expr => parse_for_expr(primary_pair)?,
        Rule::vector_literal => parse_vector_literal(primary_pair)?,
        Rule::expr => parse_expr(primary_pair)?,
        Rule::base_expr => parse_base_expr(primary_pair)?,
        Rule::conv_expr => parse_conv_expr(primary_pair)?,
        Rule::comp_expr => parse_comp_expr(primary_pair)?,
        _ => return Err(PhysureError::Generic(format!("Unexpected rule in factor: {:?}", primary_pair.as_rule()))),
    };

    let result = if let Some(op_pair) = inner.next() {
        if op_pair.as_rule() == Rule::op_pow {
            let right_pair = inner.next().unwrap();
            let right = parse_factor(right_pair)?;
            Expr::BinaryOp {
                op: BinaryOp::Pow,
                left: Box::new(left),
                right: Box::new(right),
            }
        } else {
            left
        }
    } else {
        left
    };

    if negate {
        Ok(Expr::BinaryOp {
            op: BinaryOp::Mul,
            left: Box::new(Expr::Quantity(QuantityNode { magnitude: -1.0, uncertainty: None, uncertainty_lower: None, is_sigma: false, unit: None })),
            right: Box::new(result),
        })
    } else {
        Ok(result)
    }
}

fn parse_if_expr(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut inner = pair.into_inner();
    let cond = parse_expr(inner.next().unwrap())?;
    let then_e = parse_expr(inner.next().unwrap())?;
    let else_e = parse_expr(inner.next().unwrap())?;
    Ok(Expr::FunctionCall {
        name: "if_then_else".to_string(),
        args: vec![cond, then_e, else_e],
        kwargs: Vec::new(),
    })
}

fn parse_for_expr(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut inner = pair.into_inner();
    let first = inner.next().unwrap();
    let var = if first.as_rule() == Rule::_for_kw {
        inner.next().unwrap().as_str().to_string()
    } else {
        first.as_str().to_string()
    };
    let iterable = Box::new(parse_expr(inner.next().unwrap())?);
    let body = Box::new(parse_expr(inner.next().unwrap())?);
    Ok(Expr::ForExpr { var, iterable, body })
}


/// Desugars `body where a = 1, b = a * 2` into nested `FunctionCall { name: "let", args: [name,
/// value, body] }`, a form the interpreter special-cases to bind `name` in a local scope before
/// evaluating `body` (see `PhsInterpreter::eval_expr`) without needing a dedicated `Expr` variant.
/// The nesting runs in source order so a later binding can use an earlier one.
fn parse_where_expr(pair: pest::iterators::Pair<Rule>, body: Expr) -> PhysureResult<Expr> {
    let mut bindings = Vec::new();
    for bind in pair.into_inner() {
        let mut parts = bind.into_inner();
        let name = parts
            .next()
            .ok_or_else(|| PhysureError::Generic("A `where` binding is missing its name".into()))?
            .as_str()
            .to_string();
        let value_pair = parts
            .next()
            .ok_or_else(|| PhysureError::Generic(format!("`where {}` is missing its value", name)))?;
        bindings.push((name, parse_base_expr(value_pair)?));
    }
    Ok(bindings.into_iter().rev().fold(body, |acc, (name, value)| Expr::FunctionCall {
        name: "let".to_string(),
        args: vec![Expr::Identifier(name), value, acc],
        kwargs: Vec::new(),
    }))
}

fn parse_vector_literal(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut elems = Vec::new();
    let mut unit_str = None;
    
    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::unit_expr => {
                unit_str = Some(inner.as_str().to_string());
            }
            Rule::expr => {
                elems.push(parse_expr(inner)?);
            }
            _ => {}
        }
    }
    
    let vec_expr = Expr::FunctionCall {
        name: "vector".to_string(),
        args: elems,
        kwargs: Vec::new(),
    };
    
    if let Some(u) = unit_str {
        Ok(Expr::BinaryOp {
            op: BinaryOp::Mul,
            left: Box::new(vec_expr),
            right: Box::new(Expr::Identifier(u)),
        })
    } else {
        Ok(vec_expr)
    }
}

fn parse_number_quantity(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let s = pair.as_str().trim();
    let mag = match s {
        "inf" | "+inf" | "infinity" | "+infinity" | "oo" | "+oo" | "∞" | "+∞" => f64::INFINITY,
        "-inf" | "-infinity" | "-oo" | "-∞" => f64::NEG_INFINITY,
        _ => s.parse::<f64>().map_err(|_| PhysureError::Generic(format!("Invalid number: {}", s)))?,
    };
    Ok(Expr::Quantity(QuantityNode {
        magnitude: mag,
        uncertainty: None,
        uncertainty_lower: None,
        is_sigma: false,
        unit: None,
    }))
}

fn parse_function_call(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut name = String::new();
    let mut args = Vec::new();
    let mut kwargs = Vec::new();

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::identifier => {
                name = inner.as_str().to_string();
            }
            Rule::call_arg => {
                let mut arg_inner = inner.into_inner();
                let first = arg_inner.next().unwrap();
                if first.as_rule() == Rule::identifier {
                    let kwarg_name = first.as_str().to_string();
                    let value = parse_expr(arg_inner.next().unwrap())?;
                    kwargs.push((kwarg_name, value));
                } else {
                    args.push(parse_expr(first)?);
                }
            }
            _ => {}
        }
    }

    Ok(Expr::FunctionCall { name, args, kwargs })
}

fn parse_method_call(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut inner = pair.into_inner();
    let base_pair = inner.next().unwrap();
    let mut current_expr = match base_pair.as_rule() {
        Rule::quantity => parse_quantity(base_pair)?,
        Rule::function_call => parse_function_call(base_pair)?,
        Rule::identifier => Expr::Identifier(base_pair.as_str().to_string()),
        Rule::string_lit => Expr::Str(base_pair.as_str().trim_matches('"').to_string()),
        Rule::expr => parse_expr(base_pair)?,
        _ => parse_base_expr(base_pair)?,
    };

    while let Some(method_item) = inner.next() {
        if method_item.as_rule() == Rule::identifier {
            let method_name = method_item.as_str().to_string();
            let mut args = vec![current_expr];
            let mut kwargs = Vec::new();

            while let Some(call_arg_pair) = inner.peek() {
                if call_arg_pair.as_rule() == Rule::call_arg {
                    let arg_pair = inner.next().unwrap();
                    let mut arg_inner = arg_pair.into_inner();
                    let first = arg_inner.next().unwrap();
                    if first.as_rule() == Rule::identifier && arg_inner.peek().is_some() {
                        let kwarg_name = first.as_str().to_string();
                        let value = parse_expr(arg_inner.next().unwrap())?;
                        kwargs.push((kwarg_name, value));
                    } else {
                        args.push(parse_expr(first)?);
                    }
                } else {
                    break;
                }
            }
            current_expr = Expr::FunctionCall { name: method_name, args, kwargs };
        }
    }

    Ok(current_expr)
}

