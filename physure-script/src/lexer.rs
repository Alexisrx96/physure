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

            // Comments: # ... or // ...
            if ch == '#' || (ch == '/' && self.input[self.pos..].starts_with("//")) {
                self.skip_comment();
                continue;
            }

            if self.lex_display_fence(&mut tokens) {
                continue;
            }
            let start = self.pos;
            tokens.push(self.lex_token(ch, start)?);
        }

        Ok(tokens)
    }

    /// Skips to the start of the next line (or the end of the input).
    fn skip_comment(&mut self) {
        self.pos = self.input[self.pos..].find('\n').map_or(self.input.len(), |newline| self.pos + newline + 1);
    }

    /// Display text fence: ``` ... ```, as its text and then the fence itself. Returns false, with nothing
    /// consumed, when there is no closing fence on this line (e.g. a fence that spans multiple lines, when
    /// tokenized line-by-line by callers like the LSP): the opening "```" is then lexed as a plain operator.
    fn lex_display_fence(&mut self, tokens: &mut Vec<PhsToken>) -> bool {
        if !self.input[self.pos..].starts_with("```") {
            return false;
        }
        let fence_start = self.pos;
        let text_start = fence_start + 3;
        let Some(end_rel) = self.input[text_start..].find("```") else {
            return false;
        };
        let text = self.input[text_start..text_start + end_rel].to_string();
        self.pos = text_start + end_rel + 3;
        tokens.push(token(TokenKind::StringLiteral(text.clone()), text, fence_start));
        tokens.push(token(TokenKind::Op("```".to_string()), "```".to_string(), fence_start));
        true
    }

    /// The token that starts with `ch` at `start`: a string, a square root, superscript digits, a number,
    /// an identifier or an operator.
    fn lex_token(&mut self, ch: char, start: usize) -> PhysureResult<PhsToken> {
        if ch == '"' || ch == '\'' {
            return self.lex_string(ch, start);
        }
        if ch == '√' {
            self.pos += ch.len_utf8();
            return Ok(token(TokenKind::Sqrt, "√".to_string(), start));
        }
        if is_superscript(ch) {
            let end = self.consume_while(is_superscript);
            let val = self.input[start..end].to_string();
            return Ok(token(TokenKind::Sup(val.clone()), val, start));
        }
        // Numbers: digits, decimal, exponent
        if ch.is_ascii_digit() || (ch == '.' && self.peek_digit()) {
            return self.lex_number(start);
        }
        // Identifiers: letters, underscores, and trailing prime marks (')
        if ch.is_alphabetic() || ch == '_' {
            self.consume_while(|c| c.is_alphanumeric() || c == '_');
            self.consume_while(|c| c == '\'');
            let val = self.input[start..self.pos].to_string();
            return Ok(token(TokenKind::Ident(val.clone()), val, start));
        }
        // Operators & Punctuation
        let op_str = self.consume_operator()?;
        Ok(token(TokenKind::Op(op_str.clone()), op_str, start))
    }

    /// String literals: "..." or '...'
    fn lex_string(&mut self, quote: char, start: usize) -> PhysureResult<PhsToken> {
        self.pos += 1;
        let str_start = self.pos;
        self.consume_while(|c| c != quote);
        if self.pos >= self.input.len() {
            return Err(PhysureError::Generic(format!(
                "Unclosed string literal starting at column {}",
                start
            )));
        }
        let content = self.input[str_start..self.pos].to_string();
        self.pos += 1; // consume closing quote
        Ok(token(TokenKind::StringLiteral(content.clone()), content, start))
    }

    fn lex_number(&mut self, start: usize) -> PhysureResult<PhsToken> {
        let end = self.consume_number();
        let val_str = &self.input[start..end];
        let num: f64 = val_str.replace(' ', "").parse().map_err(|_| {
            PhysureError::Generic(format!("Invalid number literal: '{}'", val_str))
        })?;
        Ok(token(TokenKind::Number(num), val_str.to_string(), start))
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
        while let Some(ch) = self.input[self.pos..].chars().next() {
            if ch.is_ascii_digit() {
                self.pos += 1;
            } else if ch == '.' && !seen_dot && !seen_e {
                seen_dot = true;
                self.pos += 1;
            } else if (ch == 'e' || ch == 'E') && !seen_e {
                let Some(marker_len) = self.exponent_marker_len() else {
                    break;
                };
                seen_e = true;
                self.pos += marker_len;
            } else {
                break;
            }
        }
        self.pos
    }

    /// The length of the `e`/`E` at the current position, with its sign if it has one, when a digit follows:
    /// `1e5` and `1e-5` have an exponent, `1em` does not.
    fn exponent_marker_len(&self) -> Option<usize> {
        let after_e = &self.input[self.pos + 1..];
        let sign_len = usize::from(matches!(after_e.chars().next(), Some('+' | '-')));
        after_e[sign_len..].chars().next().filter(|c| c.is_ascii_digit()).map(|_| 1 + sign_len)
    }

    fn consume_operator(&mut self) -> PhysureResult<String> {
        let rest = &self.input[self.pos..];
        let ops = [
            "```", "+/-", "±", "<=", ">=", "!=", "==", "=>", "->", "≈", "**", "..",
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

fn token(kind: TokenKind, value: String, pos: usize) -> PhsToken {
    PhsToken { kind, value, pos }
}

fn is_superscript(c: char) -> bool {
    matches!(c, '⁻' | '⁰' | '¹' | '²' | '³' | '⁴' | '⁵' | '⁶' | '⁷' | '⁸' | '⁹')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ops(input: &str) -> Vec<String> {
        PhsLexer::new(input)
            .tokenize()
            .unwrap()
            .into_iter()
            .filter(|t| matches!(t.kind, TokenKind::Op(_)))
            .map(|t| t.value)
            .collect()
    }

    #[test]
    fn an_uncertainty_pair_does_not_collide_with_the_binary_operators() {
        // The multi-character forms come first in the operator table, so `+/-` is never read
        // as `+` followed by `/`, and the parentheses that follow it stay ordinary tokens.
        assert_eq!(ops("12.3 +/- (0.5, 0.4) pb"), ["+/-", "(", ",", ")"]);
        assert_eq!(ops("12.3 ± (0.5, 0.4) pb"), ["±", "(", ",", ")"]);
        // A bare sign is still addition, whatever follows it.
        assert_eq!(ops("12.3 + (0.5)"), ["+", "(", ")"]);
        assert_eq!(ops("12.3 - 0.5"), ["-"]);
    }
}
