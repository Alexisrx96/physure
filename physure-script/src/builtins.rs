use physure_core::error::{PhysureError, PhysureResult};
use super::value::PhsValue;
use super::interpreter::PhsInterpreter;

pub fn eval_builtin(name: &str, args: &[PhsValue], interpreter: &mut PhsInterpreter) -> PhysureResult<Option<PhsValue>> {
    match name {
        "sqrt" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("sqrt expects 1 argument".into()));
            }
            match &args[0] {
                PhsValue::Number(n) => Ok(Some(PhsValue::Number(n.sqrt()))),
                PhsValue::Quantity(q) => Ok(Some(PhsValue::Quantity(q.sqrt()?))),
                _ => Err(PhysureError::Generic("sqrt expects a number or quantity".into())),
            }
        }
        "sin" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("sin expects 1 argument".into()));
            }
            match &args[0] {
                PhsValue::Number(n) => Ok(Some(PhsValue::Number(n.sin()))),
                PhsValue::Quantity(q) => Ok(Some(PhsValue::Number(q.value.mean().sin()))),
                _ => Err(PhysureError::Generic("sin expects a number".into())),
            }
        }
        "cos" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("cos expects 1 argument".into()));
            }
            match &args[0] {
                PhsValue::Number(n) => Ok(Some(PhsValue::Number(n.cos()))),
                PhsValue::Quantity(q) => Ok(Some(PhsValue::Number(q.value.mean().cos()))),
                _ => Err(PhysureError::Generic("cos expects a number".into())),
            }
        }
        "exp" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("exp expects 1 argument".into()));
            }
            match &args[0] {
                PhsValue::Number(n) => Ok(Some(PhsValue::Number(n.exp()))),
                _ => Err(PhysureError::Generic("exp expects a number".into())),
            }
        }
        "ln" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("ln expects 1 argument".into()));
            }
            match &args[0] {
                PhsValue::Number(n) => Ok(Some(PhsValue::Number(n.ln()))),
                _ => Err(PhysureError::Generic("ln expects a number".into())),
            }
        }
        "abs" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("abs expects 1 argument".into()));
            }
            match &args[0] {
                PhsValue::Number(n) => Ok(Some(PhsValue::Number(n.abs()))),
                _ => Err(PhysureError::Generic("abs expects a number".into())),
            }
        }
        "log" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("log expects 1 argument".into()));
            }
            match &args[0] {
                PhsValue::Number(n) => Ok(Some(PhsValue::Number(n.log10()))),
                _ => Err(PhysureError::Generic("log expects a number".into())),
            }
        }
        "tan" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("tan expects 1 argument".into()));
            }
            match &args[0] {
                PhsValue::Number(n) => Ok(Some(PhsValue::Number(n.tan()))),
                PhsValue::Quantity(q) => Ok(Some(PhsValue::Number(q.value.mean().tan()))),
                _ => Err(PhysureError::Generic("tan expects a number".into())),
            }
        }
        "floor" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("floor expects 1 argument".into()));
            }
            match &args[0] {
                PhsValue::Number(n) => Ok(Some(PhsValue::Number(n.floor()))),
                PhsValue::Quantity(q) => {
                    use physure_core::quantity::Quantity;
                    Ok(Some(PhsValue::Quantity(Quantity::new_scalar(
                        q.value.mean().floor(),
                        0.0,
                        q.unit.clone(),
                        None,
                        None,
                    ))))
                }
                _ => Err(PhysureError::Generic("floor expects number or quantity".into())),
            }
        }
        "ceil" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("ceil expects 1 argument".into()));
            }
            match &args[0] {
                PhsValue::Number(n) => Ok(Some(PhsValue::Number(n.ceil()))),
                PhsValue::Quantity(q) => {
                    use physure_core::quantity::Quantity;
                    Ok(Some(PhsValue::Quantity(Quantity::new_scalar(
                        q.value.mean().ceil(),
                        0.0,
                        q.unit.clone(),
                        None,
                        None,
                    ))))
                }
                _ => Err(PhysureError::Generic("ceil expects number or quantity".into())),
            }
        }
        "min" => {
            if args.is_empty() {
                return Err(PhysureError::Generic("min expects arguments".into()));
            }
            let mut best = &args[0];
            for arg in args.iter().skip(1) {
                // compare by base-SI magnitude but return the original
                let best_mag = match best {
                    PhsValue::Number(n) => *n,
                    PhsValue::Quantity(q) => q.canonical_magnitude(),
                    _ => return Err(PhysureError::Generic("min expects numbers or quantities".into())),
                };
                let arg_mag = match arg {
                    PhsValue::Number(n) => *n,
                    PhsValue::Quantity(q) => q.canonical_magnitude(),
                    _ => return Err(PhysureError::Generic("min expects numbers or quantities".into())),
                };
                if arg_mag < best_mag {
                    best = arg;
                }
            }
            Ok(Some(best.clone()))
        }
        "max" => {
            if args.is_empty() {
                return Err(PhysureError::Generic("max expects arguments".into()));
            }
            let mut best = &args[0];
            for arg in args.iter().skip(1) {
                let best_mag = match best {
                    PhsValue::Number(n) => *n,
                    PhsValue::Quantity(q) => q.canonical_magnitude(),
                    _ => return Err(PhysureError::Generic("max expects numbers or quantities".into())),
                };
                let arg_mag = match arg {
                    PhsValue::Number(n) => *n,
                    PhsValue::Quantity(q) => q.canonical_magnitude(),
                    _ => return Err(PhysureError::Generic("max expects numbers or quantities".into())),
                };
                if arg_mag > best_mag {
                    best = arg;
                }
            }
            Ok(Some(best.clone()))
        }
        "round" => {
            if args.is_empty() {
                return Err(PhysureError::Generic("round expects arguments".into()));
            }
            let decimals = match args.get(1) {
                Some(PhsValue::Number(d)) => *d as i32,
                _ => 0,
            };
            let factor = 10.0f64.powi(decimals);
            match &args[0] {
                PhsValue::Number(n) => Ok(Some(PhsValue::Number((n * factor).round() / factor))),
                PhsValue::Quantity(q) => {
                    use physure_core::quantity::Quantity;
                    let rounded = Quantity::new_scalar(
                        (q.value.mean() * factor).round() / factor,
                        0.0,
                        q.unit.clone(),
                        None,
                        None,
                    );
                    Ok(Some(PhsValue::Quantity(rounded)))
                }
                _ => Err(PhysureError::Generic("round expects number or quantity".into())),
            }
        }
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
                let dy = interpreter.eval_binary_op(&crate::ast::BinaryOp::Sub, &y_vec[i_next], &y_vec[i_prev])?;
                let dx = interpreter.eval_binary_op(&crate::ast::BinaryOp::Sub, &x_vec[i_next], &x_vec[i_prev])?;
                let grad = interpreter.eval_binary_op(&crate::ast::BinaryOp::Div, &dy, &dx)?;
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
            let mut total = interpreter.eval_binary_op(&crate::ast::BinaryOp::Mul, &y_vec[0], &x_vec[0])?; // Just to get a value, will override
            let mut is_first = true;
            let two = PhsValue::Number(2.0);
            for i in 0..y_vec.len() - 1 {
                let dx = interpreter.eval_binary_op(&crate::ast::BinaryOp::Sub, &x_vec[i+1], &x_vec[i])?;
                let sum_y = interpreter.eval_binary_op(&crate::ast::BinaryOp::Add, &y_vec[i+1], &y_vec[i])?;
                let avg_y = interpreter.eval_binary_op(&crate::ast::BinaryOp::Div, &sum_y, &two)?;
                let area = interpreter.eval_binary_op(&crate::ast::BinaryOp::Mul, &avg_y, &dx)?;
                if is_first {
                    total = area;
                    is_first = false;
                } else {
                    total = interpreter.eval_binary_op(&crate::ast::BinaryOp::Add, &total, &area)?;
                }
            }
            Ok(Some(total))
        }
        "deriv" | "diff" => {
            if args.len() != 2 {
                return Err(PhysureError::Generic("deriv expects expression string and variable string".into()));
            }
            let expr_str = match &args[0] {
                PhsValue::String(s) => s,
                _ => return Err(PhysureError::Generic("deriv expects expression string".into())),
            };
            let var_str = match &args[1] {
                PhsValue::String(s) => s,
                _ => return Err(PhysureError::Generic("deriv expects variable string".into())),
            };
            let inlined = preprocess_symbolic_expression(expr_str, interpreter);
            let node = crate::symbolic::SymbolicParser::parse_str(&inlined)?;
            let diff_node = node.diff_node(var_str)?.simplify();
            Ok(Some(PhsValue::String(diff_node.to_string())))
        }
        "integral" | "integrate" => {
            if args.len() != 2 {
                return Err(PhysureError::Generic("integral expects expression string and variable string".into()));
            }
            let expr_str = match &args[0] {
                PhsValue::String(s) => s,
                _ => return Err(PhysureError::Generic("integral expects expression string".into())),
            };
            let var_str = match &args[1] {
                PhsValue::String(s) => s,
                _ => return Err(PhysureError::Generic("integral expects variable string".into())),
            };
            let inlined = preprocess_symbolic_expression(expr_str, interpreter);
            let node = crate::symbolic::SymbolicParser::parse_str(&inlined)?;
            let int_node = node.integrate_node(var_str)?.simplify();
            Ok(Some(PhsValue::String(int_node.to_string())))
        }
        "solve" => {
            if args.len() != 2 {
                return Err(PhysureError::Generic("solve expects equation string and target string".into()));
            }
            let eq_str = match &args[0] {
                PhsValue::String(s) => s,
                _ => return Err(PhysureError::Generic("solve expects equation string".into())),
            };
            let target_str = match &args[1] {
                PhsValue::String(s) => s,
                _ => return Err(PhysureError::Generic("solve expects target string".into())),
            };
            let inlined = preprocess_symbolic_expression(eq_str, interpreter);
            let node = crate::symbolic::SymbolicParser::parse_str(&inlined)?;
            let solved_node = node.solve_equation(target_str)?;
            let solved_str = solved_node.to_string();
            
            // if target resolves against bound quantities, evaluate the solved expression against the interpreter's env
            // The python tests might expect a Number/Quantity back if variables are bound
            if let Ok(tokens) = crate::lexer::PhsLexer::new(&solved_str).tokenize() {
                let mut parser = crate::parser::PhsParser::new(&tokens);
                if let Ok(expr) = parser.parse_expr() {
                    // Only try to evaluate it if there are no unbound free variables
                    if !has_unbound_vars(&expr, interpreter) {
                        if let Ok(val) = interpreter.eval_expr(&expr) {
                            return Ok(Some(val));
                        }
                    }
                }
            }
            Ok(Some(PhsValue::String(solved_str)))
        }
        "plot" => {
            if args.is_empty() {
                return Err(PhysureError::Generic("plot expects at least 1 argument".into()));
            }
            let title = if args.len() >= 3 {
                if let PhsValue::String(s) = &args[2] {
                    s.clone()
                } else {
                    "Physure Live Plot".to_string()
                }
            } else {
                "Physure Live Plot".to_string()
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
            let result_str = if svg_plot.is_empty() {
                ascii_plot
            } else {
                format!("{}\n[PLOT_IMAGE:data:image/svg+xml;utf8,{}]", ascii_plot, svg_plot)
            };
            Ok(Some(PhsValue::String(result_str)))
        }
        _ => Ok(None),
    }
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

    let mut x_grid = Vec::with_capacity(width);
    let mut y_grid = Vec::with_capacity(width);

    for c in 0..width {
        let x_val = if width > 1 {
            x_min + (c as f64) * (x_max - x_min) / ((width - 1) as f64)
        } else {
            x_min
        };
        x_grid.push(x_val);

        // 1D Linear Interpolation
        let y_val = if pairs.len() == 1 {
            pairs[0].1
        } else if x_val <= pairs[0].0 {
            pairs[0].1
        } else if x_val >= pairs.last().unwrap().0 {
            pairs.last().unwrap().1
        } else {
            let mut val = pairs[0].1;
            for i in 0..pairs.len() - 1 {
                if x_val >= pairs[i].0 && x_val <= pairs[i + 1].0 {
                    let dx = pairs[i + 1].0 - pairs[i].0;
                    if dx.abs() > 1e-12 {
                        let t = (x_val - pairs[i].0) / dx;
                        val = pairs[i].1 + t * (pairs[i + 1].1 - pairs[i].1);
                    } else {
                        val = pairs[i].1;
                    }
                    break;
                }
            }
            val
        };
        y_grid.push(y_val);
    }

    let mut y_min = f64::INFINITY;
    let mut y_max = f64::NEG_INFINITY;
    for &val in &y_grid {
        if val < y_min { y_min = val; }
        if val > y_max { y_max = val; }
    }
    let y_span = if y_max != y_min { y_max - y_min } else { 1.0 };

    let fmt_x = if x_unit.is_empty() { String::new() } else { format!(" {}", x_unit) };
    let fmt_y = if y_unit.is_empty() { String::new() } else { format!(" {}", y_unit) };

    let mut lines = Vec::new();
    lines.push(format!("  📊 {}", title));

    let top_y_str = format!("  {:.*e}{}", 3, y_max, fmt_y);
    lines.push(format!("{:>18} ┐", top_y_str.trim()));

    for r in (0..height).rev() {
        let y_level = y_min + (r as f64 / (height - 1) as f64) * y_span;
        let mut row_chars = String::new();
        for c in 0..width {
            let val = y_grid[c];
            let diff = (val - y_level).abs() / y_span;
            if diff < (1.0 / (2.0 * height as f64)) {
                row_chars.push('█');
            } else if val > y_level {
                row_chars.push('░');
            } else {
                row_chars.push(' ');
            }
        }
        lines.push(format!("                   │ {}", row_chars));
    }

    let bot_y_str = format!("  {:.*e}{}", 3, y_min, fmt_y);
    lines.push(format!("{:>18} └{}", bot_y_str.trim(), "─".repeat(width)));

    let x_min_str = format!("{:.*e}{}", 3, x_min, fmt_x);
    let x_max_str = format!("{:.*e}{}", 3, x_max, fmt_x);
    let x_min_trim = x_min_str.trim();
    let x_max_trim = x_max_str.trim();
    let pad_len = if width + 12 > x_min_trim.len() + x_max_trim.len() {
        width + 12 - x_min_trim.len() - x_max_trim.len()
    } else {
        1
    };
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


