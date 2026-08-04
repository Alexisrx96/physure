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
        while let Some(t) = self.peek() {
            if t.value == "*" {
                self.next();
                let right = self.parse_power()?;
                left = Node::Mul(vec![left, right]);
            } else if t.value == "/" {
                self.next();
                let right = self.parse_power()?;
                left = Node::Div(Box::new(left), Box::new(right));
            } else if matches!(t.kind, TokenKind::Ident(_)) || t.value == "(" || matches!(t.kind, TokenKind::Sqrt) {
                // Implicit multiplication, which is how a quantity reaches the symbolic
                // layer: `deriv("0.5 * 2.0 kg * v^2", "v")` stranded `kg` after the number
                // and the whole derivative silently collapsed to 0. The unit rides along as
                // a symbolic constant, exactly as a bare `kg` already did.
                let right = self.parse_power()?;
                left = Node::Mul(vec![left, right]);
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_power(&mut self) -> PhysureResult<Node> {
        let mut left = self.parse_unary()?;
        while let Some(t) = self.peek() {
            if t.value == "^" || t.value == "**" {
                self.next();
                let mut right = self.parse_power()?;
                if matches!(right, Node::Number(_)) {
                    if let Some(next_t) = self.peek() {
                        if matches!(next_t.kind, TokenKind::Ident(_)) {
                            let factor = self.parse_power()?;
                            right = Node::Mul(vec![right, factor]);
                        }
                    }
                }
                left = Node::Pow(Box::new(left), Box::new(right));
            } else {
                break;
            }
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
            TokenKind::Ident(ref name) => {
                if let Some(next_t) = self.peek() {
                    if next_t.value == "(" {
                        return self.parse_func_call(name);
                    }
                }
                if name.len() > 1 && name.ends_with('e') {
                    if let Some(next_t) = self.peek() {
                        if next_t.value == "^" || next_t.value == "**" {
                            let prefix = name[..name.len() - 1].to_string();
                            self.tokens.insert(self.pos, PhsToken {
                                kind: TokenKind::Ident("e".to_string()),
                                value: "e".to_string(),
                                pos: tok.pos + prefix.len(),
                            });
                            return Ok(Node::Symbol(prefix));
                        }
                    }
                }
                Ok(Node::Symbol(name.clone()))
            }
            TokenKind::Op(ref op) if op == "(" => {
                let node = self.parse_equality()?;
                if let Some(close_t) = self.next() {
                    if close_t.value != ")" {
                        return Err(PhysureError::Generic("Expected closing ')'".to_string()));
                    }
                } else {
                    return Err(PhysureError::Generic("Expected closing ')'".to_string()));
                }
                Ok(node)
            }
            TokenKind::StringLiteral(ref s) => {
                Self::parse_str(s)
            }
            TokenKind::Sqrt => {
                if let Some(next_t) = self.peek() {
                    if next_t.value == "(" {
                        return self.parse_func_call("sqrt");
                    }
                }
                Err(PhysureError::Generic("Expected '(' after sqrt".to_string()))
            }
            _ => Err(PhysureError::Generic(format!(
                "Unexpected token '{}' in expression",
                tok.value
            ))),
        }
    }

    fn parse_func_call(&mut self, name: &str) -> PhysureResult<Node> {
        self.next(); // consume '('
        let mut args = Vec::new();
        if let Some(t) = self.peek() {
            if t.value != ")" {
                loop {
                    args.push(self.parse_equality()?);
                    if let Some(next_t) = self.peek() {
                        if next_t.value == "," {
                            self.next();
                            continue;
                        }
                    }
                    break;
                }
            }
        }
        if let Some(close_t) = self.next() {
            if close_t.value != ")" {
                return Err(PhysureError::Generic("Expected closing ')' after function arguments".to_string()));
            }
        }

        match name {
            "sqrt" => {
                if args.len() != 1 {
                    return Err(PhysureError::Generic("sqrt requires 1 argument".to_string()));
                }
                Ok(Node::Sqrt(Box::new(args.remove(0))))
            }
            "sin" => {
                if args.len() != 1 {
                    return Err(PhysureError::Generic("sin requires 1 argument".to_string()));
                }
                Ok(Node::Sin(Box::new(args.remove(0))))
            }
            "cos" => {
                if args.len() != 1 {
                    return Err(PhysureError::Generic("cos requires 1 argument".to_string()));
                }
                Ok(Node::Cos(Box::new(args.remove(0))))
            }

            "ln" | "log" => {
                if args.len() != 1 {
                    return Err(PhysureError::Generic("ln/log requires 1 argument".to_string()));
                }
                Ok(Node::Ln(Box::new(args.remove(0))))
            }
            "exp" => {
                if args.len() != 1 {
                    return Err(PhysureError::Generic("exp requires 1 argument".to_string()));
                }
                Ok(Node::Exp(Box::new(args.remove(0))))
            }
            "tan" => {
                if args.len() != 1 { return Err(PhysureError::Generic("tan requires 1 argument".to_string())); }
                Ok(Node::Tan(Box::new(args.remove(0))))
            }
            "cot" => {
                if args.len() != 1 { return Err(PhysureError::Generic("cot requires 1 argument".to_string())); }
                Ok(Node::Cot(Box::new(args.remove(0))))
            }
            "sec" => {
                if args.len() != 1 { return Err(PhysureError::Generic("sec requires 1 argument".to_string())); }
                Ok(Node::Sec(Box::new(args.remove(0))))
            }
            "csc" | "cosec" => {
                if args.len() != 1 { return Err(PhysureError::Generic("csc requires 1 argument".to_string())); }
                Ok(Node::Csc(Box::new(args.remove(0))))
            }
            "arcsin" | "asin" => {
                if args.len() != 1 { return Err(PhysureError::Generic("arcsin requires 1 argument".to_string())); }
                Ok(Node::Arcsin(Box::new(args.remove(0))))
            }
            "arccos" | "acos" => {
                if args.len() != 1 { return Err(PhysureError::Generic("arccos requires 1 argument".to_string())); }
                Ok(Node::Arccos(Box::new(args.remove(0))))
            }
            "arctan" | "atan" => {
                if args.len() != 1 { return Err(PhysureError::Generic("arctan requires 1 argument".to_string())); }
                Ok(Node::Arctan(Box::new(args.remove(0))))
            }
            "arccot" | "acot" => {
                if args.len() != 1 { return Err(PhysureError::Generic("arccot requires 1 argument".to_string())); }
                Ok(Node::Arccot(Box::new(args.remove(0))))
            }
            "arcsec" | "asec" => {
                if args.len() != 1 { return Err(PhysureError::Generic("arcsec requires 1 argument".to_string())); }
                Ok(Node::Arcsec(Box::new(args.remove(0))))
            }
            "arccsc" | "acsc" => {
                if args.len() != 1 { return Err(PhysureError::Generic("arccsc requires 1 argument".to_string())); }
                Ok(Node::Arccsc(Box::new(args.remove(0))))
            }
            "sinh" => {
                if args.len() != 1 { return Err(PhysureError::Generic("sinh requires 1 argument".to_string())); }
                Ok(Node::Sinh(Box::new(args.remove(0))))
            }
            "cosh" => {
                if args.len() != 1 { return Err(PhysureError::Generic("cosh requires 1 argument".to_string())); }
                Ok(Node::Cosh(Box::new(args.remove(0))))
            }
            "tanh" => {
                if args.len() != 1 { return Err(PhysureError::Generic("tanh requires 1 argument".to_string())); }
                Ok(Node::Tanh(Box::new(args.remove(0))))
            }
            "coth" => {
                if args.len() != 1 { return Err(PhysureError::Generic("coth requires 1 argument".to_string())); }
                Ok(Node::Coth(Box::new(args.remove(0))))
            }
            "sech" => {
                if args.len() != 1 { return Err(PhysureError::Generic("sech requires 1 argument".to_string())); }
                Ok(Node::Sech(Box::new(args.remove(0))))
            }
            "csch" => {
                if args.len() != 1 { return Err(PhysureError::Generic("csch requires 1 argument".to_string())); }
                Ok(Node::Csch(Box::new(args.remove(0))))
            }
            "abs" => {
                if args.len() != 1 { return Err(PhysureError::Generic("abs requires 1 argument".to_string())); }
                Ok(Node::Abs(Box::new(args.remove(0))))
            }
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
            _ => Err(PhysureError::Generic(format!("Unknown symbolic function '{}'", name))),
        }
    }
}
