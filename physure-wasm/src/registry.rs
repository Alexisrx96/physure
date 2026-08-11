use crate::error::to_js_error;
use physure_core::units::conf::{parse_physure_conf, DEFAULT_PHYSURE_CONF};
use physure_core::units::parser::Parser;
use physure_core::UnitRegistry as CoreUnitRegistry;
use physure_script::PhsInterpreter;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

pub(crate) struct RegistryState {
    pub(crate) registry: CoreUnitRegistry,
    #[allow(dead_code)]
    pub(crate) interpreter: PhsInterpreter,
}

fn load_default() -> CoreUnitRegistry {
    let mut registry = CoreUnitRegistry::new();
    let mut constants = HashMap::new();
    parse_physure_conf(DEFAULT_PHYSURE_CONF, &mut registry, &mut constants);
    registry
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
                interpreter: PhsInterpreter::default(),
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
                interpreter: PhsInterpreter::default(),
            })),
        }
    }

    #[wasm_bindgen(js_name = getUnitExponents)]
    pub fn get_unit_exponents(&self, expr: &str) -> Result<js_sys::Map, JsValue> {
        let state = self.state.borrow();
        let unit = Parser::parse_expression_with_registry(expr, &state.registry).map_err(to_js_error)?;
        let map = js_sys::Map::new();
        for (symbol, (num, _den)) in &unit.dimensions {
            map.set(&JsValue::from_str(symbol), &JsValue::from_f64(*num as f64));
        }
        Ok(map)
    }

    #[wasm_bindgen(js_name = getUnitScale)]
    pub fn get_unit_scale(&self, expr: &str) -> Result<f64, JsValue> {
        let state = self.state.borrow();
        Parser::parse_expression_with_registry(expr, &state.registry)
            .map(|u| u.scale)
            .map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = getCategories)]
    pub fn get_categories(&self) -> js_sys::Map {
        let state = self.state.borrow();
        let map = js_sys::Map::new();
        for (category, units) in &state.registry.categories {
            let array = js_sys::Array::new();
            for unit in units {
                array.push(&JsValue::from_str(unit));
            }
            map.set(&JsValue::from_str(category), &array);
        }
        map
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
        let categories = reg.get_categories();
        assert!(categories.get(&"length".into()).is_array());
    }

    #[wasm_bindgen_test]
    fn from_content_applies_an_override_on_top_of_the_default_config() {
        let reg = UnitRegistry::from_content("[Units]\nfurlong = 201.168 m\n");
        let scale = reg.get_unit_scale("furlong").unwrap();
        assert!((scale - 201.168).abs() < 1e-6);
    }
}
