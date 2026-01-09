use pyo3::prelude::*;
use sprs::{CsMat, TriMat};
use std::collections::HashMap;
use numpy::{PyReadonlyArrayDyn, PyArray1, IntoPyArray};
use numpy::ndarray::{Ix1, Ix2};

type VariableID = u64;

#[pyclass(module = "measurekit_core")]
#[derive(Clone, Copy)]
pub struct PruningConfig {
    #[pyo3(get, set)]
    pub max_age: usize,
    #[pyo3(get, set)]
    pub enabled: bool,
    #[pyo3(get, set)]
    pub corr_threshold: f64,
}

#[pymethods]
impl PruningConfig {

    #[new]
    #[pyo3(signature = (max_age = 100, enabled = false, corr_threshold = 1e-6))]
    fn new(max_age: usize, enabled: bool, corr_threshold: f64) -> Self {
        PruningConfig { max_age, enabled, corr_threshold }
    }
}

#[pyclass(module = "measurekit_core")]
pub struct CovarianceStore {
    blocks: HashMap<(VariableID, VariableID), CsMat<f64>>,
    access_ledger: HashMap<VariableID, u64>,
    current_step: u64,
    config: PruningConfig,
}

impl CovarianceStore {
    fn get_block_internal(&self, id1: VariableID, id2: VariableID) -> Option<CsMat<f64>> {
        let key = if id1 <= id2 { (id1, id2) } else { (id2, id1) };
        let mat = self.blocks.get(&key)?;
        if id1 <= id2 {
            Some(mat.clone())
        } else {
            Some(mat.transpose_view().to_csr())
        }
    }

