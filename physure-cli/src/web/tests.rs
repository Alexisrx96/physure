use super::*;
use crate::latex::format_comparison_latex_expr;
use physure_core::quantity::Quantity;
use physure_core::units::RationalUnit;
use physure_script::ast::Statement;

fn i18n() -> I18nLabels {
    PhysureConfig::load().i18n()
}

fn step(label: &str, latex: &str, value: PhsValue) -> ExecutionStep {
    ExecutionStep {
        label: label.to_string(),
        expr_code: label.to_string(),
        latex_expr: latex.to_string(),
        value,
        is_display_text: false,
        precision_override: None,
    }
}

fn prose(text: &str) -> ExecutionStep {
    ExecutionStep { is_display_text: true, ..step("doc", "", PhsValue::String(text.to_string())) }
}

fn plot(title: &str, svg: &str) -> ExecutionStep {
    step(
        "p",
        "",
        PhsValue::Plot(PlotData {
            title: title.to_string(),
            x_unit: "s".to_string(),
            y_unit: "m".to_string(),
            ascii: String::new(),
            svg: svg.to_string(),
        }),
    )
}

fn function(source: &str) -> PhsValue {
    let program = physure_script::parse_phs(source).unwrap();
    match program.statements.into_iter().next() {
        Some(Statement::FunctionDef(node)) => PhsValue::Function(node),
        other => panic!("{source:?} is not a function: {other:?}"),
    }
}

#[test]
fn format_val_latex_applies_gum_rounding_to_an_uncertain_quantity() {
    let i18n = i18n();
    let q = Quantity::new_scalar(625.0, 40.0195264839553, RationalUnit::base("J"), None, None);
    let latex = format_val_latex(&PhsValue::Quantity(q), &i18n, None);
    assert!(latex.contains("630"), "expected the GUM-rounded mean, got: {latex}");
    assert!(latex.contains("40"), "expected the GUM-rounded uncertainty, got: {latex}");
    assert!(!latex.contains("40.0195264839553"), "expected no un-rounded uncertainty, got: {latex}");
}

#[test]
fn format_val_latex_respects_a_precision_override() {
    let i18n = i18n();
    let q = Quantity::new_scalar(std::f64::consts::PI, 0.0, RationalUnit::dimensionless(), None, None);
    let latex = format_val_latex(&PhsValue::Quantity(q), &i18n, Some(2));
    assert!(latex.contains("3.14"), "expected 2 decimal places, got: {latex}");
    assert!(!latex.contains("3.14159"), "expected the override to actually apply, got: {latex}");
}

#[test]
fn format_val_latex_prints_a_number_a_boolean_a_function_and_the_rest() {
    let i18n = i18n();
    let latex = |value: PhsValue| format_val_latex(&value, &i18n, None);
    assert_eq!(latex(PhsValue::Number(1.5)), "= 1.5");
    assert_eq!(latex(PhsValue::Bool(true)), "= \\text{True}");
    assert_eq!(latex(PhsValue::Bool(false)), "= \\text{False}");

    // a function is printed as its last expression, or by its name when it ends in no expression
    let last_expression = latex(function("fn double(x) = x * 2\n"));
    assert!(last_expression.starts_with("= ") && last_expression.contains('2'), "{last_expression}");
    let mut empty = match function("fn double(x) = x * 2\n") {
        PhsValue::Function(node) => node,
        _ => unreachable!(),
    };
    empty.body_stmts.clear();
    assert_eq!(latex(PhsValue::Function(empty)), "= \\text{double}");

    // anything else is printed as the manuscript prints it
    assert_eq!(latex(PhsValue::String("words".to_string())), printed_value_latex(&PhsValue::String("words".to_string()), &i18n));
}

