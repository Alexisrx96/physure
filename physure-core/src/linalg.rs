use crate::error::{PhysureError, PhysureResult};
use crate::quantity::Quantity;

#[derive(Debug, Clone, PartialEq)]
pub struct QuantityVector {
    pub components: Vec<Quantity>,
}

impl QuantityVector {
    pub fn new(components: Vec<Quantity>) -> Self {
        Self { components }
    }

    pub fn len(&self) -> usize {
        self.components.len()
    }

    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }

    pub fn dot(&self, other: &QuantityVector) -> PhysureResult<Quantity> {
        if self.len() != other.len() {
            return Err(PhysureError::Generic("Vector dimension mismatch in dot product".into()));
        }
        if self.components.is_empty() {
            return Err(PhysureError::Generic("Cannot compute dot product of empty vectors".into()));
        }
        let mut sum = self.components[0].mul(&other.components[0])?;
        for i in 1..self.len() {
            let prod = self.components[i].mul(&other.components[i])?;
            sum = sum.add(&prod)?;
        }
        Ok(sum)
    }

    pub fn cross(&self, other: &QuantityVector) -> PhysureResult<QuantityVector> {
        if self.len() != 3 || other.len() != 3 {
            return Err(PhysureError::Generic("Cross product requires 3D vectors".into()));
        }
        let (a1, a2, a3) = (&self.components[0], &self.components[1], &self.components[2]);
        let (b1, b2, b3) = (&other.components[0], &other.components[1], &other.components[2]);

        let c1 = a2.mul(b3)?.sub(&a3.mul(b2)?)?;
        let c2 = a3.mul(b1)?.sub(&a1.mul(b3)?)?;
        let c3 = a1.mul(b2)?.sub(&a2.mul(b1)?)?;

        Ok(QuantityVector::new(vec![c1, c2, c3]))
    }

    pub fn norm(&self) -> PhysureResult<Quantity> {
        let dot_val = self.dot(self)?;
        dot_val.pow(0.5)
    }

    pub fn unit_vector(&self) -> PhysureResult<QuantityVector> {
        let n = self.norm()?;
        let mut unit_comps = Vec::with_capacity(self.len());
        for c in &self.components {
            unit_comps.push(c.div(&n)?);
        }
        Ok(QuantityVector::new(unit_comps))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuantityMatrix {
    pub rows: usize,
    pub cols: usize,
    pub data: Vec<Vec<Quantity>>,
}

impl QuantityMatrix {
    pub fn new(data: Vec<Vec<Quantity>>) -> PhysureResult<Self> {
        if data.is_empty() {
            return Err(PhysureError::Generic("Matrix cannot be empty".into()));
        }
        let rows = data.len();
        let cols = data[0].len();
        for r in &data {
            if r.len() != cols {
                return Err(PhysureError::Generic("Matrix rows must have equal length".into()));
            }
        }
        Ok(Self { rows, cols, data })
    }

    pub fn transpose(&self) -> Self {
        let mut transposed = vec![vec![self.data[0][0].clone(); self.rows]; self.cols];
        for r in 0..self.rows {
            for c in 0..self.cols {
                transposed[c][r] = self.data[r][c].clone();
            }
        }
        Self { rows: self.cols, cols: self.rows, data: transposed }
    }

    pub fn matmul(&self, other: &QuantityMatrix) -> PhysureResult<Self> {
        if self.cols != other.rows {
            return Err(PhysureError::Generic("Matrix multiplication dimension mismatch".into()));
        }
        let mut res_data = Vec::with_capacity(self.rows);
        for r in 0..self.rows {
            let mut row = Vec::with_capacity(other.cols);
            for c in 0..other.cols {
                let mut sum = self.data[r][0].mul(&other.data[0][c])?;
                for k in 1..self.cols {
                    let prod = self.data[r][k].mul(&other.data[k][c])?;
                    sum = sum.add(&prod)?;
                }
                row.push(sum);
            }
            res_data.push(row);
        }
        Self::new(res_data)
    }

    pub fn det(&self) -> PhysureResult<Quantity> {
        if self.rows != self.cols {
            return Err(PhysureError::Generic("Determinant requires a square matrix".into()));
        }
        if self.rows == 1 {
            return Ok(self.data[0][0].clone());
        }
        if self.rows == 2 {
            let a = &self.data[0][0];
            let b = &self.data[0][1];
            let c = &self.data[1][0];
            let d = &self.data[1][1];
            return a.mul(d)?.sub(&b.mul(c)?);
        }
        if self.rows == 3 {
            let a = &self.data[0][0]; let b = &self.data[0][1]; let c = &self.data[0][2];
            let d = &self.data[1][0]; let e = &self.data[1][1]; let f = &self.data[1][2];
            let g = &self.data[2][0]; let h = &self.data[2][1]; let i = &self.data[2][2];

            let term1 = a.mul(&e.mul(i)?.sub(&f.mul(h)?)?)?;
            let term2 = b.mul(&d.mul(i)?.sub(&f.mul(g)?)?)?;
            let term3 = c.mul(&d.mul(h)?.sub(&e.mul(g)?)?)?;

            return term1.sub(&term2)?.add(&term3);
        }
        Err(PhysureError::Generic("Determinant of matrices > 3x3 not yet supported".into()))
    }
}
