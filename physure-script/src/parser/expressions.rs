use physure_core::error::{PhysureError, PhysureResult};
use crate::ast::*;
use super::Rule;
use super::quantities::parse_quantity;

/// The deepest expression tree the parser builds; a leaf is one level, and so is every operator of a chain:
/// `1 + 1 + 1` is `(1 + 1) + 1`, three levels. A flat chain is no nesting of brackets, but the checker, the
/// evaluator, `Clone` and `Drop` all recurse over the tree it makes, and a stack overflow ends the whole process
/// (200,000 terms ended `phs conf verify --run`).
///
/// Measured in a debug build on a 1 MiB stack (a Windows main thread), the most expensive pass that has no stack
/// check of its own is the static checker: 349 levels of `not` or of method calls (about 3 KB a level; `Clone` and
/// `Drop` hold about 1,050, the parser about 490). 128 leaves a factor of 2.7. The evaluator costs more (16 to 30 KB
/// a level in a debug build, 3 to 11 KB in release): a sandboxed run checks its own stack budget and stops with a
/// `stack limit` error, which is what keeps it safe, not this bound. A formula written by hand is far below it (a sum
/// of a hundred terms is about a hundred levels).
pub const MAX_EXPRESSION_DEPTH: usize = 128;

fn too_deep() -> PhysureError {
    PhysureError::Generic(format!(
        "Parse error: expression too deeply nested: at most {MAX_EXPRESSION_DEPTH} levels (every operator of a chain \
         such as `a + b + c` is one more level; split it into named parts)"
    ))
}

/// Depth of `expr`, counted without recursion and without walking past `MAX_EXPRESSION_DEPTH + 1`.
fn expr_depth(expr: &Expr) -> usize {
    let mut deepest = 0;
    let mut pending = vec![(expr, 1usize)];
    while let Some((node, depth)) = pending.pop() {
        deepest = deepest.max(depth);
        if depth > MAX_EXPRESSION_DEPTH {
            break;
        }
        match node {
            Expr::BinaryOp { left, right, .. } => pending.extend([(&**left, depth + 1), (&**right, depth + 1)]),
            Expr::FunctionCall { args, kwargs, .. } => {
                pending.extend(args.iter().map(|arg| (arg, depth + 1)));
                pending.extend(kwargs.iter().map(|(_, arg)| (arg, depth + 1)));
            }
            Expr::ForExpr { iterable, body, .. } => pending.extend([(&**iterable, depth + 1), (&**body, depth + 1)]),
            Expr::Quantity(_) | Expr::Identifier(_) | Expr::Str(_) | Expr::Bool(_) => {}
        }
    }
    deepest
}

/// `expr`, or the error when its tree is deeper than `MAX_EXPRESSION_DEPTH`. Every tier that builds a chain calls
/// it on its result, so a tier always starts from operands within the bound, and its own loop stops after
/// `MAX_EXPRESSION_DEPTH` operators (see [`count_operator`]): the tree an error drops is never more than twice
/// the bound deep.
fn bounded(expr: Expr) -> PhysureResult<Expr> {
    if expr_depth(&expr) > MAX_EXPRESSION_DEPTH {
        return Err(too_deep());
    }
    Ok(expr)
}

/// Counts one more operator of a chain, refusing the one that makes the chain alone deeper than the bound.
fn count_operator(count: &mut usize) -> PhysureResult<()> {
    *count += 1;
    if *count >= MAX_EXPRESSION_DEPTH {
        return Err(too_deep());
    }
    Ok(())
}

/// `expr` after a chain of `operators`: unchanged (already bounded) when there were none.
fn bounded_chain(expr: Expr, operators: usize) -> PhysureResult<Expr> {
    if operators == 0 {
        Ok(expr)
    } else {
        bounded(expr)
    }
}

