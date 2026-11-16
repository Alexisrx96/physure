//! The two-sided Student-t coverage factor against `scipy.stats` (`tests/golden/student_t_two_sided.tsv`,
//! written by `scripts/gen_student_t_golden.py`), and against the values the Guide prints in its Table G.2.

use physure_core::math::stats::student_t_two_sided;

const GOLDEN: &str = include_str!("../../tests/golden/student_t_two_sided.tsv");

fn rows() -> Vec<(f64, f64, f64)> {
    GOLDEN
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let mut fields = line.split('\t');
            let nu = match fields.next().unwrap() {
                "inf" => f64::INFINITY,
                number => number.parse().unwrap(),
            };
            let p = fields.next().unwrap().parse().unwrap();
            let k = fields.next().unwrap().parse().unwrap();
            (nu, p, k)
        })
        .collect()
}

#[test]
fn every_row_of_the_scipy_grid_is_reproduced_to_nine_digits() {
    let rows = rows();
    assert_eq!(rows.len(), 160, "20 degrees of freedom x 8 probabilities");
    for (nu, p, expected) in rows {
        let k = student_t_two_sided(p, nu).unwrap();
        let relative = ((k - expected) / expected).abs();
        assert!(relative < 1e-9, "nu={nu} p={p}: got {k}, scipy says {expected} (relative error {relative:e})");
    }
}

#[test]
fn the_values_the_guide_prints_are_reproduced() {
    // JCGM 100:2008, Table G.2: the 95.45 % and 99 % factors for 16 degrees of freedom, as the Guide rounds them
    let k = |p: f64, nu: f64| student_t_two_sided(p, nu).unwrap();
    assert_eq!(format!("{:.2}", k(0.95, 16.0)), "2.12");
    assert_eq!(format!("{:.2}", k(0.99, 16.0)), "2.92");
    assert_eq!(format!("{:.2}", k(0.95, f64::INFINITY)), "1.96");
    assert_eq!(format!("{:.2}", k(0.99, f64::INFINITY)), "2.58");
    // one degree of freedom is the Cauchy distribution: the median of |T| is 1 and the 95 % factor is 12.706
    assert!((k(0.5, 1.0) - 1.0).abs() < 1e-12);
    assert!((k(0.95, 1.0) - 12.7062047).abs() < 1e-6);
}

#[test]
fn a_fractional_number_of_degrees_of_freedom_lies_between_its_neighbours() {
    let k = |nu: f64| student_t_two_sided(0.95, nu).unwrap();
    assert!(k(16.0) > k(16.75) && k(16.75) > k(17.0));
    assert!((k(16.7519) - 2.1122).abs() < 1e-3, "the Guide's budget, not truncated: {}", k(16.7519));
}

#[test]
fn what_has_no_factor_is_refused_with_the_reason() {
    for (p, nu) in [(0.0, 5.0), (1.0, 5.0), (-0.1, 5.0), (1.5, 5.0), (f64::NAN, 5.0), (0.95, 0.0), (0.95, -2.0), (0.95, f64::NAN)] {
        let error = student_t_two_sided(p, nu).unwrap_err().to_string();
        assert!(error.contains("Domain error"), "p={p} nu={nu}: {error}");
    }
}
