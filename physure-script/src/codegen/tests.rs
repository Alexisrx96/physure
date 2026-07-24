#[cfg(test)]
mod tests {
    use crate::codegen::{transpile, Target};

    #[test]
    fn test_transpile_rust_basic() {
        let code = "m_p = 1.673e-27 kg\nm_p * 3";
        let res = transpile(Target::Rust, code).unwrap();
        assert!(res.contains("Quantity::new(1.673e-27, \"kg\")"));
        assert!(!res.contains("PhsInterpreter"));
    }

    #[test]
    fn test_transpile_python_basic() {
        let code = "m_p = 1.673e-27 kg\nm_p * 3";
        let res = transpile(Target::Python, code).unwrap();
        assert!(res.contains("Q_(1.673e-27, 'kg')"));
    }

    #[test]
    fn test_transpile_java_basic() {
        let code = "m_p = 1.673e-27 kg\nm_p * 3";
        let res = transpile(Target::Java, code).unwrap();
        assert!(res.contains("new Quantity(1.673e-27, \"kg\")"));
    }
}