#[test]
fn the_steps_become_prose_figures_and_numbered_equations_in_order() {
    let i18n = i18n();
    let steps = vec![
        prose("first <b>line</b>\nsecond line"),
        step("n", "n = 3", PhsValue::Number(3.0)),
        plot("Speed & time", "<svg id=\"one\"></svg>"),
        step("m", "m = 4", PhsValue::Number(4.0)),
        plot("Second", "<svg id=\"two\"></svg>"),
        // a display text that is not a string leaves no trace
        ExecutionStep { is_display_text: true, ..step("odd", "", PhsValue::Number(9.0)) },
    ];

    let html = steps_html(&steps, &i18n);

    assert!(html.contains("first &lt;b&gt;line&lt;/b&gt;<br/>second line"), "the prose is escaped, with its line breaks kept: {html}");
    assert!(html.contains("\\[ n = 3 \\quad = 3.0 \\]") || html.contains("\\[ n = 3 \\quad = 3 \\]"), "{html}");
    assert!(html.contains("<div class=\"latex-eq-num\">(1)</div>") && html.contains("<div class=\"latex-eq-num\">(2)</div>"));
    assert!(!html.contains("(3)</div>"), "figures and prose are not numbered as equations: {html}");
    assert!(html.contains(&format!("<strong>{} 1.</strong> Speed &amp; time.", i18n.fig_prefix)), "{html}");
    assert!(html.contains(&format!("<strong>{} 2.</strong> Second.", i18n.fig_prefix)), "{html}");
    assert!(html.contains("<svg id=\"one\"></svg>") && html.contains("<svg id=\"two\"></svg>"), "the figures are inlined: {html}");
    let positions: Vec<usize> = ["first &lt;b&gt;", "n = 3", "id=\"one\"", "m = 4", "id=\"two\""].iter().map(|part| html.find(part).unwrap()).collect();
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]), "the steps keep their order: {positions:?}");
    assert!(!html.contains("odd") && !html.contains("9.0"), "{html}");
}

#[test]
fn a_comparison_is_shown_with_the_verdict_of_its_result() {
    let i18n = i18n();
    let comparison = format_comparison_latex_expr("2", "3", "<", "\\geq");
    let holds = step("c", &comparison, PhsValue::Bool(true));
    let fails = step("c", &comparison, PhsValue::Bool(false));
    let measured = step("c", &comparison, PhsValue::Quantity(Quantity::new_scalar(1.0, 0.0, RationalUnit::dimensionless(), None, None)));

    let holds_html = steps_html(&[holds], &i18n);
    assert!(holds_html.contains("2 < 3") && holds_html.contains(i18n.true_word), "{holds_html}");
    let fails_html = steps_html(&[fails], &i18n);
    assert!(fails_html.contains("2 \\geq 3") && fails_html.contains(i18n.false_word), "{fails_html}");
    // a quantity above one half counts as true
    assert!(steps_html(&[measured], &i18n).contains("2 < 3"));
}

#[test]
fn the_paper_has_the_metadata_of_the_script_the_steps_and_the_source() {
    let code = "# @title: Beam <study>\n# @author: Ada\n# @institution: Analytical Engines\n# @date: 1843\n# @abstract: Loads & limits.\nx = 1 m\n";
    let page = live_paper("fallback", code, &[prose("A paragraph."), step("x", "x = 1", PhsValue::Number(1.0))]);

    assert!(page.starts_with("<!DOCTYPE html>"));
    assert!(page.contains("<title>Beam &lt;study&gt; &mdash; "), "{}", &page[..200.min(page.len())]);
    assert!(page.contains("<h1 class=\"paper-title\">Beam &lt;study&gt;</h1>"));
    assert!(page.contains("Analytical Engines") && page.contains("Ada &bull; 1843 &bull; "));
    assert!(page.contains("<p>Loads &amp; limits.</p>"), "the abstract");
    assert!(page.contains("<p>A paragraph.</p>") && page.contains("x = 1"));
    assert!(page.contains("# @abstract: Loads &amp; limits.\nx = 1 m"), "the source goes in the appendix, escaped");
    assert!(page.contains("renderMathInElement") && page.contains("katex"), "the page carries its own math renderer");
}

#[test]
fn a_script_without_metadata_gets_the_title_it_is_given_and_a_default_header() {
    let page = live_paper("My <script>", "x = 1\n", &[]);
    assert!(page.contains("<h1 class=\"paper-title\">My &lt;script&gt;</h1>"));
    assert!(page.contains("Physure Scientific Live Manuscript") && page.contains("Physure Engine"));
    assert!(!page.contains("latex-abstract\">"), "no abstract is shown when the script has none");
    let year = chrono::Local::now().format("%Y").to_string();
    assert!(page.contains(&year), "the date is today's when the script has none");
}

#[test]
fn the_page_is_served_to_every_request_as_html() {
    let server = Server::http("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}/anything", server.server_addr().to_ip().unwrap().port());
    std::thread::spawn(move || serve_page(&server, "<html>the paper</html>"));

    for _ in 0..2 {
        let response = ureq::get(&url).call().unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.header("Content-Type"), Some("text/html; charset=utf-8"));
        assert_eq!(response.into_string().unwrap(), "<html>the paper</html>");
    }
}
