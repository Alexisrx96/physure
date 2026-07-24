use crate::config::I18nLabels;

/// Escapes characters that are special to LaTeX text mode (`\text{...}`).
pub fn escape_latex_text(s: &str) -> String {
    s.replace('\\', "\\backslash ")
        .replace('^', "\\textasciicircum{}")
        .replace('~', "\\textasciitilde{}")
        .replace('_', "\\_")
        .replace('%', "\\%")
        .replace('$', "\\$")
        .replace('#', "\\#")
        .replace('&', "\\&")
}

pub fn format_symbol_latex(name: &str) -> String {
    format!("\\text{{{}}}", escape_latex_text(name))
}

pub fn format_expr_latex_summary(expr: &physure_script::ast::Expr, i18n: &I18nLabels) -> String {
    match expr {
        physure_script::ast::Expr::Quantity(q) => {
            let mut s = physure_core::quantity::format_float(q.magnitude);
            if let Some(ref u) = q.unit {
                let u_s = physure_script::ast::unit_to_latex(u);
                if !u_s.is_empty() {
                    s = format!("{}\\, {}", s, u_s);
                }
            }
            s
        }
        physure_script::ast::Expr::Identifier(s) => {
            let clean = s.trim_matches('"');
            format!("\\text{{{}}}", escape_latex_text(clean))
        }
        physure_script::ast::Expr::BinaryOp { op, left, right } => {
            let l = format_expr_latex_summary(left, i18n);
            let r = format_expr_latex_summary(right, i18n);
            match op {
                physure_script::ast::BinaryOp::Add => format!("{} + {}", l, r),
                physure_script::ast::BinaryOp::Sub => format!("{} - {}", l, r),
                physure_script::ast::BinaryOp::Mul => format!("{} \\cdot {}", l, r),
                physure_script::ast::BinaryOp::Div => format!("\\frac{{{}}}{{{}}}", l, r),
                physure_script::ast::BinaryOp::Pow => format!("{{{}}}^{{{}}}", l, r),
                physure_script::ast::BinaryOp::Convert => format!("{} \\Rightarrow {}", l, r),
            }
        }
        physure_script::ast::Expr::FunctionCall { name, args } => {
            let args_s: Vec<String> = args.iter().map(|a| format_expr_latex_summary(a, i18n)).collect();
            if name == "solve" && args.len() == 2 {
                let eq_str = args_s[0].trim_matches('"').replace('_', "\\_");
                let var_str = args_s[1].trim_matches('"').replace('_', "\\_");
                format!("\\text{{{} }} {} \\text{{ {} }} {}:", i18n.solve_from, eq_str, i18n.solve_solving_for, var_str)
            } else if (name == "ternary" || name == "if_then_else") && args.len() == 3 {
                format!("\\text{{{} }} {} \\text{{ {} }} {} \\text{{ {} }} {}", i18n.if_word, args_s[0], i18n.then_word, args_s[1], i18n.else_word, args_s[2])
            } else {
                format!("\\text{{{}}}({})", name, args_s.join(", "))
            }
        }
    }
}

/// Extracts the raw text of a string-literal argument (e.g. `deriv("t^2", "t")`),
/// falling back to LaTeX rendering for non-literal expressions.
pub fn raw_identifier_text(expr: &physure_script::ast::Expr, i18n: &I18nLabels) -> String {
    match expr {
        physure_script::ast::Expr::Identifier(s) => s.trim_matches('"').to_string(),
        _ => format_expr_latex_summary(expr, i18n),
    }
}

/// Parses a raw algebraic expression string (e.g. from a `deriv`/`integral` argument
/// or result) into an `Expr`, by wrapping it as a throwaway assignment.
fn parse_raw_expr(raw: &str) -> Option<physure_script::ast::Expr> {
    let wrapped = format!("__phs_latex_tmp = {}", raw);
    let program = physure_script::parse_phs(&wrapped).ok()?;
    match program.statements.into_iter().next()? {
        physure_script::ast::Statement::Assignment(node) => Some(node.value),
        _ => None,
    }
}

/// Renders a raw math expression string as proper LaTeX math (e.g. `t^2` -> `{t}^{2}`),
/// falling back to escaped plain text if it doesn't parse as an expression.
pub fn render_raw_math(raw: &str, i18n: &I18nLabels) -> String {
    match parse_raw_expr(raw) {
        Some(expr) => format_expr_latex_summary(&expr, i18n),
        None => format!("\\text{{{}}}", escape_latex_text(raw)),
    }
}
