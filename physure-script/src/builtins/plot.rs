use physure_core::error::{PhysureError, PhysureResult};
use physure_core::units::parser::Parser as UnitParser;
use crate::value::PhsValue;
use crate::interpreter::PhsInterpreter;
use super::preprocess_symbolic_expression;

#[cfg(test)]
pub(crate) fn eval_plot_builtin(name: &str, args: &[PhsValue], interpreter: &PhsInterpreter) -> PhysureResult<Option<PhsValue>> {
    let empty_env = std::collections::HashMap::new();
    eval_plot_builtin_with_kwargs(name, args, &[], interpreter, &empty_env)
}

pub(crate) fn eval_plot_builtin_with_kwargs(
    name: &str,
    args: &[PhsValue],
    kwargs: &[(String, PhsValue)],
    interpreter: &PhsInterpreter,
    env: &std::collections::HashMap<String, PhsValue>,
) -> PhysureResult<Option<PhsValue>> {
    match name {
        "plot" => plot_2d(args),
        "plot3d" | "export3d" | "export_3d" => plot_3d(name, args, kwargs, interpreter, env),
        "plot_field" => plot_field(args, interpreter),
        "plot_nd" => plot_nd(args),
        _ => Ok(None),
    }
}

fn plot_value(title: String, x_unit: &str, y_unit: &str, ascii: String, svg: String) -> Option<PhsValue> {
    Some(PhsValue::Plot(crate::value::PlotData {
        title,
        x_unit: x_unit.to_string(),
        y_unit: y_unit.to_string(),
        ascii,
        svg,
    }))
}

fn plot_2d(args: &[PhsValue]) -> PhysureResult<Option<PhsValue>> {
    if args.is_empty() {
        return Err(PhysureError::Generic("plot expects at least 1 argument".into()));
    }
    let title = match args.get(2) {
        Some(PhsValue::String(s)) => s.clone(),
        _ => "Physure Live Plot".to_string(),
    };

    let ((x_arr, x_unit), (y_arr, y_unit)) = if args.len() >= 2 {
        (extract_vec_f64_and_unit(&args[0]), extract_vec_f64_and_unit(&args[1]))
    } else {
        let (y_a, y_u) = extract_vec_f64_and_unit(&args[0]);
        let x_a: Vec<f64> = (0..y_a.len()).map(|i| i as f64).collect();
        ((x_a, String::new()), (y_a, y_u))
    };

    let ascii_plot = draw_ascii_plot(&x_arr, &y_arr, &title, &x_unit, &y_unit);
    let svg_plot = draw_svg_plot(&x_arr, &y_arr, &title, &x_unit, &y_unit);
    Ok(plot_value(title, &x_unit, &y_unit, ascii_plot, svg_plot))
}

/// What a `plot3d` or `export3d` call asks for: the ranges and units of x and y, the title, the file and its format.
struct SurfaceRequest {
    x_min: f64,
    x_max: f64,
    x_unit: String,
    y_min: f64,
    y_max: f64,
    y_unit: String,
    title: String,
    filename: String,
    format_name: String,
}

impl SurfaceRequest {
    fn read(name: &str, args: &[PhsValue], kwargs: &[(String, PhsValue)]) -> Self {
        let mut req = SurfaceRequest {
            x_min: -2.0,
            x_max: 2.0,
            x_unit: "m".to_string(),
            y_min: -2.0,
            y_max: 2.0,
            y_unit: "m".to_string(),
            title: "3D Surface Plot".to_string(),
            filename: "plot_3d.stl".to_string(),
            format_name: "stl".to_string(),
        };
        for (k, v) in kwargs {
            match (k.as_str(), v) {
                ("x" | "x_range", _) => read_range(v, &mut req.x_min, &mut req.x_max, &mut req.x_unit),
                ("y" | "y_range", _) => read_range(v, &mut req.y_min, &mut req.y_max, &mut req.y_unit),
                ("title", PhsValue::String(s)) => req.title = s.clone(),
                ("file" | "filename", PhsValue::String(s)) => req.set_filename(s),
                ("format", PhsValue::String(s)) => req.format_name = s.clone(),
                _ => {}
            }
        }
        if let Some(PhsValue::String(s)) = args.get(1) {
            if name == "plot3d" {
                req.title = s.clone();
            } else {
                req.set_filename(s);
            }
        }
        if let Some(PhsValue::String(s)) = args.get(2) {
            if name != "plot3d" {
                req.format_name = s.clone();
            }
        }
        req
    }

