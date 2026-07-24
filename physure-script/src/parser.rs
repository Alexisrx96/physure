use pest::Parser;
use pest_derive::Parser;
use physure_core::error::{PhysureError, PhysureResult};
use physure_core::UnitRegistry;
use std::sync::OnceLock;
use crate::ast::*;

#[derive(Parser)]
#[grammar = "phs.pest"]
pub struct PhsParser;

pub fn parse_phs(code: &str) -> PhysureResult<Program> {
    let pairs = PhsParser::parse(Rule::program, code)
        .map_err(|e| PhysureError::Generic(format!("Parse error: {}", e)))?;
    
    let mut statements = Vec::new();
    for pair in pairs {
        if pair.as_rule() == Rule::stmt {
            let inner = pair.into_inner().next().unwrap();
            statements.push(parse_statement(inner)?);
        }
    }
    
    Ok(Program { statements })
}

pub fn parse_phs_with_lines(code: &str) -> PhysureResult<Vec<(usize, Statement)>> {
    let pairs = PhsParser::parse(Rule::program, code)
        .map_err(|e| PhysureError::Generic(format!("Parse error: {}", e)))?;

    let mut statements = Vec::new();
    for pair in pairs {
        if pair.as_rule() == Rule::stmt {
            let line = pair.line_col().0 - 1;
            let inner = pair.into_inner().next().unwrap();
            statements.push((line, parse_statement(inner)?));
        }
    }

    Ok(statements)
}

fn parse_statement(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Statement> {
    match pair.as_rule() {
        Rule::stmt => parse_statement(pair.into_inner().next().unwrap()),
        Rule::import_stmt => parse_import(pair),
        Rule::export_stmt => parse_export(pair),
        Rule::function_def | Rule::assignment_fn => parse_function_def(pair),
        Rule::assignment => parse_assignment(pair),
        Rule::raw_block => Ok(Statement::Expr(Expr::Identifier(pair.as_str().to_string()))),
        Rule::expr => Ok(Statement::Expr(parse_expr(pair)?)),
        _ => Err(PhysureError::Generic(format!("Unexpected statement rule: {:?}", pair.as_rule()))),
    }
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
                let symbols_str = inner.as_str().trim();
                if symbols_str == "*" {
                    specifier = ImportSpecifier::Wildcard;
                } else {
                    let mut symbols = Vec::new();
                    for sym_pair in inner.into_inner() {
                        if sym_pair.as_rule() == Rule::import_symbol_item {
                            let mut name = String::new();
                            let mut sym_alias = None;
                            for p in sym_pair.into_inner() {
                                if name.is_empty() {
                                    name = p.as_str().to_string();
                                } else {
                                    sym_alias = Some(p.as_str().to_string());
                                }
                            }
                            symbols.push(ImportSymbol { name, alias: sym_alias });
                        }
                    }
                    specifier = ImportSpecifier::Symbols(symbols);
                }
            }
            Rule::string_lit => {
                path = inner.as_str().trim_matches('"').to_string();
            }
            Rule::identifier => {
                if is_use && path.is_empty() {
                    path = inner.as_str().to_string();
                } else if !is_use && path.is_empty() {
                    path = inner.as_str().to_string();
                } else {
                    alias = Some(inner.as_str().to_string());
                }
            }
            _ => {}
        }
    }
    
    if !is_use {
        if let Some(a) = alias {
            specifier = ImportSpecifier::ModuleAlias(a);
        } else {
            specifier = ImportSpecifier::Wildcard; // default for `import "path"`
        }
    }
    
    Ok(Statement::Import(ImportNode { path, specifier }))
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
    let mut name = String::new();
    let mut params = Vec::new();
    let mut param_units = Vec::new();
    let mut body_stmts = Vec::new();

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::identifier => {
                name = inner.as_str().to_string();
            }
            Rule::params => {
                for p in inner.into_inner() {
                    if p.as_rule() == Rule::param_item {
                        let mut param_inner = p.into_inner();
                        let id_str = param_inner.next().unwrap().as_str().to_string();
                        let unit_str = param_inner.next().map(|u| u.as_str().trim().to_string());
                        params.push(id_str);
                        param_units.push(unit_str);
                    } else {
                        params.push(p.as_str().to_string());
                        param_units.push(None);
                    }
                }
            }
            Rule::expr => {
                body_stmts.push(Statement::Expr(parse_expr(inner)?));
            }
            Rule::block_body => {
                for stmt_pair in inner.into_inner() {
                    if stmt_pair.as_rule() == Rule::stmt {
                        let inner_stmt = stmt_pair.into_inner().next().unwrap();
                        body_stmts.push(parse_statement(inner_stmt)?);
                    } else if stmt_pair.as_rule() != Rule::_nl_indent {
                        body_stmts.push(parse_statement(stmt_pair)?);
                    }
                }
            }
            _ => {}
        }
    }

    Ok(Statement::FunctionDef(FunctionDefNode {
        name,
        params,
        param_units,
        body_stmts,
    }))
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
    }))
}