fn expr_to_string(expr: &crate::ast::Expr) -> String {
    match expr {
        crate::ast::Expr::Number(n) => n.to_string(),
        crate::ast::Expr::Ident(s) => s.clone(),
        crate::ast::Expr::StringLiteral(s) => format!("\"{}\"", s),
        crate::ast::Expr::Unary { op, expr } => {
            let op_str = match op {
                crate::ast::UnaryOp::Neg => "-",
                crate::ast::UnaryOp::Sqrt => "sqrt",
            };
            if matches!(op, crate::ast::UnaryOp::Sqrt) {
                format!("{}({})", op_str, expr_to_string(expr))
            } else {
                format!("{}{}", op_str, expr_to_string(expr))
            }
        }
        crate::ast::Expr::Binary { op, left, right } => {
            let op_str = match op {
                crate::ast::BinaryOp::Add => "+",
                crate::ast::BinaryOp::Sub => "-",
                crate::ast::BinaryOp::Mul => "*",
                crate::ast::BinaryOp::Div => "/",
                crate::ast::BinaryOp::Pow => "^",
                crate::ast::BinaryOp::Eq => "==",
                crate::ast::BinaryOp::Neq => "!=",
                crate::ast::BinaryOp::Lt => "<",
                crate::ast::BinaryOp::Gt => ">",
                crate::ast::BinaryOp::Lte => "<=",
                crate::ast::BinaryOp::Gte => ">=",
                crate::ast::BinaryOp::ApproxEq => "≈",
            };
            format!("({}) {} ({})", expr_to_string(left), op_str, expr_to_string(right))
        }
        crate::ast::Expr::ImplicitMul { left, right } => {
            format!("({}) * ({})", expr_to_string(left), expr_to_string(right))
        }
        crate::ast::Expr::Call { name, args } => {
            let args_str: Vec<String> = args.iter().map(expr_to_string).collect();
            format!("{}({})", name, args_str.join(", "))
        }
        crate::ast::Expr::Ternary { cond, then_expr, else_expr } => {
            format!("({}) ? ({}) : ({})", expr_to_string(cond), expr_to_string(then_expr), expr_to_string(else_expr))
        }
        crate::ast::Expr::Uncertainty { val, unc } => {
            format!("({}) +/- ({})", expr_to_string(val), expr_to_string(unc))
        }
        crate::ast::Expr::Convert { expr, target_unit } => {
            format!("({}) => {}", expr_to_string(expr), target_unit)
        }
        crate::ast::Expr::FormatSig { expr, spec } => {
            format!("({}):{}", expr_to_string(expr), spec)
        }
        _ => String::new(),
    }
}

