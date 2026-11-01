use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use physure_core::error::{PhysureError, PhysureResult};
use physure_core::quantity::Quantity;
use physure_core::units::parser::Parser as UnitParser;
use physure_core::units::RationalUnit;
use crate::ast::{BinaryOp, Expr, Statement};
use crate::debug::StackFrame;
use crate::symbolic::Node;
use crate::value::PhsValue;
use super::{PhsInterpreter, coerce_equation_string};
use super::helpers::{is_truthy, make_range, strip_unit_comment};

struct CallStackGuard {
    call_stack: Arc<Mutex<Vec<StackFrame>>>,
}

impl Drop for CallStackGuard {
    fn drop(&mut self) {
        self.call_stack.lock().unwrap_or_else(|e| e.into_inner()).pop();
    }
}


fn eval_template_string(text: &str, interp: &PhsInterpreter, env: &HashMap<String, PhsValue>) -> String {
    interpolate(text.trim_matches('`').trim(), interp, env)
}

/// Substitutes every `{expr}` in `text` with the value of `expr` in `env`, leaving the
/// braces untouched when the expression does not evaluate. Unlike `eval_template_string`
/// this keeps the surrounding whitespace, which a quoted string literal is entitled to.
fn interpolate(text: &str, interp: &PhsInterpreter, env: &HashMap<String, PhsValue>) -> String {
    let mut result = String::new();
    let mut rest = text;

    while let Some(start) = rest.find('{') {
        result.push_str(&rest[..start]);
        rest = &rest[start + 1..];
        let Some(end) = rest.find('}') else {
            result.push('{');
            break;
        };
        let expr_str = rest[..end].trim();
        rest = &rest[end + 1..];
        match evaluate_span(expr_str, interp, env) {
            Some(text) => result.push_str(&text),
            None => {
                result.push('{');
                result.push_str(expr_str);
                result.push('}');
            }
        }
    }
    result.push_str(rest);
    result
}

/// The text of one `{expr}` span's value, or `None` when it does not parse or evaluate.
fn evaluate_span(expr_str: &str, interp: &PhsInterpreter, env: &HashMap<String, PhsValue>) -> Option<String> {
    let prog = crate::parse_phs(expr_str).ok()?;
    let stmt = prog.statements.first()?;
    interp.eval_statement_with_env(stmt, &mut env.clone()).ok().map(|val| val.to_string())
}

fn unit_symbol_as_quantity(name: &str) -> Option<Quantity> {
    if !crate::parser::is_known_unit_symbol(name) {
        return None;
    }
    let unit = UnitParser::parse_expression(name).ok()?;
    Some(Quantity::new_scalar(1.0, 0.0, unit, None, None))
}


/// A quantity literal, with the unit and uncertainty it was written with.
fn eval_quantity_literal(node: &crate::ast::QuantityNode) -> PhysureResult<PhsValue> {
    if let Some(reason) = node.asymmetric_refusal() {
        return Err(PhysureError::Generic(reason));
    }
    let clean_unit_str = node.unit.as_deref().map_or("", strip_unit_comment);
    let unit = if clean_unit_str.is_empty() {
        RationalUnit::dimensionless()
    } else {
        UnitParser::parse_expression(clean_unit_str)?
    };
    let q = Quantity::new_scalar(node.magnitude, node.uncertainty.unwrap_or(0.0), unit, None, None);
    if node.is_sigma {
        Ok(PhsValue::SigmaBound(q, node.uncertainty.unwrap_or(1.0)))
    } else {
        Ok(PhsValue::Quantity(q))
    }
}

