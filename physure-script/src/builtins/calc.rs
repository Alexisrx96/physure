use physure_core::error::{PhysureError, PhysureResult};
use crate::value::PhsValue;
use crate::interpreter::PhsInterpreter;
use super::preprocess_symbolic_expression;

pub(crate) fn clean_diff_var(v: &str) -> &str {
    let s = v.trim();
    if s.starts_with("d(") && s.ends_with(')') && s.len() > 3 {
        &s[2..s.len() - 1]
    } else if s.starts_with('d') && s.len() > 1 && s[1..].chars().all(|c| c.is_alphanumeric() || c == '_') {
        &s[1..]
    } else {
        s
    }
}

pub(crate) fn parse_leibniz_single_arg(spec: &str) -> Option<(String, String, usize)> {
    let s = spec.trim();
    if s.contains('/') {
        let parts: Vec<&str> = s.splitn(2, '/').collect();
        let num = parts[0].trim();
        let den = parts[1].trim();

        if num == "d" || num.starts_with("d/") {
            if den.contains('(') && den.ends_with(')') {
                let p1 = den.find('(')?;
                let v = &den[..p1];
                let expr = &den[p1 + 1..den.len() - 1];
                return Some((expr.to_string(), clean_diff_var(v).to_string(), 1));
            }
        }

        let den_var = if den.starts_with('d') {
            let v = &den[1..];
            if let Some(hat_idx) = v.find('^') {
                clean_diff_var(&v[..hat_idx])
            } else if v.starts_with('(') && v.ends_with(')') {
                clean_diff_var(v)
            } else if v.chars().all(|c| c.is_alphanumeric() || c == '_') {
                clean_diff_var(den)
            } else {
                clean_diff_var(v)
            }
        } else {
            den
        };

        let (order, expr) = if num.starts_with("d^") {
            let hat_idx = num.find('^').unwrap();
            let rest = &num[hat_idx + 1..];
            let end_digit = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
            let ord = rest[..end_digit].parse::<usize>().unwrap_or(1);
            let e = rest[end_digit..].trim();
            (ord, e)
        } else if num.starts_with("d2") || num.starts_with("d3") || num.starts_with("d4") {
            let ord = num[1..2].parse::<usize>().unwrap_or(1);
            (ord, num[2..].trim())
        } else if num.starts_with('d') && num.len() > 1 {
            (1, num[1..].trim())
        } else {
            (1, num)
        };

        let expr_clean = if expr.starts_with('(') && expr.ends_with(')') {
            &expr[1..expr.len() - 1]
        } else {
            expr
        };

        if !expr_clean.is_empty() && !den_var.is_empty() {
            return Some((expr_clean.to_string(), den_var.to_string(), order));
        }
    }
    None
}

pub(crate) fn parse_differential_integrand(s: &str) -> Option<(String, String)> {
    let trimmed = s.trim();
    if let Some(space_idx) = trimmed.rfind(' ') {
        let last_word = trimmed[space_idx + 1..].trim();
        if last_word.starts_with('d') && last_word.len() > 1 && last_word[1..].chars().all(|c| c.is_alphanumeric() || c == '_') {
            let expr = trimmed[..space_idx].trim();
            let var = clean_diff_var(last_word);
            if !expr.is_empty() && !var.is_empty() {
                return Some((expr.to_string(), var.to_string()));
            }
        }
    }
    None
}

