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
    let mut lexer = NestingLexer { chars: code.chars().collect(), i: 0, tokens: Vec::new(), line_start: true, indented: false };
    while let Some(c) = lexer.at(0) {
        lexer.step(c);
    }
    lexer.tokens
}

/// The reading of `nesting_tokens`: the source, where it is in it, the tokens so far, and whether no token of the
/// current line has been read yet and blanks have.
struct NestingLexer {
    chars: Vec<char>,
    i: usize,
    tokens: Vec<(NestingToken, bool)>,
    line_start: bool,
    indented: bool,
}

impl NestingLexer {
    /// The character `offset` places after the current one.
    fn at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.i + offset).copied()
    }

    fn push(&mut self, token: NestingToken) {
        let first_indented = self.line_start && self.indented;
        self.line_start = token == NestingToken::LineEnd;
        self.indented = false;
        self.tokens.push((token, first_indented));
    }

    fn skip_while(&mut self, keep: impl Fn(char) -> bool) {
        while self.at(0).is_some_and(&keep) {
            self.i += 1;
        }
    }

    fn at_raw_fence(&self) -> bool {
        self.at(0) == Some('`') && self.at(1) == Some('`') && self.at(2) == Some('`')
    }

    /// Reads what starts with the current character `c`: a line end, a blank, a comment or a token.
    fn step(&mut self, c: char) {
        match c {
            '\n' | '\r' => {
                self.i += if c == '\r' && self.at(1) == Some('\n') { 2 } else { 1 };
                self.push(NestingToken::LineEnd);
            }
            ' ' | '\t' => {
                self.indented |= self.line_start;
                self.i += 1;
            }
            '#' => self.comment(),
            '/' if self.at(1) == Some('/') => self.comment(),
            '`' if self.at_raw_fence() => self.raw_block(),
            '"' => self.string(),
            _ if c.is_alphanumeric() || c == '_' => self.word(),
            _ => {
                self.i += 1;
                self.push(NestingToken::Symbol(c));
            }
        }
    }

    /// A comment up to the end of its line: nothing, or a `Doc` for a `///` line.
    fn comment(&mut self) {
        let doc = self.at(0) == Some('/') && self.at(2) == Some('/');
        self.skip_while(|c| c != '\n' && c != '\r');
        if doc {
            self.push(NestingToken::Doc);
        }
    }

    /// A raw block from its opening fence to past its closing one (or the end of the source).
    fn raw_block(&mut self) {
        self.i += 3;
        while self.at(0).is_some() && !self.at_raw_fence() {
            self.i += 1;
        }
        self.i += 3;
        self.push(NestingToken::Text);
    }

    /// A string literal from its opening quote to past its closing one (or the end of the source).
    fn string(&mut self) {
        self.i += 1;
        self.skip_while(|c| c != '"');
        self.i += 1;
        self.push(NestingToken::Text);
    }

    fn word(&mut self) {
        let start = self.i;
        self.skip_while(|c| c.is_alphanumeric() || c == '_');
        let word = self.chars[start..self.i].iter().collect();
        self.push(NestingToken::Word(word));
    }
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
            return self.end_line();
        }
        self.place_in_line(&token, indented);
        self.count(&token)?;
        self.previous = Some(token);
        Ok(())
    }

    /// The end of a line: one that ends in `=` may start a function body, and the line is remembered as the one
    /// before the next.
    fn end_line(&mut self) -> PhysureResult<()> {
        let (Some(first), Some(last)) = (self.first.take(), self.last.take()) else {
            return Ok(());
        };
        if is_symbol(&last, &['=']) {
            self.bodies += 1;
            self.refuse_past_limit("function bodies are")?;
        }
        let decorated = is_symbol(&first, &['@']) || first == NestingToken::Doc;
        self.line_before = Some((last, decorated));
        Ok(())
    }

    /// Records `token` as the last of its line, and as the first when it is; a first token that surely starts a
    /// new statement closes every function body counted so far.
    fn place_in_line(&mut self, token: &NestingToken, indented: bool) {
        if self.first.is_none() {
            if !indented && self.base == 0 && !self.continues(token) {
                self.bodies = 0;
            }
            self.first = Some(token.clone());
        }
        self.last = Some(token.clone());
    }

    /// Counts the `if` or the bracket `token` is, refusing the source past the limit.
    fn count(&mut self, token: &NestingToken) -> PhysureResult<()> {
        match token {
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
    let mut previous_end = None;
    for pair in pairs {
        if pair.as_rule() == Rule::stmt {
            statements::refuse_unseparated(previous_end, &pair)?;
            previous_end = Some(pair.as_span().end());
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
    let mut previous_end = None;
    for pair in pairs {
        if pair.as_rule() == Rule::stmt {
            statements::refuse_unseparated(previous_end, &pair)?;
            previous_end = Some(pair.as_span().end());
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
