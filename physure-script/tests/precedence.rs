//! The order in which PHS binds its operators (audit B26), through the entry the CLI's `phs eval` uses
//! (`report::evaluate`). Tightest first:
//!
//! `^` and unary minus → `*` `/` and implicit multiplication → `+` `-` → `=>` (conversion; left-associative, its
//! target is a unit) → comparison (one per expression) → `..` (one per expression) → format spec → `not` → `and` →
//! `or` → ternary → `where`.
//!
//! Arithmetic binds tighter than comparison as it does in mathematics and in every language a scientist reads:
//! `0.1 + 0.2 == 0.3` is `(0.1 + 0.2) == 0.3`. Before, the comparison bound tighter than `+`, so that expression
//! was `0.1 + (0.2 == 0.3)` and failed with "Invalid operand types". Two readings that used to parse are refused
//! with a message instead of being re-read silently: a `+`/`-` after `=> unit` (the target of `=>` is a unit, not
//! an expression), and a chain of comparisons.
//!
//! The last test checks the parse tree of random expressions against a reference parser written here, by
//! precedence climbing over a table, independently of the grammar.

use physure_script::ast::{BinaryOp, Expr, Statement};
use physure_script::interpreter::eval_phs;
use physure_script::parse_phs;
use physure_script::report::{evaluate, Report, DEFAULT_SEED};
use physure_script::symbolic::ast::Node;
use physure_script::symbolic::parser::SymbolicParser;
use physure_script::Limits;
use std::time::Duration;

fn run(source: &str) -> Report {
    evaluate(source, Limits::sandboxed(Duration::from_secs(60)), DEFAULT_SEED)
}

/// The text of the value of the last line of `source`, as `phs eval` reports it.
fn value(source: &str) -> String {
    let last = source.lines().count();
    let report = run(source);
    match report.results.last() {
        Some(item) if item.line == last => item.value.text.clone(),
        _ => panic!("{source:?} gave no value: {:?}", report.diagnostics),
    }
}

/// The boolean `phs eval` reports for the last line of `source`.
fn truth(source: &str) -> bool {
    let last = source.lines().count();
    let report = run(source);
    match report.results.last() {
        Some(item) if item.line == last && item.value.kind == "bool" => item.value.text == "True",
        _ => panic!("{source:?} gave no boolean: {:?} {:?}", report.results.last(), report.diagnostics),
    }
}

/// The parse error for `source`: `phs eval` answers nothing and reports it as a syntax diagnostic.
fn parse_error(source: &str) -> String {
    let report = run(source);
    assert!(report.results.is_empty(), "{source:?} was answered: {:?}", report.results);
    assert!(
        report.diagnostics.iter().any(|d| d.code == "syntax"),
        "{source:?} gave no syntax error: {:?}",
        report.diagnostics
    );
    match parse_phs(source) {
        Err(e) => e.to_string(),
        Ok(program) => panic!("{source:?} parsed: {program:?}"),
    }
}

#[test]
fn arithmetic_binds_tighter_than_comparison() {
    for source in [
        "0.1 + 0.2 == 0.3",
        "0.1 m + 0.2 m == 0.3 m",
        "a = 1\nb = 2\nc = 2.5\na + b > c",
        "a = 5\nb = 2\nc = 4\nd = 1\na - b == c - d",
        "a = 2\nb = 3\nc = 4\nd = 10\na * b + c == d",
        "2 m * 3 + 1 m == 7 m",
        "1 + 1 == 2 and 2 < 3",
        "10 - 2 * 3 >= 4",
        "2 ^ 3 + 1 != 8",
    ] {
        assert!(truth(source), "{source:?}");
    }
    assert!(!truth("a = 1\nb = 2\nc = 5\na + b > c"));
}

#[test]
fn conversion_sits_between_the_sum_and_the_comparison() {
    assert!(truth("2 m + 3 m => km == 0.005 km"));
    assert!(truth("x = 1 m\nx => km < 5 km"));
    assert!(truth("1 m => km == 0.001 km"));
    assert!(truth("1 m => cm => km == 1 m"));
    // the conversion applies to the whole sum, as it did before
    assert_eq!(value("1 m + 2 m => cm"), "300.0 cm");
    assert_eq!(value("2 m * 3 => cm"), "600.0 cm");
    // and a converted operand in parentheses is still an operand
    assert_eq!(value("(1 m => km) - 5 m"), "-0.004 km");
}