/// Which side of an equation a call with named arguments evaluates: the one the arguments leave fully bound.
/// Algebra (e.g. multiplying both sides) can move the unknown to either side of the equation, so try whichever
/// side is fully bound by the supplied kwargs rather than assuming it's the RHS.
fn solved_side<'a>(name: &str, lhs: &'a Node, rhs: &'a Node, local_env: &HashMap<String, PhsValue>) -> PhysureResult<&'a Node> {
    let unbound = |s: &&String| !local_env.contains_key(*s);
    let mut rhs_free = std::collections::HashSet::new();
    rhs.free_symbols(&mut rhs_free);
    let rhs_missing: Vec<&String> = rhs_free.iter().filter(unbound).collect();
    if rhs_missing.is_empty() {
        return Ok(rhs);
    }
    let mut lhs_free = std::collections::HashSet::new();
    lhs.free_symbols(&mut lhs_free);
    let lhs_missing: Vec<&String> = lhs_free.iter().filter(unbound).collect();
    if lhs_missing.is_empty() {
        return Ok(lhs);
    }
    // Neither side is fully bound by name alone, which is what a unit in
    // the equation text looks like: `"Q = m * 4.18 J/(g*K) * (T2 - T1)"`
    // leaves J, g and K free on the right and the unknown Q on the left.
    // Ignoring unit symbols separates the two, and only here — after both
    // strict passes have failed — so a unit-named unknown (solving for
    // `V`, `T`, `A`) is still picked up by the passes above.
    if rhs_missing.iter().all(|s| crate::parser::is_known_unit_symbol(s)) {
        return Ok(rhs);
    }
    if lhs_missing.iter().all(|s| crate::parser::is_known_unit_symbol(s)) {
        return Ok(lhs);
    }
    let missing = if rhs_missing.len() <= lhs_missing.len() { rhs_missing } else { lhs_missing };
    Err(PhysureError::Generic(format!(
        "Missing argument(s) for equation '{}': {}",
        name,
        missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
    )))
}

/// The elements a `for` expression runs over: a vector's own, or a range's whole numbers.
fn iteration_items(iterable: PhsValue) -> PhysureResult<Vec<PhsValue>> {
    match iterable {
        PhsValue::Vector(v) => Ok(v),
        PhsValue::Range(start, end) => range_items(&start, &end),
        other => Err(PhysureError::Generic(format!("Cannot iterate over {}", other))),
    }
}

fn range_items(start: &PhsValue, end: &PhsValue) -> PhysureResult<Vec<PhsValue>> {
    let (start_num, unit) = match start {
        PhsValue::Number(n) => (*n, None),
        PhsValue::Quantity(q) => {
            let u = if q.unit.dimensions.is_empty() {
                None
            } else {
                Some(q.unit.clone())
            };
            (q.value.mean(), u)
        }
        _ => return Err(PhysureError::Generic("Range start must be a number or quantity".into())),
    };
    let end_num = match end {
        PhsValue::Number(n) => *n,
        PhsValue::Quantity(q) => q.value.mean(),
        _ => return Err(PhysureError::Generic("Range end must be a number or quantity".into())),
    };
    let start_i = start_num as i64;
    let end_i = end_num as i64;
    let count = end_i.saturating_sub(start_i).max(0) as usize;
    let ceiling = physure_core::settings::max_loop_iterations();
    if count > ceiling {
        return Err(PhysureError::Generic(format!(
            "for-loop range `{start_i}..{end_i}` would materialize {count} elements, exceeding the max_loop_iterations ceiling of {ceiling}; raise `max_loop_iterations` in physure.conf's [Settings] section if this is a legitimate workload"
        )));
    }
    Ok((start_i..end_i)
        .map(|i| match &unit {
            Some(u) => PhsValue::Quantity(Quantity::new_scalar(i as f64, 0.0, u.clone(), None, None)),
            None => PhsValue::Number(i as f64),
        })
        .collect())
}

/// `f'(x)` where `f` is a string holding an expression: the derivative, as text, by the variable of the call.
fn derivative_of_text(expr_str: &str, order: usize, args: &[Expr]) -> PhysureResult<PhsValue> {
    let var_name = match args.first() {
        Some(Expr::Identifier(v)) => v.as_str(),
        _ => "x",
    };
    let node = crate::symbolic::SymbolicParser::parse_str(expr_str)?;
    let diff_node = node.diff_node_n(var_name, order)?;
    Ok(PhsValue::String(diff_node.to_string()))
}

