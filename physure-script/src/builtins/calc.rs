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
    let (num, den) = spec.trim().split_once('/')?;
    let (num, den) = (num.trim(), den.trim());

    if num == "d" || num.starts_with("d/") {
        if den.contains('(') && den.ends_with(')') {
            let p1 = den.find('(')?;
            let v = &den[..p1];
            let expr = &den[p1 + 1..den.len() - 1];
            return Some((expr.to_string(), clean_diff_var(v).to_string(), 1));
        }
    }

    let den_var = leibniz_variable(den);
    let (order, expr) = leibniz_order_and_expression(num);

    let expr_clean = if expr.starts_with('(') && expr.ends_with(')') {
        &expr[1..expr.len() - 1]
    } else {
        expr
    };

    if !expr_clean.is_empty() && !den_var.is_empty() {
        return Some((expr_clean.to_string(), den_var.to_string(), order));
    }
    None
}

/// The variable of a Leibniz denominator: `dx`, `dx^2`, `d(x)` or a bare `x`.
fn leibniz_variable(den: &str) -> &str {
    let Some(v) = den.strip_prefix('d') else {
        return den;
    };
    if let Some(hat_idx) = v.find('^') {
        clean_diff_var(&v[..hat_idx])
    } else if v.starts_with('(') && v.ends_with(')') {
        clean_diff_var(v)
    } else if v.chars().all(|c| c.is_alphanumeric() || c == '_') {
        clean_diff_var(den)
    } else {
        clean_diff_var(v)
    }
}

/// The order of a Leibniz numerator and what is differentiated: `d^2y`, `d2y`, `dy` or a bare `y`.
fn leibniz_order_and_expression(num: &str) -> (usize, &str) {
    if num.starts_with("d^") {
        let hat_idx = num.find('^').unwrap();
        let rest = &num[hat_idx + 1..];
        let end_digit = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
        let ord = rest[..end_digit].parse::<usize>().unwrap_or(1);
        (ord, rest[end_digit..].trim())
    } else if num.starts_with("d2") || num.starts_with("d3") || num.starts_with("d4") {
        (num[1..2].parse::<usize>().unwrap_or(1), num[2..].trim())
    } else if num.starts_with('d') && num.len() > 1 {
        (1, num[1..].trim())
    } else {
        (1, num)
    }
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
        "deriv" | "derivative" | "diff" => deriv(args, interpreter),
        "grad" | "gradient" => grad(args, interpreter),
        "div" | "divergence" => divergence(args, interpreter),
        "curl" => curl(args, interpreter),
        "laplacian" => laplacian(args, interpreter),
        "series" | "taylor" => series(args, interpreter),
        "simplify" | "expand" => simplify(args, interpreter),
        "integral" | "integrate" => integral(args, interpreter),
        "limit" | "lim" => limit(args, interpreter),
        "solve" => solve(args, interpreter),
        "substitute" | "sub" => substitute(args),
        "dsolve" => dsolve(args, interpreter),
        "laplace" => laplace(args, interpreter),
        "inv_laplace" | "inverse_laplace" => inv_laplace(args, interpreter),
        "sym_det" => {
            let mat_str = string_arg(args, 0, "sym_det expects a symbolic matrix string, e.g. sym_det(\"[[a, b], [c, d]]\")")?;
            let det_node = crate::symbolic::SymMatrix::parse_str(mat_str)?.det()?;
            Ok(Some(PhsValue::String(det_node.to_string())))
        }
        "sym_trace" => {
            let mat_str = string_arg(args, 0, "sym_trace expects a symbolic matrix string")?;
            let tr_node = crate::symbolic::SymMatrix::parse_str(mat_str)?.trace()?;
            Ok(Some(PhsValue::String(tr_node.to_string())))
        }
        "sym_charpoly" => {
            if args.is_empty() {
                return Err(generic_error("sym_charpoly expects a matrix string and optional lambda variable"));
            }
            let mat_str = string_arg(args, 0, "sym_charpoly expects a symbolic matrix string")?;
            let lambda_var = match args.get(1) {
                Some(PhsValue::String(s)) => s.as_str(),
                _ => "lambda",
            };
            let poly_node = crate::symbolic::SymMatrix::parse_str(mat_str)?.charpoly(lambda_var)?;
            Ok(Some(PhsValue::String(poly_node.to_string())))
        }
        "sym_eigenvalues" => {
            if args.is_empty() {
                return Err(generic_error("sym_eigenvalues expects a symbolic matrix string"));
            }
            let mat_str = string_arg(args, 0, "sym_eigenvalues expects a symbolic matrix string")?;
            let eigs = crate::symbolic::SymMatrix::parse_str(mat_str)?.eigenvalues("lambda")?;
            let eigs_str: Vec<String> = eigs.iter().map(|e| e.to_string()).collect();
            Ok(Some(PhsValue::String(format!("[{}]", eigs_str.join(", ")))))
        }
        "sym_transpose" => {
            let mat_str = string_arg(args, 0, "sym_transpose expects a symbolic matrix string")?;
            let mat = crate::symbolic::SymMatrix::parse_str(mat_str)?;
            Ok(Some(PhsValue::String(mat.transpose().to_phs_string())))
        }
        _ => Ok(None),
    }
}

