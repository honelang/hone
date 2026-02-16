//! TOML emitter for Hone values

use super::Emitter;
use crate::errors::{HoneError, HoneResult};
use crate::evaluator::Value;

/// TOML output emitter
pub struct TomlEmitter;

impl TomlEmitter {
    pub fn new() -> Self {
        Self
    }

    /// Emit the top-level value as TOML
    fn emit_toplevel(&self, value: &Value) -> HoneResult<String> {
        match value {
            Value::Object(obj) => {
                let mut result = String::new();
                let mut tables = Vec::new();

                // First pass: emit simple key-value pairs at the top level
                for (key, val) in obj {
                    match val {
                        Value::Object(_) => {
                            tables.push((key.clone(), val.clone()));
                        }
                        Value::Array(arr)
                            if !arr.is_empty() && matches!(arr[0], Value::Object(_)) =>
                        {
                            tables.push((key.clone(), val.clone()));
                        }
                        _ => {
                            result.push_str(&self.escape_key(key));
                            result.push_str(" = ");
                            result.push_str(&self.emit_value(val)?);
                            result.push('\n');
                        }
                    }
                }

                // Second pass: emit tables
                for (key, val) in tables {
                    if !result.is_empty() && !result.ends_with("\n\n") {
                        result.push('\n');
                    }
                    match val {
                        Value::Object(ref inner) => {
                            self.emit_table(&mut result, std::slice::from_ref(&key), inner)?;
                        }
                        Value::Array(ref arr) => {
                            self.emit_array_of_tables(
                                &mut result,
                                std::slice::from_ref(&key),
                                arr,
                            )?;
                        }
                        _ => unreachable!(),
                    }
                }

                Ok(result)
            }
            _ => Err(HoneError::io_error(
                "TOML output requires a top-level object".to_string(),
            )),
        }
    }

    /// Emit a [table] section
    fn emit_table(
        &self,
        result: &mut String,
        path: &[String],
        obj: &indexmap::IndexMap<String, Value>,
    ) -> HoneResult<()> {
        let header = path
            .iter()
            .map(|k| self.escape_key(k))
            .collect::<Vec<_>>()
            .join(".");
        result.push_str(&format!("[{}]\n", header));

        let mut sub_tables = Vec::new();

        for (key, val) in obj {
            match val {
                Value::Object(_) => {
                    sub_tables.push((key.clone(), val.clone()));
                }
                Value::Array(arr) if !arr.is_empty() && matches!(arr[0], Value::Object(_)) => {
                    sub_tables.push((key.clone(), val.clone()));
                }
                _ => {
                    result.push_str(&self.escape_key(key));
                    result.push_str(" = ");
                    result.push_str(&self.emit_value(val)?);
                    result.push('\n');
                }
            }
        }

        for (key, val) in sub_tables {
            let mut sub_path = path.to_vec();
            sub_path.push(key.clone());
            if !result.ends_with("\n\n") {
                result.push('\n');
            }
            match val {
                Value::Object(ref inner) => {
                    self.emit_table(result, &sub_path, inner)?;
                }
                Value::Array(ref arr) => {
                    self.emit_array_of_tables(result, &sub_path, arr)?;
                }
                _ => unreachable!(),
            }
        }

        Ok(())
    }

    /// Emit [[array.of.tables]]
    fn emit_array_of_tables(
        &self,
        result: &mut String,
        path: &[String],
        arr: &[Value],
    ) -> HoneResult<()> {
        let header = path
            .iter()
            .map(|k| self.escape_key(k))
            .collect::<Vec<_>>()
            .join(".");

        for item in arr {
            match item {
                Value::Object(obj) => {
                    result.push_str(&format!("[[{}]]\n", header));
                    let mut sub_tables = Vec::new();

                    for (key, val) in obj {
                        match val {
                            Value::Object(_) => {
                                sub_tables.push((key.clone(), val.clone()));
                            }
                            Value::Array(inner_arr)
                                if !inner_arr.is_empty()
                                    && matches!(inner_arr[0], Value::Object(_)) =>
                            {
                                sub_tables.push((key.clone(), val.clone()));
                            }
                            _ => {
                                result.push_str(&self.escape_key(key));
                                result.push_str(" = ");
                                result.push_str(&self.emit_value(val)?);
                                result.push('\n');
                            }
                        }
                    }

                    for (key, val) in sub_tables {
                        let mut sub_path = path.to_vec();
                        sub_path.push(key.clone());
                        if !result.ends_with("\n\n") {
                            result.push('\n');
                        }
                        match val {
                            Value::Object(ref inner) => {
                                self.emit_table(result, &sub_path, inner)?;
                            }
                            Value::Array(ref inner_arr) => {
                                self.emit_array_of_tables(result, &sub_path, inner_arr)?;
                            }
                            _ => unreachable!(),
                        }
                    }

                    result.push('\n');
                }
                _ => {
                    return Err(HoneError::io_error(
                        "TOML array of tables requires all elements to be objects".to_string(),
                    ));
                }
            }
        }

        Ok(())
    }