impl PhsInterpreter {
    pub fn eval_expr(&self, expr: &Expr, env: &HashMap<String, PhsValue>) -> PhysureResult<PhsValue> {
        self.check_budget()?;
        match expr {
            Expr::Quantity(node) => eval_quantity_literal(node),
            Expr::Bool(value) => Ok(PhsValue::Bool(*value)),
            // A string literal is the text the user wrote, never a variable lookup: with
            // `v = 3 m/s` in scope, `deriv("0.5*m*v^2", "v")` used to receive the quantity
            // instead of the name. `{v}` folds a value in explicitly.
            Expr::Str(text) => Ok(PhsValue::String(interpolate(text, self, env))),
            Expr::Identifier(name) => Ok(self.eval_identifier(name, env)),
            Expr::BinaryOp { op, left, right } => self.eval_binary_expr(*op, left, right, env),
            Expr::FunctionCall { name, args, kwargs } => self.eval_call(name, args, kwargs, env),
            Expr::ForExpr { var, iterable, body } => self.eval_for(var, iterable, body, env),
        }
    }

    fn eval_identifier(&self, name: &str, env: &HashMap<String, PhsValue>) -> PhsValue {
        if name.starts_with('`') || (name.contains('{') && name.contains('}')) {
            PhsValue::String(eval_template_string(name, self, env))
        } else if let Some(val) = env.get(name) {
            val.clone()
        } else if let Some(unit) = unit_symbol_as_quantity(name) {
            // The symbolic layer has no notion of units: it parses `2.0 J / (gram * K)`
            // into plain algebra over the symbols J, gram and K. Re-evaluating such a
            // symbol as one of its unit reassembles the right dimensions, which is what
            // makes a unit-bearing equation string survive the round-trip. A binding in
            // `env` still wins, so this only ever fires for an otherwise-free name.
            PhsValue::Quantity(unit)
        } else {
            PhsValue::String(name.to_string())
        }
    }

    fn eval_binary_expr(&self, op: BinaryOp, left: &Expr, right: &Expr, env: &HashMap<String, PhsValue>) -> PhysureResult<PhsValue> {
        let l_val = self.eval_expr(left, env)?;
        if op == BinaryOp::Convert {
            let target_unit = crate::codegen::expr_to_unit_string(right);
            let clean_target = strip_unit_comment(&target_unit);
            if clean_target.is_empty() {
                return Ok(l_val);
            }
            let parsed_unit = UnitParser::parse_expression(clean_target)?;
            return self.convert_value_to_unit(l_val, &parsed_unit);
        }
        let r_val = self.eval_expr(right, env)?;
        self.eval_binary_op_vals(op, l_val, r_val)
    }

    fn eval_call(&self, name: &str, args: &[Expr], kwargs: &[(String, Expr)], env: &HashMap<String, PhsValue>) -> PhysureResult<PhsValue> {
        if name == "let" && args.len() == 3 {
            if let Expr::Identifier(var_name) = &args[0] {
                let val = self.eval_expr(&args[1], env)?;
                let mut local_env = env.clone();
                local_env.insert(var_name.clone(), val);
                return self.eval_expr(&args[2], &local_env);
            }
        }

        if let Some(PhsValue::Equation(lhs, rhs)) = env.get(name).cloned().map(coerce_equation_string) {
            return self.call_equation(name, &lhs, &rhs, args, kwargs, env);
        }

        if let Some(result) = self.eval_short_circuit(name, args, kwargs, env) {
            return result;
        }

        let mut arg_vals = Vec::new();
        for arg in args {
            arg_vals.push(self.eval_expr(arg, env)?);
        }

        let mut kwarg_vals = Vec::new();
        for (kw_name, kw_expr) in kwargs {
            let kw_val = self.eval_expr(kw_expr, env)?;
            kwarg_vals.push((kw_name.clone(), kw_val));
        }

        if let Some(composed) = self.compose_functions(name, args, kwargs, env) {
            return Ok(composed);
        }

        self.call_by_name(name, args, arg_vals, &kwarg_vals, env)
    }

