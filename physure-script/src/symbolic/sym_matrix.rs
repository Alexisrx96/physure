use physure_core::error::{PhysureError, PhysureResult};
use super::ast::Node;
use super::parser::SymbolicParser;

#[derive(Clone, Debug, PartialEq)]
pub struct SymMatrix {
    pub rows: usize,
    pub cols: usize,
    pub data: Vec<Vec<Node>>,
}

impl SymMatrix {
    pub fn new(data: Vec<Vec<Node>>) -> PhysureResult<Self> {
        let rows = data.len();
        if rows == 0 {
            return Err(PhysureError::Generic("Matrix cannot be empty".into()));
        }
        let cols = data[0].len();
        for (i, row) in data.iter().enumerate() {
            if row.len() != cols {
                return Err(PhysureError::Generic(format!(
                    "Inconsistent column count at row {}: expected {}, got {}",
                    i, cols, row.len()
                )));
            }
        }
        Ok(SymMatrix { rows, cols, data })
    }

    /// Parses a string formatted matrix like "[[a, b], [c, d]]"
    pub fn parse_str(input: &str) -> PhysureResult<Self> {
        let trimmed = input.trim();
        if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
            return Err(PhysureError::Generic("Matrix must be enclosed in []".into()));
        }
        let inner = &trimmed[1..trimmed.len() - 1].trim();
        
        let mut rows = Vec::new();
        let mut current_row = String::new();
        let mut depth = 0;

        for ch in inner.chars() {
            match ch {
                '[' => {
                    depth += 1;
                    current_row.push(ch);
                }
                ']' => {
                    depth -= 1;
                    current_row.push(ch);
                    if depth == 0 {
                        let r_str = current_row.trim();
                        if r_str.starts_with('[') && r_str.ends_with(']') {
                            let content = &r_str[1..r_str.len() - 1];
                            let items: Vec<&str> = content.split(',').collect();
                            let mut row_nodes = Vec::new();
                            for item in items {
                                if !item.trim().is_empty() {
                                    row_nodes.push(SymbolicParser::parse_str(item.trim())?);
                                }
                            }
                            rows.push(row_nodes);
                        }
                        current_row.clear();
                    }
                }
                ',' if depth == 0 => {}
                _ => {
                    if depth > 0 {
                        current_row.push(ch);
                    }
                }
            }
        }

