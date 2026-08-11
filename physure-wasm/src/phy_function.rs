use crate::error::to_js_error;
use crate::quantity::Quantity;
use crate::registry::{RegistryState, UnitRegistry};
use physure_core::error::{PhysureError, PhysureResult};
use physure_core::quantity::Quantity as CoreQuantity;
use physure_script::{parse_phs, PhsInterpreter, PhsValue};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

fn escape_phs_str(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn register_body(interpreter: &mut PhsInterpreter, body: &str) -> PhysureResult<()> {
    let statements = parse_phs(body)?;
    if statements.statements.is_empty() {
        return Err(PhysureError::Generic("Empty function body".into()));
    }
    for stmt in &statements.statements {
        interpreter.run_statement(stmt)?;
    }
    Ok(())
}

#[wasm_bindgen]
pub struct PhyFunction {
    state: Rc<RefCell<RegistryState>>,
    name: String,
}

#[wasm_bindgen]
impl PhyFunction {
    #[wasm_bindgen(constructor)]
    pub fn new(registry: &UnitRegistry, name: &str, body: &str) -> Result<PhyFunction, JsValue> {
        {
            let mut state = registry
                .state
                .try_borrow_mut()
                .map_err(|_| js_sys::Error::new("Registry is currently borrowed"))?;
            register_body(&mut state.interpreter, body).map_err(to_js_error)?;
            if state.interpreter.get_fn_params(name).is_none() {
                return Err(to_js_error(PhysureError::Generic(format!(
                    "Function '{}' was not defined in the body",
                    name
                ))));
            }
        }
        Ok(PhyFunction {
            state: registry.state.clone(),
            name: name.to_string(),
        })
    }

    #[wasm_bindgen(js_name = getName)]
    pub fn get_name(&self) -> String {
        self.name.clone()
    }

    #[wasm_bindgen(js_name = getParams)]
    pub fn get_params(&self) -> Vec<String> {
        self.try_get_params().unwrap_or_default()
    }

    #[wasm_bindgen(variadic)]
    pub fn call(&self, args: Vec<JsValue>) -> Result<Quantity, JsValue> {
        let mut formatted = Vec::with_capacity(args.len());
        for arg in &args {
            formatted.push(format_call_arg(arg)?);
        }
        let call_expr = format!("{}({})", self.name, formatted.join(", "));
        let statements = parse_phs(&call_expr).map_err(to_js_error)?;
        let stmt = statements
            .statements
            .first()
            .ok_or_else(|| to_js_error(PhysureError::Generic("Empty call expression".into())))?;
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| js_sys::Error::new("Registry is currently borrowed"))?;
        match state.interpreter.run_statement(stmt).map_err(to_js_error)? {
            PhsValue::Quantity(q) => Ok(Quantity::from_core(q)),
            PhsValue::Number(n) => Ok(Quantity::from_core(CoreQuantity::new_scalar(
                n,
                0.0,
                physure_core::units::RationalUnit::dimensionless(),
                None,
                None,
            ))),
            other => Err(to_js_error(PhysureError::Generic(format!(
                "Call did not return a quantity: {:?}",
                other
            )))),
        }
    }

    pub fn deriv(&self, wrt: &str) -> Result<PhyFunction, JsValue> {
        let params = self.try_get_params()?;
        if params.is_empty() {
            return Err(to_js_error(PhysureError::Generic(
                "Cannot differentiate a function with no parameters".into(),
            )));
        }
        let params_joined = params.join(", ");
        let call_expr = format!("{}({})", self.name, params_joined);
        let deriv_result = self.evaluate_string(&format!(
            "deriv(\"{}\", \"{}\")",
            escape_phs_str(&call_expr),
            escape_phs_str(wrt)
        ))?;
        let new_name = format!("d_{}_d_{}", self.name, wrt);
        let new_body = format!("{}({}) = {}", new_name, params_joined, deriv_result);
        self.define(&new_name, &new_body)
    }

    pub fn integral(&self, wrt: &str) -> Result<PhyFunction, JsValue> {
        let params = self.try_get_params()?;
        if params.is_empty() {
            return Err(to_js_error(PhysureError::Generic(
                "Cannot integrate a function with no parameters".into(),
            )));
        }
        let params_joined = params.join(", ");
        let call_expr = format!("{}({})", self.name, params_joined);
        let integral_result = self.evaluate_string(&format!(
            "integral(\"{}\", \"{}\")",
            escape_phs_str(&call_expr),
            escape_phs_str(wrt)
        ))?;
        let new_name = format!("i_{}_d_{}", self.name, wrt);
        let new_body = format!("{}({}) = {}", new_name, params_joined, integral_result);
        self.define(&new_name, &new_body)
    }

    pub fn solve(&self, wrt: &str) -> Result<PhyFunction, JsValue> {
        let params = self.try_get_params()?;
        if params.is_empty() {
            return Err(to_js_error(PhysureError::Generic(
                "Cannot solve a function with no parameters".into(),
            )));
        }
        if !params.contains(&wrt.to_string()) {
            return Err(to_js_error(PhysureError::Generic(format!(
                "Parameter '{}' not found in function parameters",
                wrt
            ))));
        }

        let target_param = if !params.contains(&"target".to_string()) {
            "target".to_string()
        } else {
            let mut i = 1;
            loop {
                let candidate = format!("target_{}", i);
                if !params.contains(&candidate) {
                    break candidate;
                }
                i += 1;
            }
        };

        let params_joined = params.join(", ");
        let call_expr = format!("{}({})", self.name, params_joined);
        let solve_result = self.evaluate_string(&format!(
            "solve(\"{} = {}\", \"{}\")",
            escape_phs_str(&call_expr),
            escape_phs_str(&target_param),
            escape_phs_str(wrt)
        ))?;
        let new_name = format!("solve_{}_for_{}", self.name, wrt);
        let other_params: Vec<String> = params.into_iter().filter(|p| p != wrt).collect();
        let mut new_params = vec![target_param];
        new_params.extend(other_params);
        let new_params_joined = new_params.join(", ");
        let new_body = format!("{}({}) = {}", new_name, new_params_joined, solve_result);
        self.define(&new_name, &new_body)
    }
}

