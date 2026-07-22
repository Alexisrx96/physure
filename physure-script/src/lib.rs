pub mod ast;
pub mod lexer;

pub use ast::{Expr, Statement};
pub use lexer::{PhsLexer, PhsToken, TokenKind};

#[derive(Default, Clone)]
pub struct PhsInterpreter {}
impl PhsInterpreter {
    pub fn new() -> Self { Self {} }
    pub fn with_registry(_reg: physure_core::UnitRegistry) -> Self { Self {} }
    pub fn registry_mut(&mut self) -> &mut physure_core::UnitRegistry {
        unimplemented!()
    }
    pub fn run_statement(&mut self, _stmt: &Statement) -> Result<PhsValue, physure_core::error::PhysureError> {
        Ok(PhsValue::None)
    }
    pub fn get_var(&self, _name: &str) -> Option<PhsValue> { None }
    pub fn get_fn_params(&self, _name: &str) -> Option<Vec<String>> { None }
}

pub fn parse_phs(_input: &str) -> Result<Vec<Statement>, physure_core::error::PhysureError> {
    Ok(vec![])
}

pub fn eval_phs(_input: &str) -> Result<Vec<PhsValue>, physure_core::error::PhysureError> {
    Ok(vec![])
}

pub fn transpile(_ast: &[Statement], _target: Target) -> Result<String, physure_core::error::PhysureError> { Ok(String::new()) }
pub enum Target { Python, Javascript }

#[derive(Clone, PartialEq)]
pub enum PhsValue {
    None,
    Number(f64),
    Bool(bool),
    String(String),
    Quantity(physure_core::quantity::Quantity),
    Sigma(f64),
    SigmaBound(physure_core::quantity::Quantity, f64),
    Plot(PlotData),
    Vector(Vec<PhsValue>),
}

impl std::fmt::Display for PhsValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PhsValue")
    }
}

#[derive(Clone, PartialEq)]
pub struct PlotData {
    pub ascii: String,
}

pub mod symbolic {
    #[derive(Clone, PartialEq, Debug, Hash)]
    pub struct Expr;
    impl Expr {
        pub fn parse(_s: &str) -> Result<Self, physure_core::error::PhysureError> { Ok(Expr) }
        pub fn symbol(_s: &str) -> Self { Expr }
        pub fn quantity(_s: &str, _u: &physure_core::RationalUnit) -> Self { Expr }
        pub fn variable(_s: &str) -> Self { Expr }
        pub fn number(_f: f64) -> Self { Expr }
        pub fn constant(_s: &str) -> Self { Expr }
        pub fn sin(&self) -> Self { Expr }
        pub fn cos(&self) -> Self { Expr }
        pub fn ln(&self) -> Self { Expr }
        pub fn exp(&self) -> Self { Expr }
        pub fn add(&self, _other: &Expr) -> Result<Self, physure_core::error::PhysureError> { Ok(Expr) }
        pub fn sub(&self, _other: &Expr) -> Result<Self, physure_core::error::PhysureError> { Ok(Expr) }
        pub fn mul(&self, _other: &Expr) -> Self { Expr }
        pub fn div(&self, _other: &Expr) -> Self { Expr }
        pub fn pow(&self, _other: &Expr) -> Self { Expr }
        pub fn simplify(&self) -> Self { Expr }
        pub fn factor(&self) -> Self { Expr }
        pub fn diff(&self, _var: &str, _n: usize) -> Result<Self, physure_core::error::PhysureError> { Ok(Expr) }
        pub fn integrate(&self, _var: &str) -> Result<Self, physure_core::error::PhysureError> { Ok(Expr) }
        pub fn unit(&self) -> Result<Option<physure_core::RationalUnit>, physure_core::error::PhysureError> { Ok(None) }
    }
}

pub mod value {
    pub use super::{PhsValue, PlotData};
}

pub fn unit_to_latex(_unit_str: &str) -> String {
    String::new()
}
