use std::collections::HashMap;
use physure_core::error::{PhysureError, PhysureResult};
use physure_core::units::UnitRegistry;
use super::ast::{BinaryOp, Expr, ParamDef, Statement, UnaryOp};
use super::builtins::eval_builtin;
use super::value::PhsValue;

pub fn eval_phs(input: &str) -> PhysureResult<Vec<PhsValue>> {
    let stmts = super::parser::parse_phs(input)?;
    let mut interp = PhsInterpreter::new();
    let mut results = Vec::new();
    for stmt in stmts {
        let val = interp.run_statement(&stmt)?;
        if val != PhsValue::None {
            results.push(val);
        }
    }
    Ok(results)
}

#[derive(Debug, Clone)]
pub struct UserFn {
    pub params: Vec<ParamDef>,
    pub body: Vec<Statement>,
}

pub type NativeFn = std::sync::Arc<dyn Fn(&[PhsValue], &mut PhsInterpreter) -> PhysureResult<PhsValue> + Send + Sync>;

#[derive(Clone)]
pub struct PhsInterpreter {
    env: HashMap<String, PhsValue>,
    functions: HashMap<String, UserFn>,
    custom_functions: HashMap<String, NativeFn>,
    registry: UnitRegistry,
    recursion_depth: usize,
}

impl Default for PhsInterpreter {
    fn default() -> Self {
        Self::new()
    }
}

impl PhsInterpreter {
    pub fn new() -> Self {
        let (registry, _constants) = physure_core::units::conf::build_registry_from_conf();
        Self {
            env: HashMap::new(),
            functions: HashMap::new(),
            custom_functions: HashMap::new(),
            registry,
            recursion_depth: 0,
        }
    }

    pub fn with_registry(registry: UnitRegistry) -> Self {
        Self {
            env: HashMap::new(),
            functions: HashMap::new(),
            custom_functions: HashMap::new(),
            registry,
            recursion_depth: 0,
        }
    }

    pub fn registry(&self) -> &UnitRegistry {
        &self.registry
    }

    pub fn registry_mut(&mut self) -> &mut UnitRegistry {
        &mut self.registry
    }

    pub fn get_fn_params(&self, name: &str) -> Option<Vec<String>> {
        self.functions.get(name).map(|f| f.params.iter().map(|p| p.name.clone()).collect())
    }

    pub fn get_user_fn(&self, name: &str) -> Option<&UserFn> {
        self.functions.get(name)
    }

    pub fn register_fn<F>(&mut self, name: impl Into<String>, func: F)
    where
        F: Fn(&[PhsValue], &mut PhsInterpreter) -> PhysureResult<PhsValue> + Send + Sync + 'static,
    {
        self.custom_functions.insert(name.into(), std::sync::Arc::new(func));
    }

    pub fn get_var(&self, name: &str) -> Option<&PhsValue> {
        self.env.get(name)
    }

    pub fn set_var(&mut self, name: impl Into<String>, val: PhsValue) {
        self.env.insert(name.into(), val);
    }