    /// Calling an equation by name: `eq(x=1)` solves it for the one unknown the named arguments leave.
    fn call_equation(
        &self,
        name: &str,
        lhs: &Node,
        rhs: &Node,
        args: &[Expr],
        kwargs: &[(String, Expr)],
        env: &HashMap<String, PhsValue>,
    ) -> PhysureResult<PhsValue> {
        if !args.is_empty() {
            return Err(PhysureError::Generic(format!(
                "Calling equation '{}' requires named arguments only, e.g. {}(x=1), got positional arguments",
                name, name
            )));
        }
        if kwargs.is_empty() {
            return Err(PhysureError::Generic(format!(
                "Calling equation '{}' requires at least one named argument",
                name
            )));
        }
        let mut local_env = env.clone();
        for (kwarg_name, kwarg_expr) in kwargs {
            let val = self.eval_expr(kwarg_expr, env)?;
            local_env.insert(kwarg_name.clone(), val);
        }
        let solved_str = solved_side(name, lhs, rhs, &local_env)?.to_phs_string();
        let program = crate::parser::parse_phs(&solved_str)?;
        let Some(Statement::Expr(expr)) = program.statements.first() else {
            return Err(PhysureError::Generic(format!("Failed to evaluate equation '{}'", name)));
        };
        self.eval_expr(expr, &local_env)
    }

    /// Short-circuit logical operators: language semantics, not an optimization,
    /// so they must run before the eager argument evaluation of an ordinary call rather than as
    /// ordinary builtins. Strict Bool checking here (never `is_truthy`) is what
    /// keeps `5 m and enabled` a type error instead of a silent truthiness test.
    /// `None` when the call is not one of them.
    fn eval_short_circuit(
        &self,
        name: &str,
        args: &[Expr],
        kwargs: &[(String, Expr)],
        env: &HashMap<String, PhsValue>,
    ) -> Option<PhysureResult<PhsValue>> {
        if !kwargs.is_empty() {
            return None;
        }
        match (name, args) {
            ("op_not", [operand]) => Some(self.eval_not(operand, env)),
            ("op_and" | "op_or", [left, right]) => Some(self.eval_and_or(name == "op_and", left, right, env)),
            _ => None,
        }
    }

    fn eval_not(&self, operand: &Expr, env: &HashMap<String, PhsValue>) -> PhysureResult<PhsValue> {
        let operand = self.eval_expr(operand, env)?;
        let PhsValue::Bool(b) = operand else {
            return Err(PhysureError::Generic(format!(
                "`not` expects a Bool operand, got {}", operand.type_name()
            )));
        };
        Ok(PhsValue::Bool(!b))
    }

    fn eval_and_or(&self, is_and: bool, left: &Expr, right: &Expr, env: &HashMap<String, PhsValue>) -> PhysureResult<PhsValue> {
        let word = if is_and { "and" } else { "or" };
        let left = self.eval_expr(left, env)?;
        let PhsValue::Bool(left_b) = left else {
            return Err(PhysureError::Generic(format!(
                "`{}` expects Bool operands, left side was {}", word, left.type_name()
            )));
        };
        // `false and _` and `true or _` are decided by the left side alone
        if left_b != is_and {
            return Ok(PhsValue::Bool(left_b));
        }
        let right = self.eval_expr(right, env)?;
        let PhsValue::Bool(right_b) = right else {
            return Err(PhysureError::Generic(format!(
                "`{}` expects Bool operands, right side was {}", word, right.type_name()
            )));
        };
        Ok(PhsValue::Bool(right_b))
    }

