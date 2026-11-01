use physure_core::error::{PhysureError, PhysureResult};
use physure_core::quantity::Quantity;
use physure_core::units::parser::Parser as UnitParser;
use physure_core::units::RationalUnit;
use crate::ast::{BinaryOp, Expr, Statement};
use crate::symbolic::Node;
use crate::value::PhsValue;
use super::PhsInterpreter;
use super::helpers::{make_range, strip_unit_comment};

fn node_op(op: BinaryOp, a: Node, b: Node) -> PhysureResult<Node> {
    Ok(match op {
        BinaryOp::Add => Node::Add(vec![a, b]),
        BinaryOp::Sub => Node::Sub(Box::new(a), Box::new(b)),
        BinaryOp::Mul => Node::Mul(vec![a, b]),
        BinaryOp::Div => Node::Div(Box::new(a), Box::new(b)),
        _ => return Err(PhysureError::Generic("Pow/Convert are not supported for equation algebra yet".into())),
    })
}

/// Converts a non-Equation operand into a symbolic `Node` for equation algebra.
/// A dimensionless `Quantity` (e.g. a bare scale factor) becomes its numeric value;
/// a dimensioned one (e.g. `2 m`) is kept as `number * unit_symbol` so the unit isn't
/// silently dropped from the resulting equation's text.
/// ponytail: the unit symbol isn't a real bindable variable, so it stays purely
/// symbolic — collides only if the equation also has a variable named the same as the unit.
fn value_to_symbolic_node(val: &PhsValue) -> PhysureResult<Node> {
    match val {
        PhsValue::Number(n) => Ok(Node::Number(*n)),
        PhsValue::String(s) => crate::symbolic::SymbolicParser::parse_str(s),
        PhsValue::Quantity(q) if q.unit == RationalUnit::dimensionless() => Ok(Node::Number(q.value.mean())),
        PhsValue::Quantity(q) => Ok(Node::Mul(vec![Node::Number(q.value.mean()), Node::Symbol(q.unit.__repr__())])),
        _ => Err(PhysureError::Generic("Equation algebra only supports Number, String, Equation, or Quantity operands".into())),
    }
}

/// A plain string holding `"lhs = rhs"` (e.g. from a bare assignment, not `solve()`)
/// is coerced into an `Equation` so it supports the same arithmetic. Strings without
/// a top-level `=` (unit symbols, bare variable names) pass through unchanged.
pub(crate) fn coerce_equation_string(val: PhsValue) -> PhsValue {
    if let PhsValue::String(ref s) = val {
        if let Ok(Some((l, r))) = crate::symbolic::SymbolicParser::parse_equation_str(s) {
            return PhsValue::Equation(l, r);
        }
    }
    val
}

