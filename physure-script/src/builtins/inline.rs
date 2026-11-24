//! The user's functions, inlined into the expression string of a calc or plot builtin: `deriv("f(2*t)", "t")` with
//! `f(x) = x^2` differentiates `((2*t) ^ 2)`.
//!
//! A call is found in the string as a name the lexer reads followed by `(`, its arguments as the text between its
//! commas at depth one. The body is printed from the function's syntax tree with each parameter replaced by its
//! argument, all at once (so `h(b, a)` swaps), every operation and every argument in parentheses: the text reads
//! as the same tree in the symbolic parser and in the PHS parser, which are the two that read the result.
//!
//! What a symbolic expression cannot carry is refused, naming it, never dropped: a recursive function, a wrong
//! number of arguments, a parameter with a declared unit (which converts and checks its argument), a decorator (whose
//! check would not run), an uncertain literal, a body of more than one expression, and the parts of the language that
//! are not algebra (comparisons, conditionals, strings, vectors, conversions, ranges, loops), and a name of the module
//! an imported function comes from (the expression would read it where it is evaluated, not in the module).

use std::collections::HashMap;

use physure_core::error::{PhysureError, PhysureResult};

use crate::ast::{BinaryOp, Expr, FunctionDefNode, QuantityNode, Statement};
use crate::interpreter::PhsInterpreter;
use crate::value::PhsValue;
use crate::{PhsLexer, PhsToken, TokenKind};

/// `text` with every call of a user function in it replaced by the function's body. The functions are those of
/// `scope` (where the builtin is called), then those of the interpreter that `scope` does not name.
pub(crate) fn preprocess_symbolic_expression(
    text: &str,
    interpreter: &PhsInterpreter,
    scope: &HashMap<String, PhsValue>,
) -> PhysureResult<String> {
    let mut functions: HashMap<&str, &FunctionDefNode> = HashMap::new();
    for (name, value) in interpreter.env.iter().chain(scope.iter()) {
        match value {
            PhsValue::Function(func) => {
                functions.insert(name, func);
            }
            // a variable of the scope hides a function of the same name
            _ => {
                functions.remove(name.as_str());
            }
        }
    }
    Inliner { functions }.text(text.trim(), &mut Vec::new())
}

fn refusal(message: String) -> PhysureError {
    PhysureError::Generic(message)
}

struct Inliner<'a> {
    functions: HashMap<&'a str, &'a FunctionDefNode>,
}

