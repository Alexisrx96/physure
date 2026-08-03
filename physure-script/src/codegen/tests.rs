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

    #[test]
    fn test_rust_keeps_a_grouped_divisor_grouped() {
        // `12 m / (3 s * 2)` is 2 m/s. Emitted without parentheses the target
        // language re-associates it to `12 / 3 * 2` and answers 8 — a wrong
        // number that still compiles and still carries a plausible unit.
        let res = transpile(Target::Rust, "r = 12.0 m / (3.0 s * 2.0)").unwrap();
        let rhs = res
            .lines()
            .find(|l| l.contains("let r ="))
            .expect("binding for r");
        assert!(
            rhs.contains("/ (Quantity::new(3.0, \"s\").unwrap() * Quantity::new(2.0, \"\").unwrap())"),
            "divisor lost its grouping: {rhs}"
        );
    }
}
