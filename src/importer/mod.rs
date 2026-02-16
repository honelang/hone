// YAML/JSON to Hone importer
//
// Converts existing configuration files to Hone source code,
// enabling gradual migration without rewriting everything.

use std::collections::HashMap;
use std::path::Path;

use crate::errors::{HoneError, HoneResult};

/// Options for the import process
#[derive(Debug, Clone, Default)]
pub struct ImportOptions {
    /// Attempt to extract repeated values as variables
    pub extract_vars: bool,
    /// Split multi-document YAML into separate `--- name` sections
    pub split_docs: bool,
    /// Indent width (default: 2)
    pub indent: usize,
}

impl ImportOptions {
    pub fn new() -> Self {
        Self {
            indent: 2,
            ..Default::default()
        }
    }

    pub fn with_extract_vars(mut self, extract: bool) -> Self {
        self.extract_vars = extract;
        self
    }

    pub fn with_split_docs(mut self, split: bool) -> Self {
        self.split_docs = split;
        self
    }
}

/// Import a YAML or JSON file and convert to Hone source
pub fn import_file(path: &Path, options: &ImportOptions) -> HoneResult<String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| HoneError::io_error(format!("failed to read {}: {}", path.display(), e)))?;

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    match ext.to_lowercase().as_str() {
        "yaml" | "yml" => import_yaml(&content, options),
        "json" => import_json(&content, options),
        _ => {
            // Try to auto-detect
            if content.trim().starts_with('{') || content.trim().starts_with('[') {
                import_json(&content, options)
            } else {
                import_yaml(&content, options)
            }
        }
    }
}

/// Import YAML content to Hone
pub fn import_yaml(content: &str, options: &ImportOptions) -> HoneResult<String> {
    use serde::Deserialize;

    // Parse all YAML documents
    let mut documents: Vec<serde_yaml::Value> = Vec::new();

    for doc in serde_yaml::Deserializer::from_str(content) {
        let value = serde_yaml::Value::deserialize(doc)
            .map_err(|e| HoneError::io_error(format!("YAML parse error: {}", e)))?;
        documents.push(value);
    }

    if documents.is_empty() {
        return Ok(String::new());
    }

    let mut output = String::new();

    // Extract variables if requested
    let vars = if options.extract_vars {
        extract_variables(&documents)
    } else {
        HashMap::new()
    };

    // Write variable declarations
    if !vars.is_empty() {
        output.push_str("# Extracted variables\n");
        for (name, value) in &vars {
            output.push_str(&format!("let {} = {}\n", name, format_scalar(value)));
        }
        output.push('\n');
    }

    // Convert documents
    if documents.len() == 1 {
        write_yaml_value(&mut output, &documents[0], 0, options.indent, &vars, true);
    } else if options.split_docs {
        for (i, doc) in documents.iter().enumerate() {
            if i > 0 {
                output.push('\n');
            }
            output.push_str(&format!("---doc{}\n", i + 1));
            write_yaml_value(&mut output, doc, 0, options.indent, &vars, true);
        }
    } else {
        // Output as array of documents
        output.push_str("[\n");
        for (i, doc) in documents.iter().enumerate() {
            write_yaml_value(
                &mut output,
                doc,
                options.indent,
                options.indent,
                &vars,
                false,
            );
            if i < documents.len() - 1 {
                output.push(',');
            }
            output.push('\n');
        }
        output.push_str("]\n");
    }

    Ok(output)
}

/// Import JSON content to Hone
pub fn import_json(content: &str, options: &ImportOptions) -> HoneResult<String> {
    let value: serde_json::Value = serde_json::from_str(content)
        .map_err(|e| HoneError::io_error(format!("JSON parse error: {}", e)))?;

    let yaml_value = json_to_yaml(&value);

    let vars = if options.extract_vars {
        extract_variables(std::slice::from_ref(&yaml_value))
    } else {
        HashMap::new()
    };

    let mut output = String::new();

    if !vars.is_empty() {
        output.push_str("# Extracted variables\n");
        for (name, value) in &vars {
            output.push_str(&format!("let {} = {}\n", name, format_scalar(value)));
        }
        output.push('\n');
    }

    write_yaml_value(&mut output, &yaml_value, 0, options.indent, &vars, true);
    Ok(output)
}