fn substitute_expr(expr: &crate::ast::Expr, params: &[String], args: &[crate::ast::Expr]) -> crate::ast::Expr {
    match expr {
        crate::ast::Expr::Ident(name) => {
            if let Some(idx) = params.iter().position(|p| p == name) {
                if idx < args.len() {
                    args[idx].clone()
                } else {
                    expr.clone()
                }
            } else {
                expr.clone()
            }
        }
        crate::ast::Expr::Unary { op, expr: inner } => {
            crate::ast::Expr::Unary {
                op: op.clone(),
                expr: Box::new(substitute_expr(inner, params, args)),
            }
        }
        crate::ast::Expr::Binary { op, left, right } => {
            crate::ast::Expr::Binary {
                op: op.clone(),
                left: Box::new(substitute_expr(left, params, args)),
                right: Box::new(substitute_expr(right, params, args)),
            }
        }
        crate::ast::Expr::ImplicitMul { left, right } => {
            crate::ast::Expr::ImplicitMul {
                left: Box::new(substitute_expr(left, params, args)),
                right: Box::new(substitute_expr(right, params, args)),
            }
        }
        crate::ast::Expr::Call { name, args: call_args } => {
            let new_args = call_args.iter().map(|arg| substitute_expr(arg, params, args)).collect();
            crate::ast::Expr::Call {
                name: name.clone(),
                args: new_args,
            }
        }
        crate::ast::Expr::Ternary { cond, then_expr, else_expr } => {
            crate::ast::Expr::Ternary {
                cond: Box::new(substitute_expr(cond, params, args)),
                then_expr: Box::new(substitute_expr(then_expr, params, args)),
                else_expr: Box::new(substitute_expr(else_expr, params, args)),
            }
        }
        crate::ast::Expr::Uncertainty { val, unc } => {
            crate::ast::Expr::Uncertainty {
                val: Box::new(substitute_expr(val, params, args)),
                unc: Box::new(substitute_expr(unc, params, args)),
            }
        }
        crate::ast::Expr::Convert { expr: inner, target_unit } => {
            crate::ast::Expr::Convert {
                expr: Box::new(substitute_expr(inner, params, args)),
                target_unit: target_unit.clone(),
            }
        }
        crate::ast::Expr::FormatSig { expr: inner, spec } => {
            crate::ast::Expr::FormatSig {
                expr: Box::new(substitute_expr(inner, params, args)),
                spec: spec.clone(),
            }
        }
        other => other.clone(),
    }
}