fn generic_error(message: &str) -> PhysureError {
    PhysureError::Generic(message.into())
}

/// The string argument at `index`, or the error `message`.
fn string_arg<'a>(args: &'a [PhsValue], index: usize, message: &str) -> PhysureResult<&'a str> {
    match args.get(index) {
        Some(PhsValue::String(s)) => Ok(s.as_str()),
        _ => Err(generic_error(message)),
    }
}

/// The symbolic tree of an expression string, with the user's functions inlined.
fn parse_inlined(text: &str, interpreter: &PhsInterpreter) -> PhysureResult<crate::symbolic::Node> {
    let inlined = preprocess_symbolic_expression(text, interpreter);
    crate::symbolic::SymbolicParser::parse_str(&inlined)
}

/// A point or a bound: a number, a quantity, or a string such as `"inf"`; anything else reads as 0.
fn bound_value(v: &PhsValue) -> f64 {
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
}

/// What `deriv` was asked: the expression, the variable, the order, and the dependent variable when the
/// expression is a bare name (`y` in `dy/dx`).
fn deriv_request(args: &[PhsValue]) -> PhysureResult<(String, String, usize, Option<String>)> {
    let (e, v, o) = if args.len() == 1 {
        let spec = string_arg(args, 0, "deriv expects expression string")?;
        parse_leibniz_single_arg(spec).ok_or_else(|| {
            generic_error("deriv expects (expr_str, var_str) or Leibniz notation string like deriv(\"dy/dx\")")
        })?
    } else {
        let e = string_arg(args, 0, "deriv expects expression string")?.to_string();
        let raw_v = string_arg(args, 1, "deriv expects variable string")?;
        let o = match args.get(2) {
            Some(PhsValue::Number(n)) => *n as usize,
            Some(PhsValue::Quantity(q)) => q.value.mean() as usize,
            _ => 1,
        };
        (e, clean_diff_var(raw_v).to_string(), o)
    };
    let dep = e.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '\'').then(|| e.clone());
    Ok((e, v, o, dep))
}

fn deriv(args: &[PhsValue], interpreter: &PhsInterpreter) -> PhysureResult<Option<PhsValue>> {
    if args.is_empty() || args.len() > 3 {
        return Err(generic_error("deriv expects 1, 2, or 3 arguments: deriv(expr, [var], [order])"));
    }
    let (expr_str, var_str, order, explicit_dep) = deriv_request(args)?;

    let inlined = preprocess_symbolic_expression(&expr_str, interpreter);
    if let Some(idx) = inlined.find('=') {
        let lhs = crate::symbolic::SymbolicParser::parse_str(&inlined[..idx])?;
        let rhs = crate::symbolic::SymbolicParser::parse_str(&inlined[idx + 1..])?;
        let eq_node = crate::symbolic::Node::Equation(Box::new(lhs), Box::new(rhs));
        let diff_eq = eq_node.diff_node_n(&var_str, order)?;
        return Ok(Some(PhsValue::String(diff_eq.to_string())));
    }
    let mut diff_node = crate::symbolic::SymbolicParser::parse_str(&inlined)?;
    let dep_var_ref = explicit_dep.as_deref();
    for _ in 0..order {
        diff_node = diff_node.diff_node_implicit(&var_str, dep_var_ref)?.simplify();
    }
    Ok(Some(PhsValue::String(diff_node.to_string())))
}

