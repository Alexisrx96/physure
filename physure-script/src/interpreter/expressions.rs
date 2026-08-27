use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use physure_core::error::{PhysureError, PhysureResult};
use physure_core::quantity::Quantity;
use physure_core::units::parser::Parser as UnitParser;
use physure_core::units::RationalUnit;
use crate::ast::{BinaryOp, Expr, Statement};
use crate::debug::StackFrame;
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
        if let Some(end) = rest.find('}') {
            let expr_str = rest[..end].trim();
            rest = &rest[end + 1..];

            if let Ok(prog) = crate::parse_phs(expr_str) {
                if let Some(stmt) = prog.statements.first() {
                    if let Ok(val) = interp.eval_statement_with_env(stmt, &mut env.clone()) {
                        result.push_str(&val.to_string());
                        continue;
                    }
                }
            }
            result.push('{');
            result.push_str(expr_str);
            result.push('}');
        } else {
            result.push('{');
            break;
        }
    }
    result.push_str(rest);
    result
}

fn unit_symbol_as_quantity(name: &str) -> Option<Quantity> {
    if !crate::parser::is_known_unit_symbol(name) {
        return None;
    }
    let unit = UnitParser::parse_expression(name).ok()?;
    Some(Quantity::new_scalar(1.0, 0.0, unit, None, None))
}