    /// The file to write; its extension, if it has one, is the format.
    fn set_filename(&mut self, filename: &str) {
        self.filename = filename.to_string();
        if filename.contains('.') {
            self.format_name = filename.split('.').last().unwrap_or("stl").to_string();
        }
    }
}

/// A `start .. end` range: its ends, and the unit of the start when it is a quantity.
fn read_range(v: &PhsValue, min: &mut f64, max: &mut f64, unit: &mut String) {
    let PhsValue::Range(start, end) = v else {
        return;
    };
    match start.as_ref() {
        PhsValue::Quantity(q) => {
            *min = q.value.mean();
            *unit = q.unit.__repr__();
        }
        PhsValue::Number(n) => *min = *n,
        _ => {}
    }
    match end.as_ref() {
        PhsValue::Quantity(q) => *max = q.value.mean(),
        PhsValue::Number(n) => *max = *n,
        _ => {}
    }
}

/// `steps` evenly spaced values from `min` to `max`.
fn grid(min: f64, max: f64, steps: usize) -> Vec<f64> {
    (0..steps).map(|i| min + (i as f64) * (max - min) / ((steps - 1) as f64)).collect()
}

/// A height of the surface as a number, noting the unit of the first one that has a unit.
fn height_value(value: PhsValue, z_unit: &mut String) -> f64 {
    match value {
        PhsValue::Quantity(q) => {
            if z_unit.is_empty() {
                *z_unit = q.unit.__repr__();
            }
            q.value.mean()
        }
        PhsValue::Number(n) => n,
        _ => 0.0,
    }
}

/// The heights of a function `fn P(x, y)` over the grid (a row of x for each y), and their unit.
fn sample_function(
    func: &crate::ast::FunctionDefNode,
    x_grid: &[f64],
    y_grid: &[f64],
    x_unit: &str,
    y_unit: &str,
    interpreter: &PhsInterpreter,
    env: &std::collections::HashMap<String, PhsValue>,
) -> PhysureResult<(Vec<f64>, String)> {
    let x_q_unit = UnitParser::parse_expression(x_unit).ok();
    let y_q_unit = UnitParser::parse_expression(y_unit).ok();
    let argument = |value: f64, unit: &Option<physure_core::units::RationalUnit>| match unit {
        Some(u) => PhsValue::Quantity(physure_core::quantity::Quantity::new_scalar(value, 0.0, u.clone(), None, None)),
        None => PhsValue::Number(value),
    };
    let mut z_grid = Vec::with_capacity(x_grid.len() * y_grid.len());
    let mut z_unit = String::new();
    for y in y_grid {
        for x in x_grid {
            let args = vec![argument(*x, &x_q_unit), argument(*y, &y_q_unit)];
            let res = interpreter.call_function_node(func, args, env)?;
            z_grid.push(height_value(res, &mut z_unit));
        }
    }
    Ok((z_grid, z_unit))
}

