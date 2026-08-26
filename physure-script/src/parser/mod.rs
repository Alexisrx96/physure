pub(crate) mod expressions;
use expressions::parse_expr;
pub(crate) mod statements;
use pest::Parser;
use pest_derive::Parser;
use physure_core::error::{PhysureError, PhysureResult};
use physure_core::UnitRegistry;
use std::collections::HashSet;
use std::sync::OnceLock;
use crate::ast::*;

#[derive(Parser)]
#[grammar = "phs.pest"]
pub struct PhsParser;

pub fn parse_phs(code: &str) -> PhysureResult<Program> {
    let pairs = PhsParser::parse(Rule::program, code)
        .map_err(|e| PhysureError::Generic(format!("Parse error: {}", e)))?;
    
    let mut statements = Vec::new();
    let mut lines = Vec::new();
    let mut statement_pos = Vec::new();
    for pair in pairs {
        if pair.as_rule() == Rule::stmt {
            let (line, col) = pair.line_col();
            let inner = pair.into_inner().next().unwrap();
            statements.push(statements::parse_statement(inner)?);
            lines.push(line);
            statement_pos.push((line, col));
        }
    }

    validate_unit_shadowing(&statements, &statement_pos)?;
    crate::decorators::validate_decorators(&statements)?;
    Ok(Program { statements, lines })
}

pub fn parse_phs_with_lines(code: &str) -> PhysureResult<Vec<(usize, Statement)>> {
    let pairs = PhsParser::parse(Rule::program, code)
        .map_err(|e| PhysureError::Generic(format!("Parse error: {}", e)))?;

    let mut statements = Vec::new();
    let mut statement_pos = Vec::new();
    for pair in pairs {
        if pair.as_rule() == Rule::stmt {
            let (line, col) = pair.line_col();
            let inner = pair.into_inner().next().unwrap();
            statements.push((line - 1, statements::parse_statement(inner)?));
            statement_pos.push((line, col));
        }
    }

    let stmts_only: Vec<Statement> = statements.iter().map(|(_, s)| s.clone()).collect();
    validate_unit_shadowing(&stmts_only, &statement_pos)?;
    crate::decorators::validate_decorators(&stmts_only)?;

    Ok(statements)
}

/// Cached full unit registry (master `physure.conf` + prefixes) used to decide whether a
/// `unit_expr` token names a real unit. Must match what the interpreter resolves quantity
/// units against (`physure_core::units::parser::Parser::parse_expression`, see
/// `interpreter.rs`) so that a token accepted here — including prefixed symbols like "mA"
/// or "kOhm" — is guaranteed to resolve the same way at evaluation time.
fn unit_registry() -> &'static UnitRegistry {
    static REGISTRY: OnceLock<UnitRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| physure_core::units::conf::build_registry_from_conf().0)
}

/// True if `name` resolves to a registered unit. Used to tell a unit symbol apart from an
/// unbound variable when checking that a keyword call to an equation supplies every unknown:
/// in `"y = x * 2.0 m"` the `m` is metres, not a variable the caller forgot to pass.
pub(crate) fn is_known_unit_symbol(name: &str) -> bool {
    unit_registry().get_unit(name).is_some()
}

/// True if a `unit_term` pair (e.g. "kg", "r ^ 2", "m2", "a0", "Å") names a registered
/// unit. The optional `^exp` suffix is irrelevant to the lookup.
///
/// A trailing digit run is ambiguous: in "m2" it is an embedded exponent (metre squared),
/// in "a0" it is part of the symbol itself (the Bohr radius). `physure_core`'s unit parser
/// already resolves that ambiguity, so try the whole symbol first and fall back to the
/// digit-stripped stem — matching what evaluation will do with the same text.
///
/// The scan must not be restricted to ASCII: `unit_term` accepts any Unicode `LETTER`, so
/// an ASCII-only check silently rejects registered symbols like "Å" and "°" and hands them
/// to the expression parser, where they are not valid identifiers either.
fn unit_term_is_registered(text: &str, registry: &UnitRegistry) -> bool {
    let symbol: String = text
        .trim_start()
        .chars()
        .take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '°' | '%' | '$'))
        .collect();
    if symbol.is_empty() {
        return false;
    }
    if registry.get_unit(&symbol).is_some() {
        return true;
    }
    let stem = symbol.trim_end_matches(|c: char| c.is_ascii_digit());
    stem.len() < symbol.len() && !stem.is_empty() && registry.get_unit(stem).is_some()
}