pub(crate) fn eval_calc_builtin(name: &str, args: &[PhsValue], interpreter: &PhsInterpreter) -> PhysureResult<Option<PhsValue>> {
    match name {
        "deriv" | "derivative" | "diff" => {
            if args.is_empty() || args.len() > 3 {
                return Err(PhysureError::Generic("deriv expects 1, 2, or 3 arguments: deriv(expr, [var], [order])".into()));
            }
            let (expr_str, var_str, order, explicit_dep) = if args.len() == 1 {
                let spec = match &args[0] {
                    PhsValue::String(s) => s.as_str(),
                    _ => return Err(PhysureError::Generic("deriv expects expression string".into())),
                };
                if let Some((e, v, o)) = parse_leibniz_single_arg(spec) {
                    let dep = if e.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '\'') {
                        Some(e.clone())
                    } else {
                        None
                    };
                    (e, v, o, dep)
                } else {
                    return Err(PhysureError::Generic("deriv expects (expr_str, var_str) or Leibniz notation string like deriv(\"dy/dx\")".into()));
                }
            } else {
                let e = match &args[0] {
                    PhsValue::String(s) => s.to_string(),
                    _ => return Err(PhysureError::Generic("deriv expects expression string".into())),
                };
                let raw_v = match &args[1] {
                    PhsValue::String(s) => s.as_str(),
                    _ => return Err(PhysureError::Generic("deriv expects variable string".into())),
                };
                let v = clean_diff_var(raw_v).to_string();
                let o = if args.len() == 3 {
                    match &args[2] {
                        PhsValue::Number(n) => *n as usize,
                        PhsValue::Quantity(q) => q.value.mean() as usize,
                        _ => 1,
                    }
                } else {
                    1
                };
                let dep = if e.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '\'') {
                    Some(e.clone())
                } else {
                    None
                };
                (e, v, o, dep)
            };

            let inlined = preprocess_symbolic_expression(&expr_str, interpreter);
            if let Some(idx) = inlined.find('=') {
                let lhs_str = &inlined[..idx];
                let rhs_str = &inlined[idx + 1..];
                let lhs = crate::symbolic::SymbolicParser::parse_str(lhs_str)?;
                let rhs = crate::symbolic::SymbolicParser::parse_str(rhs_str)?;
                let eq_node = crate::symbolic::Node::Equation(Box::new(lhs), Box::new(rhs));
                let diff_eq = eq_node.diff_node_n(&var_str, order)?;
                return Ok(Some(PhsValue::String(diff_eq.to_string())));
            }
            let node = crate::symbolic::SymbolicParser::parse_str(&inlined)?;
            let dep_var_ref = explicit_dep.as_deref();
            let mut diff_node = node;
            for _ in 0..order {
                diff_node = diff_node.diff_node_implicit(&var_str, dep_var_ref)?.simplify();
            }
            Ok(Some(PhsValue::String(diff_node.to_string())))
        }
        "grad" | "gradient" => {
            if args.len() != 2 {
                return Err(PhysureError::Generic("grad expects (expression_string, variables_vector)".into()));
            }
            let expr_str = match &args[0] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("grad expects expression string".into())),
            };
            let vars = extract_string_list(&args[1])?;
            let inlined = preprocess_symbolic_expression(expr_str, interpreter);
            let node = crate::symbolic::SymbolicParser::parse_str(&inlined)?;
            let mut grads = Vec::new();
            for v in vars {
                let clean_v = clean_diff_var(&v);
                let d = node.diff_node(clean_v)?.simplify();
                grads.push(PhsValue::String(d.to_string()));
            }
            Ok(Some(PhsValue::Vector(grads)))
        }
        "div" | "divergence" => {
            if args.len() != 2 {
                return Err(PhysureError::Generic("div expects (vector_field, variables_vector)".into()));
            }
            let field_exprs = extract_string_list(&args[0])?;
            let vars = extract_string_list(&args[1])?;
            if field_exprs.len() != vars.len() {
                return Err(PhysureError::Generic("Vector field and variables dimension mismatch".into()));
            }
            let mut sum_nodes = Vec::new();
            for (f_str, v) in field_exprs.iter().zip(vars.iter()) {
                let clean_v = clean_diff_var(v);
                let inlined = preprocess_symbolic_expression(f_str, interpreter);
                let node = crate::symbolic::SymbolicParser::parse_str(&inlined)?;
                let d = node.diff_node(clean_v)?.simplify();
                sum_nodes.push(d);
            }
            let div_node = crate::symbolic::Node::Add(sum_nodes).simplify();
            Ok(Some(PhsValue::String(div_node.to_string())))
        }
        "curl" => {
            if args.len() != 2 {
                return Err(PhysureError::Generic("curl expects (3D_vector_field, 3D_variables_vector)".into()));
            }
            let field = extract_string_list(&args[0])?;
            let vars = extract_string_list(&args[1])?;
            if field.len() != 3 || vars.len() != 3 {
                return Err(PhysureError::Generic("curl requires 3D vector field and 3D variables".into()));
            }
            let parse_node = |s: &str| -> PhysureResult<crate::symbolic::Node> {
                let inlined = preprocess_symbolic_expression(s, interpreter);
                crate::symbolic::SymbolicParser::parse_str(&inlined)
            };
            let (p, q, r) = (parse_node(&field[0])?, parse_node(&field[1])?, parse_node(&field[2])?);
            let (x, y, z) = (clean_diff_var(&vars[0]), clean_diff_var(&vars[1]), clean_diff_var(&vars[2]));

            let cx = crate::symbolic::Node::Sub(Box::new(r.diff_node(y)?), Box::new(q.diff_node(z)?)).simplify();
            let cy = crate::symbolic::Node::Sub(Box::new(p.diff_node(z)?), Box::new(r.diff_node(x)?)).simplify();
            let cz = crate::symbolic::Node::Sub(Box::new(q.diff_node(x)?), Box::new(p.diff_node(y)?)).simplify();

            Ok(Some(PhsValue::Vector(vec![
                PhsValue::String(cx.to_string()),
                PhsValue::String(cy.to_string()),
                PhsValue::String(cz.to_string()),
            ])))
        }
        "laplacian" => {
            if args.len() != 2 {
                return Err(PhysureError::Generic("laplacian expects (expression_string, variables_vector)".into()));
            }
            let expr_str = match &args[0] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("laplacian expects expression string".into())),
            };
            let vars = extract_string_list(&args[1])?;
            let inlined = preprocess_symbolic_expression(expr_str, interpreter);
            let node = crate::symbolic::SymbolicParser::parse_str(&inlined)?;
            let mut second_diffs = Vec::new();
            for v in vars {
                let clean_v = clean_diff_var(&v);
                let d2 = node.diff_node_n(clean_v, 2)?.simplify();
                second_diffs.push(d2);
            }
            let lap_node = crate::symbolic::Node::Add(second_diffs).simplify();
            Ok(Some(PhsValue::String(lap_node.to_string())))
        }
        "series" | "taylor" => {
            if args.len() < 2 || args.len() > 4 {
                return Err(PhysureError::Generic(
                    "series expects (expression_string, variable_string, [about], [order]); `order` is the highest power kept, default 6".into(),
                ));
            }
            let expr_str = match &args[0] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("series expects expression string".into())),
            };
            let var = match &args[1] {
                PhsValue::String(s) => clean_diff_var(s).to_string(),
                _ => return Err(PhysureError::Generic("series expects variable string".into())),
            };
            let about = match args.get(2) {
                None => crate::symbolic::Node::Number(0.0),
                Some(PhsValue::Number(n)) => crate::symbolic::Node::Number(*n),
                Some(PhsValue::Quantity(q)) => crate::symbolic::Node::Number(q.value.mean()),
                Some(PhsValue::String(s)) => crate::symbolic::SymbolicParser::parse_str(s)?,
                Some(_) => {
                    return Err(PhysureError::Generic(
                        "series expansion point must be a number or an expression string".into(),
                    ))
                }
            };
            let order = match args.get(3) {
                None => 6,
                Some(PhsValue::Number(n)) => *n as usize,
                Some(PhsValue::Quantity(q)) => q.value.mean() as usize,
                Some(_) => {
                    return Err(PhysureError::Generic("series order must be a number".into()))
                }
            };
            let inlined = preprocess_symbolic_expression(expr_str, interpreter);
            let node = crate::symbolic::SymbolicParser::parse_str(&inlined)?;
            Ok(Some(PhsValue::String(node.series(&var, &about, order)?.to_string())))
        }
        "simplify" | "expand" => {
            if args.is_empty() {
                return Err(PhysureError::Generic("simplify expects an expression string".into()));
            }
            let expr_str = match &args[0] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("simplify expects expression string".into())),
            };
            let inlined = preprocess_symbolic_expression(expr_str, interpreter);
            let node = crate::symbolic::SymbolicParser::parse_str(&inlined)?;
            Ok(Some(PhsValue::String(node.simplify().to_string())))
        }
        "integral" | "integrate" => {
            if args.is_empty() || args.len() > 4 {
                return Err(PhysureError::Generic("integral expects (expression_string, variable_string) or (expression_string, variable_string, a, b)".into()));
            }
            let (expr_str, var_str) = if args.len() == 1 {
                let s = match &args[0] {
                    PhsValue::String(s) => s.as_str(),
                    _ => return Err(PhysureError::Generic("integral expects expression string".into())),
                };
                if let Some((e, v)) = parse_differential_integrand(s) {
                    (e, v)
                } else {
                    return Err(PhysureError::Generic("integral expects (expression_string, variable_string) or single differential string like integral(\"sin(x) dx\")".into()));
                }
            } else {
                let e = match &args[0] {
                    PhsValue::String(s) => s.clone(),
                    _ => return Err(PhysureError::Generic("integral expects expression string".into())),
                };
                let raw_v = match &args[1] {
                    PhsValue::String(s) => s.as_str(),
                    _ => return Err(PhysureError::Generic("integral expects variable string".into())),
                };
                (e, clean_diff_var(raw_v).to_string())
            };

            if args.len() == 4 {
                let extract_bound = |v: &PhsValue| -> f64 {
                    match v {
                        PhsValue::Number(n) => *n,
                        PhsValue::Quantity(q) => q.value.mean(),
                        PhsValue::String(s) => match s.trim() {
                            "inf" | "+inf" | "infinity" | "oo" | "∞" => f64::INFINITY,
                            "-inf" | "-infinity" | "-oo" | "-∞" => f64::NEG_INFINITY,
                            _ => 0.0,
                        },
                        _ => 0.0,
                    }
                };

                let a = extract_bound(&args[2]);
                let b = extract_bound(&args[3]);

                let res_val = eval_definite_integral(&expr_str, &var_str, a, b, interpreter)?;
                return Ok(Some(PhsValue::Number(res_val)));
            }

            let inlined = preprocess_symbolic_expression(&expr_str, interpreter);
            let node = crate::symbolic::SymbolicParser::parse_str(&inlined)?;
            let int_node = node.integrate_node(&var_str)?.simplify();
            Ok(Some(PhsValue::String(int_node.to_string())))
        }
        "limit" | "lim" => {
            if args.len() != 3 {
                return Err(PhysureError::Generic("limit expects (expression_string, variable_string, target_point)".into()));
            }
            let expr_str = match &args[0] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("limit expects expression string".into())),
            };
            let var_str = match &args[1] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("limit expects variable string".into())),
            };

            let point_val = match &args[2] {
                PhsValue::Number(n) => *n,
                PhsValue::Quantity(q) => q.value.mean(),
                PhsValue::String(s) => match s.trim() {
                    "inf" | "+inf" | "infinity" | "oo" | "∞" => f64::INFINITY,
                    "-inf" | "-infinity" | "-oo" | "-∞" => f64::NEG_INFINITY,
                    _ => 0.0,
                },
                _ => 0.0,
            };

            let inlined = preprocess_symbolic_expression(expr_str, interpreter);
            if let Ok(program) = crate::parser::parse_phs(&inlined) {
                if let Some(stmt) = program.statements.first() {
                    let expr = match stmt {
                        crate::ast::Statement::Expr(e) => e.clone(),
                        crate::ast::Statement::Assignment(node) => node.value.clone(),
                        _ => return Ok(Some(PhsValue::Number(0.0))),
                    };
                    let test_val = if point_val.is_infinite() {
                        if point_val.is_sign_positive() { 1e7 } else { -1e7 }
                    } else {
                        point_val + 1e-8
                    };
                    let mut local_env = interpreter.env.clone();
                    let q_val = physure_core::Quantity::new_scalar(test_val, 0.0, physure_core::units::RationalUnit::dimensionless(), None, None);
                    local_env.insert(var_str.to_string(), PhsValue::Quantity(q_val));
                    if let Ok(val) = interpreter.eval_expr(&expr, &local_env) {
                        let num = match &val {
                            PhsValue::Number(n) => *n,
                            PhsValue::Quantity(q) => q.value.mean(),
                            _ => 0.0,
                        };
                        if num.abs() < 1e-5 {
                            return Ok(Some(PhsValue::Number(0.0)));
                        }
                        if num.abs() > 1e6 {
                            let sign = if num.is_sign_positive() { f64::INFINITY } else { f64::NEG_INFINITY };
                            return Ok(Some(PhsValue::Number(sign)));
                        }
                        return Ok(Some(val));
                    }
                }
            }
            Ok(Some(PhsValue::Number(0.0)))
        }
        "solve" => {
            if args.len() != 2 {
                return Err(PhysureError::Generic("solve expects equation string and target string".into()));
            }
            let target_str = match &args[1] {
                PhsValue::String(s) => s,
                _ => return Err(PhysureError::Generic("solve expects target string".into())),
            };
            let node = match &args[0] {
                PhsValue::String(eq_str) => {
                    let inlined = preprocess_symbolic_expression(eq_str, interpreter);
                    crate::symbolic::SymbolicParser::parse_str(&inlined)?
                }
                PhsValue::Equation(l, r) => crate::symbolic::Node::Sub(Box::new(l.clone()), Box::new(r.clone())),
                _ => return Err(PhysureError::Generic("solve expects an equation string or a previously-solved equation".into())),
            };
            let solved_node = node.solve_equation(target_str)?;
            let solved_str = solved_node.to_string();

            // if target resolves against bound quantities, evaluate the solved expression against the interpreter's env
            // The python tests might expect a Number/Quantity back if variables are bound
            if let Ok(program) = crate::parser::parse_phs(&solved_str) {
                if let Some(crate::ast::Statement::Expr(expr)) = program.statements.first() {
                    if !has_unbound_vars(expr, interpreter) {
                        if let Ok(val) = interpreter.eval_expr(expr, &interpreter.env) {
                            return Ok(Some(val));
                        }
                    }
                }
            }
            Ok(Some(PhsValue::Equation(crate::symbolic::Node::Symbol(target_str.clone()), solved_node)))
        }
        "substitute" | "sub" => {
            if args.len() != 3 {
                return Err(PhysureError::Generic("substitute expects (equation_or_string, target_symbol, replacement_or_equation)".into()));
            }
            let target_str = match &args[1] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("substitute target must be a symbol string".into())),
            };

            let replacement_str = match &args[2] {
                PhsValue::String(s) => {
                    if s.contains('=') {
                        s.split('=').nth(1).unwrap_or(s).trim().to_string()
                    } else {
                        s.clone()
                    }
                }
                PhsValue::Equation(_, r) => r.to_phs_string(),
                _ => args[2].to_string(),
            };

            match &args[0] {
                PhsValue::Equation(l, r) => {
                    let new_l_str = l.to_phs_string().replace(target_str, &format!("({})", replacement_str));
                    let new_r_str = r.to_phs_string().replace(target_str, &format!("({})", replacement_str));
                    let new_l = crate::symbolic::SymbolicParser::parse_str(&new_l_str)?;
                    let new_r = crate::symbolic::SymbolicParser::parse_str(&new_r_str)?;
                    Ok(Some(PhsValue::Equation(new_l, new_r)))
                }
                PhsValue::String(eq_str) => {
                    let parts: Vec<&str> = eq_str.split('=').collect();
                    if parts.len() == 2 {
                        let new_l_str = parts[0].trim().replace(target_str, &format!("({})", replacement_str));
                        let new_r_str = parts[1].trim().replace(target_str, &format!("({})", replacement_str));
                        let new_l = crate::symbolic::SymbolicParser::parse_str(&new_l_str)?;
                        let new_r = crate::symbolic::SymbolicParser::parse_str(&new_r_str)?;
                        Ok(Some(PhsValue::Equation(new_l, new_r)))
                    } else {
                        let new_str = eq_str.replace(target_str, &format!("({})", replacement_str));
                        let new_node = crate::symbolic::SymbolicParser::parse_str(&new_str)?;
                        Ok(Some(PhsValue::String(new_node.to_string())))
                    }
                }
                _ => Err(PhysureError::Generic("substitute base must be an equation or string".into())),
            }
        }
        "dsolve" => {
            if args.len() < 3 {
                return Err(PhysureError::Generic("dsolve expects 3 arguments: dsolve(eq_str, dep_var, indep_var)".into()));
            }
            let eq_str = match &args[0] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("dsolve expects equation string".into())),
            };
            let dep_var = match &args[1] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("dsolve expects dependent variable string".into())),
            };
            let indep_var = match &args[2] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("dsolve expects independent variable string".into())),
            };
            let inlined = preprocess_symbolic_expression(eq_str, interpreter);
            let res = crate::symbolic::dsolve_str(&inlined, dep_var, indep_var)?;
            Ok(Some(PhsValue::String(res)))
        }
        "laplace" => {
            if args.len() < 3 {
                return Err(PhysureError::Generic("laplace expects 3 arguments: laplace(expr_str, t_var, s_var)".into()));
            }
            let expr_str = match &args[0] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("laplace expects expression string".into())),
            };
            let t_var = match &args[1] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("laplace expects time variable string".into())),
            };
            let s_var = match &args[2] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("laplace expects s variable string".into())),
            };
            let inlined = preprocess_symbolic_expression(expr_str, interpreter);
            let res = crate::symbolic::laplace_str(&inlined, t_var, s_var)?;
            Ok(Some(PhsValue::String(res)))
        }
        "inv_laplace" | "inverse_laplace" => {
            if args.len() < 3 {
                return Err(PhysureError::Generic("inv_laplace expects 3 arguments: inv_laplace(expr_str, s_var, t_var)".into()));
            }
            let expr_str = match &args[0] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("inv_laplace expects expression string".into())),
            };
            let s_var = match &args[1] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("inv_laplace expects s variable string".into())),
            };
            let t_var = match &args[2] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("inv_laplace expects time variable string".into())),
            };
            let inlined = preprocess_symbolic_expression(expr_str, interpreter);
            let res = crate::symbolic::inv_laplace_str(&inlined, s_var, t_var)?;
            Ok(Some(PhsValue::String(res)))
        }
        "sym_det" => {
            let mat_str = match args.first() {
                Some(PhsValue::String(s)) => s.as_str(),
                _ => return Err(PhysureError::Generic("sym_det expects a symbolic matrix string, e.g. sym_det(\"[[a, b], [c, d]]\")".into())),
            };
            let mat = crate::symbolic::SymMatrix::parse_str(mat_str)?;
            let det_node = mat.det()?;
            Ok(Some(PhsValue::String(det_node.to_string())))
        }
        "sym_trace" => {
            let mat_str = match args.first() {
                Some(PhsValue::String(s)) => s.as_str(),
                _ => return Err(PhysureError::Generic("sym_trace expects a symbolic matrix string".into())),
            };
            let mat = crate::symbolic::SymMatrix::parse_str(mat_str)?;
            let tr_node = mat.trace()?;
            Ok(Some(PhsValue::String(tr_node.to_string())))
        }
        "sym_charpoly" => {
            if args.len() < 1 {
                return Err(PhysureError::Generic("sym_charpoly expects a matrix string and optional lambda variable".into()));
            }
            let mat_str = match &args[0] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("sym_charpoly expects a symbolic matrix string".into())),
            };
            let lambda_var = if args.len() > 1 {
                match &args[1] {
                    PhsValue::String(s) => s.as_str(),
                    _ => "lambda",
                }
            } else {
                "lambda"
            };
            let mat = crate::symbolic::SymMatrix::parse_str(mat_str)?;
            let poly_node = mat.charpoly(lambda_var)?;
            Ok(Some(PhsValue::String(poly_node.to_string())))
        }
        "sym_eigenvalues" => {
            if args.is_empty() {
                return Err(PhysureError::Generic("sym_eigenvalues expects a symbolic matrix string".into()));
            }
            let mat_str = match &args[0] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("sym_eigenvalues expects a symbolic matrix string".into())),
            };
            let mat = crate::symbolic::SymMatrix::parse_str(mat_str)?;
            let eigs = mat.eigenvalues("lambda")?;
            let eigs_str: Vec<String> = eigs.iter().map(|e| e.to_string()).collect();
            Ok(Some(PhsValue::String(format!("[{}]", eigs_str.join(", ")))))
        }
        "sym_transpose" => {
            let mat_str = match args.first() {
                Some(PhsValue::String(s)) => s.as_str(),
                _ => return Err(PhysureError::Generic("sym_transpose expects a symbolic matrix string".into())),
            };
            let mat = crate::symbolic::SymMatrix::parse_str(mat_str)?;
            Ok(Some(PhsValue::String(mat.transpose().to_phs_string())))
        }
        _ => Ok(None),
    }
}