/// The heights of an expression in `x` and `y` over the grid, and their unit; a point it cannot evaluate at is 0.
fn sample_expression(
    expr_str: &str,
    x_grid: &[f64],
    y_grid: &[f64],
    interpreter: &PhsInterpreter,
    env: &std::collections::HashMap<String, PhsValue>,
) -> PhysureResult<(Vec<f64>, String)> {
    let inlined = preprocess_symbolic_expression(expr_str, interpreter);
    let program = crate::parser::parse_phs(&inlined)?;
    let expr = match program.statements.first() {
        Some(crate::ast::Statement::Expr(e)) => e.clone(),
        Some(crate::ast::Statement::Assignment(node)) => node.value.clone(),
        _ => return Err(PhysureError::Generic("Failed to parse 3D expression".into())),
    };
    let mut z_grid = Vec::with_capacity(x_grid.len() * y_grid.len());
    let mut z_unit = String::new();
    for y in y_grid {
        for x in x_grid {
            let mut local_env = env.clone();
            local_env.insert("x".to_string(), PhsValue::Number(*x));
            local_env.insert("y".to_string(), PhsValue::Number(*y));
            let value = interpreter.eval_expr(&expr, &local_env).unwrap_or(PhsValue::None);
            z_grid.push(height_value(value, &mut z_unit));
        }
    }
    Ok((z_grid, z_unit))
}

fn plot_3d(
    name: &str,
    args: &[PhsValue],
    kwargs: &[(String, PhsValue)],
    interpreter: &PhsInterpreter,
    env: &std::collections::HashMap<String, PhsValue>,
) -> PhysureResult<Option<PhsValue>> {
    let mut req = SurfaceRequest::read(name, args, kwargs);

    let steps = 25;
    let x_grid = grid(req.x_min, req.x_max, steps);
    let y_grid = grid(req.y_min, req.y_max, steps);

    let (z_grid, z_unit) = match args.first() {
        Some(PhsValue::Function(func)) => {
            if req.title == "3D Surface Plot" {
                req.title = format!(
                    "3D Surface: fn {}({}, {})",
                    func.name,
                    func.params.first().cloned().unwrap_or("x".into()),
                    func.params.get(1).cloned().unwrap_or("y".into())
                );
            }
            sample_function(func, &x_grid, &y_grid, &req.x_unit, &req.y_unit, interpreter, env)?
        }
        Some(PhsValue::String(expr_str)) => sample_expression(expr_str, &x_grid, &y_grid, interpreter, env)?,
        _ => {
            return Err(PhysureError::Generic(
                "plot3d/export3d expects a function (e.g. fn P(x, y)) or expression string as first argument".into(),
            ))
        }
    };

    let clean_z = physure_core::plotting::sanitize_unit_label(if z_unit.is_empty() { "units" } else { &z_unit });
    let mesh_data = physure_core::plotting::Mesh3DData::new(
        &req.title,
        &format!("x ({})", req.x_unit),
        &format!("y ({})", req.y_unit),
        &format!("z ({})", clean_z),
        x_grid,
        y_grid,
        z_grid,
        steps,
        steps,
    );

    if name == "plot3d" {
        let html_str = mesh_data.export_html_threejs();
        let ascii_plot = draw_3d_surface_ascii(&req.title, &req.title, interpreter)?;
        return Ok(plot_value(req.title, &req.x_unit, &req.y_unit, ascii_plot, html_str));
    }
    if !interpreter.limits.allow_files {
        return Err(PhysureError::Generic(format!("forbidden: writing '{}' is not allowed here", req.filename)));
    }
    let bytes = mesh_data.export_format(&req.format_name)?;
    std::fs::write(&req.filename, &bytes)
        .map_err(|e| PhysureError::Generic(format!("Failed to write {}: {}", req.filename, e)))?;
    Ok(Some(PhsValue::String(format!("✓ Exported 3D mesh '{}' ({})", req.filename, req.format_name))))
}

fn plot_field(args: &[PhsValue], interpreter: &PhsInterpreter) -> PhysureResult<Option<PhsValue>> {
    let u_expr = match args.first() {
        Some(PhsValue::String(s)) => s.as_str(),
        _ => return Err(PhysureError::Generic("plot_field expects u(x, y) expression string".into())),
    };
    let v_expr = match args.get(1) {
        Some(PhsValue::String(s)) => s.as_str(),
        _ => return Err(PhysureError::Generic("plot_field expects v(x, y) expression string".into())),
    };
    let title = match args.get(2) {
        Some(PhsValue::String(s)) => s.clone(),
        _ => format!("Vector Field Plot: F = ({}, {})", u_expr, v_expr),
    };

    let svg_plot = draw_vector_field_svg(u_expr, v_expr, &title, interpreter)?;
    let ascii_plot = draw_vector_field_ascii(u_expr, v_expr, &title, interpreter)?;
    Ok(plot_value(title, "x", "y", ascii_plot, svg_plot))
}