impl PhsInterpreter {
    pub fn eval_binary_op_vals(&self, op: BinaryOp, l_val: PhsValue, r_val: PhsValue) -> PhysureResult<PhsValue> {
        if op == BinaryOp::Range {
            return make_range(l_val, r_val);
        }
        let l_val = coerce_equation_string(l_val);
        let r_val = coerce_equation_string(r_val);
        // A range is its two endpoints and nothing else, so an operation on one is that
        // operation on both: `(0 .. 100) m` is `0 m .. 100 m` and `(0 m .. 100 m) => km` is
        // `0 km .. 0.1 km`. Only the operations that keep it a range are distributed —
        // adding two ranges asks a question about intervals that PHS has not been told the
        // answer to, and guessing one is worse than refusing.
        if let PhsValue::Range(start, end) = &l_val {
            if matches!(op, BinaryOp::Mul | BinaryOp::Div | BinaryOp::Convert) {
                let lo = self.eval_binary_op_vals(op, (**start).clone(), r_val.clone())?;
                let hi = self.eval_binary_op_vals(op, (**end).clone(), r_val)?;
                return make_range(lo, hi);
            }
        }
        match (l_val, r_val) {
            (PhsValue::Function(f), PhsValue::Function(g)) => Ok(PhsValue::Function(combine_functions(op, &f, &g))),
            (PhsValue::Equation(l1, r1), PhsValue::Equation(l2, r2)) => equation_op(op, (l1, r1), (l2, r2)),
            (PhsValue::Equation(l, r), other) => {
                let node = value_to_symbolic_node(&other)?;
                equation_op(op, (l, r), (node.clone(), node))
            }
            (other, PhsValue::Equation(l, r)) => {
                let node = value_to_symbolic_node(&other)?;
                equation_op(op, (node.clone(), node), (l, r))
            }
            (PhsValue::Vector(l_vec), PhsValue::Vector(r_vec)) => {
                if l_vec.len() != r_vec.len() {
                    return Err(PhysureError::Generic("Vector length mismatch in binary operation".into()));
                }
                let pairs = l_vec.into_iter().zip(r_vec);
                collect_vector(pairs, |(l_item, r_item)| self.eval_binary_op_vals(op, l_item, r_item))
            }
            (PhsValue::Vector(v_vec), scalar) => collect_vector(v_vec, |item| self.eval_binary_op_vals(op, item, scalar.clone())),
            (scalar, PhsValue::Vector(v_vec)) => collect_vector(v_vec, |item| self.eval_binary_op_vals(op, scalar.clone(), item)),
            (PhsValue::Quantity(l), PhsValue::Quantity(r)) => quantity_op(op, l, r),
            (PhsValue::Quantity(l), PhsValue::Number(r)) => {
                self.eval_binary_op_vals(op, PhsValue::Quantity(l), PhsValue::Quantity(plain_quantity(r)))
            }
            (PhsValue::Number(l), PhsValue::Quantity(r)) => {
                self.eval_binary_op_vals(op, PhsValue::Quantity(plain_quantity(l)), PhsValue::Quantity(r))
            }
            (PhsValue::Number(l), PhsValue::Number(r)) => number_op(op, l, r),
            // A bare word that isn't a bound variable arrives here as a String, so these
            // four arms are where `5 foobar` is decided. The unit parser now reports the
            // offending symbol and the nearest registered one; swallowing that with `if
            // let Ok` would replace a usable message with a bare "Unknown unit symbol".
            (PhsValue::Quantity(l), PhsValue::String(r)) => quantity_by_unit_text(op, l, &r, true),
            (PhsValue::Number(l), PhsValue::String(r)) => quantity_by_unit_text(op, plain_quantity(l), &r, false),
            (PhsValue::String(l), PhsValue::Quantity(r)) => unit_text_by_quantity(op, &l, r),
            (PhsValue::String(l), PhsValue::Number(r)) => unit_text_by_quantity(op, &l, plain_quantity(r)),
            _ => Err(PhysureError::Generic("Invalid operand types for binary operation".into())),
        }
    }
}

/// A dimensionless quantity of exactly `x`.
fn plain_quantity(x: f64) -> Quantity {
    Quantity::new_scalar(x, 0.0, RationalUnit::dimensionless(), None, None)
}

/// `f op g` for two functions: the function that applies `op` to what each returns for the same arguments.
fn combine_functions(op: BinaryOp, f: &crate::ast::FunctionDefNode, g: &crate::ast::FunctionDefNode) -> crate::ast::FunctionDefNode {
    let (params, param_units) = if !f.params.is_empty() {
        (f.params.clone(), f.param_units.clone())
    } else {
        (g.params.clone(), g.param_units.clone())
    };
    let args_expr: Vec<Expr> = params.iter().map(|p| Expr::Identifier(p.clone())).collect();
    let body = Statement::Expr(Expr::BinaryOp {
        op,
        left: Box::new(Expr::FunctionCall { name: f.name.clone(), args: args_expr.clone(), kwargs: Vec::new() }),
        right: Box::new(Expr::FunctionCall { name: g.name.clone(), args: args_expr, kwargs: Vec::new() }),
    });
    let name = match op {
        BinaryOp::Add => format!("{}_add_{}", f.name, g.name),
        BinaryOp::Sub => format!("{}_sub_{}", f.name, g.name),
        BinaryOp::Mul => format!("{}_mul_{}", f.name, g.name),
        BinaryOp::Div => format!("{}_div_{}", f.name, g.name),
        _ => format!("{}_op_{}", f.name, g.name),
    };
    crate::ast::FunctionDefNode {
        name,
        params,
        param_units,
        body_stmts: vec![body],
        body_lines: vec![],
        decorators: Vec::new(),
        doc: None,
    }
}

