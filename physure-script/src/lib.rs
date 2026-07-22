pub mod ast;
pub mod lexer;
pub mod parser;
pub mod resolver;
pub mod interpreter;
pub mod codegen;
pub mod exporter;

pub use codegen::{transpile, Target};

pub use ast::{Expr, Program, Statement};
pub use lexer::{PhsLexer, PhsToken, TokenKind};
pub use parser::parse_phs;
pub use interpreter::{eval_phs, PhsInterpreter};

#[derive(Clone, PartialEq, Debug)]
pub enum PhsValue {
    None,
    Number(f64),
    Bool(bool),
    String(String),
    Quantity(physure_core::quantity::Quantity),
    Function(ast::FunctionDefNode),
    Vector(Vec<PhsValue>),
}

impl std::fmt::Display for PhsValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PhsValue::None => write!(f, "None"),
            PhsValue::Number(n) => write!(f, "{}", n),
            PhsValue::Bool(b) => write!(f, "{}", b),
            PhsValue::String(s) => write!(f, "{}", s),
            PhsValue::Quantity(q) => write!(f, "{}", q),
            PhsValue::Function(func) => write!(f, "Function({})", func.name),
            PhsValue::Vector(v) => write!(f, "Vector({} elements)", v.len()),
        }
    }
}