fn plot_nd(args: &[PhsValue]) -> PhysureResult<Option<PhsValue>> {
    let Some(data) = args.first() else {
        return Err(PhysureError::Generic("plot_nd expects a 2D matrix of data points".into()));
    };
    let title = match args.get(1) {
        Some(PhsValue::String(s)) => s.clone(),
        _ => "N-Dimensional Parallel Coordinates Plot".to_string(),
    };

    let (svg_plot, ascii_plot) = draw_nd_parallel_coords_svg(data, &title)?;
    Ok(plot_value(title, "dim", "val", ascii_plot, svg_plot))
}

fn extract_vec_f64_and_unit(val: &PhsValue) -> (Vec<f64>, String) {
    match val {
        PhsValue::Number(n) => (vec![*n], String::new()),
        PhsValue::Quantity(q) => (vec![q.value.mean()], q.unit.__repr__()),
        PhsValue::Vector(vec) => {
            let mut nums = Vec::new();
            let mut unit_str = String::new();
            for item in vec {
                match item {
                    PhsValue::Number(n) => nums.push(*n),
                    PhsValue::Quantity(q) => {
                        nums.push(q.value.mean());
                        if unit_str.is_empty() {
                            unit_str = q.unit.__repr__();
                        }
                    }
                    _ => {}
                }
            }
            (nums, unit_str)
        }
        _ => (Vec::new(), String::new()),
    }
}

/// The curve's height at `x_val`, by linear interpolation between the neighbouring points (the end values
/// outside the data). `pairs` is sorted by x and not empty.
fn linear_interpolation(pairs: &[(f64, f64)], x_val: f64) -> f64 {
    let first = pairs[0];
    let last = pairs[pairs.len() - 1];
    if pairs.len() == 1 || x_val <= first.0 {
        return first.1;
    }
    if x_val >= last.0 {
        return last.1;
    }
    for window in pairs.windows(2) {
        let (left, right) = (window[0], window[1]);
        if x_val >= left.0 && x_val <= right.0 {
            let dx = right.0 - left.0;
            return if dx.abs() > 1e-12 { left.1 + (x_val - left.0) / dx * (right.1 - left.1) } else { left.1 };
        }
    }
    first.1
}

/// One line of the ASCII plot: a full block where the curve passes `y_level`, a light one below the curve.
fn ascii_row(y_grid: &[f64], y_level: f64, y_span: f64, height: usize) -> String {
    y_grid
        .iter()
        .map(|&val| {
            let diff = (val - y_level).abs() / y_span;
            if diff < (1.0 / (2.0 * height as f64)) {
                '█'
            } else if val > y_level {
                '░'
            } else {
                ' '
            }
        })
        .collect()
}

