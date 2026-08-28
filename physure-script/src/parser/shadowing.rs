use std::collections::HashSet;
use physure_core::error::{PhysureError, PhysureResult};
use crate::ast::*;

/// One word of a quantity literal's unit chain, together with where it sits in the chain text.
struct UnitToken {
    text: String,
    /// The `*` or `/` that introduced this word, and its byte offset. `None` for the first
    /// word, which juxtaposition — never multiplication — attached to the number. It is
    /// still checked against `bound` like every later word (see `check_expr_shadowing`); the
    /// distinction only changes the rewrite `unit_shadowing_error` suggests.
    op: Option<(char, usize)>,
}

/// Splits a quantity literal's unit chain (`"kg * g"`, `"N * m ^ 2 / C ^ 2"`, `"m / (s * s)"`)
/// into its words, recording the operator that introduced each one.
///
/// Exponent digits are dropped: `s ^ 2` contributes the word `s`, and the bare `2` can never
/// name a variable. Parentheses are structure, not content, so they are simply skipped — a
/// word inside a group is still introduced by whatever `*` or `/` preceded the group.
fn unit_chain_tokens(unit: &str) -> Vec<UnitToken> {
    let bytes = unit.as_bytes();
    let mut tokens = Vec::new();
    let mut pending_op: Option<(char, usize)> = None;
    let mut in_exponent = false;
    let mut idx = 0usize;

    while idx < unit.len() {
        let ch = unit[idx..].chars().next().unwrap();
        let ch_len = ch.len_utf8();

        if ch == '*' || ch == '/' {
            pending_op = Some((ch, idx));
            in_exponent = false;
            idx += ch_len;
            continue;
        }
        if ch == '^' {
            in_exponent = true;
            idx += ch_len;
            continue;
        }
        if ch.is_alphanumeric() || matches!(ch, '_' | '°' | '%' | '$') {
            let start = idx;
            let mut end = idx;
            while end < unit.len() {
                let c = unit[end..].chars().next().unwrap();
                if c.is_alphanumeric() || matches!(c, '_' | '°' | '%' | '$') {
                    end += c.len_utf8();
                } else {
                    break;
                }
            }
            // A run that starts with a digit is an exponent or an embedded power, never a name.
            if !in_exponent && !bytes[start].is_ascii_digit() {
                tokens.push(UnitToken {
                    text: unit[start..end].to_string(),
                    op: pending_op.take(),
                });
            }
            idx = end;
            continue;
        }
        idx += ch_len;
    }

    tokens
}

/// The error raised when a unit-chain word is also a name the script bound.
///
/// The two suggested rewrites are the escape hatches that already exist in the grammar, spelled
/// out against the user's own literal rather than described abstractly. The head word (`op ==
/// None`) needs a different rewrite from a later chain word: parenthesising the *magnitude*
/// doesn't help there, because a parenthesised magnitude is just another `quantity` head and
/// still greedily reattaches the following word as its unit — `(3) m` parses exactly like
/// `3 m`. An explicit `*` is what actually changes the parse for the head word, on both sides:
/// a bare identifier to its right reads as the variable, and a quoted string reads as a unit
/// (the interpreter parses a string operand as a full unit *expression*, not a single symbol,
/// so this also covers a compound head like `m/s`).
fn unit_shadowing_error(magnitude: f64, unit: &str, token: &UnitToken, line: usize, col: usize) -> PhysureError {
    let literal = format!("{} {}", magnitude, unit);

    let (as_variable, as_unit) = match &token.op {
        Some((op_char, op_at)) => {
            let head = unit[..*op_at].trim();
            // The tail starts just past the operator rather than at the word itself, so a
            // group's opening "(" — which sits between the two — stays attached: `kg * (g *
            // m)` must be suggested back as `(g * m)`, not as the unbalanced `g * m)`.
            let tail = unit[op_at + op_char.len_utf8()..].trim();
            (
                format!("({} {}) {} {}", magnitude, head, op_char, tail),
                format!("{} {} {} \"{}\"", magnitude, head, op_char, tail),
            )
        }
        None => (
            format!("{} * {}", magnitude, unit),
            format!("{} * \"{}\"", magnitude, unit),
        ),
    };

    PhysureError::Generic(format!(
        "--> {line}:{col}\nAmbiguous '{token}' in the quantity literal `{literal}`: '{token}' is a registered unit \
symbol and also a name this script binds earlier, and PhysureScript will not guess which one you \
meant. Write `{as_variable}` to multiply by the variable, or `{as_unit}` to keep the unit.",
        line = line,
        col = col,
        token = token.text,
    ))
}