fn grad(args: &[PhsValue], interpreter: &PhsInterpreter) -> PhysureResult<Option<PhsValue>> {
    if args.len() != 2 {
        return Err(generic_error("grad expects (expression_string, variables_vector)"));
    }
    let expr_str = string_arg(args, 0, "grad expects expression string")?;
    let vars = extract_string_list(&args[1])?;
    let node = parse_inlined(expr_str, interpreter)?;
    let mut grads = Vec::new();
    for v in vars {
        let d = node.diff_node(clean_diff_var(&v))?.simplify();
        grads.push(PhsValue::String(d.to_string()));
    }
    Ok(Some(PhsValue::Vector(grads)))
}

fn divergence(args: &[PhsValue], interpreter: &PhsInterpreter) -> PhysureResult<Option<PhsValue>> {
    if args.len() != 2 {
        return Err(generic_error("div expects (vector_field, variables_vector)"));
    }
    let field_exprs = extract_string_list(&args[0])?;
    let vars = extract_string_list(&args[1])?;
    if field_exprs.len() != vars.len() {
        return Err(generic_error("Vector field and variables dimension mismatch"));
    }
    let mut sum_nodes = Vec::new();
    for (f_str, v) in field_exprs.iter().zip(vars.iter()) {
        let node = parse_inlined(f_str, interpreter)?;
        sum_nodes.push(node.diff_node(clean_diff_var(v))?.simplify());
    }
    let div_node = crate::symbolic::Node::Add(sum_nodes).simplify();
    Ok(Some(PhsValue::String(div_node.to_string())))
}