impl PhyFunction {
    fn try_get_params(&self) -> Result<Vec<String>, JsValue> {
        let state = self
            .state
            .try_borrow()
            .map_err(|_| js_sys::Error::new("Registry is currently borrowed"))?;
        state
            .interpreter
            .get_fn_params(&self.name)
            .ok_or_else(|| to_js_error(PhysureError::Generic(format!("Function '{}' not found", self.name))))
    }

    fn evaluate_string(&self, expr: &str) -> Result<String, JsValue> {
        let statements = parse_phs(expr).map_err(to_js_error)?;
        let stmt = statements
            .statements
            .first()
            .ok_or_else(|| to_js_error(PhysureError::Generic("Empty expression".into())))?;
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| js_sys::Error::new("Registry is currently borrowed"))?;
        match state.interpreter.run_statement(stmt).map_err(to_js_error)? {
            PhsValue::String(s) => Ok(s),
            PhsValue::Equation(_, rhs) => Ok(rhs.to_phs_string()),
            other => Err(to_js_error(PhysureError::Generic(format!(
                "Expected a string or equation result, got: {:?}",
                other
            )))),
        }
    }

    fn define(&self, name: &str, body: &str) -> Result<PhyFunction, JsValue> {
        {
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| js_sys::Error::new("Registry is currently borrowed"))?;
            register_body(&mut state.interpreter, body).map_err(to_js_error)?;
        }
        Ok(PhyFunction {
            state: self.state.clone(),
            name: name.to_string(),
        })
    }
}

fn format_call_arg(arg: &JsValue) -> Result<String, JsValue> {
    if let Some(s) = arg.as_string() {
        return Ok(s);
    }
    if is_quantity_instance(arg) {
        if let Ok(to_string_val) = js_sys::Reflect::get(arg, &JsValue::from_str("toString")) {
            if to_string_val.is_function() {
                let func: js_sys::Function = to_string_val.unchecked_into();
                if let Ok(res) = js_sys::Reflect::apply(&func, arg, &js_sys::Array::new()) {
                    if let Some(s) = res.as_string() {
                        return Ok(s);
                    }
                }
            }
        }
    }
    Err(js_sys::Error::new("PhyFunction.call: each argument must be a Quantity or a string").into())
}

