use crate::error::to_js_error;
use physure_core::quantity::Quantity as CoreQuantity;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
#[derive(Debug)]
pub struct Quantity {
    pub(crate) inner: CoreQuantity,
}

#[wasm_bindgen]
impl Quantity {
    #[wasm_bindgen(constructor)]
    pub fn new(value: f64, unit: &str) -> Result<Quantity, JsValue> {
        CoreQuantity::new(value, unit)
            .map(Quantity::from_core)
            .map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = withUncertainty)]
    pub fn with_uncertainty(value: f64, uncertainty: f64, unit: &str) -> Result<Quantity, JsValue> {
        CoreQuantity::with_uncertainty(value, uncertainty, unit)
            .map(Quantity::from_core)
            .map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = toString)]
    pub fn to_js_string(&self) -> String {
        self.inner.to_string()
    }

    #[wasm_bindgen(getter)]
    pub fn value(&self) -> f64 {
        self.inner.value.mean()
    }

    #[wasm_bindgen(getter)]
    pub fn uncertainty(&self) -> f64 {
        self.inner.value.std_dev()
    }

    #[wasm_bindgen(getter)]
    pub fn unit(&self) -> String {
        self.inner.unit.__repr__()
    }
}

impl Quantity {
    pub(crate) fn from_core(inner: CoreQuantity) -> Self {
        Quantity { inner }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen::JsCast;
    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    fn constructs_a_quantity_and_exposes_its_getters() {
        let q = Quantity::new(75.0, "kg").unwrap();
        assert_eq!(q.value(), 75.0);
        assert_eq!(q.uncertainty(), 0.0);
        assert_eq!(q.unit(), "kg");
    }

    #[wasm_bindgen_test]
    fn constructs_a_quantity_with_uncertainty() {
        let q = Quantity::with_uncertainty(75.0, 0.5, "kg").unwrap();
        assert_eq!(q.value(), 75.0);
        assert_eq!(q.uncertainty(), 0.5);
    }

    #[wasm_bindgen_test]
    fn to_js_string_renders_value_and_unit() {
        let q = Quantity::new(5.0, "m/s").unwrap();
        // physure_core's Display always prints a decimal point for whole-number
        // magnitudes (format_float(5.0) => "5.0"), so this is "5.0 m/s", not "5 m/s".
        assert_eq!(q.to_js_string(), "5.0 m/s");
    }

    #[wasm_bindgen_test]
    fn rejects_an_unknown_unit_with_a_catchable_error() {
        let err = Quantity::new(1.0, "not_a_real_unit").unwrap_err();
        let js_error = err
            .dyn_into::<js_sys::Error>()
            .expect("should be a JS Error");
        assert!(String::from(js_error.message()).contains("Unknown unit"));
    }
}