    pub fn run_statement(&mut self, stmt: &Statement) -> PhysureResult<PhsValue> {
        match stmt {
            Statement::Assign { name, expr } => {
                if self.functions.contains_key(name) {
                    return Err(PhysureError::Generic(format!("Cannot assign to '{}': already defined as a function", name)));
                }
                if let Some(fn_def) = self.try_build_fn_algebra(name, expr)? {
                    self.functions.insert(name.clone(), fn_def);
                    return Ok(PhsValue::None);
                }
                let val = self.eval_expr(expr)?;
                self.env.insert(name.clone(), val);
                Ok(PhsValue::None)
            }
            Statement::Query { expr } => self.eval_expr(expr),
            Statement::AssignAndQuery { name, expr } => {
                if let Some(fn_def) = self.try_build_fn_algebra(name, expr)? {
                    self.functions.insert(name.clone(), fn_def);
                    return Ok(PhsValue::None);
                }
                let val = self.eval_expr(expr)?;
                self.env.insert(name.clone(), val.clone());
                Ok(val)
            }
            Statement::Assert { left, right, op } => {
                let l_val = self.eval_expr(left)?;
                let r_val = self.eval_expr(right)?;
                let res = self.eval_binary_op(op, &l_val, &r_val)?;
                Ok(res)
            }
            Statement::FnDef { name, params, body } => {
                if self.env.contains_key(name) {
                    return Err(PhysureError::Generic(format!("Cannot define function '{}': already defined as a variable", name)));
                }
                self.functions.insert(
                    name.clone(),
                    UserFn {
                        params: params.clone(),
                        body: body.clone(),
                    },
                );
                Ok(PhsValue::None)
            }
            Statement::DisplayText(text) => {
                let mut result = String::new();
                let mut rest = text.as_str();
                while let Some(start) = rest.find('{') {
                    result.push_str(&rest[..start]);
                    if let Some(end) = rest[start..].find('}') {
                        let expr_str = rest[start + 1..start + end].trim();
                        if let Ok(tokens) = crate::lexer::PhsLexer::new(expr_str).tokenize() {
                            let mut parser = crate::parser::PhsParser::new(&tokens);
                            if let Ok(stmts) = parser.parse_statements() {
                                let mut last_val = PhsValue::None;
                                for stmt in &stmts {
                                    if let Ok(val) = self.run_statement(stmt) {
                                        last_val = val;
                                    }
                                }
                                if last_val != PhsValue::None {
                                    result.push_str(&last_val.to_string());
                                } else {
                                    result.push_str(&rest[start..=start + end]);
                                }
                            } else {
                                result.push_str(&rest[start..=start + end]);
                            }
                        } else {
                            result.push_str(&rest[start..=start + end]);
                        }
                        rest = &rest[start + end + 1..];
                    } else {
                        result.push_str(&rest[start..]);
                        rest = "";
                        break;
                    }
                }
                result.push_str(rest);
                Ok(PhsValue::String(result))
            }
            Statement::ExprStmt(expr) => self.eval_expr(expr),
        }
    }