/// `op` applied to both sides of two equations (a plain value being the equation `x = x`).
fn equation_op(op: BinaryOp, left: (Node, Node), right: (Node, Node)) -> PhysureResult<PhsValue> {
    let new_l = node_op(op, left.0, right.0)?.simplify();
    let new_r = node_op(op, left.1, right.1)?.simplify();
    Ok(PhsValue::Equation(new_l, new_r))
}

/// The vector of `f` applied to each item, or the first error.
fn collect_vector<T>(items: impl IntoIterator<Item = T>, f: impl FnMut(T) -> PhysureResult<PhsValue>) -> PhysureResult<PhsValue> {
    items.into_iter().map(f).collect::<PhysureResult<Vec<_>>>().map(PhsValue::Vector)
}

fn quantity_op(op: BinaryOp, l: Quantity, r: Quantity) -> PhysureResult<PhsValue> {
    let res = match op {
        BinaryOp::Add => l.add(&r)?,
        BinaryOp::Sub => l.sub(&r)?,
        BinaryOp::Mul => l.mul(&r)?,
        BinaryOp::Div => l.div(&r)?,
        BinaryOp::Pow => {
            if r.unit == RationalUnit::dimensionless() && r.value.std_dev() == 0.0 {
                l.pow(r.value.mean())?
            } else {
                return Err(PhysureError::Generic("Exponent must be a dimensionless constant".into()));
            }
        }
        BinaryOp::Convert | BinaryOp::Range => unreachable!(),
    };
    Ok(PhsValue::Quantity(res))
}

fn number_op(op: BinaryOp, l: f64, r: f64) -> PhysureResult<PhsValue> {
    let res = match op {
        BinaryOp::Add => l + r,
        BinaryOp::Sub => l - r,
        BinaryOp::Mul => l * r,
        BinaryOp::Div => {
            if r == 0.0 {
                return Err(PhysureError::Generic("Division by zero".into()));
            }
            l / r
        }
        BinaryOp::Pow => {
            if l < 0.0 && r.fract() != 0.0 {
                return Err(PhysureError::DomainError(format!(
                    "{l}^{r} cannot be computed for a negative base with a non-integer exponent"
                )));
            }
            l.powf(r)
        }
        BinaryOp::Convert | BinaryOp::Range => unreachable!(),
    };
    Ok(PhsValue::Number(res))
}

fn unsupported_with_unit_text() -> PhysureError {
    PhysureError::Generic("Unsupported op with unit string".into())
}

/// `quantity op unit-word`: `5 m / s`, and a conversion to the unit the word names when `convert` (a plain number
/// is not converted: `5 => m` stays unsupported).
fn quantity_by_unit_text(op: BinaryOp, l: Quantity, text: &str, convert: bool) -> PhysureResult<PhsValue> {
    let parsed_unit = UnitParser::parse_expression(strip_unit_comment(text))?;
    let unit_q = Quantity::new_scalar(1.0, 0.0, parsed_unit.clone(), None, None);
    let res = match op {
        BinaryOp::Mul => l.mul(&unit_q)?,
        BinaryOp::Div => l.div(&unit_q)?,
        BinaryOp::Convert if convert => l.convert_to(&parsed_unit)?,
        _ => return Err(unsupported_with_unit_text()),
    };
    Ok(PhsValue::Quantity(res))
}

/// `unit-word op quantity`: `m * 5`, `s ^ 2`.
fn unit_text_by_quantity(op: BinaryOp, text: &str, r: Quantity) -> PhysureResult<PhsValue> {
    let parsed_unit = UnitParser::parse_expression(strip_unit_comment(text))?;
    let unit_q = Quantity::new_scalar(1.0, 0.0, parsed_unit, None, None);
    let res = match op {
        BinaryOp::Mul => unit_q.mul(&r)?,
        BinaryOp::Pow => unit_q.pow(r.value.mean())?,
        _ => return Err(unsupported_with_unit_text()),
    };
    Ok(PhsValue::Quantity(res))
}
