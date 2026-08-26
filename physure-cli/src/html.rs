use std::collections::HashMap;
use std::env;
use std::fs;
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

fn base64_encode(input: &str) -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    let mut i = 0;
    while i < bytes.len() {
        let b0 = bytes[i] as u32;
        let b1 = if i + 1 < bytes.len() { bytes[i + 1] as u32 } else { 0 };
        let b2 = if i + 2 < bytes.len() { bytes[i + 2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(CHARSET[((triple >> 18) & 63) as usize] as char);
        out.push(CHARSET[((triple >> 12) & 63) as usize] as char);
        if i + 1 < bytes.len() {
            out.push(CHARSET[((triple >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if i + 2 < bytes.len() {
            out.push(CHARSET[(triple & 63) as usize] as char);
        } else {
            out.push('=');
        }
        i += 3;
    }
    out
}

fn format_val_latex(val: &PhsValue, i18n: &I18nLabels) -> String {
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
        PhsValue::Vector(v) => {
            let items: Vec<String> = v.iter().map(|item| {
                let s = format_val_latex(item, i18n);
                s.trim_start_matches("= ").to_string()
            }).collect();
            if items.len() > 4 {
                format!("= [{}, \\dots, {}]", items[..3].join(", "), items.last().unwrap_or(&String::new()))
            } else {
                format!("= [{}]", items.join(", "))
            }
        },
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

pub fn open_standalone_html(title: &str, output_path: &std::path::Path, code: &str, steps: &[ExecutionStep], _vars: &HashMap<String, PhsValue>) -> Result<(), Box<dyn std::error::Error>> {
    let cfg = PhysureConfig::load();
    let i18n = cfg.i18n();

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
                let trimmed_svg = svg.trim_start();
                if trimmed_svg.starts_with("<!DOCTYPE") || trimmed_svg.starts_with("<html") {
                    let b64 = base64_encode(svg);
                    content_html.push_str(&format!(
                        r#"<figure class="latex-figure native-3d-figure">
                            <div class="fig-frame-3d">
                                <iframe src="data:text/html;charset=utf-8;base64,{}" style="width: 100%; height: 100%; border: none;" sandbox="allow-scripts allow-same-origin"></iframe>
                            </div>
                            <figcaption class="fig-caption">
                                <strong>{} {}.</strong> {}.
                            </figcaption>
                        </figure>"#,
                        b64, i18n.fig_prefix, fig_counter, escape_html(p_title)
                    ));
                } else {
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
                }
                fig_counter += 1;
            }
            _ => {
                let is_true = matches!(&step.value, PhsValue::Quantity(q) if q.value.mean() > 0.5)
                    || matches!(&step.value, PhsValue::Bool(true));

                let math_body = if let Some(cmp) = resolve_comparison_latex(&step.latex_expr, is_true, &i18n) {
                    cmp
                } else {
                    let mut eval_latex = format_val_latex(&step.value, &i18n);
                    if !step.latex_expr.is_empty() && eval_latex.starts_with("= ") {
                        let trimmed_expr = step.latex_expr.trim_end();
                        if trimmed_expr.ends_with('=') || trimmed_expr.ends_with("\\Rightarrow") {
                            eval_latex = eval_latex.trim_start_matches("= ").to_string();
                        }
                    }
                    if step.latex_expr.is_empty() {
                        eval_latex
                    } else {
                        format!("{} {}", step.latex_expr, eval_latex)
                    }
                };
                let eq_num = eq_counter;
                eq_counter += 1;

                content_html.push_str(&format!(
                    r#"<div class="latex-eq-container">
                        <div class="latex-eq-main">\[ {} \]</div>
                        <div class="latex-eq-num">({})</div>
                    </div>"#,
                    math_body, eq_num
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
            font-size: 12pt;
            color: #0f172a;
            background-color: #f8fafc;
            line-height: 1.7;
            margin: 0;
            padding: 32px 20px;
        }}

        .paper-manuscript {{
            max-width: 1100px;
            margin: 0 auto;
            background: #ffffff;
            padding: 48px 56px;
            border-radius: 12px;
            border: 1px solid #e2e8f0;
            box-shadow: 0 10px 40px -10px rgba(15, 23, 42, 0.08);
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
            word-break: break-all;
            overflow-wrap: break-word;
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

        .fig-frame-3d {{
            width: 100%;
            height: 600px;
            padding: 0;
            display: block;
            border: 1px solid #cbd5e1;
            background: #0f172a;
            border-radius: 10px;
            overflow: hidden;
            box-shadow: 0 8px 30px rgba(0,0,0,0.18);
        }}

        @media (max-width: 768px) {{
            body {{ padding: 12px 8px; }}
            .paper-manuscript {{ padding: 24px 18px; border-radius: 8px; }}
            .fig-frame-3d {{ height: 420px; }}
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
            body {{ padding: 0; background: #ffffff; }}
            .paper-manuscript {{ max-width: 100%; padding: 0; border: none; box-shadow: none; }}
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

    fs::write(output_path, html_content)?;
    println!("\x1b[1;32m📄 Manuscrito científico HTML generado (100% offline):\x1b[0m {}", output_path.display());
    // Set for CLI integration tests, which must not pop an actual browser window.
    if env::var_os("PHS_NO_OPEN").is_none() {
        open::that(output_path)?;
    }
    Ok(())
}
