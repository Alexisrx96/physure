use physure_core::error::{PhysureError, PhysureResult};
use crate::{PhsLexer, PhsToken, TokenKind};
use super::ast::Node;

pub struct SymbolicParser {
    tokens: Vec<PhsToken>,
    pos: usize,
}

impl SymbolicParser {
    pub fn parse_str(input: &str) -> PhysureResult<Node> {
        let clean_input = input.replace('"', "").replace('\'', "");
        let lexer = PhsLexer::new(&clean_input);
        let tokens = lexer.tokenize()?;
        let mut parser = SymbolicParser { tokens, pos: 0 };
        let node = parser.parse_equality()?;
        Ok(node.simplify())
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
            if (matches!(&t.kind, TokenKind::Op(_)) || t.value == op_str) && t.value == op_str {
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
            left = Node::Sub(Box::new(left), Box::new(right));
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
                let right = self.parse_unary()?;
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
                Ok(Node::Pow(Box::new(args.remove(0)), Box::new(Node::Number(0.5))))
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
