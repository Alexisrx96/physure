use crate::value::PhsValue;
use physure_core::UnitRegistry;

#[derive(Debug, Clone, PartialEq)]
pub enum ScopeKind {
    Global,
    Local { owner_fn: String, frame_depth: usize },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValueKind {
    Scalar,
    Vector(usize),
    Matrix(usize, usize),
    Function,
    Equation,
    Bool,
    String,
    None,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UncertaintySummary {
    pub std_dev: f64,
    pub backend: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValueDetail {
    None,
    Function { params: Vec<String>, param_units: Vec<Option<String>> },
    Equation { lhs: String, rhs: String },
    /// Vector/Matrix elements, each recursively inspected, capped at the first 10 -- `kind`
    /// already carries the true length/shape, so truncation here never loses that information.
    Elements(Vec<Inspection>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Inspection {
    pub name: String,
    pub kind: ValueKind,
    pub scope: ScopeKind,
    pub measure: Option<f64>,
    pub unit_display: Option<String>,
    pub prefix: Option<(String, f64)>,
    pub dimension: Vec<(String, i64, i64)>,
    pub uncertainty: Option<UncertaintySummary>,
    pub detail: ValueDetail,
}

const MAX_INSPECTED_ELEMENTS: usize = 10;

pub fn inspect(name: &str, value: &PhsValue, scope: ScopeKind, registry: &UnitRegistry) -> Inspection {
    let base = Inspection {
        name: name.to_string(),
        kind: ValueKind::None,
        scope: scope.clone(),
        measure: None,
        unit_display: None,
        prefix: None,
        dimension: Vec::new(),
        uncertainty: None,
        detail: ValueDetail::None,
    };
    match value {
        PhsValue::None => base,
        PhsValue::Number(n) => Inspection { kind: ValueKind::Scalar, measure: Some(*n), ..base },
        PhsValue::Bool(_) => Inspection { kind: ValueKind::Bool, ..base },
        PhsValue::String(_) => Inspection { kind: ValueKind::String, ..base },
        PhsValue::Quantity(q) => {
            let unit_display = Some(q.unit.__repr__());
            let prefix = q.unit.display_name.as_ref().and_then(|dn| registry.split_prefix(dn));
            let dimension = q.unit.dimensions.iter().map(|(sym, (n, d))| (sym.clone(), *n, *d)).collect();
            let std_dev = q.value.std_dev();
            let uncertainty = if std_dev > 0.0 {
                Some(UncertaintySummary { std_dev, backend: q.value.get_model_name().to_string() })
            } else {
                None
            };
            Inspection {
                kind: ValueKind::Scalar,
                measure: Some(q.value.mean()),
                unit_display,
                prefix,
                dimension,
                uncertainty,
                ..base
            }
        }
        PhsValue::Vector(v) => Inspection {
            kind: ValueKind::Vector(v.len()),
            detail: ValueDetail::Elements(
                v.iter()
                    .take(MAX_INSPECTED_ELEMENTS)
                    .enumerate()
                    .map(|(i, el)| inspect(&format!("{name}[{i}]"), el, scope.clone(), registry))
                    .collect(),
            ),
            ..base
        },
        PhsValue::Matrix(m) => Inspection {
            kind: ValueKind::Matrix(m.rows, m.cols),
            detail: ValueDetail::Elements(
                m.data
                    .iter()
                    .flatten()
                    .take(MAX_INSPECTED_ELEMENTS)
                    .enumerate()
                    .map(|(i, q)| inspect(&format!("{name}[{i}]"), &PhsValue::Quantity(q.clone()), scope.clone(), registry))
                    .collect(),
            ),
            ..base
        },
        PhsValue::Function(f) => Inspection {
            kind: ValueKind::Function,
            detail: ValueDetail::Function { params: f.params.clone(), param_units: f.param_units.clone() },
            ..base
        },
        PhsValue::Equation(l, r) => Inspection {
            kind: ValueKind::Equation,
            detail: ValueDetail::Equation { lhs: format!("{l:?}"), rhs: format!("{r:?}") },
            ..base
        },
        // Sigma/SigmaBound/Plot/Range: no dedicated Inspection shape yet -- fall back to the
        // untyped base rather than guessing at a decomposition the roadmap never specified.
        _ => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use physure_core::quantity::Quantity;
    use physure_core::units::parser::Parser as UnitParser;

    fn registry() -> UnitRegistry {
        physure_core::units::conf::build_registry_from_conf().0
    }

    #[test]
    fn inspects_a_plain_scalar_number() {
        let reg = registry();
        let insp = inspect("x", &PhsValue::Number(3.0), ScopeKind::Global, &reg);
        assert_eq!(insp.kind, ValueKind::Scalar);
        assert_eq!(insp.measure, Some(3.0));
        assert_eq!(insp.unit_display, None);
        assert_eq!(insp.dimension, vec![]);
        assert_eq!(insp.prefix, None);
    }

    #[test]
    fn inspects_a_km_quantity_with_prefix_present() {
        let reg = registry();
        let unit = UnitParser::parse_expression_with_registry("km", &reg).unwrap();
        let q = Quantity::new_scalar(5.0, 0.0, unit, None, None);
        let insp = inspect("d", &PhsValue::Quantity(q), ScopeKind::Global, &reg);
        assert_eq!(insp.kind, ValueKind::Scalar);
        assert_eq!(insp.measure, Some(5.0));
        assert_eq!(insp.prefix, Some(("k".to_string(), 1000.0)));
        assert!(insp.dimension.iter().any(|(sym, _, _)| sym == "m"));
    }

    #[test]
    fn inspects_a_compound_unit_with_prefix_absent() {
        let reg = registry();
        let unit = UnitParser::parse_expression_with_registry("km/h", &reg).unwrap();
        let q = Quantity::new_scalar(60.0, 0.0, unit, None, None);
        let insp = inspect("v", &PhsValue::Quantity(q), ScopeKind::Global, &reg);
        assert_eq!(insp.prefix, None);
    }
}
