//! .env emitter for Hone values
//!
//! Flattens nested objects into KEY=VALUE pairs, using underscore separation
//! and uppercase keys. Suitable for .env files and environment variable configs.

use super::Emitter;
use crate::errors::{HoneError, HoneResult};
use crate::evaluator::Value;

/// .env output emitter
pub struct DotenvEmitter;

impl DotenvEmitter {
    pub fn new() -> Self {
        Self
    }

    /// Flatten a value into key=value pairs
    fn flatten(
        &self,
        value: &Value,
        prefix: &str,
        pairs: &mut Vec<(String, String)>,
    ) -> HoneResult<()> {
        match value {
            Value::Object(obj) => {
                for (key, val) in obj {
                    let full_key = if prefix.is_empty() {
                        Self::to_env_key(key)
                    } else {
                        format!("{}_{}", prefix, Self::to_env_key(key))
                    };
                    self.flatten(val, &full_key, pairs)?;
                }
            }
            Value::Null => {
                // Skip null values
            }
            Value::Bool(b) => {
                pairs.push((
                    prefix.to_string(),
                    if *b { "true" } else { "false" }.to_string(),
                ));
            }
            Value::Int(n) => {
                pairs.push((prefix.to_string(), n.to_string()));
            }
            Value::Float(n) => {
                if n.fract() == 0.0 {
                    pairs.push((prefix.to_string(), format!("{:.1}", n)));
                } else {
                    pairs.push((prefix.to_string(), n.to_string()));
                }
            }
            Value::String(s) => {
                pairs.push((prefix.to_string(), s.clone()));
            }
            Value::Array(arr) => {
                // If all non-null elements are scalars, comma-join them.
                // Otherwise, flatten with __index__ separators (dotnet-style).
                let has_complex = arr
                    .iter()
                    .any(|item| matches!(item, Value::Object(_) | Value::Array(_)));

                if has_complex {
                    for (i, item) in arr.iter().enumerate() {
                        let indexed_key = format!("{}__{}", prefix, i);
                        self.flatten(item, &indexed_key, pairs)?;
                    }
                } else {
                    let mut items = Vec::new();
                    for item in arr {
                        match item {
                            Value::Null => {}
                            Value::String(s) => items.push(s.clone()),
                            other => items.push(other.to_string()),
                        }
                    }
                    pairs.push((prefix.to_string(), items.join(",")));
                }
            }
        }
        Ok(())
    }

    /// Convert a key to ENV_VARIABLE style (uppercase, hyphens to underscores)
    fn to_env_key(key: &str) -> String {
        key.chars()
            .map(|c| {
                if c == '-' || c == '.' {
                    '_'
                } else {
                    c.to_ascii_uppercase()
                }
            })
            .collect()
    }

    /// Quote a value if it contains special characters
    fn quote_value(value: &str) -> String {
        if value.is_empty()
            || value.contains(' ')
            || value.contains('"')
            || value.contains('\'')
            || value.contains('#')
            || value.contains('$')
            || value.contains('\\')
            || value.contains('\n')
            || value.contains('=')
        {
            // Use double quotes and escape
            let escaped = value
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n")
                .replace('$', "\\$");
            format!("\"{}\"", escaped)
        } else {
            value.to_string()
        }
    }
}

impl Default for DotenvEmitter {
    fn default() -> Self {
        Self::new()
    }
}

impl Emitter for DotenvEmitter {
    fn emit(&self, value: &Value) -> HoneResult<String> {
        match value {
            Value::Object(_) => {
                let mut pairs = Vec::new();
                self.flatten(value, "", &mut pairs)?;

                let mut result = String::new();
                for (key, val) in &pairs {
                    result.push_str(key);
                    result.push('=');
                    result.push_str(&Self::quote_value(val));
                    result.push('\n');
                }
                Ok(result)
            }
            _ => Err(HoneError::io_error(
                ".env output requires a top-level object".to_string(),
            )),
        }
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
        let emitter = DotenvEmitter::new();
        let value = obj(&[
            ("host", Value::String("localhost".into())),
            ("port", Value::Int(8080)),
            ("debug", Value::Bool(true)),
        ]);
        let result = emitter.emit(&value).unwrap();
        assert!(result.contains("HOST=localhost\n"));
        assert!(result.contains("PORT=8080\n"));
        assert!(result.contains("DEBUG=true\n"));
    }

    #[test]
    fn test_nested_flattening() {
        let emitter = DotenvEmitter::new();
        let value = obj(&[(
            "server",
            obj(&[
                ("host", Value::String("localhost".into())),
                ("port", Value::Int(8080)),
            ]),
        )]);
        let result = emitter.emit(&value).unwrap();
        assert!(result.contains("SERVER_HOST=localhost\n"));
        assert!(result.contains("SERVER_PORT=8080\n"));
    }

    #[test]
    fn test_deep_nesting() {
        let emitter = DotenvEmitter::new();
        let value = obj(&[(
            "database",
            obj(&[(
                "connection",
                obj(&[("host", Value::String("db.example.com".into()))]),
            )]),
        )]);
        let result = emitter.emit(&value).unwrap();
        assert!(result.contains("DATABASE_CONNECTION_HOST=db.example.com\n"));
    }

