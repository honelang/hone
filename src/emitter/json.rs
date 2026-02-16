//! JSON emitter for Hone values

use super::Emitter;
use crate::errors::HoneResult;
use crate::evaluator::Value;

/// JSON output emitter
pub struct JsonEmitter {
    /// Whether to pretty-print with indentation
    pretty: bool,
    /// Indentation string (spaces or tabs)
    indent: String,
}

impl JsonEmitter {
    /// Create a new JSON emitter
    pub fn new(pretty: bool) -> Self {
        Self {
            pretty,
            indent: "  ".to_string(),
        }
    }

    /// Create a JSON emitter with custom indentation
    pub fn with_indent(indent: impl Into<String>) -> Self {
        Self {
            pretty: true,
            indent: indent.into(),
        }
    }

    /// Emit a value with the given depth
    fn emit_value(&self, value: &Value, depth: usize) -> String {
        match value {
            Value::Null => "null".to_string(),
            Value::Bool(b) => if *b { "true" } else { "false" }.to_string(),
            Value::Int(n) => n.to_string(),
            Value::Float(n) => {
                if n.is_infinite() || n.is_nan() {
                    eprintln!(
                        "warning: non-finite float ({}) converted to null in JSON output; use --format yaml for non-finite float support",
                        if n.is_nan() { "NaN".to_string() } else { format!("{}", n) }
                    );
                    "null".to_string()
                } else if n.fract() == 0.0 {
                    format!("{:.1}", n)
                } else {
                    n.to_string()
                }
            }
            Value::String(s) => self.escape_string(s),
            Value::Array(arr) => self.emit_array(arr, depth),
            Value::Object(obj) => self.emit_object(obj, depth),
        }
    }

    /// Escape a string for JSON
    fn escape_string(&self, s: &str) -> String {
        let mut result = String::with_capacity(s.len() + 2);
        result.push('"');

        for ch in s.chars() {
            match ch {
                '"' => result.push_str("\\\""),
                '\\' => result.push_str("\\\\"),
                '\n' => result.push_str("\\n"),
                '\r' => result.push_str("\\r"),
                '\t' => result.push_str("\\t"),
                c if c.is_control() => {
                    result.push_str(&format!("\\u{:04x}", c as u32));
                }
                c => result.push(c),
            }
        }

        result.push('"');
        result
    }

    /// Emit an array
    fn emit_array(&self, arr: &[Value], depth: usize) -> String {
        if arr.is_empty() {
            return "[]".to_string();
        }

        if self.pretty {
            let mut result = String::from("[\n");
            let inner_indent = self.indent.repeat(depth + 1);
            let outer_indent = self.indent.repeat(depth);

            for (i, item) in arr.iter().enumerate() {
                result.push_str(&inner_indent);
                result.push_str(&self.emit_value(item, depth + 1));
                if i < arr.len() - 1 {
                    result.push(',');
                }
                result.push('\n');
            }

            result.push_str(&outer_indent);
            result.push(']');
            result
        } else {
            let items: Vec<_> = arr.iter().map(|v| self.emit_value(v, depth + 1)).collect();
            format!("[{}]", items.join(","))
        }
    }

    /// Emit an object
    fn emit_object(&self, obj: &indexmap::IndexMap<String, Value>, depth: usize) -> String {
        if obj.is_empty() {
            return "{}".to_string();
        }

        if self.pretty {
            let mut result = String::from("{\n");
            let inner_indent = self.indent.repeat(depth + 1);
            let outer_indent = self.indent.repeat(depth);

            for (i, (key, value)) in obj.iter().enumerate() {
                result.push_str(&inner_indent);
                result.push_str(&self.escape_string(key));
                result.push_str(": ");
                result.push_str(&self.emit_value(value, depth + 1));
                if i < obj.len() - 1 {
                    result.push(',');
                }
                result.push('\n');
            }

            result.push_str(&outer_indent);
            result.push('}');
            result
        } else {
            let items: Vec<_> = obj
                .iter()
                .map(|(k, v)| {
                    format!(
                        "{}:{}",
                        self.escape_string(k),
                        self.emit_value(v, depth + 1)
                    )
                })
                .collect();
            format!("{{{}}}", items.join(","))
        }
    }
}

