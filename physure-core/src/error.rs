use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum PhysureError {
    UnitMismatch { expected: String, actual: String },
    UnknownUnit { symbol: String, suggestion: Option<String> },
    IncompatibleDimensions { op: &'static str, dim1: String, dim2: String },
    DivisionByZero(String),
    NonConstantExponent(String),
    NonLinearArgument { function: &'static str },
    UnsupportedIntegration(String),
    ArrowError(String),
    CovarianceError(String),
    ParseError(String),
    Generic(String),
    /// A `@requires`/`@ensures` condition evaluated to false. `decorator` is the
    /// decorator name without the `@` (`"requires"` or `"ensures"`); `message` is the
    /// user-supplied explanation string from the decorator's second argument.
    ContractViolation { decorator: String, message: String },
}

impl fmt::Display for PhysureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PhysureError::UnitMismatch { expected, actual } => {
                write!(f, "Unit mismatch: expected '{}', got '{}'", expected, actual)
            }
            PhysureError::UnknownUnit { symbol, suggestion: Some(hint) } => {
                write!(f, "Unknown unit '{}' — did you mean '{}'?", symbol, hint)
            }
            PhysureError::UnknownUnit { symbol, suggestion: None } => {
                write!(f, "Unknown unit '{}'", symbol)
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
            PhysureError::ContractViolation { decorator, message } => {
                write!(f, "@{} violated: {}", decorator, message)
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_violation_displays_decorator_and_message() {
        let err = PhysureError::ContractViolation {
            decorator: "requires".to_string(),
            message: "x must be positive".to_string(),
        };
        assert_eq!(err.to_string(), "@requires violated: x must be positive");
    }
}
