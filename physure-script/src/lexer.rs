use physure_core::error::{PhysureError, PhysureResult};

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Number(f64),
    Ident(String),
    StringLiteral(String),
    Op(String),
    Sup(String),
    Sqrt,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PhsToken {
    pub kind: TokenKind,
    pub value: String,
    pub pos: usize,
}

pub struct PhsLexer<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> PhsLexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    pub fn tokenize(mut self) -> PhysureResult<Vec<PhsToken>> {
        let mut tokens = Vec::new();
        let len = self.input.len();

        while self.pos < len {
            let ch = self.input[self.pos..].chars().next().unwrap();
            if ch.is_whitespace() {
                self.pos += ch.len_utf8();
                continue;
            }

            let start = self.pos;

            // String literals: "..." or '...'
            if ch == '"' || ch == '\'' {
                let quote = ch;
                self.pos += 1;
                let str_start = self.pos;
                while self.pos < len {
                    let c = self.input[self.pos..].chars().next().unwrap();
                    if c == quote {
                        break;
                    }
                    self.pos += c.len_utf8();
                }
                if self.pos >= len {
                    return Err(PhysureError::Generic(format!(
                        "Unclosed string literal starting at column {}",
                        start
                    )));
                }
                let content = self.input[str_start..self.pos].to_string();
                self.pos += 1; // consume closing quote
                tokens.push(PhsToken {
                    kind: TokenKind::StringLiteral(content.clone()),
                    value: content,
                    pos: start,
                });
                continue;
            }

            // Square root symbol
            if ch == '√' {
                self.pos += '√'.len_utf8();
                tokens.push(PhsToken {
                    kind: TokenKind::Sqrt,
                    value: "√".to_string(),
                    pos: start,
                });
                continue;
            }

            // Superscript digits
            if is_superscript(ch) {
                let end = self.consume_while(is_superscript);
                let val = self.input[start..end].to_string();
                tokens.push(PhsToken {
                    kind: TokenKind::Sup(val.clone()),
                    value: val,
                    pos: start,
                });
                continue;
            }

            // Numbers: digits, decimal, exponent
            if ch.is_ascii_digit() || (ch == '.' && self.peek_digit()) {
                let end = self.consume_number();
                let val_str = &self.input[start..end];
                let clean_str = val_str.replace(' ', "");
                let num: f64 = clean_str.parse().map_err(|_| {
                    PhysureError::Generic(format!("Invalid number literal: '{}'", val_str))
                })?;
                tokens.push(PhsToken {
                    kind: TokenKind::Number(num),
                    value: val_str.to_string(),
                    pos: start,
                });
                continue;
            }

            // Identifiers: letters, underscores
            if ch.is_alphabetic() || ch == '_' {
                let end = self.consume_while(|c| c.is_alphanumeric() || c == '_');
                let val = self.input[start..end].to_string();
                tokens.push(PhsToken {
                    kind: TokenKind::Ident(val.clone()),
                    value: val,
                    pos: start,
                });
                continue;
            }

            // Operators & Punctuation
            let op_str = self.consume_operator()?;
            tokens.push(PhsToken {
                kind: TokenKind::Op(op_str.clone()),
                value: op_str,
                pos: start,
            });
        }

        Ok(tokens)
    }

    fn peek_digit(&self) -> bool {
        let mut chars = self.input[self.pos..].chars();
        chars.next(); // skip current '.'
        chars.next().map_or(false, |c| c.is_ascii_digit())
    }

    fn consume_while<F>(&mut self, mut predicate: F) -> usize
    where
        F: FnMut(char) -> bool,
    {
        while self.pos < self.input.len() {
            let ch = self.input[self.pos..].chars().next().unwrap();
            if predicate(ch) {
                self.pos += ch.len_utf8();
            } else {
                break;
            }
        }
        self.pos
    }

    fn consume_number(&mut self) -> usize {
        let mut seen_dot = false;
        let mut seen_e = false;
        while self.pos < self.input.len() {
            let ch = self.input[self.pos..].chars().next().unwrap();
            if ch.is_ascii_digit() {
                self.pos += 1;
            } else if ch == '.' && !seen_dot && !seen_e {
                seen_dot = true;
                self.pos += 1;
            } else if (ch == 'e' || ch == 'E') && !seen_e {
                let rest = &self.input[self.pos + ch.len_utf8()..];
                let mut chars = rest.chars();
                let mut next_c = chars.next();
                if next_c == Some('+') || next_c == Some('-') {
                    next_c = chars.next();
                }
                if next_c.map_or(false, |c| c.is_ascii_digit()) {
                    seen_e = true;
                    self.pos += ch.len_utf8();
                    if self.pos < self.input.len() {
                        let sign_c = self.input[self.pos..].chars().next().unwrap();
                        if sign_c == '+' || sign_c == '-' {
                            self.pos += sign_c.len_utf8();
                        }
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        self.pos
    }

    fn consume_operator(&mut self) -> PhysureResult<String> {
        let rest = &self.input[self.pos..];
        let ops = [
            "```", "+/-", "±", "<=", ">=", "!=", "==", "=>", "->", "≈", "**",
            "+", "-", "*", "/", "^", "(", ")", "=", "?", "<", ">", "×", "÷", ",", ":", ";", ".", "|", "[", "]", "{", "}", "`",
        ];

        for &op in &ops {
            if rest.starts_with(op) {
                self.pos += op.len();
                let canonical_op = match op {
                    "×" => "*",
                    "÷" => "/",
                    _ => op,
                };
                return Ok(canonical_op.to_string());
            }
        }

        let ch = rest.chars().next().unwrap();
        Err(PhysureError::Generic(format!(
            "Unexpected character '{}' at column {} in: '{:?}'",
            ch, self.pos, self.input
        )))
    }
}

fn is_superscript(c: char) -> bool {
    matches!(c, '⁻' | '⁰' | '¹' | '²' | '³' | '⁴' | '⁵' | '⁶' | '⁷' | '⁸' | '⁹')
}
