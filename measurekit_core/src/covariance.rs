use pyo3::prelude::*;
use sprs::{CsMat, TriMat};
use std::collections::HashMap;
use numpy::{PyReadonlyArrayDyn, PyArray1};
use numpy::ndarray::{Ix1, Ix2};
use arrow::array::{UInt64Array, UInt32Array, ListBuilder, PrimitiveBuilder, ArrayRef};
use arrow::datatypes::{DataType, Field, Schema, UInt64Type, UInt32Type, Float64Type, Int32Type};
use arrow::record_batch::RecordBatch;
use arrow::ipc::writer::StreamWriter;
use arrow::ipc::reader::StreamReader;
use arrow::array::AsArray;
use std::sync::Arc;
use std::io::Cursor;

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

    fn __getstate__(&self) -> (usize, bool, f64) {
        (self.max_age, self.enabled, self.corr_threshold)
    }

    fn __setstate__(&mut self, state: (usize, bool, f64)) {
        self.max_age = state.0;
        self.enabled = state.1;
        self.corr_threshold = state.2;
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

    fn compute_output_variance(&self, input_ids: &[VariableID], js: &[CsMat<f64>]) -> Option<CsMat<f64>> {
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
        let all_keys: Vec<(VariableID, VariableID)> = self.blocks.keys().cloned().collect();
        for (r, c) in all_keys {
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

        if let Some(variance) = self.compute_output_variance(&input_ids, &js) {
            self.blocks.insert((out_id, out_id), variance);
        }

        let cross_covs = self.accumulate_cross_covs(out_id, &input_ids, &js);
        self.commit_cross_covs(out_id, cross_covs);

        if self.config.enabled {
            self.prune();
        }
        Ok(())
    }
    
    fn to_arrow(&self) -> PyResult<Vec<u8>> {
        let mut keys: Vec<_> = self.blocks.keys().collect();
        keys.sort();

        let mut row_ids = Vec::with_capacity(keys.len());
        let mut col_ids = Vec::with_capacity(keys.len());
        let mut shapes_rows = Vec::with_capacity(keys.len());
        let mut shapes_cols = Vec::with_capacity(keys.len());
        
        let mut data_builder = ListBuilder::new(PrimitiveBuilder::<Float64Type>::new());
        let mut indices_builder = ListBuilder::new(PrimitiveBuilder::<Int32Type>::new());
        let mut indptr_builder = ListBuilder::new(PrimitiveBuilder::<Int32Type>::new());

        for &&(r, c) in &keys {
            let mat = &self.blocks[&(r, c)];
            row_ids.push(r);
            col_ids.push(c);
            shapes_rows.push(mat.rows() as u32);
            shapes_cols.push(mat.cols() as u32);
            
            // Robust extraction: Clone to get ownership of raw vecs
            // This avoids fighting with IndPtr views.
            let mat_owned = mat.clone();
            let (indptr, indices, data) = mat_owned.into_raw_storage();
            
            // Data
            data_builder.values().append_slice(&data);
            data_builder.append(true);
            
            // Indices
            for idx in indices {
                indices_builder.values().append_value(idx as i32);
            }
            indices_builder.append(true);

            // Indptr
            for ptr in indptr {
                indptr_builder.values().append_value(ptr as i32);
            }
            indptr_builder.append(true);
        }
        
        // Build Arrays
        let row_id_array = UInt64Array::from(row_ids);
        let col_id_array = UInt64Array::from(col_ids);
        let rows_array = UInt32Array::from(shapes_rows);
        let cols_array = UInt32Array::from(shapes_cols);
        let data_array = data_builder.finish();
        let indices_array = indices_builder.finish();
        let indptr_array = indptr_builder.finish();

        // Schema
        let schema = Schema::new(vec![
            Field::new("row_id", DataType::UInt64, false),
            Field::new("col_id", DataType::UInt64, false),
            Field::new("rows", DataType::UInt32, false),
            Field::new("cols", DataType::UInt32, false),
            Field::new("data", DataType::List(Arc::new(Field::new("item", DataType::Float64, true))), false),
            Field::new("indices", DataType::List(Arc::new(Field::new("item", DataType::Int32, true))), false),
            Field::new("indptr", DataType::List(Arc::new(Field::new("item", DataType::Int32, true))), false),
        ]);

        let batch = RecordBatch::try_new(
            Arc::new(schema),
            vec![
                Arc::new(row_id_array) as ArrayRef,
                Arc::new(col_id_array) as ArrayRef,
                Arc::new(rows_array) as ArrayRef,
                Arc::new(cols_array) as ArrayRef,
                Arc::new(data_array) as ArrayRef,
                Arc::new(indices_array) as ArrayRef,
                Arc::new(indptr_array) as ArrayRef,
            ],
        ).map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Arrow error: {}", e)))?;

        let mut buffer = Vec::new();
        {
            let mut writer = StreamWriter::try_new(&mut buffer, &batch.schema()).unwrap();
            writer.write(&batch).unwrap();
            writer.finish().unwrap();
        }
        Ok(buffer)
    }

    fn __getstate__(&self) -> PyResult<Vec<u8>> {
        self.to_arrow()
    }

    fn __setstate__(&mut self, state: Vec<u8>) -> PyResult<()> {
        let cursor = Cursor::new(state);
        let reader = StreamReader::try_new(cursor, None)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Arrow reader error: {}", e)))?;
        
        // Clear existing blocks
        self.blocks.clear();

        for batch_result in reader {
             let batch = batch_result.map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Arrow batch error: {}", e)))?;
             
             let row_ids = batch.column(0).as_primitive::<UInt64Type>();
             let col_ids = batch.column(1).as_primitive::<UInt64Type>();
             let rows_arr = batch.column(2).as_primitive::<UInt32Type>();
             let cols_arr = batch.column(3).as_primitive::<UInt32Type>();
             
             let data_list = batch.column(4).as_list::<i32>();
             let indices_list = batch.column(5).as_list::<i32>();
             let indptr_list = batch.column(6).as_list::<i32>();

             for i in 0..batch.num_rows() {
                 let r_id = row_ids.value(i);
                 let c_id = col_ids.value(i);
                 let n_rows = rows_arr.value(i) as usize;
                 let n_cols = cols_arr.value(i) as usize;
                 
                 // Extract CsMat components
                 // Data
                 let data_vals: Vec<f64> = data_list.value(i).as_primitive::<Float64Type>().values().to_vec();
                 
                 // Indices
                 let indices_vals: Vec<usize> = indices_list.value(i).as_primitive::<Int32Type>().values().iter().map(|&x| x as usize).collect();

                 // Indptr
                 let indptr_vals: Vec<usize> = indptr_list.value(i).as_primitive::<Int32Type>().values().iter().map(|&x| x as usize).collect();

                 // Reconstruct CsMat
                 // Note: CsMat::new takes validation, might panic if data invalid. Use try_new ideally.
                 let mat = CsMat::new((n_rows, n_cols), indptr_vals, indices_vals, data_vals);
                 self.blocks.insert((r_id, c_id), mat);
             }
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
        self.prune_by_age();
        if self.config.corr_threshold > 0.0 {
            self.prune_weak_correlations();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sprs::TriMat;
    use pyo3::Python;

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
        // J=I, Sigma=I → I*I*I^T = I
        assert_eq!(mat_to_vec(&result), vec![1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn compute_variance_scalar_jacobian() {
        let mut store = make_store();
        store.blocks.insert((0, 0), diag(&[4.0, 4.0]));
        let j = diag(&[2.0, 2.0]);
        let result = store.compute_output_variance(&[0], &[j]).unwrap();
        // J=2I, Sigma=4I → 2I*4I*2I^T = 16I
        assert_eq!(mat_to_vec(&result), vec![16.0, 0.0, 0.0, 16.0]);
    }

    #[test]
    fn compute_variance_returns_none_when_no_block() {
        let store = make_store();
        assert!(store.compute_output_variance(&[0], &[diag(&[1.0])]).is_none());
    }

    #[test]
    fn commit_cross_covs_ordered() {
        let mut store = make_store();
        let mat = diag(&[1.0, 1.0]);
        let mut cross = HashMap::new();
        cross.insert(5u64, mat.clone());
        store.commit_cross_covs(10, cross); // out_id=10 > k=5 → stored as (5,10)
        assert!(store.blocks.contains_key(&(5, 10)));
        assert!(!store.blocks.contains_key(&(10, 5)));
    }

    #[test]
    fn commit_cross_covs_reversed_key() {
        let mut store = make_store();
        let mat = diag(&[1.0, 1.0]);
        let mut cross = HashMap::new();
        cross.insert(20u64, mat.clone());
        store.commit_cross_covs(10, cross); // out_id=10 < k=20 → stored as (10,20)
        assert!(store.blocks.contains_key(&(10, 20)));
    }

    #[test]
    fn collect_variance_norms_diagonal_only() {
        let mut store = make_store();
        store.blocks.insert((0, 0), diag(&[3.0, 4.0])); // Frobenius = sqrt(9+16) = 5
        store.blocks.insert((0, 1), diag(&[1.0, 1.0])); // off-diag, should be ignored
        let norms = store.collect_variance_norms();
        assert_eq!(norms.len(), 1);
        assert!((norms[&0] - 5.0).abs() < 1e-10);
    }

    #[test]
    fn prune_by_age_removes_stale_blocks() {
        let mut store = CovarianceStore {
            blocks: HashMap::new(),
            config: PruningConfig { enabled: true, max_age: 2, corr_threshold: 0.0 },
            current_step: 10,
            access_ledger: [(0u64, 1u64), (1u64, 9u64)].into(), // 0=stale, 1=fresh
        };
        store.blocks.insert((0, 0), diag(&[1.0]));
        store.blocks.insert((1, 1), diag(&[1.0]));
        store.prune_by_age();
        assert!(!store.blocks.contains_key(&(0, 0)));
        assert!(store.blocks.contains_key(&(1, 1)));
    }

    #[test]
    fn prune_weak_correlations_removes_below_threshold() {
        let mut store = CovarianceStore {
            blocks: HashMap::new(),
            config: PruningConfig { enabled: true, max_age: 1000, corr_threshold: 0.5 },
            current_step: 0,
            access_ledger: HashMap::new(),
        };
        // var(0)=1, var(1)=1, cov(0,1)=0.1 → corr=0.1 < 0.5 → pruned
        store.blocks.insert((0, 0), diag(&[1.0]));
        store.blocks.insert((1, 1), diag(&[1.0]));
        store.blocks.insert((0, 1), diag(&[0.1]));
        store.prune_weak_correlations();
        assert!(!store.blocks.contains_key(&(0, 1)));
        assert!(store.blocks.contains_key(&(0, 0)));
        assert!(store.blocks.contains_key(&(1, 1)));
    }

    #[test]
    fn prune_strong_correlations_kept() {
        let mut store = CovarianceStore {
            blocks: HashMap::new(),
            config: PruningConfig { enabled: true, max_age: 1000, corr_threshold: 0.05 },
            current_step: 0,
            access_ledger: HashMap::new(),
        };
        store.blocks.insert((0, 0), diag(&[1.0]));
        store.blocks.insert((1, 1), diag(&[1.0]));
        store.blocks.insert((0, 1), diag(&[0.9])); // corr=0.9 > 0.05 → kept
        store.prune_weak_correlations();
        assert!(store.blocks.contains_key(&(0, 1)));
    }

    #[test]
    fn accumulate_cross_covs_one_shared_input() {
        let mut store = make_store();
        // input 0 has variance 1x1=[1], and cross-cov with var 2: sigma(0,2)=[0.5]
        store.blocks.insert((0, 0), diag(&[1.0]));
        store.blocks.insert((0, 2), diag(&[0.5]));
        let j = diag(&[2.0]); // J_0 = 2
        // C_{new,2} = J_0 * sigma(0,2) = 2 * 0.5 = 1
        let cross = store.accumulate_cross_covs(99, &[0], &[j]);
        assert!(cross.contains_key(&2));
        let v: Vec<f64> = cross[&2].iter().map(|(&x, _)| x).collect();
        assert!((v[0] - 1.0).abs() < 1e-10);
    }

    #[test]
    fn accumulate_cross_covs_excludes_output_id() {
        let mut store = make_store();
        store.blocks.insert((0, 0), diag(&[1.0]));
        store.blocks.insert((0, 99), diag(&[1.0])); // 99 = out_id, should be excluded
        let j = diag(&[1.0]);
        let cross = store.accumulate_cross_covs(99, &[0], &[j]);
        assert!(!cross.contains_key(&99));
    }

    #[test]
    fn get_block_internal_transpose_branch() {
        let mut store = make_store();
        // store block as (0,1); querying (1,0) should return its transpose
        store.blocks.insert((0, 1), diag(&[3.0, 4.0]));
        let direct = store.get_block_internal(0, 1).unwrap();
        let transposed = store.get_block_internal(1, 0).unwrap();
        // For a diagonal matrix, transpose == itself
        let d: Vec<f64> = direct.iter().map(|(&v, _)| v).collect();
        let t: Vec<f64> = transposed.iter().map(|(&v, _)| v).collect();
        assert_eq!(d, t);
    }

    #[test]
    fn compute_variance_two_inputs_covers_cross_term() {
        let mut store = make_store();
        store.blocks.insert((0, 0), diag(&[1.0]));
        store.blocks.insert((1, 1), diag(&[1.0]));
        store.blocks.insert((0, 1), diag(&[0.5])); // cross-cov
        let j0 = diag(&[1.0]);
        let j1 = diag(&[1.0]);
        // sigma_out = J0*S00*J0^T + J0*S01*J1^T + J1*S10*J0^T + J1*S11*J1^T = 1+0.5+0.5+1 = 3
        let result = store.compute_output_variance(&[0, 1], &[j0, j1]).unwrap();
        let vals: Vec<f64> = result.iter().map(|(&v, _)| v).collect();
        assert!((vals[0] - 3.0).abs() < 1e-10);
    }

    #[test]
    fn accumulate_cross_covs_transpose_path() {
        let mut store = make_store();
        // Block stored as (0, 2): r=0, c=2. Input is [2].
        // Second call: c=2 is input, ext_id=r=0, block_key=(0,2), transpose=true.
        store.blocks.insert((0, 2), diag(&[0.5]));
        let j = diag(&[2.0]);
        let cross = store.accumulate_cross_covs(99, &[2], &[j]);
        // C_{new,0} = J_2 * sigma(2,0) = J_2 * sigma(0,2)^T = 2 * 0.5 = 1.0
        assert!(cross.contains_key(&0));
        let v: Vec<f64> = cross[&0].iter().map(|(&x, _)| x).collect();
        assert!((v[0] - 1.0).abs() < 1e-10);
    }

    #[test]
    fn prune_by_age_no_op_when_nothing_expired() {
        let mut store = CovarianceStore {
            blocks: HashMap::new(),
            config: PruningConfig { enabled: true, max_age: 100, corr_threshold: 0.0 },
            current_step: 5,
            access_ledger: [(0u64, 5u64)].into(),
        };
        store.blocks.insert((0, 0), diag(&[1.0]));
        store.prune_by_age();
        assert!(store.blocks.contains_key(&(0, 0))); // nothing removed
    }

    #[test]
    fn pruning_config_new_and_state() {
        let cfg = PruningConfig::new(50, true, 0.1);
        assert_eq!(cfg.max_age, 50);
        assert!(cfg.enabled);
        assert!((cfg.corr_threshold - 0.1).abs() < 1e-10);

        let state = cfg.__getstate__();
        assert_eq!(state, (50, true, 0.1));

        let mut cfg2 = PruningConfig { max_age: 0, enabled: false, corr_threshold: 0.0 };
        cfg2.__setstate__(state);
        assert_eq!(cfg2.max_age, 50);
        assert!(cfg2.enabled);
    }

    #[test]
    fn covariance_store_new_default() {
        let store = CovarianceStore::new(None);
        assert!(!store.config.enabled);
        assert_eq!(store.current_step, 0);
        assert!(store.blocks.is_empty());
    }

    #[test]
    fn covariance_store_new_custom_config() {
        let cfg = PruningConfig { enabled: true, max_age: 50, corr_threshold: 0.1 };
        let store = CovarianceStore::new(Some(cfg));
        assert!(store.config.enabled);
        assert_eq!(store.config.max_age, 50);
    }

    #[test]
    fn pymethods_prune_triggers_both_phases() {
        let mut store = CovarianceStore {
            blocks: HashMap::new(),
            config: PruningConfig { enabled: true, max_age: 1, corr_threshold: 0.5 },
            current_step: 10,
            access_ledger: [(0u64, 1u64), (1u64, 9u64)].into(),
        };
        store.blocks.insert((0, 0), diag(&[1.0]));
        store.blocks.insert((1, 1), diag(&[1.0]));
        store.blocks.insert((0, 1), diag(&[0.1])); // weak → pruned
        store.prune();
        assert!(!store.blocks.contains_key(&(0, 0))); // stale
        assert!(!store.blocks.contains_key(&(0, 1))); // weak
        assert!(store.blocks.contains_key(&(1, 1)));  // fresh
    }

    #[test]
    fn to_arrow_setstate_roundtrip() {
        let mut store = make_store();
        store.blocks.insert((0, 0), diag(&[1.0, 2.0]));
        store.blocks.insert((1, 1), diag(&[3.0]));
        let bytes = store.to_arrow().unwrap();
        let mut store2 = make_store();
        store2.__setstate__(bytes).unwrap();
        assert!(store2.blocks.contains_key(&(0, 0)));
        assert!(store2.blocks.contains_key(&(1, 1)));
        assert_eq!(
            mat_to_vec(&store.blocks[&(0, 0)]),
            mat_to_vec(&store2.blocks[&(0, 0)])
        );
    }

    #[test]
    fn numpy_to_csr_2d_matrix() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let np = py.import("numpy").unwrap();
            let arr = np.call_method1("array", (vec![vec![1.0f64, 0.0], vec![0.0, 2.0]],)).unwrap();
            let readonly: PyReadonlyArrayDyn<f64> = arr.extract().unwrap();
            let csr = CovarianceStore::numpy_to_csr(&readonly);
            assert_eq!(csr.rows(), 2);
            assert_eq!(csr.cols(), 2);
        });
    }

    #[test]
    fn numpy_to_csr_1d_vector() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let np = py.import("numpy").unwrap();
            let arr = np.call_method1("array", (vec![1.0f64, 2.0, 3.0],)).unwrap();
            let readonly: PyReadonlyArrayDyn<f64> = arr.extract().unwrap();
            let csr = CovarianceStore::numpy_to_csr(&readonly);
            assert_eq!(csr.rows(), 3);
            assert_eq!(csr.cols(), 1);
        });
    }

    #[test]
    fn register_variable_stores_block() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let np = py.import("numpy").unwrap();
            let arr = np.call_method1("array", (vec![vec![4.0f64, 0.0], vec![0.0, 9.0]],)).unwrap();
            let readonly: PyReadonlyArrayDyn<f64> = arr.extract().unwrap();
            let mut store = CovarianceStore::new(None);
            store.register_variable(7, readonly).unwrap();
            assert!(store.blocks.contains_key(&(7, 7)));
        });
    }

    #[test]
    fn register_diagonal_stores_block() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let np = py.import("numpy").unwrap();
            let arr = np.call_method1("array", (vec![1.0f64, 4.0, 9.0],)).unwrap();
            let readonly: PyReadonlyArrayDyn<f64> = arr.extract().unwrap();
            let mut store = CovarianceStore::new(None);
            store.register_diagonal(3, readonly).unwrap();
            assert!(store.blocks.contains_key(&(3, 3)));
            let mat = &store.blocks[&(3, 3)];
            assert_eq!(mat.rows(), 3);
        });
    }

    #[test]
    fn propagate_via_numpy() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let np = py.import("numpy").unwrap();
            let sigma = np.call_method1("array", (vec![vec![1.0f64, 0.0], vec![0.0, 1.0]],)).unwrap();
            let jacobian = np.call_method1("array", (vec![vec![2.0f64, 0.0], vec![0.0, 2.0]],)).unwrap();
            let sigma_r: PyReadonlyArrayDyn<f64> = sigma.extract().unwrap();
            let jac_r: PyReadonlyArrayDyn<f64> = jacobian.extract().unwrap();
            let mut store = CovarianceStore::new(None);
            store.register_variable(0, sigma_r).unwrap();
            store.propagate(1, vec![0], vec![jac_r]).unwrap();
            assert!(store.blocks.contains_key(&(1, 1)));
        });
    }

    #[test]
    fn get_block_csr_py_returns_csr() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let mut store = make_store();
            store.blocks.insert((0, 0), diag(&[3.0, 4.0]));
            let result = store.get_block_csr_py(py, 0, 0).unwrap();
            assert!(result.is_some());
            let (_data, _indices, _indptr, shape) = result.unwrap();
            assert_eq!(shape, (2, 2));
        });
    }

    #[test]
    fn get_block_csr_py_missing_returns_none() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let store = make_store();
            let result = store.get_block_csr_py(py, 99, 99).unwrap();
            assert!(result.is_none());
        });
    }

    #[test]
    fn numpy_to_csr_3d_fallback_empty() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let np = py.import("numpy").unwrap();
            // 3D array → neither 2D nor 1D → returns empty (0,0) CSR
            let arr = np.call_method1("zeros", ((2, 2, 2),)).unwrap();
            let readonly: PyReadonlyArrayDyn<f64> = arr.extract().unwrap();
            let csr = CovarianceStore::numpy_to_csr(&readonly);
            assert_eq!(csr.rows(), 0);
            assert_eq!(csr.cols(), 0);
        });
    }

    #[test]
    fn covariance_store_getstate_setstate() {
        let mut store = make_store();
        store.blocks.insert((0, 0), diag(&[1.0, 4.0]));
        let bytes = store.__getstate__().unwrap();
        let mut store2 = make_store();
        store2.__setstate__(bytes).unwrap();
        assert!(store2.blocks.contains_key(&(0, 0)));
    }

    #[test]
    fn propagate_triggers_prune_when_enabled() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let np = py.import("numpy").unwrap();
            let sigma = np.call_method1("array", (vec![vec![1.0f64, 0.0], vec![0.0, 1.0]],)).unwrap();
            let jacobian = np.call_method1("array", (vec![vec![1.0f64, 0.0], vec![0.0, 1.0]],)).unwrap();
            let sigma_r: PyReadonlyArrayDyn<f64> = sigma.extract().unwrap();
            let jac_r: PyReadonlyArrayDyn<f64> = jacobian.extract().unwrap();
            // enabled=true with aggressive age pruning so prune() is called
            let cfg = PruningConfig { enabled: true, max_age: 1000, corr_threshold: 0.0 };
            let mut store = CovarianceStore::new(Some(cfg));
            store.register_variable(0, sigma_r).unwrap();
            store.propagate(1, vec![0], vec![jac_r]).unwrap();
            assert!(store.blocks.contains_key(&(1, 1)));
        });
    }
}