/// Rejects quantity literals whose unit chain names a variable the script already bound.
///
/// `unit_expr` is greedy: `f = 10.0 kg * g` parses as one literal whose unit is `kg * g`, so
/// with `g = 9.81 m/s^2` in scope the author's 98.1 N silently becomes a mass squared — and the
/// same wrong reading reaches every codegen target, since they all consume this AST. Guessing
/// either way would be a confident wrong answer, so the ambiguity is reported instead.
///
/// The walk is in source order because a name bound *after* a use site does not shadow it; in
/// particular `g = 9.81 m / s ^ 2` must not flag its own `g`, whose binding only takes effect
/// once the right-hand side has been read.
pub(crate) fn validate_unit_shadowing(statements: &[Statement], statement_pos: &[(usize, usize)]) -> PhysureResult<()> {
    let mut bound: HashSet<String> = HashSet::new();
    for (i, stmt) in statements.iter().enumerate() {
        let (line, col) = statement_pos.get(i).copied().unwrap_or((1, 1));
        check_statement_shadowing(stmt, &mut bound, line, col)?;
    }
    Ok(())
}

fn check_statement_shadowing(stmt: &Statement, bound: &mut HashSet<String>, line: usize, col: usize) -> PhysureResult<()> {
    match stmt {
        Statement::Assignment(node) => {
            check_expr_shadowing(&node.value, bound, line, col)?;
            bound.insert(node.name.clone());
        }
        Statement::FunctionDef(node) => {
            // The header binds the name before the body is read, so a body may refer to it.
            bound.insert(node.name.clone());
            // Parameters and the body's own locals live in a scope of their own: they must not
            // leak out and shadow units in the statements that follow the definition.
            let mut local = bound.clone();
            local.extend(node.params.iter().cloned());
            for body_stmt in &node.body_stmts {
                check_statement_shadowing(body_stmt, &mut local, line, col)?;
            }
        }
        Statement::Expr(expr) | Statement::Return(expr) => check_expr_shadowing(expr, bound, line, col)?,
        Statement::GuardReturn { cond, value } => {
            check_expr_shadowing(cond, bound, line, col)?;
            check_expr_shadowing(value, bound, line, col)?;
        }
        Statement::While { cond, body, .. } => {
            check_expr_shadowing(cond, bound, line, col)?;
            for s in body {
                check_statement_shadowing(s, bound, line, col)?;
            }
        }
        Statement::Import(_) | Statement::Export(_) => {}
    }
    Ok(())
}

fn check_expr_shadowing(expr: &Expr, bound: &HashSet<String>, line: usize, col: usize) -> PhysureResult<()> {
    match expr {
        Expr::Quantity(node) => {
            let Some(unit) = &node.unit else { return Ok(()) };
            for token in unit_chain_tokens(unit) {
                // The first word used to be exempt — juxtaposition is never multiplication, so
                // `3 m` was read as a quantity even in a script that bound `m`. But that same
                // exemption let `fn kinetic_energy(m, v) = 1/2 m v^2` silently read its own `m`
                // parameter as the *metre* attached to the `2` split out of `1/2`, dropping the
                // mass argument entirely (see unit_shadowing.rs's
                // `the_kinetic_energy_shorthand_that_motivated_this_check_is_rejected`). The
                // first word is checked exactly like every later one now, at the cost of also
                // flagging an ordinary standalone literal whose unit happens to collide with a
                // name in scope — still cheaper than a confident wrong answer.
                if bound.contains(&token.text) {
                    return Err(unit_shadowing_error(node.magnitude, unit, &token, line, col));
                }
            }
            Ok(())
        }
        Expr::Identifier(_) | Expr::Str(_) | Expr::Bool(_) => Ok(()),
        Expr::BinaryOp { left, right, .. } => {
            check_expr_shadowing(left, bound, line, col)?;
            check_expr_shadowing(right, bound, line, col)
        }
        Expr::FunctionCall { name, args, kwargs } => {
            // `where` desugars to `let(name, value, body)` (see `parse_where_expr`), and the
            // binding is only visible in the body — its own value expression predates it.
            if name == "let" && args.len() == 3 {
                if let Expr::Identifier(var) = &args[0] {
                    check_expr_shadowing(&args[1], bound, line, col)?;
                    let mut local = bound.clone();
                    local.insert(var.clone());
                    return check_expr_shadowing(&args[2], &local, line, col);
                }
            }
            for arg in args {
                check_expr_shadowing(arg, bound, line, col)?;
            }
            for (_, value) in kwargs {
                check_expr_shadowing(value, bound, line, col)?;
            }
            Ok(())
        }
        Expr::ForExpr { var, iterable, body } => {
            check_expr_shadowing(iterable, bound, line, col)?;
            let mut local = bound.clone();
            local.insert(var.clone());
            check_expr_shadowing(body, &local, line, col)
        }
    }
}

