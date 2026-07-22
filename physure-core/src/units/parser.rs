use crate::error::{PhysureError, PhysureResult};
use crate::units::rational::RationalUnit;
use crate::units::registry::UnitRegistry;
use num_rational::Rational64;
use std::collections::HashMap;

/// Normalizes unicode superscripts and subscripts in a string to standard ASCII digits.
fn normalize_unicode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '⁰' => out.push('0'),
            '¹' => out.push('1'),
            '²' => out.push('2'),
            '³' => out.push('3'),
            '⁴' => out.push('4'),
            '⁵' => out.push('5'),
            '⁶' => out.push('6'),
            '⁷' => out.push('7'),
            '⁸' => out.push('8'),
            '⁹' => out.push('9'),
            '⁻' | '₋' => out.push('-'),
            '₀' => out.push('0'),
            '₁' => out.push('1'),
            '₂' => out.push('2'),
            '₃' => out.push('3'),
            '₄' => out.push('4'),
            '₅' => out.push('5'),
            '₆' => out.push('6'),
            '₇' => out.push('7'),
            '₈' => out.push('8'),
            '₉' => out.push('9'),
            '⋅' | '·' => out.push('*'),
            other => out.push(other),
        }
    }
    out
}

#[derive(Debug, PartialEq, Clone)]
enum Token {
    Symbol(String),
    Number(i64, i64),
    Mul,
    Div,
    Pow,
    LParen,
    RParen,
    Eof,
}

struct Lexer<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        Lexer {
            chars: input.chars().peekable(),
        }
    }

    fn next_token(&mut self) -> PhysureResult<Token> {
        while let Some(&c) = self.chars.peek() {
            if c.is_whitespace() {
                self.chars.next();
                continue;
            }
            break;
        }

        let c = match self.chars.next() {
            Some(c) => c,
            None => return Ok(Token::Eof),
        };

        match c {
            '(' => Ok(Token::LParen),
            ')' => Ok(Token::RParen),
            '*' => {
                if self.chars.peek() == Some(&'*') {
                    self.chars.next();
                }
                Ok(Token::Mul)
            }
            '/' => Ok(Token::Div),
            '^' => Ok(Token::Pow),
            '0'..='9' => {
                let mut num_str = String::new();
                num_str.push(c);
                while let Some(&d) = self.chars.peek() {
                    if d.is_ascii_digit() {
                        num_str.push(d);
                        self.chars.next();
                    } else {
                        break;
                    }
                }
                let val: i64 = num_str.parse().map_err(|_| PhysureError::ParseError("Invalid integer".into()))?;
                Ok(Token::Number(val, 1))
            }
            '-' => {
                if let Some(&d) = self.chars.peek() {
                    if d.is_ascii_digit() {
                        self.chars.next();
                        let mut num_str = String::new();
                        num_str.push(d);
                        while let Some(&next_d) = self.chars.peek() {
                            if next_d.is_ascii_digit() {
                                num_str.push(next_d);
                                self.chars.next();
                            } else {
                                break;
                            }
                        }
                        let val: i64 = num_str.parse().map_err(|_| PhysureError::ParseError("Invalid integer".into()))?;
                        return Ok(Token::Number(-val, 1));
                    }
                }
                Err(PhysureError::ParseError("Unexpected character '-'".into()))
            }
            _ if is_unit_char(c) => {
                let mut sym = String::new();
                sym.push(c);
                while let Some(&next_c) = self.chars.peek() {
                    if is_unit_char(next_c) || next_c.is_ascii_digit() {
                        sym.push(next_c);
                        self.chars.next();
                    } else {
                        break;
                    }
                }
                Ok(Token::Symbol(sym))
            }
            other => Err(PhysureError::ParseError(format!("Unexpected character '{}'", other))),
        }
    }
}

fn is_unit_char(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '°' || c == 'Ω' || c == 'µ' || c == '$' || c == '%'
}