fn parse_expr(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut inner = pair.into_inner();
    let first = match inner.next() {
        Some(f) => f,
        None => return Ok(Expr::Quantity(QuantityNode { magnitude: 0.0, uncertainty: None, is_sigma: false, unit: None })),
    };
    let left = if first.as_rule() == Rule::base_expr {
        parse_base_expr(first)?
    } else {
        parse_comp_expr(first)?
    };
    
    if let Some(then_pair) = inner.next() {
        let else_pair = inner.next().unwrap();
        let then_expr = parse_base_expr(then_pair)?;
        let else_expr = parse_base_expr(else_pair)?;
        Ok(Expr::FunctionCall {
            name: "ternary".to_string(),
            args: vec![left, then_expr, else_expr],
        })
    } else {
        Ok(left)
    }
}

fn parse_base_expr(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut inner = pair.into_inner();
    let first = inner.next().unwrap();
    let mut left = parse_comp_expr(first)?;
    
    while let Some(op_pair) = inner.next() {
        let op = match op_pair.as_rule() {
            Rule::op_add => BinaryOp::Add,
            Rule::op_sub => BinaryOp::Sub,
            Rule::op_convert => BinaryOp::Convert,
            _ => return Err(PhysureError::Generic(format!("Unexpected op in base_expr: {:?}", op_pair.as_rule()))),
        };
        let right_pair = inner.next().unwrap();
        let right = parse_comp_expr(right_pair)?;
        left = Expr::BinaryOp {
            op,
            left: Box::new(left),
            right: Box::new(right),
        };
    }
    
    Ok(left)
}

