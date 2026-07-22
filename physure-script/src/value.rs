use std::fmt;
use physure_core::quantity::Quantity;

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
    Function(crate::ast::FunctionDefNode),
    Sigma(f64),
    SigmaBound(Quantity, f64),
    Plot(PlotData),
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
                let items: Vec<String> = v.iter().map(|item| item.to_string()).collect();
                write!(f, "[{}]", items.join(", "))
            }
            PhsValue::Sigma(k) => write!(f, "{}σ", physure_core::quantity::format_float(*k)),
            PhsValue::SigmaBound(q, k) => write!(f, "{} ± {}σ", q, physure_core::quantity::format_float(*k)),
            PhsValue::Plot(p) => write!(f, "{}", p.ascii),
            PhsValue::Function(func) => write!(f, "fn {}", func.name),
        }
    }
}