pub struct Parser<'a> {
    lexer: Lexer<'a>,
    current_token: Token,
    registry: Option<&'a UnitRegistry>,
}

impl<'a> Parser<'a> {
    pub fn new(input: &'a str) -> PhysureResult<Self> {
        let mut lexer = Lexer::new(input);
        let current_token = lexer.next_token()?;
        Ok(Parser {
            lexer,
            current_token,
            registry: None,
        })
    }

    fn advance(&mut self) -> PhysureResult<()> {
        self.current_token = self.lexer.next_token()?;
        Ok(())
    }

    pub fn parse_expression(input: &str) -> PhysureResult<RationalUnit> {
        let (registry, _) = crate::units::conf::build_registry_from_conf();
        Self::parse_expression_impl(input, Some(&registry))
    }

    /// Like `parse_expression`, but resolves each symbol against `registry` first so
    /// aliases and prefixed units (e.g. "km", "kN") carry their real scale factor.
    /// Symbols not found in the registry fall back to the atomic (scale-1) behavior.
    pub fn parse_expression_with_registry(input: &str, registry: &UnitRegistry) -> PhysureResult<RationalUnit> {
        Self::parse_expression_impl(input, Some(registry))
    }

    fn parse_expression_impl(input: &str, registry: Option<&UnitRegistry>) -> PhysureResult<RationalUnit> {
        let normalized = normalize_unicode(input);
        let mut lexer = Lexer::new(&normalized);
        let first_tok = lexer.next_token()?;
        let mut p = Parser {
            lexer,
            current_token: first_tok,
            registry,
        };
        let mut res = p.parse_expr()?;
        if p.current_token != Token::Eof {
            return Err(PhysureError::ParseError(format!("Trailing tokens after expression: {:?}", p.current_token)));
        }
        res.display_name = Some(input.trim().to_string());
        Ok(res)
    }

    fn parse_expr(&mut self) -> PhysureResult<RationalUnit> {
        if self.current_token == Token::Eof {
            return Ok(RationalUnit::dimensionless());
        }

        let mut left = self.parse_factor()?;

        while self.current_token == Token::Mul
            || self.current_token == Token::Div
            || matches!(self.current_token, Token::Symbol(_))
            || self.current_token == Token::LParen
        {
            if self.current_token == Token::Mul {
                self.advance()?;
                let right = self.parse_factor()?;
                left = left.mul(&right);
            } else if self.current_token == Token::Div {
                self.advance()?;
                let right = self.parse_factor()?;
                left = left.div(&right);
            } else {
                // Implicit multiplication (space or adjacent units e.g., "kg m")
                let right = self.parse_factor()?;
                left = left.mul(&right);
            }
        }
        Ok(left)
    }

    fn parse_factor(&mut self) -> PhysureResult<RationalUnit> {
        let mut base = match &self.current_token {
            Token::Symbol(sym) => {
                let symbol_name = sym.clone();
                self.advance()?;
                // Check if symbol has embedded exponent like "m2" or "s-1"
                let (name, exp_opt) = split_embedded_exponent(&symbol_name);
                let u = match self.registry.and_then(|r| r.get_unit(&name)) {
                    Some(registered) => registered,
                    None => {
                        let mut dims = HashMap::new();
                        dims.insert(name, (1, 1));
                        RationalUnit::new_from_dimensions(dims)
                    }
                };
                if let Some((n, d)) = exp_opt {
                    u.pow(Rational64::new(n, d))
                } else {
                    u
                }
            }
            Token::LParen => {
                self.advance()?;
                let inner = self.parse_expr()?;
                if self.current_token != Token::RParen {
                    return Err(PhysureError::ParseError("Expected ')'".into()));
                }
                self.advance()?;
                inner
            }
            Token::Number(1, 1) => {
                self.advance()?;
                RationalUnit::dimensionless()
            }
            tok => return Err(PhysureError::ParseError(format!("Unexpected token in factor: {:?}", tok))),
        };

        // Parse optional exponent operator ^ or ** or raw Token::Number (e.g. s-1 or m2)
        if self.current_token == Token::Pow {
            self.advance()?;
            let exp_rat = match self.current_token {
                Token::Number(n, d) => {
                    self.advance()?;
                    Rational64::new(n, d)
                }
                ref tok => return Err(PhysureError::ParseError(format!("Expected exponent number after '^', got {:?}", tok))),
            };
            base = base.pow(exp_rat);
        } else if let Token::Number(n, d) = self.current_token {
            self.advance()?;
            base = base.pow(Rational64::new(n, d));
        }

        Ok(base)
    }
}