fn parse_comp_expr(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut inner = pair.into_inner();
    let first = inner.next().unwrap();
    let mut left = parse_term(first)?;
    
    while let Some(op_pair) = inner.next() {
        if op_pair.as_rule() == Rule::op_format {
            let spec = op_pair.into_inner().next().map(|p| p.as_str().to_string()).unwrap_or_default();
            left = Expr::FunctionCall {
                name: "format".to_string(),
                args: vec![left, Expr::Identifier(spec)],
            };
        } else if op_pair.as_rule() == Rule::op_compare {
            let right_pair = inner.next().unwrap();
            let right = parse_term(right_pair)?;
            let cmp_op = op_pair.as_str().to_string();
            left = Expr::FunctionCall {
                name: format!("op_{}", cmp_op),
                args: vec![left, right],
            };
        } else {
            let right = parse_term(op_pair)?;
            left = Expr::BinaryOp {
                op: BinaryOp::Mul,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
    }
    
    Ok(left)
}

fn parse_term(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut inner = pair.into_inner();
    let first = inner.next().unwrap(); // factor
    let mut left = parse_factor(first)?;
    
    while let Some(next_pair) = inner.next() {
        match next_pair.as_rule() {
            Rule::op_mul => {
                let right_pair = inner.next().unwrap();
                let right = parse_factor(right_pair)?;
                left = Expr::BinaryOp {
                    op: BinaryOp::Mul,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            }
            Rule::op_div => {
                let right_pair = inner.next().unwrap();
                let right = parse_factor(right_pair)?;
                left = Expr::BinaryOp {
                    op: BinaryOp::Div,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            }
            Rule::factor => {
                // implicit multiplication
                let right = parse_factor(next_pair)?;
                left = Expr::BinaryOp {
                    op: BinaryOp::Mul,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            }
            _ => return Err(PhysureError::Generic(format!("Unexpected rule in term: {:?}", next_pair.as_rule()))),
        }
    }
    Ok(left)
}

fn parse_factor(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut inner = pair.into_inner();
    let primary_pair = inner.next().unwrap();
    
    let left = match primary_pair.as_rule() {
        Rule::quantity => parse_quantity(primary_pair)?,
        Rule::number => parse_number_quantity(primary_pair)?,
        Rule::function_call => parse_function_call(primary_pair)?,
        Rule::identifier => Expr::Identifier(primary_pair.as_str().to_string()),
        Rule::string_lit => Expr::Identifier(primary_pair.as_str().trim_matches('"').to_string()),
        Rule::if_expr => parse_if_expr(primary_pair)?,
        Rule::vector_literal => parse_vector_literal(primary_pair)?,
        Rule::expr => parse_expr(primary_pair)?,
        Rule::base_expr => parse_base_expr(primary_pair)?,
        Rule::comp_expr => parse_comp_expr(primary_pair)?,
        _ => return Err(PhysureError::Generic(format!("Unexpected rule in factor: {:?}", primary_pair.as_rule()))),
    };
    
    if let Some(op_pair) = inner.next() {
        if op_pair.as_rule() == Rule::op_pow {
            let right_pair = inner.next().unwrap();
            let right = parse_factor(right_pair)?;
            return Ok(Expr::BinaryOp {
                op: BinaryOp::Pow,
                left: Box::new(left),
                right: Box::new(right),
            });
        }
    }
    
    Ok(left)
}

fn parse_if_expr(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut inner = pair.into_inner();
    let cond = parse_expr(inner.next().unwrap())?;
    let then_e = parse_expr(inner.next().unwrap())?;
    let else_e = parse_expr(inner.next().unwrap())?;
    Ok(Expr::FunctionCall {
        name: "if_then_else".to_string(),
        args: vec![cond, then_e, else_e],
    })
}

fn parse_vector_literal(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut elems = Vec::new();
    let mut unit_str = None;
    
    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::unit_expr => {
                unit_str = Some(inner.as_str().to_string());
            }
            Rule::expr => {
                elems.push(parse_expr(inner)?);
            }
            _ => {}
        }
    }
    
    let vec_expr = Expr::FunctionCall {
        name: "vector".to_string(),
        args: elems,
    };
    
    if let Some(u) = unit_str {
        Ok(Expr::BinaryOp {
            op: BinaryOp::Mul,
            left: Box::new(vec_expr),
            right: Box::new(Expr::Identifier(u)),
        })
    } else {
        Ok(vec_expr)
    }
}

fn parse_number_quantity(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mag = pair.as_str().trim().parse::<f64>().map_err(|_| PhysureError::Generic("Invalid number".to_string()))?;
    Ok(Expr::Quantity(QuantityNode {
        magnitude: mag,
        uncertainty: None,
        is_sigma: false,
        unit: None,
    }))
}

fn parse_function_call(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut name = String::new();
    let mut args = Vec::new();
    
    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::identifier => {
                name = inner.as_str().to_string();
            }
            Rule::expr => {
                args.push(parse_expr(inner)?);
            }
            _ => {}
        }
    }
    
    Ok(Expr::FunctionCall { name, args })
}

/// Cached default-SI registry used to decide whether a `unit_expr` token names a real unit.
/// Must match the registry the interpreter resolves quantity units against
/// (`physure_core::UnitRegistry::build_default_si`, see `interpreter.rs`) so that a token
/// accepted here is guaranteed to resolve the same way at evaluation time.
fn unit_registry() -> &'static UnitRegistry {
    static REGISTRY: OnceLock<UnitRegistry> = OnceLock::new();
    REGISTRY.get_or_init(UnitRegistry::build_default_si)
}