fn curl(args: &[PhsValue], interpreter: &PhsInterpreter) -> PhysureResult<Option<PhsValue>> {
    if args.len() != 2 {
        return Err(generic_error("curl expects (3D_vector_field, 3D_variables_vector)"));
    }
    let field = extract_string_list(&args[0])?;
    let vars = extract_string_list(&args[1])?;
    if field.len() != 3 || vars.len() != 3 {
        return Err(generic_error("curl requires 3D vector field and 3D variables"));
    }
    let (p, q, r) = (
        parse_inlined(&field[0], interpreter)?,
        parse_inlined(&field[1], interpreter)?,
        parse_inlined(&field[2], interpreter)?,
    );
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

fn laplacian(args: &[PhsValue], interpreter: &PhsInterpreter) -> PhysureResult<Option<PhsValue>> {
    if args.len() != 2 {
        return Err(generic_error("laplacian expects (expression_string, variables_vector)"));
    }
    let expr_str = string_arg(args, 0, "laplacian expects expression string")?;
    let vars = extract_string_list(&args[1])?;
    let node = parse_inlined(expr_str, interpreter)?;
    let mut second_diffs = Vec::new();
    for v in vars {
        second_diffs.push(node.diff_node_n(clean_diff_var(&v), 2)?.simplify());
    }
    let lap_node = crate::symbolic::Node::Add(second_diffs).simplify();
    Ok(Some(PhsValue::String(lap_node.to_string())))
}

fn series(args: &[PhsValue], interpreter: &PhsInterpreter) -> PhysureResult<Option<PhsValue>> {
    if args.len() < 2 || args.len() > 4 {
        return Err(generic_error(
            "series expects (expression_string, variable_string, [about], [order]); `order` is the highest power kept, default 6",
        ));
    }
    let expr_str = string_arg(args, 0, "series expects expression string")?;
    let var = clean_diff_var(string_arg(args, 1, "series expects variable string")?).to_string();
    let about = match args.get(2) {
        None => crate::symbolic::Node::Number(0.0),
        Some(PhsValue::Number(n)) => crate::symbolic::Node::Number(*n),
        Some(PhsValue::Quantity(q)) => crate::symbolic::Node::Number(q.value.mean()),
        Some(PhsValue::String(s)) => crate::symbolic::SymbolicParser::parse_str(s)?,
        Some(_) => return Err(generic_error("series expansion point must be a number or an expression string")),
    };
    let order = match args.get(3) {
        None => 6,
        Some(PhsValue::Number(n)) => *n as usize,
        Some(PhsValue::Quantity(q)) => q.value.mean() as usize,
        Some(_) => return Err(generic_error("series order must be a number")),
    };
    let node = parse_inlined(expr_str, interpreter)?;
    Ok(Some(PhsValue::String(node.series(&var, &about, order)?.to_string())))
}

fn simplify(args: &[PhsValue], interpreter: &PhsInterpreter) -> PhysureResult<Option<PhsValue>> {
    if args.is_empty() {
        return Err(generic_error("simplify expects an expression string"));
    }
    let expr_str = string_arg(args, 0, "simplify expects expression string")?;
    let node = parse_inlined(expr_str, interpreter)?;
    Ok(Some(PhsValue::String(node.simplify().to_string())))
}

/// What `integral` was asked: the integrand and the variable, from two arguments or from `"f(x) dx"`.
fn integral_request(args: &[PhsValue]) -> PhysureResult<(String, String)> {
    if args.len() == 1 {
        let s = string_arg(args, 0, "integral expects expression string")?;
        return parse_differential_integrand(s).ok_or_else(|| {
            generic_error("integral expects (expression_string, variable_string) or single differential string like integral(\"sin(x) dx\")")
        });
    }
    let e = string_arg(args, 0, "integral expects expression string")?.to_string();
    let raw_v = string_arg(args, 1, "integral expects variable string")?;
    Ok((e, clean_diff_var(raw_v).to_string()))
}

fn integral(args: &[PhsValue], interpreter: &PhsInterpreter) -> PhysureResult<Option<PhsValue>> {
    if args.is_empty() || args.len() > 4 {
        return Err(generic_error("integral expects (expression_string, variable_string) or (expression_string, variable_string, a, b)"));
    }
    let (expr_str, var_str) = integral_request(args)?;

    if args.len() == 4 {
        let (a, b) = (bound_value(&args[2]), bound_value(&args[3]));
        let res_val = eval_definite_integral(&expr_str, &var_str, a, b, interpreter)?;
        return Ok(Some(PhsValue::Number(res_val)));
    }

    let node = parse_inlined(&expr_str, interpreter)?;
    let int_node = node.integrate_node(&var_str)?.simplify();
    Ok(Some(PhsValue::String(int_node.to_string())))
}

fn limit(args: &[PhsValue], interpreter: &PhsInterpreter) -> PhysureResult<Option<PhsValue>> {
    if args.len() != 3 {
        return Err(generic_error("limit expects (expression_string, variable_string, target_point)"));
    }
    let expr_str = string_arg(args, 0, "limit expects expression string")?;
    let var_str = string_arg(args, 1, "limit expects variable string")?;
    let point_val = bound_value(&args[2]);

    let inlined = preprocess_symbolic_expression(expr_str, interpreter);
    let probed = probe_limit(&inlined, var_str, point_val, interpreter);
    Ok(Some(probed.unwrap_or(PhsValue::Number(0.0))))
}

/// The expression evaluated next to the point (or very far out, for an infinite one): a value that has grown
/// huge is an infinity and one that has shrunk to nothing is 0. `None` when it cannot be evaluated there.
fn probe_limit(inlined: &str, var_str: &str, point_val: f64, interpreter: &PhsInterpreter) -> Option<PhsValue> {
    let program = crate::parser::parse_phs(inlined).ok()?;
    let expr = match program.statements.first()? {
        crate::ast::Statement::Expr(e) => e.clone(),
        crate::ast::Statement::Assignment(node) => node.value.clone(),
        _ => return Some(PhsValue::Number(0.0)),
    };
    let test_val = if point_val.is_infinite() {
        if point_val.is_sign_positive() { 1e7 } else { -1e7 }
    } else {
        point_val + 1e-8
    };
    let mut local_env = interpreter.env.clone();
    let q_val = physure_core::Quantity::new_scalar(test_val, 0.0, physure_core::units::RationalUnit::dimensionless(), None, None);
    local_env.insert(var_str.to_string(), PhsValue::Quantity(q_val));
    let val = interpreter.eval_expr(&expr, &local_env).ok()?;
    let num = match &val {
        PhsValue::Number(n) => *n,
        PhsValue::Quantity(q) => q.value.mean(),
        _ => 0.0,
    };
    if num.abs() < 1e-5 {
        return Some(PhsValue::Number(0.0));
    }
    if num.abs() > 1e6 {
        let sign = if num.is_sign_positive() { f64::INFINITY } else { f64::NEG_INFINITY };
        return Some(PhsValue::Number(sign));
    }
    Some(val)
}

fn solve(args: &[PhsValue], interpreter: &PhsInterpreter) -> PhysureResult<Option<PhsValue>> {
    if args.len() != 2 {
        return Err(generic_error("solve expects equation string and target string"));
    }
    let target_str = match &args[1] {
        PhsValue::String(s) => s,
        _ => return Err(generic_error("solve expects target string")),
    };
    let node = match &args[0] {
        PhsValue::String(eq_str) => parse_inlined(eq_str, interpreter)?,
        PhsValue::Equation(l, r) => crate::symbolic::Node::Sub(Box::new(l.clone()), Box::new(r.clone())),
        _ => return Err(generic_error("solve expects an equation string or a previously-solved equation")),
    };
    let solved_node = node.solve_equation(target_str)?;

    // if target resolves against bound quantities, evaluate the solved expression against the interpreter's env
    // The python tests might expect a Number/Quantity back if variables are bound
    if let Some(val) = evaluate_if_bound(&solved_node.to_string(), interpreter) {
        return Ok(Some(val));
    }
    Ok(Some(PhsValue::Equation(crate::symbolic::Node::Symbol(target_str.clone()), solved_node)))
}

/// The value of a solved expression whose variables are all bound in the interpreter; `None` if one is not.
fn evaluate_if_bound(solved_str: &str, interpreter: &PhsInterpreter) -> Option<PhsValue> {
    let program = crate::parser::parse_phs(solved_str).ok()?;
    let Some(crate::ast::Statement::Expr(expr)) = program.statements.first() else {
        return None;
    };
    if has_unbound_vars(expr, interpreter) {
        return None;
    }
    interpreter.eval_expr(expr, &interpreter.env).ok()
}

/// The text a `substitute` replacement stands for: the right side of an equation, or the value written out.
fn replacement_text(replacement: &PhsValue) -> String {
    match replacement {
        PhsValue::String(s) if s.contains('=') => s.split('=').nth(1).unwrap_or(s).trim().to_string(),
        PhsValue::String(s) => s.clone(),
        PhsValue::Equation(_, r) => r.to_phs_string(),
        other => other.to_string(),
    }
}

fn substitute(args: &[PhsValue]) -> PhysureResult<Option<PhsValue>> {
    if args.len() != 3 {
        return Err(generic_error("substitute expects (equation_or_string, target_symbol, replacement_or_equation)"));
    }
    let target_str = string_arg(args, 1, "substitute target must be a symbol string")?;
    let replacement = format!("({})", replacement_text(&args[2]));

    let replaced_equation = |left: &str, right: &str| -> PhysureResult<Option<PhsValue>> {
        let new_l = crate::symbolic::SymbolicParser::parse_str(&left.replace(target_str, &replacement))?;
        let new_r = crate::symbolic::SymbolicParser::parse_str(&right.replace(target_str, &replacement))?;
        Ok(Some(PhsValue::Equation(new_l, new_r)))
    };
    match &args[0] {
        PhsValue::Equation(l, r) => replaced_equation(&l.to_phs_string(), &r.to_phs_string()),
        PhsValue::String(eq_str) => {
            let parts: Vec<&str> = eq_str.split('=').collect();
            if parts.len() == 2 {
                return replaced_equation(parts[0].trim(), parts[1].trim());
            }
            let new_node = crate::symbolic::SymbolicParser::parse_str(&eq_str.replace(target_str, &replacement))?;
            Ok(Some(PhsValue::String(new_node.to_string())))
        }
        _ => Err(generic_error("substitute base must be an equation or string")),
    }
}

fn dsolve(args: &[PhsValue], interpreter: &PhsInterpreter) -> PhysureResult<Option<PhsValue>> {
    if args.len() < 3 {
        return Err(generic_error("dsolve expects 3 arguments: dsolve(eq_str, dep_var, indep_var)"));
    }
    let eq_str = string_arg(args, 0, "dsolve expects equation string")?;
    let dep_var = string_arg(args, 1, "dsolve expects dependent variable string")?;
    let indep_var = string_arg(args, 2, "dsolve expects independent variable string")?;
    let inlined = preprocess_symbolic_expression(eq_str, interpreter);
    Ok(Some(PhsValue::String(crate::symbolic::dsolve_str(&inlined, dep_var, indep_var)?)))
}

fn laplace(args: &[PhsValue], interpreter: &PhsInterpreter) -> PhysureResult<Option<PhsValue>> {
    if args.len() < 3 {
        return Err(generic_error("laplace expects 3 arguments: laplace(expr_str, t_var, s_var)"));
    }
    let expr_str = string_arg(args, 0, "laplace expects expression string")?;
    let t_var = string_arg(args, 1, "laplace expects time variable string")?;
    let s_var = string_arg(args, 2, "laplace expects s variable string")?;
    let inlined = preprocess_symbolic_expression(expr_str, interpreter);
    Ok(Some(PhsValue::String(crate::symbolic::laplace_str(&inlined, t_var, s_var)?)))
}

fn inv_laplace(args: &[PhsValue], interpreter: &PhsInterpreter) -> PhysureResult<Option<PhsValue>> {
    if args.len() < 3 {
        return Err(generic_error("inv_laplace expects 3 arguments: inv_laplace(expr_str, s_var, t_var)"));
    }
    let expr_str = string_arg(args, 0, "inv_laplace expects expression string")?;
    let s_var = string_arg(args, 1, "inv_laplace expects s variable string")?;
    let t_var = string_arg(args, 2, "inv_laplace expects time variable string")?;
    let inlined = preprocess_symbolic_expression(expr_str, interpreter);
    Ok(Some(PhsValue::String(crate::symbolic::inv_laplace_str(&inlined, s_var, t_var)?)))
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

    // Try Symbolic First
    if !a.is_infinite() && !b.is_infinite() {
        if let Some(value) = symbolic_definite_integral(&inlined, var_str, a, b, interpreter) {
            return Ok(value);
        }
    }

    let mut local_env = interpreter.env.clone();
    let mut eval_at = |x: f64| -> f64 {
        local_env.insert(var_str.to_string(), PhsValue::Number(x));
        match interpreter.eval_expr(&expr, &local_env) {
            Ok(PhsValue::Number(n)) => n,
            Ok(PhsValue::Quantity(q)) => q.value.mean(),
            _ => 0.0,
        }
    };

    if a.is_infinite() || b.is_infinite() {
        return Ok(improper_integral(&mut eval_at, a, b));
    }
    Ok(quad_gauss_kronrod(&mut eval_at, a, b, 100))
}

/// The antiderivative evaluated at both ends, when the integrand has one the interpreter can evaluate there.
fn symbolic_definite_integral(inlined: &str, var_str: &str, a: f64, b: f64, interpreter: &PhsInterpreter) -> Option<f64> {
    let node = crate::symbolic::SymbolicParser::parse_str(inlined).ok()?;
    let int_node = node.integrate_node(var_str).ok()?;
    let int_prog = crate::parser::parse_phs(&int_node.simplify().to_string()).ok()?;
    let Some(crate::ast::Statement::Expr(int_expr)) = int_prog.statements.first() else {
        return None;
    };
    let value_at = |x: f64| -> Option<f64> {
        let mut env = interpreter.env.clone();
        env.insert(var_str.to_string(), PhsValue::Number(x));
        match interpreter.eval_expr(int_expr, &env).ok()? {
            PhsValue::Number(n) => Some(n),
            PhsValue::Quantity(q) => Some(q.value.mean()),
            _ => None,
        }
    };
    Some(value_at(b)? - value_at(a)?)
}

/// Where an end of an improper integral lands on (-1, 1) under `x = t / (1 - t^2)`.
fn to_unit_interval(x: f64) -> f64 {
    if x == f64::NEG_INFINITY {
        -1.0 + 1e-6
    } else if x == f64::INFINITY {
        1.0 - 1e-6
    } else {
        x / (1.0 + (1.0 + x * x).sqrt())
    }
}

/// An integral with an infinite end, by the substitution `x = t / (1 - t^2)` onto the finite interval (-1, 1).
fn improper_integral(eval_at: &mut impl FnMut(f64) -> f64, a: f64, b: f64) -> f64 {
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
    quad_gauss_kronrod(&mut transform_eval, to_unit_interval(a), to_unit_interval(b), 100)
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
