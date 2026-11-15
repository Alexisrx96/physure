use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use physure_core::quantity::Quantity;
use crate::symbolic::Node;

#[derive(Debug, Clone, PartialEq)]
pub struct PlotData {
    pub title: String,
    pub x_unit: String,
    pub y_unit: String,
    pub ascii: String,
    pub svg: String,
}

/// The names a function sees besides its parameters, when they are not those of whoever calls it: the environment of
/// the module it was imported from (its values, functions and imports, as the module left them), or of the caller
/// that handed it to a function of a module.
///
/// Two scopes are equal when they are the same one (a scope is never compared by its contents).
#[derive(Clone)]
pub struct ModuleScope {
    /// The module, or `"the caller"`: what an error about the scope names.
    pub(crate) module: Arc<str>,
    pub(crate) env: Arc<HashMap<String, PhsValue>>,
}

impl ModuleScope {
    pub(crate) fn new(module: &str, env: HashMap<String, PhsValue>) -> Self {
        Self { module: Arc::from(module), env: Arc::new(env) }
    }

    /// The module this scope is the environment of.
    pub fn module(&self) -> &str {
        &self.module
    }

    /// The value the scope binds to `name`.
    pub fn get(&self, name: &str) -> Option<&PhsValue> {
        self.env.get(name)
    }

    /// `value`, made to see this scope when it is a function that sees whoever calls it.
    pub(crate) fn close_over(&self, value: PhsValue) -> PhsValue {
        match value {
            PhsValue::Function(mut func) if func.scope.is_none() => {
                func.scope = Some(self.clone());
                PhsValue::Function(func)
            }
            other => other,
        }
    }
}

impl PartialEq for ModuleScope {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.env, &other.env)
    }
}

impl fmt::Debug for ModuleScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ModuleScope({})", self.module)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PhsValue {
    None,
    Number(f64),
    Quantity(Quantity),
    Bool(bool),
    String(String),
    Vector(Vec<PhsValue>),
    Matrix(physure_core::linalg::QuantityMatrix),
    Function(crate::ast::FunctionDefNode),
    Sigma(f64),
    SigmaBound(Quantity, f64),
    Plot(PlotData),
    Equation(Node, Node),
    Range(Box<PhsValue>, Box<PhsValue>),
}

impl fmt::Display for PhsValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PhsValue::None => write!(f, ""),
            PhsValue::Number(n) => write!(f, "{}", physure_core::quantity::format_float(*n)),
            PhsValue::Quantity(q) => write!(f, "{}", q),
            PhsValue::Bool(b) => write!(f, "{}", if *b { "True" } else { "False" }),
            PhsValue::String(s) => write!(f, "{}", s),
            PhsValue::Vector(v) => {
                if v.len() > 4 {
                    let first_three: Vec<String> = v[..3].iter().map(|item| item.to_string()).collect();
                    write!(f, "[{}, ... ({} items)]", first_three.join(", "), v.len())
                } else {
                    let items: Vec<String> = v.iter().map(|item| item.to_string()).collect();
                    write!(f, "[{}]", items.join(", "))
                }
            }
            PhsValue::Matrix(m) => {
                let rows: Vec<String> = m.data.iter().map(|r| {
                    let items: Vec<String> = r.iter().map(|q| q.to_string()).collect();
                    format!("[{}]", items.join(", "))
                }).collect();
                write!(f, "[{}]", rows.join(", "))
            }
            PhsValue::Sigma(k) => write!(f, "{}σ", physure_core::quantity::format_float(*k)),
            PhsValue::SigmaBound(q, k) => write!(f, "{} ± {}σ", q, physure_core::quantity::format_float(*k)),
            PhsValue::Plot(p) => write!(f, "{}", p.ascii),
            PhsValue::Function(func) => write!(f, "fn {}", func.name),
            PhsValue::Equation(lhs, rhs) => write!(f, "{} = {}", lhs.to_phs_string(), rhs.to_phs_string()),
            PhsValue::Range(start, end) => write!(f, "{} .. {}", start, end),
        }
    }
}

impl PhsValue {
    /// A short, human-readable name for this value's PHS type. Used by the strict-Bool
    /// logical operators and the `assert`/`exact_assert` dispatcher, neither of which may
    /// guess a truth -- their errors need to say what type they actually got.
    pub(crate) fn type_name(&self) -> &'static str {
        match self {
            PhsValue::None => "None",
            PhsValue::Number(_) => "Number",
            PhsValue::Quantity(_) => "Quantity",
            PhsValue::Bool(_) => "Bool",
            PhsValue::String(_) => "String",
            PhsValue::Vector(_) => "Vector",
            PhsValue::Matrix(_) => "Matrix",
            PhsValue::Function(_) => "Function",
            PhsValue::Sigma(_) => "Sigma",
            PhsValue::SigmaBound(_, _) => "SigmaBound",
            PhsValue::Plot(_) => "Plot",
            PhsValue::Equation(_, _) => "Equation",
            PhsValue::Range(_, _) => "Range",
        }
    }
}