impl Inliner<'_> {
    /// `text` with its calls of user functions inlined. `stack` holds the functions being inlined, outermost first.
    fn text(&self, text: &str, stack: &mut Vec<String>) -> PhysureResult<String> {
        if !self.functions.keys().any(|name| text.contains(name)) {
            return Ok(text.to_string());
        }
        let tokens = PhsLexer::new(text).tokenize()?;
        let mut out = String::with_capacity(text.len());
        let mut copied = 0;
        let mut i = 0;
        while i < tokens.len() {
            let is_call = matches!(&tokens[i].kind, TokenKind::Ident(name) if self.functions.contains_key(name.as_str()))
                && tokens.get(i + 1).is_some_and(|t| is_op(t, "("));
            if !is_call {
                i += 1;
                continue;
            }
            let name = &tokens[i].value;
            let (arguments, close) = call_arguments(text, &tokens, i + 1, name)?;
            let arguments = arguments.into_iter().map(|a| self.text(a, stack)).collect::<PhysureResult<Vec<_>>>()?;
            out.push_str(&text[copied..tokens[i].pos]);
            out.push_str(&self.call(name, &arguments, stack)?);
            copied = tokens[close].pos + 1;
            i = close + 1;
        }
        out.push_str(&text[copied..]);
        Ok(out)
    }

    /// The body of the user function `name` with `arguments` (texts already inlined) for its parameters, in
    /// parentheses.
    fn call(&self, name: &str, arguments: &[String], stack: &mut Vec<String>) -> PhysureResult<String> {
        if let Some(first) = stack.iter().position(|n| n == name) {
            let cycle = stack[first..].join(" -> ");
            return Err(refusal(format!(
                "'{name}' cannot be inlined into a symbolic expression: it is recursive ({cycle} -> {name})"
            )));
        }
        let func = self.functions[name];
        check_inlinable(name, func, arguments.len())?;
        check_reads_no_module_name(name, func)?;
        let body = match func.body_stmts.as_slice() {
            [Statement::Expr(e)] | [Statement::Return(e)] => e,
            [_] => {
                return Err(refusal(format!(
                    "'{name}' cannot be inlined into a symbolic expression: its body is a statement, not an expression"
                )))
            }
            body => {
                return Err(refusal(format!(
                    "'{name}' cannot be inlined into a symbolic expression: its body is {} statements, and only a \
                     body that is one expression can be",
                    body.len()
                )))
            }
        };
        let substitution: HashMap<&str, &str> =
            func.params.iter().map(String::as_str).zip(arguments.iter().map(String::as_str)).collect();
        stack.push(name.to_string());
        let printed = self.expr(body, &substitution, name, stack);
        stack.pop();
        Ok(format!("({})", printed?))
    }

    /// `e` (a part of the body of `owner`) as text, with the arguments in `substitution` for the parameters.
    fn expr(
        &self,
        e: &Expr,
        substitution: &HashMap<&str, &str>,
        owner: &str,
        stack: &mut Vec<String>,
    ) -> PhysureResult<String> {
        let cannot = |what: &str| {
            refusal(format!("'{owner}' cannot be inlined into a symbolic expression: its body holds {what}"))
        };
        match e {
            Expr::Identifier(name) => Ok(match substitution.get(name.as_str()) {
                Some(argument) => format!("({argument})"),
                None => name.clone(),
            }),
            Expr::Quantity(q) => quantity(q).map_err(|what| cannot(&what)),
            Expr::BinaryOp { op, left, right } => {
                let symbol = match op {
                    BinaryOp::Add => "+",
                    BinaryOp::Sub => "-",
                    BinaryOp::Mul => "*",
                    BinaryOp::Div => "/",
                    BinaryOp::Pow => "^",
                    BinaryOp::Convert => return Err(cannot("a conversion `=>`")),
                    BinaryOp::Range => return Err(cannot("a range `..`")),
                };
                let mut left_text = self.expr(left, substitution, owner, stack)?;
                // a name before `^` in parentheses: the symbolic parser reads `rate^2` as `rat * e^2`
                if *op == BinaryOp::Pow && matches!(**left, Expr::Identifier(_)) && !left_text.starts_with('(') {
                    left_text = format!("({left_text})");
                }
                let right_text = self.expr(right, substitution, owner, stack)?;
                Ok(format!("({left_text} {symbol} {right_text})"))
            }
            Expr::FunctionCall { name, args, kwargs } => {
                if let Some(what) = not_algebra(name) {
                    return Err(cannot(what));
                }
                if !kwargs.is_empty() {
                    return Err(cannot(&format!("a call of '{name}' with keyword arguments")));
                }
                if substitution.contains_key(name.as_str()) {
                    return Err(cannot(&format!("a call of its parameter '{name}'")));
                }
                let arguments =
                    args.iter().map(|a| self.expr(a, substitution, owner, stack)).collect::<PhysureResult<Vec<_>>>()?;
                if self.functions.contains_key(name.as_str()) {
                    self.call(name, &arguments, stack)
                } else {
                    Ok(format!("{name}({})", arguments.join(", ")))
                }
            }
            Expr::Str(s) => Err(cannot(&format!("the string {s:?}"))),
            Expr::Bool(b) => Err(cannot(&format!("the boolean {}", if *b { "True" } else { "False" }))),
            Expr::ForExpr { .. } => Err(cannot("a `for` loop")),
        }
    }
}