fn split_embedded_exponent(sym: &str) -> (String, Option<(i64, i64)>) {
    let bytes = sym.as_bytes();
    for i in 1..bytes.len() {
        if bytes[i].is_ascii_digit() || (bytes[i] == b'-' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit()) {
            let name = sym[..i].to_string();
            if let Ok(num) = sym[i..].parse::<i64>() {
                // An embedded exponent of 0 is never a legitimate unit annotation (no real
                // unit means "raised to the zero power"); treat the whole symbol as atomic
                // instead, so registry aliases like "a0" (Bohr radius) aren't mis-split.
                if num != 0 {
                    return (name, Some((num, 1)));
                }
                break;
            }
        }
    }
    (sym.to_string(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_units() {
        let u = Parser::parse_expression("m").unwrap();
        assert_eq!(u.dimensions_map().get("m"), Some(&(1, 1)));

        let u2 = Parser::parse_expression("kg * m / s^2").unwrap();
        let dims = u2.dimensions_map();
        assert_eq!(dims.get("kg"), Some(&(1, 1)));
        assert_eq!(dims.get("m"), Some(&(1, 1)));
        assert_eq!(dims.get("s"), Some(&(-2, 1)));
    }

    #[test]
    fn test_parse_embedded_and_superscripts() {
        let u = Parser::parse_expression("m² s⁻¹").unwrap();
        let dims = u.dimensions_map();
        assert_eq!(dims.get("m"), Some(&(2, 1)));
        assert_eq!(dims.get("s"), Some(&(-1, 1)));

        let u2 = Parser::parse_expression("m2 / s2").unwrap();
        let dims2 = u2.dimensions_map();
        assert_eq!(dims2.get("m"), Some(&(2, 1)));
        assert_eq!(dims2.get("s"), Some(&(-2, 1)));
    }

    #[test]
    fn test_parse_parens() {
        let u = Parser::parse_expression("(m / s)^2").unwrap();
        let dims = u.dimensions_map();
        assert_eq!(dims.get("m"), Some(&(2, 1)));
        assert_eq!(dims.get("s"), Some(&(-2, 1)));
    }

    #[test]
    fn test_no_split_on_zero_exponent() {
        let u = Parser::parse_expression("a0").unwrap();
        let dims = u.dimensions_map();
        assert_eq!(dims.get("a0"), Some(&(1, 1)));
        assert_eq!(dims.len(), 1);

        let u2 = Parser::parse_expression("tau0").unwrap();
        let dims2 = u2.dimensions_map();
        assert_eq!(dims2.get("tau0"), Some(&(1, 1)));
        assert_eq!(dims2.len(), 1);

        // Compound expressions must not silently drop the atomic symbol.
        let u3 = Parser::parse_expression("a0/s").unwrap();
        let dims3 = u3.dimensions_map();
        assert_eq!(dims3.get("a0"), Some(&(1, 1)));
        assert_eq!(dims3.get("s"), Some(&(-1, 1)));

        let u4 = Parser::parse_expression("kg*a0").unwrap();
        let dims4 = u4.dimensions_map();
        assert_eq!(dims4.get("kg"), Some(&(1, 1)));
        assert_eq!(dims4.get("a0"), Some(&(1, 1)));
    }
}
