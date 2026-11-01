use physure_core::error::{PhysureError, PhysureResult};
use crate::{PhsLexer, PhsToken, TokenKind};
use super::ast::Node;

pub struct SymbolicParser {
    tokens: Vec<PhsToken>,
    pos: usize,
}

impl SymbolicParser {
    pub fn parse_str(input: &str) -> PhysureResult<Node> {
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
        Ok(node.simplify())
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
            Ok(Some((left.simplify(), right.simplify())))
        } else {
            Ok(None)
        }
    }

    fn peek(&self) -> Option<&PhsToken> {
        self.tokens.get(self.pos)
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
        let mut left = self.parse_power()?;
        while let Some(step) = self.peek().map(product_step) {
            match step {
                ProductStep::Mul => {
                    self.next();
                    let right = self.parse_power()?;
                    left = Node::Mul(vec![left, right]);
                }
                ProductStep::Div => {
                    self.next();
                    let right = self.parse_power()?;
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

    fn parse_power(&mut self) -> PhysureResult<Node> {
        let mut left = self.parse_unary()?;
        while self.peek().is_some_and(|t| t.value == "^" || t.value == "**") {
            self.next();
            let mut right = self.parse_power()?;
            // a number exponent followed by a name takes the name along
            if matches!(right, Node::Number(_)) && self.peek().is_some_and(|next_t| matches!(next_t.kind, TokenKind::Ident(_))) {
                let factor = self.parse_power()?;
                right = Node::Mul(vec![right, factor]);
            }
            left = Node::Pow(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

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
        self.parse_atom()
    }

    fn parse_atom(&mut self) -> PhysureResult<Node> {
        let tok = self.next().ok_or_else(|| {
            PhysureError::Generic("Unexpected end of expression while parsing AST".to_string())
        })?;

        match tok.kind {
            TokenKind::Number(n) => Ok(Node::Number(n)),
            TokenKind::Ident(ref name) => self.parse_identifier(name, tok.pos),
            TokenKind::Op(ref op) if op == "(" => {
                let node = self.parse_equality()?;
                match self.next() {
                    Some(close_t) if close_t.value == ")" => Ok(node),
                    _ => Err(PhysureError::Generic("Expected closing ')'".to_string())),
                }
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

    /// A name: a function call when `(` follows, otherwise a symbol. A name ending in `e` before `^` is the
    /// symbol and the constant `e` (`2xe^x`), so the `e` goes back as a token of its own.
    fn parse_identifier(&mut self, name: &str, pos: usize) -> PhysureResult<Node> {
        if self.peek().is_some_and(|next_t| next_t.value == "(") {
            return self.parse_func_call(name);
        }
        let before_power = self.peek().is_some_and(|next_t| next_t.value == "^" || next_t.value == "**");
        if name.len() > 1 && name.ends_with('e') && before_power {
            let prefix = name[..name.len() - 1].to_string();
            self.tokens.insert(self.pos, PhsToken {
                kind: TokenKind::Ident("e".to_string()),
                value: "e".to_string(),
                pos: pos + prefix.len(),
            });
            return Ok(Node::Symbol(prefix));
        }
        Ok(Node::Symbol(name.to_string()))
    }

    fn parse_func_call(&mut self, name: &str) -> PhysureResult<Node> {
        self.next(); // consume '('
        let mut args = self.parse_call_arguments()?;
        if let Some(close_t) = self.next() {
            if close_t.value != ")" {
                return Err(PhysureError::Generic("Expected closing ')' after function arguments".to_string()));
            }
        }

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
