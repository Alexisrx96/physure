//! Standard atomic weights: symbol, weight in g/mol, absolute uncertainty.
//!
//! The values are the IUPAC 2013 standard atomic weights (Meija et al. 2016), as the Python
//! package they were ported from has them, not the 2021 table (aluminium is 26.9815385 here,
//! 26.9815384 in 2021). Updating to 2021 is a deliberate follow-up that would change results.
//!
//! An element with no stable isotope has the mass of its longest-lived known isotope and an
//! uncertainty of 0.0. The table was generated from `ATOMIC_WEIGHTS` in
//! `physure-python/physure/ext/chemistry/species.py`; from here on this is the one copy.
//! Sorted by symbol (byte order) so that `atomic_weight` can binary-search it.
//!
//! Reference: Meija, J. et al. (2016), Pure and Applied Chemistry 88(3), 265-291,
//! <https://doi.org/10.1515/pac-2015-0305>.

static ATOMIC_WEIGHTS: &[(&str, f64, f64)] = &[
    ("Ac", 227.0, 0.0),
    ("Ag", 107.8682, 0.0002),
    ("Al", 26.9815385, 7e-07),
    ("Am", 243.0, 0.0),
    ("Ar", 39.948, 0.001),
    ("As", 74.921595, 6e-06),
    ("At", 210.0, 0.0),
    ("Au", 196.966569, 5e-06),
    ("B", 10.81, 0.01),
    ("Ba", 137.327, 0.007),
    ("Be", 9.0121831, 5e-07),
    ("Bh", 270.0, 0.0),
    ("Bi", 208.9804, 1e-05),
    ("Bk", 247.0, 0.0),
    ("Br", 79.904, 0.003),
    ("C", 12.011, 0.001),
    ("Ca", 40.078, 0.004),
    ("Cd", 112.414, 0.004),
    ("Ce", 140.116, 0.001),
    ("Cf", 251.0, 0.0),
    ("Cl", 35.45, 0.01),
    ("Cm", 247.0, 0.0),
    ("Cn", 285.0, 0.0),
    ("Co", 58.933194, 4e-06),
    ("Cr", 51.9961, 0.0006),
    ("Cs", 132.90545196, 6e-08),
    ("Cu", 63.546, 0.003),
    ("Db", 268.0, 0.0),
    ("Ds", 281.0, 0.0),
    ("Dy", 162.5, 0.001),
    ("Er", 167.259, 0.003),
    ("Es", 252.0, 0.0),
    ("Eu", 151.964, 0.001),
    ("F", 18.998403163, 1e-06),
    ("Fe", 55.845, 0.002),
    ("Fl", 289.0, 0.0),
    ("Fm", 257.0, 0.0),
    ("Fr", 223.0, 0.0),
    ("Ga", 69.723, 0.001),
    ("Gd", 157.25, 0.03),
    ("Ge", 72.63, 0.008),
    ("H", 1.008, 0.0002),
    ("He", 4.002602, 2e-06),
    ("Hf", 178.49, 0.02),
    ("Hg", 200.592, 0.003),
    ("Ho", 164.930328, 7e-06),
    ("Hs", 269.0, 0.0),
    ("I", 126.90447, 3e-05),
    ("In", 114.818, 0.001),
    ("Ir", 192.217, 0.002),
    ("K", 39.0983, 0.0001),
    ("Kr", 83.798, 0.002),
    ("La", 138.90547, 7e-05),
    ("Li", 6.94, 0.02),
    ("Lr", 266.0, 0.0),
    ("Lu", 174.9668, 0.0001),
    ("Lv", 293.0, 0.0),
    ("Mc", 290.0, 0.0),
    ("Md", 258.0, 0.0),
    ("Mg", 24.305, 0.001),
    ("Mn", 54.938044, 3e-06),
    ("Mo", 95.95, 0.01),
    ("Mt", 278.0, 0.0),
    ("N", 14.007, 0.001),
    ("Na", 22.98976928, 2e-08),
    ("Nb", 92.90637, 1e-05),
    ("Nd", 144.242, 0.003),
    ("Ne", 20.1797, 0.0006),
    ("Nh", 286.0, 0.0),
    ("Ni", 58.6934, 0.0004),
    ("No", 259.0, 0.0),
    ("Np", 237.0, 0.0),
    ("O", 15.999, 0.001),
    ("Og", 294.0, 0.0),
    ("Os", 190.23, 0.03),
    ("P", 30.973761998, 5e-09),
    ("Pa", 231.03588, 1e-05),
    ("Pb", 207.2, 0.1),
    ("Pd", 106.42, 0.01),
    ("Pm", 145.0, 0.0),
    ("Po", 209.0, 0.0),
    ("Pr", 140.90766, 1e-05),
    ("Pt", 195.084, 0.009),
    ("Pu", 244.0, 0.0),
    ("Ra", 226.0, 0.0),
    ("Rb", 85.4678, 0.0003),
    ("Re", 186.207, 0.001),
    ("Rf", 267.0, 0.0),
    ("Rg", 282.0, 0.0),
    ("Rh", 102.9055, 2e-05),
    ("Rn", 222.0, 0.0),
    ("Ru", 101.07, 0.02),
    ("S", 32.06, 0.02),
    ("Sb", 121.76, 0.001),
    ("Sc", 44.955908, 5e-06),
    ("Se", 78.971, 0.008),
    ("Sg", 269.0, 0.0),
    ("Si", 28.085, 0.001),
    ("Sm", 150.36, 0.02),
    ("Sn", 118.71, 0.007),
    ("Sr", 87.62, 0.01),
    ("Ta", 180.94788, 2e-05),
    ("Tb", 158.925354, 8e-06),
    ("Tc", 98.0, 0.0),
    ("Te", 127.6, 0.03),
    ("Th", 232.0377, 0.0004),
    ("Ti", 47.867, 0.001),
    ("Tl", 204.38, 0.01),
    ("Tm", 168.934218, 6e-06),
    ("Ts", 294.0, 0.0),
    ("U", 238.02891, 3e-05),
    ("V", 50.9415, 0.0001),
    ("W", 183.84, 0.01),
    ("Xe", 131.293, 0.006),
    ("Y", 88.90584, 1e-05),
    ("Yb", 173.045, 0.01),
    ("Zn", 65.38, 0.02),
    ("Zr", 91.224, 0.002),
];

/// `(weight in g/mol, absolute uncertainty)` of an element, by its symbol (`"Fe"`).
pub fn atomic_weight(symbol: &str) -> Option<(f64, f64)> {
    ATOMIC_WEIGHTS
        .binary_search_by_key(&symbol, |&(s, _, _)| s)
        .ok()
        .map(|i| (ATOMIC_WEIGHTS[i].1, ATOMIC_WEIGHTS[i].2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_has_118_elements_in_strictly_increasing_order() {
        assert_eq!(ATOMIC_WEIGHTS.len(), 118);
        assert!(ATOMIC_WEIGHTS.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn known_weights() {
        assert_eq!(atomic_weight("H"), Some((1.008, 0.0002)));
        assert_eq!(atomic_weight("Fe"), Some((55.845, 0.002)));
        assert_eq!(atomic_weight("Og"), Some((294.0, 0.0)));
    }

    #[test]
    fn a_symbol_that_is_not_an_element_is_none() {
        assert_eq!(atomic_weight("Xx"), None);
        assert_eq!(atomic_weight("h"), None);
        assert_eq!(atomic_weight(""), None);
    }
}