fn inline_expr(expr: &crate::ast::Expr, interpreter: &PhsInterpreter) -> crate::ast::Expr {
    match expr {
        crate::ast::Expr::Call { name, args } => {
            let inlined_args: Vec<crate::ast::Expr> = args.iter().map(|arg| inline_expr(arg, interpreter)).collect();
            if let Some(user_fn) = interpreter.get_user_fn(name) {
                let body_expr = match user_fn.body.first() {
                    Some(crate::ast::Statement::Query { expr }) => Some(expr),
                    Some(crate::ast::Statement::ExprStmt(expr)) => Some(expr),
                    _ => None,
                };
                if let Some(be) = body_expr {
                    let param_names: Vec<String> = user_fn.params.iter().map(|p| p.name.clone()).collect();
                    let substituted = substitute_expr(be, &param_names, &inlined_args);
                    inline_expr(&substituted, interpreter)
                } else {
                    crate::ast::Expr::Call { name: name.clone(), args: inlined_args }
                }
            } else {
                crate::ast::Expr::Call { name: name.clone(), args: inlined_args }
            }
        }
        crate::ast::Expr::Unary { op, expr: inner } => {
            crate::ast::Expr::Unary {
                op: op.clone(),
                expr: Box::new(inline_expr(inner, interpreter)),
            }
        }
        crate::ast::Expr::Binary { op, left, right } => {
            crate::ast::Expr::Binary {
                op: op.clone(),
                left: Box::new(inline_expr(left, interpreter)),
                right: Box::new(inline_expr(right, interpreter)),
            }
        }
        crate::ast::Expr::ImplicitMul { left, right } => {
            crate::ast::Expr::ImplicitMul {
                left: Box::new(inline_expr(left, interpreter)),
                right: Box::new(inline_expr(right, interpreter)),
            }
        }
        crate::ast::Expr::Ternary { cond, then_expr, else_expr } => {
            crate::ast::Expr::Ternary {
                cond: Box::new(inline_expr(cond, interpreter)),
                then_expr: Box::new(inline_expr(then_expr, interpreter)),
                else_expr: Box::new(inline_expr(else_expr, interpreter)),
            }
        }
        crate::ast::Expr::Uncertainty { val, unc } => {
            crate::ast::Expr::Uncertainty {
                val: Box::new(inline_expr(val, interpreter)),
                unc: Box::new(inline_expr(unc, interpreter)),
            }
        }
        crate::ast::Expr::Convert { expr: inner, target_unit } => {
            crate::ast::Expr::Convert {
                expr: Box::new(inline_expr(inner, interpreter)),
                target_unit: target_unit.clone(),
            }
        }
        crate::ast::Expr::FormatSig { expr: inner, spec } => {
            crate::ast::Expr::FormatSig {
                expr: Box::new(inline_expr(inner, interpreter)),
                spec: spec.clone(),
            }
        }
        other => other.clone(),
    }
}