impl Emitter for JsonEmitter {
    fn emit(&self, value: &Value) -> HoneResult<String> {
        Ok(self.emit_value(value, 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    fn obj(pairs: &[(&str, Value)]) -> Value {
        let mut map = IndexMap::new();
        for (k, v) in pairs {
            map.insert(k.to_string(), v.clone());
        }
        Value::Object(map)
    }

    #[test]
    fn test_emit_null() {
        let emitter = JsonEmitter::new(false);
        assert_eq!(emitter.emit(&Value::Null).unwrap(), "null");
    }

    #[test]
    fn test_emit_bool() {
        let emitter = JsonEmitter::new(false);
        assert_eq!(emitter.emit(&Value::Bool(true)).unwrap(), "true");
        assert_eq!(emitter.emit(&Value::Bool(false)).unwrap(), "false");
    }

    #[test]
    fn test_emit_int() {
        let emitter = JsonEmitter::new(false);
        assert_eq!(emitter.emit(&Value::Int(42)).unwrap(), "42");
        assert_eq!(emitter.emit(&Value::Int(-123)).unwrap(), "-123");
    }

    #[test]
    fn test_emit_float() {
        let emitter = JsonEmitter::new(false);
        assert_eq!(emitter.emit(&Value::Float(3.14)).unwrap(), "3.14");
        assert_eq!(emitter.emit(&Value::Float(2.0)).unwrap(), "2.0");
    }

    #[test]
    fn test_emit_string() {
        let emitter = JsonEmitter::new(false);
        assert_eq!(
            emitter.emit(&Value::String("hello".into())).unwrap(),
            r#""hello""#
        );
        assert_eq!(
            emitter.emit(&Value::String("line\nbreak".into())).unwrap(),
            r#""line\nbreak""#
        );
        assert_eq!(
            emitter.emit(&Value::String("quote\"test".into())).unwrap(),
            r#""quote\"test""#
        );
    }

    #[test]
    fn test_emit_array() {
        let emitter = JsonEmitter::new(false);
        let arr = Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        assert_eq!(emitter.emit(&arr).unwrap(), "[1,2,3]");
    }

    #[test]
    fn test_emit_array_pretty() {
        let emitter = JsonEmitter::new(true);
        let arr = Value::Array(vec![Value::Int(1), Value::Int(2)]);
        let expected = "[\n  1,\n  2\n]";
        assert_eq!(emitter.emit(&arr).unwrap(), expected);
    }

    #[test]
    fn test_emit_object() {
        let emitter = JsonEmitter::new(false);
        let obj = obj(&[("a", Value::Int(1)), ("b", Value::Int(2))]);
        assert_eq!(emitter.emit(&obj).unwrap(), r#"{"a":1,"b":2}"#);
    }

    #[test]
    fn test_emit_object_pretty() {
        let emitter = JsonEmitter::new(true);
        let obj = obj(&[("a", Value::Int(1))]);
        let expected = "{\n  \"a\": 1\n}";
        assert_eq!(emitter.emit(&obj).unwrap(), expected);
    }

    #[test]
    fn test_emit_nested() {
        let emitter = JsonEmitter::new(false);
        let obj = obj(&[(
            "server",
            obj(&[
                ("host", Value::String("localhost".into())),
                ("port", Value::Int(8080)),
            ]),
        )]);
        assert_eq!(
            emitter.emit(&obj).unwrap(),
            r#"{"server":{"host":"localhost","port":8080}}"#
        );
    }

    #[test]
    fn test_emit_empty() {
        let emitter = JsonEmitter::new(false);
        assert_eq!(emitter.emit(&Value::Array(vec![])).unwrap(), "[]");
        assert_eq!(emitter.emit(&Value::Object(IndexMap::new())).unwrap(), "{}");
    }
}
