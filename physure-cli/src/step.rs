use physure_script::value::PhsValue;

#[derive(Debug, Clone)]
pub struct ExecutionStep {
    pub label: String,
    pub expr_code: String,
    pub latex_expr: String,
    pub value: PhsValue,
    pub is_display_text: bool,
}
