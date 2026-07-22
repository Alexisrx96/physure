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
            json.push_str(&format!("\"{}\": {}", k, value_to_json(v)));
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
            csv.push_str(&format!("{},{}\n", k, value_to_csv(v)));
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
            py.push_str(&format!("'{}': {}", k, value_to_py(v)));
        }
        py.push('}');
        Ok(py)
    }
}

fn value_to_json(v: &PhsValue) -> String {
    match v {
        PhsValue::None => "null".to_string(),
        PhsValue::Number(n) => format!("{}", n),
        PhsValue::Bool(b) => if *b { "true".to_string() } else { "false".to_string() },
        PhsValue::String(s) => format!("\"{}\"", s),
        PhsValue::Quantity(q) => format!("{{\"value\": {}, \"unit\": \"{}\"}}", q.value.mean(), q.unit.__repr__()),
        PhsValue::Function(_) => "\"<function>\"".to_string(),
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
    }
}

fn value_to_csv(v: &PhsValue) -> String {
    match v {
        PhsValue::None => "".to_string(),
        PhsValue::Number(n) => format!("{}", n),
        PhsValue::Bool(b) => if *b { "true".to_string() } else { "false".to_string() },
        PhsValue::String(s) => format!("\"{}\"", s),
        PhsValue::Quantity(q) => {
            let unit = q.unit.__repr__();
            if unit.is_empty() {
                format!("{}", q.value.mean())
            } else {
                format!("{} {}", q.value.mean(), unit)
            }
        },
        PhsValue::Function(_) => "<function>".to_string(),
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
    }
}

fn value_to_py(v: &PhsValue) -> String {
    match v {
        PhsValue::None => "None".to_string(),
        PhsValue::Number(n) => format!("{}", n),
        PhsValue::Bool(b) => if *b { "True".to_string() } else { "False".to_string() },
        PhsValue::String(s) => format!("'{}'", s),
        PhsValue::Quantity(q) => format!("{{'value': {}, 'unit': '{}'}}", q.value.mean(), q.unit.__repr__()),
        PhsValue::Function(_) => "'<function>'".to_string(),
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
