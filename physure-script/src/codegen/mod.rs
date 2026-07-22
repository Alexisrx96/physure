use crate::ast::Program;

#[derive(Debug)]
pub enum CodegenError {
    Generic(String),
}

impl std::fmt::Display for CodegenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodegenError::Generic(msg) => write!(f, "Codegen error: {}", msg),
        }
    }
}

impl std::error::Error for CodegenError {}

pub trait CodeGenerator {
    fn generate_program(&self, program: &Program) -> Result<String, CodegenError>;
}

pub mod python;
pub mod rust;
pub mod java;

pub enum Target {
    Python,
    Rust,
    Java,
}

pub fn transpile(program: &Program, target: Target) -> Result<String, CodegenError> {
    match target {
        Target::Python => python::PythonTranspiler.generate_program(program),
        Target::Rust => rust::RustTranspiler.generate_program(program),
        Target::Java => java::JavaTranspiler.generate_program(program),
    }
}
