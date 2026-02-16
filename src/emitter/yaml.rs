//! YAML emitter for Hone values

use super::Emitter;
use crate::errors::HoneResult;
use crate::evaluator::Value;

/// YAML output emitter
pub struct YamlEmitter {
    /// Indentation string
    indent: String,
}

impl Default for YamlEmitter {
    fn default() -> Self {
        Self::new()
    }
}

impl YamlEmitter {
    /// Create a new YAML emitter
    pub fn new() -> Self {
        Self {
            indent: "  ".to_string(),
        }
    }

    /// Create a YAML emitter with custom indentation
    pub fn with_indent(indent: impl Into<String>) -> Self {
        Self {
            indent: indent.into(),
        }
    }

    /// Emit a value at the given depth
    fn emit_value(&self, value: &Value, depth: usize, inline: bool) -> String {
        match value {
            Value::Null => "null".to_string(),
            Value::Bool(b) => if *b { "true" } else { "false" }.to_string(),
            Value::Int(n) => n.to_string(),
            Value::Float(n) => {
                if n.is_infinite() {
                    if n.is_sign_positive() {
                        ".inf"
                    } else {
                        "-.inf"
                    }
                    .to_string()
                } else if n.is_nan() {
                    ".nan".to_string()
                } else if n.fract() == 0.0 {
                    format!("{:.1}", n)
                } else {
                    n.to_string()
                }
            }
            Value::String(s) if s.contains('\n') && !inline => self.emit_block_string(s, depth),
            Value::String(s) => self.escape_string(s),
            Value::Array(arr) => self.emit_array(arr, depth, inline),
            Value::Object(obj) => self.emit_object(obj, depth, inline),
        }
    }

    /// Emit a multiline string using YAML literal block style (|)
    fn emit_block_string(&self, s: &str, depth: usize) -> String {
        let indent = self.indent.repeat(depth + 1);
        let chomp = if s.ends_with('\n') { "" } else { "-" };
        let mut result = format!("|{}\n", chomp);
        for line in s.split('\n') {
            if line.is_empty() {
                result.push('\n');
            } else {
                result.push_str(&indent);
                result.push_str(line);
                result.push('\n');
            }
        }
        // Remove the trailing newline we added for the last line
        // (the chomp indicator handles whether the original had one)
        if !s.ends_with('\n') {
            result.pop();
        }
        result
    }

    /// Escape a string for YAML
    fn escape_string(&self, s: &str) -> String {
        // Check if we need quoting
        let needs_quotes = s.is_empty()
            || s.starts_with(' ')
            || s.ends_with(' ')
            || s.contains(':')
            || s.contains('#')
            || s.contains('\n')
            || s.contains('"')
            || s.contains('\'')
            || s.starts_with('@')
            || s.starts_with('`')
            || s.starts_with('|')
            || s.starts_with('>')
            || s.starts_with('!')
            || s.starts_with('*')
            || s.starts_with('&')
            || s.starts_with('{')
            || s.starts_with('[')
            || s.starts_with('%')
            || self.looks_like_number(s)
            || self.looks_like_bool(s)
            || s == "null"
            || s == "~";

        if !needs_quotes {
            return s.to_string();
        }

        // Use double quotes and escape special chars
        let mut result = String::with_capacity(s.len() + 2);
        result.push('"');

        for ch in s.chars() {
            match ch {
                '"' => result.push_str("\\\""),
                '\\' => result.push_str("\\\\"),
                '\n' => result.push_str("\\n"),
                '\r' => result.push_str("\\r"),
                '\t' => result.push_str("\\t"),
                c => result.push(c),
            }
        }

        result.push('"');
        result
    }

    /// Check if a string looks like a number
    fn looks_like_number(&self, s: &str) -> bool {
        s.parse::<f64>().is_ok() || s.parse::<i64>().is_ok()
    }

    /// Check if a string looks like a boolean
    fn looks_like_bool(&self, s: &str) -> bool {
        matches!(
            s.to_lowercase().as_str(),
            "true" | "false" | "yes" | "no" | "on" | "off"
        )
    }