/// Convert JSON value to YAML value for uniform processing
fn json_to_yaml(value: &serde_json::Value) -> serde_yaml::Value {
    match value {
        serde_json::Value::Null => serde_yaml::Value::Null,
        serde_json::Value::Bool(b) => serde_yaml::Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                serde_yaml::Value::Number(i.into())
            } else if let Some(f) = n.as_f64() {
                serde_yaml::Value::Number(serde_yaml::Number::from(f))
            } else {
                serde_yaml::Value::String(n.to_string())
            }
        }
        serde_json::Value::String(s) => serde_yaml::Value::String(s.clone()),
        serde_json::Value::Array(arr) => {
            serde_yaml::Value::Sequence(arr.iter().map(json_to_yaml).collect())
        }
        serde_json::Value::Object(obj) => {
            let mut map = serde_yaml::Mapping::new();
            for (k, v) in obj {
                map.insert(serde_yaml::Value::String(k.clone()), json_to_yaml(v));
            }
            serde_yaml::Value::Mapping(map)
        }
    }
}

/// Write a YAML value to Hone, handling objects with block syntax
fn write_yaml_value(
    output: &mut String,
    value: &serde_yaml::Value,
    indent: usize,
    indent_width: usize,
    vars: &HashMap<String, serde_yaml::Value>,
    is_root: bool,
) {
    let spaces = " ".repeat(indent);

    match value {
        serde_yaml::Value::Null => output.push_str("null"),
        serde_yaml::Value::Bool(b) => output.push_str(&b.to_string()),
        serde_yaml::Value::Number(n) => output.push_str(&n.to_string()),
        serde_yaml::Value::String(s) => {
            // Check if this value matches a variable
            for (var_name, var_value) in vars {
                if let serde_yaml::Value::String(vs) = var_value {
                    if vs == s {
                        output.push_str(var_name);
                        return;
                    }
                }
            }
            output.push_str(&format_string(s));
        }
        serde_yaml::Value::Sequence(arr) => {
            write_array(output, arr, indent, indent_width, vars);
        }
        serde_yaml::Value::Mapping(map) => {
            write_object(output, map, indent, indent_width, vars, is_root);
        }
        serde_yaml::Value::Tagged(tagged) => {
            output.push_str(&format!("{}# YAML tag: {}\n", spaces, tagged.tag));
            write_yaml_value(output, &tagged.value, indent, indent_width, vars, is_root);
        }
    }
}

/// Write an array
fn write_array(
    output: &mut String,
    arr: &[serde_yaml::Value],
    indent: usize,
    indent_width: usize,
    vars: &HashMap<String, serde_yaml::Value>,
) {
    if arr.is_empty() {
        output.push_str("[]");
        return;
    }

    // Check if all elements are simple scalars
    let all_simple = arr.iter().all(is_simple_value);

    if all_simple && arr.len() <= 5 {
        // Inline array
        output.push('[');
        for (i, item) in arr.iter().enumerate() {
            if i > 0 {
                output.push_str(", ");
            }
            write_yaml_value(output, item, 0, indent_width, vars, false);
        }
        output.push(']');
    } else {
        // Multi-line array
        output.push_str("[\n");
        for (i, item) in arr.iter().enumerate() {
            output.push_str(&" ".repeat(indent + indent_width));
            write_yaml_value(
                output,
                item,
                indent + indent_width,
                indent_width,
                vars,
                false,
            );
            if i < arr.len() - 1 {
                output.push(',');
            }
            output.push('\n');
        }
        output.push_str(&" ".repeat(indent));
        output.push(']');
    }
}

