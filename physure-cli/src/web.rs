use std::collections::HashMap;
use tiny_http::{Response, Server};
use physure_script::value::{PhsValue, PlotData};
use physure_script::ast::unit_to_latex;
use crate::step::ExecutionStep;
use crate::katex_assets::{KATEX_CSS, KATEX_JS, AUTO_RENDER_JS};
use crate::config::{I18nLabels, PhysureConfig};
use crate::latex::{render_raw_math, format_expr_latex_summary, escape_latex_text, resolve_comparison_latex};

struct ScriptMetadata {
    title: Option<String>,
    author: Option<String>,
    institution: Option<String>,
    date: Option<String>,
    abstract_text: Option<String>,
}

fn extract_metadata(code: &str) -> ScriptMetadata {
    let mut meta = ScriptMetadata {
        title: None,
        author: None,
        institution: None,
        date: None,
        abstract_text: None,
    };
    for line in code.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("# @title:") {
            meta.title = Some(trimmed.trim_start_matches("# @title:").trim().to_string());
        } else if trimmed.starts_with("# @author:") {
            meta.author = Some(trimmed.trim_start_matches("# @author:").trim().to_string());
        } else if trimmed.starts_with("# @institution:") {
            meta.institution = Some(trimmed.trim_start_matches("# @institution:").trim().to_string());
        } else if trimmed.starts_with("# @date:") {
            meta.date = Some(trimmed.trim_start_matches("# @date:").trim().to_string());
        } else if trimmed.starts_with("# @abstract:") {
            meta.abstract_text = Some(trimmed.trim_start_matches("# @abstract:").trim().to_string());
        }
    }
    meta
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// `format_float`/`gum_round` both hand back plain-decimal or `1.23e4`-style strings; this
/// turns the latter into `1.23 \times 10^{4}` for LaTeX.
fn sci_notation_to_latex(mut s: String) -> String {
    if s.contains('e') || s.contains('E') {
        let parts: Vec<&str> = s.split(['e', 'E']).collect();
        if parts.len() == 2 {
            s = format!("{} \\times 10^{{{}}}", parts[0], parts[1].trim_start_matches('+'));
        }
    }
    s
}

fn format_val_latex(val: &PhsValue, i18n: &I18nLabels, precision_override: Option<u32>) -> String {
    match val {
        PhsValue::Quantity(q) => {
            let std_dev = q.value.std_dev();
            let val_s = if std_dev > 0.0 {
                // GUM rounding correlates the two, so both come from one call rather than
                // each being formatted to its own independent precision.
                let (mean_s, unc_s) = physure_core::quantity::gum_round(q.value.mean(), std_dev, precision_override);
                format!("({} \\pm {})", sci_notation_to_latex(mean_s), sci_notation_to_latex(unc_s))
            } else {
                match precision_override {
                    Some(n) => sci_notation_to_latex(format!("{:.*}", n as usize, q.value.mean())),
                    None => sci_notation_to_latex(physure_core::quantity::format_float(q.value.mean())),
                }
            };
            let u_s = unit_to_latex(&q.unit.__repr__());
            if u_s.is_empty() {
                format!("= {}", val_s)
            } else {
                format!("= {}\\; {}", val_s, u_s)
            }
        }
        PhsValue::Number(n) => {
            let mut s = physure_core::quantity::format_float(*n);
            if s.contains('e') || s.contains('E') {
                let parts: Vec<&str> = s.split(['e', 'E']).collect();
                if parts.len() == 2 {
                    s = format!("{} \\times 10^{{{}}}", parts[0], parts[1].trim_start_matches('+'));
                }
            }
            format!("= {}", s)
        }
        PhsValue::Bool(b) => format!("= \\text{{{}}}", if *b { "True" } else { "False" }),
        PhsValue::Function(func) => {
            match func.body_stmts.last() {
                Some(physure_script::ast::Statement::Expr(e)) => format!("= {}", format_expr_latex_summary(e, i18n)),
                _ => format!("= \\text{{{}}}", escape_latex_text(&func.name)),
            }
        }
        _ => {
            let raw = val.to_string();
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return String::new();
            }
            if trimmed == "True" || trimmed == "False" {
                return format!("= \\text{{{}}}", trimmed);
            }

            let mut parts = trimmed.splitn(2, ' ');
            let first = parts.next().unwrap_or("");
            let rest = parts.next().unwrap_or("").trim();

            if let Ok(num) = first.parse::<f64>() {
                let mut val_s = physure_core::quantity::format_float(num);
                if val_s.contains('e') || val_s.contains('E') {
                    let p: Vec<&str> = val_s.split(['e', 'E']).collect();
                    if p.len() == 2 {
                        val_s = format!("{} \\times 10^{{{}}}", p[0], p[1].trim_start_matches('+'));
                    }
                }
                if !rest.is_empty() {
                    let u_s = unit_to_latex(rest);
                    format!("= {}\\; {}", val_s, u_s)
                } else {
                    format!("= {}", val_s)
                }
            } else {
                format!("= {}", render_raw_math(trimmed, i18n))
            }
        }
    }
}