    /// `f(g)` for a function `f` bound in `env` and a function value `g` is their composition `f_g`.
    fn compose_functions(
        &self,
        name: &str,
        args: &[Expr],
        kwargs: &[(String, Expr)],
        env: &HashMap<String, PhsValue>,
    ) -> Option<PhsValue> {
        let Some(PhsValue::Function(func)) = env.get(name) else {
            return None;
        };
        if args.len() != 1 || !kwargs.is_empty() {
            return None;
        }
        let Ok(PhsValue::Function(arg_func)) = self.eval_expr(&args[0], env) else {
            return None;
        };
        let params = arg_func.params.clone();
        let param_units = arg_func.param_units.clone();
        let inner_args: Vec<Expr> = params.iter().map(|p| Expr::Identifier(p.clone())).collect();
        let body = Statement::Expr(Expr::FunctionCall {
            name: func.name.clone(),
            args: vec![Expr::FunctionCall {
                name: arg_func.name.clone(),
                args: inner_args,
                kwargs: Vec::new(),
            }],
            kwargs: Vec::new(),
        });
        Some(PhsValue::Function(crate::ast::FunctionDefNode {
            name: format!("{}_{}", func.name, arg_func.name),
            params,
            param_units,
            body_stmts: vec![body],
            body_lines: vec![],
            decorators: Vec::new(),
            doc: None,
        }))
    }

    /// The call of `name` once its arguments are evaluated: a core builtin, an unlocked domain builtin, an
    /// external function, a user function, or the derivative of one (`f'`).
    fn call_by_name(
        &self,
        name: &str,
        args: &[Expr],
        arg_vals: Vec<PhsValue>,
        kwarg_vals: &[(String, PhsValue)],
        env: &HashMap<String, PhsValue>,
    ) -> PhysureResult<PhsValue> {
        if let Some(val) = crate::builtins::eval_core_builtin(name, &arg_vals, self, env)? {
            return Ok(val);
        }

        if let Some((domain, canonical)) = self.unlocked_builtins.lock().unwrap_or_else(|e| e.into_inner()).get(name).cloned() {
            if let Some(val) = crate::builtins::eval_domain_builtin_with_kwargs(domain, &canonical, &arg_vals, kwarg_vals, self, env)? {
                return Ok(val);
            }
        }

        let external = self.externals.get(name).cloned()
            .or_else(|| self.dynamic_externals.lock().unwrap_or_else(|e| e.into_inner()).get(name).cloned());
        if let Some(f) = external {
            return f(&arg_vals);
        }

        if let Some(PhsValue::Function(func)) = env.get(name) {
            return self.call_function_node(func, arg_vals, env);
        }

        if name.ends_with('\'') {
            let base_name = name.trim_end_matches('\'');
            let order = name.len() - base_name.len();
            if let Some(res) = self.eval_prime_function_call(base_name, order, &arg_vals, args, env)? {
                return Ok(res);
            }
        }

        Err(PhysureError::Generic(format!("Undefined function '{}'", name)))
    }

    fn eval_for(&self, var: &str, iterable: &Expr, body: &Expr, env: &HashMap<String, PhsValue>) -> PhysureResult<PhsValue> {
        let iterable_val = self.eval_expr(iterable, env)?;
        let items = iteration_items(iterable_val)?;

        // Switch to parallel evaluation if the iteration count meets the threshold.
        // Note: rayon's parallel collect stops scheduling new work on error but may
        // leave in-flight work on other threads to complete, so loop body side effects
        // (e.g. I/O in external functions) may partially execute even if evaluation fails.
        if items.len() >= physure_core::settings::parallel_threshold() && !self.debug_hook_is_set() && !self.limits.sequential {
            use rayon::prelude::*;
            let results: Vec<PhsValue> = items
                .into_par_iter()
                .map(|item| {
                    let mut local_env = env.clone();
                    local_env.insert(var.to_string(), item);
                    self.eval_expr(body, &local_env)
                })
                .collect::<PhysureResult<Vec<PhsValue>>>()?;
            return Ok(PhsValue::Vector(results));
        }
        let mut local_env = env.clone();
        let mut results = Vec::with_capacity(items.len());
        for item in items {
            local_env.insert(var.to_string(), item);
            results.push(self.eval_expr(body, &local_env)?);
        }
        Ok(PhsValue::Vector(results))
    }