    pub fn eval_expr(&mut self, expr: &Expr) -> PhysureResult<PhsValue> {
        match expr {
            Expr::Number(n) => Ok(PhsValue::Number(*n)),
            Expr::StringLiteral(s) => Ok(PhsValue::String(s.clone())),
            Expr::Ident(name) => {
                if let Some(val) = self.env.get(name) {
                    return Ok(val.clone());
                }
                if let Some(unit) = self.registry.get_unit(name) {
                    use physure_core::quantity::Quantity;
                    return Ok(PhsValue::Quantity(Quantity::new_scalar(
                        1.0, 0.0, unit, None, None,
                    )));
                }
                if let Ok(unit) = physure_core::units::parser::Parser::parse_expression(name) {
                    use physure_core::quantity::Quantity;
                    return Ok(PhsValue::Quantity(Quantity::new_scalar(
                        1.0, 0.0, unit, None, None,
                    )));
                }
                Ok(PhsValue::ident(name.clone()))
            }
            Expr::Convert { expr, target_unit } => {
                let val = self.eval_expr(expr)?;
                let parsed_unit = physure_core::units::parser::Parser::parse_expression_with_registry(target_unit, &self.registry)?;
                match val {
                    PhsValue::Quantity(q) => {
                        let converted = q.convert_to(&parsed_unit)?;
                        Ok(PhsValue::Quantity(converted))
                    }
                    PhsValue::Vector(vec) => {
                        let converted_vec = vec.into_iter().map(|item| match item {
                            PhsValue::Quantity(q) => q.convert_to(&parsed_unit).map(PhsValue::Quantity),
                            other => Ok(other),
                        }).collect::<PhysureResult<Vec<_>>>()?;
                        Ok(PhsValue::Vector(converted_vec))
                    }
                    _ => Ok(val)
                }
            }
            Expr::FormatSig { expr, spec } => {
                let val = self.eval_expr(expr)?;
                let spec_trim = spec.trim();
                if let Ok(sig_figs) = spec_trim.parse::<i32>() {
                    match val {
                        PhsValue::Number(n) => {
                            if n == 0.0 {
                                return Ok(PhsValue::Number(0.0));
                            }
                            let d = n.abs().log10().ceil() as i32;
                            let power = sig_figs - d;
                            let magnitude = 10_f64.powi(power);
                            Ok(PhsValue::Number((n * magnitude).round() / magnitude))
                        }
                        PhsValue::Quantity(q) => {
                            let n = q.value.mean();
                            if n == 0.0 {
                                return Ok(PhsValue::Quantity(q));
                            }
                            let d = n.abs().log10().ceil() as i32;
                            let power = sig_figs - d;
                            let magnitude = 10_f64.powi(power);
                            let rounded = (n * magnitude).round() / magnitude;
                            use physure_core::quantity::Quantity;
                            let rounded_q = Quantity::new_scalar(
                                rounded,
                                0.0,
                                q.unit.clone(),
                                None,
                                None,
                            );
                            Ok(PhsValue::Quantity(rounded_q))
                        }
                        _ => Ok(val)
                    }
                } else if spec_trim == "base" || spec_trim == "si" {
                    match val {
                        PhsValue::Quantity(q) => Ok(PhsValue::String(format!("{} {}", physure_core::quantity::format_float(q.value.mean()), q.unit.base_repr()))),
                        _ => Ok(val)
                    }
                } else if spec_trim.ends_with('e') {
                    let num_part = spec_trim.trim_end_matches('e').trim_start_matches('.').trim_start_matches("0.");
                    if let Ok(prec) = num_part.parse::<usize>() {
                        match val {
                            PhsValue::Number(n) => Ok(PhsValue::String(format!("{:.*e}", prec, n))),
                            PhsValue::Quantity(q) => Ok(PhsValue::String(format!("{:.*e} {}", prec, q.value.mean(), q.unit.__repr__()))),
                            _ => Ok(val)
                        }
                    } else {
                        Ok(val)
                    }
                } else if spec_trim == "frac" {
                    match val {
                        PhsValue::Number(n) => Ok(PhsValue::String(format!("{}/1", n as i64))),
                        PhsValue::Quantity(q) => Ok(PhsValue::String(format!("{}/1 {}", q.value.mean() as i64, q.unit.__repr__()))),
                        _ => Ok(val)
                    }
                } else if spec_trim == "base" || spec_trim == "si" {
                    match val {
                        PhsValue::Quantity(q) => Ok(PhsValue::String(format!("{} {}", physure_core::quantity::format_float(q.value.mean()), q.unit.base_repr()))),
                        _ => Ok(val)
                    }
                } else {
                    Ok(val)
                }
            }
            Expr::Unary { op, expr } => {
                let val = self.eval_expr(expr)?;
                match (op, val) {
                    (UnaryOp::Neg, PhsValue::Number(n)) => Ok(PhsValue::Number(-n)),
                    (UnaryOp::Sqrt, PhsValue::Number(n)) => Ok(PhsValue::Number(n.sqrt())),
                    (UnaryOp::Sqrt, PhsValue::Quantity(q)) => Ok(PhsValue::Quantity(q.sqrt()?)),
                    _ => Err(PhysureError::Generic("Unsupported unary operation".into())),
                }
            }
            Expr::Binary { op, left, right } => {
                let l_val = self.eval_expr(left)?;
                let r_val = self.eval_expr(right)?;
                self.eval_binary_op(op, &l_val, &r_val)
            }
            Expr::ImplicitMul { left, right } => {
                let l_val = self.eval_expr(left)?;
                let r_val = self.eval_expr(right)?;
                self.eval_binary_op(&BinaryOp::Mul, &l_val, &r_val)
            }
            Expr::Call { name, args } => {
                let evaluated_args: PhysureResult<Vec<PhsValue>> =
                    args.iter().map(|arg| self.eval_expr(arg)).collect();
                let evaluated_args = evaluated_args?;

                if let Some(builtin_res) = eval_builtin(name, &evaluated_args, self)? {
                    return Ok(builtin_res);
                }

                if let Some(custom_fn) = self.custom_functions.get(name).cloned() {
                    return custom_fn(&evaluated_args, self);
                }

                if let Some(user_fn) = self.functions.get(name).cloned() {
                    let recursion_limit = match self.env.get("mkml_recursion_limit") {
                        Some(PhsValue::Number(n)) => *n as usize,
                        _ => 1000,
                    };
                    if self.recursion_depth >= recursion_limit {
                        return Err(PhysureError::Generic("Recursion limit exceeded".into()));
                    }

                    if user_fn.params.len() != evaluated_args.len() {
                        return Err(PhysureError::Generic(format!(
                            "Function '{}' expects {} arguments, got {}",
                            name,
                            user_fn.params.len(),
                            evaluated_args.len()
                        )));
                    }
                    let mut local_interpreter = PhsInterpreter {
                        env: self.env.clone(),
                        functions: self.functions.clone(),
                        custom_functions: self.custom_functions.clone(),
                        registry: self.registry.clone(),
                        recursion_depth: self.recursion_depth + 1,
                    };
                    for (param, arg_val) in user_fn.params.iter().zip(evaluated_args) {
                        if let Some(ref unit_str) = param.unit {
                            let parsed_unit = physure_core::units::parser::Parser::parse_expression_with_registry(unit_str, &self.registry)?;
                            match &arg_val {
                                PhsValue::Quantity(q) => {
                                    if !q.unit.same_dimensions(&parsed_unit) {
                                        return Err(PhysureError::Generic(format!("Argument for '{}' must be a quantity with compatible units", param.name)));
                                    }
                                }
                                _ => return Err(PhysureError::Generic(format!("Argument for '{}' must be a quantity with compatible units", param.name))),
                            }
                        }
                        local_interpreter.env.insert(param.name.clone(), arg_val);
                    }
                    let mut last_val = PhsValue::None;
                    for stmt in &user_fn.body {
                        last_val = local_interpreter.run_statement(stmt)?;
                    }
                    return Ok(last_val);
                }

                Err(PhysureError::Generic(format!("Unknown function '{}'", name)))
            }
            Expr::Ternary {
                cond,
                then_expr,
                else_expr,
            } => {
                let cond_val = self.eval_expr(cond)?;
                if is_truthy(&cond_val) {
                    self.eval_expr(then_expr)
                } else {
                    self.eval_expr(else_expr)
                }
            }
            Expr::Let { name, val, body } => {
                let evaluated_val = self.eval_expr(val)?;
                let mut local_env = self.env.clone();
                local_env.insert(name.clone(), evaluated_val);
                let mut local_interpreter = PhsInterpreter {
                    env: local_env,
                    functions: self.functions.clone(),
                    custom_functions: self.custom_functions.clone(),
                    registry: self.registry.clone(),
                    recursion_depth: self.recursion_depth,
                };
                local_interpreter.eval_expr(body)
            }
            Expr::If {
                cond,
                then_expr,
                else_expr,
            } => {
                let cond_val = self.eval_expr(cond)?;
                if is_truthy(&cond_val) {
                    self.eval_expr(then_expr)
                } else {
                    self.eval_expr(else_expr)
                }
            }
            Expr::Vector(items) => {
                let evaluated: PhysureResult<Vec<PhsValue>> =
                    items.iter().map(|item| self.eval_expr(item)).collect();
                Ok(PhsValue::Vector(evaluated?))
            }
            Expr::Uncertainty { val, unc } => {
                let mean_val = self.eval_expr(val)?;
                let unc_val = self.eval_expr(unc)?;
                let sigma_k = match &unc_val {
                    PhsValue::Sigma(k) => Some(*k),
                    PhsValue::Quantity(q) if q.unit.__repr__() == "sigma" || q.unit.__repr__() == "σ" => Some(q.value.mean()),
                    _ => None,
                };
                if let Some(k) = sigma_k {
                    if let PhsValue::Quantity(q) = mean_val {
                        return Ok(PhsValue::SigmaBound(q, k));
                    }
                }
                let (mean, unit) = match mean_val {
                    PhsValue::Number(n) => (n, physure_core::units::RationalUnit::dimensionless()),
                    PhsValue::Quantity(q) => (q.value.mean(), q.unit),
                    _ => (0.0, physure_core::units::RationalUnit::dimensionless()),
                };
                let std_dev = match unc_val {
                    PhsValue::Number(n) => n,
                    PhsValue::Quantity(q) => q.value.mean(),
                    _ => 0.0,
                };
                use physure_core::quantity::Quantity;
                Ok(PhsValue::Quantity(Quantity::new_scalar(mean, std_dev, unit, None, None)))
            }
        }
    }

