//! Reactions: parsing an equation and finding the smallest whole stoichiometric coefficients.
//!
//! Balancing solves the element-conservation null space by Gauss-Jordan elimination over exact
//! rationals. Floats mis-round large stoichiometries (octane combustion), and every operation
//! is checked: an overflow is an error, never a wrapped number.
//!
//! Reference: Smith, W.R. & Missen, R.W. (1999), Chemical Reaction Stoichiometry (CRS): A Tutorial,
//! Chemical Engineering Education 33(1).

use std::collections::BTreeSet;
use std::fmt;

use num_rational::Rational64;
use num_traits::{CheckedDiv, CheckedMul, CheckedSub, Zero};

use crate::chemistry::formula::subscripts_to_ascii;
use crate::chemistry::species::Species;
use crate::error::{PhysureError, PhysureResult};

const ARROWS: [&str; 5] = ["<=>", "->", "=", "→", "⇌"];

type Row = Vec<Rational64>;

fn overflow() -> PhysureError {
    PhysureError::DomainError("the stoichiometric coefficients do not fit in 64-bit integers".into())
}

fn gcd(a: i64, b: i64) -> i64 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

fn lcm_checked(a: i64, b: i64) -> PhysureResult<i64> {
    (a / gcd(a, b)).checked_mul(b).ok_or_else(overflow)
}

/// Reduced row-echelon form of `m` and the columns that hold a pivot.
fn rref(mut m: Vec<Row>) -> PhysureResult<(Vec<Row>, Vec<usize>)> {
    let rows = m.len();
    let cols = m.first().map_or(0, Vec::len);
    let mut pivot_row = 0;
    let mut pivots = Vec::new();
    for col in 0..cols {
        let Some(found) = (pivot_row..rows).find(|&r| !m[r][col].is_zero()) else {
            continue;
        };
        m.swap(pivot_row, found);
        let pivot_value = m[pivot_row][col];
        for v in m[pivot_row].iter_mut() {
            *v = v.checked_div(&pivot_value).ok_or_else(overflow)?;
        }
        let pivot_values = m[pivot_row].clone();
        for (r, row) in m.iter_mut().enumerate() {
            if r == pivot_row || row[col].is_zero() {
                continue;
            }
            let factor = row[col];
            for (cell, p) in row.iter_mut().zip(&pivot_values) {
                let scaled = factor.checked_mul(p).ok_or_else(overflow)?;
                *cell = cell.checked_sub(&scaled).ok_or_else(overflow)?;
            }
        }
        pivots.push(col);
        pivot_row += 1;
        if pivot_row == rows {
            break;
        }
    }
    Ok((m, pivots))
}

/// The smallest positive whole coefficients that conserve every element, as
/// `(reactants, products)`.
pub fn balance(reactants: &[Species], products: &[Species]) -> PhysureResult<(Vec<i64>, Vec<i64>)> {
    let species: Vec<&Species> = reactants.iter().chain(products).collect();
    let n_react = reactants.len();
    let elements: BTreeSet<&str> =
        species.iter().flat_map(|s| s.composition().keys().map(String::as_str)).collect();
    let matrix: Vec<Row> = elements
        .iter()
        .map(|element| {
            species
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    let count = i64::from(s.composition().get(*element).copied().unwrap_or(0));
                    Rational64::from_integer(if i < n_react { count } else { -count })
                })
                .collect()
        })
        .collect();

    let (reduced, pivots) = rref(matrix)?;
    let free: Vec<usize> = (0..species.len()).filter(|c| !pivots.contains(c)).collect();
    if free.len() != 1 {
        return Err(PhysureError::DomainError(
            "cannot uniquely balance this reaction (expected exactly one degree of freedom)".into(),
        ));
    }
    let free_col = free[0];

    let zero = Rational64::from_integer(0);
    let mut coeffs = vec![zero; species.len()];
    coeffs[free_col] = Rational64::from_integer(1);
    for (row, &col) in pivots.iter().enumerate() {
        coeffs[col] = -reduced[row][free_col];
    }
    if coeffs.iter().any(|c| *c < zero) {
        for c in &mut coeffs {
            *c = -*c;
        }
    }
    if coeffs.iter().any(|c| *c <= zero) {
        return Err(PhysureError::DomainError(
            "cannot balance this reaction with positive integer coefficients".into(),
        ));
    }

    let mut denominators = 1_i64;
    for c in &coeffs {
        denominators = lcm_checked(denominators, *c.denom())?;
    }
    let mut ints = Vec::with_capacity(coeffs.len());
    for c in &coeffs {
        ints.push(c.numer().checked_mul(&(denominators / c.denom())).ok_or_else(overflow)?);
    }
    let common = ints.iter().fold(0, |g, &v| gcd(g, v));
    if common > 1 {
        for v in &mut ints {
            *v /= common;
        }
    }
    let products_ints = ints.split_off(n_react);
    Ok((ints, products_ints))
}

