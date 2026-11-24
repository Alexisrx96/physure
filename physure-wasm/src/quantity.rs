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

    /// A Type A evaluation (JCGM 100:2008 4.2) of repeated observations: the mean, its standard uncertainty
    /// `s / sqrt(n)` and `n - 1` degrees of freedom. Observations that are all equal give an exact quantity.
    #[wasm_bindgen(js_name = fromSamples)]
    pub fn from_samples(samples: Vec<f64>, unit: &str) -> Result<Quantity, JsValue> {
        CoreQuantity::from_samples(&samples, unit)
            .map(Quantity::from_core)
            .map_err(to_js_error)
    }

    /// This measurement with `nu` degrees of freedom declared for its standard uncertainty. The degrees of freedom
    /// belong to one measurement: an exact quantity, a result that combines several, `nu` that is not positive or
    /// differs from what the measurement already has is an error.
    #[wasm_bindgen(js_name = withDof)]
    pub fn with_dof(&self, nu: f64) -> Result<Quantity, JsValue> {
        self.inner.clone().with_dof(nu).map(Quantity::from_core).map_err(to_js_error)
    }

    /// The effective degrees of freedom (Welch-Satterthwaite, JCGM 100:2008 G.4.1); `Infinity` when none of the
    /// measurements it comes from has a finite number.
    #[wasm_bindgen(js_name = effectiveDof)]
    pub fn effective_dof(&self) -> Result<f64, JsValue> {
        self.inner.effective_dof().map_err(to_js_error)
    }

    /// The coverage factor for the coverage probability `p` in (0, 1): the Student-t factor at the effective
    /// degrees of freedom truncated to an integer, the normal factor when they are infinite.
    #[wasm_bindgen(js_name = coverageFactor)]
    pub fn coverage_factor(&self, p: f64) -> Result<f64, JsValue> {
        self.inner.coverage_factor(p).map_err(to_js_error)
    }

    /// The expanded uncertainty `U = k u_c` for the coverage probability `p`, in the unit `uncertainty` is in.
    #[wasm_bindgen(js_name = expandedUncertainty)]
    pub fn expanded_uncertainty(&self, p: f64) -> Result<f64, JsValue> {
        Ok(self.inner.coverage_factor(p).map_err(to_js_error)? * self.uncertainty())
    }

    #[wasm_bindgen(js_name = toString)]
    pub fn to_js_string(&self) -> String {
        self.inner.to_string()
    }

    // The three getters read one consistent triple, the one `toString` shows: `2 m * 3 km` is 6000 m^2.
    // The stored magnitude (6) next to the scale-less symbol `m^2` would be 1000 times too small.
    #[wasm_bindgen(getter)]
    pub fn value(&self) -> f64 {
        self.inner.display_parts().0
    }

    #[wasm_bindgen(getter)]
    pub fn uncertainty(&self) -> f64 {
        self.inner.display_parts().1
    }

    #[wasm_bindgen(getter)]
    pub fn unit(&self) -> String {
        self.inner.display_parts().2
    }

    pub fn add(&self, other: &Quantity) -> Result<Quantity, JsValue> {
        self.inner.add(&other.inner).map(Quantity::from_core).map_err(to_js_error)
    }

    pub fn subtract(&self, other: &Quantity) -> Result<Quantity, JsValue> {
        self.inner.sub(&other.inner).map(Quantity::from_core).map_err(to_js_error)
    }

    pub fn multiply(&self, other: &Quantity) -> Result<Quantity, JsValue> {
        self.inner.mul(&other.inner).map(Quantity::from_core).map_err(to_js_error)
    }

    pub fn divide(&self, other: &Quantity) -> Result<Quantity, JsValue> {
        self.inner.div(&other.inner).map(Quantity::from_core).map_err(to_js_error)
    }

    pub fn pow(&self, exponent: f64) -> Result<Quantity, JsValue> {
        self.inner.pow(exponent).map(Quantity::from_core).map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = convertTo)]
    pub fn convert_to(&self, unit: &str) -> Result<Quantity, JsValue> {
        self.inner.to(unit).map(Quantity::from_core).map_err(to_js_error)
    }

    /// PHS `<`, `>`, `<=`, `>=`, `==`, `!=` (`op`) by the core's rule (`Quantity::phs_compare`): canonical
    /// magnitudes, an error for different dimensions, a zero without a unit, or a level against a plain number.
    /// What JavaScript and TypeScript transpiled from PHS call, so that they answer what the interpreter answers.
    #[wasm_bindgen(js_name = phsCompare)]
    pub fn phs_compare(&self, other: &Quantity, op: &str) -> Result<bool, JsValue> {
        self.inner.phs_compare(&other.inner, op).map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = physAssert)]
    pub fn phs_assert(&self, other: &Quantity) -> Result<(), JsValue> {
        self.inner.phs_assert(&other.inner).map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = physExactAssert)]
    pub fn phs_exact_assert(&self, other: &Quantity) -> Result<(), JsValue> {
        self.inner.phs_exact_assert(&other.inner).map_err(to_js_error)
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

    #[wasm_bindgen_test]
    fn arithmetic_and_conversion_between_compatible_units() {
        let d = Quantity::new(10.0, "m").unwrap();
        let t = Quantity::new(2.0, "s").unwrap();
        let v = d.divide(&t).unwrap();
        assert_eq!(v.value(), 5.0);
        assert_eq!(v.unit(), "m * s^-1");

        let v_kmh = v.convert_to("km/h").unwrap();
        assert!((v_kmh.value() - 18.0).abs() < 1e-9);

        let doubled = v.multiply(&Quantity::new(2.0, "").unwrap()).unwrap();
        assert_eq!(doubled.value(), 10.0);

        let squared = v.pow(2.0).unwrap();
        assert_eq!(squared.value(), 25.0);
        assert_eq!(squared.unit(), "m^2 * s^-2");
    }

    #[wasm_bindgen_test]
    fn add_and_subtract_require_compatible_dimensions() {
        let m = Quantity::new(5.0, "m").unwrap();
        let s = Quantity::new(5.0, "s").unwrap();
        let err = m.add(&s).unwrap_err();
        let js_error = err
            .dyn_into::<js_sys::Error>()
            .expect("should be a JS Error");
        assert!(
            String::from(js_error.message()).contains("Unit mismatch")
                || String::from(js_error.message()).contains("Incompatible dimensions")
        );
    }

    #[wasm_bindgen_test]
    fn phys_assert_passes_for_equal_dimension_and_magnitude() {
        let a = Quantity::new(1.0, "km").unwrap();
        let b = Quantity::new(1000.0, "m").unwrap();
        assert!(a.phs_assert(&b).is_ok());
    }

    #[wasm_bindgen_test]
    fn phys_assert_throws_a_js_error_on_mismatch() {
        let a = Quantity::new(1.0, "m").unwrap();
        let b = Quantity::new(1.0, "s").unwrap();
        let err = a.phs_assert(&b).unwrap_err();
        let js_error = err.dyn_into::<js_sys::Error>().expect("should be a JS Error");
        assert!(String::from(js_error.message()).contains("assert failed"));
    }

    #[wasm_bindgen_test]
    fn phs_compare_is_the_interpreter_s_comparison() {
        let km = Quantity::new(1.0, "km").unwrap();
        assert!(km.phs_compare(&Quantity::new(999.0, "m").unwrap(), ">").unwrap());
        assert!(km.phs_compare(&Quantity::new(1000.0, "m").unwrap(), "==").unwrap());
        for (left, right) in [((1.0, "m"), (0.0, "")), ((20.0, "dB"), (100.0, "")), ((1.0, "m"), (1.0, "s"))] {
            let err = Quantity::new(left.0, left.1).unwrap().phs_compare(&Quantity::new(right.0, right.1).unwrap(), ">");
            let js_error = err.unwrap_err().dyn_into::<js_sys::Error>().expect("should be a JS Error");
            assert!(String::from(js_error.message()).contains("compare") || String::from(js_error.message()).contains("mismatch"));
        }
        // a temperature point and a temperature interval have no comparison; K compares with both
        let err = Quantity::new(5.0, "delta_degC").unwrap().phs_compare(&Quantity::new(20.0, "degC").unwrap(), "<");
        let js_error = err.unwrap_err().dyn_into::<js_sys::Error>().expect("should be a JS Error");
        assert!(String::from(js_error.message()).contains("with a temperature interval"));
        assert!(Quantity::new(20.0, "degC").unwrap().phs_compare(&Quantity::new(10.0, "K").unwrap(), ">").unwrap());
    }

    #[wasm_bindgen_test]
    fn a_temperature_point_and_an_interval_do_not_convert_and_a_difference_of_points_is_an_interval() {
        let message = |err: JsValue| String::from(err.dyn_into::<js_sys::Error>().expect("should be a JS Error").message());
        let point = Quantity::new(20.0, "degC").unwrap();
        assert!(message(point.convert_to("delta_degC").unwrap_err()).contains("cannot convert a temperature point"));
        assert!(message(Quantity::new(5.0, "delta_degC").unwrap().convert_to("degC").unwrap_err())
            .contains("cannot convert a temperature interval"));
        assert!(Quantity::new(10.0, "K").unwrap().convert_to("delta_degC").is_ok());
        let difference = point.subtract(&Quantity::new(10.0, "degC").unwrap()).unwrap();
        assert!(message(difference.phs_compare(&point, "<").unwrap_err()).contains("with a temperature interval"));
    }

    #[wasm_bindgen_test]
    fn a_point_converted_to_kelvin_stays_a_point_and_a_kelvin_by_hand_stands_for_either() {
        let message = |err: JsValue| String::from(err.dyn_into::<js_sys::Error>().expect("should be a JS Error").message());
        let interval = Quantity::new(5.0, "delta_degC").unwrap();
        let point = Quantity::new(20.0, "degC").unwrap().convert_to("K").unwrap();
        assert!(message(point.phs_compare(&interval, "<").unwrap_err()).contains("cannot compare a temperature point (`293.15 K`)"));
        assert!(message(point.convert_to("delta_degC").unwrap_err()).contains("cannot convert a temperature point"));
        let average = point.add(&Quantity::new(30.0, "degC").unwrap().convert_to("K").unwrap()).unwrap();
        assert!(average.phs_compare(&interval, ">").unwrap(), "a sum of two points is of unknown kind");
        assert!(Quantity::new(300.0, "K").unwrap().phs_compare(&interval, ">").unwrap());
    }

    #[wasm_bindgen_test]
    fn phys_exact_assert_throws_when_conversion_would_be_required() {
        let a = Quantity::new(1.0, "km").unwrap();
        let b = Quantity::new(1000.0, "m").unwrap();
        let err = a.phs_exact_assert(&b).unwrap_err();
        let js_error = err.dyn_into::<js_sys::Error>().expect("should be a JS Error");
        assert!(String::from(js_error.message()).contains("exact_assert failed"));
    }
}