fn draw_ascii_plot(x: &[f64], y: &[f64], title: &str, x_unit: &str, y_unit: &str) -> String {
    if x.is_empty() || y.is_empty() {
        return format!("📊 {}: [No data points]", title);
    }
    let n = x.len().min(y.len());
    let mut pairs: Vec<(f64, f64)> = x[..n].iter().zip(y[..n].iter()).map(|(&a, &b)| (a, b)).collect();
    pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let x_min = pairs[0].0;
    let x_max = pairs.last().unwrap().0;

    let width = 46;
    let height = 8;

    let y_grid: Vec<f64> = (0..width)
        .map(|c| linear_interpolation(&pairs, x_min + (c as f64) * (x_max - x_min) / ((width - 1) as f64)))
        .collect();

    let y_min = y_grid.iter().cloned().fold(f64::INFINITY, f64::min);
    let y_max = y_grid.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let y_span = if y_max != y_min { y_max - y_min } else { 1.0 };

    let fmt_x = if x_unit.is_empty() { String::new() } else { format!(" {}", x_unit) };
    let fmt_y = if y_unit.is_empty() { String::new() } else { format!(" {}", y_unit) };

    let mut lines = Vec::new();
    lines.push(format!("  📊 {}", title));

    let top_y_str = format!("  {:.*e}{}", 3, y_max, fmt_y);
    lines.push(format!("{:>18} ┐", top_y_str.trim()));

    for r in (0..height).rev() {
        let y_level = y_min + (r as f64 / (height - 1) as f64) * y_span;
        lines.push(format!("                   │ {}", ascii_row(&y_grid, y_level, y_span, height)));
    }

    let bot_y_str = format!("  {:.*e}{}", 3, y_min, fmt_y);
    lines.push(format!("{:>18} └{}", bot_y_str.trim(), "─".repeat(width)));

    let x_min_str = format!("{:.*e}{}", 3, x_min, fmt_x);
    let x_max_str = format!("{:.*e}{}", 3, x_max, fmt_x);
    let x_min_trim = x_min_str.trim();
    let x_max_trim = x_max_str.trim();
    let pad_len = (width + 12).saturating_sub(x_min_trim.len() + x_max_trim.len()).max(1);
    lines.push(format!("                     {}{}{}", x_min_trim, " ".repeat(pad_len), x_max_trim));

    lines.join("\n")
}

fn draw_svg_plot(x: &[f64], y: &[f64], title: &str, x_unit: &str, y_unit: &str) -> String {
    if x.is_empty() || y.is_empty() {
        return String::new();
    }
    let n = x.len().min(y.len());
    let mut pairs: Vec<(f64, f64)> = x[..n].iter().zip(y[..n].iter()).map(|(&a, &b)| (a, b)).collect();
    pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let x_min = pairs[0].0;
    let x_max = pairs.last().unwrap().0;

    let mut y_min = f64::INFINITY;
    let mut y_max = f64::NEG_INFINITY;
    for &(_, val) in &pairs {
        if val < y_min { y_min = val; }
        if val > y_max { y_max = val; }
    }
    let y_span = if y_max != y_min { y_max - y_min } else { 1.0 };
    let x_span = if x_max != x_min { x_max - x_min } else { 1.0 };

    let width = 600.0;
    let height = 350.0;
    let padding_left = 80.0;
    let padding_bottom = 50.0;
    let padding_top = 40.0;
    let padding_right = 30.0;

    let plot_w = width - padding_left - padding_right;
    let plot_h = height - padding_top - padding_bottom;

    let points: Vec<String> = pairs.iter().map(|&(px, py)| {
        let sx = padding_left + ((px - x_min) / x_span) * plot_w;
        let sy = padding_top + (1.0 - (py - y_min) / y_span) * plot_h;
        format!("{:.1},{:.1}", sx, sy)
    }).collect();

    let points_str = points.join(" ");

    let fill_first = format!("{:.1},{:.1}", padding_left, padding_top + plot_h);
    let fill_last = format!("{:.1},{:.1}", padding_left + plot_w, padding_top + plot_h);
    let fill_points = format!("{} {} {}", fill_first, points_str, fill_last);

    let x_label = if x_unit.is_empty() { "x".to_string() } else { format!("x ({})", x_unit) };
    let y_label = if y_unit.is_empty() { "y".to_string() } else { format!("y ({})", y_unit) };

    format!(
        r###"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}" style="background-color:#1e1e1e; font-family:sans-serif;"><rect width="100%" height="100%" fill="#1e1e1e"/><text x="{title_x}" y="25" fill="#569cd6" font-size="14" font-weight="bold" text-anchor="middle">{title}</text><rect x="{pl}" y="{pt}" width="{pw}" height="{ph}" fill="#252526" stroke="#444444" stroke-width="1"/><polygon points="{fill_points}" fill="#4ec9b0" fill-opacity="0.15"/><polyline points="{points_str}" fill="none" stroke="#4ec9b0" stroke-width="2.5" stroke-linecap="round"/><text x="{pl}" y="{y_max_y}" fill="#cccccc" font-size="10" text-anchor="end" dx="-8">{y_max:.3e}</text><text x="{pl}" y="{y_min_y}" fill="#cccccc" font-size="10" text-anchor="end" dx="-8">{y_min:.3e}</text><text x="{pl}" y="{x_min_y}" fill="#cccccc" font-size="10" text-anchor="middle">{x_min:.3e}</text><text x="{x_max_x}" y="{x_min_y}" fill="#cccccc" font-size="10" text-anchor="middle">{x_max:.3e}</text><text x="{title_x}" y="{x_lbl_y}" fill="#cccccc" font-size="11" text-anchor="middle">{x_label}</text><text x="15" y="{y_lbl_y}" fill="#cccccc" font-size="11" text-anchor="middle" transform="rotate(-90 15 {y_lbl_y})">{y_label}</text></svg>"###,
        w = width, h = height,
        title_x = width / 2.0,
        title = title,
        pl = padding_left, pt = padding_top, pw = plot_w, ph = plot_h,
        fill_points = fill_points,
        points_str = points_str,
        y_max_y = padding_top + 12.0,
        y_min_y = padding_top + plot_h,
        x_min_y = padding_top + plot_h + 20.0,
        x_max_x = padding_left + plot_w,
        x_lbl_y = padding_top + plot_h + 38.0,
        y_lbl_y = padding_top + plot_h / 2.0,
        x_min = x_min, x_max = x_max, y_min = y_min, y_max = y_max,
        x_label = x_label, y_label = y_label
    )
}