/// Refuses `func` when what makes it more than its body cannot come along: its arity, its declared units, its
/// decorators.
fn check_inlinable(name: &str, func: &FunctionDefNode, given: usize) -> PhysureResult<()> {
    if func.params.len() != given {
        return Err(refusal(format!(
            "'{name}' takes {} argument{} ({}), {given} given",
            func.params.len(),
            if func.params.len() == 1 { "" } else { "s" },
            func.params.join(", ")
        )));
    }
    if let Some((param, unit)) =
        func.params.iter().zip(&func.param_units).find_map(|(p, u)| u.as_ref().map(|u| (p, u)))
    {
        return Err(refusal(format!(
            "'{name}' cannot be inlined into a symbolic expression: its parameter '{param}' declares the unit {unit}, \
             which converts and checks the argument, and a symbolic expression cannot carry that"
        )));
    }
    if let Some(decorator) = func.decorators.first() {
        return Err(refusal(format!(
            "'{name}' cannot be inlined into a symbolic expression: it carries @{}, which the inlined body would not \
             apply",
            decorator.name
        )));
    }
    Ok(())
}

/// Refuses an imported `func` whose body names something of its module (a value or a function), which the inlined
/// text would look up where the expression is evaluated instead.
fn check_reads_no_module_name(name: &str, func: &FunctionDefNode) -> PhysureResult<()> {
    let Some(scope) = &func.scope else { return Ok(()) };
    let mut used = Vec::new();
    for stmt in &func.body_stmts {
        if let Statement::Expr(e) | Statement::Return(e) = stmt {
            names_in(e, &mut used);
        }
    }
    match used.into_iter().find(|n| !func.params.contains(n) && scope.get(n).is_some()) {
        Some(used) => Err(refusal(format!(
            "'{name}' cannot be inlined into a symbolic expression: it uses '{used}' of {}, which the expression would              read where it is evaluated instead",
            if scope.module() == "the caller" { "the caller".to_string() } else { format!("the module {}", scope.module()) }
        ))),
        None => Ok(()),
    }
}

/// Every name `e` reads: its identifiers and the functions it calls.
fn names_in(e: &Expr, out: &mut Vec<String>) {
    match e {
        Expr::Identifier(name) => out.push(name.clone()),
        Expr::BinaryOp { left, right, .. } => {
            names_in(left, out);
            names_in(right, out);
        }
        Expr::FunctionCall { name, args, kwargs } => {
            out.push(name.clone());
            args.iter().chain(kwargs.iter().map(|(_, a)| a)).for_each(|a| names_in(a, out));
        }
        Expr::ForExpr { iterable, body, .. } => {
            names_in(iterable, out);
            names_in(body, out);
        }
        Expr::Quantity(_) | Expr::Str(_) | Expr::Bool(_) => {}
    }
}

/// What a call the parser builds for a part of the language that is not algebra stands for.
fn not_algebra(name: &str) -> Option<&'static str> {
    Some(match name {
        "if_then_else" | "ternary" => "a conditional",
        "let" => "a `where` binding",
        "vector" => "a vector",
        "format" => "a formatted string",
        _ if name.starts_with("op_") => "a comparison or a logical operator",
        _ => return None,
    })
}

/// A literal of the body as text (a negative number or a quantity in parentheses), or what it holds that cannot be.
fn quantity(q: &QuantityNode) -> Result<String, String> {
    let unit = q.unit.as_deref().unwrap_or("").trim();
    let written = if unit.is_empty() { format!("{}", q.magnitude) } else { format!("{} {}", q.magnitude, unit) };
    if q.uncertainty.is_some() || q.uncertainty_lower.is_some() || q.is_sigma || q.dof.is_some() {
        return Err(format!("the uncertain literal {written} (a symbolic expression carries no uncertainty)"));
    }
    if !q.magnitude.is_finite() {
        return Err(format!("the literal {written}"));
    }
    Ok(if unit.is_empty() && q.magnitude >= 0.0 { written } else { format!("({written})") })
}

fn is_op(token: &PhsToken, op: &str) -> bool {
    matches!(&token.kind, TokenKind::Op(o) if o == op)
}

