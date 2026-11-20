//! The unit holes of `max`, `min`, `vector`, `linspace` and `round` (spec `2026-11-17-unit-holes-design.md`): each
//! one answered with a confident unit where there is no answer. Every refused row is checked with the words of the
//! error, and every valid neighbour still works.

use physure_script::interpreter::eval_phs;

fn refused(source: &str) -> String {
    match eval_phs(source) {
        Ok(values) => panic!("{source:?} gave {values:?}, expected an error"),
        Err(error) => error.to_string(),
    }
}

fn last(source: &str) -> String {
    eval_phs(source).unwrap_or_else(|e| panic!("{source:?}: {e}")).last().unwrap().to_string()
}

#[test]
fn an_extremum_of_different_dimensions_is_refused() {
    for source in ["max(1 m, 1 s)", "min(1 m, 2)", "max(2, 1 m)", "max(1 m, 1 kg, 1 s)", "max(1 rad, 1)"] {
        assert!(refused(source).contains("of different dimensions"), "{source}: {}", refused(source));
    }
    let message = refused("max(1 m, 1 s)");
    assert!(message.contains("max of different dimensions"), "{message}");
    assert!(message.contains("`1 m`") && message.contains("`1 s`"), "{message}");
    assert!(refused("min(1 m, 2)").contains("min of different dimensions"));
}

#[test]
fn an_extremum_of_one_dimension_still_works() {
    assert_eq!(last("max(1 m, 3 mm)"), last("1 m"));
    assert_eq!(last("min(1 m, 3 mm)"), last("3 mm"));
    assert!(eval_phs("max(5 degC, 3 degC)").is_ok());
    assert!(eval_phs("max(1 m, 2 +/- 1 m)").is_ok());
    assert!(eval_phs("max(100 percent, 2)").is_ok());
    assert!(eval_phs("max(1 rad, 2 deg)").is_ok());
    assert_eq!(last("min(3, 5)"), "3.0");
}

#[test]
fn a_single_argument_extremum_checks_what_it_is_given() {
    assert_eq!(last("max(vector(1 m, 2 m, 3 m))"), last("3 m"));
    assert_eq!(last("min(vector(1 m, 2 m, 3 m))"), last("1 m"));
    assert_eq!(last("max(2 m)"), last("2 m"));
    assert_eq!(last("max(4)"), "4.0");
    assert!(refused("max(vector(1 m, 2 s))").contains("a vector holds one dimension"));
    assert!(refused("max(\"a\")").contains("expects numbers or quantities"));
    assert!(refused("max(vector(\"a\", \"b\"))").contains("expects numbers or quantities"));
    assert!(refused("max(vector())").contains("max"));
}

#[test]
fn a_vector_holds_one_dimension() {
    for source in ["vector(1 m, 2 s)", "[1 m, 2 s]", "vector(1, 2 m)", "v = [1 m, 2 m, 3 s]\nv"] {
        assert!(refused(source).contains("a vector holds one dimension"), "{source}: {}", refused(source));
    }
    let message = refused("vector(1 m, 2 s)");
    assert!(message.contains("`1 m`") && message.contains("`2 s`"), "{message}");
}

#[test]
fn a_vector_of_one_dimension_keeps_working() {
    assert!(eval_phs("vector(1 m, 2 km)").is_ok());
    assert!(eval_phs("vector(1, 2)").is_ok());
    assert!(eval_phs("vector(1, 50 percent)").is_ok());
    assert!(eval_phs("vector(vector(1 m, 2 m), vector(3 m, 4 m))").is_ok());
    assert!(eval_phs("vector(\"a\", \"b\")").is_ok());
}

#[test]
fn an_elementwise_result_of_mixed_dimensions_is_refused() {
    // the spec's row: its right operand is already a vector of two dimensions, refused where it is made
    let message = refused("vector(1 m, 2 m) * vector(1 s, 2 m)");
    assert!(message.contains("a vector holds one dimension"), "{message}");
    // operands of one dimension each whose elementwise result would hold two: refused by the operation, named
    for source in ["vector(2 m, 3 m) ^ vector(1, 2)", "2 m ^ vector(1, 2)"] {
        let message = refused(source);
        assert!(message.contains("a vector holds one dimension: the elementwise `^`"), "{source}: {message}");
    }
    assert!(eval_phs("vector(2 m, 3 m) ^ vector(2, 2)").is_ok());
    assert!(eval_phs("vector(1 m, 2 m) * vector(1 s, 2 s)").is_ok());
    assert!(eval_phs("vector(1 m, 2 m) * 3 s").is_ok());
}

#[test]
fn linspace_needs_endpoints_of_one_dimension_and_a_plain_count() {
    assert!(refused("use * from array\nlinspace(0 m, 1 s, 3)").contains("linspace"));
    assert!(refused("use * from array\nlinspace(0 m, 10, 3)").contains("linspace"));
    assert!(refused("use * from array\nlinspace(0 m, 10 m, 5 s)").contains("linspace"));
    assert!(eval_phs("use * from array\nlinspace(0 m, 10 m, 3)").is_ok());
    assert!(eval_phs("use * from array\nlinspace(0, 10, 3)").is_ok());
    // the points are in the unit of the first endpoint
    assert_eq!(last("use * from array\nlinspace(0 m, 1 km, 3)"), last("vector(0 m, 500 m, 1000 m)"));
}

