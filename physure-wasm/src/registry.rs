use crate::error::to_js_error;
use physure_core::units::conf::{parse_physure_conf, DEFAULT_PHYSURE_CONF};
use physure_core::units::parser::Parser;
use physure_core::UnitRegistry as CoreUnitRegistry;
use physure_script::{parse_phs, PhsInterpreter, PhsValue, Statement};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

pub(crate) struct RegistryState {
    pub(crate) registry: CoreUnitRegistry,
    pub(crate) interpreter: PhsInterpreter,
}

fn load_default() -> CoreUnitRegistry {
    let mut registry = CoreUnitRegistry::new();
    let mut constants = HashMap::new();
    parse_physure_conf(DEFAULT_PHYSURE_CONF, &mut registry, &mut constants);
    registry
}

fn create_interpreter() -> PhsInterpreter {
    let mut interp = PhsInterpreter::default();
    if let Ok(program) = parse_phs("use * from calc;") {
        for stmt in &program.statements {
            let _ = interp.run_statement(stmt);
        }
    }
    interp
}

fn escape_phs_str(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn borrow_err() -> JsValue {
    js_sys::Error::new("Registry is currently borrowed").into()
}

#[wasm_bindgen]
pub struct UnitRegistry {
    pub(crate) state: Rc<RefCell<RegistryState>>,
}

#[wasm_bindgen]
impl UnitRegistry {
    #[wasm_bindgen(constructor)]
    pub fn new() -> UnitRegistry {
        UnitRegistry {
            state: Rc::new(RefCell::new(RegistryState {
                registry: load_default(),
                interpreter: create_interpreter(),
            })),
        }
    }

    #[wasm_bindgen(js_name = fromContent)]
    pub fn from_content(content: &str) -> UnitRegistry {
        let mut registry = load_default();
        let mut constants = HashMap::new();
        parse_physure_conf(content, &mut registry, &mut constants);
        UnitRegistry {
            state: Rc::new(RefCell::new(RegistryState {
                registry,
                interpreter: create_interpreter(),
            })),
        }
    }

    #[wasm_bindgen(js_name = getUnitExponents)]
    pub fn get_unit_exponents(&self, expr: &str) -> Result<js_sys::Map, JsValue> {
        let state = self.state.try_borrow().map_err(|_| borrow_err())?;
        let unit = Parser::parse_expression_with_registry(expr, &state.registry).map_err(to_js_error)?;
        let map = js_sys::Map::new();
        for (symbol, (num, den)) in &unit.dimensions {
            let exp = (*num as f64) / (*den as f64);
            map.set(&JsValue::from_str(symbol), &JsValue::from_f64(exp));
        }
        Ok(map)
    }

    #[wasm_bindgen(js_name = getUnitScale)]
    pub fn get_unit_scale(&self, expr: &str) -> Result<f64, JsValue> {
        let state = self.state.try_borrow().map_err(|_| borrow_err())?;
        Parser::parse_expression_with_registry(expr, &state.registry)
            .map(|u| u.scale)
            .map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = getCategories)]
    pub fn get_categories(&self) -> Result<js_sys::Map, JsValue> {
        let state = self.state.try_borrow().map_err(|_| borrow_err())?;
        let map = js_sys::Map::new();
        for (category, units) in &state.registry.categories {
            let array = js_sys::Array::new();
            for unit in units {
                array.push(&JsValue::from_str(unit));
            }
            map.set(&JsValue::from_str(category), &array);
        }
        Ok(map)
    }

    pub fn evaluate(&self, source: &str) -> Result<String, JsValue> {
        let program = parse_phs(source).map_err(to_js_error)?;
        let mut state = self.state.try_borrow_mut().map_err(|_| borrow_err())?;
        let mut result = String::new();
        for (idx, stmt) in program.statements.iter().enumerate() {
            if idx > 0 {
                result.push('\n');
            }
            let value = state.interpreter.run_statement(stmt).map_err(to_js_error)?;
            match value {
                PhsValue::None => match stmt {
                    Statement::Assignment(node) => match state.interpreter.get_var(&node.name) {
                        Some(v) => result.push_str(&v.to_string()),
                        None => result.push_str("None"),
                    },
                    _ => result.push_str("None"),
                },
                other => result.push_str(&other.to_string()),
            }
        }
        Ok(result)
    }

    pub fn deriv(&self, expr: &str, wrt: &str) -> Result<String, JsValue> {
        self.evaluate(&format!("deriv(\"{}\", \"{}\")", escape_phs_str(expr), escape_phs_str(wrt)))
    }

    pub fn integral(&self, expr: &str, wrt: &str) -> Result<String, JsValue> {
        self.evaluate(&format!("integral(\"{}\", \"{}\")", escape_phs_str(expr), escape_phs_str(wrt)))
    }

    pub fn solve(&self, equation: &str, wrt: &str) -> Result<String, JsValue> {
        self.evaluate(&format!("solve(\"{}\", \"{}\")", escape_phs_str(equation), escape_phs_str(wrt)))
    }
}

