pub(crate) mod shadowing;
pub(crate) mod quantities;
pub(crate) use quantities::is_known_unit_symbol;
pub(crate) mod expressions;
pub(crate) mod statements;
use pest::Parser;
use pest_derive::Parser;
use physure_core::error::{PhysureError, PhysureResult};
use crate::ast::*;

#[derive(Parser)]
#[grammar = "phs.pest"]
pub struct PhsParser;

/// The deepest nesting of brackets the parser follows. The recursive descent spends several kilobytes of
/// stack on every level, so a few hundred nested brackets overflow it, and a stack overflow ends the
/// whole process (the JVM, the CLI and the language server die with it; no caller can catch it). 64 levels
/// are far beyond a formula written by hand.
pub const MAX_NESTING_DEPTH: usize = 64;

/// Refuses source that nests brackets deeper than `MAX_NESTING_DEPTH`, before the parser recurses into it.
/// Brackets inside a string literal or a comment are text and do not count.
pub(crate) fn check_nesting_depth(code: &str) -> PhysureResult<()> {
    let (mut depth, mut in_string, mut in_comment) = (0usize, false, false);
    let mut chars = code.chars().peekable();
    while let Some(c) = chars.next() {
        if in_comment {
            in_comment = c != '\n' && c != '\r';
        } else if in_string {
            in_string = c != '"';
        } else {
            match c {
                '"' => in_string = true,
                '#' => in_comment = true,
                '/' if chars.peek() == Some(&'/') => in_comment = true,
                '(' | '[' | '{' => {
                    depth += 1;
                    if depth > MAX_NESTING_DEPTH {
                        return Err(PhysureError::Generic(format!(
                            "Parse error: brackets are nested more than {MAX_NESTING_DEPTH} levels deep"
                        )));
                    }
                }
                ')' | ']' | '}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    Ok(())
}

pub fn parse_phs(code: &str) -> PhysureResult<Program> {
    check_nesting_depth(code)?;
    let pairs = PhsParser::parse(Rule::program, code)
        .map_err(|e| PhysureError::Generic(format!("Parse error: {}", e)))?;
    
    let mut statements = Vec::new();
    let mut lines = Vec::new();
    let mut statement_pos = Vec::new();
    for pair in pairs {
        if pair.as_rule() == Rule::stmt {
            let (line, col) = pair.line_col();
            let inner = pair.into_inner().next().unwrap();
            statements.push(statements::parse_statement(inner)?);
            lines.push(line);
            statement_pos.push((line, col));
        }
    }

    shadowing::validate_unit_shadowing(&statements, &statement_pos)?;
    crate::decorators::validate_decorators(&statements)?;
    Ok(Program { statements, lines })
}

pub fn parse_phs_with_lines(code: &str) -> PhysureResult<Vec<(usize, Statement)>> {
    check_nesting_depth(code)?;
    let pairs = PhsParser::parse(Rule::program, code)
        .map_err(|e| PhysureError::Generic(format!("Parse error: {}", e)))?;

    let mut statements = Vec::new();
    let mut statement_pos = Vec::new();
    for pair in pairs {
        if pair.as_rule() == Rule::stmt {
            let (line, col) = pair.line_col();
            let inner = pair.into_inner().next().unwrap();
            statements.push((line - 1, statements::parse_statement(inner)?));
            statement_pos.push((line, col));
        }
    }

    let stmts_only: Vec<Statement> = statements.iter().map(|(_, s)| s.clone()).collect();
    shadowing::validate_unit_shadowing(&stmts_only, &statement_pos)?;
    crate::decorators::validate_decorators(&stmts_only)?;

    Ok(statements)
}


#[cfg(test)]
mod tests;