    pub fn eval_binary_op(
        &mut self,
        op: &BinaryOp,
        l_val: &PhsValue,
        r_val: &PhsValue,
    ) -> PhysureResult<PhsValue> {
        use physure_core::quantity::Quantity;

        let l_q = match l_val {
            PhsValue::String(s) => {
                if let Ok(unit) = physure_core::units::parser::Parser::parse_expression(s) {
                    Some(Quantity::new_scalar(1.0, 0.0, unit, None, None))
                } else {
                    None
                }
            }
            _ => None,
        };
        let r_q = match r_val {
            PhsValue::String(s) => {
                if let Ok(unit) = physure_core::units::parser::Parser::parse_expression(s) {
                    Some(Quantity::new_scalar(1.0, 0.0, unit, None, None))
                } else {
                    None
                }
            }
            _ => None,
        };

        if l_q.is_some() || r_q.is_some() {
            let left_v = l_q.map(PhsValue::Quantity).unwrap_or_else(|| l_val.clone());
            let right_v = r_q.map(PhsValue::Quantity).unwrap_or_else(|| r_val.clone());
            return self.eval_binary_op(op, &left_v, &right_v);
        }

        match (op, l_val, r_val) {
            // Number op Number
            (BinaryOp::Add, PhsValue::Number(a), PhsValue::Number(b)) => Ok(PhsValue::Number(a + b)),
            (BinaryOp::Sub, PhsValue::Number(a), PhsValue::Number(b)) => Ok(PhsValue::Number(a - b)),
            (BinaryOp::Mul, PhsValue::Number(a), PhsValue::Number(b)) => Ok(PhsValue::Number(a * b)),
            (BinaryOp::Div, PhsValue::Number(a), PhsValue::Number(b)) => {
                if *b == 0.0 {
                    Err(PhysureError::Generic("Division by zero".into()))
                } else {
                    Ok(PhsValue::Number(a / b))
                }
            }
            (BinaryOp::Pow, PhsValue::Number(a), PhsValue::Number(b)) => Ok(PhsValue::Number(a.powf(*b))),
            (BinaryOp::Eq, PhsValue::Number(a), PhsValue::Number(b)) => Ok(PhsValue::Bool((a - b).abs() < 1e-9)),
            (BinaryOp::Neq, PhsValue::Number(a), PhsValue::Number(b)) => Ok(PhsValue::Bool((a - b).abs() >= 1e-9)),
            (BinaryOp::Lt, PhsValue::Number(a), PhsValue::Number(b)) => Ok(PhsValue::Bool(a < b)),
            (BinaryOp::Gt, PhsValue::Number(a), PhsValue::Number(b)) => Ok(PhsValue::Bool(a > b)),
            (BinaryOp::Lte, PhsValue::Number(a), PhsValue::Number(b)) => Ok(PhsValue::Bool(a <= b)),
            (BinaryOp::Gte, PhsValue::Number(a), PhsValue::Number(b)) => Ok(PhsValue::Bool(a >= b)),
            (BinaryOp::ApproxEq, PhsValue::Number(a), PhsValue::Number(b)) => {
                let diff = (a - b).abs();
                let tol = 1e-5_f64.max(0.1 * a.abs().max(b.abs()));
                Ok(PhsValue::Bool(diff <= tol))
            }
            (BinaryOp::ApproxEq, PhsValue::Quantity(a), PhsValue::Quantity(b)) => Ok(PhsValue::Bool(a.approx_eq(b, 0.1, 1e-5))),

            // Quantity op Quantity
            (BinaryOp::Add, PhsValue::Quantity(a), PhsValue::Quantity(b)) => Ok(PhsValue::Quantity(a.add(b)?)),
            (BinaryOp::Sub, PhsValue::Quantity(a), PhsValue::Quantity(b)) => Ok(PhsValue::Quantity(a.sub(b)?)),
            (BinaryOp::Mul, PhsValue::Quantity(a), PhsValue::Quantity(b)) => Ok(PhsValue::Quantity(a.mul(b)?)),
            (BinaryOp::Div, PhsValue::Quantity(a), PhsValue::Quantity(b)) => Ok(PhsValue::Quantity(a.div(b)?)),
            (BinaryOp::Pow, PhsValue::Quantity(a), PhsValue::Number(b)) => Ok(PhsValue::Quantity(a.pow(*b)?)),
            (BinaryOp::Gt, PhsValue::Quantity(a), PhsValue::Quantity(b)) => Ok(PhsValue::Bool(a.value.mean() > b.value.mean())),
            (BinaryOp::Lt, PhsValue::Quantity(a), PhsValue::Quantity(b)) => Ok(PhsValue::Bool(a.value.mean() < b.value.mean())),
            (BinaryOp::Gte, PhsValue::Quantity(a), PhsValue::Quantity(b)) => Ok(PhsValue::Bool(a.value.mean() >= b.value.mean())),
            (BinaryOp::Lte, PhsValue::Quantity(a), PhsValue::Quantity(b)) => Ok(PhsValue::Bool(a.value.mean() <= b.value.mean())),

            (BinaryOp::Mul, PhsValue::Number(n), PhsValue::String(s)) if s == "sigma" || s == "σ" => {
                Ok(PhsValue::Sigma(*n))
            }
            (BinaryOp::Eq, PhsValue::Quantity(a), PhsValue::SigmaBound(b, k)) => {
                let diff = (a.value.mean() - b.value.mean()).abs();
                let std_dev = a.value.std_dev().max(b.value.std_dev());
                let bound = if std_dev > 0.0 { k * std_dev } else { 1e-5 };
                Ok(PhsValue::Bool(diff <= bound))
            }
            (BinaryOp::Eq, PhsValue::SigmaBound(b, k), PhsValue::Quantity(a)) => {
                let diff = (a.value.mean() - b.value.mean()).abs();
                let std_dev = a.value.std_dev().max(b.value.std_dev());
                let bound = if std_dev > 0.0 { k * std_dev } else { 1e-5 };
                Ok(PhsValue::Bool(diff <= bound))
            }
            (BinaryOp::Mul, PhsValue::Number(n), PhsValue::Quantity(q)) => {
                let scaled = Quantity::new_scalar(n * q.value.mean(), 0.0, q.unit.clone(), None, None);
                Ok(PhsValue::Quantity(scaled))
            }
            (BinaryOp::Mul, PhsValue::Quantity(q), PhsValue::Number(n)) => {
                let scaled = Quantity::new_scalar(n * q.value.mean(), 0.0, q.unit.clone(), None, None);
                Ok(PhsValue::Quantity(scaled))
            }
            (BinaryOp::Div, PhsValue::Quantity(q), PhsValue::Number(n)) => {
                if *n == 0.0 {
                    Err(PhysureError::Generic("Division by zero".into()))
                } else {
                    let scaled = Quantity::new_scalar(q.value.mean() / n, 0.0, q.unit.clone(), None, None);
                    Ok(PhsValue::Quantity(scaled))
                }
            }

            // Vector operations (element-wise)
            (op, PhsValue::Vector(v1), PhsValue::Vector(v2)) if v1.len() == v2.len() => {
                let res: PhysureResult<Vec<PhsValue>> = v1.iter().zip(v2.iter()).map(|(a, b)| self.eval_binary_op(op, a, b)).collect();
                Ok(PhsValue::Vector(res?))
            }
            (op, PhsValue::Vector(vec), other) => {
                let res: PhysureResult<Vec<PhsValue>> = vec.iter().map(|item| self.eval_binary_op(op, item, other)).collect();
                Ok(PhsValue::Vector(res?))
            }
            (op, other, PhsValue::Vector(vec)) => {
                let res: PhysureResult<Vec<PhsValue>> = vec.iter().map(|item| self.eval_binary_op(op, other, item)).collect();
                Ok(PhsValue::Vector(res?))
            }

            _ => Err(PhysureError::Generic("Operation not implemented for types".into())),
        }
    }

