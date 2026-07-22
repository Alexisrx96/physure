use physure_core::error::PhysureResult;
use crate::ast::{Expr, Statement};

pub mod rust;
pub mod python;
pub mod java;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Rust,
    Python,
    Java,
}

pub trait CodeGenerator {
    fn generate_program(&mut self, statements: &[Statement]) -> PhysureResult<String>;
    fn generate_statement(&mut self, stmt: &Statement) -> PhysureResult<String>;
    fn generate_expr(&mut self, expr: &Expr) -> PhysureResult<String>;
}

/// Primary public entry point for native PHS transpilation.
pub fn transpile(target: Target, phs_code: &str) -> PhysureResult<String> {
    let statements = crate::parse_phs(phs_code)?;
    match target {
        Target::Rust => rust::RustCodeGenerator::new().generate_program(&statements),
        Target::Python => python::PythonCodeGenerator::new().generate_program(&statements),
        Target::Java => java::JavaCodeGenerator::new().generate_program(&statements),
    }
}