/// The alphabetic symbol a `unit_term` pair (e.g. "r ^ 2") actually looks up in the
/// registry — its optional exponent suffix is irrelevant to whether it names a real unit.
fn unit_term_base_name(text: &str) -> String {
    text.trim_start().chars().take_while(|c| c.is_ascii_alphabetic()).collect()
}

/// True if every `unit_term` inside `pair` (recursing through parenthesized groups) names
/// a registered unit. Used to decide whether a group like `(kg * s^2)` is consumed
/// wholesale as part of a quantity's unit.
fn unit_expr_all_valid(pair: pest::iterators::Pair<Rule>, registry: &UnitRegistry) -> bool {
    pair.into_inner().all(|child| match child.as_rule() {
        Rule::unit_term => registry.get_unit(&unit_term_base_name(child.as_str())).is_some(),
        Rule::unit_expr => unit_expr_all_valid(child, registry),
        _ => false,
    })
}

/// Splits a `unit_expr` pair into the longest valid leading run of registered units
/// (returned as a unit string) plus, if that run doesn't consume the whole match, the
/// operator connecting it to the raw source text of everything after it.
///
/// `unit_expr` is purely syntactic — any `ASCII_ALPHA+` chain joined by `*`/`/`/whitespace
/// parses as one, so pest alone can't tell a real unit symbol from a variable reference.
/// Concretely, `1.602e-19 C / r ^ 2` on one line (no statement boundary before `/ r`) would
/// otherwise have its `unit_expr` greedily swallow `C / r ^ 2` as the literal's unit even
/// though `r` is a bound function parameter, fabricating a bogus `r` dimension and silently
/// dropping the real division at evaluation time. Validating against the registry here lets
/// the leftover be handed back to the expression grammar as a normal variable reference.
fn split_unit_expr(pair: pest::iterators::Pair<Rule>, registry: &UnitRegistry) -> (Option<String>, Option<(BinaryOp, String)>) {
    let base_str = pair.as_str();
    let base_start = pair.as_span().start();
    let mut consumed_end_rel = 0usize;

    for child in pair.into_inner() {
        let span = child.as_span();
        let valid = match child.as_rule() {
            Rule::unit_term => registry.get_unit(&unit_term_base_name(child.as_str())).is_some(),
            Rule::unit_expr => unit_expr_all_valid(child, registry),
            _ => false,
        };
        if valid {
            consumed_end_rel = span.end() - base_start;
            continue;
        }

        let child_start_rel = span.start() - base_start;
        let gap = &base_str[consumed_end_rel..child_start_rel];
        let op = if gap.contains('/') { BinaryOp::Div } else { BinaryOp::Mul };
        // Start the remainder right after the operator (if any) rather than at the
        // invalid child's own span start, so a parenthesized group's "(" — which lies in
        // `gap`, before the child's span — stays attached to the remainder text instead
        // of being dropped (e.g. "kg / (r^2)" must give back "(r^2)", not "r^2)").
        let op_offset = gap.find(|c| c == '*' || c == '/').map(|i| i + 1).unwrap_or(0);
        let remainder = base_str[consumed_end_rel + op_offset..].trim().to_string();
        let unit = if consumed_end_rel == 0 {
            None
        } else {
            Some(base_str[..consumed_end_rel].trim().to_string())
        };
        return (unit, Some((op, remainder)));
    }

    (Some(base_str.trim().to_string()), None)
}