        if rows.is_empty() {
            return Err(PhysureError::Generic("Invalid symbolic matrix format".into()));
        }
        Self::new(rows)
    }

    pub fn transpose(&self) -> Self {
        let mut data = vec![vec![Node::Number(0.0); self.rows]; self.cols];
        for r in 0..self.rows {
            for c in 0..self.cols {
                data[c][r] = self.data[r][c].clone();
            }
        }
        SymMatrix {
            rows: self.cols,
            cols: self.rows,
            data,
        }
    }

    pub fn trace(&self) -> PhysureResult<Node> {
        if self.rows != self.cols {
            return Err(PhysureError::Generic("Trace requires a square matrix".into()));
        }
        let mut diag = Vec::new();
        for i in 0..self.rows {
            diag.push(self.data[i][i].clone());
        }
        Ok(Node::Add(diag).simplify())
    }

    pub fn det(&self) -> PhysureResult<Node> {
        if self.rows != self.cols {
            return Err(PhysureError::Generic("Determinant requires a square matrix".into()));
        }
        self.det_recursive(&self.data)
    }

    fn det_recursive(&self, mat: &[Vec<Node>]) -> PhysureResult<Node> {
        let n = mat.len();
        if n == 1 {
            return Ok(mat[0][0].clone().simplify());
        }
        if n == 2 {
            // ad - bc
            let ad = Node::Mul(vec![mat[0][0].clone(), mat[1][1].clone()]);
            let bc = Node::Mul(vec![mat[0][1].clone(), mat[1][0].clone()]);
            return Ok(Node::Sub(Box::new(ad), Box::new(bc)).simplify());
        }

        // Laplace cofactor expansion along row 0
        let mut terms = Vec::new();
        for col in 0..n {
            let elem = &mat[0][col];
            if matches!(elem.simplify(), Node::Number(val) if val == 0.0) {
                continue;
            }

            // Build submatrix excluding row 0 and column `col`
            let mut submat = Vec::new();
            for r in 1..n {
                let mut subrow = Vec::new();
                for c in 0..n {
                    if c != col {
                        subrow.push(mat[r][c].clone());
                    }
                }
                submat.push(subrow);
            }

            let sub_det = self.det_recursive(&submat)?;
            let term = Node::Mul(vec![elem.clone(), sub_det]).simplify();

            if col % 2 == 0 {
                terms.push(term);
            } else {
                terms.push(Node::Mul(vec![Node::Number(-1.0), term]).simplify());
            }
        }

        if terms.is_empty() {
            Ok(Node::Number(0.0))
        } else if terms.len() == 1 {
            Ok(terms[0].clone())
        } else {
            Ok(Node::Add(terms).simplify())
        }
    }

    pub fn charpoly(&self, lambda_var: &str) -> PhysureResult<Node> {
        if self.rows != self.cols {
            return Err(PhysureError::Generic("Characteristic polynomial requires a square matrix".into()));
        }
        let lambda = Node::Symbol(lambda_var.to_string());
        let mut mat_sub_lambda = self.data.clone();

        for i in 0..self.rows {
            let entry = &self.data[i][i];
            let sub = Node::Sub(Box::new(entry.clone()), Box::new(lambda.clone())).simplify();
            mat_sub_lambda[i][i] = sub;
        }

        self.det_recursive(&mat_sub_lambda)
    }

    pub fn eigenvalues(&self, lambda_var: &str) -> PhysureResult<Vec<Node>> {
        if self.rows != self.cols {
            return Err(PhysureError::Generic("Eigenvalues require a square matrix".into()));
        }
        if self.rows > 2 {
            return Err(PhysureError::Generic("Symbolic eigenvalues are currently supported for matrices up to 2x2".into()));
        }

        let poly = self.charpoly(lambda_var)?;
        // Solve poly = 0 for lambda_var
        let eq = Node::Equation(Box::new(poly), Box::new(Node::Number(0.0)));

        if self.rows == 2 {
            // Quadratic equation for lambda: det(A - lambda I) = lambda^2 - tr(A)*lambda + det(A) = 0
            let tr = self.trace()?;
            let det = self.det()?;
            
            // lambda = (tr ± sqrt(tr^2 - 4*det)) / 2
            let tr_sq = Node::Pow(Box::new(tr.clone()), Box::new(Node::Number(2.0)));
            let four_det = Node::Mul(vec![Node::Number(4.0), det]).simplify();
            let disc = Node::Sub(Box::new(tr_sq), Box::new(four_det)).simplify();
            let sqrt_disc = Node::Sqrt(Box::new(disc));

            let e1 = Node::Div(
                Box::new(Node::Add(vec![tr.clone(), sqrt_disc.clone()])),
                Box::new(Node::Number(2.0)),
            ).simplify();

            let e2 = Node::Div(
                Box::new(Node::Sub(Box::new(tr), Box::new(sqrt_disc))),
                Box::new(Node::Number(2.0)),
            ).simplify();

            Ok(vec![e1, e2])
        } else {
            let sol = eq.solve_equation(lambda_var)?;
            Ok(vec![sol])
        }
    }

    pub fn to_phs_string(&self) -> String {
        let rows_str: Vec<String> = self
            .data
            .iter()
            .map(|r| {
                let elems: Vec<String> = r.iter().map(|e| e.to_phs_string()).collect();
                format!("[{}]", elems.join(", "))
            })
            .collect();
        format!("[{}]", rows_str.join(", "))
    }
}