/// The argument texts of the call of `name` whose `(` is `tokens[open]`, and the index of its `)`.
fn call_arguments<'t>(
    text: &'t str,
    tokens: &[PhsToken],
    open: usize,
    name: &str,
) -> PhysureResult<(Vec<&'t str>, usize)> {
    let mut arguments = Vec::new();
    let mut depth = 0usize;
    let mut start = tokens[open].pos + 1;
    for (index, token) in tokens.iter().enumerate().skip(open) {
        if is_op(token, "(") || is_op(token, "[") {
            depth += 1;
        } else if is_op(token, ")") || is_op(token, "]") {
            depth -= 1;
            if depth == 0 {
                let last = text[start..token.pos].trim();
                // `f()` has no arguments; `f(a, )` has an empty one
                if !(last.is_empty() && arguments.is_empty()) {
                    arguments.push(last);
                }
                if arguments.iter().any(|a| a.is_empty()) {
                    return Err(refusal(format!("the call of '{name}' in {text:?} has an empty argument")));
                }
                return Ok((arguments, index));
            }
        } else if depth == 1 && is_op(token, ",") {
            arguments.push(text[start..token.pos].trim());
            start = token.pos + 1;
        } else if depth == 1 && is_op(token, "=") {
            return Err(refusal(format!(
                "the call of '{name}' in {text:?} names an argument, and an inlined call takes its arguments by position"
            )));
        }
    }
    Err(refusal(format!("the call of '{name}' in {text:?} is not closed")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The functions every case may call: one per kind of body the inliner prints or refuses.
    const DEFINITIONS: &[&str] = &[
        "fn sq(x) = x^2",
        "fn lin(a, b) = a * b / 2 - a + b",
        "fn free(x) = x + k",
        "fn kp(x) = k^x + (k)^2",
        "fn lit(x) = x * 2.5 + (-3) * 4 m",
        "fn big(x) = x * inf",
        "fn unc(x) = x + 2 +/- 0.1",
        "fn conv(x) = x => m",
        "fn rng(x) = x..3",
        "fn pw(x) = (x + 1)^3",
        "fn nest(x) = sq(x) + sin(x)",
        "fn cond(x) = if x > 0 then x else 0",
        "fn wh(x) = y * 2 where y = x",
        "fn vec(x) = [x, 1]",
        "fn fmt(x) = format(\"{}\", x)",
        "fn cmp(x) = x > 1",
        "fn kw(x) = round(x, digits = 2)",
        "fn pcall(f) = f(1)",
        "fn text(x) = \"hello\"",
        "fn yes(x) = True",
        "fn no(x) = False",
        "fn loop(x) = for i in 1..3 { i }",
        "fn two(x) =
    y = x
    y",
        "fn stmt(x) =
    return x",
        "fn asg(x) =
    y = x",
        "fn self_rec(x) = self_rec(x) + 1",
        "fn ping(x) = pong(x)
fn pong(x) = ping(x)",
        "fn typed(x: m) = x",
        "@stable
fn deco(x) = x",
    ];

    /// Each expression and what the inliner makes of it (the inlined text, or the refusal), as it was before
    /// `Inliner::expr` and `call_arguments` were split into helpers (sonar S3776).
    const EXPECTED: &[(&str, &str)] = &[
        ("t + 1", "ok t + 1"),
        ("sq(t)", "ok (((t) ^ 2))"),
        ("sq(2*t) + sq(sq(t))", "ok (((2*t) ^ 2)) + ((((((t) ^ 2))) ^ 2))"),
        ("lin(t, y)", "ok ((((((t) * (y)) / 2) - (t)) + (y)))"),
        ("lin(y, t)", "ok ((((((y) * (t)) / 2) - (y)) + (t)))"),
        ("lin((t + 1), [1, 2])", "ok (((((((t + 1)) * ([1, 2])) / 2) - ((t + 1))) + ([1, 2])))"),
        ("free(t)", "ok (((t) + k))"),
        ("kp(t)", "ok ((((k) ^ (t)) + ((k) ^ 2)))"),
        ("big(t)", "error 'big' cannot be inlined into a symbolic expression: its body holds the literal inf"),
        ("asg(t)", "error 'asg' cannot be inlined into a symbolic expression: its body is a statement, not an expression"),
        ("lit(t)", "ok ((((t) * 2.5) + (((-1) * 3) * (4 m))))"),
        ("unc(t)", "error 'unc' cannot be inlined into a symbolic expression: its body holds the uncertain literal 2 (a symbolic expression carries no uncertainty)"),
        ("conv(t)", "error 'conv' cannot be inlined into a symbolic expression: its body holds a conversion `=>`"),
        ("rng(t)", "error 'rng' cannot be inlined into a symbolic expression: its body holds a range `..`"),
        ("pw(t)", "ok ((((t) + 1) ^ 3))"),
        ("nest(t)", "ok ((((((t)) ^ 2)) + sin((t))))"),
        ("cond(t)", "error 'cond' cannot be inlined into a symbolic expression: its body holds a conditional"),
        ("wh(t)", "error 'wh' cannot be inlined into a symbolic expression: its body holds a `where` binding"),
        ("vec(t)", "error 'vec' cannot be inlined into a symbolic expression: its body holds a vector"),
        ("fmt(t)", "error 'fmt' cannot be inlined into a symbolic expression: its body holds a formatted string"),
        ("cmp(t)", "error 'cmp' cannot be inlined into a symbolic expression: its body holds a comparison or a logical operator"),
        ("kw(t)", "error 'kw' cannot be inlined into a symbolic expression: its body holds a call of 'round' with keyword arguments"),
        ("pcall(t)", "error 'pcall' cannot be inlined into a symbolic expression: its body holds a call of its parameter 'f'"),
        ("text(t)", "error 'text' cannot be inlined into a symbolic expression: its body holds the string \"hello\""),
        ("yes(t)", "error 'yes' cannot be inlined into a symbolic expression: its body holds the boolean True"),
        ("no(t)", "error 'no' cannot be inlined into a symbolic expression: its body holds the boolean False"),
        ("loop(t)", "error 'loop' cannot be inlined into a symbolic expression: its body holds a `for` loop"),
        ("two(t)", "error 'two' cannot be inlined into a symbolic expression: its body is 2 statements, and only a body that is one expression can be"),
        ("stmt(t)", "ok ((t))"),
        ("self_rec(t)", "error 'self_rec' cannot be inlined into a symbolic expression: it is recursive (self_rec -> self_rec)"),
        ("ping(t)", "error 'ping' cannot be inlined into a symbolic expression: it is recursive (ping -> pong -> ping)"),
        ("typed(t)", "error 'typed' cannot be inlined into a symbolic expression: its parameter 'x' declares the unit m, which converts and checks the argument, and a symbolic expression cannot carry that"),
        ("deco(t)", "error 'deco' cannot be inlined into a symbolic expression: it carries @stable, which the inlined body would not apply"),
        ("sq()", "error 'sq' takes 1 argument (x), 0 given"),
        ("sq(t, )", "error the call of 'sq' in \"sq(t, )\" has an empty argument"),
        ("sq(, t)", "error the call of 'sq' in \"sq(, t)\" has an empty argument"),
        ("sq(x = t)", "error the call of 'sq' in \"sq(x = t)\" names an argument, and an inlined call takes its arguments by position"),
        ("sq(t", "error the call of 'sq' in \"sq(t\" is not closed"),
        ("sq(f(t, 2), 3)", "error 'sq' takes 1 argument (x), 2 given"),
        ("sq(t) * sqrt(t)", "ok (((t) ^ 2)) * sqrt(t)"),
        ("sqrt(sq(t))", "ok sqrt((((t) ^ 2)))"),
    ];

    fn inlined(interpreter: &PhsInterpreter, text: &str) -> String {
        match preprocess_symbolic_expression(text, interpreter, &HashMap::new()) {
            Ok(text) => format!("ok {text}"),
            Err(error) => format!("error {error}"),
        }
    }

    #[test]
    fn the_inlined_texts_and_refusals_are_pinned() {
        let mut interpreter = PhsInterpreter::default();
        for definition in DEFINITIONS {
            interpreter.eval_str(definition).unwrap_or_else(|e| panic!("{definition:?}: {e}"));
        }
        for (text, expected) in EXPECTED {
            assert_eq!(inlined(&interpreter, text), *expected, "{text:?}");
        }
    }
}
