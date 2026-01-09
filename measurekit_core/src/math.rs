use sprs::{CsMat, SpIndex};

pub trait SandwichProduct {
    fn sandwich_product(&self, center: &CsMat<f64>) -> CsMat<f64>;
}

impl SandwichProduct for CsMat<f64> {
    fn sandwich_product(&self, center: &CsMat<f64>) -> CsMat<f64> {
        // (J * Sigma) * J.T
        let temp = self * center;
        &temp * &self.transpose_view()
    }
}

pub fn sparse_sandwich(j: &CsMat<f64>, sigma: &CsMat<f64>) -> CsMat<f64> {
    j.sandwich_product(sigma)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sprs::TriMat;

    #[test]
    fn test_sparse_sandwich_identity() {
        // J = Identity, Sigma = Identity -> Result = Identity
        let n = 3;
        let mut tri = TriMat::new((n, n));
        for i in 0..n {
            tri.add_triplet(i, i, 1.0);
        }
        let identity = tri.to_csc(); // sprs mul prefers CSC for rhs usually, but let's see. 
        // sprs 0.11: Mul<&CsMat, &CsMat> -> CsMat.
        
        let res = sparse_sandwich(&identity, &identity);
        
        // Check if diagonal result
        for i in 0..n {
            assert_eq!(res.get(i, i), Some(&1.0));
        }
    }
}