/// Write an object using block syntax for nested objects
fn write_object(
    output: &mut String,
    map: &serde_yaml::Mapping,
    indent: usize,
    indent_width: usize,
    vars: &HashMap<String, serde_yaml::Value>,
    is_root: bool,
) {
    if map.is_empty() {
        output.push_str("{}");
        return;
    }

    // For non-root objects inside arrays, we need braces
    if !is_root {
        output.push_str("{\n");
    }

    let inner_indent = if is_root {
        indent
    } else {
        indent + indent_width
    };

    for (k, v) in map.iter() {
        let key = format_key_yaml(k);
        output.push_str(&" ".repeat(inner_indent));

        match v {
            serde_yaml::Value::Mapping(inner_map) if !inner_map.is_empty() => {
                // Use block syntax: key { ... }
                output.push_str(&key);
                output.push_str(" {\n");
                write_object_body(
                    output,
                    inner_map,
                    inner_indent + indent_width,
                    indent_width,
                    vars,
                );
                output.push_str(&" ".repeat(inner_indent));
                output.push_str("}\n");
            }
            _ => {
                // Regular key: value
                output.push_str(&key);
                output.push_str(": ");
                write_yaml_value(output, v, inner_indent, indent_width, vars, false);
                output.push('\n');
            }
        }
    }

    if !is_root {
        output.push_str(&" ".repeat(indent));
        output.push('}');
    }
}

/// Write the body of an object (without braces)
fn write_object_body(
    output: &mut String,
    map: &serde_yaml::Mapping,
    indent: usize,
    indent_width: usize,
    vars: &HashMap<String, serde_yaml::Value>,
) {
    for (k, v) in map.iter() {
        let key = format_key_yaml(k);
        output.push_str(&" ".repeat(indent));

        match v {
            serde_yaml::Value::Mapping(inner_map) if !inner_map.is_empty() => {
                // Nested block syntax
                output.push_str(&key);
                output.push_str(" {\n");
                write_object_body(output, inner_map, indent + indent_width, indent_width, vars);
                output.push_str(&" ".repeat(indent));
                output.push_str("}\n");
            }
            _ => {
                output.push_str(&key);
                output.push_str(": ");
                write_yaml_value(output, v, indent, indent_width, vars, false);
                output.push('\n');
            }
        }
    }
}

/// Format a YAML key for Hone
fn format_key_yaml(key: &serde_yaml::Value) -> String {
    match key {
        serde_yaml::Value::String(s) => format_key(s),
        serde_yaml::Value::Number(n) => format!("[{}]", n),
        serde_yaml::Value::Bool(b) => format!("[\"{}\"]", b),
        _ => format!("[{}]", format_string(key.as_str().unwrap_or("unknown"))),
    }
}

/// Format a key, quoting if necessary
fn format_key(key: &str) -> String {
    let needs_quote = key.is_empty()
        || !key
            .chars()
            .next()
            .map(|c| c.is_alphabetic() || c == '_')
            .unwrap_or(false)
        || key
            .chars()
            .any(|c| !c.is_alphanumeric() && c != '_' && c != '-')
        || is_reserved_word(key);

    if needs_quote {
        format_string(key)
    } else {
        key.to_string()
    }
}

/// Check if a word is reserved in Hone
fn is_reserved_word(word: &str) -> bool {
    matches!(
        word,
        "let"
            | "if"
            | "else"
            | "for"
            | "in"
            | "when"
            | "import"
            | "from"
            | "as"
            | "true"
            | "false"
            | "null"
            | "assert"
            | "schema"
            | "type"
            | "use"
    )
}

/// Format a string value with proper escaping
fn format_string(s: &str) -> String {
    let needs_escape = s.contains('\\')
        || s.contains('"')
        || s.contains('\n')
        || s.contains('\r')
        || s.contains('\t');

    if s.contains('\n') && s.lines().count() > 1 {
        format!("\"\"\"\n{}\n\"\"\"", s)
    } else if needs_escape {
        let escaped = s
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t");
        format!("\"{}\"", escaped)
    } else {
        format!("\"{}\"", s)
    }
}

/// Check if a value is a simple scalar
fn is_simple_value(value: &serde_yaml::Value) -> bool {
    matches!(
        value,
        serde_yaml::Value::Null
            | serde_yaml::Value::Bool(_)
            | serde_yaml::Value::Number(_)
            | serde_yaml::Value::String(_)
    )
}