    fn eval_prime_function_call(
        &self,
        base_name: &str,
        order: usize,
        arg_vals: &[PhsValue],
        args: &[crate::ast::Expr],
        env: &HashMap<String, PhsValue>,
    ) -> PhysureResult<Option<PhsValue>> {
        match env.get(base_name) {
            Some(PhsValue::Function(func)) => self.derivative_of_function(func, base_name, order, arg_vals, env).map(Some),
            Some(PhsValue::String(expr_str)) => derivative_of_text(expr_str, order, args).map(Some),
            _ => Ok(None),
        }
    }

    /// `f'(x)` for a user function `f`: its derivative, evaluated at `x` when `x` is a number or a quantity.
    fn derivative_of_function(
        &self,
        func: &crate::ast::FunctionDefNode,
        base_name: &str,
        order: usize,
        arg_vals: &[PhsValue],
        env: &HashMap<String, PhsValue>,
    ) -> PhysureResult<PhsValue> {
        let Some(var_name) = func.params.first() else {
            return Err(PhysureError::Generic(format!("Function {} has no parameters to differentiate", base_name)));
        };
        let mut body_str = String::new();
        for stmt in &func.body_stmts {
            if let Statement::Return(expr) | Statement::Expr(expr) = stmt {
                body_str = crate::codegen::expr_to_phs_string(expr);
            }
        }
        if body_str.is_empty() {
            return Err(PhysureError::Generic(format!("Cannot extract expression for function {}", base_name)));
        }
        let node = crate::symbolic::SymbolicParser::parse_str(&body_str)?;
        let diff_node = node.diff_node_n(var_name, order)?;

        if let Some(point @ (PhsValue::Number(_) | PhsValue::Quantity(_))) = arg_vals.first() {
            let mut local_env = env.clone();
            local_env.insert(var_name.clone(), point.clone());
            let expr_node = crate::parser::parse_phs(&diff_node.to_phs_string())?;
            if let Some(Statement::Expr(e)) = expr_node.statements.first() {
                return self.eval_expr(e, &local_env);
            }
        }
        Ok(PhsValue::String(diff_node.to_string()))
    }

    /// `call_site_line` is always `0` here -- `Expr` carries no line numbers (only `Statement`
    /// does), and this is the only call path `Expr::FunctionCall` reaches, so there is no real
    /// line to pass. This is a known, accepted v1 limitation, not a bug: it means
    /// `StackFrame::call_site_line` (and therefore the CLI debugger's "called from line N" and
    /// `backtrace` output) always reads `0` for every call, for every debug session, today.
    /// Expression-level call-site precision is out of scope for LAB-READY.
    pub fn call_function_node(
        &self,
        func: &crate::ast::FunctionDefNode,
        arg_vals: Vec<PhsValue>,
        env: &HashMap<String, PhsValue>,
    ) -> PhysureResult<PhsValue> {
        self.call_function_node_at(func, arg_vals, env, 0)
    }