    fn numpy_to_csr(arr: &PyReadonlyArrayDyn<f64>) -> CsMat<f64> {
        let view = arr.as_array();
        
        if let Ok(arr2) = view.clone().into_dimensionality::<Ix2>() {
            let (rows, cols) = arr2.dim();
            let mut tri = TriMat::new((rows, cols));
            for ((r, c), &val) in arr2.indexed_iter() {
                if val.abs() > 1e-12 {
                     tri.add_triplet(r, c, val);
                }
            }
            return tri.to_csr();
        } 
        
        if let Ok(arr1) = view.into_dimensionality::<Ix1>() {
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


}

#[pymethods]
impl CovarianceStore {
    #[new]
    #[pyo3(signature = (config = None))]
    fn new(config: Option<PruningConfig>) -> Self {
         let default_config = PruningConfig { max_age: 100, enabled: false, corr_threshold: 1e-6 };
        CovarianceStore {
            blocks: HashMap::new(),
            access_ledger: HashMap::new(),
            current_step: 0,
            config: config.unwrap_or(default_config),
        }
    }

    #[pyo3(signature = (var_id, variance))]
    fn register_variable(&mut self, var_id: VariableID, variance: PyReadonlyArrayDyn<f64>) -> PyResult<()> {
        let csr = Self::numpy_to_csr(&variance);
        self.blocks.insert((var_id, var_id), csr);
        self.access_ledger.insert(var_id, self.current_step);
        Ok(())
    }

    #[pyo3(signature = (var_id, variance_diag))]
    fn register_diagonal(&mut self, var_id: VariableID, variance_diag: PyReadonlyArrayDyn<f64>) -> PyResult<()> {
        let arr = variance_diag.as_array();
        let size = arr.len();
        let mut tri = TriMat::new((size, size));
        for (i, &val) in arr.iter().enumerate() {
             if val.abs() > 1e-12 {
                 tri.add_triplet(i, i, val);
             }
        }
        self.blocks.insert((var_id, var_id), tri.to_csr());
        self.access_ledger.insert(var_id, self.current_step);
        Ok(())
    }

    #[pyo3(signature = (out_id, input_ids, jacobians))]
    fn propagate(
        &mut self,
        out_id: VariableID,
        input_ids: Vec<VariableID>,
        jacobians: Vec<PyReadonlyArrayDyn<f64>>
    ) -> PyResult<()> {
        self.current_step += 1;
        self.access_ledger.insert(out_id, self.current_step);

        let js: Vec<CsMat<f64>> = jacobians.iter().map(|j| Self::numpy_to_csr(j)).collect();
        
        let mut sigma_out: Option<CsMat<f64>> = None;

        for (i, &id_i) in input_ids.iter().enumerate() {
            for (j, &id_j) in input_ids.iter().enumerate() {
                if let Some(sigma_ij) = self.get_block_internal(id_i, id_j) {
                     // Term: J_i * Sigma_ij * J_j^T
                     
                     let term_val = if i == j {
                         crate::math::sparse_sandwich(&js[i], &sigma_ij)
                     } else {
                         let temp = &js[i] * &sigma_ij;
                         &temp * &js[j].transpose_view()
                     };
                     
                     if let Some(ref mut acc) = sigma_out {
                         *acc = &*acc + &term_val;
                     } else {
                         sigma_out = Some(term_val);
                     }
                }
            }
        }

        if let Some(res) = sigma_out {
            self.blocks.insert((out_id, out_id), res);
        }

        // ---------------------------------------------------------------------
        // CROSS-CORRELATION STEP (Phase 1.2 / 4)
        // C_{new, k} = sum_i (J_i * Sigma_{i, k})
        // Where k is ANY existing variable in the store.
        // However, iterating over ALL k is expensive (O(N_vars)).
        // We only care about k where Sigma_{i, k} is non-zero.
        // I.e., k is correlated with at least one input i.
        //
        // Optimization:
        // We iterate over the input_ids (i).
        // For each i, we find all k such that Block(i, k) exists.
        // Then we accumulate: C_{new, k} += J_i * Sigma_{i, k}.
        // ---------------------------------------------------------------------
        
        // Map to accumulate cross-terms: k -> C_{new, k} (CsMat)
        // Since we are iterating, we might visit the same k multiple times (if k is correlated with multiple inputs).
        let mut cross_cov_accumulator: HashMap<VariableID, CsMat<f64>> = HashMap::new();
        
        // We need to iterate existing keys in self.blocks, but we are mutating self.blocks at the end.
        // Collecting keys relevant to inputs first is safer.
        // Or better: Iterate inputs i, then query self.blocks for keys involving i.
        // But HashMap doesn't support "get all keys with i".
        // We must scan `blocks` keys.
        // This is O(Total_Blocks), which is efficient if sparse.
        
        let all_block_keys: Vec<(VariableID, VariableID)> = self.blocks.keys().cloned().collect();
        
        for (r, c) in all_block_keys {
             // We are looking for block (r, c) where ONE of them is an input ID.
             // The OTHER one is 'k' (the external variable).
             
             let is_diag = r == c;
             
             // Check if r is in inputs?
             if let Some(idx_i) = input_ids.iter().position(|&x| x == r) {
                 // r is an input (Input Index idx_i).
                 // The correlation block is Sigma_{r, c}.
                 // This block represents correlation between Input r and Variable c.
                 // We want to calculate C_{new, c} += J_{idx_i} * Sigma_{r, c}.
                 
                 let k = c;
                 // Skip if k is the output itself (we only want cross-covs with others)
                 // Although theoretically C_{new, new} is Variance, already computed.
                 if k != out_id {
                     if let Some(sigma_ik) = self.blocks.get(&(r, c)) {
                         // eprintln!("  Processing r={} (Input {}), k={}. Found block.", r, idx_i, k);
                         let j_i = &js[idx_i];
                         
                         // Note: sigma_ik is &CsMat, j_i is &CsMat.
                         // Standard multiplication works.
                         let term = j_i * sigma_ik;

                         cross_cov_accumulator.entry(k)
                            .and_modify(|acc| *acc = &*acc + &term)
                            .or_insert(term);
                     }
                 }
             } 
             
             // Check if c is in inputs?
             // Only process if distinct from r (avoid double counting diagonals)
             if !is_diag {
                 if let Some(idx_i) = input_ids.iter().position(|&x| x == c) {
                     // c is an input (Input Index idx_i).
                     // The correlation block is Sigma_{r, c}.
                     // This block represents correlation between Variable r and Input c.
                     // We need Sigma_{c, r} (Correlation between Input c and Variable r).
                     // Sigma_{c, r} = Sigma_{r, c}.T.
                     
                     let k = r;
                     if k != out_id {
                         if let Some(sigma_rk) = self.blocks.get(&(r, c)) {
                             // eprintln!("  Processing c={} (Input {}), k={}. Found block.", c, idx_i, k);
                             // Need to transpose because stored block is (r, c) but we need (c, r)
                             let sigma_ik = sigma_rk.transpose_view();
                             let j_i = &js[idx_i];
                             
                             // Convert to owned to be safe with sprs
                             let sigma_ik_owned = sigma_ik.to_csr();
                             let term = j_i * &sigma_ik_owned;
                             
                             cross_cov_accumulator.entry(k)
                                .and_modify(|acc| *acc = &*acc + &term)
                                .or_insert(term);
                         }
                     }
                 }
             }
        }
        
        // Commit cross-covariances to store
        for (k, mat) in cross_cov_accumulator {

             
             if out_id <= k {
                 self.blocks.insert((out_id, k), mat);
             } else {
                 self.blocks.insert((k, out_id), mat.transpose_view().to_csr());
             }
        }
        
        if self.config.enabled {
             self.prune();
        }

        Ok(())
    }
    
    #[pyo3(name = "get_block_csr")] 
    fn get_block_csr_py<'py>(
        &self,
        py: Python<'py>,
        id1: VariableID,
        id2: VariableID
    ) -> PyResult<Option<(Bound<'py, PyArray1<f64>>, Bound<'py, PyArray1<i32>>, Bound<'py, PyArray1<i32>>, (usize, usize))>> {
        if let Some(mat) = self.get_block_internal(id1, id2) {
             let csr: CsMat<f64> = if mat.is_csr() { mat } else { mat.to_csr() };
             let shape = (csr.rows(), csr.cols());
             let (indptr, indices, data) = csr.into_raw_storage();
             
             let py_data = PyArray1::from_vec(py, data);
             let py_indices = PyArray1::from_vec(py, indices.iter().map(|&x| x as i32).collect());
             let py_indptr = PyArray1::from_vec(py, indptr.iter().map(|&x| x as i32).collect());
             
             Ok(Some((py_data, py_indices, py_indptr, shape)))
        } else {
            Ok(None)
        }
    }
    
    fn prune(&mut self) {
        // 1. Age-based Pruning
        let max_age = self.config.max_age as u64;
        let limit = if self.current_step > max_age { self.current_step - max_age } else { 0 };
        
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
        
        // 2. Relevance-based Pruning (Weak Correlation)
        // Iterate over off-diagonal blocks
        // We can't easily iterate and remove from self.blocks at the same time efficiently without collecting keys.
        let threshold = self.config.corr_threshold;
        
        if threshold > 0.0 {
             let mut to_remove = Vec::new();
             
             for (&(id1, id2), mat) in self.blocks.iter() {
                 if id1 != id2 {
                     // Check correlation strength
                     // Frobenius norm of Cov block
                     let cov_norm = mat.iter().map(|(&v, _)| v * v).sum::<f64>().sqrt();
                     
                     if cov_norm > 0.0 {
                         // Get variance norms (approximate denominator)
                         // We need self reference here, but we are borrowing immutable iterator.
                         // We can't call self.get_variance_norm in loop.
                         // But we can just lookup diagonal blocks if we are careful?
                         // Actually, accessing self.blocks inside self.blocks loop is tricky in Rust.
                         // Simplify: Calculate 'weakness' purely on absolute value? 
                         // Or collect keys first.
                         // Collecting keys to check is better.
                     } else {
                         // Zero block -> Remove
                         to_remove.push((id1, id2));
                     }
                 }
             }
             
             // Two-pass approach for full correlation check
             // Pass 1: Collect diagonal norms
             let mut variance_norms = HashMap::new();
              // Iterate keys to find diagonals
             let all_keys: Vec<(u64, u64)> = self.blocks.keys().cloned().collect();
             for (r, c) in &all_keys {
                 if r == c {
                     if let Some(mat) = self.blocks.get(&(*r, *c)) {
                         let norm = mat.iter().map(|(&v, _)| v * v).sum::<f64>().sqrt();
                         variance_norms.insert(*r, norm);
                     }
                 }
             }

             // Pass 2: Check off-diagonals
             for (r, c) in all_keys {
                 if r != c {
                     if let Some(mat) = self.blocks.get(&(r, c)) {
                         let cov_norm = mat.iter().map(|(&v, _)| v * v).sum::<f64>().sqrt();
                         
                         let norm_r = variance_norms.get(&r).cloned().unwrap_or(1.0);
                         let norm_c = variance_norms.get(&c).cloned().unwrap_or(1.0);
                         
                         let correlation = cov_norm / (norm_r * norm_c).sqrt();
                         
                         if correlation < threshold {
                             to_remove.push((r, c));
                         }
                     }
                 }
             }

             for key in to_remove {
                 self.blocks.remove(&key);
             }
        }
    }
}

