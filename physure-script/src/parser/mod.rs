pub(crate) mod shadowing;
pub(crate) mod quantities;
pub(crate) use quantities::is_known_unit_symbol;
pub(crate) mod expressions;
pub(crate) mod statements;
use pest::Parser;
use pest_derive::Parser;
use physure_core::error::{PhysureError, PhysureResult};
use crate::ast::*;

#[derive(Parser)]
#[grammar = "phs.pest"]
pub struct PhsParser;

pub fn parse_phs(code: &str) -> PhysureResult<Program> {
    let pairs = PhsParser::parse(Rule::program, code)
        .map_err(|e| PhysureError::Generic(format!("Parse error: {}", e)))?;
    
    let mut statements = Vec::new();
    let mut lines = Vec::new();
    let mut statement_pos = Vec::new();
    for pair in pairs {
        if pair.as_rule() == Rule::stmt {
            let (line, col) = pair.line_col();
            let inner = pair.into_inner().next().unwrap();
            statements.push(statements::parse_statement(inner)?);
            lines.push(line);
            statement_pos.push((line, col));
        }
    }

    shadowing::validate_unit_shadowing(&statements, &statement_pos)?;
    crate::decorators::validate_decorators(&statements)?;
    Ok(Program { statements, lines })
}

pub fn parse_phs_with_lines(code: &str) -> PhysureResult<Vec<(usize, Statement)>> {
    let pairs = PhsParser::parse(Rule::program, code)
        .map_err(|e| PhysureError::Generic(format!("Parse error: {}", e)))?;

    let mut statements = Vec::new();
    let mut statement_pos = Vec::new();
    for pair in pairs {
        if pair.as_rule() == Rule::stmt {
            let (line, col) = pair.line_col();
            let inner = pair.into_inner().next().unwrap();
            statements.push((line - 1, statements::parse_statement(inner)?));
            statement_pos.push((line, col));
        }
    }

    let stmts_only: Vec<Statement> = statements.iter().map(|(_, s)| s.clone()).collect();
    shadowing::validate_unit_shadowing(&stmts_only, &statement_pos)?;
    crate::decorators::validate_decorators(&stmts_only)?;

    Ok(statements)
}

