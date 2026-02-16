//! Emitters for Hone configuration language
//!
//! Converts evaluated Value trees to JSON, YAML, or other output formats.

mod dotenv;
mod json;
mod toml;
mod yaml;

pub use dotenv::DotenvEmitter;
pub use json::JsonEmitter;
pub use toml::TomlEmitter;
pub use yaml::YamlEmitter;

use crate::errors::HoneResult;
use crate::evaluator::Value;

/// Output format for emission
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Json,
    JsonPretty,
    Yaml,
    Toml,
    Dotenv,
}

impl OutputFormat {
    /// Parse from string
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "json" => Some(OutputFormat::Json),
            "json-pretty" | "jsonpretty" => Some(OutputFormat::JsonPretty),
            "yaml" | "yml" => Some(OutputFormat::Yaml),
            "toml" => Some(OutputFormat::Toml),
            "dotenv" | "env" => Some(OutputFormat::Dotenv),
            _ => None,
        }
    }
}

/// Trait for output emitters
pub trait Emitter {
    /// Emit a value to string
    fn emit(&self, value: &Value) -> HoneResult<String>;

    /// Emit a value to a writer
    fn emit_to_writer<W: std::io::Write>(&self, value: &Value, writer: &mut W) -> HoneResult<()> {
        let output = self.emit(value)?;
        writer
            .write_all(output.as_bytes())
            .map_err(|e| crate::errors::HoneError::IoError {
                message: e.to_string(),
            })
    }
}

/// Emit a value to a string in the specified format
pub fn emit(value: &Value, format: OutputFormat) -> HoneResult<String> {
    match format {
        OutputFormat::Json => JsonEmitter::new(false).emit(value),
        OutputFormat::JsonPretty => JsonEmitter::new(true).emit(value),
        OutputFormat::Yaml => YamlEmitter::new().emit(value),
        OutputFormat::Toml => TomlEmitter::new().emit(value),
        OutputFormat::Dotenv => DotenvEmitter::new().emit(value),
    }
}

/// Emit multiple values (for multi-document output)
pub fn emit_multi(values: &[(Option<String>, Value)], format: OutputFormat) -> HoneResult<String> {
    let mut output = String::new();

    for (i, (name, value)) in values.iter().enumerate() {
        if i > 0 {
            output.push('\n');
        }

        match format {
            OutputFormat::Json | OutputFormat::JsonPretty => {
                if let Some(name) = name {
                    output.push_str(&format!("// Document: {}\n", name));
                }
                output.push_str(&emit(value, format)?);
            }
            OutputFormat::Yaml => {
                if i > 0 || name.is_some() {
                    output.push_str("---");
                    if let Some(name) = name {
                        output.push_str(&format!(" # {}", name));
                    }
                    output.push('\n');
                }
                output.push_str(&emit(value, format)?);
            }
            OutputFormat::Toml => {
                if let Some(name) = name {
                    output.push_str(&format!("# Document: {}\n", name));
                }
                output.push_str(&emit(value, format)?);
            }
            OutputFormat::Dotenv => {
                if let Some(name) = name {
                    output.push_str(&format!("# Document: {}\n", name));
                }
                output.push_str(&emit(value, format)?);
            }
        }
    }

    Ok(output)
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
    fn test_output_format_from_str() {
        assert_eq!(OutputFormat::parse("json"), Some(OutputFormat::Json));
        assert_eq!(OutputFormat::parse("JSON"), Some(OutputFormat::Json));
        assert_eq!(OutputFormat::parse("yaml"), Some(OutputFormat::Yaml));
        assert_eq!(OutputFormat::parse("yml"), Some(OutputFormat::Yaml));
        assert_eq!(
            OutputFormat::parse("json-pretty"),
            Some(OutputFormat::JsonPretty)
        );
        assert_eq!(OutputFormat::parse("toml"), Some(OutputFormat::Toml));
        assert_eq!(OutputFormat::parse("TOML"), Some(OutputFormat::Toml));
        assert_eq!(OutputFormat::parse("dotenv"), Some(OutputFormat::Dotenv));
        assert_eq!(OutputFormat::parse("env"), Some(OutputFormat::Dotenv));
        assert_eq!(OutputFormat::parse("unknown"), None);
    }

    #[test]
    fn test_emit_json() {
        let value = obj(&[("name", Value::String("test".into()))]);
        let json = emit(&value, OutputFormat::Json).unwrap();
        assert_eq!(json, r#"{"name":"test"}"#);
    }

    #[test]
    fn test_emit_yaml() {
        let value = obj(&[("name", Value::String("test".into()))]);
        let yaml = emit(&value, OutputFormat::Yaml).unwrap();
        assert!(yaml.contains("name: test"));
    }
}
