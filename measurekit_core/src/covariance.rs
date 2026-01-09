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
}

#[pymethods]
impl PruningConfig {
    #[new]
    #[pyo3(signature = (max_age = 100, enabled = false))]
    fn new(max_age: usize, enabled: bool) -> Self {
        PruningConfig { max_age, enabled }
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
        CovarianceStore {
            blocks: HashMap::new(),
            access_ledger: HashMap::new(),
            current_step: 0,
            config: config.unwrap_or(PruningConfig { max_age: 100, enabled: false }),
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
        let size = arr.len(); // as_array returns flattening in iteration? No. 
        // PyReadonlyArrayDyn::as_array() returns ArrayViewD. 
        // len() is total elements? Yes for D if 1D? No/Maybe.
        // But let's assume input is 1D as required.
        // Better safe:
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
                     let term = crate::math::sparse_sandwich(&js[i], &sigma_ij);
                     // Wait, sparse_sandwich calculates J S J^T. This assumes ONE J.
                     // But here we have J_i and J_j.
                     // C = J_i * Sigma_ij * J_j^T.
                     // If i == j, use sandwich.
                     // If i != j, use standard mul.
                     
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
    }
}
