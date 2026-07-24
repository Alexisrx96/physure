use pest::Parser;
use pest_derive::Parser;
use physure_core::error::{PhysureError, PhysureResult};
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
    let mut body_stmts = Vec::new();
    
    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::identifier => {
                name = inner.as_str().to_string();
            }
            Rule::params => {
                for p in inner.into_inner() {
                    let id_str = if p.as_rule() == Rule::param_item {
                        p.into_inner().next().unwrap().as_str().to_string()
                    } else {
                        p.as_str().to_string()
                    };
                    params.push(id_str);
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

fn parse_quantity(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut magnitude = None;
    let mut magnitude_expr = None;
    let mut uncertainty = None;
    let mut unit = None;
    
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
                unit = Some(inner.as_str().trim().to_string());
            }
            _ => {}
        }
    }
    
    if let Some(mag) = magnitude {
        Ok(Expr::Quantity(QuantityNode {
            magnitude: mag,
            uncertainty,
            is_sigma,
            unit,
        }))
    } else if let Some(mag_expr) = magnitude_expr {
        if let Some(u) = unit {
            if let Expr::Quantity(mut q) = mag_expr {
                q.unit = Some(u);
                q.is_sigma = is_sigma;
                q.uncertainty = uncertainty.or(q.uncertainty);
                Ok(Expr::Quantity(q))
            } else {
                Ok(Expr::BinaryOp {
                    op: BinaryOp::Mul,
                    left: Box::new(mag_expr),
                    right: Box::new(Expr::Identifier(u)),
                })
            }
        } else {
            Ok(mag_expr)
        }
    } else {
        Err(PhysureError::Generic("Missing magnitude in quantity".to_string()))
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
            // 1/2 m v^2
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