    fn try_build_fn_algebra(&self, _new_name: &str, expr: &Expr) -> PhysureResult<Option<UserFn>> {
        match expr {
            Expr::Binary { op, left, right } => {
                if let (Expr::Ident(left_id), Expr::Ident(right_id)) = (&**left, &**right) {
                    if self.functions.contains_key(left_id) && self.functions.contains_key(right_id) {
                        let left_params = self.get_fn_params(left_id).unwrap_or_default();
                        let right_params = self.get_fn_params(right_id).unwrap_or_default();
                        
                        let mut combined_names = left_params.clone();
                        for p in &right_params {
                            if !combined_names.contains(p) {
                                combined_names.push(p.clone());
                            }
                        }
                        
                        let params: Vec<ParamDef> = combined_names.iter().map(|p| ParamDef { name: p.clone(), unit: None }).collect();
                        
                        let left_args: Vec<Expr> = left_params.iter().map(|p| Expr::Ident(p.clone())).collect();
                        let right_args: Vec<Expr> = right_params.iter().map(|p| Expr::Ident(p.clone())).collect();
                        
                        let left_call = Expr::Call { name: left_id.clone(), args: left_args };
                        let right_call = Expr::Call { name: right_id.clone(), args: right_args };
                        
                        let body_expr = Expr::Binary {
                            op: op.clone(),
                            left: Box::new(left_call),
                            right: Box::new(right_call),
                        };
                        
                        let body = vec![Statement::ExprStmt(body_expr)];
                        
                        return Ok(Some(UserFn { params, body }));
                    }
                }
            }
            Expr::Call { name: f_id, args } => {
                if args.len() == 1 {
                    if let Expr::Ident(g_id) = &args[0] {
                        if self.functions.contains_key(f_id) && self.functions.contains_key(g_id) {
                            let params_f = self.get_fn_params(f_id).unwrap_or_default();
                            let params_g = self.get_fn_params(g_id).unwrap_or_default();
                            
                            if params_f.is_empty() {
                                return Err(PhysureError::Generic("Outer function must have at least one parameter.".into()));
                            }
                            
                            let mut combined_names = params_g.clone();
                            for p in &params_f[1..] {
                                if !combined_names.contains(p) {
                                    combined_names.push(p.clone());
                                }
                            }
                            
                            let params: Vec<ParamDef> = combined_names.iter().map(|p| ParamDef { name: p.clone(), unit: None }).collect();
                            
                            let g_args: Vec<Expr> = params_g.iter().map(|p| Expr::Ident(p.clone())).collect();
                            let g_call = Expr::Call { name: g_id.clone(), args: g_args };
                            
                            let mut f_args = vec![g_call];
                            for p in &params_f[1..] {
                                f_args.push(Expr::Ident(p.clone()));
                            }
                            
                            let body_expr = Expr::Call { name: f_id.clone(), args: f_args };
                            let body = vec![Statement::ExprStmt(body_expr)];
                            
                            return Ok(Some(UserFn { params, body }));
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(None)
    }
}

// Internal helper for ident handling in interpreter
impl PhsValue {
    fn ident(s: String) -> Self {
        PhsValue::String(s)
    }
}

fn is_truthy(val: &PhsValue) -> bool {
    match val {
        PhsValue::Bool(b) => *b,
        PhsValue::Number(n) => *n != 0.0,
        PhsValue::None => false,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_custom_fn_registration() {
        let mut interp = PhsInterpreter::new();
        interp.register_fn("custom_double", |args, _| {
            if let Some(PhsValue::Number(n)) = args.first() {
                Ok(PhsValue::Number(n * 2.0))
            } else {
                Err(PhysureError::Generic("Expected number".into()))
            }
        });

        let stmt = crate::parse_phs("custom_double(21)").unwrap().remove(0);
        let res = interp.run_statement(&stmt).unwrap();
        assert_eq!(res, PhsValue::Number(42.0));
    }

    #[test]
    fn test_function_algebra_phs() {
        let mut interp = PhsInterpreter::new();
        
        // Define f and g
        let f_stmt = crate::parse_phs("f(x) = 2 * x").unwrap().remove(0);
        let g_stmt = crate::parse_phs("g(x) = 3 * x").unwrap().remove(0);
        interp.run_statement(&f_stmt).unwrap();
        interp.run_statement(&g_stmt).unwrap();
        
        // h = f + g
        let h_stmt = crate::parse_phs("h = f + g").unwrap().remove(0);
        interp.run_statement(&h_stmt).unwrap();
        
        // Call h(2)
        let call_stmt = crate::parse_phs("h(2)").unwrap().remove(0);
        let res = interp.run_statement(&call_stmt).unwrap();
        assert_eq!(res, PhsValue::Number(10.0));
        
        // c = f(g)
        let c_stmt = crate::parse_phs("c = f(g)").unwrap().remove(0);
        interp.run_statement(&c_stmt).unwrap();
        
        // Call c(2)
        let call_c_stmt = crate::parse_phs("c(2)").unwrap().remove(0);
        let res_c = interp.run_statement(&call_c_stmt).unwrap();
        assert_eq!(res_c, PhsValue::Number(12.0));
    }
}
