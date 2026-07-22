use std::collections::HashMap;
use std::env;
use std::fs;
use physure_script::value::{PhsValue, PlotData};
use physure_script::ast::unit_to_latex;
use crate::step::ExecutionStep;
use crate::katex_assets::{KATEX_CSS, KATEX_JS, AUTO_RENDER_JS};
use crate::config::PhysureConfig;

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

fn format_val_latex(val: &PhsValue) -> String {
    match val {
        PhsValue::Quantity(q) => {
            let mut val_s = physure_core::quantity::format_float(q.value.mean());
            if val_s.contains('e') || val_s.contains('E') {
                let parts: Vec<&str> = val_s.split(['e', 'E']).collect();
                if parts.len() == 2 {
                    val_s = format!("{} \\times 10^{{{}}}", parts[0], parts[1].trim_start_matches('+'));
                }
            }
            let std_dev = q.value.std_dev();
            if std_dev > 0.0 {
                let mut unc_s = physure_core::quantity::format_float(std_dev);
                if unc_s.contains('e') || unc_s.contains('E') {
                    let parts: Vec<&str> = unc_s.split(['e', 'E']).collect();
                    if parts.len() == 2 {
                        unc_s = format!("{} \\times 10^{{{}}}", parts[0], parts[1].trim_start_matches('+'));
                    }
                }
                val_s = format!("({} \\pm {})", val_s, unc_s);
            }
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
                let escaped = trimmed
                    .replace('\\', "\\backslash ")
                    .replace('_', "\\_")
                    .replace('&', "\\&");
                format!("= \\text{{{}}}", escaped)
            }
        }
    }
}

pub fn open_standalone_html(title: &str, code: &str, steps: &[ExecutionStep], _vars: &HashMap<String, PhsValue>) -> Result<(), Box<dyn std::error::Error>> {
    let cfg = PhysureConfig::load();
    let i18n = cfg.i18n();

    let mut temp_dir = env::temp_dir();
    let file_name = format!("physure_{}.html", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs());
    temp_dir.push(file_name);

    let meta = extract_metadata(code);
    let paper_title = meta.title.clone().unwrap_or_else(|| title.to_string());
    let paper_inst = meta.institution.clone().unwrap_or_else(|| "Physure Technical & Academic Computation Manuscript".to_string());
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
                let eval_latex = format_val_latex(&step.value);
                let eq_num = eq_counter;
                eq_counter += 1;

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

    fs::write(&temp_dir, html_content)?;
    println!("\x1b[1;32m📄 Manuscrito científico HTML generado (100% offline):\x1b[0m {}", temp_dir.display());
    open::that(&temp_dir)?;
    Ok(())
}
