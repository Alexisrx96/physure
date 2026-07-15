#[derive(Clone, Copy, Debug)]
pub struct PruningConfig {
    pub max_age: usize,
    pub enabled: bool,
    pub corr_threshold: f64,
}

impl Default for PruningConfig {
    fn default() -> Self {
        PruningConfig { max_age: 100, enabled: false, corr_threshold: 1e-6 }
    }
}

impl PruningConfig {
    pub fn new(max_age: usize, enabled: bool, corr_threshold: f64) -> Self {
        PruningConfig { max_age, enabled, corr_threshold }
    }
}