pub(crate) fn parse_expr(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut inner = pair.into_inner();
    let first = match inner.next() {
        Some(f) => f,
        None => return Ok(Expr::Quantity(QuantityNode { magnitude: 0.0, uncertainty: None, uncertainty_lower: None, is_sigma: false, dof: None, unit: None })),
    };
    let mut result = match first.as_rule() {
        Rule::if_expr => parse_if_expr(first)?,
        Rule::for_expr => parse_for_expr(first)?,
        Rule::or_expr => parse_or_expr(first)?,
        Rule::base_expr => parse_base_expr(first)?,
        Rule::comp_expr => parse_comp_expr(first)?,
        Rule::conv_expr => parse_conv_expr(first)?,
        _ => parse_add_expr(first)?,
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
    bounded(result)
}

/// The loosest tier: the one optional `..` and the format spec that closes the expression.
fn parse_base_expr(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut inner = pair.into_inner();
    let first = inner.next().unwrap();
    let mut left = parse_comp_expr(first)?;

    while let Some(op_pair) = inner.next() {
        // The spec closes the expression — `a + b: .2f` formats the sum, not `b`. The spec is a word of the
        // syntax, so it is text (`format(x, "base")`): a variable named `base` does not change it.
        if op_pair.as_rule() == Rule::op_format {
            let spec = op_pair.into_inner().next().map(|p| p.as_str().to_string()).unwrap_or_default();
            left = Expr::FunctionCall {
                name: "format".to_string(),
                args: vec![left, Expr::Str(spec)],
                kwargs: Vec::new(),
            };
            continue;
        }
        if op_pair.as_rule() != Rule::op_range {
            return Err(PhysureError::Generic(format!("Unexpected op in base_expr: {:?}", op_pair.as_rule())));
        }
        // A missing endpoint is a grammar error, so `inner.next()` is the endpoint the
        // grammar already required rather than something that might not be there.
        let right = parse_comp_expr(inner.next().unwrap())?;
        left = Expr::BinaryOp {
            op: BinaryOp::Range,
            left: Box::new(left),
            right: Box::new(right),
        };
    }

    bounded(left)
}

/// `term ((+|-) term)*`, left-associative.
fn parse_add_expr(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut inner = pair.into_inner();
    let mut left = parse_term(inner.next().unwrap())?;
    let mut operators = 0;

    while let Some(op_pair) = inner.next() {
        count_operator(&mut operators)?;
        let op = match op_pair.as_rule() {
            Rule::op_add => BinaryOp::Add,
            Rule::op_sub => BinaryOp::Sub,
            other => return Err(PhysureError::Generic(format!("Unexpected op in add_expr: {other:?}"))),
        };
        let right = parse_term(inner.next().unwrap())?;
        left = Expr::BinaryOp { op, left: Box::new(left), right: Box::new(right) };
    }

    bounded_chain(left, operators)
}

/// `add_expr (=> unit)*`, left-associative: `a + b => km` converts the sum. A `+`/`-` after the last unit
/// (`add_after_convert`) is refused with the parentheses to write.
fn parse_conv_expr(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let start = pair.as_span().start();
    let mut inner = pair.into_inner();
    let first = inner.next().unwrap();
    let mut left = parse_add_expr(first)?;
    let mut operators = 0;
    let mut converted = String::new();

    while let Some(op_pair) = inner.next() {
        if op_pair.as_rule() == Rule::add_after_convert {
            return Err(sum_after_conversion(&op_pair, start, &converted));
        }
        count_operator(&mut operators)?;
        let right_pair = inner.next().unwrap();
        // `=>`'s target is a `unit_expr`, not an expression (see phs.pest's `conv_expr`) --
        // every consumer of this side (the interpreter's `BinaryOp::Convert` arm, every
        // codegen target) reads it back through `expr_to_unit_string`, which flattens an
        // `Expr` to unit-chain text and only special-cases `Identifier`/`Quantity`/`BinaryOp`.
        // Reusing the matched text as one `Identifier` is exactly what that flattening would
        // produce for a real unit chain anyway, and it sidesteps rebuilding `unit_expr`'s
        // internal `*`/`/` structure into an equivalent `Expr` tree for no benefit -- nothing
        // ever inspects this side's shape, only its flattened text.
        converted = right_pair.as_str().trim().to_string();
        left = Expr::BinaryOp {
            op: BinaryOp::Convert,
            left: Box::new(left),
            right: Box::new(Expr::Identifier(converted.clone())),
        };
    }

    bounded_chain(left, operators)
}

/// The error for `x => km - y`: the target of `=>` is a unit, so the `-` has no reading, and the one it had before
/// the order was standard (`(x => km) - y`) is not what the order says. The message writes the parentheses.
fn sum_after_conversion(tail: &pest::iterators::Pair<Rule>, start: usize, unit: &str) -> PhysureError {
    let input = tail.get_input();
    let converted = input[start..tail.as_span().start()].trim_end();
    let rest = tail.as_str().trim();
    let op = &rest[..1];
    let operand = rest[1..].trim_start();
    let (line, col) = tail.line_col();
    PhysureError::Generic(format!(
        "Parse error:  --> {line}:{col}\nafter `=> {unit}`, a `{op}` needs parentheses: write `({converted}) {op} {operand}` \
         (the target of `=>` is a unit, and arithmetic binds tighter than the conversion)"
    ))
}

/// `conv_expr (op conv_expr)?`: one comparison. A further comparison (`comparison_chain`) is refused.
fn parse_comp_expr(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut inner = pair.into_inner();
    let first = inner.next().unwrap();
    let left = parse_conv_expr(first)?;
    let Some(op_pair) = inner.next() else { return Ok(left) };
    let right = parse_conv_expr(inner.next().unwrap())?;
    if let Some(chain) = inner.next() {
        let (line, col) = chain.line_col();
        return Err(PhysureError::Generic(format!(
            "Parse error:  --> {line}:{col}\na comparison chain has no meaning here: write `a < b and b < c` \
             (`{}` compares the result of the comparison before it)",
            chain.as_str().trim()
        )));
    }
    bounded(Expr::FunctionCall {
        name: format!("op_{}", op_pair.as_str()),
        args: vec![left, right],
        kwargs: Vec::new(),
    })
}

fn parse_bool_lit(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    Ok(Expr::Bool(pair.as_str() == "True"))
}

/// `_not_kw* ~ base_expr` -- zero or more `not` prefixes wrapping one `base_expr`. Wrapping
/// in a plain loop (rather than tracking parity) keeps `not not x` and `not x` both correct
/// without special-casing an even count.
fn parse_not_expr(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut not_count = 0usize;
    let mut base_pair = None;
    for p in pair.into_inner() {
        if p.as_rule() == Rule::_not_kw {
            count_operator(&mut not_count)?;
        } else {
            base_pair = Some(p);
        }
    }
    // The grammar (`not_expr = { _not_kw* ~ base_expr }`) guarantees exactly one
    // non-`_not_kw` child: `base_expr`.
    let mut result = parse_base_expr(base_pair.unwrap())?;
    for _ in 0..not_count {
        result = Expr::FunctionCall { name: "op_not".to_string(), args: vec![result], kwargs: Vec::new() };
    }
    bounded_chain(result, not_count)
}

fn parse_and_expr(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut inner = pair.into_inner();
    let first = inner.next().unwrap();
    let mut left = parse_not_expr(first)?;
    let mut operators = 0;
    while inner.next().is_some() {
        count_operator(&mut operators)?;
        // The consumed pair is `_and_kw`; `and_expr = { not_expr ~ (_nl ~ _and_kw ~ _nl ~
        // not_expr)* }` guarantees a `not_expr` immediately follows it.
        let right = parse_not_expr(inner.next().unwrap())?;
        left = Expr::FunctionCall { name: "op_and".to_string(), args: vec![left, right], kwargs: Vec::new() };
    }
    bounded_chain(left, operators)
}

fn parse_or_expr(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut inner = pair.into_inner();
    let first = inner.next().unwrap();
    let mut left = parse_and_expr(first)?;
    let mut operators = 0;
    while inner.next().is_some() {
        count_operator(&mut operators)?;
        // The consumed pair is `_or_kw`; `or_expr = { and_expr ~ (_nl ~ _or_kw ~ _nl ~
        // and_expr)* }` guarantees an `and_expr` immediately follows it.
        let right = parse_and_expr(inner.next().unwrap())?;
        left = Expr::FunctionCall { name: "op_or".to_string(), args: vec![left, right], kwargs: Vec::new() };
    }
    bounded_chain(left, operators)
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
    let mut operators = 0;

    while let Some(next_pair) = inner.next() {
        count_operator(&mut operators)?;
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
    bounded_chain(left, operators)
}

fn parse_factor(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut inner = pair.into_inner();
    let mut primary_pair = inner.next().unwrap();
    let negate = primary_pair.as_rule() == Rule::op_sub;
    if negate {
        primary_pair = inner.next().unwrap();
    }

    let left = parse_primary_pair(primary_pair)?;

    let result = if let Some(op_pair) = inner.next() {
        if op_pair.as_rule() == Rule::op_pow {
            let right_pair = inner.next().unwrap();
            let right = parse_primary_pair(right_pair)?;
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
            left: Box::new(Expr::Quantity(QuantityNode { magnitude: -1.0, uncertainty: None, uncertainty_lower: None, is_sigma: false, dof: None, unit: None })),
            right: Box::new(result),
        })
    } else {
        Ok(result)
    }
}

/// The expression a primary pair stands for. Every primary is a `method_call` pair (the base, then
/// zero or more `.name(...)`), so the base is read through here too.
fn parse_primary_pair(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    match pair.as_rule() {
        Rule::quantity => parse_quantity(pair),
        Rule::number => parse_number_quantity(pair),
        Rule::method_call => parse_method_call(pair),
        Rule::function_call => parse_function_call(pair),
        Rule::identifier => Ok(Expr::Identifier(pair.as_str().to_string())),
        Rule::string_lit => Ok(Expr::Str(pair.as_str().trim_matches('"').to_string())),
        Rule::bool_lit => parse_bool_lit(pair),
        Rule::if_expr => parse_if_expr(pair),
        Rule::for_expr => parse_for_expr(pair),
        Rule::vector_literal => parse_vector_literal(pair),
        Rule::expr => parse_expr(pair),
        Rule::base_expr => parse_base_expr(pair),
        Rule::conv_expr => parse_conv_expr(pair),
        Rule::comp_expr => parse_comp_expr(pair),
        Rule::add_expr => parse_add_expr(pair),
        other => Err(PhysureError::Generic(format!("Unexpected rule in factor: {:?}", other))),
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
        if bindings.len() >= MAX_EXPRESSION_DEPTH {
            return Err(too_deep());
        }
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
            right: Box::new(super::quantities::unit_literal(&u)),
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
        dof: None,
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

/// The `call_arg`s that follow a method name: a `name value` pair is a keyword argument.
fn parse_method_args(
    inner: &mut pest::iterators::Pairs<Rule>,
    args: &mut Vec<Expr>,
    kwargs: &mut Vec<(String, Expr)>,
) -> PhysureResult<()> {
    while inner.peek().is_some_and(|p| p.as_rule() == Rule::call_arg) {
        let mut arg_inner = inner.next().unwrap().into_inner();
        let first = arg_inner.next().unwrap();
        if first.as_rule() == Rule::identifier && arg_inner.peek().is_some() {
            let value = parse_expr(arg_inner.next().unwrap())?;
            kwargs.push((first.as_str().to_string(), value));
        } else {
            args.push(parse_expr(first)?);
        }
    }
    Ok(())
}

fn parse_method_call(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut inner = pair.into_inner();
    let mut current_expr = parse_primary_pair(inner.next().unwrap())?;
    let mut operators = 0;

    while let Some(method_item) = inner.next() {
        if method_item.as_rule() == Rule::identifier {
            count_operator(&mut operators)?;
            let mut args = vec![current_expr];
            let mut kwargs = Vec::new();
            parse_method_args(&mut inner, &mut args, &mut kwargs)?;
            current_expr = Expr::FunctionCall { name: method_item.as_str().to_string(), args, kwargs };
        }
    }

    bounded_chain(current_expr, operators)
}

