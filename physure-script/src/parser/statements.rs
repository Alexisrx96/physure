use physure_core::error::{PhysureError, PhysureResult};
use crate::ast::*;
use super::Rule;
use super::expressions::parse_expr;

pub(crate) fn parse_statement(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Statement> {
    match pair.as_rule() {
        Rule::stmt => parse_statement(pair.into_inner().next().unwrap()),
        Rule::import_stmt => parse_import(pair),
        Rule::export_stmt => parse_export(pair),
        Rule::documented_stmt => parse_documented_stmt(pair),
        Rule::decorated_stmt => parse_decorated_stmt(pair),
        Rule::function_def | Rule::assignment_fn => parse_function_def(pair),
        Rule::assignment => parse_assignment(pair),
        Rule::guard_if_stmt => parse_guard_if_stmt(pair),
        Rule::return_stmt => parse_return_stmt(pair),
        Rule::while_stmt => parse_while_stmt(pair),
        Rule::raw_block => Ok(Statement::Expr(Expr::Identifier(pair.as_str().to_string()))),
        Rule::expr => Ok(Statement::Expr(parse_expr(pair)?)),
        _ => Err(PhysureError::Generic(format!("Unexpected statement rule: {:?}", pair.as_rule()))),
    }
}

fn parse_while_stmt(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Statement> {
    let mut inner = pair.into_inner();
    let first = inner.next().unwrap();
    let cond_pair = if first.as_rule() == Rule::_while_kw {
        inner.next().unwrap()
    } else {
        first
    };
    let cond = parse_expr(cond_pair)?;
    let mut body = Vec::new();
    let mut body_lines = Vec::new();
    for stmt_pair in inner {
        if stmt_pair.as_rule() == Rule::stmt {
            body_lines.push(stmt_pair.line_col().0);
            body.push(parse_statement(stmt_pair)?);
        }
    }
    Ok(Statement::While { cond, body, body_lines })
}


fn parse_import(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Statement> {
    let mut path = String::new();
    let mut specifier = ImportSpecifier::Wildcard;
    let mut is_use = false;
    let mut alias = None;

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::import_symbols => {
                is_use = true;
                specifier = parse_import_symbols(inner);
            }
            Rule::string_lit => {
                path = inner.as_str().trim_matches('"').to_string();
            }
            Rule::identifier if path.is_empty() => {
                path = inner.as_str().to_string();
            }
            Rule::identifier => {
                alias = Some(inner.as_str().to_string());
            }
            _ => {}
        }
    }

    if !is_use {
        // `import "path"` brings in everything, under the alias if it is given one
        specifier = alias.map_or(ImportSpecifier::Wildcard, ImportSpecifier::ModuleAlias);
    }

    Ok(Statement::Import(ImportNode { path, specifier }))
}

/// `*`, or the named symbols of a `use`, each with the alias it is imported under if it has one.
fn parse_import_symbols(pair: pest::iterators::Pair<Rule>) -> ImportSpecifier {
    if pair.as_str().trim() == "*" {
        return ImportSpecifier::Wildcard;
    }
    let symbols = pair
        .into_inner()
        .filter(|sym_pair| sym_pair.as_rule() == Rule::import_symbol_item)
        .map(|sym_pair| {
            let mut names = sym_pair.into_inner().map(|p| p.as_str().to_string());
            let name = names.next().unwrap_or_default();
            ImportSymbol { name, alias: names.last() }
        })
        .collect();
    ImportSpecifier::Symbols(symbols)
}

fn parse_export(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Statement> {
    let mut symbol = String::new();
    let mut export_name = String::new();
    
    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::identifier if symbol.is_empty() => {
                symbol = inner.as_str().to_string();
                export_name = symbol.clone();
            }
            Rule::identifier | Rule::string_lit => {
                export_name = inner.as_str().trim_matches('"').to_string();
            }
            _ => {}
        }
    }
    
    Ok(Statement::Export(ExportNode { symbol, export_name }))
}

fn parse_function_def(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Statement> {
    // Captured before `pair.into_inner()` consumes `pair` by value below -- this is the whole
    // `fn ... = ...` construct's own starting line, used for the single-expression-body case
    // (`fn f(x) = x^2`), which has exactly one body statement and no `stmt`-level pair of its
    // own to read a line from.
    let def_line = pair.line_col().0;
    let mut name = String::new();
    let mut params = Vec::new();
    let mut param_units = Vec::new();
    let mut body_stmts = Vec::new();
    let mut body_lines = Vec::new();

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::identifier => {
                name = inner.as_str().to_string();
            }
            Rule::params => {
                (params, param_units) = parse_params(inner);
            }
            Rule::expr => {
                body_lines.push(def_line);
                body_stmts.push(Statement::Expr(parse_expr(inner)?));
            }
            Rule::block_body => {
                parse_block_body(inner, &mut body_stmts, &mut body_lines)?;
            }
            _ => {}
        }
    }

    Ok(Statement::FunctionDef(FunctionDefNode {
        name,
        params,
        param_units,
        body_stmts,
        body_lines,
        decorators: Vec::new(),
        doc: None,
    }))
}