/// A balanced chemical reaction.
#[derive(Debug, Clone, PartialEq)]
pub struct Reaction {
    reactants: Vec<Species>,
    products: Vec<Species>,
    reactant_coeffs: Vec<i64>,
    product_coeffs: Vec<i64>,
    reversible: bool,
}

impl Reaction {
    pub fn new(reactants: Vec<Species>, products: Vec<Species>, reversible: bool) -> PhysureResult<Self> {
        let (reactant_coeffs, product_coeffs) = balance(&reactants, &products)?;
        Ok(Reaction { reactants, products, reactant_coeffs, product_coeffs, reversible })
    }

    pub fn reactants(&self) -> &[Species] {
        &self.reactants
    }

    pub fn products(&self) -> &[Species] {
        &self.products
    }

    /// The whole coefficients of the reactants, in the order of [`Reaction::reactants`].
    pub fn reactant_coeffs(&self) -> &[i64] {
        &self.reactant_coeffs
    }

    /// The whole coefficients of the products, in the order of [`Reaction::products`].
    pub fn product_coeffs(&self) -> &[i64] {
        &self.product_coeffs
    }

    pub fn reversible(&self) -> bool {
        self.reversible
    }

    /// Reads `"2 H2 + O2 -> 2 H2O"`. Coefficients that are written are re-derived. The
    /// separators are `->`, `=`, `→` (irreversible) and `<=>`, `⇌` (reversible).
    pub fn from_string(equation: &str) -> PhysureResult<Self> {
        let invalid = || PhysureError::ParseError(format!("invalid reaction equation: {equation:?}"));
        let (at, arrow) = find_arrow(equation, 0).ok_or_else(invalid)?;
        let end = at + arrow.len();
        if find_arrow(equation, end).is_some() {
            return Err(invalid());
        }
        let side = |text: &str| text.split('+').map(parse_term).collect::<PhysureResult<Vec<_>>>();
        let reversible = arrow == "<=>" || arrow == "⇌";
        Reaction::new(side(&equation[..at])?, side(&equation[end..])?, reversible)
    }
}

/// The leftmost separator at or after byte `from`, with the longer ones tried first.
fn find_arrow(text: &str, from: usize) -> Option<(usize, &'static str)> {
    text[from..].char_indices().find_map(|(i, _)| {
        ARROWS.iter().find(|arrow| text[from + i..].starts_with(**arrow)).map(|arrow| (from + i, *arrow))
    })
}

/// One term, `"2 H2O"` or `"H2O"`: an optional coefficient (ignored), then a formula.
fn parse_term(term: &str) -> PhysureResult<Species> {
    let text = subscripts_to_ascii(term.trim());
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    let formula = text[digits..].trim_start();
    if formula.is_empty() {
        return Err(PhysureError::ParseError(format!("invalid reaction term: {term:?}")));
    }
    Species::new(formula)
}

