use sprs::{CsMat, TriMat};
use std::collections::HashMap;
use ndarray::{ArrayViewD, Ix1, Ix2};
use super::pruning::PruningConfig;

pub type VariableID = u64;

pub struct CovarianceStore {
    pub(crate) blocks: HashMap<(VariableID, VariableID), CsMat<f64>>,
    pub(crate) access_ledger: HashMap<VariableID, u64>,
    pub(crate) current_step: u64,
    pub(crate) config: PruningConfig,
}

impl CovarianceStore {
    pub fn new(config: PruningConfig) -> Self {
        CovarianceStore {
            blocks: HashMap::new(),
            access_ledger: HashMap::new(),
            current_step: 0,
            config,
        }
    }

    pub fn get_block_internal(&self, id1: VariableID, id2: VariableID) -> Option<CsMat<f64>> {
        let key = if id1 <= id2 { (id1, id2) } else { (id2, id1) };
        let mat = self.blocks.get(&key)?;
        if id1 <= id2 {
            Some(mat.clone())
        } else {
            Some(mat.transpose_view().to_csr())
        }
    }

    pub(crate) fn numpy_to_csr(arr: ArrayViewD<'_, f64>) -> CsMat<f64> {
        if let Ok(arr2) = arr.clone().into_dimensionality::<Ix2>() {
            let (rows, cols) = arr2.dim();
            let mut tri = TriMat::new((rows, cols));
            for ((r, c), &val) in arr2.indexed_iter() {
                if val.abs() > 1e-12 {
                     tri.add_triplet(r, c, val);
                }
            }
            return tri.to_csr();
        } 
        
        if let Ok(arr1) = arr.into_dimensionality::<Ix1>() {
             let size = arr1.dim();
             let mut tri = TriMat::new((size, 1));
             for (i, &val) in arr1.indexed_iter() {
                 if val.abs() > 1e-12 {
                     tri.add_triplet(i, 0, val);
                 }
             }
             return tri.to_csr();
        }
        
        TriMat::new((0, 0)).to_csr()
    }

    pub(crate) fn compute_output_variance(&self, input_ids: &[VariableID], js: &[CsMat<f64>]) -> Option<CsMat<f64>> {
        let mut sigma_out: Option<CsMat<f64>> = None;
        for (i, &id_i) in input_ids.iter().enumerate() {
            for (j, &id_j) in input_ids.iter().enumerate() {
                if let Some(sigma_ij) = self.get_block_internal(id_i, id_j) {
                    let term = if i == j {
                        crate::math::sparse_sandwich(&js[i], &sigma_ij)
                    } else {
                        let temp = &js[i] * &sigma_ij;
                        &temp * &js[j].transpose_view()
                    };
                    match sigma_out {
                        Some(ref mut acc) => *acc = &*acc + &term,
                        None => sigma_out = Some(term),
                    }
                }
            }
        }
        sigma_out
    }

    fn try_add_cross_cov_term(
        &self,
        out_id: VariableID,
        input_ids: &[VariableID],
        js: &[CsMat<f64>],
        input_id: VariableID,
        ext_id: VariableID,
        transpose: bool,
        acc: &mut HashMap<VariableID, CsMat<f64>>,
    ) {
        if ext_id == out_id { return; }
        let Some(idx_i) = input_ids.iter().position(|&x| x == input_id) else { return; };
        let block_key = if transpose { (ext_id, input_id) } else { (input_id, ext_id) };
        let Some(sigma) = self.blocks.get(&block_key) else { return; };
        let sigma_owned = if transpose { sigma.transpose_view().to_csr() } else { sigma.clone() };
        let term = &js[idx_i] * &sigma_owned;
        acc.entry(ext_id).and_modify(|a| *a = &*a + &term).or_insert(term);
    }

    fn accumulate_cross_covs(
        &self,
        out_id: VariableID,
        input_ids: &[VariableID],
        js: &[CsMat<f64>],
    ) -> HashMap<VariableID, CsMat<f64>> {
        let mut acc: HashMap<VariableID, CsMat<f64>> = HashMap::new();
        for &(r, c) in self.blocks.keys() {
            self.try_add_cross_cov_term(out_id, input_ids, js, r, c, false, &mut acc);
            if r != c {
                self.try_add_cross_cov_term(out_id, input_ids, js, c, r, true, &mut acc);
            }
        }
        acc
    }

