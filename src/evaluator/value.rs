//! Runtime values for the Hone evaluator
//!
//! Values are the result of evaluating Hone expressions.
//! They can be serialized to JSON or YAML.

use indexmap::IndexMap;
use std::fmt;

/// A runtime value in Hone
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Null value
    Null,
    /// Boolean
    Bool(bool),
    /// Integer (64-bit signed)
    Int(i64),
    /// Floating point (64-bit)
    Float(f64),
    /// String
    String(String),
    /// Array of values
    Array(Vec<Value>),
    /// Object (ordered map of string keys to values)
    Object(IndexMap<String, Value>),
}

impl Value {
    /// Get the type name of this value
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Bool(_) => "bool",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        }
    }

    /// Check if this value is null
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// Check if this value is truthy
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Int(n) => *n != 0,
            Value::Float(n) => *n != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::Array(a) => !a.is_empty(),
            Value::Object(o) => !o.is_empty(),
        }
    }

    /// Check if this value is an empty object
    pub fn is_empty_object(&self) -> bool {
        matches!(self, Value::Object(o) if o.is_empty())
    }

    /// Try to get as boolean
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Try to get as integer
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(n) => Some(*n),
            _ => None,
        }
    }

    /// Try to get as float (converts int to float)
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Value::Float(n) => Some(*n),
            Value::Int(n) => Some(*n as f64),
            _ => None,
        }
    }

    /// Try to get as string
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// Try to get as array
    pub fn as_array(&self) -> Option<&Vec<Value>> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    /// Try to get as mutable array
    pub fn as_array_mut(&mut self) -> Option<&mut Vec<Value>> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    /// Try to get as object
    pub fn as_object(&self) -> Option<&IndexMap<String, Value>> {
        match self {
            Value::Object(o) => Some(o),
            _ => None,
        }
    }

    /// Try to get as mutable object
    pub fn as_object_mut(&mut self) -> Option<&mut IndexMap<String, Value>> {
        match self {
            Value::Object(o) => Some(o),
            _ => None,
        }
    }

    /// Get a value by key path (e.g., "server.port")
    pub fn get_path(&self, path: &[&str]) -> Option<&Value> {
        let mut current = self;
        for key in path {
            match current {
                Value::Object(obj) => {
                    current = obj.get(*key)?;
                }
                Value::Array(arr) => {
                    let idx: usize = key.parse().ok()?;
                    current = arr.get(idx)?;
                }
                _ => return None,
            }
        }
        Some(current)
    }

    /// Set a value by key path, creating intermediate objects as needed
    pub fn set_path(&mut self, path: &[&str], value: Value) -> bool {
        if path.is_empty() {
            return false;
        }

        if path.len() == 1 {
            if let Value::Object(obj) = self {
                obj.insert(path[0].to_string(), value);
                return true;
            }
            return false;
        }

        // Navigate to parent
        let parent_path = &path[..path.len() - 1];
        let key = path[path.len() - 1];

        let mut current = self;
        for segment in parent_path {
            match current {
                Value::Object(obj) => {
                    // Create intermediate object if needed
                    if !obj.contains_key(*segment) {
                        obj.insert(segment.to_string(), Value::Object(IndexMap::new()));
                    }
                    current = obj.get_mut(*segment).unwrap();
                }
                _ => return false,
            }
        }

        if let Value::Object(obj) = current {
            obj.insert(key.to_string(), value);
            true
        } else {
            false
        }
    }

    /// Convert to a number (int or float)
    pub fn to_number(&self) -> Option<f64> {
        match self {
            Value::Int(n) => Some(*n as f64),
            Value::Float(n) => Some(*n),
            _ => None,
        }
    }

    /// Check equality with type coercion for numbers
    pub fn equals(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Float(b)) => (*a as f64) == *b,
            (Value::Float(a), Value::Int(b)) => *a == (*b as f64),
            _ => self == other,
        }
    }

    /// Convert a Hone Value to serde_json::Value
    pub fn to_serde_json(&self) -> serde_json::Value {
        match self {
            Value::Null => serde_json::Value::Null,
            Value::Bool(b) => serde_json::Value::Bool(*b),
            Value::Int(n) => serde_json::Value::Number(serde_json::Number::from(*n)),
            Value::Float(n) => serde_json::Number::from_f64(*n)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
            Value::String(s) => serde_json::Value::String(s.clone()),
            Value::Array(arr) => {
                serde_json::Value::Array(arr.iter().map(|v| v.to_serde_json()).collect())
            }
            Value::Object(obj) => {
                let map: serde_json::Map<String, serde_json::Value> = obj
                    .iter()
                    .map(|(k, v)| (k.clone(), v.to_serde_json()))
                    .collect();
                serde_json::Value::Object(map)
            }
        }
    }

    /// Convert a serde_json::Value to Hone Value
    pub fn from_serde_json(json: serde_json::Value) -> Value {
        match json {
            serde_json::Value::Null => Value::Null,
            serde_json::Value::Bool(b) => Value::Bool(b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Value::Int(i)
                } else {
                    Value::Float(n.as_f64().unwrap_or(0.0))
                }
            }
            serde_json::Value::String(s) => Value::String(s),
            serde_json::Value::Array(arr) => {
                Value::Array(arr.into_iter().map(Value::from_serde_json).collect())
            }
            serde_json::Value::Object(obj) => {
                let mut map = IndexMap::new();
                for (k, v) in obj {
                    map.insert(k, Value::from_serde_json(v));
                }
                Value::Object(map)
            }
        }
    }
}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match (self, other) {
            (Value::Null, Value::Null) => Some(std::cmp::Ordering::Equal),
            (Value::Bool(a), Value::Bool(b)) => a.partial_cmp(b),
            (Value::Int(a), Value::Int(b)) => a.partial_cmp(b),
            (Value::Float(a), Value::Float(b)) => a.partial_cmp(b),
            (Value::Int(a), Value::Float(b)) => (*a as f64).partial_cmp(b),
            (Value::Float(a), Value::Int(b)) => a.partial_cmp(&(*b as f64)),
            (Value::String(a), Value::String(b)) => a.partial_cmp(b),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null => write!(f, "null"),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Int(n) => write!(f, "{}", n),
            Value::Float(n) => {
                if n.fract() == 0.0 {
                    write!(f, "{}.0", n)
                } else {
                    write!(f, "{}", n)
                }
            }
            Value::String(s) => write!(f, "{}", s),
            Value::Array(arr) => {
                write!(f, "[")?;
                for (i, v) in arr.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", v)?;
                }
                write!(f, "]")
            }
            Value::Object(obj) => {
                write!(f, "{{")?;
                for (i, (k, v)) in obj.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", k, v)?;
                }
                write!(f, "}}")
            }
        }
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