/// Parses text left over after `split_unit_expr` stops consuming a unit chain (e.g.
/// "r ^ 2" or "(r^2)") as a normal expression, so it participates in the AST as a variable
/// reference / arithmetic instead of a fabricated unit dimension.
fn parse_unit_leftover(text: &str) -> PhysureResult<Expr> {
    let mut pairs = PhsParser::parse(Rule::expr, text)
        .map_err(|e| PhysureError::Generic(format!("Parse error in quantity unit remainder '{}': {}", text, e)))?;
    let pair = pairs
        .next()
        .ok_or_else(|| PhysureError::Generic(format!("Empty quantity unit remainder '{}'", text)))?;
    parse_expr(pair)
}

fn parse_quantity(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut magnitude = None;
    let mut magnitude_expr = None;
    let mut uncertainty = None;
    let mut unit = None;
    let mut unit_leftover: Option<(BinaryOp, String)> = None;

    let mut is_sigma = false;

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::number => {
                magnitude = Some(inner.as_str().parse::<f64>().map_err(|_| PhysureError::Generic("Invalid number".to_string()))?);
            }
            Rule::expr => {
                magnitude_expr = Some(parse_expr(inner)?);
            }
            Rule::uncertainty => {
                for u_inner in inner.into_inner() {
                    if u_inner.as_rule() == Rule::uncertainty_val {
                        let mut val_str = u_inner.as_str().trim().to_string();
                        let is_percent = val_str.ends_with('%');
                        if is_percent {
                            val_str.pop();
                        }
                        if val_str.contains("sigma") || val_str.contains("σ") {
                            is_sigma = true;
                            val_str = val_str.replace("sigma", "").replace("σ", "");
                        }
                        let val = val_str.trim().parse::<f64>().map_err(|_| PhysureError::Generic("Invalid uncertainty".to_string()))?;
                        uncertainty = Some(val);
                    }
                }
            }
            Rule::unit_expr => {
                let (u, leftover) = split_unit_expr(inner, unit_registry());
                unit = u;
                unit_leftover = leftover;
            }
            _ => {}
        }
    }

    let quantity_expr = if let Some(mag) = magnitude {
        Expr::Quantity(QuantityNode {
            magnitude: mag,
            uncertainty,
            is_sigma,
            unit,
        })
    } else if let Some(mag_expr) = magnitude_expr {
        if let Some(u) = unit {
            if let Expr::Quantity(mut q) = mag_expr {
                q.unit = Some(u);
                q.is_sigma = is_sigma;
                q.uncertainty = uncertainty.or(q.uncertainty);
                Expr::Quantity(q)
            } else {
                Expr::BinaryOp {
                    op: BinaryOp::Mul,
                    left: Box::new(mag_expr),
                    right: Box::new(Expr::Identifier(u)),
                }
            }
        } else {
            mag_expr
        }
    } else {
        return Err(PhysureError::Generic("Missing magnitude in quantity".to_string()));
    };

    match unit_leftover {
        Some((op, remainder)) => {
            let remainder_expr = parse_unit_leftover(&remainder)?;
            Ok(Expr::BinaryOp {
                op,
                left: Box::new(quantity_expr),
                right: Box::new(remainder_expr),
            })
        }
        None => Ok(quantity_expr),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_explicit_imports() {
        let code1 = "use g, c as speed_of_light from \"physics/constants\"";
        let prog1 = parse_phs(code1).unwrap();
        assert_eq!(prog1.statements.len(), 1);
        if let Statement::Import(imp) = &prog1.statements[0] {
            assert_eq!(imp.path, "physics/constants");
            if let ImportSpecifier::Symbols(syms) = &imp.specifier {
                assert_eq!(syms[0].name, "g");
                assert_eq!(syms[1].name, "c");
                assert_eq!(syms[1].alias.as_deref(), Some("speed_of_light"));
            } else { panic!("expected symbols"); }
        } else { panic!("expected import"); }

        let code2 = "use * from \"physics/thermodynamics\"";
        let prog2 = parse_phs(code2).unwrap();
        if let Statement::Import(imp) = &prog2.statements[0] {
            assert_eq!(imp.path, "physics/thermodynamics");
            assert!(matches!(imp.specifier, ImportSpecifier::Wildcard));
        } else { panic!("expected import"); }

        let code3 = "import \"physics/constants\" as consts";
        let prog3 = parse_phs(code3).unwrap();
        if let Statement::Import(imp) = &prog3.statements[0] {
            assert_eq!(imp.path, "physics/constants");
            if let ImportSpecifier::ModuleAlias(alias) = &imp.specifier {
                assert_eq!(alias, "consts");
            } else { panic!("expected module alias"); }
        } else { panic!("expected import"); }
    }

    #[test]
    fn test_natural_function_definitions() {
        let code = "fn kinetic_energy(m, v) = 1/2 m v^2";
        let prog = parse_phs(code).unwrap();
        if let Statement::FunctionDef(f) = &prog.statements[0] {
            assert_eq!(f.name, "kinetic_energy");
            assert_eq!(f.params, vec!["m", "v"]);
            assert_eq!(f.param_units, vec![None, None]);
            // 1/2 m v^2
        } else { panic!("expected func def"); }
    }

    #[test]
    fn test_function_def_param_unit_annotation() {
        let code = "E_campo(r: m) =\n    r\n";
        let prog = parse_phs(code).unwrap();
        if let Statement::FunctionDef(f) = &prog.statements[0] {
            assert_eq!(f.name, "E_campo");
            assert_eq!(f.params, vec!["r"]);
            assert_eq!(f.param_units, vec![Some("m".to_string())]);
        } else { panic!("expected func def"); }
    }

    #[test]
    fn test_quantity_literals() {
        let code = "m = 75.0 ± 0.5 kg";
        let prog = parse_phs(code).unwrap();
        if let Statement::Assignment(a) = &prog.statements[0] {
            assert_eq!(a.name, "m");
            if let Expr::Quantity(q) = &a.value {
                assert_eq!(q.magnitude, 75.0);
                assert_eq!(q.uncertainty, Some(0.5));
                assert_eq!(q.unit.as_deref(), Some("kg"));
            } else { panic!("expected quantity"); }
        } else { panic!("expected assignment"); }

        let code = "m = 75.0 +/- 0.5 kg";
        let prog = parse_phs(code).unwrap();
        if let Statement::Assignment(a) = &prog.statements[0] {
            if let Expr::Quantity(q) = &a.value {
                assert_eq!(q.magnitude, 75.0);
                assert_eq!(q.uncertainty, Some(0.5));
                assert_eq!(q.unit.as_deref(), Some("kg"));
            }
        }

        let code = "v = 10 m/s";
        let prog = parse_phs(code).unwrap();
        if let Statement::Assignment(a) = &prog.statements[0] {
            if let Expr::Quantity(q) = &a.value {
                assert_eq!(q.magnitude, 10.0);
                assert_eq!(q.uncertainty, None);
                assert_eq!(q.unit.as_deref(), Some("m/s"));
            }
        }
    }

    #[test]
    fn test_exports() {
        let code = "export E as \"kinetic_energy\"";
        let prog = parse_phs(code).unwrap();
        if let Statement::Export(e) = &prog.statements[0] {
            assert_eq!(e.symbol, "E");
            assert_eq!(e.export_name, "kinetic_energy");
        } else { panic!("expected export"); }
    }

    #[test]
    fn test_assignment_fn_standalone() {
        let code = "f(v: m / s) =\n    resta = 1 m / s\n    v * 2 - resta";
        let pairs = PhsParser::parse(Rule::assignment_fn, code);
        assert!(pairs.is_ok());
    }

    #[test]
    fn test_parse_1_cargas() {
        if let Ok(code) = std::fs::read_to_string("D:/Projects/test_physure/1_cargas.phs") {
            let res = parse_phs(&code);
            assert!(res.is_ok());
        }
    }
}
