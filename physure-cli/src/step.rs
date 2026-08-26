use physure_script::value::PhsValue;

#[derive(Debug, Clone)]
pub struct ExecutionStep {
    pub label: String,
    pub expr_code: String,
    pub latex_expr: String,
    pub value: PhsValue,
    pub is_display_text: bool,
    /// From `@precision(N)` on this statement, if it was a variable assignment carrying one.
    /// `None` means "use the default GUM rounding" everywhere this is consulted.
    pub precision_override: Option<u32>,
}