fn draw_3d_surface_ascii(_expr_str: &str, title: &str, _interpreter: &PhsInterpreter) -> PhysureResult<String> {
    Ok(format!("🏔️ {} [3D Surface View]", title))
}

fn draw_vector_field_svg(u_expr: &str, v_expr: &str, title: &str, interpreter: &PhsInterpreter) -> PhysureResult<String> {
    let parse_expr = |s: &str| -> PhysureResult<crate::ast::Expr> {
        let inlined = preprocess_symbolic_expression(s, interpreter);
        let program = crate::parser::parse_phs(&inlined)?;
        match program.statements.first() {
            Some(crate::ast::Statement::Expr(e)) => Ok(e.clone()),
            Some(crate::ast::Statement::Assignment(node)) => Ok(node.value.clone()),
            _ => Err(PhysureError::Generic("Failed to parse field expression".into())),
        }
    };
    let u_ast = parse_expr(u_expr)?;
    let v_ast = parse_expr(v_expr)?;

    let grid_size = 12;
    let plot_w = 480.0;
    let plot_h = 300.0;
    let padding_left = 60.0;
    let padding_top = 50.0;

    let mut arrows = Vec::new();

    for i in 0..grid_size {
        let gx = -2.0 + (i as f64) * 4.0 / ((grid_size - 1) as f64);
        let sx = padding_left + (i as f64) * plot_w / ((grid_size - 1) as f64);
        for j in 0..grid_size {
            let gy = -2.0 + (j as f64) * 4.0 / ((grid_size - 1) as f64);
            let sy = padding_top + (1.0 - (j as f64) / ((grid_size - 1) as f64)) * plot_h;

            let mut env = interpreter.env.clone();
            env.insert("x".to_string(), PhsValue::Number(gx));
            env.insert("y".to_string(), PhsValue::Number(gy));

            let u = match interpreter.eval_expr(&u_ast, &env) { Ok(PhsValue::Number(n)) => n, Ok(PhsValue::Quantity(q)) => q.value.mean(), _ => 0.0 };
            let v = match interpreter.eval_expr(&v_ast, &env) { Ok(PhsValue::Number(n)) => n, Ok(PhsValue::Quantity(q)) => q.value.mean(), _ => 0.0 };

            let len = (u * u + v * v).sqrt();
            let scale = if len > 1e-6 { (15.0 / len).min(18.0) } else { 0.0 };
            let ex = sx + u * scale;
            let ey = sy - v * scale;

            let hue = (240.0 - (len / 5.0).min(1.0) * 240.0).clamp(0.0, 240.0);

            arrows.push(format!(
                "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"hsl({:.0},80%,60%)\" stroke-width=\"2\" marker-end=\"url(#arrow)\"/>",
                sx, sy, ex, ey, hue
            ));
        }
    }

    Ok(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 600 400\" width=\"100%\" height=\"100%\"><defs><marker id=\"arrow\" viewBox=\"0 0 10 10\" refX=\"5\" refY=\"5\" markerWidth=\"6\" markerHeight=\"6\" orient=\"auto-start-reverse\"><path d=\"M 0 0 L 10 5 L 0 10 z\" fill=\"#58a6ff\"/></marker></defs><rect width=\"100%\" height=\"100%\" fill=\"#0d1117\"/><text x=\"300\" y=\"30\" text-anchor=\"middle\" fill=\"#58a6ff\" font-family=\"sans-serif\" font-size=\"16\" font-weight=\"bold\">{}</text>{}</svg>",
        title, arrows.join("\n")
    ))
}