#[test]
fn round_takes_a_plain_number_of_decimals() {
    assert!(refused("round(1.5 m, 1 s)").contains("round"));
    assert!(refused("round(1.2345 m, 2 m)").contains("round"));
    assert_eq!(last("round(1.2345, 2)"), "1.23");
    assert!(eval_phs("round(1.5 m)").is_ok());
    assert_eq!(last("round(1.2345 m, 2)"), last("1.23 m"));
}

#[test]
fn a_level_and_a_plain_number_have_no_order_nor_a_vector() {
    // as `20 dB > 2` and `20 dB + 2` are refused: whether 2 is a ratio or 2 dB is a convention nothing written states
    for source in ["max(20 dB, 2)", "max(3 dB, 2)", "min(2, 20 dB)", "max(20 dB, 50 percent)"] {
        let message = refused(source);
        assert!(message.contains("of different dimensions") && message.contains("a level"), "{source}: {message}");
    }
    for source in ["vector(10 dB, 3)", "vector(10 dB, 3 percent)", "[3, 10 dB]"] {
        let message = refused(source);
        assert!(message.contains("a vector holds one dimension") && message.contains("a level"), "{source}: {message}");
    }
    // two levels compare, as `<` compares them
    assert_eq!(last("max(20 dB, 30 dB)"), last("30 dB"));
    assert!(eval_phs("vector(10 dB, 20 dB)").is_ok());
}

#[test]
fn an_angle_and_a_solid_angle_are_named_by_their_kind() {
    let message = refused("vector(1 sr, 1 rad)");
    assert!(message.contains("`1 sr` is a solid angle and `1 rad` an angle"), "{message}");
}

#[test]
fn every_vector_the_interpreter_builds_holds_one_dimension() {
    for (source, made_by) in [
        ("v = for i in 0..2 { if i == 0 then 1 m else 1 s }", "the `for` gives one where"),
        ("fn f(x) = for i in 0..2 { if i == 0 then x else 1 s }\nf(1 m)", "the `for` gives one where"),
        ("fn g(i) = if i == 1 then 1 m else 1 s\nparallel_map(g, [1, 2])", "`parallel_map` gives one where"),
        ("use * from array\ntranspose([[1 m], [1 s]])", "the transpose gives one where"),
        ("use * from array\ntranspose([[1 m, 2 m], [1 s, 2 s]])", "the transpose gives one where"),
    ] {
        let message = refused(source);
        assert!(message.contains(&format!("a vector holds one dimension: {made_by}")), "{source}: {message}");
    }
    // the same programs of one dimension are unchanged
    assert_eq!(last("v = for i in 0..2 { i * 1 m }\nw = for e in v { e }\nw"), last("[0 m, 1 m]"));
    assert!(eval_phs("fn g(i) = i * 1 m\nparallel_map(g, [1, 2])").is_ok());
    assert!(eval_phs("use * from array\ntranspose([[1 m, 2 m], [3 m, 4 km]])").is_ok());
}

#[test]
fn a_matrix_may_hold_rows_of_different_dimensions() {
    // decision of 2026-11-17: the rows are not compared with each other; each row holds one dimension
    assert!(eval_phs("[[1 m, 2 m], [1 s, 2 s]]").is_ok());
    assert!(refused("[[1 m, 2 s], [1 s, 2 s]]").contains("a vector holds one dimension"));
}

#[test]
fn linspace_has_numeric_endpoints_and_a_whole_count() {
    for source in ["linspace(0 m, \"x\", 3)", "linspace(\"x\", 2 m, 3)", "linspace(\"a\", \"b\", 3)", "linspace(True, 2, 3)"] {
        let message = refused(&format!("use * from array\n{source}"));
        assert!(message.contains("linspace's endpoints are numbers or quantities"), "{source}: {message}");
    }
    for source in ["linspace(0, 1, 50 percent)", "linspace(0, 1, 2.7)", "linspace(0, 1, -3)"] {
        let message = refused(&format!("use * from array\n{source}"));
        assert!(message.contains("linspace needs a whole, non-negative count"), "{source}: {message}");
    }
    // the default count stays 50, and a whole count written as a quantity is still a count
    assert!(last("use * from array\nlinspace(0, 1)").contains("(50 items)"));
    assert_eq!(last("use * from array\nlinspace(0, 1, 300 percent)"), last("[0, 0.5, 1]"));
    assert!(eval_phs("use * from array\nlinspace(0, 1, 0)").is_ok());
}

#[test]
fn round_takes_a_whole_number_of_decimals() {
    assert!(refused("round(1.2345, 2.7)").contains("round needs a whole number of decimals"));
    assert!(refused("round(1.2345 m, 1 percent)").contains("round needs a whole number of decimals"));
    assert_eq!(last("round(1234.5, -1)"), "1230.0");
    assert_eq!(last("round(1.2345 m, 2)"), last("1.23 m"));
}
