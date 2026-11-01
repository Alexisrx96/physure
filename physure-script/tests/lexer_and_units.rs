//! The lexer of PHS (what it makes of numbers, strings, comments, fences, superscripts and operators) and the unit
//! that the symbolic expressions infer for what they combine.

use physure_script::symbolic::Expr;
use physure_script::{PhsLexer, TokenKind};

fn tokens(source: &str) -> Vec<(TokenKind, String)> {
    PhsLexer::new(source)
        .tokenize()
        .unwrap_or_else(|e| panic!("{source:?} did not tokenize: {e}"))
        .into_iter()
        .map(|t| (t.kind, t.value))
        .collect()
}

fn kinds(source: &str) -> Vec<TokenKind> {
    tokens(source).into_iter().map(|(kind, _)| kind).collect()
}

#[test]
fn numbers_are_read_with_their_fraction_and_exponent() {
    assert_eq!(kinds("12"), vec![TokenKind::Number(12.0)]);
    assert_eq!(kinds("1.5e3"), vec![TokenKind::Number(1500.0)]);
    assert_eq!(kinds("1e+2"), vec![TokenKind::Number(100.0)]);
    assert_eq!(kinds("2.5E-1"), vec![TokenKind::Number(0.25)]);
    assert_eq!(kinds(".5"), vec![TokenKind::Number(0.5)]);
    // an `e` that no digit follows belongs to the next word, not to the number
    assert_eq!(kinds("2em"), vec![TokenKind::Number(2.0), TokenKind::Ident("em".to_string())]);
    // a second dot ends the number
    assert_eq!(kinds("1.2.3"), vec![TokenKind::Number(1.2), TokenKind::Number(0.3)]);
}

#[test]
fn words_may_end_in_primes() {
    assert_eq!(kinds("f'' + x_1"), vec![
        TokenKind::Ident("f''".to_string()),
        TokenKind::Op("+".to_string()),
        TokenKind::Ident("x_1".to_string()),
    ]);
}

#[test]
fn comments_run_to_the_end_of_the_line() {
    assert_eq!(kinds("1 # a comment\n2 // another\n3"), vec![TokenKind::Number(1.0), TokenKind::Number(2.0), TokenKind::Number(3.0)]);
    assert!(tokens("# nothing but a comment").is_empty());
}

#[test]
fn strings_keep_what_is_between_the_quotes() {
    assert_eq!(kinds(r#""a b" 'c d'"#), vec![TokenKind::StringLiteral("a b".to_string()), TokenKind::StringLiteral("c d".to_string())]);
    let message = PhsLexer::new("\"never closed").tokenize().unwrap_err().to_string();
    assert!(message.contains("Unclosed string literal"), "{message}");
}

#[test]
fn a_display_fence_is_text_and_an_open_one_is_an_operator() {
    let fenced = tokens("```some text```");
    assert_eq!(fenced[0], (TokenKind::StringLiteral("some text".to_string()), "some text".to_string()));
    assert_eq!(fenced[1].0, TokenKind::Op("```".to_string()));
    // a fence that closes on a later line (the lexer sees one line at a time) is only its marker
    assert_eq!(kinds("```open"), vec![TokenKind::Op("```".to_string()), TokenKind::Ident("open".to_string())]);
}

#[test]
fn roots_and_superscripts_are_tokens_of_their_own() {
    assert_eq!(kinds("√x"), vec![TokenKind::Sqrt, TokenKind::Ident("x".to_string())]);
    // a superscript digit that touches a word is part of the word; after a space it is a power of its own
    assert_eq!(kinds("m²"), vec![TokenKind::Ident("m²".to_string())]);
    assert_eq!(kinds("m ²"), vec![TokenKind::Ident("m".to_string()), TokenKind::Sup("²".to_string())]);
    assert_eq!(kinds("s ⁻¹"), vec![TokenKind::Ident("s".to_string()), TokenKind::Sup("⁻¹".to_string())]);
}

#[test]
fn operators_are_read_longest_first_and_the_typographic_ones_are_canonical() {
    let ops = |source: &str| -> Vec<String> {
        tokens(source)
            .into_iter()
            .filter_map(|(kind, _)| if let TokenKind::Op(op) = kind { Some(op) } else { None })
            .collect()
    };
    assert_eq!(ops("a <= b >= c != d == e => f -> g ** h .. i"), ["<=", ">=", "!=", "==", "=>", "->", "**", ".."]);
    assert_eq!(ops("a × b ÷ c"), ["*", "/"]);
    assert_eq!(ops("5 +/- 1"), ["+/-"]);
    assert_eq!(ops("5 ± 1 ≈ 4"), ["±", "≈"]);
    assert_eq!(ops("f(a, b)[0]{x}; y: z | w"), ["(", ",", ")", "[", "]", "{", "}", ";", ":", "|"]);
}

#[test]
fn a_character_that_means_nothing_is_an_error_with_its_place() {
    let message = PhsLexer::new("a ? b @ c").tokenize().unwrap_err().to_string();
    assert!(message.contains("Unexpected character '@'") && message.contains("column"), "{message}");
}

fn unit(text: &str) -> physure_core::units::RationalUnit {
    physure_core::units::parser::Parser::parse_expression(text).unwrap()
}

#[test]
fn the_unit_of_a_product_a_quotient_and_a_power_follows_the_algebra_of_units() {
    let length = Expr::quantity("L".to_string(), &unit("m"));
    let time = Expr::quantity("t".to_string(), &unit("s"));
    let repr = |e: Expr| e.unit().unwrap().unwrap().__repr__();
    assert_eq!(repr(length.mul(&time)), "m * s");
    assert_eq!(repr(length.div(&time)), "m * s^-1");
    assert_eq!(repr(length.pow(&Expr::number(2.0))), "m^2");
    assert_eq!(repr(Expr::sqrt(&length.mul(&length))), "m");
    assert_eq!(repr(Expr::abs(&length)), "m");
    // a plain number alongside a quantity does not change the unit of a product or a quotient
    assert_eq!(repr(length.mul(&Expr::number(3.0))), "m");
    // the reciprocal of a time is a frequency
    assert_eq!(repr(Expr::number(1.0).div(&time)), "Hz");
    // a symbol has no unit of its own, and so leaves a sum's unit to the other term
    assert!(Expr::symbol("x".to_string()).unit().unwrap().is_none());
    assert_eq!(repr(length.add(&Expr::symbol("x".to_string())).unwrap()), "m");
}

#[test]
fn what_cannot_have_a_unit_is_refused() {
    let length = Expr::quantity("L".to_string(), &unit("m"));
    let time = Expr::quantity("t".to_string(), &unit("s"));
    let error = |e: Expr| e.unit().unwrap_err().to_string();
    assert!(error(length.pow(&time)).contains("non-constant power"));
    assert!(error(Expr::sin(&length)).contains("dimensionless"));
    assert!(error(Expr::exp(&time)).contains("dimensionless"));
    // a dimensionless argument is fine, and the function has no unit
    assert!(Expr::sin(&Expr::number(1.0)).unit().unwrap().is_none());
    assert!(length.add(&time).is_err());
    assert!(length.sub(&time).is_err());
}
