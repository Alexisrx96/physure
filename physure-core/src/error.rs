use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum PhysureError {
    UnitMismatch { expected: String, actual: String },
    IncompatibleDimensions { op: &'static str, dim1: String, dim2: String },
    DivisionByZero(String),
    NonConstantExponent(String),
    NonLinearArgument { function: &'static str },
    UnsupportedIntegration(String),
    ArrowError(String),
    CovarianceError(String),
    ParseError(String),
    Generic(String),
}

impl fmt::Display for PhysureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PhysureError::UnitMismatch { expected, actual } => {
                write!(f, "Unit mismatch: expected '{}', got '{}'", expected, actual)
            }
            PhysureError::IncompatibleDimensions { op, dim1, dim2 } => {
                write!(f, "Incompatible dimensions in {}: '{}' vs '{}'", op, dim1, dim2)
            }
            PhysureError::DivisionByZero(msg) => write!(f, "Division by zero: {}", msg),
            PhysureError::NonConstantExponent(msg) => write!(f, "Non-constant exponent: {}", msg),
            PhysureError::NonLinearArgument { function } => {
                write!(f, "Non-linear argument in integration for {}", function)
            }
            PhysureError::UnsupportedIntegration(msg) => {
                write!(f, "Unsupported integration pattern: {}", msg)
            }
            PhysureError::ArrowError(msg) => write!(f, "Arrow error: {}", msg),
            PhysureError::CovarianceError(msg) => write!(f, "Covariance error: {}", msg),
            PhysureError::ParseError(msg) => write!(f, "Parse error: {}", msg),
            PhysureError::Generic(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for PhysureError {}

pub type PhysureResult<T> = Result<T, PhysureError>;

impl From<String> for PhysureError {
    fn from(msg: String) -> Self {
        PhysureError::Generic(msg)
    }
}

impl From<&str> for PhysureError {
    fn from(msg: &str) -> Self {
        PhysureError::Generic(msg.to_string())
    }
}