impl From<i64> for Value {
    fn from(n: i64) -> Self {
        Value::Int(n)
    }
}

impl From<i32> for Value {
    fn from(n: i32) -> Self {
        Value::Int(n as i64)
    }
}

impl From<f64> for Value {
    fn from(n: f64) -> Self {
        Value::Float(n)
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::String(s)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::String(s.to_string())
    }
}

impl<T: Into<Value>> From<Vec<T>> for Value {
    fn from(v: Vec<T>) -> Self {
        Value::Array(v.into_iter().map(Into::into).collect())
    }
}

impl From<IndexMap<String, Value>> for Value {
    fn from(m: IndexMap<String, Value>) -> Self {
        Value::Object(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_names() {
        assert_eq!(Value::Null.type_name(), "null");
        assert_eq!(Value::Bool(true).type_name(), "bool");
        assert_eq!(Value::Int(42).type_name(), "int");
        assert_eq!(Value::Float(3.14).type_name(), "float");
        assert_eq!(Value::String("hello".into()).type_name(), "string");
        assert_eq!(Value::Array(vec![]).type_name(), "array");
        assert_eq!(Value::Object(IndexMap::new()).type_name(), "object");
    }

    #[test]
    fn test_truthiness() {
        assert!(!Value::Null.is_truthy());
        assert!(!Value::Bool(false).is_truthy());
        assert!(Value::Bool(true).is_truthy());
        assert!(!Value::Int(0).is_truthy());
        assert!(Value::Int(1).is_truthy());
        assert!(!Value::String("".into()).is_truthy());
        assert!(Value::String("hello".into()).is_truthy());
        assert!(!Value::Array(vec![]).is_truthy());
        assert!(Value::Array(vec![Value::Int(1)]).is_truthy());
    }

    #[test]
    fn test_get_path() {
        let mut obj = IndexMap::new();
        let mut server = IndexMap::new();
        server.insert("port".to_string(), Value::Int(8080));
        server.insert("host".to_string(), Value::String("localhost".into()));
        obj.insert("server".to_string(), Value::Object(server));

        let value = Value::Object(obj);

        assert_eq!(value.get_path(&["server", "port"]), Some(&Value::Int(8080)));
        assert_eq!(
            value.get_path(&["server", "host"]),
            Some(&Value::String("localhost".into()))
        );
        assert_eq!(value.get_path(&["nonexistent"]), None);
    }

    #[test]
    fn test_set_path() {
        let mut value = Value::Object(IndexMap::new());

        value.set_path(&["server", "port"], Value::Int(8080));
        assert_eq!(value.get_path(&["server", "port"]), Some(&Value::Int(8080)));
    }

    #[test]
    fn test_number_coercion() {
        assert!(Value::Int(42).equals(&Value::Float(42.0)));
        assert!(Value::Float(42.0).equals(&Value::Int(42)));
        assert!(!Value::Int(42).equals(&Value::Float(42.1)));
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", Value::Null), "null");
        assert_eq!(format!("{}", Value::Bool(true)), "true");
        assert_eq!(format!("{}", Value::Int(42)), "42");
        assert_eq!(format!("{}", Value::Float(3.14)), "3.14");
        assert_eq!(format!("{}", Value::String("hello".into())), "hello");
    }

    #[test]
    fn test_from_conversions() {
        let _: Value = true.into();
        let _: Value = 42i64.into();
        let _: Value = 3.14f64.into();
        let _: Value = "hello".into();
        let _: Value = vec![1i64, 2, 3].into();
    }
}
