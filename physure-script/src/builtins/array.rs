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
        "linspace" => linspace(args),
        "gradient" => gradient(args),
        "trapz" => trapz(args),
        "dot" => dot(args, interpreter),
        "cross" => cross(args, interpreter),
        "norm" => norm(args, interpreter),
        "transpose" => transpose(args),
        "matmul" => matmul(args, interpreter),
        "det" => det(args, interpreter),
        _ => Ok(None),
    }
}

fn generic_error(message: &str) -> PhysureError {
    PhysureError::Generic(message.into())
}

fn expect_arg_count(args: &[PhsValue], count: usize, message: &str) -> PhysureResult<()> {
    if args.len() == count {
        Ok(())
    } else {
        Err(generic_error(message))
    }
}

fn vector_arg<'a>(value: &'a PhsValue, message: &str) -> PhysureResult<&'a Vec<PhsValue>> {
    match value {
        PhsValue::Vector(v) => Ok(v),
        _ => Err(generic_error(message)),
    }
}

/// A vector of vectors as its rows.
fn matrix_arg(value: &PhsValue, message: &str) -> PhysureResult<Vec<Vec<PhsValue>>> {
    vector_arg(value, message)?
        .iter()
        .map(|row| vector_arg(row, message).cloned())
        .collect()
}

/// The `y` and `x` vectors of `gradient` and `trapz`: the same length, at least two points.
fn y_and_x<'a>(name: &str, args: &'a [PhsValue]) -> PhysureResult<(&'a Vec<PhsValue>, &'a Vec<PhsValue>)> {
    expect_arg_count(args, 2, &format!("{name} expects y and x vectors"))?;
    let y_vec = vector_arg(&args[0], &format!("{name} expects y vector"))?;
    let x_vec = vector_arg(&args[1], &format!("{name} expects x vector"))?;
    if y_vec.len() != x_vec.len() || y_vec.len() < 2 {
        return Err(PhysureError::Generic(format!("{name} expects equal length vectors with at least 2 elements")));
    }
    Ok((y_vec, x_vec))
}

/// `a(0) * b(0) + a(1) * b(1) + ...` over `terms` pairs (at least one), in the units the products carry.
fn sum_of_products(
    interpreter: &PhsInterpreter,
    terms: usize,
    factors: impl Fn(usize) -> (PhsValue, PhsValue),
) -> PhysureResult<PhsValue> {
    let (a, b) = factors(0);
    let mut sum = interpreter.eval_binary_op_vals(BinaryOp::Mul, a, b)?;
    for i in 1..terms {
        let (a, b) = factors(i);
        let prod = interpreter.eval_binary_op_vals(BinaryOp::Mul, a, b)?;
        sum = interpreter.eval_binary_op_vals(BinaryOp::Add, sum, prod)?;
    }
    Ok(sum)
}

fn linspace(args: &[PhsValue]) -> PhysureResult<Option<PhsValue>> {
    if args.len() < 2 {
        return Err(generic_error("linspace expects start and stop"));
    }
    let mean_or = |v: &PhsValue, default: f64| match v {
        PhsValue::Number(n) => *n,
        PhsValue::Quantity(q) => q.value.mean(),
        _ => default,
    };
    let start = mean_or(&args[0], 0.0);
    let stop = mean_or(&args[1], 1.0);
    let count = match args.get(2) {
        Some(PhsValue::Number(n)) => *n as usize,
        _ => 50,
    };
    let unit = args[..2].iter().find_map(|v| match v {
        PhsValue::Quantity(q) => Some(q.unit.clone()),
        _ => None,
    });
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
            match &unit {
                Some(u) => PhsValue::Quantity(physure_core::quantity::Quantity::new_scalar(val, 0.0, u.clone(), None, None)),
                None => PhsValue::Number(val),
            }
        })
        .collect();
    Ok(Some(PhsValue::Vector(vec)))
}

fn gradient(args: &[PhsValue]) -> PhysureResult<Option<PhsValue>> {
    let (y_vec, x_vec) = y_and_x("gradient", args)?;
    let last = y_vec.len() - 1;
    let mut result = Vec::new();
    for i in 0..y_vec.len() {
        // central differences inside, one-sided at the two ends
        let (i_prev, i_next) = (i.saturating_sub(1), (i + 1).min(last));
        let dy = eval_bin_op(BinaryOp::Sub, &y_vec[i_next], &y_vec[i_prev])?;
        let dx = eval_bin_op(BinaryOp::Sub, &x_vec[i_next], &x_vec[i_prev])?;
        result.push(eval_bin_op(BinaryOp::Div, &dy, &dx)?);
    }
    Ok(Some(PhsValue::Vector(result)))
}

