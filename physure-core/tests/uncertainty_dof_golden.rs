//! The core against `tests/golden/uncertainty_dof.tsv`: the effective degrees of freedom, coverage factor and
//! expanded uncertainty of a sum of independent measurements, as JCGM 100:2008 G.4 gives them (computed
//! independently with scipy by `scripts/gen_uncertainty_dof_golden.py`). The Java and WASM bindings are held to
//! the same file.

use physure_core::Quantity;

const GOLDEN: &str = include_str!("../../tests/golden/uncertainty_dof.tsv");

struct Row {
    name: String,
    budget: Vec<(f64, f64, f64)>,
    p: f64,
    nu_eff: f64,
    k: f64,
    u_c: f64,
    big_u: f64,
}

fn number(text: &str) -> f64 {
    if text == "inf" {
        f64::INFINITY
    } else {
        text.parse().unwrap_or_else(|e| panic!("{text:?}: {e}"))
    }
}

fn rows() -> Vec<Row> {
    GOLDEN
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let f: Vec<&str> = line.trim_end_matches('\r').split('\t').collect();
            let budget = f[1]
                .split(';')
                .map(|term| {
                    let parts: Vec<&str> = term.split(':').collect();
                    (number(parts[0]), number(parts[1]), number(parts[2]))
                })
                .collect();
            Row { name: f[0].to_string(), budget, p: number(f[2]), nu_eff: number(f[3]), k: number(f[4]), u_c: number(f[5]), big_u: number(f[6]) }
        })
        .collect()
}

fn close(a: f64, b: f64, relative: f64) -> bool {
    a == b || ((a - b) / b).abs() < relative
}

/// The sum of the measurements of a budget, each one declared with its degrees of freedom when it has them.
fn sum_of(budget: &[(f64, f64, f64)]) -> Quantity {
    let mut parts = budget.iter().map(|&(mean, u, nu)| {
        let measurement = Quantity::with_uncertainty(mean, u, "m").unwrap();
        if nu.is_finite() { measurement.with_dof(nu).unwrap() } else { measurement }
    });
    let first = parts.next().expect("a budget has a measurement");
    parts.fold(first, |sum, part| sum.add(&part).unwrap())
}

#[test]
fn every_row_of_the_golden_file_is_reproduced() {
    let rows = rows();
    assert_eq!(rows.len(), 11);
    for row in rows {
        let sum = sum_of(&row.budget);
        let expanded = sum.expanded(row.p).unwrap();
        let name = &row.name;
        assert!(close(sum.value.std_dev(), row.u_c, 1e-12), "{name}: u_c {} vs {}", sum.value.std_dev(), row.u_c);
        assert!(close(expanded.dof_eff, row.nu_eff, 1e-12), "{name}: nu_eff {} vs {}", expanded.dof_eff, row.nu_eff);
        assert!(close(expanded.coverage_factor, row.k, 1e-9), "{name}: k {} vs {}", expanded.coverage_factor, row.k);
        assert!(close(expanded.expanded_uncertainty, row.big_u, 1e-9), "{name}: U {} vs {}", expanded.expanded_uncertainty, row.big_u);
    }
}