#[test]
fn not_applies_to_the_whole_comparison() {
    assert!(truth("a = 1\nb = 2\nc = 5\nnot a + b > c"));
    assert!(!truth("not 1 < 2"));
    assert!(truth("True or False and False"));
    assert!(!truth("(True or False) and False"));
}

#[test]
fn what_did_not_change() {
    assert_eq!(value("-2 ^ 2"), "-4.0");
    assert_eq!(value("2 * 3 m"), "6.0 m");
    assert_eq!(value("6 m / 2 * 3"), "9.0 m");
    assert_eq!(value("2 3 m"), "6.0 m");
    assert_eq!(value("0 m .. 10 m => km"), "0.0 m .. 0.01 km");
    assert_eq!(value("(0 m .. 10 m) => km"), "0.0 km .. 0.01 km");
    assert_eq!(value("1 m + 1 m .. 5 m"), "2.0 m .. 5.0 m");
    assert_eq!(value("0.1 + 0.2: .3f"), "0.300");
    assert_eq!(value("1 < 2 ? 3 : 4"), "3.0");
    assert_eq!(value("1 + 1 == 2 ? 3 : 4"), "3.0");
    assert_eq!(value("x * 2 where x = 1 + 1"), "4.0");
    assert_eq!(value("if 1 + 1 == 2 then 1 else 0"), "1.0");
    // a power chain is still refused: write the parentheses
    assert!(parse_error("2 ^ 3 ^ 2").contains("Parse error"));
}

/// The target of `=>` is a unit, not an expression, so `x => km - 5 m` cannot mean `x => (km - 5 m)`; and under the
/// standard order it cannot mean `(x => km) - 5 m` either, which is what it used to give. It is refused, with the
/// parentheses to write, and never split into two statements.
#[test]
fn a_sum_after_a_conversion_needs_parentheses() {
    for (source, unit, op) in [
        ("x = 1 m\nx => km - 5 m", "km", "-"),
        ("x = 1 m\nx => km -5 m", "km", "-"),
        ("x = 1 m\nx => km + 5 m", "km", "+"),
        ("y = 1 m => km - 5 m", "km", "-"),
        ("f(x) = x => km - 1 m; f(1 m)", "km", "-"),
        ("[1,2] m => cm - 1 m", "cm", "-"),
        ("x = 1 m\n(x => km - 5 m)", "km", "-"),
        ("x = 1 m\nabs(x => km - 5 m)", "km", "-"),
        ("x = 1 m\nx => m => km + 1 m", "km", "+"),
        ("x = 1 m\nx => m/s - 1 m/s", "m/s", "-"),
    ] {
        let message = parse_error(source);
        assert!(
            message.contains(&format!("after `=> {unit}`, a `{op}` needs parentheses: write `(")),
            "{source:?}: {message}"
        );
        assert!(eval_phs(source).is_err(), "{source:?}");
    }
    let message = parse_error("x = 1 m\nx => km - 5 m");
    assert!(message.contains("write `(x => km) - 5 m`"), "{message}");
    assert!(message.contains("--> 2:"), "{message}");
}

#[test]
fn a_chain_of_comparisons_is_a_parse_error() {
    for source in [
        "1 < 2 < 3",
        "True == False == False",
        "a = 1\nb = 2\nc = 3\na < b <= c",
        "1 m < 2 m == True",
        "1 == 1 == 1 == 1",
    ] {
        let message = parse_error(source);
        assert!(
            message.contains("a comparison chain has no meaning here: write `a < b and b < c`"),
            "{source:?}: {message}"
        );
    }
    // a comparison in parentheses is an operand like any other
    assert!(truth("(1 < 2) == True"));
    assert!(truth("1 < 2 and 2 < 3"));
}

// ---------------------------------------------------------------------------------------------------------------
// A reference parser, written from the precedence table and nothing else.

