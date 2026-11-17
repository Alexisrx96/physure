use physure_core::error::{PhysureError, PhysureResult};
use crate::{PhsLexer, PhsToken, TokenKind};
use super::ast::Node;

pub struct SymbolicParser {
    tokens: Vec<PhsToken>,
    pos: usize,
}

impl SymbolicParser {
    pub fn parse_str(input: &str) -> PhysureResult<Node> {
        Ok(Self::parse_tree(input)?.simplify())
    }

    /// The tree `input` is read as, before [`Node::simplify`] folds it: the shape the precedence rules give (what
    /// `tests/precedence.rs` compares with a reference parser).
    pub fn parse_tree(input: &str) -> PhysureResult<Node> {
        let trimmed = input.trim();
        let clean_input = if (trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'') && trimmed.len() > 1 && !trimmed[1..trimmed.len() - 1].contains('\''))
        {
            &trimmed[1..trimmed.len() - 1]
        } else {
            trimmed
        };
        let lexer = PhsLexer::new(clean_input);
        let tokens = lexer.tokenize()?;
        let mut parser = SymbolicParser { tokens, pos: 0 };
        let node = parser.parse_equality()?;
        parser.finish()?;
        Ok(node)
    }

    /// Parses `input` as `lhs = rhs` without collapsing to `Sub`, for coercing a
    /// plain string into a `PhsValue::Equation`. Returns `None` if there's no
    /// top-level `=`/`==` (i.e. it's a plain expression/symbol, not an equation).
    pub fn parse_equation_str(input: &str) -> PhysureResult<Option<(Node, Node)>> {
        let trimmed = input.trim();
        let clean_input = if (trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'') && trimmed.len() > 1 && !trimmed[1..trimmed.len() - 1].contains('\''))
        {
            &trimmed[1..trimmed.len() - 1]
        } else {
            trimmed
        };
        let lexer = PhsLexer::new(clean_input);
        let tokens = lexer.tokenize()?;
        let mut parser = SymbolicParser { tokens, pos: 0 };
        let left = parser.parse_sum()?;
        if parser.match_op("=") || parser.match_op("==") {
            let right = parser.parse_sum()?;
            parser.finish()?;
            Ok(Some((left.simplify(), right.simplify())))
        } else {
            Ok(None)
        }
    }

    fn peek(&self) -> Option<&PhsToken> {
        self.tokens.get(self.pos)
    }

    /// The whole text must have been read. The parser stops at the first token no rule takes, and what followed
    /// was dropped without a word: `integral("x < 0.5", "x")` was the integral of `x` (audit T2), and
    /// `deriv("x^2 )", "x")` was `2 * x`.
    fn finish(&self) -> PhysureResult<()> {
        self.refuse_comparison()?;
        match self.peek() {
            None => Ok(()),
            Some(t) => Err(PhysureError::Generic(format!("unexpected `{}` after the end of the expression", t.value))),
        }
    }

    /// A comparison is named where the parser meets one, inside parentheses too (`(y >= 2) + 1`), not reported as
    /// a missing `)`.
    fn refuse_comparison(&self) -> PhysureResult<()> {
        match self.peek() {
            Some(t) if COMPARISONS.contains(&t.value.as_str()) => Err(PhysureError::Generic(format!(
                "`{}` is a comparison, which a symbolic expression cannot hold: write an expression, or an equation \
                 with `=`",
                t.value
            ))),
            _ => Ok(()),
        }
    }

    /// Consumes the `)` that closes a group or a call, or fails with `message`.
    fn expect_close(&mut self, message: &str) -> PhysureResult<()> {
        self.refuse_comparison()?;
        // the end of the text is no closing parenthesis either: `sin(x` was `sin(x)`
        if self.next().is_some_and(|t| t.value == ")") {
            Ok(())
        } else {
            Err(PhysureError::Generic(message.to_string()))
        }
    }

    fn next(&mut self) -> Option<PhsToken> {
        let tok = self.tokens.get(self.pos).cloned();
        if tok.is_some() {
            self.pos += 1;
        }
        tok
    }

    fn match_op(&mut self, op_str: &str) -> bool {
        if let Some(t) = self.peek() {
            // The kind check this used to carry was `(is_op || matches) && matches`,
            // which is just `matches` — the disjunction made it unreachable, so the
            // token's kind was never actually consulted. Only `=` and `==` are
            // passed here and both only ever lex as `Op`, so comparing the text is
            // the whole test.
            if t.value == op_str {
                self.pos += 1;
                return true;
            }
        }
        false
    }

    fn parse_equality(&mut self) -> PhysureResult<Node> {
        let mut left = self.parse_sum()?;
        while self.match_op("=") || self.match_op("==") {
            let right = self.parse_sum()?;
            left = Node::Equation(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_sum(&mut self) -> PhysureResult<Node> {
        let mut left = self.parse_product()?;
        while let Some(t) = self.peek() {
            if t.value == "+" {
                self.next();
                let right = self.parse_product()?;
                left = Node::Add(vec![left, right]);
            } else if t.value == "-" {
                self.next();
                let right = self.parse_product()?;
                left = Node::Sub(Box::new(left), Box::new(right));
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_product(&mut self) -> PhysureResult<Node> {
        let mut left = self.parse_unary()?;
        while let Some(step) = self.peek().map(product_step) {
            match step {
                ProductStep::Mul => {
                    self.next();
                    let right = self.parse_unary()?;
                    left = Node::Mul(vec![left, right]);
                }
                ProductStep::Div => {
                    self.next();
                    let right = self.parse_unary()?;
                    left = Node::Div(Box::new(left), Box::new(right));
                }
                ProductStep::Implicit => {
                    // Implicit multiplication, which is how a quantity reaches the symbolic
                    // layer: `deriv("0.5 * 2.0 kg * v^2", "v")` stranded `kg` after the number
                    // and the whole derivative silently collapsed to 0. The unit rides along as
                    // a symbolic constant, exactly as a bare `kg` already did.
                    let right = self.parse_power()?;
                    left = Node::Mul(vec![left, right]);
                }
                ProductStep::Stop => break,
            }
        }
        Ok(left)
    }

    /// A sign binds looser than `^`, as in PHS and in mathematics: `-x^2` is `-(x^2)`. It bound tighter, so
    /// `deriv("-x^2", "x")` answered `2 * x` and `integral("-x^2", "x", 0, 1)` disagreed with its own quadrature.
    fn parse_unary(&mut self) -> PhysureResult<Node> {
        if let Some(t) = self.peek() {
            if t.value == "-" {
                self.next();
                let operand = self.parse_unary()?;
                return Ok(Node::Mul(vec![Node::Number(-1.0), operand]));
            }
            if t.value == "+" {
                self.next();
                return self.parse_unary();
            }
        }
        self.parse_power()
    }

    /// `base ^ exponent` (or `**`), where the base is an atom and the exponent an atom with an optional sign
    /// (`e^-x`). A power of a power is refused as PHS refuses it: `x^2^3` reads as `x^(2^3)` by the mathematical
    /// convention and as `(x^2)^3` left to right, and it used to mean the first here and be an error in PHS.
    fn parse_power(&mut self) -> PhysureResult<Node> {
        let base = self.parse_atom()?;
        if !self.at_power() {
            return Ok(base);
        }
        // `e` is a name like any other here and Euler's number nowhere, as in the numeric path (audit T13)
        if matches!(&base, Node::Symbol(name) if name == "e") {
            return Err(PhysureError::Generic(
                "`e` is not Euler's number in a symbolic text: write `exp(x)` for e^x, or another name for a variable".to_string(),
            ));
        }
        self.next();
        let mut exponent = self.parse_exponent()?;
        // a number exponent followed by a name takes the name along (`e^2x` is `e^(2 x)`)
        if matches!(exponent, Node::Number(_)) && self.peek().is_some_and(|next_t| matches!(next_t.kind, TokenKind::Ident(_))) {
            let factor = self.parse_power()?;
            exponent = Node::Mul(vec![exponent, factor]);
        }
        if self.at_power() {
            return Err(PhysureError::Generic(
                "a power of a power needs parentheses: write `(a^b)^c` or `a^(b^c)`".to_string(),
            ));
        }
        Ok(Node::Pow(Box::new(base), Box::new(exponent)))
    }

    fn at_power(&self) -> bool {
        self.peek().is_some_and(|t| t.value == "^" || t.value == "**")
    }

    /// An exponent: an atom, with a sign of its own if it has one.
    fn parse_exponent(&mut self) -> PhysureResult<Node> {
        if let Some(t) = self.peek() {
            if t.value == "-" {
                self.next();
                let operand = self.parse_exponent()?;
                return Ok(Node::Mul(vec![Node::Number(-1.0), operand]));
            }
            if t.value == "+" {
                self.next();
                return self.parse_exponent();
            }
        }
        self.parse_atom()
    }

    fn parse_atom(&mut self) -> PhysureResult<Node> {
        let tok = self.next().ok_or_else(|| {
            PhysureError::Generic("Unexpected end of expression while parsing AST".to_string())
        })?;

        match tok.kind {
            TokenKind::Number(n) => Ok(Node::Number(n)),
            TokenKind::Ident(ref name) => self.parse_identifier(name),
            TokenKind::Op(ref op) if op == "(" => {
                let node = self.parse_equality()?;
                self.expect_close("Expected closing ')'")?;
                Ok(node)
            }
            TokenKind::StringLiteral(ref s) => {
                Self::parse_str(s)
            }
            TokenKind::Sqrt => {
                if self.peek().is_some_and(|next_t| next_t.value == "(") {
                    return self.parse_func_call("sqrt");
                }
                Err(PhysureError::Generic("Expected '(' after sqrt".to_string()))
            }
            _ => Err(PhysureError::Generic(format!(
                "Unexpected token '{}' in expression",
                tok.value
            ))),
        }
    }

    /// A name: a function call when `(` follows, otherwise a symbol, whole. It used to cut a name that ends in `e`
    /// before `^` into the name and the constant `e` (to read `2xe^x`), so `time^2` was `tim * e^2` and `deriv("time^2",
    /// "time")` answered 0 (audit T13).
    fn parse_identifier(&mut self, name: &str) -> PhysureResult<Node> {
        if self.peek().is_some_and(|next_t| next_t.value == "(") {
            return self.parse_func_call(name);
        }
        refuse_bare_word(name)?;
        Ok(Node::Symbol(name.to_string()))
    }

    fn parse_func_call(&mut self, name: &str) -> PhysureResult<Node> {
        self.next(); // consume '('
        let mut args = self.parse_call_arguments()?;
        self.expect_close("Expected closing ')' after function arguments")?;

        match name {
            "deriv" | "diff" => {
                if args.len() != 2 {
                    return Err(PhysureError::Generic("deriv requires 2 arguments: expr, var".to_string()));
                }
                let var_str = args[1].to_phs_string();
                args[0].diff_node(&var_str)
            }
            "integral" | "integrate" => {
                if args.len() != 2 {
                    return Err(PhysureError::Generic("integral requires 2 arguments: expr, var".to_string()));
                }
                let var_str = args[1].to_phs_string();
                args[0].integrate_node(&var_str)
            }
            _ => match unary_function(name) {
                Some((label, build)) => {
                    if args.len() != 1 {
                        return Err(PhysureError::Generic(format!("{} requires 1 argument", label)));
                    }
                    Ok(build(Box::new(args.remove(0))))
                }
                None => Err(PhysureError::Generic(format!("Unknown symbolic function '{}'", name))),
            },
        }
    }

    /// The comma-separated arguments up to (not including) the closing `)`.
    fn parse_call_arguments(&mut self) -> PhysureResult<Vec<Node>> {
        let mut args = Vec::new();
        if self.peek().is_some_and(|t| t.value != ")") {
            loop {
                args.push(self.parse_equality()?);
                if self.peek().is_some_and(|next_t| next_t.value == ",") {
                    self.next();
                } else {
                    break;
                }
            }
        }
        Ok(args)
    }
}

/// The operators of PHS that compare; `=` and `==` are the equation, not in this list.
const COMPARISONS: [&str; 6] = ["<", ">", "<=", ">=", "!=", "≈"];

/// What follows a factor in a product.
#[derive(Clone, Copy)]
enum ProductStep {
    Mul,
    Div,
    /// A name, `(` or a root next to a factor: multiplication without an operator.
    Implicit,
    Stop,
}

fn product_step(t: &PhsToken) -> ProductStep {
    if t.value == "*" {
        ProductStep::Mul
    } else if t.value == "/" {
        ProductStep::Div
    } else if matches!(t.kind, TokenKind::Ident(_)) || t.value == "(" || matches!(t.kind, TokenKind::Sqrt) {
        ProductStep::Implicit
    } else {
        ProductStep::Stop
    }
}

/// A word PHS reads as something other than a name is not a symbol of a symbolic text: `x and y` was `and * x * y`,
/// and a function without its parentheses (`sin * x`) a symbol called `sin`. The words that are only plausible names
/// (`sigma`, `max`, `dof`, a statement word) stay symbols, as the variables of an equation call do.
fn refuse_bare_word(name: &str) -> PhysureResult<()> {
    use crate::reserved::{static_kind, Reserved};
    let message = match static_kind(name) {
        Some(Reserved::Keyword) => format!("`{name}` is a word of the language and is not a symbol of a symbolic text; use another name"),
        Some(Reserved::Literal) => format!("`{name}` is a literal of the language and is not a symbol of a symbolic text; use another name"),
        _ if unary_function(name).is_some() || matches!(name, "deriv" | "integral") => {
            format!("`{name}` is a function: write it with its argument in parentheses, `{name}(x)`")
        }
        _ => return Ok(()),
    };
    Err(PhysureError::Generic(message))
}

/// The node built by a one-argument symbolic function, by the name it is written with, and the name its
/// arity error uses.
fn unary_function(name: &str) -> Option<(&'static str, fn(Box<Node>) -> Node)> {
    Some(match name {
        "sqrt" => ("sqrt", Node::Sqrt),
        "sin" => ("sin", Node::Sin),
        "cos" => ("cos", Node::Cos),
        "ln" | "log" => ("ln/log", Node::Ln),
        "exp" => ("exp", Node::Exp),
        "tan" => ("tan", Node::Tan),
        "cot" => ("cot", Node::Cot),
        "sec" => ("sec", Node::Sec),
        "csc" | "cosec" => ("csc", Node::Csc),
        "arcsin" | "asin" => ("arcsin", Node::Arcsin),
        "arccos" | "acos" => ("arccos", Node::Arccos),
        "arctan" | "atan" => ("arctan", Node::Arctan),
        "arccot" | "acot" => ("arccot", Node::Arccot),
        "arcsec" | "asec" => ("arcsec", Node::Arcsec),
        "arccsc" | "acsc" => ("arccsc", Node::Arccsc),
        "sinh" => ("sinh", Node::Sinh),
        "cosh" => ("cosh", Node::Cosh),
        "tanh" => ("tanh", Node::Tanh),
        "coth" => ("coth", Node::Coth),
        "sech" => ("sech", Node::Sech),
        "csch" => ("csch", Node::Csch),
        "abs" => ("abs", Node::Abs),
        _ => return None,
    })
}
