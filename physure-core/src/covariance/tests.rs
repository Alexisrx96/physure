use super::*;
use sprs::{CsMat, TriMat};
use std::collections::HashMap;

fn make_store() -> CovarianceStore {
    CovarianceStore {
        blocks: HashMap::new(),
        config: PruningConfig { enabled: false, max_age: 100, corr_threshold: 0.0 },
        current_step: 0,
        access_ledger: HashMap::new(),
    }
}

fn diag(vals: &[f64]) -> CsMat<f64> {
    let n = vals.len();
    let mut t = TriMat::new((n, n));
    for (i, &v) in vals.iter().enumerate() {
        if v != 0.0 { t.add_triplet(i, i, v); }
    }
    t.to_csr()
}

fn mat_to_vec(m: &CsMat<f64>) -> Vec<f64> {
    let mut out = vec![0.0; m.rows() * m.cols()];
    for (v, (r, c)) in m.iter() {
        out[r * m.cols() + c] = *v;
    }
    out
}

#[test]
fn compute_variance_identity_jacobian() {
    let mut store = make_store();
    store.blocks.insert((0, 0), diag(&[1.0, 1.0]));
    let j = diag(&[1.0, 1.0]);
    let result = store.compute_output_variance(&[0], &[j]).unwrap();
    assert_eq!(mat_to_vec(&result), vec![1.0, 0.0, 0.0, 1.0]);
}

#[test]
fn to_arrow_setstate_roundtrip() {
    let mut store = make_store();
    store.blocks.insert((0, 0), diag(&[1.0, 2.0]));
    store.blocks.insert((1, 1), diag(&[3.0]));
    let bytes = store.to_arrow_bytes().unwrap();
    let mut store2 = make_store();
    store2.from_arrow_bytes(bytes).unwrap();
    assert!(store2.blocks.contains_key(&(0, 0)));
    assert!(store2.blocks.contains_key(&(1, 1)));
    assert_eq!(
        mat_to_vec(&store.blocks[&(0, 0)]),
        mat_to_vec(&store2.blocks[&(0, 0)])
    );
}