/// The binary operators and their binding power, loosest first. Unary minus binds at 9 (its operand is a power),
/// `not` at 3 (its operand is a range).
fn binding(op: &str) -> Option<u8> {
    Some(match op {
        "or" => 1,
        "and" => 2,
        ".." => 4,
        "==" | "!=" | "<" | ">" | "<=" | ">=" => 5,
        "=>" => 6,
        "+" | "-" => 7,
        "*" | "/" => 8,
        "^" => 10,
        _ => return None,
    })
}

struct Reference<'a> {
    tokens: &'a [String],
    pos: usize,
    /// Whether an exponent may carry a sign (`2^-x`): the symbolic parser's strings allow it, PHS does not.
    signed_exponent: bool,
}

impl Reference<'_> {
    fn peek(&self) -> Option<&str> {
        self.tokens.get(self.pos).map(String::as_str)
    }

    fn take(&mut self) -> Option<String> {
        let token = self.tokens.get(self.pos).cloned();
        self.pos += 1;
        token
    }

    fn primary(&mut self) -> Option<String> {
        let token = self.take()?;
        if token == "(" {
            let inner = self.climb(0)?;
            (self.take()? == ")").then_some(inner)
        } else if token.chars().all(|c| c.is_ascii_alphanumeric()) && binding(&token).is_none() && token != "not" {
            Some(token)
        } else {
            None
        }
    }

    /// Precedence climbing: an operand, then every operator that binds at least as tightly as `min`.
    fn climb(&mut self, min: u8) -> Option<String> {
        let mut left = match self.peek()? {
            "not" => {
                if min > 3 {
                    return None;
                }
                self.take();
                format!("(not {})", self.climb(3)?)
            }
            "-" => {
                if min > 9 {
                    return None;
                }
                self.take();
                if self.peek()? == "-" {
                    return None;
                }
                format!("(* -1 {})", self.climb(10)?)
            }
            _ => self.primary()?,
        };
        while let Some(op) = self.peek().map(str::to_string) {
            let Some(power) = binding(&op) else { break };
            if power < min {
                break;
            }
            self.take();
            left = match op.as_str() {
                // the exponent is a primary, and a power is not an operand of another power
                "^" => {
                    let exponent = if self.signed_exponent && self.peek() == Some("-") {
                        self.take();
                        format!("(* -1 {})", self.primary()?)
                    } else {
                        self.primary()?
                    };
                    if self.peek() == Some("^") {
                        return None;
                    }
                    format!("(^ {left} {exponent})")
                }
                "=>" => {
                    let unit = self.take()?;
                    // a unit is never followed by `+` or `-`
                    if matches!(self.peek(), Some("+" | "-")) {
                        return None;
                    }
                    format!("(=> {left} {unit})")
                }
                // non-associative: one comparison, one range
                "==" | "!=" | "<" | ">" | "<=" | ">=" | ".." => {
                    let right = self.climb(power + 1)?;
                    if self.peek().and_then(binding) == Some(power) {
                        return None;
                    }
                    format!("({op} {left} {right})")
                }
                _ => format!("({op} {left} {})", self.climb(power + 1)?),
            };
        }
        Some(left)
    }
}

/// The reference reading of `tokens`, or `None` when the table says the text is not an expression.
fn reference(tokens: &[String], signed_exponent: bool) -> Option<String> {
    let mut parser = Reference { tokens, pos: 0, signed_exponent };
    let tree = parser.climb(0)?;
    (parser.pos == tokens.len()).then_some(tree)
}

/// The same notation for the tree the PHS parser built.
fn tree(expr: &Expr) -> String {
    match expr {
        Expr::Quantity(q) if q.unit.is_none() => format!("{}", q.magnitude),
        Expr::Identifier(name) => name.clone(),
        Expr::BinaryOp { op, left, right } => {
            let op = match op {
                BinaryOp::Add => "+",
                BinaryOp::Sub => "-",
                BinaryOp::Mul => "*",
                BinaryOp::Div => "/",
                BinaryOp::Pow => "^",
                BinaryOp::Convert => "=>",
                BinaryOp::Range => "..",
            };
            format!("({op} {} {})", tree(left), tree(right))
        }
        Expr::FunctionCall { name, args, .. } if name.starts_with("op_") => {
            let args: Vec<String> = args.iter().map(tree).collect();
            format!("({} {})", &name[3..], args.join(" "))
        }
        other => panic!("not an expression of the generator: {other:?}"),
    }
}

