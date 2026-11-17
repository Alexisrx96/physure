//! The global identifiers of PHS: `pi` (`π`) and every constant of the catalog in force.
//!
//! **One resolution order** for a bare name, used by the interpreter, by `integral` and `limit` (their integrands
//! and their bounds) and by the static checker:
//!
//! 1. lexical bindings (function parameters, `where` bindings, `for` variables) and variables in scope;
//! 2. user functions (they live with the variables);
//! 3. `pi` and `π` (the number `std::f64::consts::PI`), then the constants of the catalog in force, each a quantity
//!    in the unit the catalog gives it;
//! 4. units (`J` alone is one joule);
//! 5. anything else is unknown (today the text of the name).
//!
//! The catalog in force is the one unit text is read against: the innermost `units::with_catalog` scope of this
//! thread, else the configuration in force (a frozen file, or the embedded catalog plus the working directory's
//! `physure.conf`). A constant is read from it on every use, never copied into the interpreter, so an override or a
//! frozen file governs it. There is no cache to forget: a lookup costs what reading the unit of a literal costs (the
//! catalog is the cached one of `frozen::active`, and its unit text is parsed once per use).
//!
//! **Shadowing is an error.** A name of step 3 cannot be bound by an assignment, a parameter, a `for` variable, a
//! `where` binding, a function definition, an import or a host: a scientist who writes `pi = 3` has made a mistake,
//! not a choice. Which names of step 3 those are depends on the catalog in force, so a conf that adds a `[Constants]`
//! entry makes a script that binds that name an error under that conf only. The statement words, the literals and the
//! functions of the core (see [`crate::reserved`]) are refused the same way under every conf.
//!
//! Only the catalog's own names are constants: `e`, `c`, `k`, `R` are not (`h`, `g`, `G` are units).

use physure_core::error::{PhysureError, PhysureResult};
use physure_core::{Quantity, UnitRegistry};

use crate::ast::{Expr, ImportSpecifier, Statement};
use crate::value::PhsValue;

/// The names of the circle constant.
pub const PI_NAMES: [&str; 2] = ["pi", "π"];

/// Runs `f` against the catalog in force on this thread (see the module's documentation).
fn with_catalog_in_force<R>(f: impl FnOnce(&UnitRegistry) -> R) -> PhysureResult<R> {
    physure_core::units::with_current_catalog(|scoped| match scoped {
        Some(registry) => Ok(f(registry)),
        None => physure_core::units::conf::try_with_conf_registry(f).map_err(|e| PhysureError::Generic(e.to_string())),
    })
}

/// The constant `name` of `registry`, as a quantity, `None` when the registry has no such constant.
pub fn constant_of(name: &str, registry: &UnitRegistry) -> PhysureResult<Option<Quantity>> {
    let Some(meta) = registry.constants_meta.get(name) else { return Ok(None) };
    physure_core::units::conf::constant_quantity(&meta.value, registry)
        .map(Some)
        .map_err(|why| PhysureError::Generic(format!("the catalog constant {name} cannot be used: {why}")))
}

/// Whether `name` is a global identifier of `registry`: `pi`, `π` or one of its constants.
pub fn is_global_in(name: &str, registry: &UnitRegistry) -> bool {
    PI_NAMES.contains(&name) || registry.constants_meta.contains_key(name)
}

/// The value of the global identifier `name` in the catalog in force: `pi` and `π` are the number π, a constant is
/// its quantity. `None` when `name` is not one.
pub fn global_value(name: &str) -> PhysureResult<Option<PhsValue>> {
    if PI_NAMES.contains(&name) {
        return Ok(Some(PhsValue::Number(std::f64::consts::PI)));
    }
    with_catalog_in_force(|registry| constant_of(name, registry))?.map(|q| q.map(PhsValue::Quantity))
}

/// Whether `name` is a global identifier of the catalog in force.
pub fn is_global(name: &str) -> PhysureResult<bool> {
    if PI_NAMES.contains(&name) {
        return Ok(true);
    }
    with_catalog_in_force(|registry| registry.constants_meta.contains_key(name))
}

/// The text of the value of the constant `name` of the catalog in force (`8.31446261815324 J*mol^-1*K^-1`).
pub fn constant_text(name: &str) -> PhysureResult<Option<String>> {
    with_catalog_in_force(|registry| registry.constants_meta.get(name).map(|meta| meta.value.trim().to_string()))
}

/// The constants of the catalog in force, sorted by name, with the text of their values.
pub fn constants() -> PhysureResult<Vec<(String, String)>> {
    with_catalog_in_force(|registry| {
        let mut all: Vec<(String, String)> =
            registry.constants_meta.iter().map(|(name, meta)| (name.clone(), meta.value.clone())).collect();
        all.sort();
        all
    })
}

/// The error for binding the global identifier `name` as `what` ("a variable", "a parameter of `f`", ...).
pub fn shadowing_error(name: &str, what: &str, value: Option<&str>) -> String {
    let which = if PI_NAMES.contains(&name) {
        "the constant π".to_string()
    } else {
        match value {
            Some(value) => format!("a constant of the catalog in force ({value})"),
            None => "a constant of the catalog in force".to_string(),
        }
    };
    format!("`{name}` is {which} and cannot be bound as {what}; choose another name")
}