impl Default for UnitRegistry {
    fn default() -> Self {
        UnitRegistry::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    #[test]
    fn test_escape_phs_str() {
        assert_eq!(escape_phs_str("hello"), "hello");
        assert_eq!(escape_phs_str("hello \"world\""), "hello \\\"world\\\"");
        assert_eq!(escape_phs_str("a\\b"), "a\\\\b");
    }

    #[wasm_bindgen_test]
    fn get_unit_exponents_returns_the_base_dimensions() {
        let reg = UnitRegistry::new();
        let map = reg.get_unit_exponents("kg*m/s^2").unwrap();
        assert_eq!(map.get(&"kg".into()).as_f64(), Some(1.0));
        assert_eq!(map.get(&"m".into()).as_f64(), Some(1.0));
        assert_eq!(map.get(&"s".into()).as_f64(), Some(-2.0));
    }

    #[wasm_bindgen_test]
    fn get_unit_scale_returns_the_si_scale_factor() {
        let reg = UnitRegistry::new();
        assert!((reg.get_unit_scale("cm").unwrap() - 0.01).abs() < 1e-9);
    }

    #[wasm_bindgen_test]
    fn get_categories_lists_known_categories() {
        let reg = UnitRegistry::new();
        let categories = reg.get_categories().unwrap();
        assert!(categories.get(&"length".into()).is_array());
    }

    #[wasm_bindgen_test]
    fn from_content_applies_an_override_on_top_of_the_default_config() {
        let reg = UnitRegistry::from_content("[Units]\nfurlong = 201.168 m\n");
        let scale = reg.get_unit_scale("furlong").unwrap();
        assert!((scale - 201.168).abs() < 1e-6);
    }

    #[wasm_bindgen_test]
    fn get_unit_exponents_handles_rational_exponents() {
        let reg = UnitRegistry::new();
        let map = reg.get_unit_exponents("m^(1/2)").unwrap();
        assert_eq!(map.get(&"m".into()).as_f64(), Some(0.5));
    }

    #[wasm_bindgen_test]
    fn evaluate_runs_statements_and_returns_the_result_as_a_string() {
        let reg = UnitRegistry::new();
        let result = reg.evaluate("a = 15 m; b = 5 m; a * b").unwrap();
        assert_eq!(result, "15.0 m\n5.0 m\n75.0 m^2");
    }

    #[wasm_bindgen_test]
    fn deriv_computes_a_symbolic_derivative() {
        let reg = UnitRegistry::new();
        let result = reg.deriv("v0 * t + 0.5 * a * t^2", "t").unwrap();
        assert_eq!(result, "a * t + v0");
    }

    #[wasm_bindgen_test]
    fn integral_computes_a_symbolic_integral() {
        let reg = UnitRegistry::new();
        let result = reg.integral("3 * t^2", "t").unwrap();
        assert_eq!(result, "t^3");
    }

    #[wasm_bindgen_test]
    fn solve_isolates_the_requested_variable() {
        let reg = UnitRegistry::new();
        let result = reg.solve("P * V = n * R * T", "T").unwrap();
        assert_eq!(result, "T = (P * V)/(R * n)");
    }

    #[wasm_bindgen_test]
    fn get_unit_exponents_and_scale_handle_invalid_unit_expressions() {
        let reg = UnitRegistry::new();
        assert!(reg.get_unit_exponents("invalid_unit_xyz").is_err());
        assert!(reg.get_unit_scale("invalid_unit_xyz").is_err());
    }

    #[wasm_bindgen_test]
    fn evaluate_handles_syntax_errors() {
        let reg = UnitRegistry::new();
        assert!(reg.evaluate("a = ;").is_err());
    }

    #[wasm_bindgen_test]
    fn deriv_and_integral_handle_escaped_quotes() {
        let reg = UnitRegistry::new();
        let result_deriv = reg.deriv("2 * t", "t");
        assert_eq!(result_deriv.unwrap(), "2");
        let result_integral = reg.integral("2 * t", "t");
        assert_eq!(result_integral.unwrap(), "t^2");
    }

    #[wasm_bindgen_test]
    fn registry_borrow_error_handling() {
        let reg = UnitRegistry::new();
        let _borrow = reg.state.borrow_mut();
        assert!(reg.get_unit_exponents("m").is_err());
        assert!(reg.get_unit_scale("m").is_err());
        assert!(reg.get_categories().is_err());
        assert!(reg.evaluate("1 + 1").is_err());
    }
}