/// The parameter names of a `fn`, and the unit each one is declared in (`None` when it has none).
fn parse_params(pair: pest::iterators::Pair<Rule>) -> (Vec<String>, Vec<Option<String>>) {
    let mut params = Vec::new();
    let mut param_units = Vec::new();
    for p in pair.into_inner() {
        if p.as_rule() == Rule::param_item {
            let mut param_inner = p.into_inner();
            params.push(param_inner.next().unwrap().as_str().to_string());
            param_units.push(param_inner.next().map(|u| u.as_str().trim().to_string()));
        } else {
            params.push(p.as_str().to_string());
            param_units.push(None);
        }
    }
    (params, param_units)
}

/// The statements of an indented `fn` body, each with the line it starts on.
fn parse_block_body(pair: pest::iterators::Pair<Rule>, body_stmts: &mut Vec<Statement>, body_lines: &mut Vec<usize>) -> PhysureResult<()> {
    for stmt_pair in pair.into_inner() {
        if stmt_pair.as_rule() == Rule::stmt {
            body_lines.push(stmt_pair.line_col().0);
            let inner_stmt = stmt_pair.into_inner().next().unwrap();
            body_stmts.push(parse_statement(inner_stmt)?);
        } else if stmt_pair.as_rule() != Rule::_nl_indent {
            body_lines.push(stmt_pair.line_col().0);
            body_stmts.push(parse_statement(stmt_pair)?);
        }
    }
    Ok(())
}

fn parse_assignment(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Statement> {
    let mut name = String::new();
    let mut value = None;
    
    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::identifier => {
                name = inner.as_str().to_string();
            }
            Rule::expr => {
                value = Some(parse_expr(inner)?);
            }
            _ => {}
        }
    }
    
    Ok(Statement::Assignment(AssignmentNode {
        name,
        value: value.unwrap(),
        decorators: Vec::new(),
    }))
}

fn parse_decorated_stmt(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Statement> {
    let mut decorators = Vec::new();
    let mut target = None;

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::decorator => {
                let raw = parse_decorator(inner)?;
                for lowered in crate::decorators::lower_range(raw)? {
                    decorators.push(lowered);
                }
            }
            Rule::function_def | Rule::assignment_fn => {
                target = Some(parse_function_def(inner)?);
            }
            Rule::assignment => {
                target = Some(parse_assignment(inner)?);
            }
            _ => {}
        }
    }

    let mut stmt = target.ok_or_else(|| {
        PhysureError::Generic("decorated statement is missing its function or assignment".to_string())
    })?;
    match &mut stmt {
        Statement::FunctionDef(node) => node.decorators = decorators,
        Statement::Assignment(node) => node.decorators = decorators,
        _ => unreachable!("decorated_stmt only ever wraps function_def, assignment_fn, or assignment"),
    }
    Ok(stmt)
}

/// Collects consecutive `doc_comment` pairs (stripping the `///` prefix and one leading space
/// per line, joining with `\n`), parses the wrapped target via the existing parse functions, and
/// attaches the joined text to `FunctionDefNode.doc`. A doc comment stacked on a bare
/// `Statement::Assignment` parses without error but the text is dropped — `AssignmentNode` has no
/// `doc` field (§2.4 of the Track E design spec: doc comments document functions, not constants).
fn parse_documented_stmt(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Statement> {
    let mut doc_lines = Vec::new();
    let mut target = None;

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::doc_comment => {
                let text = inner.as_str().trim_start_matches("///");
                let text = text.strip_prefix(' ').unwrap_or(text);
                doc_lines.push(text.to_string());
            }
            Rule::decorated_stmt => target = Some(parse_decorated_stmt(inner)?),
            Rule::function_def | Rule::assignment_fn => target = Some(parse_function_def(inner)?),
            Rule::assignment => target = Some(parse_assignment(inner)?),
            _ => {}
        }
    }

    let mut stmt = target.ok_or_else(|| {
        PhysureError::Generic("documented statement is missing its function or assignment".to_string())
    })?;
    if let Statement::FunctionDef(node) = &mut stmt {
        node.doc = Some(doc_lines.join("\n"));
    }
    Ok(stmt)
}

fn parse_decorator(pair: pest::iterators::Pair<Rule>) -> PhysureResult<DecoratorNode> {
    let mut inner = pair.into_inner();
    let name = inner.next().unwrap().as_str().to_string();
    let mut args = Vec::new();
    for arg_pair in inner {
        args.push(parse_expr(arg_pair)?);
    }
    Ok(DecoratorNode { name, args })
}

fn parse_guard_if_stmt(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Statement> {
    let mut inner = pair.into_inner();
    let cond = parse_expr(inner.next().unwrap())?;
    let return_pair = inner.next().unwrap();
    let value = parse_expr(return_pair.into_inner().next().unwrap())?;
    Ok(Statement::GuardReturn { cond, value })
}

fn parse_return_stmt(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Statement> {
    Ok(Statement::Return(parse_expr(pair.into_inner().next().unwrap())?))
}

