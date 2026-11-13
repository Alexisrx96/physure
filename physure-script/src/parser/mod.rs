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

/// The deepest nesting the parser follows: brackets, plus `if` expressions chained without brackets (`else if`,
/// `then if`, `if if`) and function bodies nested by indentation, which make the parser recurse just as brackets
/// do. The recursive descent spends several kilobytes of stack on every level, so a few hundred levels overflow
/// it, and a stack overflow ends the whole process (the JVM, the CLI and the language server die with it; no caller
/// can catch it). 64 levels are far beyond a formula written by hand.
pub const MAX_NESTING_DEPTH: usize = 64;

pub use expressions::MAX_EXPRESSION_DEPTH;

/// Refuses source that nests deeper than `MAX_NESTING_DEPTH`, before the parser recurses into it. Brackets inside a
/// string literal, a raw block or a comment are text and do not count.
///
/// The count is a bound, never less than the parser's real recursion: every bracket open, every `if` that follows
/// `else`, `then` or `if` in the same bracket (a fresh `if` starts a chain of one), and every line that ends in `=`
/// (where a function body can start) since the last line that surely starts a new statement: one at column 0,
/// outside brackets, that continues nothing (the line before it does not end where the grammar lets a newline in,
/// after `if`, `then`, `else`, `and`, `or`, `where`, `for`, `in`, `while`, `?`, `:`, `,`, `=` or `{`, and is not a
/// decorator or a `///` line; and it does not start with `then`, `else`, `and`, `or`, `in`, `:`, `,`, `=`, `{` or
/// `}`). Any other line at column 0 may still be inside the body above it (`x where` and then `a = 1` on the next
/// line), so it closes nothing.
pub(crate) fn check_nesting_depth(code: &str) -> PhysureResult<()> {
    let mut nesting = Nesting::default();
    for (token, indented) in nesting_tokens(code) {
        nesting.read(token, indented)?;
    }
    Ok(())
}

/// What the nesting count reads of the source: words, symbols, line ends. Text that is not code (a string, a raw
/// block) is one `Text` token, and a comment is nothing.
#[derive(Clone, Debug, PartialEq)]
enum NestingToken {
    Word(String),
    Symbol(char),
    Text,
    /// A `///` line, which documents the definition on the lines after it.
    Doc,
    LineEnd,
}

/// The tokens of `code`, each with whether it is the first of its line and has blanks before it.
fn nesting_tokens(code: &str) -> Vec<(NestingToken, bool)> {
    let chars: Vec<char> = code.chars().collect();
    let at = |i: usize| chars.get(i).copied();
    let (mut tokens, mut i, mut line_start, mut indented) = (Vec::new(), 0, true, false);
    let mut push = |token: NestingToken, line_start: &mut bool, indented: &mut bool| {
        let first_indented = *line_start && *indented;
        *line_start = token == NestingToken::LineEnd;
        *indented = false;
        tokens.push((token, first_indented));
    };
    while let Some(c) = at(i) {
        if c == '\n' || c == '\r' {
            i += if c == '\r' && at(i + 1) == Some('\n') { 2 } else { 1 };
            push(NestingToken::LineEnd, &mut line_start, &mut indented);
        } else if c == ' ' || c == '\t' {
            indented |= line_start;
            i += 1;
        } else if c == '#' || (c == '/' && at(i + 1) == Some('/')) {
            let doc = c == '/' && at(i + 2) == Some('/');
            while at(i).is_some_and(|c| c != '\n' && c != '\r') {
                i += 1;
            }
            if doc {
                push(NestingToken::Doc, &mut line_start, &mut indented);
            }
        } else if c == '`' && at(i + 1) == Some('`') && at(i + 2) == Some('`') {
            i += 3;
            while at(i).is_some() && !(at(i) == Some('`') && at(i + 1) == Some('`') && at(i + 2) == Some('`')) {
                i += 1;
            }
            i += 3;
            push(NestingToken::Text, &mut line_start, &mut indented);
        } else if c == '"' {
            i += 1;
            while at(i).is_some_and(|c| c != '"') {
                i += 1;
            }
            i += 1;
            push(NestingToken::Text, &mut line_start, &mut indented);
        } else if c.is_alphanumeric() || c == '_' {
            let start = i;
            while at(i).is_some_and(|c| c.is_alphanumeric() || c == '_') {
                i += 1;
            }
            push(NestingToken::Word(chars[start..i].iter().collect()), &mut line_start, &mut indented);
        } else {
            i += 1;
            push(NestingToken::Symbol(c), &mut line_start, &mut indented);
        }
    }
    tokens
}