fn has_unbound_vars(expr: &crate::ast::Expr, interpreter: &PhsInterpreter) -> bool {
    match expr {
        crate::ast::Expr::Identifier(name) => {
            interpreter.get_var(name).is_none()
        }
        crate::ast::Expr::BinaryOp { left, right, .. } => {
            has_unbound_vars(left, interpreter) || has_unbound_vars(right, interpreter)
        }
        crate::ast::Expr::FunctionCall { args, .. } => {
            args.iter().any(|arg| has_unbound_vars(arg, interpreter))
        }
        _ => false,
    }
}

fn extract_string_list(val: &PhsValue) -> PhysureResult<Vec<String>> {
    match val {
        PhsValue::Vector(vec) => {
            let mut list = Vec::new();
            for item in vec {
                match item {
                    PhsValue::String(s) => list.push(s.clone()),
                    _ => list.push(item.to_string()),
                }
            }
            Ok(list)
        }
        PhsValue::String(s) => Ok(s.split(',').map(|item| item.trim().to_string()).collect()),
        _ => Err(PhysureError::Generic("Expected vector or comma-separated string list".into())),
    }
}

fn eval_definite_integral(expr_str: &str, var_str: &str, a: f64, b: f64, interpreter: &PhsInterpreter) -> PhysureResult<f64> {
    let inlined = preprocess_symbolic_expression(expr_str, interpreter);
    let program = crate::parser::parse_phs(&inlined)?;
    let expr = match program.statements.first() {
        Some(crate::ast::Statement::Expr(e)) => e.clone(),
        Some(crate::ast::Statement::Assignment(node)) => node.value.clone(),
        _ => return Err(PhysureError::Generic("Failed to parse integrand expression".into())),
    };

    let mut local_env = interpreter.env.clone();
    let mut eval_at = |x: f64| -> f64 {
        local_env.insert(var_str.to_string(), PhsValue::Number(x));
        if let Ok(val) = interpreter.eval_expr(&expr, &local_env) {
            match val {
                PhsValue::Number(n) => n,
                PhsValue::Quantity(q) => q.value.mean(),
                _ => 0.0,
            }
        } else {
            0.0
        }
    };

    // Try Symbolic First
    if !a.is_infinite() && !b.is_infinite() {
        if let Ok(node) = crate::symbolic::SymbolicParser::parse_str(&inlined) {
            if let Ok(int_node) = node.integrate_node(var_str) {
                let int_str = int_node.simplify().to_string();
                if let Ok(int_prog) = crate::parser::parse_phs(&int_str) {
                    if let Some(crate::ast::Statement::Expr(int_expr)) = int_prog.statements.first() {
                        let mut env_a = interpreter.env.clone();
                        env_a.insert(var_str.to_string(), PhsValue::Number(a));
                        let mut env_b = interpreter.env.clone();
                        env_b.insert(var_str.to_string(), PhsValue::Number(b));
                        
                        let val_b = interpreter.eval_expr(int_expr, &env_b);
                        let val_a = interpreter.eval_expr(int_expr, &env_a);

                        let get_num = |v: PhysureResult<PhsValue>| -> Option<f64> {
                            match v.ok()? {
                                PhsValue::Number(n) => Some(n),
                                PhsValue::Quantity(q) => Some(q.value.mean()),
                                _ => None,
                            }
                        };
                        if let (Some(nb), Some(na)) = (get_num(val_b), get_num(val_a)) {
                            return Ok(nb - na);
                        }
                    }
                }
            }
        }
    }


    if a.is_infinite() || b.is_infinite() {
        let mut transform_eval = |t: f64| -> f64 {
            if t.abs() >= 1.0 - 1e-7 {
                return 0.0;
            }
            let x = t / (1.0 - t * t);
            let dxdt = (1.0 + t * t) / ((1.0 - t * t) * (1.0 - t * t));
            let fx = eval_at(x);
            if fx.is_nan() || fx.is_infinite() {
                0.0
            } else {
                fx * dxdt
            }
        };

        let t_a = if a == f64::NEG_INFINITY { -1.0 + 1e-6 } else if a == f64::INFINITY { 1.0 - 1e-6 } else { a / (1.0 + (1.0 + a * a).sqrt()) };
        let t_b = if b == f64::INFINITY { 1.0 - 1e-6 } else if b == f64::NEG_INFINITY { -1.0 + 1e-6 } else { b / (1.0 + (1.0 + b * b).sqrt()) };
        
        return Ok(quad_gauss_kronrod(&mut transform_eval, t_a, t_b, 100));
    }

    Ok(quad_gauss_kronrod(&mut eval_at, a, b, 100))
}

fn quad_gauss_kronrod<F>(mut f: F, a: f64, b: f64, steps: usize) -> f64
where F: FnMut(f64) -> f64 {
    let h = (b - a) / steps as f64;
    let mut sum = 0.0;
    for i in 0..steps {
        let x0 = a + i as f64 * h;
        let x1 = x0 + h;
        let mid = 0.5 * (x0 + x1);
        let half_h = 0.5 * h;
        let f0 = f(x0);
        let f1 = f(mid - half_h * 0.5773502691896257);
        let f2 = f(mid);
        let f3 = f(mid + half_h * 0.5773502691896257);
        let f4 = f(x1);
        sum += (half_h / 9.0) * (f0 + 4.0 * f2 + f4 + 2.0 * (f1 + f3));
    }
    sum
}