/// True if every `unit_term` inside `pair` (recursing through parenthesized groups) names
/// a registered unit. Used to decide whether a group like `(kg * s^2)` is consumed
/// wholesale as part of a quantity's unit.
fn unit_expr_all_valid(pair: pest::iterators::Pair<Rule>, registry: &UnitRegistry) -> bool {
    pair.into_inner().all(|child| match child.as_rule() {
        Rule::unit_term => unit_term_is_registered(child.as_str(), registry),
        Rule::unit_expr => unit_expr_all_valid(child, registry),
        _ => false,
    })
}

/// Splits a `unit_expr` pair into the longest valid leading run of registered units
/// (returned as a unit string) plus, if that run doesn't consume the whole match, the
/// operator connecting it to the raw source text of everything after it.
///
/// `unit_expr` is purely syntactic — any `ASCII_ALPHA+` chain joined by `*`/`/`/whitespace
/// parses as one, so pest alone can't tell a real unit symbol from a variable reference.
/// Concretely, `1.602e-19 C / r ^ 2` on one line (no statement boundary before `/ r`) would
/// otherwise have its `unit_expr` greedily swallow `C / r ^ 2` as the literal's unit even
/// though `r` is a bound function parameter, fabricating a bogus `r` dimension and silently
/// dropping the real division at evaluation time. Validating against the registry here lets
/// the leftover be handed back to the expression grammar as a normal variable reference.
fn split_unit_expr(pair: pest::iterators::Pair<Rule>, registry: &UnitRegistry) -> (Option<String>, Option<(BinaryOp, String)>) {
    let base_str = pair.as_str();
    let base_start = pair.as_span().start();
    let mut consumed_end_rel = 0usize;

    for child in pair.into_inner() {
        let span = child.as_span();
        let valid = match child.as_rule() {
            Rule::unit_term => unit_term_is_registered(child.as_str(), registry),
            Rule::unit_expr => unit_expr_all_valid(child, registry),
            _ => false,
        };
        if valid {
            consumed_end_rel = span.end() - base_start;
            continue;
        }

        let child_start_rel = span.start() - base_start;
        let gap = &base_str[consumed_end_rel..child_start_rel];
        let op = if gap.contains('/') { BinaryOp::Div } else { BinaryOp::Mul };
        // Start the remainder right after the operator (if any) rather than at the
        // invalid child's own span start, so a parenthesized group's "(" — which lies in
        // `gap`, before the child's span — stays attached to the remainder text instead
        // of being dropped (e.g. "kg / (r^2)" must give back "(r^2)", not "r^2)").
        let op_offset = gap.find(|c| c == '*' || c == '/').map(|i| i + 1).unwrap_or(0);
        let remainder = base_str[consumed_end_rel + op_offset..].trim().to_string();
        let unit = if consumed_end_rel == 0 {
            None
        } else {
            Some(base_str[..consumed_end_rel].trim().to_string())
        };
        return (unit, Some((op, remainder)));
    }

    (Some(base_str.trim().to_string()), None)
}

/// Parses text left over after `split_unit_expr` stops consuming a unit chain (e.g.
/// "r ^ 2" or "(r^2)") as a normal expression, so it participates in the AST as a variable
/// reference / arithmetic instead of a fabricated unit dimension.
fn parse_unit_leftover(text: &str) -> PhysureResult<Expr> {
    let mut pairs = PhsParser::parse(Rule::expr, text)
        .map_err(|e| PhysureError::Generic(format!("Parse error in quantity unit remainder '{}': {}", text, e)))?;
    let pair = pairs
        .next()
        .ok_or_else(|| PhysureError::Generic(format!("Empty quantity unit remainder '{}'", text)))?;
    parse_expr(pair)
}