/// Format a scalar value for inline use
fn format_scalar(value: &serde_yaml::Value) -> String {
    match value {
        serde_yaml::Value::String(s) => format_string(s),
        serde_yaml::Value::Number(n) => n.to_string(),
        serde_yaml::Value::Bool(b) => b.to_string(),
        serde_yaml::Value::Null => "null".to_string(),
        _ => format_string(&format!("{:?}", value)),
    }
}

/// Extract repeated string values as variables
fn extract_variables(documents: &[serde_yaml::Value]) -> HashMap<String, serde_yaml::Value> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut values: HashMap<String, serde_yaml::Value> = HashMap::new();

    for doc in documents {
        count_strings(doc, &mut counts, &mut values);
    }

    let mut vars = HashMap::new();
    let mut var_idx = 0;

    for (s, count) in counts {
        if count >= 2 && s.len() >= 8 {
            let var_name = generate_var_name(&s, var_idx);
            var_idx += 1;
            if let Some(value) = values.get(&s) {
                vars.insert(var_name, value.clone());
            }
        }
    }

    vars
}

fn count_strings(
    value: &serde_yaml::Value,
    counts: &mut HashMap<String, usize>,
    values: &mut HashMap<String, serde_yaml::Value>,
) {
    match value {
        serde_yaml::Value::String(s) => {
            *counts.entry(s.clone()).or_insert(0) += 1;
            values.insert(s.clone(), value.clone());
        }
        serde_yaml::Value::Sequence(arr) => {
            for item in arr {
                count_strings(item, counts, values);
            }
        }
        serde_yaml::Value::Mapping(map) => {
            for v in map.values() {
                count_strings(v, counts, values);
            }
        }
        _ => {}
    }
}

/// Generate a variable name from a string value
fn generate_var_name(s: &str, idx: usize) -> String {
    let clean: String = s
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .take(20)
        .collect();

    if clean.is_empty() || clean.chars().next().map(|c| c.is_numeric()).unwrap_or(true) {
        format!("var_{}", idx)
    } else {
        clean.to_lowercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_import_simple_yaml() {
        let yaml = r#"
name: myapp
port: 8080
enabled: true
"#;
        let result = import_yaml(yaml, &ImportOptions::new()).unwrap();
        assert!(result.contains("name: \"myapp\""));
        assert!(result.contains("port: 8080"));
        assert!(result.contains("enabled: true"));
    }

    #[test]
    fn test_import_nested_yaml() {
        let yaml = r#"
server:
  host: localhost
  port: 8080
"#;
        let result = import_yaml(yaml, &ImportOptions::new()).unwrap();
        assert!(result.contains("server {"));
        assert!(result.contains("host: \"localhost\""));
    }

    #[test]
    fn test_import_array_yaml() {
        let yaml = r#"
items:
  - one
  - two
  - three
"#;
        let result = import_yaml(yaml, &ImportOptions::new()).unwrap();
        assert!(result.contains("items:"));
        assert!(result.contains("\"one\""));
    }

    #[test]
    fn test_import_json() {
        let json = r#"{"name": "test", "count": 42}"#;
        let result = import_json(json, &ImportOptions::new()).unwrap();
        assert!(result.contains("name: \"test\""));
        assert!(result.contains("count: 42"));
    }

    #[test]
    fn test_reserved_word_quoting() {
        let yaml = "let: value\ntype: string";
        let result = import_yaml(yaml, &ImportOptions::new()).unwrap();
        assert!(result.contains("\"let\":") || result.contains("\"let\": "));
        assert!(result.contains("\"type\":") || result.contains("\"type\": "));
    }

    #[test]
    fn test_roundtrip_simple() {
        let yaml = "name: test\nport: 8080\n";
        let hone = import_yaml(yaml, &ImportOptions::new()).unwrap();
        // Should be valid Hone syntax
        assert!(hone.contains("name: \"test\""));
        assert!(hone.contains("port: 8080"));
    }
}