fn is_quantity_instance(arg: &JsValue) -> bool {
    if let Ok(constructor) = js_sys::Reflect::get(arg, &JsValue::from_str("constructor")) {
        if let Ok(name) = js_sys::Reflect::get(&constructor, &JsValue::from_str("name")) {
            if let Some(s) = name.as_string() {
                return s == "Quantity";
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::UnitRegistry;
    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    fn constructs_registers_and_reports_params_and_name() {
        let registry = UnitRegistry::new();
        let ke = PhyFunction::new(&registry, "kinetic_energy", "kinetic_energy(m, v) = 0.5 * m * v^2").unwrap();
        assert_eq!(ke.get_params(), vec!["m".to_string(), "v".to_string()]);
        assert_eq!(ke.get_name(), "kinetic_energy");
    }

    #[wasm_bindgen_test]
    fn call_with_phs_literal_string_arguments() {
        let registry = UnitRegistry::new();
        let ke = PhyFunction::new(&registry, "kinetic_energy", "kinetic_energy(m, v) = 0.5 * m * v^2").unwrap();
        let result = ke.call(vec![JsValue::from_str("10 kg"), JsValue::from_str("5 m/s")]).unwrap();
        assert_eq!(result.value(), 125.0);
        assert_eq!(result.unit(), "J");
    }

    #[wasm_bindgen_test]
    fn call_with_quantity_arguments() {
        use crate::quantity::Quantity;

        let registry = UnitRegistry::new();
        let ke = PhyFunction::new(&registry, "kinetic_energy", "kinetic_energy(m, v) = 0.5 * m * v^2").unwrap();
        let mass = Quantity::new(10.0, "kg").unwrap();
        let speed = Quantity::new(5.0, "m/s").unwrap();
        let result = ke.call(vec![JsValue::from(mass), JsValue::from(speed)]).unwrap();
        assert_eq!(result.value(), 125.0);
        assert_eq!(result.unit(), "J");
    }

    #[wasm_bindgen_test]
    fn call_rejects_an_argument_that_is_neither_a_quantity_nor_a_string() {
        let registry = UnitRegistry::new();
        let ke = PhyFunction::new(&registry, "kinetic_energy", "kinetic_energy(m, v) = 0.5 * m * v^2").unwrap();
        let err = ke.call(vec![JsValue::from_f64(10.0)]).unwrap_err();
        use wasm_bindgen::JsCast;
        let js_error = err.dyn_into::<js_sys::Error>().expect("should be a JS Error");
        assert!(String::from(js_error.message()).contains("must be a Quantity or a string"));
    }

    #[wasm_bindgen_test]
    fn register_body_executes_all_statements_in_multi_statement_body() {
        let registry = UnitRegistry::new();
        let energy_fn = PhyFunction::new(
            &registry,
            "energy",
            "c = 3e8 m/s; energy(m) = m * c^2",
        )
        .unwrap();
        let result = energy_fn
            .call(vec![JsValue::from_str("2 kg")])
            .unwrap();
        assert_eq!(result.value(), 1.8e17);
        assert_eq!(result.unit(), "J");
    }

    #[wasm_bindgen_test]
    fn call_function_returning_dimensionless_scalar_number() {
        let registry = UnitRegistry::new();
        let scale_fn = PhyFunction::new(
            &registry,
            "scale",
            "scale(x) = x * 2",
        )
        .unwrap();
        let result = scale_fn
            .call(vec![JsValue::from_str("5")])
            .unwrap();
        assert_eq!(result.value(), 10.0);
        assert_eq!(result.unit(), "");
    }

    #[wasm_bindgen_test]
    fn registry_lock_contention() {
        use wasm_bindgen::JsCast;

        let registry = UnitRegistry::new();
        let ke = PhyFunction::new(&registry, "kinetic_energy", "kinetic_energy(m, v) = 0.5 * m * v^2").unwrap();

        let _borrow = registry.state.borrow_mut();

        let new_err = PhyFunction::new(&registry, "foo", "foo(x) = x").err().unwrap();
        let js_err = new_err.dyn_into::<js_sys::Error>().expect("should be a JS Error");
        assert!(String::from(js_err.message()).contains("Registry is currently borrowed"));

        let call_err = ke.call(vec![JsValue::from_str("10 kg"), JsValue::from_str("5 m/s")]).unwrap_err();
        let js_err2 = call_err.dyn_into::<js_sys::Error>().expect("should be a JS Error");
        assert!(String::from(js_err2.message()).contains("Registry is currently borrowed"));

        let params = ke.get_params();
        assert!(params.is_empty());
    }

    #[wasm_bindgen_test]
    fn deriv_returns_a_new_callable_phy_function() {
        let registry = UnitRegistry::new();
        let ke = PhyFunction::new(&registry, "kinetic_energy", "kinetic_energy(m, v) = 0.5 * m * v^2").unwrap();
        let dke_dv = ke.deriv("v").unwrap();
        assert_eq!(dke_dv.get_params(), vec!["m".to_string(), "v".to_string()]);
        let result = dke_dv
            .call(vec![JsValue::from_str("10 kg"), JsValue::from_str("5 m/s")])
            .unwrap();
        assert_eq!(result.value(), 50.0);
    }

    #[wasm_bindgen_test]
    fn integral_returns_a_new_callable_phy_function() {
        let registry = UnitRegistry::new();
        let ke = PhyFunction::new(&registry, "kinetic_energy", "kinetic_energy(m, v) = 0.5 * m * v^2").unwrap();
        let ike_dv = ke.integral("v").unwrap();
        let result = ike_dv
            .call(vec![JsValue::from_str("10 kg"), JsValue::from_str("5 m/s")])
            .unwrap();
        assert!((result.value() - 208.33333333333334).abs() < 1e-9);
    }

    #[wasm_bindgen_test]
    fn solve_returns_a_phy_function_with_target_as_the_first_parameter() {
        let registry = UnitRegistry::new();
        let ke = PhyFunction::new(&registry, "kinetic_energy", "kinetic_energy(m, v) = 0.5 * m * v^2").unwrap();
        let solve_for_v = ke.solve("v").unwrap();
        assert_eq!(solve_for_v.get_params(), vec!["target".to_string(), "m".to_string()]);
        let result = solve_for_v
            .call(vec![JsValue::from_str("125 J"), JsValue::from_str("10 kg")])
            .unwrap();
        assert_eq!(result.value(), 5.0);
    }

    #[wasm_bindgen_test]
    fn phy_function_new_rejects_mismatched_function_name() {
        use wasm_bindgen::JsCast;
        let registry = UnitRegistry::new();
        let err = PhyFunction::new(&registry, "g", "f(x) = x * 2").unwrap_err();
        let js_err = err.dyn_into::<js_sys::Error>().expect("should be a JS Error");
        assert!(String::from(js_err.message()).contains("Function 'g' was not defined in the body"));
    }

    #[wasm_bindgen_test]
    fn solve_handles_parameter_collision_with_target() {
        let registry = UnitRegistry::new();
        let f = PhyFunction::new(&registry, "f", "f(target, x) = target + x").unwrap();
        let solve_x = f.solve("x").unwrap();
        assert_eq!(solve_x.get_params(), vec!["target_1".to_string(), "target".to_string()]);
        let result = solve_x
            .call(vec![JsValue::from_str("10"), JsValue::from_str("3")])
            .unwrap();
        assert_eq!(result.value(), 7.0);
    }

    #[wasm_bindgen_test]
    fn solve_rejects_invalid_wrt_parameter() {
        use wasm_bindgen::JsCast;
        let registry = UnitRegistry::new();
        let ke = PhyFunction::new(&registry, "kinetic_energy", "kinetic_energy(m, v) = 0.5 * m * v^2").unwrap();
        let err = ke.solve("nonexistent").unwrap_err();
        let js_err = err.dyn_into::<js_sys::Error>().expect("should be a JS Error");
        assert!(String::from(js_err.message()).contains("Parameter 'nonexistent' not found in function parameters"));
    }

    #[wasm_bindgen_test]
    fn test_escape_phs_str() {
        assert_eq!(escape_phs_str(r#"a\b"c"#), r#"a\\b\"c"#);
    }

    #[wasm_bindgen_test]
    fn deriv_and_integral_handle_string_escaping() {
        use wasm_bindgen::JsCast;
        let registry = UnitRegistry::new();
        let ke = PhyFunction::new(&registry, "kinetic_energy", "kinetic_energy(m, v) = 0.5 * m * v^2").unwrap();
        let err = ke.deriv(r#"v\"invalid"#).unwrap_err();
        let js_err = err.dyn_into::<js_sys::Error>().expect("should be a JS Error");
        assert!(!String::from(js_err.message()).contains("syntax error") && !String::from(js_err.message()).contains("Empty expression"));

        let err2 = ke.integral(r#"v\"invalid"#).unwrap_err();
        let js_err2 = err2.dyn_into::<js_sys::Error>().expect("should be a JS Error");
        assert!(!String::from(js_err2.message()).contains("syntax error") && !String::from(js_err2.message()).contains("Empty expression"));
    }
}