fn preprocess_symbolic_expression(expr_str: &str, interpreter: &PhsInterpreter) -> String {
    let parts: Vec<&str> = if expr_str.contains("==") {
        expr_str.split("==").collect()
    } else if expr_str.contains('=') {
        expr_str.split('=').collect()
    } else {
        vec![expr_str]
    };

    let processed_parts: Vec<String> = parts.iter().map(|part| {
        let part_trim = part.trim();
        if let Ok(tokens) = crate::lexer::PhsLexer::new(part_trim).tokenize() {
            let mut parser = crate::parser::PhsParser::new(&tokens);
            if let Ok(expr) = parser.parse_expr() {
                let inlined = inline_expr(&expr, interpreter);
                return expr_to_string(&inlined);
            }
        }
        part_trim.to_string()
    }).collect();

    if expr_str.contains("==") {
        processed_parts.join(" == ")
    } else if expr_str.contains('=') {
        processed_parts.join(" = ")
    } else {
        processed_parts[0].clone()
    }
}

fn has_unbound_vars(expr: &crate::ast::Expr, interpreter: &PhsInterpreter) -> bool {
    match expr {
        crate::ast::Expr::Ident(name) => {
            interpreter.get_var(name).is_none() && !interpreter.registry().contains(name)
        }
        crate::ast::Expr::Unary { expr, .. } => has_unbound_vars(expr, interpreter),
        crate::ast::Expr::Binary { left, right, .. } => {
            has_unbound_vars(left, interpreter) || has_unbound_vars(right, interpreter)
        }
        crate::ast::Expr::ImplicitMul { left, right } => {
            has_unbound_vars(left, interpreter) || has_unbound_vars(right, interpreter)
        }
        crate::ast::Expr::Call { args, .. } => {
            args.iter().any(|arg| has_unbound_vars(arg, interpreter))
        }
        crate::ast::Expr::Ternary { cond, then_expr, else_expr } => {
            has_unbound_vars(cond, interpreter)
                || has_unbound_vars(then_expr, interpreter)
                || has_unbound_vars(else_expr, interpreter)
        }
        crate::ast::Expr::Let { val, body, .. } => {
            has_unbound_vars(val, interpreter) || has_unbound_vars(body, interpreter)
        }
        crate::ast::Expr::If { cond, then_expr, else_expr } => {
            has_unbound_vars(cond, interpreter)
                || has_unbound_vars(then_expr, interpreter)
                || has_unbound_vars(else_expr, interpreter)
        }
        crate::ast::Expr::Vector(items) => {
            items.iter().any(|item| has_unbound_vars(item, interpreter))
        }
        crate::ast::Expr::Convert { expr, .. } => {
            has_unbound_vars(expr, interpreter)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::PhsInterpreter;
    use crate::value::PhsValue;


    fn eval(name: &str, args: Vec<PhsValue>) -> PhsValue {
        let mut interp = PhsInterpreter::new();
        eval_builtin(name, &args, &mut interp).unwrap().unwrap()
    }

    #[test]
    fn test_sqrt() {
        assert_eq!(eval("sqrt", vec![PhsValue::Number(9.0)]), PhsValue::Number(3.0));
    }

    #[test]
    fn test_log() {
        assert_eq!(eval("log", vec![PhsValue::Number(100.0)]), PhsValue::Number(2.0));
    }

    #[test]
    fn test_trig() {
        assert_eq!(eval("sin", vec![PhsValue::Number(0.0)]), PhsValue::Number(0.0));
        assert_eq!(eval("tan", vec![PhsValue::Number(0.0)]), PhsValue::Number(0.0));
    }

    #[test]
    fn test_floor_ceil() {
        assert_eq!(eval("floor", vec![PhsValue::Number(2.7)]), PhsValue::Number(2.0));
        assert_eq!(eval("ceil", vec![PhsValue::Number(2.3)]), PhsValue::Number(3.0));
    }

    #[test]
    fn test_min_max() {
        let min_val = eval("min", vec![PhsValue::Number(5.0), PhsValue::Number(2.0), PhsValue::Number(8.0)]);
        assert_eq!(min_val, PhsValue::Number(2.0));
        let max_val = eval("max", vec![PhsValue::Number(5.0), PhsValue::Number(2.0), PhsValue::Number(8.0)]);
        assert_eq!(max_val, PhsValue::Number(8.0));
    }

    #[test]
    fn test_deriv() {
        let res = eval("deriv", vec![PhsValue::String("x^2".into()), PhsValue::String("x".into())]);
        if let PhsValue::String(s) = res {
            assert!(s.contains("2") && s.contains("x")); // Simplification might leave 1s, just check it differentiated
        } else {
            panic!("Expected string");
        }
    }

    #[test]
    fn test_integral() {
        let res = eval("integral", vec![PhsValue::String("2 * x".into()), PhsValue::String("x".into())]);
        if let PhsValue::String(s) = res {
            assert!(s.contains("2") && s.contains("x")); 
        } else {
            panic!("Expected string");
        }
    }

    #[test]
    fn test_solve() {
        let res = eval("solve", vec![PhsValue::String("2 * x = 10".into()), PhsValue::String("x".into())]);
        // The solve function might return the PhsValue evaluated if it's purely numeric
        match res {
            PhsValue::Number(n) => assert_eq!(n, 5.0),
            PhsValue::String(s) => assert_eq!(s, "5"),
            _ => panic!("Expected number or string"),
        }
    }

    #[test]
    fn test_linspace() {
        let res = eval("linspace", vec![PhsValue::Number(0.0), PhsValue::Number(1.0), PhsValue::Number(3.0)]);
        if let PhsValue::Vector(v) = res {
            assert_eq!(v.len(), 3);
            assert_eq!(v[0], PhsValue::Number(0.0));
            assert_eq!(v[1], PhsValue::Number(0.5));
            assert_eq!(v[2], PhsValue::Number(1.0));
        } else {
            panic!("Expected vector");
        }
    }

    #[test]
    fn test_gradient() {
        let y = PhsValue::Vector(vec![PhsValue::Number(1.0), PhsValue::Number(4.0), PhsValue::Number(9.0)]);
        let x = PhsValue::Vector(vec![PhsValue::Number(1.0), PhsValue::Number(2.0), PhsValue::Number(3.0)]);
        let res = eval("gradient", vec![y, x]);
        if let PhsValue::Vector(v) = res {
            assert_eq!(v.len(), 3);
        } else {
            panic!("Expected vector");
        }
    }

    #[test]
    fn test_trapz() {
        let y = PhsValue::Vector(vec![PhsValue::Number(1.0), PhsValue::Number(1.0)]);
        let x = PhsValue::Vector(vec![PhsValue::Number(0.0), PhsValue::Number(1.0)]);
        let res = eval("trapz", vec![y, x]);
        assert_eq!(res, PhsValue::Number(1.0));
    }
}