    /// Emit an array
    fn emit_array(&self, arr: &[Value], depth: usize, inline: bool) -> String {
        if arr.is_empty() {
            return "[]".to_string();
        }

        // Use inline format for simple arrays
        if inline || self.is_simple_array(arr) {
            let items: Vec<_> = arr
                .iter()
                .map(|v| self.emit_value(v, depth, true))
                .collect();
            return format!("[{}]", items.join(", "));
        }

        // Block format
        let indent = self.indent.repeat(depth);
        let mut result = String::new();

        for item in arr {
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str(&indent);
            result.push_str("- ");

            // Handle nested structures
            match item {
                Value::Object(obj) if !obj.is_empty() => {
                    // For objects in arrays, emit inline-style on first line
                    // then continue with proper indentation
                    result.push_str(&self.emit_object_as_array_item(obj, depth + 1));
                }
                Value::Array(inner) if !inner.is_empty() && !self.is_simple_array(inner) => {
                    result.push('\n');
                    result.push_str(&self.emit_array(inner, depth + 1, false));
                }
                _ => {
                    result.push_str(&self.emit_value(item, depth + 1, true));
                }
            }
        }

        result
    }

    /// Emit an object as an array item (special formatting for YAML)
    fn emit_object_as_array_item(
        &self,
        obj: &indexmap::IndexMap<String, Value>,
        depth: usize,
    ) -> String {
        if obj.is_empty() {
            return "{}".to_string();
        }

        let indent = self.indent.repeat(depth);
        let mut result = String::new();
        let mut first = true;

        for (key, value) in obj {
            if !first {
                result.push('\n');
                result.push_str(&indent);
            }
            first = false;

            result.push_str(&self.escape_key(key));
            result.push(':');

            match value {
                Value::Object(inner) if !inner.is_empty() => {
                    result.push('\n');
                    result.push_str(&self.emit_object(inner, depth + 1, false));
                }
                Value::Array(inner) if !inner.is_empty() && !self.is_simple_array(inner) => {
                    result.push('\n');
                    result.push_str(&self.emit_array(inner, depth + 1, false));
                }
                Value::String(s) if s.contains('\n') => {
                    result.push(' ');
                    result.push_str(&self.emit_block_string(s, depth));
                }
                _ => {
                    result.push(' ');
                    result.push_str(&self.emit_value(value, depth + 1, true));
                }
            }
        }

        result
    }

    /// Emit an object
    fn emit_object(
        &self,
        obj: &indexmap::IndexMap<String, Value>,
        depth: usize,
        inline: bool,
    ) -> String {
        if obj.is_empty() {
            return "{}".to_string();
        }

        // Use inline format for simple objects
        if inline && self.is_simple_object(obj) {
            let items: Vec<_> = obj
                .iter()
                .map(|(k, v)| {
                    format!(
                        "{}: {}",
                        self.escape_key(k),
                        self.emit_value(v, depth, true)
                    )
                })
                .collect();
            return format!("{{{}}}", items.join(", "));
        }

        // Block format
        let indent = self.indent.repeat(depth);
        let mut result = String::new();

        for (key, value) in obj {
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str(&indent);
            result.push_str(&self.escape_key(key));
            result.push(':');

            match value {
                Value::Object(inner) if !inner.is_empty() => {
                    result.push('\n');
                    result.push_str(&self.emit_object(inner, depth + 1, false));
                }
                Value::Array(inner) if !inner.is_empty() && !self.is_simple_array(inner) => {
                    result.push('\n');
                    result.push_str(&self.emit_array(inner, depth + 1, false));
                }
                Value::String(s) if s.contains('\n') => {
                    result.push(' ');
                    result.push_str(&self.emit_block_string(s, depth));
                }
                _ => {
                    result.push(' ');
                    result.push_str(&self.emit_value(value, depth + 1, true));
                }
            }
        }

        result
    }

    /// Escape a key for YAML
    fn escape_key(&self, key: &str) -> String {
        // Keys need quoting for special chars, bool-like, number-like, or null-like values
        let needs_quotes = key.is_empty()
            || key.contains(':')
            || key.contains('#')
            || key.contains(' ')
            || key.starts_with('"')
            || key.starts_with('\'')
            || key.starts_with('{')
            || key.starts_with('[')
            || key.starts_with('!')
            || key.starts_with('&')
            || key.starts_with('*')
            || key.starts_with('@')
            || key.starts_with('`')
            || key.starts_with('|')
            || key.starts_with('>')
            || key.starts_with('%')
            || self.looks_like_number(key)
            || self.looks_like_bool(key)
            || key == "null"
            || key == "~";

        if needs_quotes {
            format!("\"{}\"", key.replace('"', "\\\""))
        } else {
            key.to_string()
        }
    }