    #[test]
    fn test_uppercase_conversion() {
        let emitter = DotenvEmitter::new();
        let value = obj(&[
            ("api-key", Value::String("secret123".into())),
            ("my.setting", Value::String("value".into())),
        ]);
        let result = emitter.emit(&value).unwrap();
        assert!(result.contains("API_KEY=secret123\n"));
        assert!(result.contains("MY_SETTING=value\n"));
    }

    #[test]
    fn test_quoting() {
        let emitter = DotenvEmitter::new();
        let value = obj(&[
            ("simple", Value::String("hello".into())),
            ("spaces", Value::String("hello world".into())),
            ("special", Value::String("val#ue".into())),
            ("empty", Value::String(String::new())),
        ]);
        let result = emitter.emit(&value).unwrap();
        assert!(result.contains("SIMPLE=hello\n"));
        assert!(result.contains("SPACES=\"hello world\"\n"));
        assert!(result.contains("SPECIAL=\"val#ue\"\n"));
        assert!(result.contains("EMPTY=\"\"\n"));
    }

    #[test]
    fn test_null_skipped() {
        let emitter = DotenvEmitter::new();
        let value = obj(&[
            ("present", Value::String("yes".into())),
            ("missing", Value::Null),
        ]);
        let result = emitter.emit(&value).unwrap();
        assert!(result.contains("PRESENT=yes\n"));
        assert!(!result.contains("MISSING"));
    }

    #[test]
    fn test_scalar_array() {
        let emitter = DotenvEmitter::new();
        let value = obj(&[(
            "ports",
            Value::Array(vec![Value::Int(80), Value::Int(443), Value::Int(8080)]),
        )]);
        let result = emitter.emit(&value).unwrap();
        assert!(result.contains("PORTS=80,443,8080\n"));
    }

    #[test]
    fn test_array_of_objects_indexed() {
        let emitter = DotenvEmitter::new();
        let value = obj(&[(
            "servers",
            Value::Array(vec![
                obj(&[
                    ("name", Value::String("api".into())),
                    ("port", Value::Int(8080)),
                ]),
                obj(&[
                    ("name", Value::String("worker".into())),
                    ("port", Value::Int(9090)),
                ]),
            ]),
        )]);
        let result = emitter.emit(&value).unwrap();
        assert!(result.contains("SERVERS__0_NAME=api\n"));
        assert!(result.contains("SERVERS__0_PORT=8080\n"));
        assert!(result.contains("SERVERS__1_NAME=worker\n"));
        assert!(result.contains("SERVERS__1_PORT=9090\n"));
    }

    #[test]
    fn test_nested_array_of_objects_deep() {
        let emitter = DotenvEmitter::new();
        let value = obj(&[(
            "app",
            obj(&[(
                "containers",
                Value::Array(vec![obj(&[(
                    "env",
                    Value::Array(vec![obj(&[
                        ("name", Value::String("PORT".into())),
                        ("value", Value::String("8080".into())),
                    ])]),
                )])]),
            )]),
        )]);
        let result = emitter.emit(&value).unwrap();
        assert!(result.contains("APP_CONTAINERS__0_ENV__0_NAME=PORT\n"));
        assert!(result.contains("APP_CONTAINERS__0_ENV__0_VALUE=8080\n"));
    }

    #[test]
    fn test_array_of_scalars_still_comma_joined() {
        let emitter = DotenvEmitter::new();
        let value = obj(&[(
            "tags",
            Value::Array(vec![
                Value::String("web".into()),
                Value::String("api".into()),
            ]),
        )]);
        let result = emitter.emit(&value).unwrap();
        assert!(result.contains("TAGS=web,api\n"));
    }

    #[test]
    fn test_mixed_array_with_nested_arrays() {
        let emitter = DotenvEmitter::new();
        let value = obj(&[(
            "matrix",
            Value::Array(vec![
                Value::Array(vec![Value::Int(1), Value::Int(2)]),
                Value::Array(vec![Value::Int(3), Value::Int(4)]),
            ]),
        )]);
        let result = emitter.emit(&value).unwrap();
        assert!(result.contains("MATRIX__0=1,2\n"));
        assert!(result.contains("MATRIX__1=3,4\n"));
    }

    #[test]
    fn test_non_object_toplevel_error() {
        let emitter = DotenvEmitter::new();
        assert!(emitter.emit(&Value::Int(42)).is_err());
    }

    #[test]
    fn test_empty_object() {
        let emitter = DotenvEmitter::new();
        let value = Value::Object(IndexMap::new());
        let result = emitter.emit(&value).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_hyphen_key() {
        let emitter = DotenvEmitter::new();
        let value = obj(&[(
            "my-service",
            obj(&[("api-key", Value::String("abc".into()))]),
        )]);
        let result = emitter.emit(&value).unwrap();
        assert!(result.contains("MY_SERVICE_API_KEY=abc\n"));
    }
}