#[derive(Default)]
struct Nesting {
    /// Brackets open, plus the `if` chains of the brackets around the current one.
    base: usize,
    /// The `if` chain of the current bracket.
    run: usize,
    outer_runs: Vec<usize>,
    /// Lines ending in `=` since the last line that surely starts a statement.
    bodies: usize,
    previous: Option<NestingToken>,
    /// The first and the last token of the current line.
    first: Option<NestingToken>,
    last: Option<NestingToken>,
    /// The last token of the line before, and whether that line was a decorator or a `///` line.
    line_before: Option<(NestingToken, bool)>,
}

fn is_word(token: &NestingToken, words: &[&str]) -> bool {
    matches!(token, NestingToken::Word(word) if words.contains(&word.as_str()))
}

fn is_symbol(token: &NestingToken, symbols: &[char]) -> bool {
    matches!(token, NestingToken::Symbol(symbol) if symbols.contains(symbol))
}

impl Nesting {
    fn refuse_past_limit(&self, what: &str) -> PhysureResult<()> {
        if self.base + self.run + self.bodies > MAX_NESTING_DEPTH {
            return Err(PhysureError::Generic(format!("Parse error: {what} nested more than {MAX_NESTING_DEPTH} levels deep")));
        }
        Ok(())
    }

    /// Whether a line at column 0 beginning with `first` may belong to the statement above it.
    fn continues(&self, first: &NestingToken) -> bool {
        let line_before = self.line_before.as_ref().is_some_and(|(last, decorated)| {
            *decorated
                || is_word(last, &["if", "then", "else", "and", "or", "where", "for", "in", "while"])
                || is_symbol(last, &['?', ':', ',', '=', '{'])
        });
        line_before || is_word(first, &["then", "else", "and", "or", "in"]) || is_symbol(first, &[':', ',', '=', '{', '}'])
    }

    fn read(&mut self, token: NestingToken, indented: bool) -> PhysureResult<()> {
        if token == NestingToken::LineEnd {
            if let (Some(first), Some(last)) = (self.first.take(), self.last.take()) {
                if is_symbol(&last, &['=']) {
                    self.bodies += 1;
                    self.refuse_past_limit("function bodies are")?;
                }
                let decorated = is_symbol(&first, &['@']) || first == NestingToken::Doc;
                self.line_before = Some((last, decorated));
            }
            return Ok(());
        }
        if self.first.is_none() {
            if !indented && self.base == 0 && !self.continues(&token) {
                self.bodies = 0;
            }
            self.first = Some(token.clone());
        }
        self.last = Some(token.clone());
        match &token {
            NestingToken::Word(word) if word == "if" => {
                let chained = self.previous.as_ref().is_some_and(|previous| is_word(previous, &["else", "then", "if"]));
                self.run = if chained { self.run + 1 } else { 1 };
                self.refuse_past_limit("`if` expressions are")?;
            }
            NestingToken::Symbol('(' | '[' | '{') => {
                self.outer_runs.push(self.run);
                self.base += self.run + 1;
                self.run = 0;
                self.refuse_past_limit("brackets are")?;
            }
            NestingToken::Symbol(')' | ']' | '}') => {
                if let Some(outer) = self.outer_runs.pop() {
                    self.base -= outer + 1;
                    self.run = outer;
                }
            }
            _ => {}
        }
        self.previous = Some(token);
        Ok(())
    }
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
