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
    /// and it reads exactly like a forgotten operator: `total = mass speed` used to
    /// silently return `mass * speed` with a plausible-looking unit and no error at all.
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
    fn a_literal_declares_its_degrees_of_freedom_after_the_uncertainty() {
        // `dof` or the Greek nu, with `+/-` or `±`, before or after the unit
        for code in [
            "x = 10.003 +/- 0.002 dof 9 V",
            "x = 10.003 ± 0.002 dof 9 V",
            "x = 10.003 +/- 0.002 ν 9 V",
            "x = 10.003 V +/- 0.002 dof 9",
            "x = 10.003 V ± 0.002 ν 9",
            "x = 10.003 +/- 0.002 dof 9.0 V",
        ] {
            let q = parse_one_quantity(code);
            assert_eq!((q.magnitude, q.uncertainty, q.dof), (10.003, Some(0.002), Some(9.0)), "{code}");
            assert_eq!(q.unit.as_deref(), Some("V"), "{code}");
        }
    }

    #[test]
    fn the_degrees_of_freedom_may_be_fractional_infinite_and_follow_a_percentage() {
        assert_eq!(parse_one_quantity("x = 5 +/- 0.2 dof 16.7 m").dof, Some(16.7));
        assert_eq!(parse_one_quantity("x = 5 +/- 0.2 dof inf m").dof, Some(f64::INFINITY));
        let relative = parse_one_quantity("x = 5 +/- 2% dof 4 m");
        assert_eq!((relative.uncertainty, relative.dof), (Some(0.1), Some(4.0)));
    }

    #[test]
    fn a_literal_without_the_clause_has_none() {
        assert_eq!(parse_one_quantity("x = 75.0 +/- 0.5 kg").dof, None);
        assert_eq!(parse_one_quantity("x = 75.0 kg").dof, None);
    }

    #[test]
    fn degrees_of_freedom_that_make_no_sense_are_refused_when_the_program_is_read() {
        let refused = |code: &str| parse_phs(code).err().unwrap_or_else(|| panic!("{code} should not parse")).to_string();
        assert!(refused("x = 5 +/- (0.5, 0.4) dof 4 m").contains("asymmetric"));
        assert!(refused("x = 5 +/- 2 sigma dof 4 m").contains("sigma"));
        assert!(refused("x = 5 +/- 0.5 dof 0 m").contains("positive"));
        assert!(refused("x = 5 +/- 0.5 dof -2 m").contains("positive, got -2"));
    }

    #[test]
    fn dof_and_nu_stay_ordinary_names_outside_the_clause() {
        // a variable called dof, a call dof(x), and a sum with a parenthesised addend are not the clause
        let prog = parse_phs("dof = 3\ny = dof + 1\nz = 12.3 + (0.5)").unwrap();
        assert_eq!(prog.statements.len(), 3);
        assert!(parse_phs("a = 5 +/- 1 m\nb = dof(a)").is_ok());
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
        let code = "f(v: m / s) =\n    offset = 1 m / s\n    v * 2 - offset";
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

    #[test]
    fn parses_bool_literals() {
        let prog = parse_phs("True\nFalse").unwrap();
        assert!(matches!(&prog.statements[0], Statement::Expr(Expr::Bool(true))));
        assert!(matches!(&prog.statements[1], Statement::Expr(Expr::Bool(false))));
    }

    #[test]
    fn true_and_false_are_reserved_against_identifiers() {
        assert!(parse_phs("True = 5").is_err());
        assert!(parse_phs("False = 5").is_err());
    }

    #[test]
    fn not_and_or_parse_with_the_documented_precedence_and_associativity() {
        // `not pressure > limit and enabled or override`
        //   == `((not (pressure > limit)) and enabled) or override`
        let prog = parse_phs("not a > b and c or d").unwrap();
        let Statement::Expr(expr) = &prog.statements[0] else { panic!("expected expr") };
        let Expr::FunctionCall { name: outer_name, args: outer_args, .. } = expr else { panic!("expected or_ call") };
        assert_eq!(outer_name, "op_or");
        let Expr::FunctionCall { name: and_name, args: and_args, .. } = &outer_args[0] else { panic!("expected and_ call") };
        assert_eq!(and_name, "op_and");
        let Expr::FunctionCall { name: not_name, .. } = &and_args[0] else { panic!("expected not_ call") };
        assert_eq!(not_name, "op_not");
        assert!(matches!(&outer_args[1], Expr::Identifier(d) if d == "d"));
        assert!(matches!(&and_args[1], Expr::Identifier(c) if c == "c"));
    }

    #[test]
    fn repeated_not_nests_correctly() {
        let prog = parse_phs("not not True").unwrap();
        let Statement::Expr(Expr::FunctionCall { name: outer, args: outer_args, .. }) = &prog.statements[0] else { panic!() };
        assert_eq!(outer, "op_not");
        let Expr::FunctionCall { name: inner, args: inner_args, .. } = &outer_args[0] else { panic!() };
        assert_eq!(inner, "op_not");
        assert!(matches!(&inner_args[0], Expr::Bool(true)));
    }

    #[test]
    fn and_or_accept_line_breaks_around_them() {
        let prog = parse_phs("True\n  and\n  False\n  or\n  True").unwrap();
        assert!(matches!(&prog.statements[0], Statement::Expr(Expr::FunctionCall { name, .. }) if name == "op_or"));
    }

    #[test]
    fn a_dangling_and_or_or_is_a_parse_error() {
        assert!(parse_phs("True and").is_err());
        assert!(parse_phs("and True").is_err());
    }

    #[test]
    fn not_re_enters_the_or_expr_chain_through_a_parenthesized_group() {
        let prog = parse_phs("not (a and b)").unwrap();
        let Statement::Expr(Expr::FunctionCall { name: outer, args: outer_args, .. }) = &prog.statements[0] else { panic!() };
        assert_eq!(outer, "op_not");
        let Expr::FunctionCall { name: inner, args: inner_args, .. } = &outer_args[0] else { panic!("expected op_and inside the parens") };
        assert_eq!(inner, "op_and");
        assert!(matches!(&inner_args[0], Expr::Identifier(a) if a == "a"));
        assert!(matches!(&inner_args[1], Expr::Identifier(b) if b == "b"));
    }

    #[test]
    fn bool_and_logical_expressions_parse_as_call_arguments() {
        // A later task wires `assert(condition)` through exactly this call-arg position.
        assert!(parse_phs("f(True)").is_ok());
        assert!(parse_phs("f(a and b)").is_ok());
    }

    /// Two statements on one line need a `;` between them. Without one, `1 m => km 2` was the two statements
    /// `1 m => km` and `2` and answered `2.0`: whatever follows a unit after `=>` started a statement of its own.
    #[test]
    fn two_statements_on_one_line_without_a_separator_are_a_parse_error() {
        for glued in [
            "1 m => km 2",
            "x = 2 m => km 3",
            "f(x) = x => km 2",
            "5 m => km \"t\"",
            "x = 1 m => km [1, 2]",
            "1 m => km (2)",
            "2 m => km for i in 0..2 { i }",
            "1 m => km if True then 1 else 2",
            "a not b",
            "x = 0\nwhile x < 3 {\n  x = x + 1\n  x => km 2\n}",
        ] {
            let message = parse_phs(glued).map(|_| ()).expect_err(glued).to_string();
            assert!(message.contains("expected a newline or `;`"), "{glued:?}: {message}");
            let lines = parse_phs_with_lines(glued).map(|_| ()).expect_err(glued).to_string();
            assert_eq!(lines, message, "{glued:?}");
        }
        // the message points at the statement that has no separator before it
        let message = parse_phs("y = 1\n1 m => km 2").map(|_| ()).unwrap_err().to_string();
        assert!(message.contains("--> 2:11"), "{message}");
    }

    #[test]
    fn statements_separated_by_a_newline_or_a_semicolon_still_parse() {
        for separated in [
            "1 m => km; 2",
            "1 m => km\n2",
            "1 m => km # a comment\n2",
            "1 m => km // a comment\n2",
            "a = 1; b = 2; a + b",
            "1 m => km -2",
            "fn f(x) = x * 2; f(3)",
            "fn f(x) =\n  a = x\n  a\nf(2)",
            "f(x) = x => km\nf(1 m)",
            "@requires(x > 0, \"positive\")\nfn f(x) = x\nf(1)",
            "/// doubles\nfn f(x) = 2 * x\nf(1)",
            "c = True\nc ? 1 : 2",
            "y = x * 2 where x = 3",
            "y = x * 2 where x = 3, z = 4",
            "x = 1\nwhile x < 3 { x = x + 1 }",
            "x = 1\nwhile x < 3 {\n  x = x + 1\n  y = 2\n}",
            "x = if True then 1 m else 2 m",
            "fn g(x) =\n  if x < 0 then return 0\n  x\ng(1)",
            "\n\n1 m\n\n;;2 m;\n",
        ] {
            parse_phs(separated).unwrap_or_else(|e| panic!("{separated:?}: {e}"));
            parse_phs_with_lines(separated).unwrap_or_else(|e| panic!("{separated:?}: {e}"));
        }
    }
