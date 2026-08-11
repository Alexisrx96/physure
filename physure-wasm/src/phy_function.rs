use crate::error::to_js_error;
use crate::quantity::Quantity;
use crate::registry::{RegistryState, UnitRegistry};
use physure_core::error::{PhysureError, PhysureResult};
use physure_core::quantity::Quantity as CoreQuantity;
use physure_script::{parse_phs, PhsInterpreter, PhsValue};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

fn register_body(interpreter: &mut PhsInterpreter, body: &str) -> PhysureResult<()> {
    let statements = parse_phs(body)?;
    let stmt = statements
        .statements
        .first()
        .ok_or_else(|| PhysureError::Generic("Empty function body".into()))?;
    interpreter.run_statement(stmt)?;
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
        self.state
            .try_borrow()
            .map(|state| state.interpreter.get_fn_params(&self.name).unwrap_or_default())
            .unwrap_or_default()
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
}

fn format_call_arg(arg: &JsValue) -> Result<String, JsValue> {
    if let Some(s) = arg.as_string() {
        return Ok(s);
    }
    Err(js_sys::Error::new("PhyFunction.call: each argument must be a Quantity or a string").into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::UnitRegistry;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_node);

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
}
