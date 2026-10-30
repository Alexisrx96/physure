//! Deeply nested brackets: a parser that re-reads its contents at every level takes exponential time, and one
//! that recurses without a limit ends the process in a stack overflow no caller can catch.

use physure_script::parser::MAX_NESTING_DEPTH;
use physure_script::parse_phs;
use std::time::{Duration, Instant};

fn nested(open: &str, close: &str, depth: usize) -> String {
    format!("{}1{}", open.repeat(depth), close.repeat(depth))
}

/// 24 nested parentheses took 53 seconds (twice as long for each level more): `method_call | primary_base`
/// parsed the base once for the method-call attempt and again when no `.name(...)` followed.
#[test]
fn nested_parentheses_are_parsed_in_time_that_does_not_double_per_level() {
    for depth in [8, 16, 22, 30] {
        let started = Instant::now();
        parse_phs(&nested("(", ")", depth)).unwrap();
        assert!(started.elapsed() < Duration::from_secs(2), "{depth} levels took {:?}", started.elapsed());
    }
}

#[test]
fn the_limit_itself_is_accepted() {
    // on a thread with room for it: a debug build spends more stack per level than the 2 MB of a test thread
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            parse_phs(&nested("(", ")", MAX_NESTING_DEPTH)).unwrap();
            parse_phs(&nested("[", "]", MAX_NESTING_DEPTH)).unwrap();
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn one_level_past_the_limit_is_an_error_and_never_a_stack_overflow() {
    for (open, close) in [("(", ")"), ("[", "]"), ("{", "}")] {
        let error = parse_phs(&nested(open, close, MAX_NESTING_DEPTH + 1)).unwrap_err().to_string();
        assert!(error.contains("nested more than"), "{open}: {error}");
    }
    // far past it, on a thread too small to parse it: refused before the parser recurses at all
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| assert!(parse_phs(&nested("(", ")", 100_000)).is_err()))
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn brackets_in_strings_and_comments_are_text() {
    let deep = "(".repeat(MAX_NESTING_DEPTH * 3);
    parse_phs(&format!("x = \"{deep}\"")).unwrap();
    parse_phs(&format!("# {deep}\nx = 1")).unwrap();
    parse_phs(&format!("// {deep}\nx = 1")).unwrap();
    // and a comment ends at the end of its line
    assert!(parse_phs(&format!("# {deep}\nx = {}", nested("(", ")", MAX_NESTING_DEPTH + 1))).is_err());
}

#[test]
fn brackets_that_close_do_not_add_up() {
    // many groups side by side are not nested
    let flat = (0..500).map(|_| "(1)").collect::<Vec<_>>().join(" + ");
    parse_phs(&format!("x = {flat}")).unwrap();
}