/// Reads one half of an uncertainty — `0.5`, `0.5%` or `2 sigma` — into its value and whether
/// it was written as a percentage. `is_sigma` is shared across the halves because it describes
/// the whole measurement, not one side of it.
fn parse_uncertainty_val(raw: &str, is_sigma: &mut bool) -> PhysureResult<(f64, bool)> {
    let mut val_str = raw.trim().to_string();
    let is_percent = val_str.ends_with('%');
    if is_percent {
        val_str.pop();
    }
    if val_str.contains("sigma") || val_str.contains("σ") {
        *is_sigma = true;
        val_str = val_str.replace("sigma", "").replace("σ", "");
    }
    let value = val_str
        .trim()
        .parse::<f64>()
        .map_err(|_| PhysureError::Generic("Invalid uncertainty".to_string()))?;
    Ok((value, is_percent))
}

fn parse_quantity(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut magnitude = None;
    let mut magnitude_expr = None;
    let mut unit = None;
    let mut unit_leftover: Option<(BinaryOp, String)> = None;

    let mut is_sigma = false;
    // (value, was written as a percentage), upper half first.
    let mut halves: Vec<(f64, bool)> = Vec::new();

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::number => {
                let s = inner.as_str().trim();
                let mag = match s {
                    "inf" | "+inf" | "infinity" | "+infinity" | "oo" | "+oo" | "∞" | "+∞" => f64::INFINITY,
                    "-inf" | "-infinity" | "-oo" | "-∞" => f64::NEG_INFINITY,
                    _ => s.parse::<f64>().map_err(|_| PhysureError::Generic(format!("Invalid number: {}", s)))?,
                };
                magnitude = Some(mag);
            }
            Rule::expr => {
                magnitude_expr = Some(parse_expr(inner)?);
            }
            Rule::uncertainty => {
                // One value for `+/- 0.5`, two for the asymmetric `+/- (0.5, 0.4)`, upper
                // first. `uncertainty_pair` nests them, so the halves are collected by rule
                // rather than by position.
                for u_inner in inner.into_inner() {
                    match u_inner.as_rule() {
                        Rule::uncertainty_val => halves.push(parse_uncertainty_val(u_inner.as_str(), &mut is_sigma)?),
                        Rule::uncertainty_pair => {
                            for half in u_inner.into_inner() {
                                if half.as_rule() == Rule::uncertainty_val {
                                    halves.push(parse_uncertainty_val(half.as_str(), &mut is_sigma)?);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            Rule::unit_expr => {
                let (u, leftover) = split_unit_expr(inner, unit_registry());
                unit = u;
                unit_leftover = leftover;
            }
            _ => {}
        }
    }

    // `+/- 0.5%` is a *relative* uncertainty. The percent sign was stripped and then never
    // applied, so `9.81 +/- 0.5% m/s^2` claimed ±0.5 instead of ±0.049 — twenty times too
    // wide, and nothing in the output said so.
    if halves.iter().any(|(_, is_percent)| *is_percent) {
        let base = magnitude.or(match &magnitude_expr {
            Some(Expr::Quantity(q)) => Some(q.magnitude),
            _ => None,
        });
        match base {
            Some(mag) => {
                for (value, is_percent) in halves.iter_mut() {
                    if *is_percent {
                        *value = mag.abs() * *value / 100.0;
                    }
                }
            }
            // Anything else only knows its magnitude at run time, and a percentage of an
            // unknown is not a number: say so instead of inventing one.
            None => {
                return Err(PhysureError::Generic(
                    "A percent uncertainty needs a literal magnitude to apply to; use an absolute value like `+/- 0.05`".into(),
                ))
            }
        }
    }

    // `+/- (0.5, 0.4)` is +0.5 and -0.4: upper half first, in the order the operator reads.
    // That convention lives here and nowhere else — swapping these two lines (and the
    // matching note in `phs.pest`) is the whole change if the other order ever wins.
    let uncertainty = halves.first().map(|(v, _)| *v);
    let uncertainty_lower = halves.get(1).map(|(v, _)| *v);

    let quantity_expr = if let Some(mag) = magnitude {
        Expr::Quantity(QuantityNode {
            magnitude: mag,
            uncertainty,
            uncertainty_lower,
            is_sigma,
            unit,
        })
    } else if let Some(mag_expr) = magnitude_expr {
        if let Some(u) = unit {
            if let Expr::Quantity(mut q) = mag_expr {
                q.unit = Some(u);
                q.is_sigma = is_sigma;
                q.uncertainty = uncertainty.or(q.uncertainty);
                q.uncertainty_lower = uncertainty_lower.or(q.uncertainty_lower);
                Expr::Quantity(q)
            } else {
                Expr::BinaryOp {
                    op: BinaryOp::Mul,
                    left: Box::new(mag_expr),
                    right: Box::new(Expr::Identifier(u)),
                }
            }
        } else {
            mag_expr
        }
    } else {
        return Err(PhysureError::Generic("Missing magnitude in quantity".to_string()));
    };

    match unit_leftover {
        Some((op, remainder)) => {
            let remainder_expr = parse_unit_leftover(&remainder)?;
            Ok(Expr::BinaryOp {
                op,
                left: Box::new(quantity_expr),
                right: Box::new(remainder_expr),
            })
        }
        None => Ok(quantity_expr),
    }
}

// ---------------------------------------------------------------------------
// Unit/variable ambiguity
// ---------------------------------------------------------------------------

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
fn validate_unit_shadowing(statements: &[Statement], statement_pos: &[(usize, usize)]) -> PhysureResult<()> {
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
        Expr::Identifier(_) | Expr::Str(_) => Ok(()),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_phs_records_line_numbers_for_top_level_function_and_while_bodies() {
        // PHS function bodies are indentation-delimited, not brace-delimited (only `while` uses
        // braces -- confirmed against phs.pest's `function_def = "fn" ~ ... ~ "=" ~ (block_body |
        // expr)` and `block_body = (_nl_indent ~ stmt)+`, and against a working example already in
        // the test suite: physure-script/tests/unit_shadowing.rs's `"fn f(x) =\n    t = 2.0 s\n
        // 5 m / t\n"`).
        let script = "x = 1\nfn f(a) =\n  a = a + 1\n  a\nwhile x < 3 {\n  x = x + 1\n}\n";
        let program = parse_phs(script).unwrap();

        assert_eq!(program.lines.len(), program.statements.len());
        assert_eq!(program.lines[0], 1); // x = 1

        let Statement::FunctionDef(f) = &program.statements[1] else { panic!("expected fn") };
        assert_eq!(f.body_lines.len(), f.body_stmts.len());
        assert_eq!(f.body_lines[0], 3); // a = a + 1
        assert_eq!(f.body_lines[1], 4); // a

        let Statement::While { body, body_lines, .. } = &program.statements[2] else { panic!("expected while") };
        assert_eq!(body_lines.len(), body.len());
        assert_eq!(body_lines[0], 6); // x = x + 1
    }

    /// `ternary_op` is a rule of its own, so `expr` sees it as a single child rather than
    /// as two loose `base_expr`s — reading the branches off `expr` panicked on the second.
    #[test]
    fn test_ternary_branches_come_from_the_ternary_rule() {
        for code in ["z > 2 ? 100 : 200 where z = 3", "5 m > 2 m ? 1 kg : 2 kg", "1 > 0 ? 2 m : 3 m"] {
            let prog = parse_phs(code).unwrap_or_else(|e| panic!("{code:?} failed to parse: {e:?}"));
            let expr = match &prog.statements[0] {
                Statement::Expr(e) => e,
                other => panic!("{code:?} produced {other:?}"),
            };
            // A `where` clause wraps the ternary, so look for the call anywhere in the tree.
            let rendered = format!("{expr:?}");
            assert!(rendered.contains("ternary"), "{code:?} did not build a ternary: {rendered}");
        }
    }

    #[test]
    fn test_explicit_imports() {
        let code1 = "use g, c as speed_of_light from \"physics/constants\"";
        let prog1 = parse_phs(code1).unwrap();
        assert_eq!(prog1.statements.len(), 1);
        if let Statement::Import(imp) = &prog1.statements[0] {
            assert_eq!(imp.path, "physics/constants");
            if let ImportSpecifier::Symbols(syms) = &imp.specifier {
                assert_eq!(syms[0].name, "g");
                assert_eq!(syms[1].name, "c");
                assert_eq!(syms[1].alias.as_deref(), Some("speed_of_light"));
            } else { panic!("expected symbols"); }
        } else { panic!("expected import"); }

        let code2 = "use * from \"physics/thermodynamics\"";
        let prog2 = parse_phs(code2).unwrap();
        if let Statement::Import(imp) = &prog2.statements[0] {
            assert_eq!(imp.path, "physics/thermodynamics");
            assert!(matches!(imp.specifier, ImportSpecifier::Wildcard));
        } else { panic!("expected import"); }

        let code3 = "import \"physics/constants\" as consts";
        let prog3 = parse_phs(code3).unwrap();
        if let Statement::Import(imp) = &prog3.statements[0] {
            assert_eq!(imp.path, "physics/constants");
            if let ImportSpecifier::ModuleAlias(alias) = &imp.specifier {
                assert_eq!(alias, "consts");
            } else { panic!("expected module alias"); }
        } else { panic!("expected import"); }
    }

    #[test]
    fn test_param_unit_annotation_accepts_ohm_symbol() {
        let code = "potencia2(i: A, R: \u{3a9}) = i^2 * R";
        let prog = parse_phs(&code).unwrap();
        if let Statement::FunctionDef(f) = &prog.statements[0] {
            assert_eq!(f.param_units, vec![Some("A".to_string()), Some("\u{3a9}".to_string())]);
        } else {
            panic!("expected function def");
        }
    }

    /// `1/2 m v^2` used to parse clean, but never evaluated to kinetic energy: `2` (split out
    /// of `1/2`) swallows `m` as the unit metre before the interpreter ever sees it as the
    /// mass parameter, so the body silently computed `1 / (2 m) * v^2` instead of `0.5 * m *
    /// v^2` — see `unit_shadowing.rs`'s
    /// `the_kinetic_energy_shorthand_that_motivated_this_check_is_rejected`. It is now a
    /// resolve-time ambiguity error naming `m`, not a value.
    #[test]
    fn test_natural_function_definition_shorthand_is_rejected_as_ambiguous() {
        let code = "fn kinetic_energy(m, v) = 1/2 m v^2";
        let err = parse_phs(code).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains('m'), "expected the ambiguity to name 'm': {msg}");
    }

    /// Two bare identifiers with only whitespace between them (`x y`) used to be read as
    /// `x * y` by `term`'s `_is_implicit_mul` — the same juxtaposition rule `1/2 m v^2` relies
    /// on to skip the `*` between a coefficient and a symbol. But nothing in the repo's docs,
    /// README examples, or test suite ever spells two *bare names* side by side on purpose,
    /// and it reads exactly like a forgotten operator: `total = masa velocidad` used to
    /// silently return `masa * velocidad` with a plausible-looking unit and no error at all.
    #[test]
    fn test_bare_identifier_juxtaposition_is_rejected() {
        let err = parse_phs("x y").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains('x') && msg.contains('y'), "expected both names in the error: {msg}");
    }

    /// The check has to look at the two factors actually being joined, not just the first
    /// pair in the term — an explicit `*` earlier in the chain must not exempt a later bare
    /// juxtaposition from the same rule.
    #[test]
    fn test_bare_identifier_juxtaposition_is_rejected_later_in_a_chain() {
        let err = parse_phs("x * y z").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains('y') && msg.contains('z'), "expected 'y' and 'z' in the error: {msg}");
    }

    /// Only bare-identifier-next-to-bare-identifier is banned — a quantity next to an
    /// identifier is untouched, since that is the coefficient/unit-chain pattern the language
    /// actually documents and relies on. `"m"` here is not bound by anything else in the
    /// script, so the quantity `2 m` itself is unambiguous; `x` is the separate factor
    /// `term`'s implicit multiplication still joins it to.
    #[test]
    fn test_quantity_next_to_bare_identifier_juxtaposition_still_parses() {
        let prog = parse_phs("2 m x").unwrap();
        assert!(matches!(&prog.statements[0], Statement::Expr(_)));
    }

    #[test]
    fn test_function_def_param_unit_annotation() {
        let code = "E_campo(r: m) =\n    r\n";
        let prog = parse_phs(code).unwrap();
        if let Statement::FunctionDef(f) = &prog.statements[0] {
            assert_eq!(f.name, "E_campo");
            assert_eq!(f.params, vec!["r"]);
            assert_eq!(f.param_units, vec![Some("m".to_string())]);
        } else { panic!("expected func def"); }
    }

    #[test]
    fn test_quantity_literals() {
        let code = "m = 75.0 ± 0.5 kg";
        let prog = parse_phs(code).unwrap();
        if let Statement::Assignment(a) = &prog.statements[0] {
            assert_eq!(a.name, "m");
            if let Expr::Quantity(q) = &a.value {
                assert_eq!(q.magnitude, 75.0);
                assert_eq!(q.uncertainty, Some(0.5));
                assert_eq!(q.unit.as_deref(), Some("kg"));
            } else { panic!("expected quantity"); }
        } else { panic!("expected assignment"); }

        let code = "m = 75.0 +/- 0.5 kg";
        let prog = parse_phs(code).unwrap();
        if let Statement::Assignment(a) = &prog.statements[0] {
            if let Expr::Quantity(q) = &a.value {
                assert_eq!(q.magnitude, 75.0);
                assert_eq!(q.uncertainty, Some(0.5));
                assert_eq!(q.unit.as_deref(), Some("kg"));
            }
        }

        let code = "v = 10 m/s";
        let prog = parse_phs(code).unwrap();
        if let Statement::Assignment(a) = &prog.statements[0] {
            if let Expr::Quantity(q) = &a.value {
                assert_eq!(q.magnitude, 10.0);
                assert_eq!(q.uncertainty, None);
                assert_eq!(q.unit.as_deref(), Some("m/s"));
            }
        }
    }

    /// Pulls the quantity out of `x = <quantity>`, panicking if it is anything else.
    fn parse_one_quantity(code: &str) -> QuantityNode {
        let prog = parse_phs(code).unwrap_or_else(|e| panic!("{code} did not parse: {e}"));
        match &prog.statements[0] {
            Statement::Assignment(a) => match &a.value {
                Expr::Quantity(q) => q.clone(),
                other => panic!("{code} parsed as {other:?}"),
            },
            other => panic!("{code} parsed as {other:?}"),
        }
    }

    #[test]
    fn an_asymmetric_uncertainty_keeps_both_halves() {
        // `12.3 +/- (0.5, 0.4)` reads in the order the operator does: upper first.
        for code in ["x = 12.3 +/- (0.5, 0.4) m", "x = 12.3 ± (0.5, 0.4) m"] {
            let q = parse_one_quantity(code);
            assert_eq!(q.magnitude, 12.3);
            assert_eq!(q.uncertainty, Some(0.5), "{code}");
            assert_eq!(q.uncertainty_lower, Some(0.4), "{code}");
            assert_eq!(q.unit.as_deref(), Some("m"));
        }
    }

    #[test]
    fn a_parenthesised_addend_is_not_an_uncertainty_pair() {
        // The whole risk in the notation is that `(` after a sign could be read two ways.
        // `+` alone never reaches the uncertainty rule, so this stays an addition.
        let prog = parse_phs("x = 12.3 + (0.5)").unwrap();
        let Statement::Assignment(a) = &prog.statements[0] else { panic!("expected assignment") };
        assert!(matches!(a.value, Expr::BinaryOp { op: BinaryOp::Add, .. }), "{:?}", a.value);
    }

    #[test]
    fn each_half_of_a_pair_takes_its_own_percentage() {
        let q = parse_one_quantity("x = 200.0 +/- (1%, 0.5) m");
        assert_eq!(q.uncertainty, Some(2.0));
        assert_eq!(q.uncertainty_lower, Some(0.5));
    }

    #[test]
    fn a_symmetric_uncertainty_has_no_lower_half() {
        assert_eq!(parse_one_quantity("x = 75.0 +/- 0.5 kg").uncertainty_lower, None);
    }

    #[test]
    fn test_exports() {
        let code = "export E as \"kinetic_energy\"";
        let prog = parse_phs(code).unwrap();
        if let Statement::Export(e) = &prog.statements[0] {
            assert_eq!(e.symbol, "E");
            assert_eq!(e.export_name, "kinetic_energy");
        } else { panic!("expected export"); }
    }

    #[test]
    fn test_assignment_fn_standalone() {
        let code = "f(v: m / s) =\n    resta = 1 m / s\n    v * 2 - resta";
        let pairs = PhsParser::parse(Rule::assignment_fn, code);
        assert!(pairs.is_ok());
    }

    #[test]
    fn test_decorated_stmt_rule_parses() {
        let code = "@stable\nfn f(x) = x";
        let pairs = PhsParser::parse(Rule::decorated_stmt, code);
        assert!(pairs.is_ok(), "expected decorated_stmt to parse: {:?}", pairs.err());
    }

    #[test]
    fn test_decorator_with_args_rule_parses() {
        let pairs = PhsParser::parse(Rule::decorator, "@requires(x > 0.0, \"x must be positive\")");
        assert!(pairs.is_ok(), "expected decorator with args to parse: {:?}", pairs.err());
    }

    #[test]
    fn test_decorated_stmt_rule_parses_stacked_decorators() {
        let code = "@stable\n@requires(x > 0.0, \"x must be positive\")\nfn f(x) = x";
        let pairs = PhsParser::parse(Rule::decorated_stmt, code);
        assert!(pairs.is_ok(), "expected stacked decorated_stmt to parse: {:?}", pairs.err());
    }

    #[test]
    fn test_parse_phs_attaches_decorators_to_function_def() {
        let program = parse_phs("@stable\nfn f(x) = x").unwrap();
        match &program.statements[0] {
            Statement::FunctionDef(node) => {
                assert_eq!(node.decorators.len(), 1);
                assert_eq!(node.decorators[0].name, "stable");
                assert!(node.decorators[0].args.is_empty());
            }
            other => panic!("expected FunctionDef, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_phs_attaches_decorator_args() {
        let program = parse_phs("@requires(x > 0.0, \"x must be positive\")\nfn f(x) = x").unwrap();
        match &program.statements[0] {
            Statement::FunctionDef(node) => {
                assert_eq!(node.decorators.len(), 1);
                assert_eq!(node.decorators[0].name, "requires");
                assert_eq!(node.decorators[0].args.len(), 2);
            }
            other => panic!("expected FunctionDef, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_phs_attaches_stacked_decorators_to_function_def() {
        let program = parse_phs("@stable\n@requires(x > 0.0, \"x must be positive\")\nfn f(x) = x").unwrap();
        match &program.statements[0] {
            Statement::FunctionDef(node) => {
                assert_eq!(node.decorators.len(), 2);
                assert_eq!(node.decorators[0].name, "stable");
                assert_eq!(node.decorators[1].name, "requires");
            }
            other => panic!("expected FunctionDef, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_phs_attaches_decorator_to_assignment() {
        let program = parse_phs("@stable\nx = 5").unwrap();
        match &program.statements[0] {
            Statement::Assignment(node) => {
                assert_eq!(node.decorators.len(), 1);
                assert_eq!(node.decorators[0].name, "stable");
            }
            other => panic!("expected Assignment, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_phs_rejects_unknown_decorator() {
        assert!(parse_phs("@bogus\nfn f(x) = x").is_err());
    }

    #[test]
    fn test_parse_phs_lowers_range_into_two_requires() {
        let program = parse_phs("@range(v, 0.0, 10.0)\nfn f(v) = v").unwrap();
        match &program.statements[0] {
            Statement::FunctionDef(node) => {
                assert_eq!(node.decorators.len(), 2);
                assert!(node.decorators.iter().all(|d| d.name == "requires"));
            }
            other => panic!("expected FunctionDef, got {:?}", other),
        }
    }

    #[test]
    fn doc_comment_attaches_to_function_def() {
        let program = parse_phs("/// Computes kinetic energy.\nfn ke(m, v) = 0.5 * m * v^2").unwrap();
        match &program.statements[0] {
            Statement::FunctionDef(node) => {
                assert_eq!(node.doc.as_deref(), Some("Computes kinetic energy."));
            }
            other => panic!("expected FunctionDef, got {:?}", other),
        }
    }

    #[test]
    fn multiline_doc_comment_joins_with_newline() {
        let program = parse_phs(
            "/// Line one.\n/// Line two.\nfn ke(m, v) = 0.5 * m * v^2",
        ).unwrap();
        match &program.statements[0] {
            Statement::FunctionDef(node) => {
                assert_eq!(node.doc.as_deref(), Some("Line one.\nLine two."));
            }
            other => panic!("expected FunctionDef, got {:?}", other),
        }
    }

    #[test]
    fn doc_comment_stacks_above_decorators() {
        let program = parse_phs(
            "/// Computes kinetic energy.\n@stable\nfn ke(m, v) = 0.5 * m * v^2",
        ).unwrap();
        match &program.statements[0] {
            Statement::FunctionDef(node) => {
                assert_eq!(node.doc.as_deref(), Some("Computes kinetic energy."));
                assert_eq!(node.decorators.len(), 1);
                assert_eq!(node.decorators[0].name, "stable");
            }
            other => panic!("expected FunctionDef, got {:?}", other),
        }
    }

    #[test]
    fn plain_double_slash_comment_still_parses() {
        let program = parse_phs("// just a comment\nfn ke(m, v) = 0.5 * m * v^2").unwrap();
        match &program.statements[0] {
            Statement::FunctionDef(node) => assert_eq!(node.doc, None),
            other => panic!("expected FunctionDef, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_1_cargas() {
        if let Ok(code) = std::fs::read_to_string("D:/Projects/test_physure/1_cargas.phs") {
            let res = parse_phs(&code);
            assert!(res.is_ok());
        }
    }

    #[test]
    fn test_parse_for_expr_and_while_stmt() {
        let script = "for t in 1 .. 5 {\n t * 2 \n}\nwhile x > 0 {\n x = x - 1 \n}";
        let stmts = parse_phs(script).unwrap().statements;
        assert_eq!(stmts.len(), 2);
        assert!(matches!(&stmts[0], Statement::Expr(Expr::ForExpr { .. })));
        assert!(matches!(&stmts[1], Statement::While { .. }));
    }

    #[test]
    fn test_parse_loop_newlines_before_brace() {
        let script = "for\n item\n in\n 1 .. 5\n {\n item * 2\n }\nwhile\n x > 0\n {\n x = x - 1\n }";
        let stmts = parse_phs(script).unwrap().statements;
        assert_eq!(stmts.len(), 2);
        assert!(matches!(&stmts[0], Statement::Expr(Expr::ForExpr { .. })));
        assert!(matches!(&stmts[1], Statement::While { .. }));
    }

    #[test]
    fn test_parse_while_multi_statement() {
        let script = "while x > 0 {\n a = x * 2\n x = x - 1\n }";
        let stmts = parse_phs(script).unwrap().statements;
        assert_eq!(stmts.len(), 1);
        if let Statement::While { cond: _, body, body_lines: _ } = &stmts[0] {
            assert_eq!(body.len(), 2);
        } else {
            panic!("expected While statement");
        }
    }

    #[test]
    fn test_parse_nested_loops() {
        let script = "while x > 0 {\n y = for i in 1 .. 3 {\n i * x\n }\n x = x - 1\n }";
        let stmts = parse_phs(script).unwrap().statements;
        assert_eq!(stmts.len(), 1);
        if let Statement::While { cond: _, body, body_lines: _ } = &stmts[0] {
            assert_eq!(body.len(), 2);
            assert!(matches!(&body[0], Statement::Assignment(a) if matches!(a.value, Expr::ForExpr { .. })));
        } else {
            panic!("expected While statement");
        }
    }

    #[test]
    fn test_parse_loop_keyword_prefix_identifiers() {
        let script = "for_item = 1\nwhile_count = 10\nfor_item + while_count";
        let stmts = parse_phs(script).unwrap().statements;
        assert_eq!(stmts.len(), 3);
        assert!(matches!(&stmts[0], Statement::Assignment(a) if a.name == "for_item"));
        assert!(matches!(&stmts[1], Statement::Assignment(a) if a.name == "while_count"));
    }
}

