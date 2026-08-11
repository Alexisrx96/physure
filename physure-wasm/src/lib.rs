mod error;
mod phy_function;
mod quantity;
mod registry;

use wasm_bindgen::prelude::*;

pub use phy_function::PhyFunction;
pub use quantity::Quantity;
pub use registry::UnitRegistry;

#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

#[cfg(test)]
mod tests {
    use wasm_bindgen_test::*;

    // Node.js is wasm-bindgen-test's default test runner (the crate no longer
    // exposes a `run_in_node` configure option — only `run_in_browser` and the
    // worker variants opt *out* of the default), so no configure call is
    // needed here; `wasm-pack test --node` runs these as-is.

    #[wasm_bindgen_test]
    fn wasm_pack_test_harness_runs() {
        assert_eq!(2 + 2, 4);
    }
}