    /// Emit a scalar or inline value
    fn emit_value(&self, value: &Value) -> HoneResult<String> {
        match value {
            Value::Null => Err(HoneError::io_error(
                "TOML does not support null values".to_string(),
            )),
            Value::Bool(b) => Ok(if *b { "true" } else { "false" }.to_string()),
            Value::Int(n) => Ok(n.to_string()),
            Value::Float(n) => {
                if n.is_infinite() {
                    Ok(if n.is_sign_positive() { "inf" } else { "-inf" }.to_string())
                } else if n.is_nan() {
                    Ok("nan".to_string())
                } else if n.fract() == 0.0 {
                    Ok(format!("{:.1}", n))
                } else {
                    Ok(n.to_string())
                }
            }
            Value::String(s) => Ok(self.escape_string(s)),
            Value::Array(arr) => self.emit_inline_array(arr),
            Value::Object(obj) => self.emit_inline_object(obj),
        }
    }

    /// Emit an inline array [a, b, c]
    fn emit_inline_array(&self, arr: &[Value]) -> HoneResult<String> {
        if arr.is_empty() {
            return Ok("[]".to_string());
        }
        let mut items = Vec::new();
        for val in arr {
            items.push(self.emit_value(val)?);
        }
        Ok(format!("[{}]", items.join(", ")))
    }

    /// Emit an inline table {key = val, ...}
    fn emit_inline_object(&self, obj: &indexmap::IndexMap<String, Value>) -> HoneResult<String> {
        if obj.is_empty() {
            return Ok("{}".to_string());
        }
        let mut items = Vec::new();
        for (key, val) in obj {
            items.push(format!(
                "{} = {}",
                self.escape_key(key),
                self.emit_value(val)?
            ));
        }
        Ok(format!("{{{}}}", items.join(", ")))
    }

    /// Escape a TOML key
    fn escape_key(&self, key: &str) -> String {
        if self.is_bare_key(key) {
            key.to_string()
        } else {
            self.escape_string(key)
        }
    }

    /// Check if a key can be used bare (unquoted) in TOML
    fn is_bare_key(&self, key: &str) -> bool {
        !key.is_empty()
            && key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    }

    /// Escape a string for TOML (basic string)
    fn escape_string(&self, s: &str) -> String {
        if s.contains('\n') {
            // Use multi-line basic string
            let mut result = String::from("\"\"\"\n");
            let mut consecutive_quotes = 0u32;
            for ch in s.chars() {
                match ch {
                    '"' => {
                        consecutive_quotes += 1;
                        if consecutive_quotes >= 3 {
                            // Escape to prevent closing the triple-quote delimiter
                            result.push_str("\\\"");
                            consecutive_quotes = 0;
                        } else {
                            result.push('"');
                        }
                    }
                    _ => {
                        consecutive_quotes = 0;
                        match ch {
                            '\\' => result.push_str("\\\\"),
                            '\r' => result.push_str("\\r"),
                            c if c.is_control() && c != '\n' => {
                                result.push_str(&format!("\\u{:04X}", c as u32));
                            }
                            c => result.push(c),
                        }
                    }
                }
            }
            result.push_str("\"\"\"");
            return result;
        }

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
                    result.push_str(&format!("\\u{:04X}", c as u32));
                }
                c => result.push(c),
            }
        }
        result.push('"');
        result
    }
}

impl Default for TomlEmitter {
    fn default() -> Self {
        Self::new()
    }
}

