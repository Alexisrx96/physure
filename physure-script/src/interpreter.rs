use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use physure_core::error::{PhysureError, PhysureResult};

/// A host-registered function callable from PHS source by name. Lets embedders
/// (e.g. the PyO3 binding) expose functions without physure-script depending on them.
pub type ExternalFn = Arc<dyn Fn(&[PhsValue]) -> PhysureResult<PhsValue> + Send + Sync>;
use physure_core::quantity::Quantity;
use physure_core::units::parser::Parser as UnitParser;
use physure_core::units::RationalUnit;

use crate::ast::{BinaryOp, Expr, Program, Statement};
use crate::resolver::{ModuleResolver, FsModuleResolver};
use crate::symbolic::Node;
use crate::PhsValue;

/// Applies a symbolic binary op to two `Node`s. Only `+ - * /` are meaningful for
/// equation algebra (the linear operations solve_equation and simplify understand);
/// `^` and `=>` on an equation are explicitly out of scope for this pass.
fn node_op(op: BinaryOp, a: Node, b: Node) -> PhysureResult<Node> {
    Ok(match op {
        BinaryOp::Add => Node::Add(vec![a, b]),
        BinaryOp::Sub => Node::Sub(Box::new(a), Box::new(b)),
        BinaryOp::Mul => Node::Mul(vec![a, b]),
        BinaryOp::Div => Node::Div(Box::new(a), Box::new(b)),
        _ => return Err(PhysureError::Generic("Pow/Convert are not supported for equation algebra yet".into())),
    })
}

/// Converts a non-Equation operand into a symbolic `Node` for equation algebra.
/// A `Quantity` operand is explicitly out of scope for this pass.
fn value_to_symbolic_node(val: &PhsValue) -> PhysureResult<Node> {
    match val {
        PhsValue::Number(n) => Ok(Node::Number(*n)),
        PhsValue::String(s) => crate::symbolic::SymbolicParser::parse_str(s),
        _ => Err(PhysureError::Generic("Equation algebra only supports Number, String, or Equation operands".into())),
    }
}

fn is_truthy(val: &PhsValue) -> bool {
    match val {
        PhsValue::Quantity(q) => q.value.mean() > 0.0,
        PhsValue::Number(n) => *n > 0.0,
        _ => false,
    }
}

