use physure_core::error::{PhysureError, PhysureResult};
use crate::value::PhsValue;
use crate::interpreter::PhsInterpreter;
use crate::ast::BinaryOp;

fn eval_bin_op(op: BinaryOp, l: &PhsValue, r: &PhsValue) -> PhysureResult<PhsValue> {
    let interp = PhsInterpreter::default();
    interp.eval_binary_op_vals(op, l.clone(), r.clone())
}


pub(crate) fn eval_array_builtin(name: &str, args: &[PhsValue], interpreter: &PhsInterpreter) -> PhysureResult<Option<PhsValue>> {
    match name {
        "linspace" => {
            if args.len() < 2 {
                return Err(PhysureError::Generic("linspace expects start and stop".into()));
            }
            let start = match &args[0] {
                PhsValue::Number(n) => *n,
                PhsValue::Quantity(q) => q.value.mean(),
                _ => 0.0,
            };
            let stop = match &args[1] {
                PhsValue::Number(n) => *n,
                PhsValue::Quantity(q) => q.value.mean(),
                _ => 1.0,
            };
            let count = if args.len() >= 3 {
                match &args[2] {
                    PhsValue::Number(n) => *n as usize,
                    _ => 50,
                }
            } else {
                50
            };
            let unit = match &args[0] {
                PhsValue::Quantity(q) => Some(q.unit.clone()),
                _ => match &args[1] {
                    PhsValue::Quantity(q) => Some(q.unit.clone()),
                    _ => None,
                },
            };
            // `count` is a caller-supplied argument driving `.collect()` below directly --
            // the identical unbounded-materialization shape as the for-expression's Range
            // branch and `parallel_map` (see `physure_core::settings::max_loop_iterations`'s
            // doc comment). Checked before the `.collect()` so an oversized `count` never
            // allocates.
            let ceiling = physure_core::settings::max_loop_iterations();
            if count > ceiling {
                return Err(PhysureError::Generic(format!(
                    "linspace received count={count}, exceeding the max_loop_iterations ceiling of {ceiling}; raise `max_loop_iterations` in physure.conf's [Settings] section if this is a legitimate workload"
                )));
            }
            let step = if count > 1 { (stop - start) / (count - 1) as f64 } else { 0.0 };
            let vec: Vec<PhsValue> = (0..count)
                .map(|i| {
                    let val = start + i as f64 * step;
                    if let Some(ref u) = unit {
                        use physure_core::quantity::Quantity;
                        PhsValue::Quantity(Quantity::new_scalar(val, 0.0, u.clone(), None, None))
                    } else {
                        PhsValue::Number(val)
                    }
                })
                .collect();
            Ok(Some(PhsValue::Vector(vec)))
        }
        "gradient" => {
            if args.len() != 2 {
                return Err(PhysureError::Generic("gradient expects y and x vectors".into()));
            }
            let y_vec = match &args[0] {
                PhsValue::Vector(v) => v,
                _ => return Err(PhysureError::Generic("gradient expects y vector".into())),
            };
            let x_vec = match &args[1] {
                PhsValue::Vector(v) => v,
                _ => return Err(PhysureError::Generic("gradient expects x vector".into())),
            };
            if y_vec.len() != x_vec.len() || y_vec.len() < 2 {
                return Err(PhysureError::Generic("gradient expects equal length vectors with at least 2 elements".into()));
            }
            let mut result = Vec::new();
            for i in 0..y_vec.len() {
                let (i_prev, i_next) = if i == 0 {
                    (0, 1)
                } else if i == y_vec.len() - 1 {
                    (i - 1, i)
                } else {
                    (i - 1, i + 1)
                };
                let dy = eval_bin_op(BinaryOp::Sub, &y_vec[i_next], &y_vec[i_prev])?;
                let dx = eval_bin_op(BinaryOp::Sub, &x_vec[i_next], &x_vec[i_prev])?;
                let grad = eval_bin_op(BinaryOp::Div, &dy, &dx)?;
                result.push(grad);
            }
            Ok(Some(PhsValue::Vector(result)))
        }
        "trapz" => {
            if args.len() != 2 {
                return Err(PhysureError::Generic("trapz expects y and x vectors".into()));
            }
            let y_vec = match &args[0] {
                PhsValue::Vector(v) => v,
                _ => return Err(PhysureError::Generic("trapz expects y vector".into())),
            };
            let x_vec = match &args[1] {
                PhsValue::Vector(v) => v,
                _ => return Err(PhysureError::Generic("trapz expects x vector".into())),
            };
            if y_vec.len() != x_vec.len() || y_vec.len() < 2 {
                return Err(PhysureError::Generic("trapz expects equal length vectors with at least 2 elements".into()));
            }
            let mut total = PhsValue::None;
            let mut is_first = true;
            let two = PhsValue::Number(2.0);
            for i in 0..y_vec.len() - 1 {
                let dx = eval_bin_op(BinaryOp::Sub, &x_vec[i+1], &x_vec[i])?;
                let sum_y = eval_bin_op(BinaryOp::Add, &y_vec[i+1], &y_vec[i])?;
                let avg_y = eval_bin_op(BinaryOp::Div, &sum_y, &two)?;
                let area = eval_bin_op(BinaryOp::Mul, &avg_y, &dx)?;
                if is_first {
                    total = area;
                    is_first = false;
                } else {
                    total = eval_bin_op(BinaryOp::Add, &total, &area)?;
                }
            }
            Ok(Some(total))
        }
        "dot" => {
            if args.len() != 2 {
                return Err(PhysureError::Generic("dot expects 2 vectors".into()));
            }
            let (v1, v2) = match (&args[0], &args[1]) {
                (PhsValue::Vector(v1), PhsValue::Vector(v2)) => (v1, v2),
                _ => return Err(PhysureError::Generic("dot expects 2 vectors".into())),
            };
            if v1.len() != v2.len() || v1.is_empty() {
                return Err(PhysureError::Generic("dot expects equal non-empty vector lengths".into()));
            }
            let mut sum = interpreter.eval_binary_op_vals(BinaryOp::Mul, v1[0].clone(), v2[0].clone())?;
            for i in 1..v1.len() {
                let prod = interpreter.eval_binary_op_vals(BinaryOp::Mul, v1[i].clone(), v2[i].clone())?;
                sum = interpreter.eval_binary_op_vals(BinaryOp::Add, sum, prod)?;
            }
            Ok(Some(sum))
        }
        "cross" => {
            if args.len() != 2 {
                return Err(PhysureError::Generic("cross expects 2 3D vectors".into()));
            }
            let (v1, v2) = match (&args[0], &args[1]) {
                (PhsValue::Vector(v1), PhsValue::Vector(v2)) => (v1, v2),
                _ => return Err(PhysureError::Generic("cross expects 2 3D vectors".into())),
            };
            if v1.len() != 3 || v2.len() != 3 {
                return Err(PhysureError::Generic("cross requires 3D vectors".into()));
            }
            let c1 = interpreter.eval_binary_op_vals(BinaryOp::Sub,
                interpreter.eval_binary_op_vals(BinaryOp::Mul, v1[1].clone(), v2[2].clone())?,
                interpreter.eval_binary_op_vals(BinaryOp::Mul, v1[2].clone(), v2[1].clone())?,
            )?;
            let c2 = interpreter.eval_binary_op_vals(BinaryOp::Sub,
                interpreter.eval_binary_op_vals(BinaryOp::Mul, v1[2].clone(), v2[0].clone())?,
                interpreter.eval_binary_op_vals(BinaryOp::Mul, v1[0].clone(), v2[2].clone())?,
            )?;
            let c3 = interpreter.eval_binary_op_vals(BinaryOp::Sub,
                interpreter.eval_binary_op_vals(BinaryOp::Mul, v1[0].clone(), v2[1].clone())?,
                interpreter.eval_binary_op_vals(BinaryOp::Mul, v1[1].clone(), v2[0].clone())?,
            )?;
            Ok(Some(PhsValue::Vector(vec![c1, c2, c3])))
        }
        "norm" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("norm expects 1 vector".into()));
            }
            let v = match &args[0] {
                PhsValue::Vector(v) => v,
                _ => return Err(PhysureError::Generic("norm expects vector".into())),
            };
            if v.is_empty() {
                return Err(PhysureError::Generic("norm expects non-empty vector".into()));
            }
            let mut sum = interpreter.eval_binary_op_vals(BinaryOp::Mul, v[0].clone(), v[0].clone())?;
            for i in 1..v.len() {
                let prod = interpreter.eval_binary_op_vals(BinaryOp::Mul, v[i].clone(), v[i].clone())?;
                sum = interpreter.eval_binary_op_vals(BinaryOp::Add, sum, prod)?;
            }
            let half = PhsValue::Number(0.5);
            let res = interpreter.eval_binary_op_vals(BinaryOp::Pow, sum, half)?;
            Ok(Some(res))
        }
        "transpose" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("transpose expects 1 matrix vector".into()));
            }
            let rows = match &args[0] {
                PhsValue::Vector(r) => r,
                _ => return Err(PhysureError::Generic("transpose expects 2D vector matrix".into())),
            };
            let mut matrix = Vec::new();
            for r in rows {
                match r {
                    PhsValue::Vector(cols) => matrix.push(cols.clone()),
                    _ => return Err(PhysureError::Generic("transpose expects 2D vector matrix".into())),
                }
            }
            if matrix.is_empty() {
                return Ok(Some(PhsValue::Vector(Vec::new())));
            }
            let num_rows = matrix.len();
            let num_cols = matrix[0].len();
            let mut transposed = vec![vec![PhsValue::None; num_rows]; num_cols];
            for r in 0..num_rows {
                for c in 0..num_cols {
                    transposed[c][r] = matrix[r][c].clone();
                }
            }
            let res_rows = transposed.into_iter().map(PhsValue::Vector).collect();
            Ok(Some(PhsValue::Vector(res_rows)))
        }
        "matmul" => {
            if args.len() != 2 {
                return Err(PhysureError::Generic("matmul expects 2 matrices".into()));
            }
            let extract_matrix = |v: &PhsValue| -> PhysureResult<Vec<Vec<PhsValue>>> {
                let rows = match v {
                    PhsValue::Vector(r) => r,
                    _ => return Err(PhysureError::Generic("matmul expects 2D vector matrix".into())),
                };
                let mut mat = Vec::new();
                for r in rows {
                    match r {
                        PhsValue::Vector(cols) => mat.push(cols.clone()),
                        _ => return Err(PhysureError::Generic("matmul expects 2D vector matrix".into())),
                    }
                }
                Ok(mat)
            };
            let m1 = extract_matrix(&args[0])?;
            let m2 = extract_matrix(&args[1])?;
            if m1.is_empty() || m2.is_empty() || m1[0].len() != m2.len() {
                return Err(PhysureError::Generic("Matrix multiplication dimension mismatch".into()));
            }
            let r1 = m1.len();
            let c1 = m1[0].len();
            let c2 = m2[0].len();
            let mut res_mat = Vec::with_capacity(r1);
            for r in 0..r1 {
                let mut row = Vec::with_capacity(c2);
                for c in 0..c2 {
                    let mut sum = interpreter.eval_binary_op_vals(BinaryOp::Mul, m1[r][0].clone(), m2[0][c].clone())?;
                    for k in 1..c1 {
                        let prod = interpreter.eval_binary_op_vals(BinaryOp::Mul, m1[r][k].clone(), m2[k][c].clone())?;
                        sum = interpreter.eval_binary_op_vals(BinaryOp::Add, sum, prod)?;
                    }
                    row.push(sum);
                }
                res_mat.push(PhsValue::Vector(row));
            }
            Ok(Some(PhsValue::Vector(res_mat)))
        }
        "det" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("det expects 1 square matrix".into()));
            }
            let rows = match &args[0] {
                PhsValue::Vector(r) => r,
                _ => return Err(PhysureError::Generic("det expects 2D vector matrix".into())),
            };
            let mut mat = Vec::new();
            for r in rows {
                match r {
                    PhsValue::Vector(cols) => mat.push(cols.clone()),
                    _ => return Err(PhysureError::Generic("det expects 2D vector matrix".into())),
                }
            }
            if mat.len() == 2 && mat[0].len() == 2 && mat[1].len() == 2 {
                let ad = interpreter.eval_binary_op_vals(BinaryOp::Mul, mat[0][0].clone(), mat[1][1].clone())?;
                let bc = interpreter.eval_binary_op_vals(BinaryOp::Mul, mat[0][1].clone(), mat[1][0].clone())?;
                let det = interpreter.eval_binary_op_vals(BinaryOp::Sub, ad, bc)?;
                return Ok(Some(det));
            }
            Err(PhysureError::Generic("det currently supports 2x2 matrices".into()))
        }
        _ => Ok(None),
    }
}

