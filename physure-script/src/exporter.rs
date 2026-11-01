use std::collections::HashMap;
use crate::PhsValue;

#[derive(Debug, Clone)]
pub enum ExportError {
    SerializationError(String),
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportError::SerializationError(msg) => write!(f, "Serialization error: {}", msg),
        }
    }
}

impl std::error::Error for ExportError {}

pub trait DataExporter {
    fn export_json(exports: &HashMap<String, PhsValue>) -> Result<String, ExportError>;
    fn export_csv(exports: &HashMap<String, PhsValue>) -> Result<String, ExportError>;
    fn export_py_dict(exports: &HashMap<String, PhsValue>) -> Result<String, ExportError>;
}

pub struct Exporter;

impl DataExporter for Exporter {
    fn export_json(exports: &HashMap<String, PhsValue>) -> Result<String, ExportError> {
        let mut json = String::new();
        json.push('{');
        let mut first = true;
        
        let mut keys: Vec<&String> = exports.keys().collect();
        keys.sort(); // Sort keys to make the output deterministic for tests

        for k in keys {
            let v = exports.get(k).unwrap();
            if !first {
                json.push_str(", ");
            }
            first = false;
            json.push_str(&format!("{}: {}", json_string(k), value_to_json(v)));
        }
        json.push('}');
        Ok(json)
    }

    fn export_csv(exports: &HashMap<String, PhsValue>) -> Result<String, ExportError> {
        let mut csv = String::new();
        csv.push_str("key,value\n");

        let mut keys: Vec<&String> = exports.keys().collect();
        keys.sort(); // Sort keys to make the output deterministic for tests

        for k in keys {
            let v = exports.get(k).unwrap();
            csv.push_str(&format!("{},{}\n", csv_field(k), value_to_csv(v)));
        }
        Ok(csv)
    }

    fn export_py_dict(exports: &HashMap<String, PhsValue>) -> Result<String, ExportError> {
        let mut py = String::new();
        py.push('{');
        let mut first = true;

        let mut keys: Vec<&String> = exports.keys().collect();
        keys.sort(); // Sort keys to make the output deterministic for tests

        for k in keys {
            let v = exports.get(k).unwrap();
            if !first {
                py.push_str(", ");
            }
            first = false;
            py.push_str(&format!("{}: {}", py_string(k), value_to_py(v)));
        }
        py.push('}');
        Ok(py)
    }
}

/// `s` as a JSON string literal, with the quotes, backslashes and control characters escaped.
fn json_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}

/// A JSON number. JSON has no `NaN` or infinity, so they are `null`.
fn json_number(n: f64) -> String {
    if n.is_finite() { format!("{}", n) } else { "null".to_string() }
}

/// `s` as one quoted CSV field: a quote inside it is doubled.
fn csv_quote(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// A CSV field, quoted only when it has to be.
fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) { csv_quote(s) } else { s.to_string() }
}

/// `s` as a Python string literal in single quotes.
fn py_string(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'").replace('\n', "\\n").replace('\r', "\\r"))
}

fn value_to_json(v: &PhsValue) -> String {
    match v {
        PhsValue::None => "null".to_string(),
        PhsValue::Number(n) => json_number(*n),
        PhsValue::Bool(b) => if *b { "true".to_string() } else { "false".to_string() },
        PhsValue::String(s) => json_string(s),
        PhsValue::Quantity(q) => format!("{{\"value\": {}, \"unit\": {}}}", json_number(q.value.mean()), json_string(&q.unit.__repr__())),
        PhsValue::Function(_) => "\"<function>\"".to_string(),
        PhsValue::Sigma(k) => json_number(*k),
        PhsValue::SigmaBound(q, k) => format!(
            "{{\"value\": {}, \"unit\": {}, \"sigma\": {}}}",
            json_number(q.value.mean()),
            json_string(&q.unit.__repr__()),
            json_number(*k)
        ),
        PhsValue::Plot(p) => json_string(&p.ascii),
        PhsValue::Equation(l, r) => json_string(&format!("{} = {}", l.to_phs_string(), r.to_phs_string())),
        PhsValue::Vector(vec) => {
            let mut s = String::new();
            s.push('[');
            for (i, val) in vec.iter().enumerate() {
                if i > 0 { s.push_str(", "); }
                s.push_str(&value_to_json(val));
            }
            s.push(']');
            s
        }
        PhsValue::Matrix(m) => format!("{{\"rows\": {}, \"cols\": {}, \"data\": {:?}}}", m.rows, m.cols, m.data),
        PhsValue::Range(start, end) => format!("{{\"start\": {}, \"end\": {}}}", value_to_json(start), value_to_json(end)),
    }
}