    /// Check if an array contains only simple values
    fn is_simple_array(&self, arr: &[Value]) -> bool {
        arr.len() <= 5 && arr.iter().all(|v| self.is_simple_value(v))
    }

    /// Check if an object is simple enough for inline format
    fn is_simple_object(&self, obj: &indexmap::IndexMap<String, Value>) -> bool {
        obj.len() <= 2 && obj.values().all(|v| self.is_simple_value(v))
    }

    /// Check if a value is simple (scalar or small)
    fn is_simple_value(&self, value: &Value) -> bool {
        match value {
            Value::Null | Value::Bool(_) | Value::Int(_) | Value::Float(_) => true,
            Value::String(s) => s.len() <= 50,
            Value::Array(arr) => arr.is_empty(),
            Value::Object(obj) => obj.is_empty(),
        }
    }
}

impl Emitter for YamlEmitter {
    fn emit(&self, value: &Value) -> HoneResult<String> {
        let result = match value {
            Value::Object(obj) if !obj.is_empty() => self.emit_object(obj, 0, false),
            Value::Array(arr) if !arr.is_empty() => self.emit_array(arr, 0, false),
            _ => self.emit_value(value, 0, false),
        };
        Ok(result)
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
        let emitter = YamlEmitter::new();
        assert_eq!(emitter.emit(&Value::Null).unwrap(), "null");
    }

    #[test]
    fn test_emit_bool() {
        let emitter = YamlEmitter::new();
        assert_eq!(emitter.emit(&Value::Bool(true)).unwrap(), "true");
        assert_eq!(emitter.emit(&Value::Bool(false)).unwrap(), "false");
    }

    #[test]
    fn test_emit_int() {
        let emitter = YamlEmitter::new();
        assert_eq!(emitter.emit(&Value::Int(42)).unwrap(), "42");
    }

    #[test]
    fn test_emit_float() {
        let emitter = YamlEmitter::new();
        assert_eq!(emitter.emit(&Value::Float(3.14)).unwrap(), "3.14");
    }

    #[test]
    fn test_emit_string() {
        let emitter = YamlEmitter::new();
        assert_eq!(
            emitter.emit(&Value::String("hello".into())).unwrap(),
            "hello"
        );
        assert_eq!(
            emitter.emit(&Value::String("hello world".into())).unwrap(),
            "hello world"
        );
    }

    #[test]
    fn test_emit_string_needs_quotes() {
        let emitter = YamlEmitter::new();
        // Colon needs quotes
        assert!(emitter
            .emit(&Value::String("key: value".into()))
            .unwrap()
            .starts_with('"'));
        // Number-like strings need quotes
        assert!(emitter
            .emit(&Value::String("123".into()))
            .unwrap()
            .starts_with('"'));
        // Boolean-like strings need quotes
        assert!(emitter
            .emit(&Value::String("true".into()))
            .unwrap()
            .starts_with('"'));
    }

    #[test]
    fn test_emit_simple_array() {
        let emitter = YamlEmitter::new();
        let arr = Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        assert_eq!(emitter.emit(&arr).unwrap(), "[1, 2, 3]");
    }

    #[test]
    fn test_emit_complex_array() {
        let emitter = YamlEmitter::new();
        let arr = Value::Array(vec![
            obj(&[("name", Value::String("a".into()))]),
            obj(&[("name", Value::String("b".into()))]),
        ]);
        let result = emitter.emit(&arr).unwrap();
        // Arrays of small objects use inline format for the nested object
        assert!(result.contains("- ") || result.contains("["));
    }

    #[test]
    fn test_emit_object() {
        let emitter = YamlEmitter::new();
        let obj = obj(&[
            ("host", Value::String("localhost".into())),
            ("port", Value::Int(8080)),
        ]);
        let result = emitter.emit(&obj).unwrap();
        assert!(result.contains("host: localhost"));
        assert!(result.contains("port: 8080"));
    }