impl Emitter for TomlEmitter {
    fn emit(&self, value: &Value) -> HoneResult<String> {
        self.emit_toplevel(value)
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
    fn test_flat_values() {
        let emitter = TomlEmitter::new();
        let value = obj(&[
            ("name", Value::String("test".into())),
            ("port", Value::Int(8080)),
            ("debug", Value::Bool(true)),
            ("ratio", Value::Float(3.14)),
        ]);
        let result = emitter.emit(&value).unwrap();
        assert!(result.contains("name = \"test\""));
        assert!(result.contains("port = 8080"));
        assert!(result.contains("debug = true"));
        assert!(result.contains("ratio = 3.14"));
    }

    #[test]
    fn test_nested_objects() {
        let emitter = TomlEmitter::new();
        let value = obj(&[(
            "server",
            obj(&[
                ("host", Value::String("localhost".into())),
                ("port", Value::Int(8080)),
            ]),
        )]);
        let result = emitter.emit(&value).unwrap();
        assert!(result.contains("[server]"));
        assert!(result.contains("host = \"localhost\""));
        assert!(result.contains("port = 8080"));
    }

    #[test]
    fn test_deep_nested() {
        let emitter = TomlEmitter::new();
        let value = obj(&[(
            "database",
            obj(&[(
                "connection",
                obj(&[
                    ("host", Value::String("db.example.com".into())),
                    ("port", Value::Int(5432)),
                ]),
            )]),
        )]);
        let result = emitter.emit(&value).unwrap();
        assert!(result.contains("[database.connection]"));
        assert!(result.contains("host = \"db.example.com\""));
    }

    #[test]
    fn test_arrays() {
        let emitter = TomlEmitter::new();
        let value = obj(&[(
            "ports",
            Value::Array(vec![Value::Int(80), Value::Int(443), Value::Int(8080)]),
        )]);
        let result = emitter.emit(&value).unwrap();
        assert!(result.contains("ports = [80, 443, 8080]"));
    }

    #[test]
    fn test_array_of_objects() {
        let emitter = TomlEmitter::new();
        let value = obj(&[(
            "servers",
            Value::Array(vec![
                obj(&[
                    ("name", Value::String("alpha".into())),
                    ("port", Value::Int(8080)),
                ]),
                obj(&[
                    ("name", Value::String("beta".into())),
                    ("port", Value::Int(9090)),
                ]),
            ]),
        )]);
        let result = emitter.emit(&value).unwrap();
        assert!(result.contains("[[servers]]"));
        assert!(result.contains("name = \"alpha\""));
        assert!(result.contains("name = \"beta\""));
    }

    #[test]
    fn test_mixed_types() {
        let emitter = TomlEmitter::new();
        let value = obj(&[
            ("title", Value::String("My Config".into())),
            (
                "server",
                obj(&[
                    ("host", Value::String("localhost".into())),
                    ("port", Value::Int(8080)),
                ]),
            ),
        ]);
        let result = emitter.emit(&value).unwrap();
        assert!(result.contains("title = \"My Config\""));
        assert!(result.contains("[server]"));
    }

    #[test]
    fn test_null_error() {
        let emitter = TomlEmitter::new();
        let value = obj(&[("key", Value::Null)]);
        assert!(emitter.emit(&value).is_err());
    }

    #[test]
    fn test_non_object_toplevel_error() {
        let emitter = TomlEmitter::new();
        assert!(emitter.emit(&Value::Int(42)).is_err());
        assert!(emitter.emit(&Value::String("hello".into())).is_err());
    }

    #[test]
    fn test_empty_object() {
        let emitter = TomlEmitter::new();
        let value = Value::Object(IndexMap::new());
        let result = emitter.emit(&value).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_special_floats() {
        let emitter = TomlEmitter::new();
        let value = obj(&[
            ("pos_inf", Value::Float(f64::INFINITY)),
            ("neg_inf", Value::Float(f64::NEG_INFINITY)),
            ("nan", Value::Float(f64::NAN)),
        ]);
        let result = emitter.emit(&value).unwrap();
        assert!(result.contains("pos_inf = inf"));
        assert!(result.contains("neg_inf = -inf"));
        assert!(result.contains("nan = nan"));
    }

    #[test]
    fn test_string_escaping() {
        let emitter = TomlEmitter::new();
        let value = obj(&[
            ("path", Value::String("C:\\Users\\test".into())),
            ("quote", Value::String("say \"hello\"".into())),
        ]);
        let result = emitter.emit(&value).unwrap();
        assert!(result.contains(r#"path = "C:\\Users\\test""#));
        assert!(result.contains(r#"quote = "say \"hello\"""#));
    }

    #[test]
    fn test_multiline_string_with_triple_quotes() {
        let emitter = TomlEmitter::new();
        // String containing """ which would break multiline TOML strings
        let value = obj(&[(
            "template",
            Value::String("line1\nhas \"\"\" inside\nline3".into()),
        )]);
        let result = emitter.emit(&value).unwrap();
        // The output must be valid TOML - triple quotes in content must be escaped
        assert!(result.contains("\\\""));
        // Must not contain unescaped """ inside the multiline string body
        // (excluding the delimiters)
        let inner = result
            .find("\"\"\"\n")
            .map(|start| {
                let body_start = start + 4;
                let body_end = result[body_start..].find("\"\"\"").unwrap() + body_start;
                &result[body_start..body_end]
            })
            .unwrap();
        assert!(
            !inner.contains("\"\"\""),
            "multiline string body must not contain unescaped triple quotes: {}",
            inner
        );
    }

    #[test]
    fn test_multiline_string_trailing_quotes() {
        let emitter = TomlEmitter::new();
        // String ending with 1-2 quotes is valid per TOML spec:
        // `""""` = one content quote + `"""` close delimiter
        let value = obj(&[("ending", Value::String("line1\nends with \"".into()))]);
        let result = emitter.emit(&value).unwrap();
        assert!(
            result.contains("\"\"\""),
            "must contain multiline string delimiters"
        );
        // Verify the string round-trips (content is preserved)
        assert!(result.contains("ends with"));
    }

    #[test]
    fn test_key_quoting() {
        let emitter = TomlEmitter::new();
        let value = obj(&[
            ("simple-key", Value::Int(1)),
            ("key with spaces", Value::Int(2)),
        ]);
        let result = emitter.emit(&value).unwrap();
        assert!(result.contains("simple-key = 1"));
        assert!(result.contains("\"key with spaces\" = 2"));
    }
}