/// A small deterministic generator (xorshift), so a failure is reproducible from the case number alone.
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[(self.next() % items.len() as u64) as usize]
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

/// A random stream of operands and operators, mostly well formed and sometimes not (a chain, a power of a power,
/// a `+` after a unit): the reference decides which. `arithmetic` keeps to `^`, unary minus, `* /` and `+ -`.
fn operands_and_operators(random: &mut Random, depth: usize, arithmetic: bool, out: &mut Vec<String>) {
    operand(random, depth, arithmetic, out);
    let mut after_unit = false;
    for _ in 0..random.next() % 4 {
        let op = if arithmetic {
            random.pick(&["+", "-", "*", "/", "^"])
        } else if after_unit {
            // what may follow a unit without making it a longer unit (`km * a`, `km ^ 2`)
            random.pick(&["=>", "==", "<", "..", "and", "or", "+", "-"])
        } else {
            random.pick(&["+", "-", "*", "/", "^", "=>", "==", "!=", "<", ">", "<=", ">=", "..", "and", "or"])
        };
        out.push(op.to_string());
        after_unit = op == "=>";
        if after_unit {
            out.push(random.pick(&["km", "m", "s"]).to_string());
        } else {
            operand(random, depth, arithmetic, out);
        }
    }
}

fn operand(random: &mut Random, depth: usize, arithmetic: bool, out: &mut Vec<String>) {
    if !arithmetic && random.chance(10) {
        out.push("not".to_string());
    }
    if random.chance(15) {
        out.push("-".to_string());
    }
    if depth < 3 && random.chance(20) {
        out.push("(".to_string());
        operands_and_operators(random, depth + 1, arithmetic, out);
        out.push(")".to_string());
    } else {
        out.push(random.pick(&["a", "b", "c", "1", "2", "3"]).to_string());
    }
}

/// Random expressions over `^`, unary minus, `* /`, `+ -`, `=>`, the comparisons, `..`, `not`, `and`, `or` and
/// parentheses: the PHS parser builds the tree the reference builds, and refuses what the reference refuses.
#[test]
fn the_parser_agrees_with_a_reference_precedence_climbing_parser() {
    let mut random = Random(0x9e37_79b9_7f4a_7c15);
    let (mut accepted, mut refused) = (0, 0);
    for case in 0..4000 {
        let mut tokens = Vec::new();
        operands_and_operators(&mut random, 0, false, &mut tokens);
        let source = tokens.join(" ");
        let expected = reference(&tokens, false);
        let parsed = parse_phs(&source);
        match (&expected, &parsed) {
            (Some(expected), Ok(program)) => {
                let [Statement::Expr(expr)] = program.statements.as_slice() else {
                    panic!("case {case}: {source:?} is not one expression: {program:?}")
                };
                assert_eq!(&tree(expr), expected, "case {case}: {source:?}");
                accepted += 1;
            }
            (None, Err(_)) => refused += 1,
            (Some(expected), Err(e)) => panic!("case {case}: {source:?} refused ({e}), the reference reads {expected}"),
            (None, Ok(program)) => panic!("case {case}: {source:?} parsed as {program:?}, the reference refuses it"),
        }
    }
    // the generator covers both sides
    assert!(accepted > 1000 && refused > 300, "accepted {accepted}, refused {refused}");
}