impl PhsInterpreter {
    pub fn eval_expr(&self, expr: &Expr, env: &HashMap<String, PhsValue>) -> PhysureResult<PhsValue> {
        match expr {
            Expr::Quantity(node) => {
                if let Some(reason) = node.asymmetric_refusal() {
                    return Err(PhysureError::Generic(reason));
                }
                let mut q = Quantity::new_scalar(node.magnitude, node.uncertainty.unwrap_or(0.0), RationalUnit::dimensionless(), None, None);
                if let Some(unit_str) = &node.unit {
                    let clean_unit_str = strip_unit_comment(unit_str);
                    if !clean_unit_str.is_empty() {
                        let parsed_unit = UnitParser::parse_expression(clean_unit_str)?;
                        q = Quantity::new_scalar(node.magnitude, node.uncertainty.unwrap_or(0.0), parsed_unit, None, None);
                    }
                }
                if node.is_sigma {
                    Ok(PhsValue::SigmaBound(q, node.uncertainty.unwrap_or(1.0)))
                } else {
                    Ok(PhsValue::Quantity(q))
                }
            }
            Expr::Bool(value) => Ok(PhsValue::Bool(*value)),
            // A string literal is the text the user wrote, never a variable lookup: with
            // `v = 3 m/s` in scope, `deriv("0.5*m*v^2", "v")` used to receive the quantity
            // instead of the name. `{v}` folds a value in explicitly.
            Expr::Str(text) => Ok(PhsValue::String(interpolate(text, self, env))),
            Expr::Identifier(name) => {
                if name.starts_with('`') || (name.contains('{') && name.contains('}')) {
                    let text = eval_template_string(name, self, env);
                    Ok(PhsValue::String(text))
                } else if let Some(val) = env.get(name) {
                    Ok(val.clone())
                } else if let Some(unit) = unit_symbol_as_quantity(name) {
                    // The symbolic layer has no notion of units: it parses `2.0 J / (gram * K)`
                    // into plain algebra over the symbols J, gram and K. Re-evaluating such a
                    // symbol as one of its unit reassembles the right dimensions, which is what
                    // makes a unit-bearing equation string survive the round-trip. A binding in
                    // `env` still wins, so this only ever fires for an otherwise-free name.
                    Ok(PhsValue::Quantity(unit))
                } else {
                    Ok(PhsValue::String(name.clone()))
                }
            }
            Expr::BinaryOp { op, left, right } => {
                if *op == BinaryOp::Convert {
                    let l_val = self.eval_expr(left, env)?;
                    let target_unit = crate::codegen::expr_to_unit_string(right);
                    let clean_target = strip_unit_comment(&target_unit);
                    if !clean_target.is_empty() {
                        let parsed_unit = UnitParser::parse_expression(clean_target)?;
                        return self.convert_value_to_unit(l_val, &parsed_unit);
                    }
                    return Ok(l_val);
                }
                let l_val = self.eval_expr(left, env)?;
                let r_val = self.eval_expr(right, env)?;
                self.eval_binary_op_vals(*op, l_val, r_val)
            }
            Expr::FunctionCall { name, args, kwargs } => {
                if name == "let" && args.len() == 3 {
                    if let Expr::Identifier(var_name) = &args[0] {
                        let val = self.eval_expr(&args[1], env)?;
                        let mut local_env = env.clone();
                        local_env.insert(var_name.clone(), val);
                        return self.eval_expr(&args[2], &local_env);
                    }
                }

                if let Some(PhsValue::Equation(lhs, rhs)) = env.get(name).cloned().map(coerce_equation_string) {
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
                    // Algebra (e.g. multiplying both sides) can move the unknown to
                    // either side of the equation, so try whichever side is fully
                    // bound by the supplied kwargs rather than assuming it's the RHS.
                    let unbound = |s: &&String| !local_env.contains_key(*s);
                    let mut rhs_free = std::collections::HashSet::new();
                    rhs.free_symbols(&mut rhs_free);
                    let rhs_missing: Vec<&String> = rhs_free.iter().filter(unbound).collect();
                    let solved_node = if rhs_missing.is_empty() {
                        &rhs
                    } else {
                        let mut lhs_free = std::collections::HashSet::new();
                        lhs.free_symbols(&mut lhs_free);
                        let lhs_missing: Vec<&String> = lhs_free.iter().filter(unbound).collect();
                        if lhs_missing.is_empty() {
                            &lhs
                        } else {
                            // Neither side is fully bound by name alone, which is what a unit in
                            // the equation text looks like: `"Q = m * 4.18 J/(g*K) * (T2 - T1)"`
                            // leaves J, g and K free on the right and the unknown Q on the left.
                            // Ignoring unit symbols separates the two, and only here — after both
                            // strict passes have failed — so a unit-named unknown (solving for
                            // `V`, `T`, `A`) is still picked up by the passes above.
                            let rhs_units_only =
                                rhs_missing.iter().all(|s| crate::parser::is_known_unit_symbol(s));
                            let lhs_units_only =
                                lhs_missing.iter().all(|s| crate::parser::is_known_unit_symbol(s));
                            if rhs_units_only {
                                &rhs
                            } else if lhs_units_only {
                                &lhs
                            } else {
                                let missing = if rhs_missing.len() <= lhs_missing.len() { rhs_missing } else { lhs_missing };
                                return Err(PhysureError::Generic(format!(
                                    "Missing argument(s) for equation '{}': {}",
                                    name,
                                    missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                                )));
                            }
                        }
                    };
                    let solved_str = solved_node.to_phs_string();
                    let program = crate::parser::parse_phs(&solved_str)?;
                    let Some(Statement::Expr(expr)) = program.statements.first() else {
                        return Err(PhysureError::Generic(format!("Failed to evaluate equation '{}'", name)));
                    };
                    return self.eval_expr(expr, &local_env);
                }

                // Short-circuit logical operators: language semantics, not an optimization,
                // so they must run before the eager argument evaluation below rather than as
                // ordinary builtins. Strict Bool checking here (never `is_truthy`) is what
                // keeps `5 m and enabled` a type error instead of a silent truthiness test.
                if kwargs.is_empty() && args.len() == 1 && name == "op_not" {
                    let operand = self.eval_expr(&args[0], env)?;
                    let PhsValue::Bool(b) = operand else {
                        return Err(PhysureError::Generic(format!(
                            "`not` expects a Bool operand, got {}", operand.type_name()
                        )));
                    };
                    return Ok(PhsValue::Bool(!b));
                }
                if kwargs.is_empty() && args.len() == 2 && (name == "op_and" || name == "op_or") {
                    let word = if name == "op_and" { "and" } else { "or" };
                    let left = self.eval_expr(&args[0], env)?;
                    let PhsValue::Bool(left_b) = left else {
                        return Err(PhysureError::Generic(format!(
                            "`{}` expects Bool operands, left side was {}", word, left.type_name()
                        )));
                    };
                    if (name == "op_and" && !left_b) || (name == "op_or" && left_b) {
                        return Ok(PhsValue::Bool(left_b));
                    }
                    let right = self.eval_expr(&args[1], env)?;
                    let PhsValue::Bool(right_b) = right else {
                        return Err(PhysureError::Generic(format!(
                            "`{}` expects Bool operands, right side was {}", word, right.type_name()
                        )));
                    };
                    return Ok(PhsValue::Bool(right_b));
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

                if let Some(PhsValue::Function(func)) = env.get(name) {
                    if args.len() == 1 && kwargs.is_empty() {
                        let arg_eval = self.eval_expr(&args[0], env);
                        if let Ok(PhsValue::Function(arg_func)) = arg_eval {
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
                            return Ok(PhsValue::Function(crate::ast::FunctionDefNode {
                                name: format!("{}_{}", func.name, arg_func.name),
                                params,
                                param_units,
                                body_stmts: vec![body],
                                body_lines: vec![],
                                decorators: Vec::new(),
                                doc: None,
                            }));
                        }
                    }
                }

                if let Some(val) = crate::builtins::eval_core_builtin(name, &arg_vals, self, env)? {
                    return Ok(val);
                }

                if let Some((domain, canonical)) = self.unlocked_builtins.lock().unwrap_or_else(|e| e.into_inner()).get(name).cloned() {
                    if let Some(val) = crate::builtins::eval_domain_builtin_with_kwargs(domain, &canonical, &arg_vals, &kwarg_vals, self, env)? {
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
            Expr::ForExpr { var, iterable, body } => {
                let iterable_val = self.eval_expr(iterable, env)?;
                let items: Vec<PhsValue> = match iterable_val {
                    PhsValue::Vector(v) => v,
                    PhsValue::Range(start, end) => {
                        let (start_num, unit) = match start.as_ref() {
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
                        let end_num = match end.as_ref() {
                            PhsValue::Number(n) => *n,
                            PhsValue::Quantity(q) => q.value.mean(),
                            _ => return Err(PhysureError::Generic("Range end must be a number or quantity".into())),
                        };
                        let start_i = start_num as i64;
                        let end_i = end_num as i64;
                        (start_i..end_i)
                            .map(|i| {
                                if let Some(ref u) = unit {
                                    PhsValue::Quantity(Quantity::new_scalar(i as f64, 0.0, u.clone(), None, None))
                                } else {
                                    PhsValue::Number(i as f64)
                                }
                            })
                            .collect()
                    }
                    other => return Err(PhysureError::Generic(format!("Cannot iterate over {}", other))),
                };

                // Switch to parallel evaluation if the iteration count meets the threshold.
                // Note: rayon's parallel collect stops scheduling new work on error but may
                // leave in-flight work on other threads to complete, so loop body side effects
                // (e.g. I/O in external functions) may partially execute even if evaluation fails.
                if items.len() >= physure_core::settings::parallel_threshold() && !self.debug_hook_is_set() {
                    use rayon::prelude::*;
                    let results: Vec<PhsValue> = items
                        .into_par_iter()
                        .map(|item| {
                            let mut local_env = env.clone();
                            local_env.insert(var.clone(), item);
                            self.eval_expr(body, &local_env)
                        })
                        .collect::<PhysureResult<Vec<PhsValue>>>()?;
                    Ok(PhsValue::Vector(results))
                } else {
                    let mut local_env = env.clone();
                    let old_val = local_env.get(var).cloned();
                    let mut results = Vec::with_capacity(items.len());
                    for item in items {
                        local_env.insert(var.clone(), item);
                        results.push(self.eval_expr(body, &local_env)?);
                    }
                    if let Some(old) = old_val {
                        local_env.insert(var.clone(), old);
                    } else {
                        local_env.remove(var);
                    }
                    Ok(PhsValue::Vector(results))
                }
            }
        }
    }

    fn eval_prime_function_call(
        &self,
        base_name: &str,
        order: usize,
        arg_vals: &[PhsValue],
        args: &[crate::ast::Expr],
        env: &HashMap<String, PhsValue>,
    ) -> PhysureResult<Option<PhsValue>> {
        if let Some(val) = env.get(base_name) {
            match val {
                PhsValue::Function(func) => {
                    if func.params.is_empty() {
                        return Err(PhysureError::Generic(format!("Function {} has no parameters to differentiate", base_name)));
                    }
                    let var_name = &func.params[0];
                    let mut body_str = String::new();
                    for stmt in &func.body_stmts {
                        match stmt {
                            crate::ast::Statement::Return(expr) | crate::ast::Statement::Expr(expr) => {
                                body_str = crate::codegen::expr_to_phs_string(expr);
                            }
                            _ => {}
                        }
                    }
                    if body_str.is_empty() {
                        return Err(PhysureError::Generic(format!("Cannot extract expression for function {}", base_name)));
                    }
                    let node = crate::symbolic::SymbolicParser::parse_str(&body_str)?;
                    let diff_node = node.diff_node_n(var_name, order)?;

                    if !arg_vals.is_empty() {
                        let first_arg = &arg_vals[0];
                        match first_arg {
                            PhsValue::Number(num) => {
                                let mut local_env = env.clone();
                                local_env.insert(var_name.clone(), PhsValue::Number(*num));
                                let expr_node = crate::parser::parse_phs(&diff_node.to_phs_string())?;
                                if let Some(crate::ast::Statement::Expr(e)) = expr_node.statements.first() {
                                    return Ok(Some(self.eval_expr(e, &local_env)?));
                                }
                            }
                            PhsValue::Quantity(q) => {
                                let mut local_env = env.clone();
                                local_env.insert(var_name.clone(), PhsValue::Quantity(q.clone()));
                                let expr_node = crate::parser::parse_phs(&diff_node.to_phs_string())?;
                                if let Some(crate::ast::Statement::Expr(e)) = expr_node.statements.first() {
                                    return Ok(Some(self.eval_expr(e, &local_env)?));
                                }
                            }
                            _ => {}
                        }
                    }
                    return Ok(Some(PhsValue::String(diff_node.to_string())));
                }
                PhsValue::String(expr_str) => {
                    let var_name = if !args.is_empty() {
                        if let crate::ast::Expr::Identifier(v) = &args[0] {
                            v.as_str()
                        } else {
                            "x"
                        }
                    } else {
                        "x"
                    };
                    let node = crate::symbolic::SymbolicParser::parse_str(expr_str)?;
                    let diff_node = node.diff_node_n(var_name, order)?;
                    return Ok(Some(PhsValue::String(diff_node.to_string())));
                }
                _ => {}
            }
        }
        Ok(None)
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
                "Argument for parameter '{}' of function '{}' has a unit incompatible with declared unit '{}': {:?}",
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
