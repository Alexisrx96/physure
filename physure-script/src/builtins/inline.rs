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
//! are not algebra (comparisons, conditionals, strings, vectors, conversions, ranges, loops).

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