fn trapz(args: &[PhsValue]) -> PhysureResult<Option<PhsValue>> {
    let (y_vec, x_vec) = y_and_x("trapz", args)?;
    let two = PhsValue::Number(2.0);
    let mut total: Option<PhsValue> = None;
    for i in 0..y_vec.len() - 1 {
        let dx = eval_bin_op(BinaryOp::Sub, &x_vec[i + 1], &x_vec[i])?;
        let sum_y = eval_bin_op(BinaryOp::Add, &y_vec[i + 1], &y_vec[i])?;
        let avg_y = eval_bin_op(BinaryOp::Div, &sum_y, &two)?;
        let area = eval_bin_op(BinaryOp::Mul, &avg_y, &dx)?;
        total = Some(match total {
            None => area,
            Some(so_far) => eval_bin_op(BinaryOp::Add, &so_far, &area)?,
        });
    }
    Ok(total)
}

fn dot(args: &[PhsValue], interpreter: &PhsInterpreter) -> PhysureResult<Option<PhsValue>> {
    expect_arg_count(args, 2, "dot expects 2 vectors")?;
    let v1 = vector_arg(&args[0], "dot expects 2 vectors")?;
    let v2 = vector_arg(&args[1], "dot expects 2 vectors")?;
    if v1.len() != v2.len() || v1.is_empty() {
        return Err(generic_error("dot expects equal non-empty vector lengths"));
    }
    sum_of_products(interpreter, v1.len(), |i| (v1[i].clone(), v2[i].clone())).map(Some)
}

fn cross(args: &[PhsValue], interpreter: &PhsInterpreter) -> PhysureResult<Option<PhsValue>> {
    expect_arg_count(args, 2, "cross expects 2 3D vectors")?;
    let v1 = vector_arg(&args[0], "cross expects 2 3D vectors")?;
    let v2 = vector_arg(&args[1], "cross expects 2 3D vectors")?;
    if v1.len() != 3 || v2.len() != 3 {
        return Err(generic_error("cross requires 3D vectors"));
    }
    // component `i` is v1[j] * v2[k] - v1[k] * v2[j] for (i, j, k) cycling through (0, 1, 2)
    let component = |j: usize, k: usize| {
        interpreter.eval_binary_op_vals(
            BinaryOp::Sub,
            interpreter.eval_binary_op_vals(BinaryOp::Mul, v1[j].clone(), v2[k].clone())?,
            interpreter.eval_binary_op_vals(BinaryOp::Mul, v1[k].clone(), v2[j].clone())?,
        )
    };
    Ok(Some(PhsValue::Vector(vec![component(1, 2)?, component(2, 0)?, component(0, 1)?])))
}

fn norm(args: &[PhsValue], interpreter: &PhsInterpreter) -> PhysureResult<Option<PhsValue>> {
    expect_arg_count(args, 1, "norm expects 1 vector")?;
    let v = vector_arg(&args[0], "norm expects vector")?;
    if v.is_empty() {
        return Err(generic_error("norm expects non-empty vector"));
    }
    let sum = sum_of_products(interpreter, v.len(), |i| (v[i].clone(), v[i].clone()))?;
    let res = interpreter.eval_binary_op_vals(BinaryOp::Pow, sum, PhsValue::Number(0.5))?;
    Ok(Some(res))
}

fn transpose(args: &[PhsValue]) -> PhysureResult<Option<PhsValue>> {
    expect_arg_count(args, 1, "transpose expects 1 matrix vector")?;
    let matrix = matrix_arg(&args[0], "transpose expects 2D vector matrix")?;
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

fn matmul(args: &[PhsValue], interpreter: &PhsInterpreter) -> PhysureResult<Option<PhsValue>> {
    expect_arg_count(args, 2, "matmul expects 2 matrices")?;
    let m1 = matrix_arg(&args[0], "matmul expects 2D vector matrix")?;
    let m2 = matrix_arg(&args[1], "matmul expects 2D vector matrix")?;
    if m1.is_empty() || m2.is_empty() || m1[0].len() != m2.len() {
        return Err(generic_error("Matrix multiplication dimension mismatch"));
    }
    let inner = m1[0].len();
    let columns = m2[0].len();
    let mut res_mat = Vec::with_capacity(m1.len());
    for row in &m1 {
        let mut res_row = Vec::with_capacity(columns);
        for c in 0..columns {
            res_row.push(sum_of_products(interpreter, inner, |k| (row[k].clone(), m2[k][c].clone()))?);
        }
        res_mat.push(PhsValue::Vector(res_row));
    }
    Ok(Some(PhsValue::Vector(res_mat)))
}

fn det(args: &[PhsValue], interpreter: &PhsInterpreter) -> PhysureResult<Option<PhsValue>> {
    expect_arg_count(args, 1, "det expects 1 square matrix")?;
    let mat = matrix_arg(&args[0], "det expects 2D vector matrix")?;
    if mat.len() == 2 && mat[0].len() == 2 && mat[1].len() == 2 {
        let ad = interpreter.eval_binary_op_vals(BinaryOp::Mul, mat[0][0].clone(), mat[1][1].clone())?;
        let bc = interpreter.eval_binary_op_vals(BinaryOp::Mul, mat[0][1].clone(), mat[1][0].clone())?;
        return interpreter.eval_binary_op_vals(BinaryOp::Sub, ad, bc).map(Some);
    }
    Err(generic_error("det currently supports 2x2 matrices"))
}