/// The tree the symbolic parser built, before it is simplified, in the reference's notation.
fn symbolic_tree(node: &Node) -> String {
    match node {
        Node::Number(n) => format!("{n}"),
        Node::Symbol(name) => name.clone(),
        Node::Add(terms) if terms.len() == 2 => format!("(+ {} {})", symbolic_tree(&terms[0]), symbolic_tree(&terms[1])),
        Node::Mul(factors) if factors.len() == 2 => {
            format!("(* {} {})", symbolic_tree(&factors[0]), symbolic_tree(&factors[1]))
        }
        Node::Sub(l, r) => format!("(- {} {})", symbolic_tree(l), symbolic_tree(r)),
        Node::Div(l, r) => format!("(/ {} {})", symbolic_tree(l), symbolic_tree(r)),
        Node::Pow(l, r) => format!("(^ {} {})", symbolic_tree(l), symbolic_tree(r)),
        other => panic!("not an expression of the generator: {other:?}"),
    }
}

/// The strings `deriv`, `integral`, `limit` and the other symbolic builtins read go through a second parser
/// (`symbolic::parser`). It must bind the same way: `-x^2` is `-(x^2)` there too (it was `(-x)^2`, so
/// `deriv("-x^2", "x")` answered `2 * x`), and a power of a power is refused there too. The one difference it keeps
/// is a sign on an exponent (`e^-x`), which the reference is told about.
#[test]
fn the_symbolic_parser_agrees_with_the_same_reference() {
    let mut random = Random(0x2545_f491_4f6c_dd1d);
    let (mut accepted, mut refused) = (0, 0);
    for case in 0..4000 {
        let mut tokens = Vec::new();
        operands_and_operators(&mut random, 0, true, &mut tokens);
        let source = tokens.join(" ");
        match (reference(&tokens, true), SymbolicParser::parse_tree(&source)) {
            (Some(expected), Ok(node)) => {
                assert_eq!(symbolic_tree(&node), expected, "case {case}: {source:?}");
                accepted += 1;
            }
            (None, Err(_)) => refused += 1,
            (Some(expected), Err(e)) => panic!("case {case}: {source:?} refused ({e}), the reference reads {expected}"),
            (None, Ok(node)) => panic!("case {case}: {source:?} parsed as {node:?}, the reference refuses it"),
        }
    }
    assert!(accepted > 1000 && refused > 100, "accepted {accepted}, refused {refused}");
}

/// The value of `source` after `use * from calc`, which brings the symbolic builtins in.
fn calc(source: &str) -> String {
    value(&format!("use * from calc\n{source}"))
}

#[test]
fn a_minus_sign_before_a_power_negates_the_power_in_symbolic_strings_too() {
    let derivative = calc("deriv(\"-x^2\", \"x\")");
    assert!(["-2 * x", "-(2 * x)", "-2x"].contains(&derivative.as_str()), "{derivative}");
    // the integral of -(x^2) on [0, 1] is -1/3, and the antiderivative now agrees with the quadrature
    let integral: f64 = calc("integral(\"-x^2\", \"x\", 0, 1)").parse().unwrap();
    assert!((integral + 1.0 / 3.0).abs() < 1e-12, "{integral}");
    // d/dx (-(2^x)) = -ln(2) 2^x, at x = 0: -ln 2 (as (-2)^x it was ln(-2) (-2)^x)
    let derivative = calc("deriv(\"-2^x\", \"x\")");
    assert!(!derivative.contains("ln(-2)") && derivative.contains("ln(2)"), "{derivative}");
    // a sign on the exponent still belongs to the exponent
    assert_eq!(calc("deriv(\"2^-x\", \"x\")"), calc("deriv(\"2^(-x)\", \"x\")"));
    // and a product still takes a signed factor: 3 * -(x^2)
    assert_eq!(calc("deriv(\"3 * -x^2\", \"x\")"), calc("deriv(\"-3 * x^2\", \"x\")"));
}

#[test]
fn a_power_of_a_power_is_refused_in_symbolic_strings_as_in_phs() {
    for text in ["x^2^3", "x**2**3", "2^x^2", "x^-2^3"] {
        let error = SymbolicParser::parse_str(text).map(|n| format!("{n:?}")).unwrap_err().to_string();
        assert!(error.contains("a power of a power needs parentheses"), "{text}: {error}");
    }
    assert!(SymbolicParser::parse_str("(x^2)^3").is_ok());
    assert!(SymbolicParser::parse_str("x^(2^3)").is_ok());
    assert!(SymbolicParser::parse_str("e^2x").is_ok());
}