    fn commit_cross_covs(&mut self, out_id: VariableID, cross_covs: HashMap<VariableID, CsMat<f64>>) {
        for (k, mat) in cross_covs {
            if out_id <= k {
                self.blocks.insert((out_id, k), mat);
            } else {
                self.blocks.insert((k, out_id), mat.transpose_view().to_csr());
            }
        }
    }

    fn prune_by_age(&mut self) {
        let max_age = self.config.max_age as u64;
        let limit = self.current_step.saturating_sub(max_age);
        let expired: Vec<VariableID> = self.access_ledger.iter()
            .filter(|(_, &last)| last < limit)
            .map(|(&id, _)| id)
            .collect();
        if !expired.is_empty() {
            self.blocks.retain(|(id1, id2), _| !expired.contains(id1) && !expired.contains(id2));
            for id in expired {
                self.access_ledger.remove(&id);
            }
        }
    }

    fn collect_variance_norms(&self) -> HashMap<VariableID, f64> {
        let mut norms = HashMap::new();
        for (&(r, c), mat) in &self.blocks {
            if r == c {
                norms.insert(r, mat.iter().map(|(&v, _)| v * v).sum::<f64>().sqrt());
            }
        }
        norms
    }

    fn prune_weak_correlations(&mut self) {
        let variance_norms = self.collect_variance_norms();
        let threshold = self.config.corr_threshold;
        let to_remove: Vec<(VariableID, VariableID)> = self.blocks.iter()
            .filter_map(|(&(r, c), mat)| {
                if r == c { return None; }
                let cov_norm: f64 = mat.iter().map(|(&v, _)| v * v).sum::<f64>().sqrt();
                if cov_norm == 0.0 { return Some((r, c)); }
                let norm_r = variance_norms.get(&r).cloned().unwrap_or(1.0);
                let norm_c = variance_norms.get(&c).cloned().unwrap_or(1.0);
                (cov_norm / (norm_r * norm_c).sqrt() < threshold).then_some((r, c))
            })
            .collect();
        for key in to_remove {
            self.blocks.remove(&key);
        }
    }

    pub fn register_variable(&mut self, var_id: VariableID, variance: ArrayViewD<'_, f64>) {
        let csr = Self::numpy_to_csr(variance);
        self.blocks.insert((var_id, var_id), csr);
        self.access_ledger.insert(var_id, self.current_step);
    }

    pub fn register_variable_slice(&mut self, var_id: VariableID, data: &[f64], shape: &[usize]) {
        let view = ArrayViewD::from_shape(shape, data).unwrap();
        self.register_variable(var_id, view);
    }

    pub fn register_diagonal(&mut self, var_id: VariableID, variance_diag: ArrayViewD<'_, f64>) {
        let size = variance_diag.len();
        let mut tri = TriMat::new((size, size));
        for (i, &val) in variance_diag.iter().enumerate() {
             if val.abs() > 1e-12 {
                 tri.add_triplet(i, i, val);
             }
        }
        self.blocks.insert((var_id, var_id), tri.to_csr());
        self.access_ledger.insert(var_id, self.current_step);
    }

    pub fn register_diagonal_slice(&mut self, var_id: VariableID, data: &[f64]) {
        let view = ArrayViewD::from_shape(ndarray::IxDyn(&[data.len()]), data).unwrap();
        self.register_diagonal(var_id, view);
    }

    pub fn propagate(
        &mut self,
        out_id: VariableID,
        input_ids: Vec<VariableID>,
        jacobians: Vec<ArrayViewD<'_, f64>>
    ) {
        self.current_step += 1;
        self.access_ledger.insert(out_id, self.current_step);
        let js: Vec<CsMat<f64>> = jacobians.into_iter().map(Self::numpy_to_csr).collect();

        if let Some(variance) = self.compute_output_variance(&input_ids, &js) {
            self.blocks.insert((out_id, out_id), variance);
        }

        let cross_covs = self.accumulate_cross_covs(out_id, &input_ids, &js);
        self.commit_cross_covs(out_id, cross_covs);

        if self.config.enabled {
            self.prune();
        }
    }

    pub fn propagate_slices(
        &mut self,
        out_id: VariableID,
        input_ids: Vec<VariableID>,
        jacobians: Vec<(&[f64], &[usize])>,
    ) {
        let views: Vec<_> = jacobians
            .into_iter()
            .map(|(data, shape)| ArrayViewD::from_shape(shape, data).unwrap())
            .collect();
        self.propagate(out_id, input_ids, views);
    }

    pub fn prune(&mut self) {
        self.prune_by_age();
        self.prune_weak_correlations();
    }
}