/// An error when `name` is reserved (a statement word, a literal or a function of PHS) or a global identifier of the
/// catalog in force: it cannot be bound as `what`.
pub fn refuse_binding(name: &str, what: &str) -> PhysureResult<()> {
    if let Some(kind) = crate::reserved::static_kind(name) {
        return Err(PhysureError::Generic(crate::reserved::binding_error(name, what, kind)));
    }
    refuse_constant_binding(name, what)
}

/// An error when `name` is `pi`, `π` or a constant of the catalog in force: it cannot be bound as `what`. It does not
/// refuse the reserved words, for the places where a name is local and never shadows a call.
pub fn refuse_constant_binding(name: &str, what: &str) -> PhysureResult<()> {
    if PI_NAMES.contains(&name) {
        return Err(PhysureError::Generic(shadowing_error(name, what, None)));
    }
    let value = with_catalog_in_force(|registry| registry.constants_meta.get(name).map(|meta| meta.value.clone()))?;
    match value {
        Some(value) => Err(PhysureError::Generic(shadowing_error(name, what, Some(&value)))),
        None => Ok(()),
    }
}

/// What `bindings_of` says binds a name an import brings; the text is followed by the path of the module.
pub(crate) const IMPORT_BINDING: &str = "a name imported from";

/// Every name `statement` binds, with what binds it, in source order: assignments, function names and parameters,
/// `for` variables and `where` bindings anywhere inside it (function bodies included, whether or not they ever
/// run), and the names an import of named symbols binds.
pub fn bindings_of(statement: &Statement, out: &mut Vec<(String, String)>) {
    match statement {
        Statement::Assignment(node) => {
            bindings_in(&node.value, out);
            out.push((node.name.clone(), "a variable".to_string()));
        }
        Statement::FunctionDef(node) => {
            out.push((node.name.clone(), "a function".to_string()));
            for param in &node.params {
                out.push((param.clone(), format!("a parameter of `{}`", node.name)));
            }
            for inner in &node.body_stmts {
                bindings_of(inner, out);
            }
        }
        Statement::Expr(e) | Statement::Return(e) => bindings_in(e, out),
        Statement::GuardReturn { cond, value } => {
            bindings_in(cond, out);
            bindings_in(value, out);
        }
        Statement::While { cond, body, .. } => {
            bindings_in(cond, out);
            for inner in body {
                bindings_of(inner, out);
            }
        }
        Statement::Import(import) => {
            if let ImportSpecifier::Symbols(symbols) = &import.specifier {
                for symbol in symbols {
                    let name = symbol.alias.clone().unwrap_or_else(|| symbol.name.clone());
                    out.push((name, format!("{IMPORT_BINDING} `{}`", import.path)));
                }
            }
        }
        Statement::Export(_) => {}
    }
}

/// The `for` variables and `where` bindings inside `expr`.
pub fn bindings_in(expr: &Expr, out: &mut Vec<(String, String)>) {
    match expr {
        Expr::BinaryOp { left, right, .. } => {
            bindings_in(left, out);
            bindings_in(right, out);
        }
        Expr::FunctionCall { name, args, kwargs } => {
            // `body where a = 1` is `let(a, 1, body)`
            if let ("let", [Expr::Identifier(local), ..]) = (name.as_str(), args.as_slice()) {
                out.push((local.clone(), "a `where` binding".to_string()));
            }
            for arg in args.iter().chain(kwargs.iter().map(|(_, e)| e)) {
                bindings_in(arg, out);
            }
        }
        Expr::ForExpr { var, iterable, body } => {
            out.push((var.clone(), "a `for` variable".to_string()));
            bindings_in(iterable, out);
            bindings_in(body, out);
        }
        Expr::Quantity(_) | Expr::Identifier(_) | Expr::Str(_) | Expr::Bool(_) => {}
    }
}

/// Refuses `statement` when it binds a reserved name anywhere inside it: a global identifier of the catalog in
/// force, a statement word, a literal, a function of the core, or a function of a module the program has imported
/// (`module_domain` answers the module of such a name). Checked once for a statement before it runs, so a loop
/// does not pay for it at every iteration and a body that never runs is checked too. A name an import binds (a
/// named import or an alias, from a module or from a `.phs` file) is not checked against the module functions here:
/// running `use solve from calc` again, wherever the import sits, binds nothing new, and
/// `PhsInterpreter::unlock_domain` refuses an import that would replace or remap a name the program holds.
pub(crate) fn refuse_global_bindings(
    statement: &Statement,
    module_domain: &dyn Fn(&str) -> Option<&'static str>,
) -> PhysureResult<()> {
    let mut bound = Vec::new();
    bindings_of(statement, &mut bound);
    if bound.is_empty() {
        return Ok(());
    }
    for (name, what) in bound {
        refuse_binding(&name, &what)?;
        if !what.starts_with(IMPORT_BINDING) {
            if let Some(domain) = module_domain(&name) {
                return Err(PhysureError::Generic(crate::reserved::module_binding_error(&name, domain, &what)));
            }
        }
    }
    Ok(())
}