impl fmt::Display for Reaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn side(f: &mut fmt::Formatter<'_>, species: &[Species], coeffs: &[i64]) -> fmt::Result {
            for (i, (s, c)) in species.iter().zip(coeffs).enumerate() {
                if i > 0 {
                    write!(f, " + ")?;
                }
                if *c != 1 {
                    write!(f, "{c} ")?;
                }
                write!(f, "{}", s.formula())?;
            }
            Ok(())
        }
        side(f, &self.reactants, &self.reactant_coeffs)?;
        write!(f, " {} ", if self.reversible { "⇌" } else { "->" })?;
        side(f, &self.products, &self.product_coeffs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coeffs(equation: &str) -> (Vec<i64>, Vec<i64>) {
        let r = Reaction::from_string(equation).unwrap();
        (r.reactant_coeffs().to_vec(), r.product_coeffs().to_vec())
    }

    #[test]
    fn water() {
        assert_eq!(coeffs("H2 + O2 -> H2O"), (vec![2, 1], vec![2]));
    }

    #[test]
    fn octane_combustion_is_exact() {
        assert_eq!(coeffs("C8H18 + O2 -> CO2 + H2O"), (vec![2, 25], vec![16, 18]));
    }

    #[test]
    fn iron_ore_with_carbon() {
        assert_eq!(coeffs("Fe2O3 + C -> Fe + CO2"), (vec![2, 3], vec![4, 3]));
    }

    #[test]
    fn coefficients_already_written_are_rederived() {
        assert_eq!(coeffs("2 H2 + 5 O2 -> 9 H2O"), (vec![2, 1], vec![2]));
    }

    #[test]
    fn a_hydrate_balances() {
        assert_eq!(coeffs("CuSO4·5H2O -> CuSO4 + H2O"), (vec![1], vec![1, 5]));
    }

    #[test]
    fn every_separator_is_read_and_the_reversible_ones_are_flagged() {
        for (arrow, reversible) in [("->", false), ("=", false), ("→", false), ("<=>", true), ("⇌", true)] {
            let r = Reaction::from_string(&format!("N2 + 3 H2 {arrow} 2 NH3")).unwrap();
            assert_eq!(r.reversible(), reversible, "{arrow}");
            assert_eq!((r.reactant_coeffs(), r.product_coeffs()), (&[1, 3][..], &[2][..]), "{arrow}");
        }
    }

    #[test]
    fn it_prints_the_balanced_equation() {
        assert_eq!(Reaction::from_string("H2 + O2 -> H2O").unwrap().to_string(), "2 H2 + O2 -> 2 H2O");
        assert_eq!(Reaction::from_string("N2 + H2 <=> NH3").unwrap().to_string(), "N2 + 3 H2 ⇌ 2 NH3");
    }

    fn message(equation: &str) -> String {
        Reaction::from_string(equation).unwrap_err().to_string()
    }

    #[test]
    fn an_underdetermined_system_is_refused_not_solved_arbitrarily() {
        assert!(message("H2 + O2 -> H2O + H2O2").contains("one degree of freedom"));
    }

    #[test]
    fn a_system_with_no_solution_is_refused() {
        assert!(message("H2 -> O2").contains("one degree of freedom"));
    }

    #[test]
    fn a_solution_that_is_not_positive_is_refused() {
        assert!(message("H2 + H2O -> O2").contains("positive"));
    }

    #[test]
    fn malformed_equations_are_refused() {
        for bad in ["H2 + O2", "H2 -> O2 -> H2O", "H2 + -> H2O", "-> H2O", "H2 -> ", "2 -> H2", "H2 + O 2 -> H2O"] {
            assert!(Reaction::from_string(bad).is_err(), "{bad:?} should be refused");
        }
    }

    #[test]
    fn a_bad_formula_inside_an_equation_names_the_formula() {
        assert!(message("H2 + Xx -> H2").contains("Xx"));
    }

    #[test]
    fn overflowing_arithmetic_is_an_error_not_a_wrap() {
        assert!(lcm_checked(i64::MAX, 2).is_err());
        assert_eq!(lcm_checked(4, 6).unwrap(), 12);
        assert_eq!(gcd(12, 18), 6);
    }

    #[test]
    fn rref_reports_an_overflow_instead_of_wrapping() {
        let big = Rational64::from_integer(i64::MAX);
        let one = Rational64::from_integer(1);
        let err = rref(vec![vec![big, one], vec![one, big]]).unwrap_err();
        assert!(err.to_string().contains("64-bit"), "{err}");
    }
}
