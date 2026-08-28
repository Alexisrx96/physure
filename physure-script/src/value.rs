use std::fmt;
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
    /// fall back to `is_truthy` -- their errors need to say what type they actually got.
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