    fn call_function_node_at(
        &self,
        func: &crate::ast::FunctionDefNode,
        arg_vals: Vec<PhsValue>,
        env: &HashMap<String, PhsValue>,
        call_site_line: usize,
    ) -> PhysureResult<PhsValue> {
        if func.params.len() != arg_vals.len() {
            return Err(PhysureError::Generic(format!("Function {} expects {} args, got {}", func.name, func.params.len(), arg_vals.len())));
        }
        self.check_budget()?;
        let _depth = crate::limits::CallDepth::enter(self.limits.max_call_depth).map_err(PhysureError::Generic)?;
        let implicit_units = func.decorators.iter().any(|d| d.name == "implicit_units");
        let mut local_env = env.clone();
        for (i, (param_name, arg_val)) in func.params.iter().zip(arg_vals.into_iter()).enumerate() {
            let bound_val = self.bind_param_value(&func.name, param_name, func.param_units.get(i).and_then(|u| u.as_ref()), arg_val, implicit_units)?;
            local_env.insert(param_name.clone(), bound_val);
        }
        self.check_requires(func, &local_env)?;

        // `_stack_guard` pops the pushed `StackFrame` on every exit path from this point on --
        // normal completion, an early `break` from a `Return`/`GuardReturn` arm, or `?` error
        // propagation from anywhere in the body loop below (undefined function, contract
        // violation, unit mismatch, ...). Without this, a mid-body error would leave the frame
        // on `call_stack` forever: `PhsInterpreter` is long-lived (REPL, future DAP sessions),
        // and every enclosing call in a chain hits the same unguarded early return, so a deep
        // call stack would leak one frame per active call on every error.
        let _stack_guard = if self.debug_hook.is_some() {
            self.call_stack.lock().unwrap_or_else(|e| e.into_inner())
                .push(StackFrame::new(func, call_site_line));
            Some(CallStackGuard { call_stack: self.call_stack.clone() })
        } else {
            None
        };

        let mut last_val = PhsValue::None;
        for (i, stmt) in func.body_stmts.iter().enumerate() {
            let line = func.body_lines.get(i).copied().unwrap_or(0);
            match stmt {
                Statement::Return(expr) => {
                    self.debug_checkpoint(line, &local_env)?;
                    last_val = self.eval_expr(expr, &local_env)?;
                    break;
                }
                Statement::GuardReturn { cond, value } => {
                    self.debug_checkpoint(line, &local_env)?;
                    let cond_val = self.eval_expr(cond, &local_env)?;
                    if is_truthy(&cond_val) {
                        last_val = self.eval_expr(value, &local_env)?;
                        break;
                    }
                }
                _ => {
                    last_val = self.eval_statement_with_env_at(stmt, &mut local_env, line)?;
                }
            }
        }

        self.check_ensures(func, &local_env, &last_val)?;
        Ok(last_val)
    }

    /// Evaluates every `@requires` condition against the already-bound parameters,
    /// erroring on the first one that is not truthy. Conditions are ordinary `Expr`s —
    /// a comparison like `m > 0.0` is a `FunctionCall { name: "op_>", .. }` under the
    /// hood, so this needs no evaluator support beyond `eval_expr`/`is_truthy`.
    ///
    /// Note: conditions must evaluate to a numeric/`Quantity` truthy value (as produced by
    /// comparison operators like `op_>`); a plugin-provided `PhsValue::Bool` is currently
    /// always treated as falsy by `is_truthy`, so boolean-returning plugin predicates are
    /// not yet safe to use directly as `@requires`/`@ensures` conditions.
    fn check_requires(&self, func: &crate::ast::FunctionDefNode, local_env: &HashMap<String, PhsValue>) -> PhysureResult<()> {
        for dec in &func.decorators {
            if dec.name == "requires" {
                let cond = &dec.args[0];
                if !is_truthy(&self.eval_expr(cond, local_env)?) {
                    let message = self.eval_expr(&dec.args[1], local_env)?.to_string();
                    return Err(PhysureError::ContractViolation { decorator: "requires".to_string(), message });
                }
            }
        }
        Ok(())
    }

    /// Evaluates every `@ensures` condition with `result` bound to the function's
    /// return value. `validate_decorators` (Task 5) already rejects `@ensures` on any
    /// function with a parameter literally named `result`, so this insert can never
    /// silently shadow a caller-visible binding.
    ///
    /// See `check_requires`'s note on `PhsValue::Bool`.
    fn check_ensures(&self, func: &crate::ast::FunctionDefNode, local_env: &HashMap<String, PhsValue>, result: &PhsValue) -> PhysureResult<()> {
        if !func.decorators.iter().any(|d| d.name == "ensures") {
            return Ok(());
        }
        let mut result_env = local_env.clone();
        result_env.insert("result".to_string(), result.clone());
        for dec in &func.decorators {
            if dec.name == "ensures" {
                let cond = &dec.args[0];
                if !is_truthy(&self.eval_expr(cond, &result_env)?) {
                    let message = self.eval_expr(&dec.args[1], &result_env)?.to_string();
                    return Err(PhysureError::ContractViolation { decorator: "ensures".to_string(), message });
                }
            }
        }
        Ok(())
    }