    #[test]
    fn test_emit_nested_object() {
        let emitter = YamlEmitter::new();
        let obj = obj(&[(
            "server",
            obj(&[
                ("host", Value::String("localhost".into())),
                ("port", Value::Int(8080)),
            ]),
        )]);
        let result = emitter.emit(&obj).unwrap();
        assert!(result.contains("server:"));
        assert!(result.contains("  host: localhost"));
        assert!(result.contains("  port: 8080"));
    }

    #[test]
    fn test_emit_empty() {
        let emitter = YamlEmitter::new();
        assert_eq!(emitter.emit(&Value::Array(vec![])).unwrap(), "[]");
        assert_eq!(emitter.emit(&Value::Object(IndexMap::new())).unwrap(), "{}");
    }

    #[test]
    fn test_emit_special_floats() {
        let emitter = YamlEmitter::new();
        assert_eq!(emitter.emit(&Value::Float(f64::INFINITY)).unwrap(), ".inf");
        assert_eq!(
            emitter.emit(&Value::Float(f64::NEG_INFINITY)).unwrap(),
            "-.inf"
        );
        assert_eq!(emitter.emit(&Value::Float(f64::NAN)).unwrap(), ".nan");
    }

    #[test]
    fn test_emit_block_string() {
        let emitter = YamlEmitter::new();
        // Multiline string in an object should use block style
        let obj = obj(&[(
            "script",
            Value::String("#!/bin/bash\necho hello\nexit 0".into()),
        )]);
        let result = emitter.emit(&obj).unwrap();
        assert!(
            result.contains("script: |-"),
            "Expected block style, got: {}",
            result
        );
        assert!(result.contains("  #!/bin/bash"));
        assert!(result.contains("  echo hello"));
        assert!(result.contains("  exit 0"));
    }

    #[test]
    fn test_emit_block_string_trailing_newline() {
        let emitter = YamlEmitter::new();
        // String ending with newline uses | (not |-)
        let obj = obj(&[("content", Value::String("line1\nline2\n".into()))]);
        let result = emitter.emit(&obj).unwrap();
        assert!(
            result.contains("content: |\n"),
            "Expected | chomp, got: {}",
            result
        );
    }

    #[test]
    fn test_emit_block_string_nested() {
        let emitter = YamlEmitter::new();
        // Block string inside nested object gets proper indentation
        let obj = obj(&[(
            "config",
            obj(&[("script", Value::String("line1\nline2".into()))]),
        )]);
        let result = emitter.emit(&obj).unwrap();
        assert!(
            result.contains("script: |-"),
            "Expected block style, got: {}",
            result
        );
        assert!(result.contains("    line1"));
        assert!(result.contains("    line2"));
    }

    #[test]
    fn test_emit_multiline_string_inline_stays_escaped() {
        let emitter = YamlEmitter::new();
        // In inline context (arrays), multiline strings should stay escaped
        let arr = Value::Array(vec![Value::String("a\nb".into())]);
        let result = emitter.emit(&arr).unwrap();
        // Simple array uses inline format, so newline should be escaped
        assert!(
            result.contains("\\n"),
            "Expected escaped newline in inline context, got: {}",
            result
        );
    }

    #[test]
    fn test_emit_key_quoting() {
        let emitter = YamlEmitter::new();
        // Keys that look like booleans need quoting
        let obj_bool = obj(&[("yes", Value::String("value".into()))]);
        assert!(emitter.emit(&obj_bool).unwrap().contains("\"yes\":"));

        // Keys that look like numbers need quoting
        let obj_num = obj(&[("123", Value::String("value".into()))]);
        assert!(emitter.emit(&obj_num).unwrap().contains("\"123\":"));

        // Keys that look like null need quoting
        let obj_null = obj(&[("null", Value::String("value".into()))]);
        assert!(emitter.emit(&obj_null).unwrap().contains("\"null\":"));

        // Normal keys don't need quoting
        let obj_normal = obj(&[("name", Value::String("value".into()))]);
        let result = emitter.emit(&obj_normal).unwrap();
        assert!(result.contains("name:"));
        assert!(!result.contains("\"name\""));
    }
}
