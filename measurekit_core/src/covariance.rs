use pyo3::prelude::*;
use sprs::{CsMatI, TriMatI};

#[pyclass]
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

#[pyclass]
pub struct CovarianceStore {
    matrix: CsMatI<f64, usize>,
    last_updated: Vec<usize>,
    current_step: usize,
    config: PruningConfig,
    next_idx: usize,
}

#[pymethods]
impl CovarianceStore {
    #[new]
    #[pyo3(signature = (config = None))]
    fn new(config: Option<PruningConfig>) -> Self {
        CovarianceStore {
            matrix: CsMatI::new_csc((0, 0), vec![0], vec![], vec![]),
            last_updated: Vec::new(),
            current_step: 0,
            config: config.unwrap_or(PruningConfig { max_age: 100, enabled: false }),
            next_idx: 0,
        }
    }

    fn allocate(&mut self, size: usize) -> (usize, usize) {
        let start = self.next_idx;
        let end = start + size;
        self.next_idx = end;
        self.last_updated.resize(end, self.current_step);
        (start, end)
    }

    fn update_covariance(&mut self, _out_idx: (usize, usize), in_indices: Vec<(usize, usize)>) {
        self.current_step += 1;
        for (start, end) in in_indices {
            for i in start..end {
                self.last_updated[i] = self.current_step;
            }
        }
        if self.config.enabled {
            self.prune();
        }
    }

    fn prune(&mut self) {
        let max_age = self.config.max_age;
        let mut to_zero = Vec::new();
        for (i, &last) in self.last_updated.iter().enumerate() {
            if self.current_step - last > max_age {
                to_zero.push(i);
            }
        }
        if to_zero.is_empty() { return; }
        
        // Zero out elements associated with pruned variables
        let mut new_tri = TriMatI::new(self.matrix.shape());
        for (&val, (r, c)) in self.matrix.iter() {
            if !to_zero.contains(&r) && !to_zero.contains(&c) {
                new_tri.add_triplet(r, c, val);
            }
        }
        self.matrix = new_tri.to_csr();
    }
}