fn eval_template_string(text: &str, interp: &PhsInterpreter, env: &HashMap<String, PhsValue>) -> String {
    let clean = text.trim_matches('`').trim();
    let mut result = String::new();
    let mut rest = clean;

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

#[derive(Clone)]
pub struct PhsInterpreter {
    pub env: HashMap<String, PhsValue>,
    pub resolver: Arc<dyn ModuleResolver>,
    pub externals: HashMap<String, ExternalFn>,
    plugin_state: Arc<Mutex<crate::plugin::PluginState>>,
    plugin_base_dir: Option<std::path::PathBuf>,
    /// call-name -> (domain, canonical builtin name), populated by `use x from calc` etc.
    unlocked_builtins: Arc<Mutex<HashMap<String, (&'static str, String)>>>,
    /// Lazily-loaded plugin/ext functions, keyed by their `use`d (possibly aliased) name.
    dynamic_externals: Arc<Mutex<HashMap<String, ExternalFn>>>,
}

impl Default for PhsInterpreter {
    fn default() -> Self {
        Self::new(Arc::new(FsModuleResolver::default()))
    }
}

impl PhsInterpreter {
    pub fn new(resolver: Arc<dyn ModuleResolver>) -> Self {
        Self {
            env: HashMap::new(),
            resolver,
            externals: HashMap::new(),
            plugin_state: Arc::new(Mutex::new(crate::plugin::PluginState::default())),
            plugin_base_dir: None,
            unlocked_builtins: Arc::new(Mutex::new(HashMap::new())),
            dynamic_externals: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn new_default() -> Self {
        Self::default()
    }

    /// Like `default()`, but resolves `import` paths relative to `base_dir`
    /// (typically the directory containing the script being run) instead of `.`.
    /// Native plugins under `<base_dir>/ext/` are not loaded eagerly — they're
    /// dlopen'd on demand the first time a script `use`s a symbol from them.
    pub fn with_base_dir(base_dir: impl Into<std::path::PathBuf>) -> Self {
        let base_dir = base_dir.into();
        let mut interp = Self::new(Arc::new(FsModuleResolver::new(base_dir.clone())));
        interp.plugin_base_dir = Some(base_dir);
        interp
    }

    /// Re-checks only the native plugin stems some `use` statement has already
    /// caused to be loaded, and installs any updated functions. No-op if the
    /// interpreter wasn't constructed with a base dir or nothing has been
    /// `use`d yet. Returns the names of functions (re)installed.
    pub fn reload_native_ext(&mut self) -> Vec<String> {
        if self.plugin_base_dir.is_none() {
            return Vec::new();
        }
        let plugin_state = self.plugin_state.clone();
        let mut state = plugin_state.lock().unwrap_or_else(|e| e.into_inner());
        let mut dynamic_externals = self.dynamic_externals.lock().unwrap_or_else(|e| e.into_inner());
        state.reload_loaded_into(&mut dynamic_externals)
    }

    /// Registers a host function under `name`, callable from PHS source like any builtin.
    /// Takes precedence over user-defined PHS functions but not over builtins.
    pub fn register_fn<F>(&mut self, name: impl Into<String>, f: F)
    where
        F: Fn(&[PhsValue]) -> PhysureResult<PhsValue> + Send + Sync + 'static,
    {
        self.externals.insert(name.into(), Arc::new(f));
    }

    pub fn eval_str(&mut self, code: &str) -> PhysureResult<Vec<PhsValue>> {
        let prog = crate::parse_phs(code)?;
        let mut results = Vec::new();
        for stmt in &prog.statements {
            results.push(self.eval_statement(stmt)?);
        }
        Ok(results)
    }

    pub fn run_statement(&mut self, stmt: &Statement) -> PhysureResult<PhsValue> {
        self.eval_statement(stmt)
    }

    pub fn get_var(&self, name: &str) -> Option<&PhsValue> {
        self.env.get(name)
    }

    pub fn env(&self) -> &HashMap<String, PhsValue> {
        &self.env
    }

    pub fn get_fn_params(&self, name: &str) -> Option<Vec<String>> {
        if let Some(PhsValue::Function(f)) = self.env.get(name) {
            Some(f.params.clone())
        } else {
            None
        }
    }

    pub fn eval_program(&mut self, program: &Program) -> PhysureResult<HashMap<String, PhsValue>> {
        for stmt in &program.statements {
            self.eval_statement(stmt)?;
        }
        Ok(self.env.clone())
    }

    pub fn eval_statement_with_env(&self, stmt: &Statement, env: &mut HashMap<String, PhsValue>) -> PhysureResult<PhsValue> {
        match stmt {
            Statement::Assignment(node) => {
                let val = self.eval_expr(&node.value, env)?;
                env.insert(node.name.clone(), val.clone());
                Ok(val)
            }
            Statement::FunctionDef(node) => {
                env.insert(node.name.clone(), PhsValue::Function(node.clone()));
                Ok(PhsValue::None)
            }
            Statement::Expr(expr) => {
                self.eval_expr(expr, env)
            }
            Statement::Import(node) => self.resolve_use(node, env),
            Statement::Export(_node) => Ok(PhsValue::None),
            Statement::Return(expr) => self.eval_expr(expr, env),
            Statement::GuardReturn { cond, value } => {
                let cond_val = self.eval_expr(cond, env)?;
                if is_truthy(&cond_val) {
                    self.eval_expr(value, env)
                } else {
                    Ok(PhsValue::None)
                }
            }
        }
    }

    /// Resolves a `use` statement against, in order: builtin domains (`core` is
    /// always on, so this only matters for `calc`/`plot`/`array`), `.phs` modules,
    /// and native plugin stems (`<base_dir>/ext/<stem>.<DLL_EXTENSION>`, dlopen'd lazily).
    fn resolve_use(&self, node: &crate::ast::ImportNode, env: &mut HashMap<String, PhsValue>) -> PhysureResult<PhsValue> {
        use crate::ast::ImportSpecifier;

        if let Some(members) = crate::builtins::domain_members(&node.path) {
            let domain: &'static str = match node.path.as_str() {
                "calc" => "calc",
                "plot" => "plot",
                "array" => "array",
                _ => unreachable!("domain_members returned Some for unknown domain"),
            };
            let mut unlocked = self.unlocked_builtins.lock().unwrap_or_else(|e| e.into_inner());
            match &node.specifier {
                ImportSpecifier::Wildcard => {
                    for &member in members {
                        unlocked.insert(member.to_string(), (domain, member.to_string()));
                    }
                }
                ImportSpecifier::Symbols(syms) => {
                    for sym in syms {
                        if !members.contains(&sym.name.as_str()) {
                            return Err(PhysureError::Generic(format!("no such function '{}' in domain '{}'", sym.name, node.path)));
                        }
                        let call_name = sym.alias.as_deref().unwrap_or(&sym.name).to_string();
                        unlocked.insert(call_name, (domain, sym.name.clone()));
                    }
                }
                ImportSpecifier::ModuleAlias(_alias) => {
                    return Err(PhysureError::Generic("Module aliases not yet supported by interpreter".into()));
                }
            }
            return Ok(PhsValue::None);
        }

        if let Ok(export) = self.resolver.resolve(&node.path) {
            match &node.specifier {
                ImportSpecifier::Wildcard => {
                    for (name, expr) in export.symbols {
                        let val = self.eval_expr(&expr, env)?;
                        env.insert(name, val);
                    }
                    for (name, func) in export.functions {
                        env.insert(name, PhsValue::Function(func));
                    }
                }
                ImportSpecifier::Symbols(syms) => {
                    for sym in syms {
                        if let Some(expr) = export.symbols.get(&sym.name) {
                            let val = self.eval_expr(expr, env)?;
                            let target_name = sym.alias.as_deref().unwrap_or(&sym.name).to_string();
                            env.insert(target_name, val);
                        } else if let Some(func) = export.functions.get(&sym.name) {
                            let target_name = sym.alias.as_deref().unwrap_or(&sym.name).to_string();
                            env.insert(target_name, PhsValue::Function(func.clone()));
                        } else {
                            return Err(PhysureError::Generic(format!("Symbol {} not found in module {}", sym.name, node.path)));
                        }
                    }
                }
                ImportSpecifier::ModuleAlias(_alias) => {
                    return Err(PhysureError::Generic("Module aliases not yet supported by interpreter".into()));
                }
            }
            return Ok(PhsValue::None);
        }

        if let Some(base_dir) = &self.plugin_base_dir {
            let plugin_state = self.plugin_state.clone();
            let mut state = plugin_state.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(functions) = state.ensure_stem_loaded(base_dir, &node.path)? {
                self.install_dynamic_externals(&node.specifier, &node.path, &functions)?;
                return Ok(PhsValue::None);
            }
        }

        Err(PhysureError::Generic(format!("No such module or domain '{}'", node.path)))
    }

    /// Installs the requested (possibly aliased) subset of `functions` into
    /// `dynamic_externals`, erroring if a requested name isn't exported.
    fn install_dynamic_externals(&self, specifier: &crate::ast::ImportSpecifier, module: &str, functions: &HashMap<String, ExternalFn>) -> PhysureResult<()> {
        use crate::ast::ImportSpecifier;
        let mut dynamic = self.dynamic_externals.lock().unwrap_or_else(|e| e.into_inner());
        match specifier {
            ImportSpecifier::Wildcard => {
                for (name, f) in functions {
                    dynamic.insert(name.clone(), f.clone());
                }
            }
            ImportSpecifier::Symbols(syms) => {
                for sym in syms {
                    let f = functions.get(&sym.name).ok_or_else(|| {
                        PhysureError::Generic(format!("Symbol {} not found in module {}", sym.name, module))
                    })?;
                    let target_name = sym.alias.as_deref().unwrap_or(&sym.name).to_string();
                    dynamic.insert(target_name, f.clone());
                }
            }
            ImportSpecifier::ModuleAlias(_alias) => {
                return Err(PhysureError::Generic("Module aliases not yet supported by interpreter".into()));
            }
        }
        Ok(())
    }

    pub fn eval_statement(&mut self, stmt: &Statement) -> PhysureResult<PhsValue> {
        let mut env = self.env.clone();
        let res = self.eval_statement_with_env(stmt, &mut env)?;
        self.env = env;
        Ok(res)
    }

    pub fn eval_expr(&self, expr: &Expr, env: &HashMap<String, PhsValue>) -> PhysureResult<PhsValue> {
        match expr {
            Expr::Quantity(node) => {
                let mut q = Quantity::new_scalar(node.magnitude, node.uncertainty.unwrap_or(0.0), RationalUnit::dimensionless(), None, None);
                if let Some(unit_str) = &node.unit {
                    let clean_unit_str = unit_str.split('#').next().unwrap().split("//").next().unwrap().trim();
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
            Expr::Identifier(name) => {
                if name.starts_with('`') || (name.contains('{') && name.contains('}')) {
                    let text = eval_template_string(name, self, env);
                    Ok(PhsValue::String(text))
                } else if let Some(val) = env.get(name) {
                    Ok(val.clone())
                } else {
                    Ok(PhsValue::String(name.clone()))
                }
            }
            Expr::BinaryOp { op, left, right } => {
                if *op == BinaryOp::Convert {
                    let l_val = self.eval_expr(left, env)?;
                    return if let Expr::Identifier(ref target_unit) = **right {
                        let clean_target = target_unit.split('#').next().unwrap().split("//").next().unwrap().trim();
                        let parsed_unit = UnitParser::parse_expression(clean_target)?;
                        self.convert_value_to_unit(l_val, &parsed_unit)
                    } else {
                        Ok(l_val)
                    };
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

                if let Some(PhsValue::Equation(_, rhs)) = env.get(name) {
                    let rhs = rhs.clone();
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
                    let mut free = std::collections::HashSet::new();
                    rhs.free_symbols(&mut free);
                    let missing: Vec<&String> = free.iter().filter(|s| !local_env.contains_key(*s)).collect();
                    if !missing.is_empty() {
                        return Err(PhysureError::Generic(format!(
                            "Missing argument(s) for equation '{}': {}",
                            name,
                            missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                        )));
                    }
                    let solved_str = rhs.to_phs_string();
                    let program = crate::parser::parse_phs(&solved_str)?;
                    let Some(Statement::Expr(expr)) = program.statements.first() else {
                        return Err(PhysureError::Generic(format!("Failed to evaluate equation '{}'", name)));
                    };
                    return self.eval_expr(expr, &local_env);
                }

                if !kwargs.is_empty() {
                    return Err(PhysureError::Generic(format!(
                        "Named arguments are only supported when calling an equation, but '{}' is not an equation",
                        name
                    )));
                }

                if let Some(PhsValue::Function(func)) = env.get(name) {
                    if args.len() == 1 {
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
                            }));
                        }
                    }
                }

                let mut arg_vals = Vec::new();
                for arg in args {
                    arg_vals.push(self.eval_expr(arg, env)?);
                }

                if let Some(val) = crate::builtins::eval_core_builtin(name, &arg_vals, self)? {
                    return Ok(val);
                }

                if let Some((domain, canonical)) = self.unlocked_builtins.lock().unwrap_or_else(|e| e.into_inner()).get(name).cloned() {
                    if let Some(val) = crate::builtins::eval_domain_builtin(domain, &canonical, &arg_vals, self)? {
                        return Ok(val);
                    }
                }

                let external = self.externals.get(name).cloned()
                    .or_else(|| self.dynamic_externals.lock().unwrap_or_else(|e| e.into_inner()).get(name).cloned());
                if let Some(f) = external {
                    return f(&arg_vals);
                }

                if let Some(PhsValue::Function(func)) = env.get(name) {
                    if func.params.len() != args.len() {
                        return Err(PhysureError::Generic(format!("Function {} expects {} args, got {}", name, func.params.len(), args.len())));
                    }
                    let mut local_env = env.clone();
                    for (i, (param_name, arg_val)) in func.params.iter().zip(arg_vals.into_iter()).enumerate() {
                        let bound_val = self.bind_param_value(name, param_name, func.param_units.get(i).and_then(|u| u.as_ref()), arg_val)?;
                        local_env.insert(param_name.clone(), bound_val);
                    }
                    let mut last_val = PhsValue::None;
                    for stmt in &func.body_stmts {
                        match stmt {
                            Statement::Return(expr) => {
                                last_val = self.eval_expr(expr, &local_env)?;
                                break;
                            }
                            Statement::GuardReturn { cond, value } => {
                                let cond_val = self.eval_expr(cond, &local_env)?;
                                if is_truthy(&cond_val) {
                                    last_val = self.eval_expr(value, &local_env)?;
                                    break;
                                }
                            }
                            _ => {
                                last_val = self.eval_statement_with_env(stmt, &mut local_env)?;
                            }
                        }
                    }
                    Ok(last_val)
                } else {
                    Err(PhysureError::Generic(format!("Undefined function '{}'", name)))
                }
            }
        }
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
    fn bind_param_value(
        &self,
        fn_name: &str,
        param_name: &str,
        declared_unit: Option<&String>,
        arg_val: PhsValue,
    ) -> PhysureResult<PhsValue> {
        let Some(unit_str) = declared_unit else {
            return Ok(arg_val);
        };
        let PhsValue::Quantity(q) = arg_val else {
            return Ok(arg_val);
        };
        let clean_unit_str = unit_str.split('#').next().unwrap().split("//").next().unwrap().trim();
        if clean_unit_str.is_empty() {
            return Ok(PhsValue::Quantity(q));
        }
        let target_unit = UnitParser::parse_expression(clean_unit_str)?;
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
            other => Ok(other),
        }
    }

    pub fn eval_binary_op_vals(&self, op: BinaryOp, l_val: PhsValue, r_val: PhsValue) -> PhysureResult<PhsValue> {
        match (l_val, r_val) {
            (PhsValue::Function(f), PhsValue::Function(g)) => {
                let (params, param_units) = if !f.params.is_empty() {
                    (f.params.clone(), f.param_units.clone())
                } else {
                    (g.params.clone(), g.param_units.clone())
                };
                let args_expr: Vec<Expr> = params.iter().map(|p| Expr::Identifier(p.clone())).collect();
                let body = Statement::Expr(Expr::BinaryOp {
                    op,
                    left: Box::new(Expr::FunctionCall { name: f.name.clone(), args: args_expr.clone(), kwargs: Vec::new() }),
                    right: Box::new(Expr::FunctionCall { name: g.name.clone(), args: args_expr, kwargs: Vec::new() }),
                });
                let name = match op {
                    BinaryOp::Add => format!("{}_add_{}", f.name, g.name),
                    BinaryOp::Sub => format!("{}_sub_{}", f.name, g.name),
                    BinaryOp::Mul => format!("{}_mul_{}", f.name, g.name),
                    BinaryOp::Div => format!("{}_div_{}", f.name, g.name),
                    _ => format!("{}_op_{}", f.name, g.name),
                };
                Ok(PhsValue::Function(crate::ast::FunctionDefNode {
                    name,
                    params,
                    param_units,
                    body_stmts: vec![body],
                }))
            }
            (PhsValue::Equation(l1, r1), PhsValue::Equation(l2, r2)) => {
                let new_l = node_op(op, l1, l2)?.simplify();
                let new_r = node_op(op, r1, r2)?.simplify();
                Ok(PhsValue::Equation(new_l, new_r))
            }
            (PhsValue::Equation(l, r), other) => {
                let node = value_to_symbolic_node(&other)?;
                let new_l = node_op(op, l, node.clone())?.simplify();
                let new_r = node_op(op, r, node)?.simplify();
                Ok(PhsValue::Equation(new_l, new_r))
            }
            (other, PhsValue::Equation(l, r)) => {
                let node = value_to_symbolic_node(&other)?;
                let new_l = node_op(op, node.clone(), l)?.simplify();
                let new_r = node_op(op, node, r)?.simplify();
                Ok(PhsValue::Equation(new_l, new_r))
            }
            (PhsValue::Vector(l_vec), PhsValue::Vector(r_vec)) => {
                if l_vec.len() != r_vec.len() {
                    return Err(PhysureError::Generic("Vector length mismatch in binary operation".into()));
                }
                let mut results = Vec::new();
                for (l_item, r_item) in l_vec.into_iter().zip(r_vec.into_iter()) {
                    results.push(self.eval_binary_op_vals(op, l_item, r_item)?);
                }
                Ok(PhsValue::Vector(results))
            }
            (PhsValue::Vector(v_vec), scalar) => {
                let mut results = Vec::new();
                for item in v_vec {
                    results.push(self.eval_binary_op_vals(op, item, scalar.clone())?);
                }
                Ok(PhsValue::Vector(results))
            }
            (scalar, PhsValue::Vector(v_vec)) => {
                let mut results = Vec::new();
                for item in v_vec {
                    results.push(self.eval_binary_op_vals(op, scalar.clone(), item)?);
                }
                Ok(PhsValue::Vector(results))
            }
            (PhsValue::Quantity(l), PhsValue::Quantity(r)) => {
                let res = match op {
                    BinaryOp::Add => l.add(&r)?,
                    BinaryOp::Sub => l.sub(&r)?,
                    BinaryOp::Mul => l.mul(&r)?,
                    BinaryOp::Div => l.div(&r)?,
                    BinaryOp::Pow => {
                        if r.unit == RationalUnit::dimensionless() && r.value.std_dev() == 0.0 {
                            l.pow(r.value.mean())?
                        } else {
                            return Err(PhysureError::Generic("Exponent must be a dimensionless constant".into()));
                        }
                    }
                    BinaryOp::Convert => unreachable!(),
                };
                Ok(PhsValue::Quantity(res))
            }
            (PhsValue::Number(l), PhsValue::Number(r)) => {
                let res = match op {
                    BinaryOp::Add => l + r,
                    BinaryOp::Sub => l - r,
                    BinaryOp::Mul => l * r,
                    BinaryOp::Div => {
                        if r == 0.0 {
                            return Err(PhysureError::Generic("Division by zero".into()));
                        }
                        l / r
                    }
                    BinaryOp::Pow => l.powf(r),
                    BinaryOp::Convert => unreachable!(),
                };
                Ok(PhsValue::Number(res))
            }
            (PhsValue::Quantity(l), PhsValue::Number(r)) => {
                let r_q = Quantity::new_scalar(r, 0.0, RationalUnit::dimensionless(), None, None);
                let res = match op {
                    BinaryOp::Add => l.add(&r_q)?,
                    BinaryOp::Sub => l.sub(&r_q)?,
                    BinaryOp::Mul => l.mul(&r_q)?,
                    BinaryOp::Div => l.div(&r_q)?,
                    BinaryOp::Pow => l.pow(r)?,
                    BinaryOp::Convert => unreachable!(),
                };
                Ok(PhsValue::Quantity(res))
            }
            (PhsValue::Number(l), PhsValue::Quantity(r)) => {
                let l_q = Quantity::new_scalar(l, 0.0, RationalUnit::dimensionless(), None, None);
                let res = match op {
                    BinaryOp::Add => l_q.add(&r)?,
                    BinaryOp::Sub => l_q.sub(&r)?,
                    BinaryOp::Mul => l_q.mul(&r)?,
                    BinaryOp::Div => l_q.div(&r)?,
                    BinaryOp::Pow => return Err(PhysureError::Generic("Quantity exponent not supported".into())),
                    BinaryOp::Convert => unreachable!(),
                };
                Ok(PhsValue::Quantity(res))
            }
            (PhsValue::Quantity(l), PhsValue::String(r)) => {
                let clean_r = r.split('#').next().unwrap().split("//").next().unwrap().trim();
                if let Ok(parsed_unit) = UnitParser::parse_expression(clean_r) {
                    let unit_q = Quantity::new_scalar(1.0, 0.0, parsed_unit.clone(), None, None);
                    let res = match op {
                        BinaryOp::Mul => l.mul(&unit_q)?,
                        BinaryOp::Div => l.div(&unit_q)?,
                        BinaryOp::Convert => l.convert_to(&parsed_unit)?,
                        _ => return Err(PhysureError::Generic("Unsupported op with unit string".into())),
                    };
                    Ok(PhsValue::Quantity(res))
                } else {
                    Err(PhysureError::Generic(format!("Unknown unit symbol: {}", r)))
                }
            }
            (PhsValue::Number(l), PhsValue::String(r)) => {
                let clean_r = r.split('#').next().unwrap().split("//").next().unwrap().trim();
                if let Ok(parsed_unit) = UnitParser::parse_expression(clean_r) {
                    let unit_q = Quantity::new_scalar(1.0, 0.0, parsed_unit, None, None);
                    let num_q = Quantity::new_scalar(l, 0.0, RationalUnit::dimensionless(), None, None);
                    let res = match op {
                        BinaryOp::Mul => num_q.mul(&unit_q)?,
                        BinaryOp::Div => num_q.div(&unit_q)?,
                        _ => return Err(PhysureError::Generic("Unsupported op with unit string".into())),
                    };
                    Ok(PhsValue::Quantity(res))
                } else {
                    Err(PhysureError::Generic(format!("Unknown unit symbol: {}", r)))
                }
            }
            (PhsValue::String(l), PhsValue::Quantity(r)) => {
                let clean_l = l.split('#').next().unwrap().split("//").next().unwrap().trim();
                if let Ok(parsed_unit) = UnitParser::parse_expression(clean_l) {
                    let unit_q = Quantity::new_scalar(1.0, 0.0, parsed_unit, None, None);
                    let res = match op {
                        BinaryOp::Mul => unit_q.mul(&r)?,
                        BinaryOp::Pow => unit_q.pow(r.value.mean())?,
                        _ => return Err(PhysureError::Generic("Unsupported op with unit string".into())),
                    };
                    Ok(PhsValue::Quantity(res))
                } else {
                    Err(PhysureError::Generic(format!("Unknown unit symbol: {}", l)))
                }
            }
            (PhsValue::String(l), PhsValue::Number(r)) => {
                let clean_l = l.split('#').next().unwrap().split("//").next().unwrap().trim();
                if let Ok(parsed_unit) = UnitParser::parse_expression(clean_l) {
                    let unit_q = Quantity::new_scalar(1.0, 0.0, parsed_unit, None, None);
                    let num_q = Quantity::new_scalar(r, 0.0, RationalUnit::dimensionless(), None, None);
                    let res = match op {
                        BinaryOp::Mul => unit_q.mul(&num_q)?,
                        BinaryOp::Pow => unit_q.pow(r)?,
                        _ => return Err(PhysureError::Generic("Unsupported op with unit string".into())),
                    };
                    Ok(PhsValue::Quantity(res))
                } else {
                    Err(PhysureError::Generic(format!("Unknown unit symbol: {}", l)))
                }
            }
            _ => Err(PhysureError::Generic("Invalid operand types for binary operation".into())),
        }
    }
}

pub fn eval_phs(input: &str) -> PhysureResult<Vec<PhsValue>> {
    let program = crate::parser::parse_phs(input)?;
    let mut interp = PhsInterpreter::default();
    
    let mut results = Vec::new();
    for stmt in &program.statements {
        let val = interp.eval_statement(stmt)?;
        if val != PhsValue::None {
            results.push(val);
        }
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::*;
    use crate::resolver::{MemoryModuleResolver, ModuleExport};
    
    #[test]
    fn test_unary_minus_multiline_if_and_guard_return() {
        let mut interp = PhsInterpreter::default();
        let code = r#"
CERO_A = 0 A
CERO_V = 0 V

circuito_abierto(V: V, I: A) =
    if I == CERO_A
    then return 0
    if V == CERO_V
    then
        - 1
    else
        1

r1 = circuito_abierto(5 V, 0 A)
r2 = circuito_abierto(0 V, 2 A)
r3 = circuito_abierto(5 V, 2 A)
"#;
        interp.eval_str(code).unwrap();
        let num = |name: &str| match interp.get_var(name).unwrap() {
            PhsValue::Quantity(q) => q.value.mean(),
            PhsValue::Number(n) => *n,
            other => panic!("expected numeric value for {name}, got {other:?}"),
        };
        assert_eq!(num("r1"), 0.0);
        assert_eq!(num("r2"), -1.0);
        assert_eq!(num("r3"), 1.0);
    }

    #[test]
    fn test_kinetic_energy() {
        let mut interp = PhsInterpreter::default();
        
        let statements = vec![
            Statement::FunctionDef(FunctionDefNode {
                name: "kinetic_energy".to_string(),
                params: vec!["m".to_string(), "v".to_string()],
                param_units: vec![None, None],
                body_stmts: vec![Statement::Expr(Expr::BinaryOp {
                    op: BinaryOp::Mul,
                    left: Box::new(Expr::BinaryOp {
                        op: BinaryOp::Mul,
                        left: Box::new(Expr::Quantity(QuantityNode {
                            magnitude: 0.5,
                            uncertainty: None,
                            is_sigma: false,
                            unit: None,
                        })),
                        right: Box::new(Expr::Identifier("m".to_string())),
                    }),
                    right: Box::new(Expr::BinaryOp {
                        op: BinaryOp::Pow,
                        left: Box::new(Expr::Identifier("v".to_string())),
                        right: Box::new(Expr::Quantity(QuantityNode {
                            magnitude: 2.0,
                            uncertainty: None,
                            is_sigma: false,
                            unit: None,
                        })),
                    })
                })]
            }),
            Statement::Assignment(AssignmentNode {
                name: "m".to_string(),
                value: Expr::Quantity(QuantityNode {
                    magnitude: 10.0,
                    uncertainty: None,
                    is_sigma: false,
                    unit: Some("kg".to_string()),
                }),
            }),
            Statement::Assignment(AssignmentNode {
                name: "v".to_string(),
                value: Expr::Quantity(QuantityNode {
                    magnitude: 2.0,
                    uncertainty: None,
                    is_sigma: false,
                    unit: Some("m/s".to_string()),
                }),
            }),
            Statement::Assignment(AssignmentNode {
                name: "E".to_string(),
                value: Expr::FunctionCall {
                    name: "kinetic_energy".to_string(),
                    args: vec![
                        Expr::Identifier("m".to_string()),
                        Expr::Identifier("v".to_string()),
                    ],
                    kwargs: Vec::new(),
                },
            }),
        ];
        
        let env = interp.eval_program(&Program { statements }).unwrap();
        
        let e_val = env.get("E").unwrap();
        if let PhsValue::Quantity(q) = e_val {
            assert_eq!(q.value.mean(), 20.0);
            
            // Check that it's equivalent to 20 J
            let parsed_j = UnitParser::parse_expression("J").unwrap();
            assert!(q.unit.same_dimensions(&parsed_j));
        } else {
            panic!("Expected quantity");
        }
    }
    
    #[test]
    fn test_uncertainty_propagation() {
        let mut interp = PhsInterpreter::default();
        let program = Program {
            statements: vec![
                Statement::Assignment(AssignmentNode {
                    name: "m".to_string(),
                    value: Expr::Quantity(QuantityNode {
                        magnitude: 75.0,
                        uncertainty: Some(0.5),
                        is_sigma: false,
                        unit: Some("kg".to_string()),
                    }),
                }),
            ],
        };
        let env = interp.eval_program(&program).unwrap();
        let m_val = env.get("m").unwrap();
        if let PhsValue::Quantity(q) = m_val {
            assert_eq!(q.value.mean(), 75.0);
            assert_eq!(q.value.std_dev(), 0.5);
            assert_eq!(q.unit.__repr__(), "kg");
        } else {
            panic!("Expected quantity");
        }
    }
    
    #[test]
    fn test_register_fn_dispatch() {
        let mut interp = PhsInterpreter::default();
        interp.register_fn("double", |args: &[PhsValue]| match args.first() {
            Some(PhsValue::Quantity(q)) => Ok(PhsValue::Number(q.value.mean() * 2.0)),
            Some(PhsValue::Number(n)) => Ok(PhsValue::Number(n * 2.0)),
            _ => Err(PhysureError::Generic("double expects a number".into())),
        });

        let results = interp.eval_str("double(21)").unwrap();
        assert_eq!(results, vec![PhsValue::Number(42.0)]);
    }

    #[test]
    fn test_virtual_module_import() {
        let mut resolver = MemoryModuleResolver::new();
        let mut export = ModuleExport {
            symbols: HashMap::new(),
            functions: HashMap::new(),
        };
        export.symbols.insert("G".to_string(), Expr::Quantity(QuantityNode {
            magnitude: 6.674e-11,
            uncertainty: None,
            is_sigma: false,
            unit: Some("m^3 / (kg * s^2)".to_string()),
        }));
        resolver.add_module("constants".to_string(), export);
        
        let mut interp = PhsInterpreter::new(Arc::new(resolver));
        let program = Program {
            statements: vec![
                Statement::Import(ImportNode {
                    path: "constants".to_string(),
                    specifier: ImportSpecifier::Wildcard,
                })
            ],
        };
        let env = interp.eval_program(&program).unwrap();
        let g_val = env.get("G").unwrap();
        if let PhsValue::Quantity(q) = g_val {
            assert_eq!(q.value.mean(), 6.674e-11);
        } else {
            panic!("Expected quantity");
        }
    }

    fn assert_is_five(val: &PhsValue) {
        match val {
            PhsValue::Number(n) => assert_eq!(*n, 5.0),
            PhsValue::Quantity(q) => assert_eq!(q.value.mean(), 5.0),
            PhsValue::String(s) => assert_eq!(s, "5"),
            other => panic!("Expected solve(...) to resolve to 5, got {:?}", other),
        }
    }

    #[test]
    fn test_domain_gated_calc_builtins() {
        // Ungated call fails with "Undefined function".
        let mut interp = PhsInterpreter::default();
        let err = interp
            .eval_str("solve(\"2 * x = 10\", \"x\")")
            .unwrap_err();
        assert!(
            err.to_string().contains("Undefined function"),
            "unexpected error: {}",
            err
        );

        // `use solve from calc` unlocks it.
        let mut interp = PhsInterpreter::default();
        interp.eval_str("use solve from calc").unwrap();
        let results = interp.eval_str("solve(\"2 * x = 10\", \"x\")").unwrap();
        assert_is_five(&results[0]);

        // `use * from calc` unlocks every member.
        let mut interp = PhsInterpreter::default();
        interp.eval_str("use * from calc").unwrap();
        interp.eval_str("deriv(\"x^2\", \"x\")").unwrap();
        interp.eval_str("integral(\"2 * x\", \"x\")").unwrap();
        let results = interp.eval_str("solve(\"2 * x = 10\", \"x\")").unwrap();
        assert_is_five(&results[0]);

        // Requesting an unknown domain member errors clearly.
        let mut interp = PhsInterpreter::default();
        let err = interp.eval_str("use bogus_fn from calc").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("no such function"), "unexpected error: {}", msg);
        assert!(msg.contains("in domain"), "unexpected error: {}", msg);
    }
}