pub fn start_web_server(title: &str, code: &str, steps: &[ExecutionStep], _vars: &HashMap<String, PhsValue>) -> Result<(), Box<dyn std::error::Error>> {
    let cfg = PhysureConfig::load();
    let i18n = cfg.i18n();

    let server = Server::http("127.0.0.1:3000").map_err(|e| format!("{}", e))?;
    println!("\x1b[1;32m🚀 Physure Scientific Live Paper (100% offline) running at http://localhost:3000\x1b[0m");
    println!("\x1b[90mPress Ctrl+C to stop the server\x1b[0m");
    let _ = open::that("http://localhost:3000");

    let meta = extract_metadata(code);
    let paper_title = meta.title.clone().unwrap_or_else(|| title.to_string());
    let paper_inst = meta.institution.clone().unwrap_or_else(|| "Physure Scientific Live Manuscript".to_string());
    let paper_author = meta.author.clone().unwrap_or_else(|| "Physure Engine".to_string());
    let paper_date = meta.date.clone().unwrap_or_else(|| {
        chrono::Local::now().format("%B %d, %Y").to_string()
    });

    let mut abstract_html = String::new();
    if let Some(ref abs_text) = meta.abstract_text {
        abstract_html = format!(
            r#"<div class="latex-abstract">
                <div class="abstract-title">{}</div>
                <p>{}</p>
            </div>"#,
            i18n.abstract_title,
            escape_html(abs_text)
        );
    }

    let mut content_html = String::new();
    let mut eq_counter = 1;
    let mut fig_counter = 1;

    for step in steps.iter() {
        if step.is_display_text {
            if let PhsValue::String(ref text) = step.value {
                content_html.push_str(&format!(
                    r#"<div class="latex-prose">
                        <p>{}</p>
                    </div>"#,
                    escape_html(text).replace("\n", "<br/>")
                ));
            }
            continue;
        }

        match &step.value {
            PhsValue::Plot(PlotData { title: p_title, svg, .. }) => {
                content_html.push_str(&format!(
                    r#"<figure class="latex-figure">
                        <div class="fig-frame">
                            {}
                        </div>
                        <figcaption class="fig-caption">
                            <strong>{} {}.</strong> {}.
                        </figcaption>
                    </figure>"#,
                    svg, i18n.fig_prefix, fig_counter, escape_html(p_title)
                ));
                fig_counter += 1;
            }
            _ => {
                let is_true = matches!(&step.value, PhsValue::Quantity(q) if q.value.mean() > 0.5)
                    || matches!(&step.value, PhsValue::Bool(true));
                let eq_num = eq_counter;
                eq_counter += 1;

                if let Some(cmp) = resolve_comparison_latex(&step.latex_expr, is_true, &i18n) {
                    content_html.push_str(&format!(
                        r#"<div class="latex-eq-container">
                            <div class="latex-eq-main">\[ {} \]</div>
                            <div class="latex-eq-num">({})</div>
                        </div>"#,
                        cmp, eq_num
                    ));
                } else {
                    let eval_latex = format_val_latex(&step.value, &i18n, step.precision_override);
                    content_html.push_str(&format!(
                        r#"<div class="latex-eq-container">
                            <div class="latex-eq-main">\[ {} \quad {} \]</div>
                            <div class="latex-eq-num">({})</div>
                        </div>"#,
                        step.latex_expr, eval_latex, eq_num
                    ));
                }
            }
        }
    }

    let html_content = format!(r#"<!DOCTYPE html>
<html lang="{}">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{} &mdash; Manuscrito Científico Physure</title>
    <style>
        {}

        @page {{
            size: A4;
            margin: 25mm 20mm;
        }}

        body {{
            font-family: 'Crimson Pro', Georgia, 'Times New Roman', 'Liberation Serif', serif;
            font-size: 11.5pt;
            color: #111111;
            background-color: #ffffff;
            line-height: 1.65;
            margin: 0;
            padding: 40px 20px;
        }}

        .paper-manuscript {{
            max-width: 820px;
            margin: 0 auto;
            background: #ffffff;
            padding: 0;
        }}

        .paper-header {{
            text-align: center;
            border-top: 1.5pt solid #000000;
            border-bottom: 1.5pt solid #000000;
            padding: 22px 0 18px 0;
            margin-bottom: 36px;
        }}

        .paper-institution {{
            font-family: system-ui, -apple-system, sans-serif;
            font-size: 0.78rem;
            text-transform: uppercase;
            letter-spacing: 1.6px;
            color: #444444;
            margin-bottom: 10px;
        }}

        .paper-title {{
            font-size: 2.2rem;
            font-weight: 700;
            margin: 0 0 12px 0;
            line-height: 1.25;
            color: #000000;
        }}

        .paper-author-meta {{
            font-style: italic;
            font-size: 0.98rem;
            color: #333333;
        }}

        .latex-abstract {{
            width: 88%;
            margin: 0 auto 38px auto;
            font-size: 0.98rem;
            font-style: italic;
            line-height: 1.65;
            text-align: justify;
            border-left: 2.5pt solid #000000;
            padding-left: 18px;
        }}

        .abstract-title {{
            font-family: system-ui, sans-serif;
            font-size: 0.8rem;
            font-weight: bold;
            text-transform: uppercase;
            letter-spacing: 1.3px;
            margin-bottom: 6px;
            font-style: normal;
            color: #000000;
        }}

        h2.paper-sec-title {{
            font-size: 1.3rem;
            font-weight: 700;
            border-bottom: 0.75pt solid #000000;
            padding-bottom: 4px;
            margin-top: 42px;
            margin-bottom: 18px;
            color: #000000;
        }}

        .latex-eq-container {{
            display: flex;
            justify-content: space-between;
            align-items: center;
            margin: 18px 0;
            padding: 4px 0;
        }}

        .latex-eq-main {{
            flex-grow: 1;
            text-align: center;
            overflow-x: auto;
            font-family: 'Crimson Pro', Georgia, serif;
            font-size: 1.15rem;
        }}

        .latex-eq-num {{
            font-family: 'Crimson Pro', serif;
            font-size: 1.05rem;
            color: #222222;
            padding-left: 16px;
            user-select: none;
        }}

        .latex-prose {{
            font-size: 1.08rem;
            line-height: 1.72;
            margin: 22px 0;
            text-align: justify;
        }}

        .latex-figure {{
            margin: 32px 0;
            text-align: center;
        }}

        .fig-frame {{
            border: 0.75pt solid #cccccc;
            padding: 14px;
            background: #ffffff;
            display: inline-block;
            max-width: 100%;
            border-radius: 2px;
        }}

        .fig-caption {{
            font-size: 0.9rem;
            color: #333333;
            margin-top: 10px;
            font-style: italic;
        }}

        .source-code-box {{
            background: #f8f9fa;
            border: 1px solid #e9ecef;
            border-radius: 3px;
            padding: 18px;
            font-family: 'Fira Code', 'Cascadia Code', Consolas, monospace;
            font-size: 0.86rem;
            white-space: pre-wrap;
            overflow-x: auto;
            color: #212529;
            margin-bottom: 40px;
        }}

        @media print {{
            body {{
                padding: 0;
            }}
        }}
    </style>
    <script>
        {}
    </script>
    <script>
        {}
    </script>
    <script>
        document.addEventListener("DOMContentLoaded", function() {{
            if (typeof renderMathInElement === 'function') {{
                renderMathInElement(document.body, {{
                    delimiters: [
                        {{left: '\\[', right: '\\]', display: true}},
                        {{left: '\\(', right: '\\)', display: false}}
                    ],
                    throwOnError: false
                }});
            }}
        }});
    </script>
</head>
<body>
    <article class="paper-manuscript">
        <header class="paper-header">
            <div class="paper-institution">{}</div>
            <h1 class="paper-title">{}</h1>
            <div class="paper-author-meta">{} &bull; {} &bull; {}</div>
        </header>

        {}

        <h2 class="paper-sec-title">{}</h2>
        {}

        <h2 class="paper-sec-title">{}</h2>
        <pre class="source-code-box">{}</pre>
    </article>
</body>
</html>
    "#,
        i18n.html_lang,
        escape_html(&paper_title),
        KATEX_CSS,
        KATEX_JS,
        AUTO_RENDER_JS,
        escape_html(&paper_inst),
        escape_html(&paper_title),
        escape_html(&paper_author),
        escape_html(&paper_date),
        i18n.footer_engine,
        abstract_html,
        i18n.sec_evaluations,
        content_html,
        i18n.sec_appendix,
        escape_html(code)
    );

    for request in server.incoming_requests() {
        let response = Response::from_string(&html_content)
            .with_header(tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap());
        let _ = request.respond(response);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use physure_core::quantity::Quantity;
    use physure_core::units::RationalUnit;

    #[test]
    fn format_val_latex_applies_gum_rounding_to_an_uncertain_quantity() {
        let i18n = PhysureConfig::load().i18n();
        let q = Quantity::new_scalar(625.0, 40.0195264839553, RationalUnit::base("J"), None, None);
        let latex = format_val_latex(&PhsValue::Quantity(q), &i18n, None);
        assert!(latex.contains("630"), "expected the GUM-rounded mean, got: {latex}");
        assert!(latex.contains("40"), "expected the GUM-rounded uncertainty, got: {latex}");
        assert!(!latex.contains("40.0195264839553"), "expected no un-rounded uncertainty, got: {latex}");
    }

    #[test]
    fn format_val_latex_respects_a_precision_override() {
        let i18n = PhysureConfig::load().i18n();
        let q = Quantity::new_scalar(3.14159265, 0.0, RationalUnit::dimensionless(), None, None);
        let latex = format_val_latex(&PhsValue::Quantity(q), &i18n, Some(2));
        assert!(latex.contains("3.14"), "expected 2 decimal places, got: {latex}");
        assert!(!latex.contains("3.14159265"), "expected the override to actually apply, got: {latex}");
    }
}