fn draw_vector_field_ascii(_u_expr: &str, _v_expr: &str, title: &str, _interpreter: &PhsInterpreter) -> PhysureResult<String> {
    Ok(format!("↗️ {} [Vector Field View]", title))
}

fn draw_nd_parallel_coords_svg(val: &PhsValue, title: &str) -> PhysureResult<(String, String)> {
    let extract_rows = match val {
        PhsValue::Vector(rows) => rows,
        _ => return Err(PhysureError::Generic("plot_nd expects a 2D matrix of data points".into())),
    };
    let mut matrix: Vec<Vec<f64>> = Vec::new();
    for r in extract_rows {
        match r {
            PhsValue::Vector(cols) => {
                let row_nums: Vec<f64> = cols.iter().map(|item| match item {
                    PhsValue::Number(n) => *n,
                    PhsValue::Quantity(q) => q.value.mean(),
                    _ => 0.0,
                }).collect();
                matrix.push(row_nums);
            }
            _ => {}
        }
    }
    if matrix.is_empty() {
        return Err(PhysureError::Generic("plot_nd matrix is empty".into()));
    }
    let num_dims = matrix[0].len();
    let num_samples = matrix.len();

    let width = 600.0;
    let height = 400.0;
    let padding_left = 60.0;
    let padding_right = 40.0;
    let padding_top = 60.0;
    let padding_bottom = 50.0;
    let plot_w = width - padding_left - padding_right;
    let plot_h = height - padding_top - padding_bottom;

    let mut svg_lines = Vec::new();
    for row_idx in 0..num_samples {
        let mut path_pts = Vec::new();
        for dim in 0..num_dims {
            let sx = padding_left + (dim as f64) * plot_w / ((num_dims - 1).max(1) as f64);
            let val = matrix[row_idx][dim];
            let sy = padding_top + (1.0 - (val / 10.0).clamp(-1.0, 1.0) * 0.5 - 0.5) * plot_h;
            path_pts.push(format!("{:.1},{:.1}", sx, sy));
        }
        let hue = (row_idx as f64 * 360.0 / num_samples as f64) % 360.0;
        svg_lines.push(format!(
            "<polyline points=\"{}\" fill=\"none\" stroke=\"hsl({:.0},70%,60%)\" stroke-width=\"2\" opacity=\"0.75\"/>",
            path_pts.join(" "), hue
        ));
    }

    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 600 400\" width=\"100%\" height=\"100%\"><rect width=\"100%\" height=\"100%\" fill=\"#0d1117\"/><text x=\"300\" y=\"35\" text-anchor=\"middle\" fill=\"#58a6ff\" font-family=\"sans-serif\" font-size=\"16\" font-weight=\"bold\">{}</text>{}</svg>",
        title, svg_lines.join("\n")
    );
    let ascii = format!("🌐 {} [N-D Parallel Coordinates: {} dimensions, {} samples]", title, num_dims, num_samples);
    Ok((svg, ascii))
}

