use pyo3::prelude::*;
use sprs::{CsMat, TriMat};
use std::collections::HashMap;
use numpy::{PyReadonlyArrayDyn, PyArray1, IntoPyArray};
use numpy::ndarray::{Ix1, Ix2};
use arrow::array::{UInt64Array, UInt32Array, Float64Array, ListBuilder, PrimitiveBuilder, Array, ArrayRef};
use arrow::datatypes::{DataType, Field, Schema, UInt64Type, UInt32Type, Float64Type, Int32Type};
use arrow::record_batch::RecordBatch;
use arrow::ipc::writer::StreamWriter;
use arrow::ipc::reader::StreamReader;
use arrow::array::{AsArray, ListArray};
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
        let mut reader = StreamReader::try_new(cursor, None)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Arrow reader error: {}", e)))?;
        
        // Clear existing blocks
        self.blocks.clear();

        while let Some(batch_result) = reader.next() {
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