    /// Binds an argument value to a function parameter, converting it to the parameter's
    /// declared unit (if any) so that dimensionally-equivalent-but-differently-scaled
    /// arguments (e.g. `5 cm` passed to a `(r: m)` parameter) produce identical results
    /// regardless of which unit the caller used.
    ///
    /// - If the parameter has no declared unit, the argument is bound as-is (no conversion).
    /// - If the argument isn't a `Quantity`, it is bound as-is (nothing to convert).
    /// - If the argument's unit is dimensionally incompatible with the declared unit,
    ///   this returns a clear error rather than silently producing a wrong result.
    /// - If `implicit_units` is set (the function carries `@implicit_units`) and the
    ///   argument is a *plain* dimensionless quantity -- exactly `RationalUnit::dimensionless()`,
    ///   which a bare number like the `1` in `calc(1, 2, 3)` always evaluates to, as opposed
    ///   to a `%`/ppm-style ratio the caller already tagged with its own unit symbol -- the
    ///   declared unit is assigned to it rather than attempted as a conversion (which would
    ///   otherwise fail: dimensionless and `m/s2` are not the same dimension). A real
    ///   dimension mismatch (`5 kg` for an `m/s2` parameter) still errors exactly as before;
    ///   this only fills in a *missing* unit, it never overrides a wrong one.
    fn bind_param_value(
        &self,
        fn_name: &str,
        param_name: &str,
        declared_unit: Option<&String>,
        arg_val: PhsValue,
        implicit_units: bool,
    ) -> PhysureResult<PhsValue> {
        let Some(unit_str) = declared_unit else {
            return Ok(arg_val);
        };
        let PhsValue::Quantity(q) = arg_val else {
            return Ok(arg_val);
        };
        let clean_unit_str = strip_unit_comment(unit_str);
        if clean_unit_str.is_empty() {
            return Ok(PhsValue::Quantity(q));
        }
        let target_unit = UnitParser::parse_expression(clean_unit_str)?;
        if implicit_units && q.unit == RationalUnit::dimensionless() && target_unit != RationalUnit::dimensionless() {
            return Ok(PhsValue::Quantity(q.with_unit(target_unit)));
        }
        let converted = q.convert_to(&target_unit).map_err(|e| {
            PhysureError::Generic(format!(
                "Argument for parameter '{}' of function '{}' has a unit incompatible with declared unit '{}': {}",
                param_name, fn_name, clean_unit_str, e
            ))
        })?;
        Ok(PhsValue::Quantity(converted))
    }

    fn convert_value_to_unit(&self, val: PhsValue, unit: &RationalUnit) -> PhysureResult<PhsValue> {
        match val {
            PhsValue::Quantity(q) => Ok(PhsValue::Quantity(q.convert_to(unit)?)),
            PhsValue::Vector(vec) => {
                let mut results = Vec::new();
                for item in vec {
                    results.push(self.convert_value_to_unit(item, unit)?);
                }
                Ok(PhsValue::Vector(results))
            }
            // A range is its endpoints, so converting it converts both: `(0 m .. 100 m) => km`
            // is `0 km .. 0.1 km`. Without this arm it fell to the catch-all below and came
            // back as the metres it went in as, with nothing said about the `=> km`.
            PhsValue::Range(start, end) => make_range(
                self.convert_value_to_unit(*start, unit)?,
                self.convert_value_to_unit(*end, unit)?,
            ),
            other => Ok(other),
        }
    }

}