/// Cached full unit registry (master `physure.conf` + prefixes) used to decide whether a
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_phs_records_line_numbers_for_top_level_function_and_while_bodies() {
        // PHS function bodies are indentation-delimited, not brace-delimited (only `while` uses
        // braces -- confirmed against phs.pest's `function_def = "fn" ~ ... ~ "=" ~ (block_body |
        // expr)` and `block_body = (_nl_indent ~ stmt)+`, and against a working example already in
        // the test suite: physure-script/tests/unit_shadowing.rs's `"fn f(x) =\n    t = 2.0 s\n
        // 5 m / t\n"`).
        let script = "x = 1\nfn f(a) =\n  a = a + 1\n  a\nwhile x < 3 {\n  x = x + 1\n}\n";
        let program = parse_phs(script).unwrap();

        assert_eq!(program.lines.len(), program.statements.len());
        assert_eq!(program.lines[0], 1); // x = 1

        let Statement::FunctionDef(f) = &program.statements[1] else { panic!("expected fn") };
        assert_eq!(f.body_lines.len(), f.body_stmts.len());
        assert_eq!(f.body_lines[0], 3); // a = a + 1
        assert_eq!(f.body_lines[1], 4); // a

        let Statement::While { body, body_lines, .. } = &program.statements[2] else { panic!("expected while") };
        assert_eq!(body_lines.len(), body.len());
        assert_eq!(body_lines[0], 6); // x = x + 1
    }

    /// `ternary_op` is a rule of its own, so `expr` sees it as a single child rather than
    /// as two loose `base_expr`s — reading the branches off `expr` panicked on the second.
    #[test]
    fn test_ternary_branches_come_from_the_ternary_rule() {
        for code in ["z > 2 ? 100 : 200 where z = 3", "5 m > 2 m ? 1 kg : 2 kg", "1 > 0 ? 2 m : 3 m"] {
            let prog = parse_phs(code).unwrap_or_else(|e| panic!("{code:?} failed to parse: {e:?}"));
            let expr = match &prog.statements[0] {
                Statement::Expr(e) => e,
                other => panic!("{code:?} produced {other:?}"),
            };
            // A `where` clause wraps the ternary, so look for the call anywhere in the tree.
            let rendered = format!("{expr:?}");
            assert!(rendered.contains("ternary"), "{code:?} did not build a ternary: {rendered}");
        }
    }

    #[test]
    fn test_explicit_imports() {
        let code1 = "use g, c as speed_of_light from \"physics/constants\"";
        let prog1 = parse_phs(code1).unwrap();
        assert_eq!(prog1.statements.len(), 1);
        if let Statement::Import(imp) = &prog1.statements[0] {
            assert_eq!(imp.path, "physics/constants");
            if let ImportSpecifier::Symbols(syms) = &imp.specifier {
                assert_eq!(syms[0].name, "g");
                assert_eq!(syms[1].name, "c");
                assert_eq!(syms[1].alias.as_deref(), Some("speed_of_light"));
            } else { panic!("expected symbols"); }
        } else { panic!("expected import"); }

        let code2 = "use * from \"physics/thermodynamics\"";
        let prog2 = parse_phs(code2).unwrap();
        if let Statement::Import(imp) = &prog2.statements[0] {
            assert_eq!(imp.path, "physics/thermodynamics");
            assert!(matches!(imp.specifier, ImportSpecifier::Wildcard));
        } else { panic!("expected import"); }

        let code3 = "import \"physics/constants\" as consts";
        let prog3 = parse_phs(code3).unwrap();
        if let Statement::Import(imp) = &prog3.statements[0] {
            assert_eq!(imp.path, "physics/constants");
            if let ImportSpecifier::ModuleAlias(alias) = &imp.specifier {
                assert_eq!(alias, "consts");
            } else { panic!("expected module alias"); }
        } else { panic!("expected import"); }
    }

    #[test]
    fn test_param_unit_annotation_accepts_ohm_symbol() {
        let code = "potencia2(i: A, R: \u{3a9}) = i^2 * R";
        let prog = parse_phs(&code).unwrap();
        if let Statement::FunctionDef(f) = &prog.statements[0] {
            assert_eq!(f.param_units, vec![Some("A".to_string()), Some("\u{3a9}".to_string())]);
        } else {
            panic!("expected function def");
        }
    }

    /// `1/2 m v^2` used to parse clean, but never evaluated to kinetic energy: `2` (split out
    /// of `1/2`) swallows `m` as the unit metre before the interpreter ever sees it as the
    /// mass parameter, so the body silently computed `1 / (2 m) * v^2` instead of `0.5 * m *
    /// v^2` — see `unit_shadowing.rs`'s
    /// `the_kinetic_energy_shorthand_that_motivated_this_check_is_rejected`. It is now a
    /// resolve-time ambiguity error naming `m`, not a value.
    #[test]
    fn test_natural_function_definition_shorthand_is_rejected_as_ambiguous() {
        let code = "fn kinetic_energy(m, v) = 1/2 m v^2";
        let err = parse_phs(code).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains('m'), "expected the ambiguity to name 'm': {msg}");
    }

    /// Two bare identifiers with only whitespace between them (`x y`) used to be read as
    /// `x * y` by `term`'s `_is_implicit_mul` — the same juxtaposition rule `1/2 m v^2` relies
    /// on to skip the `*` between a coefficient and a symbol. But nothing in the repo's docs,
    /// README examples, or test suite ever spells two *bare names* side by side on purpose,
    /// and it reads exactly like a forgotten operator: `total = masa velocidad` used to
    /// silently return `masa * velocidad` with a plausible-looking unit and no error at all.
    #[test]
    fn test_bare_identifier_juxtaposition_is_rejected() {
        let err = parse_phs("x y").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains('x') && msg.contains('y'), "expected both names in the error: {msg}");
    }

    /// The check has to look at the two factors actually being joined, not just the first
    /// pair in the term — an explicit `*` earlier in the chain must not exempt a later bare
    /// juxtaposition from the same rule.
    #[test]
    fn test_bare_identifier_juxtaposition_is_rejected_later_in_a_chain() {
        let err = parse_phs("x * y z").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains('y') && msg.contains('z'), "expected 'y' and 'z' in the error: {msg}");
    }

    /// Only bare-identifier-next-to-bare-identifier is banned — a quantity next to an
    /// identifier is untouched, since that is the coefficient/unit-chain pattern the language
    /// actually documents and relies on. `"m"` here is not bound by anything else in the
    /// script, so the quantity `2 m` itself is unambiguous; `x` is the separate factor
    /// `term`'s implicit multiplication still joins it to.
    #[test]
    fn test_quantity_next_to_bare_identifier_juxtaposition_still_parses() {
        let prog = parse_phs("2 m x").unwrap();
        assert!(matches!(&prog.statements[0], Statement::Expr(_)));
    }

    #[test]
    fn test_function_def_param_unit_annotation() {
        let code = "E_campo(r: m) =\n    r\n";
        let prog = parse_phs(code).unwrap();
        if let Statement::FunctionDef(f) = &prog.statements[0] {
            assert_eq!(f.name, "E_campo");
            assert_eq!(f.params, vec!["r"]);
            assert_eq!(f.param_units, vec![Some("m".to_string())]);
        } else { panic!("expected func def"); }
    }

    #[test]
    fn test_quantity_literals() {
        let code = "m = 75.0 ± 0.5 kg";
        let prog = parse_phs(code).unwrap();
        if let Statement::Assignment(a) = &prog.statements[0] {
            assert_eq!(a.name, "m");
            if let Expr::Quantity(q) = &a.value {
                assert_eq!(q.magnitude, 75.0);
                assert_eq!(q.uncertainty, Some(0.5));
                assert_eq!(q.unit.as_deref(), Some("kg"));
            } else { panic!("expected quantity"); }
        } else { panic!("expected assignment"); }

        let code = "m = 75.0 +/- 0.5 kg";
        let prog = parse_phs(code).unwrap();
        if let Statement::Assignment(a) = &prog.statements[0] {
            if let Expr::Quantity(q) = &a.value {
                assert_eq!(q.magnitude, 75.0);
                assert_eq!(q.uncertainty, Some(0.5));
                assert_eq!(q.unit.as_deref(), Some("kg"));
            }
        }

        let code = "v = 10 m/s";
        let prog = parse_phs(code).unwrap();
        if let Statement::Assignment(a) = &prog.statements[0] {
            if let Expr::Quantity(q) = &a.value {
                assert_eq!(q.magnitude, 10.0);
                assert_eq!(q.uncertainty, None);
                assert_eq!(q.unit.as_deref(), Some("m/s"));
            }
        }
    }

    /// Pulls the quantity out of `x = <quantity>`, panicking if it is anything else.
    fn parse_one_quantity(code: &str) -> QuantityNode {
        let prog = parse_phs(code).unwrap_or_else(|e| panic!("{code} did not parse: {e}"));
        match &prog.statements[0] {
            Statement::Assignment(a) => match &a.value {
                Expr::Quantity(q) => q.clone(),
                other => panic!("{code} parsed as {other:?}"),
            },
            other => panic!("{code} parsed as {other:?}"),
        }
    }

    #[test]
    fn an_asymmetric_uncertainty_keeps_both_halves() {
        // `12.3 +/- (0.5, 0.4)` reads in the order the operator does: upper first.
        for code in ["x = 12.3 +/- (0.5, 0.4) m", "x = 12.3 ± (0.5, 0.4) m"] {
            let q = parse_one_quantity(code);
            assert_eq!(q.magnitude, 12.3);
            assert_eq!(q.uncertainty, Some(0.5), "{code}");
            assert_eq!(q.uncertainty_lower, Some(0.4), "{code}");
            assert_eq!(q.unit.as_deref(), Some("m"));
        }
    }

    #[test]
    fn a_parenthesised_addend_is_not_an_uncertainty_pair() {
        // The whole risk in the notation is that `(` after a sign could be read two ways.
        // `+` alone never reaches the uncertainty rule, so this stays an addition.
        let prog = parse_phs("x = 12.3 + (0.5)").unwrap();
        let Statement::Assignment(a) = &prog.statements[0] else { panic!("expected assignment") };
        assert!(matches!(a.value, Expr::BinaryOp { op: BinaryOp::Add, .. }), "{:?}", a.value);
    }

    #[test]
    fn each_half_of_a_pair_takes_its_own_percentage() {
        let q = parse_one_quantity("x = 200.0 +/- (1%, 0.5) m");
        assert_eq!(q.uncertainty, Some(2.0));
        assert_eq!(q.uncertainty_lower, Some(0.5));
    }

    #[test]
    fn a_symmetric_uncertainty_has_no_lower_half() {
        assert_eq!(parse_one_quantity("x = 75.0 +/- 0.5 kg").uncertainty_lower, None);
    }

    #[test]
    fn test_exports() {
        let code = "export E as \"kinetic_energy\"";
        let prog = parse_phs(code).unwrap();
        if let Statement::Export(e) = &prog.statements[0] {
            assert_eq!(e.symbol, "E");
            assert_eq!(e.export_name, "kinetic_energy");
        } else { panic!("expected export"); }
    }

    #[test]
    fn test_assignment_fn_standalone() {
        let code = "f(v: m / s) =\n    resta = 1 m / s\n    v * 2 - resta";
        let pairs = PhsParser::parse(Rule::assignment_fn, code);
        assert!(pairs.is_ok());
    }

    #[test]
    fn test_decorated_stmt_rule_parses() {
        let code = "@stable\nfn f(x) = x";
        let pairs = PhsParser::parse(Rule::decorated_stmt, code);
        assert!(pairs.is_ok(), "expected decorated_stmt to parse: {:?}", pairs.err());
    }

    #[test]
    fn test_decorator_with_args_rule_parses() {
        let pairs = PhsParser::parse(Rule::decorator, "@requires(x > 0.0, \"x must be positive\")");
        assert!(pairs.is_ok(), "expected decorator with args to parse: {:?}", pairs.err());
    }

    #[test]
    fn test_decorated_stmt_rule_parses_stacked_decorators() {
        let code = "@stable\n@requires(x > 0.0, \"x must be positive\")\nfn f(x) = x";
        let pairs = PhsParser::parse(Rule::decorated_stmt, code);
        assert!(pairs.is_ok(), "expected stacked decorated_stmt to parse: {:?}", pairs.err());
    }

    #[test]
    fn test_parse_phs_attaches_decorators_to_function_def() {
        let program = parse_phs("@stable\nfn f(x) = x").unwrap();
        match &program.statements[0] {
            Statement::FunctionDef(node) => {
                assert_eq!(node.decorators.len(), 1);
                assert_eq!(node.decorators[0].name, "stable");
                assert!(node.decorators[0].args.is_empty());
            }
            other => panic!("expected FunctionDef, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_phs_attaches_decorator_args() {
        let program = parse_phs("@requires(x > 0.0, \"x must be positive\")\nfn f(x) = x").unwrap();
        match &program.statements[0] {
            Statement::FunctionDef(node) => {
                assert_eq!(node.decorators.len(), 1);
                assert_eq!(node.decorators[0].name, "requires");
                assert_eq!(node.decorators[0].args.len(), 2);
            }
            other => panic!("expected FunctionDef, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_phs_attaches_stacked_decorators_to_function_def() {
        let program = parse_phs("@stable\n@requires(x > 0.0, \"x must be positive\")\nfn f(x) = x").unwrap();
        match &program.statements[0] {
            Statement::FunctionDef(node) => {
                assert_eq!(node.decorators.len(), 2);
                assert_eq!(node.decorators[0].name, "stable");
                assert_eq!(node.decorators[1].name, "requires");
            }
            other => panic!("expected FunctionDef, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_phs_attaches_decorator_to_assignment() {
        let program = parse_phs("@stable\nx = 5").unwrap();
        match &program.statements[0] {
            Statement::Assignment(node) => {
                assert_eq!(node.decorators.len(), 1);
                assert_eq!(node.decorators[0].name, "stable");
            }
            other => panic!("expected Assignment, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_phs_rejects_unknown_decorator() {
        assert!(parse_phs("@bogus\nfn f(x) = x").is_err());
    }

    #[test]
    fn test_parse_phs_lowers_range_into_two_requires() {
        let program = parse_phs("@range(v, 0.0, 10.0)\nfn f(v) = v").unwrap();
        match &program.statements[0] {
            Statement::FunctionDef(node) => {
                assert_eq!(node.decorators.len(), 2);
                assert!(node.decorators.iter().all(|d| d.name == "requires"));
            }
            other => panic!("expected FunctionDef, got {:?}", other),
        }
    }

    #[test]
    fn doc_comment_attaches_to_function_def() {
        let program = parse_phs("/// Computes kinetic energy.\nfn ke(m, v) = 0.5 * m * v^2").unwrap();
        match &program.statements[0] {
            Statement::FunctionDef(node) => {
                assert_eq!(node.doc.as_deref(), Some("Computes kinetic energy."));
            }
            other => panic!("expected FunctionDef, got {:?}", other),
        }
    }

    #[test]
    fn multiline_doc_comment_joins_with_newline() {
        let program = parse_phs(
            "/// Line one.\n/// Line two.\nfn ke(m, v) = 0.5 * m * v^2",
        ).unwrap();
        match &program.statements[0] {
            Statement::FunctionDef(node) => {
                assert_eq!(node.doc.as_deref(), Some("Line one.\nLine two."));
            }
            other => panic!("expected FunctionDef, got {:?}", other),
        }
    }

    #[test]
    fn doc_comment_stacks_above_decorators() {
        let program = parse_phs(
            "/// Computes kinetic energy.\n@stable\nfn ke(m, v) = 0.5 * m * v^2",
        ).unwrap();
        match &program.statements[0] {
            Statement::FunctionDef(node) => {
                assert_eq!(node.doc.as_deref(), Some("Computes kinetic energy."));
                assert_eq!(node.decorators.len(), 1);
                assert_eq!(node.decorators[0].name, "stable");
            }
            other => panic!("expected FunctionDef, got {:?}", other),
        }
    }

    #[test]
    fn plain_double_slash_comment_still_parses() {
        let program = parse_phs("// just a comment\nfn ke(m, v) = 0.5 * m * v^2").unwrap();
        match &program.statements[0] {
            Statement::FunctionDef(node) => assert_eq!(node.doc, None),
            other => panic!("expected FunctionDef, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_1_cargas() {
        if let Ok(code) = std::fs::read_to_string("D:/Projects/test_physure/1_cargas.phs") {
            let res = parse_phs(&code);
            assert!(res.is_ok());
        }
    }

    #[test]
    fn test_parse_for_expr_and_while_stmt() {
        let script = "for t in 1 .. 5 {\n t * 2 \n}\nwhile x > 0 {\n x = x - 1 \n}";
        let stmts = parse_phs(script).unwrap().statements;
        assert_eq!(stmts.len(), 2);
        assert!(matches!(&stmts[0], Statement::Expr(Expr::ForExpr { .. })));
        assert!(matches!(&stmts[1], Statement::While { .. }));
    }

    #[test]
    fn test_parse_loop_newlines_before_brace() {
        let script = "for\n item\n in\n 1 .. 5\n {\n item * 2\n }\nwhile\n x > 0\n {\n x = x - 1\n }";
        let stmts = parse_phs(script).unwrap().statements;
        assert_eq!(stmts.len(), 2);
        assert!(matches!(&stmts[0], Statement::Expr(Expr::ForExpr { .. })));
        assert!(matches!(&stmts[1], Statement::While { .. }));
    }

    #[test]
    fn test_parse_while_multi_statement() {
        let script = "while x > 0 {\n a = x * 2\n x = x - 1\n }";
        let stmts = parse_phs(script).unwrap().statements;
        assert_eq!(stmts.len(), 1);
        if let Statement::While { cond: _, body, body_lines: _ } = &stmts[0] {
            assert_eq!(body.len(), 2);
        } else {
            panic!("expected While statement");
        }
    }

    #[test]
    fn test_parse_nested_loops() {
        let script = "while x > 0 {\n y = for i in 1 .. 3 {\n i * x\n }\n x = x - 1\n }";
        let stmts = parse_phs(script).unwrap().statements;
        assert_eq!(stmts.len(), 1);
        if let Statement::While { cond: _, body, body_lines: _ } = &stmts[0] {
            assert_eq!(body.len(), 2);
            assert!(matches!(&body[0], Statement::Assignment(a) if matches!(a.value, Expr::ForExpr { .. })));
        } else {
            panic!("expected While statement");
        }
    }

    #[test]
    fn test_parse_loop_keyword_prefix_identifiers() {
        let script = "for_item = 1\nwhile_count = 10\nfor_item + while_count";
        let stmts = parse_phs(script).unwrap().statements;
        assert_eq!(stmts.len(), 3);
        assert!(matches!(&stmts[0], Statement::Assignment(a) if a.name == "for_item"));
        assert!(matches!(&stmts[1], Statement::Assignment(a) if a.name == "while_count"));
    }
}