fn value_to_csv(v: &PhsValue) -> String {
    match v {
        PhsValue::None => "".to_string(),
        PhsValue::Number(n) => format!("{}", n),
        PhsValue::Bool(b) => if *b { "true".to_string() } else { "false".to_string() },
        PhsValue::String(s) => csv_quote(s),
        PhsValue::Quantity(q) => {
            let unit = q.unit.__repr__();
            if unit.is_empty() {
                format!("{}", q.value.mean())
            } else {
                format!("{} {}", q.value.mean(), unit)
            }
        },
        PhsValue::Function(_) => "<function>".to_string(),
        PhsValue::Sigma(k) => format!("{}σ", k),
        PhsValue::SigmaBound(q, k) => format!("{} ± {}σ", q, k),
        PhsValue::Plot(p) => csv_quote(&p.ascii.replace('\n', " ")),
        PhsValue::Equation(l, r) => csv_quote(&format!("{} = {}", l.to_phs_string(), r.to_phs_string())),
        PhsValue::Vector(vec) => {
            let mut s = String::new();
            s.push_str("\"[");
            for (i, val) in vec.iter().enumerate() {
                if i > 0 { s.push_str(", "); }
                let inner = value_to_csv(val).replace("\"", "");
                s.push_str(&inner);
            }
            s.push_str("]\"");
            s
        }
        PhsValue::Matrix(m) => format!("\"{:?}\"", m.data),
        PhsValue::Range(start, end) => format!("\"{} .. {}\"", value_to_csv(start), value_to_csv(end)),
    }
}

fn value_to_py(v: &PhsValue) -> String {
    match v {
        PhsValue::None => "None".to_string(),
        PhsValue::Number(n) => format!("{}", n),
        PhsValue::Bool(b) => if *b { "True".to_string() } else { "False".to_string() },
        PhsValue::String(s) => py_string(s),
        PhsValue::Quantity(q) => format!("{{'value': {}, 'unit': {}}}", q.value.mean(), py_string(&q.unit.__repr__())),
        PhsValue::Function(_) => "'<function>'".to_string(),
        PhsValue::Sigma(k) => format!("{}", k),
        PhsValue::SigmaBound(q, k) => format!("{{'value': {}, 'unit': {}, 'sigma': {}}}", q.value.mean(), py_string(&q.unit.__repr__()), k),
        PhsValue::Plot(p) => py_string(&p.ascii),
        PhsValue::Equation(l, r) => py_string(&format!("{} = {}", l.to_phs_string(), r.to_phs_string())),
        PhsValue::Vector(vec) => {
            let mut s = String::new();
            s.push('[');
            for (i, val) in vec.iter().enumerate() {
                if i > 0 { s.push_str(", "); }
                s.push_str(&value_to_py(val));
            }
            s.push(']');
            s
        }
        PhsValue::Matrix(m) => format!("{:?}", m.data),
        PhsValue::Range(start, end) => format!("{{'start': {}, 'end': {}}}", value_to_py(start), value_to_py(end)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use physure_core::quantity::Quantity;

    #[test]
    fn test_export_json() {
        let mut exports = HashMap::new();
        exports.insert("a".to_string(), PhsValue::Number(42.0));
        exports.insert("b".to_string(), PhsValue::String("hello".to_string()));
        exports.insert("c".to_string(), PhsValue::Quantity(Quantity::new(10.0, "m/s").unwrap()));
        
        let json = Exporter::export_json(&exports).unwrap();
        assert_eq!(json, "{\"a\": 42, \"b\": \"hello\", \"c\": {\"value\": 10, \"unit\": \"m/s\"}}");
    }

    #[test]
    fn test_export_csv() {
        let mut exports = HashMap::new();
        exports.insert("a".to_string(), PhsValue::Number(42.0));
        exports.insert("b".to_string(), PhsValue::String("hello".to_string()));
        exports.insert("c".to_string(), PhsValue::Quantity(Quantity::new(10.0, "m/s").unwrap()));
        
        let csv = Exporter::export_csv(&exports).unwrap();
        assert_eq!(csv, "key,value\na,42\nb,\"hello\"\nc,10 m/s\n");
    }
}
