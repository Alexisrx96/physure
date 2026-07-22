use physure_core::error::{PhysureError, PhysureResult};
use super::ast::{BinaryOp, Expr, ParamDef, Statement, UnaryOp};
use super::lexer::{PhsToken, TokenKind};

pub fn parse_phs(input: &str) -> PhysureResult<Vec<Statement>> {
    let lexer = super::lexer::PhsLexer::new(input);
    let tokens = lexer.tokenize()?;
    let mut parser = PhsParser::new(&tokens);
    parser.parse_statements()
}

pub struct PhsParser<'a> {
    tokens: &'a [PhsToken],
    pos: usize,
}

impl<'a> PhsParser<'a> {
    pub fn new(tokens: &'a [PhsToken]) -> Self {
        Self { tokens, pos: 0 }
    }

    pub fn parse_statements(&mut self) -> PhysureResult<Vec<Statement>> {
        let mut stmts = Vec::new();
        while !self.is_at_end() {
            let stmt = self.parse_statement()?;
            stmts.push(stmt);
        }
        Ok(stmts)
    }

    pub fn parse_statement(&mut self) -> PhysureResult<Statement> {
        // Check if function definition: ident "(" params ")" "=" expr
        if let Some(stmt) = self.try_parse_fn_def()? {
            return Ok(stmt);
        }

        let expr = self.parse_expr_with_modifiers()?;

        if self.match_op("=") || self.match_op("->") {
            if self.match_op("?") {
                if let Expr::Ident(name) = expr {
                    return Ok(Statement::AssignAndQuery {
                        name,
                        expr: Expr::Number(0.0), // placeholder if needed
                    });
                }
            }

            if let Expr::Ident(name) = expr {
                let right_expr = self.parse_expr_with_modifiers()?;
                if self.match_op("=") && self.match_op("?") {
                    return Ok(Statement::AssignAndQuery {
                        name,
                        expr: right_expr,
                    });
                }
                return Ok(Statement::Assign {
                    name,
                    expr: right_expr,
                });
            }
        }

        if self.match_op("?") {
            return Ok(Statement::Query { expr });
        }

        Ok(Statement::ExprStmt(expr))
    }

