use physure_core::error::PhysureError;
use wasm_bindgen::JsValue;

pub fn to_js_error(err: PhysureError) -> JsValue {
    js_sys::Error::new(&err.to_string()).into()
}

#[cfg(test)]
mod tests {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    fn converts_a_physure_error_into_a_js_error_with_the_same_message() {
        let err = physure_core::error::PhysureError::DivisionByZero("velocity".to_string());
        let js_err = super::to_js_error(err);
        let js_error = js_err
            .dyn_into::<js_sys::Error>()
            .expect("should convert into a JS Error instance");
        assert_eq!(String::from(js_error.message()), "Division by zero: velocity");
    }
}