    /// Parses an expression, folding any trailing `=> unit` / `: spec` modifiers
    /// into `Expr::Convert`/`Expr::FormatSig` nodes so they can chain (e.g. `500 N => kN : 2`).
    fn parse_expr_with_modifiers(&mut self) -> PhysureResult<Expr> {
        let mut expr = self.parse_expr()?;
        loop {
            if self.match_op("=>") {
                let target_unit = self.parse_unit_string()?;
                expr = Expr::Convert { expr: Box::new(expr), target_unit };
            } else if self.match_op(":") {
                let spec = self.parse_unit_string()?;
                expr = Expr::FormatSig { expr: Box::new(expr), spec };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn try_parse_fn_def(&mut self) -> PhysureResult<Option<Statement>> {
        let saved = self.pos;
        let fn_name = match self.peek_kind() {
            Some(TokenKind::Ident(name)) => name.clone(),
            _ => return Ok(None),
        };
        if self.peek_offset_kind(1) == Some(&TokenKind::Op("(".to_string())) {
            self.pos += 2; // consume name and '('
            let mut params = Vec::new();
            while !self.is_at_end() && self.peek_kind() != Some(&TokenKind::Op(")".to_string())) {
                if let Some(TokenKind::Ident(param_name)) = self.peek_kind().cloned() {
                    self.pos += 1;
                    let mut unit = None;
                    if self.match_op(":") {
                        let u = self.parse_unit_string()?;
                        unit = Some(u);
                    }
                    params.push(ParamDef { name: param_name, unit });
                    if !self.match_op(",") {
                        break;
                    }
                } else {
                    break;
                }
            }
            if self.match_op(")") && (self.match_op("=") || self.match_op("->")) {
                let mut body_stmts = Vec::new();
                while !self.is_at_end() {
                    body_stmts.push(self.parse_statement()?);
                    if self.match_op(";") {
                        continue;
                    }
                }
                return Ok(Some(Statement::FnDef {
                    name: fn_name,
                    params,
                    body: body_stmts,
                }));
            }
        }
        self.pos = saved;
        Ok(None)
    }

    pub fn parse_expr(&mut self) -> PhysureResult<Expr> {
        if self.match_ident("let") {
            let name = self.expect_any_ident()?;
            self.expect_op("=")?;
            let val = Box::new(self.parse_expr()?);
            self.expect_ident("in")?;
            let body = Box::new(self.parse_expr()?);
            return Ok(Expr::Let { name, val, body });
        }

        if self.match_ident("if") {
            let cond = Box::new(self.parse_expr()?);
            let has_brace_then = self.match_op("{");
            if !has_brace_then {
                let _ = self.match_ident("then");
            }
            let then_expr = Box::new(self.parse_expr()?);
            if has_brace_then {
                self.expect_op("}")?;
            }
            self.expect_ident("else")?;
            let has_brace_else = self.match_op("{");
            let else_expr = Box::new(self.parse_expr()?);
            if has_brace_else {
                self.expect_op("}")?;
            }
            return Ok(Expr::If {
                cond,
                then_expr,
                else_expr,
            });
        }

        self.parse_ternary()
    }

    fn parse_ternary(&mut self) -> PhysureResult<Expr> {
        let cond = self.parse_comparison()?;
        if self.match_op("?") {
            let then_expr = Box::new(self.parse_expr()?);
            self.expect_op(":")?;
            let else_expr = Box::new(self.parse_expr()?);
            return Ok(Expr::Ternary {
                cond: Box::new(cond),
                then_expr,
                else_expr,
            });
        }
        Ok(cond)
    }

    fn parse_comparison(&mut self) -> PhysureResult<Expr> {
        let mut left = self.parse_additive()?;
        loop {
            let op = if self.match_op("==") {
                BinaryOp::Eq
            } else if self.match_op("!=") {
                BinaryOp::Neq
            } else if self.match_op("<=") {
                BinaryOp::Lte
            } else if self.match_op(">=") {
                BinaryOp::Gte
            } else if self.match_op("<") {
                BinaryOp::Lt
            } else if self.match_op(">") {
                BinaryOp::Gt
            } else if self.match_op("≈") {
                BinaryOp::ApproxEq
            } else {
                break;
            };
            let right = self.parse_additive()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> PhysureResult<Expr> {
        let mut left = self.parse_multiplicative()?;
        loop {
            if self.match_op("+/-") || self.match_op("±") {
                let unc = self.parse_multiplicative()?;
                left = Expr::Uncertainty {
                    val: Box::new(left),
                    unc: Box::new(unc),
                };
            } else if self.match_op("+") {
                let right = self.parse_multiplicative()?;
                left = Expr::Binary {
                    op: BinaryOp::Add,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else if self.match_op("-") {
                let right = self.parse_multiplicative()?;
                left = Expr::Binary {
                    op: BinaryOp::Sub,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> PhysureResult<Expr> {
        let mut left = self.parse_implicit_mul()?;
        loop {
            if self.match_op("*") {
                let right = self.parse_implicit_mul()?;
                left = Expr::Binary {
                    op: BinaryOp::Mul,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else if self.match_op("/") {
                let right = self.parse_implicit_mul()?;
                left = Expr::Binary {
                    op: BinaryOp::Div,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_implicit_mul(&mut self) -> PhysureResult<Expr> {
        let mut left = self.parse_power()?;
        while self.can_start_implicit_mul() {
            let right = self.parse_power()?;
            left = Expr::ImplicitMul {
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn can_start_implicit_mul(&self) -> bool {
        if self.is_at_end() {
            return false;
        }
        match self.peek_kind() {
            Some(TokenKind::Ident(name)) => {
                !matches!(name.as_str(), "in" | "then" | "else")
            }
            Some(TokenKind::Number(_))
            | Some(TokenKind::StringLiteral(_))
            | Some(TokenKind::Sqrt) => true,
            Some(TokenKind::Op(op)) => op == "(" || op == "[",
            _ => false,
        }
    }

    fn parse_power(&mut self) -> PhysureResult<Expr> {
        let left = self.parse_unary()?;
        if self.match_op("^") || self.match_op("**") {
            let right = self.parse_power()?;
            return Ok(Expr::Binary {
                op: BinaryOp::Pow,
                left: Box::new(left),
                right: Box::new(right),
            });
        }
        if let Some(TokenKind::Sup(digits)) = self.peek_kind() {
            let digits_str = digits.clone();
            self.pos += 1;
            let val = parse_sup_digits(&digits_str)?;
            return Ok(Expr::Binary {
                op: BinaryOp::Pow,
                left: Box::new(left),
                right: Box::new(Expr::Number(val)),
            });
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> PhysureResult<Expr> {
        if self.match_op("-") {
            let expr = self.parse_unary()?;
            return Ok(Expr::Unary {
                op: UnaryOp::Neg,
                expr: Box::new(expr),
            });
        }
        if self.match_kind(&TokenKind::Sqrt) {
            let expr = self.parse_unary()?;
            return Ok(Expr::Unary {
                op: UnaryOp::Sqrt,
                expr: Box::new(expr),
            });
        }
        self.parse_atom()
    }

    fn parse_atom(&mut self) -> PhysureResult<Expr> {
        if let Some(kind) = self.peek_kind().cloned() {
            match kind {
                TokenKind::Number(n) => {
                    self.pos += 1;
                    return Ok(Expr::Number(n));
                }
                TokenKind::StringLiteral(s) => {
                    self.pos += 1;
                    return Ok(Expr::StringLiteral(s));
                }
                TokenKind::Ident(name) => {
                    self.pos += 1;
                    if self.match_op("(") {
                        let mut args = Vec::new();
                        if !self.match_op(")") {
                            loop {
                                args.push(self.parse_expr()?);
                                if !self.match_op(",") {
                                    break;
                                }
                            }
                            self.expect_op(")")?;
                        }
                        return Ok(Expr::Call { name, args });
                    }
                    return Ok(Expr::Ident(name));
                }
                TokenKind::Op(ref op) if op == "(" => {
                    self.pos += 1;
                    let inner = self.parse_expr()?;
                    self.expect_op(")")?;
                    return Ok(inner);
                }
                TokenKind::Op(ref op) if op == "[" => {
                    self.pos += 1;
                    let mut items = Vec::new();
                    if !self.match_op("]") {
                        loop {
                            items.push(self.parse_expr()?);
                            if !self.match_op(",") {
                                break;
                            }
                        }
                        self.expect_op("]")?;
                    }
                    return Ok(Expr::Vector(items));
                }
                _ => {}
            }
        }
        Err(PhysureError::Generic(format!(
            "Unexpected token at pos {}",
            self.pos
        )))
    }

    fn parse_unit_string(&mut self) -> PhysureResult<String> {
        let mut unit_parts = Vec::new();
        while !self.is_at_end() {
            match self.peek_kind() {
                Some(TokenKind::Ident(id)) => {
                    unit_parts.push(id.clone());
                    self.pos += 1;
                }
                Some(TokenKind::Op(op)) if op == "/" || op == "*" || op == "^" => {
                    unit_parts.push(op.clone());
                    self.pos += 1;
                }
                Some(TokenKind::Number(n)) => {
                    unit_parts.push(n.to_string());
                    self.pos += 1;
                }
                Some(TokenKind::Sup(s)) => {
                    unit_parts.push(s.clone());
                    self.pos += 1;
                }
                _ => break,
            }
        }
        if unit_parts.is_empty() {
            return Err(PhysureError::Generic("Expected target unit".to_string()));
        }
        Ok(unit_parts.join(""))
    }

    fn peek_kind(&self) -> Option<&TokenKind> {
        self.tokens.get(self.pos).map(|t| &t.kind)
    }

    fn peek_offset_kind(&self, offset: usize) -> Option<&TokenKind> {
        self.tokens.get(self.pos + offset).map(|t| &t.kind)
    }

    fn match_kind(&mut self, kind: &TokenKind) -> bool {
        if self.peek_kind() == Some(kind) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn match_op(&mut self, op: &str) -> bool {
        if let Some(TokenKind::Op(o)) = self.peek_kind() {
            if o == op {
                self.pos += 1;
                return true;
            }
        }
        false
    }

    fn match_ident(&mut self, ident: &str) -> bool {
        if let Some(TokenKind::Ident(i)) = self.peek_kind() {
            if i == ident {
                self.pos += 1;
                return true;
            }
        }
        false
    }

    fn expect_op(&mut self, op: &str) -> PhysureResult<()> {
        if self.match_op(op) {
            Ok(())
        } else {
            Err(PhysureError::Generic(format!("Expected operator '{}'", op)))
        }
    }

    fn expect_ident(&mut self, expected: &str) -> PhysureResult<String> {
        if let Some(TokenKind::Ident(i)) = self.peek_kind().cloned() {
            if expected.is_empty() || i == expected {
                self.pos += 1;
                return Ok(i);
            }
        }
        Err(PhysureError::Generic(format!(
            "Expected identifier '{}'",
            expected
        )))
    }

    fn expect_any_ident(&mut self) -> PhysureResult<String> {
        if let Some(TokenKind::Ident(i)) = self.peek_kind().cloned() {
            self.pos += 1;
            return Ok(i);
        }
        Err(PhysureError::Generic("Expected identifier".to_string()))
    }

    fn is_at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }
}

fn parse_sup_digits(s: &str) -> PhysureResult<f64> {
    let ascii: String = s
        .chars()
        .map(|c| match c {
            '⁻' => '-',
            '⁰' => '0',
            '¹' => '1',
            '²' => '2',
            '³' => '3',
            '⁴' => '4',
            '⁵' => '5',
            '⁶' => '6',
            '⁷' => '7',
            '⁸' => '8',
            '⁹' => '9',
            _ => c,
        })
        .collect();
    ascii
        .parse::<f64>()
        .map_err(|_| PhysureError::Generic(format!("Invalid superscript digits: {}", s)))
}
