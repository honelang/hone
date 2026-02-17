//! Type provider: generates Hone schema definitions from external type sources.
//!
//! Currently supports JSON Schema → Hone schema conversion (`hone typegen`).
// Usage: `hone typegen schema.json -o types.hone`

use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

/// Generate Hone schema source from a JSON Schema file
pub fn generate_from_file(path: &Path) -> Result<String, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;

    let schema: Value = serde_json::from_str(&content)
        .map_err(|e| format!("invalid JSON in {}: {}", path.display(), e))?;

    generate_from_schema(&schema)
}

/// Generate Hone schema source from a parsed JSON Schema value
pub fn generate_from_schema(schema: &Value) -> Result<String, String> {
    let mut generator = SchemaGenerator::new();
    generator.process_root(schema)?;
    Ok(generator.output())
}

struct SchemaGenerator {
    /// Named schemas in definition order
    schemas: Vec<(String, SchemaInfo)>,
    /// Type aliases for constrained primitives
    type_aliases: Vec<(String, String)>,
}

struct SchemaInfo {
    fields: Vec<FieldInfo>,
    open: bool,
}

struct FieldInfo {
    name: String,
    type_str: String,
    optional: bool,
}

impl SchemaGenerator {
    fn new() -> Self {
        Self {
            schemas: Vec::new(),
            type_aliases: Vec::new(),
        }
    }

    fn output(&self) -> String {
        let mut out = String::new();

        // Emit type aliases first
        for (name, type_str) in &self.type_aliases {
            out.push_str(&format!("type {} = {}\n", name, type_str));
        }

        if !self.type_aliases.is_empty() && !self.schemas.is_empty() {
            out.push('\n');
        }

        // Emit schemas
        for (i, (name, info)) in self.schemas.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(&format!("schema {} {{\n", name));
            for field in &info.fields {
                let opt = if field.optional { "?" } else { "" };
                out.push_str(&format!("  {}{}: {}\n", field.name, opt, field.type_str));
            }
            if info.open {
                out.push_str("  ...\n");
            }
            out.push_str("}\n");
        }

        out
    }

    fn process_root(&mut self, schema: &Value) -> Result<(), String> {
        // Process $defs / definitions first (referenced schemas)
        if let Some(defs) = schema.get("$defs").or_else(|| schema.get("definitions")) {
            if let Some(obj) = defs.as_object() {
                // Sort for deterministic output
                let sorted: BTreeMap<_, _> = obj.iter().collect();
                for (name, def_schema) in sorted {
                    self.process_object_schema(&pascal_case(name), def_schema)?;
                }
            }
        }

        // Process the root schema itself
        let title = schema
            .get("title")
            .and_then(|t| t.as_str())
            .map(pascal_case)
            .unwrap_or_else(|| "Root".to_string());

        // Only process root as schema if it has properties
        if schema.get("properties").is_some() {
            self.process_object_schema(&title, schema)?;
        }

        Ok(())
    }

    fn process_object_schema(&mut self, name: &str, schema: &Value) -> Result<(), String> {
        let properties = match schema.get("properties") {
            Some(p) => match p.as_object() {
                Some(obj) => obj,
                None => return Ok(()),
            },
            None => return Ok(()),
        };

        let required: Vec<String> = schema
            .get("required")
            .and_then(|r| r.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let additional_properties = schema
            .get("additionalProperties")
            .map(|v| v.as_bool().unwrap_or(true))
            .unwrap_or(true);

        // Sort properties for deterministic output
        let sorted: BTreeMap<_, _> = properties.iter().collect();
        let mut fields = Vec::new();

        for (prop_name, prop_schema) in sorted {
            let type_str = self.resolve_type(prop_name, prop_schema, name)?;
            let optional = !required.contains(prop_name);
            fields.push(FieldInfo {
                name: safe_field_name(prop_name),
                type_str,
                optional,
            });
        }

        // Don't add duplicate schemas
        if !self.schemas.iter().any(|(n, _)| n == name) {
            self.schemas.push((
                name.to_string(),
                SchemaInfo {
                    fields,
                    open: additional_properties,
                },
            ));
        }

        Ok(())
    }

    /// Resolve a JSON Schema type reference to a Hone type string
    fn resolve_type(
        &mut self,
        field_name: &str,
        schema: &Value,
        parent_name: &str,
    ) -> Result<String, String> {
        // Handle $ref
        if let Some(ref_str) = schema.get("$ref").and_then(|r| r.as_str()) {
            return Ok(self.resolve_ref(ref_str));
        }

        // Handle allOf (treat as the first schema for simplicity)
        if let Some(all_of) = schema.get("allOf").and_then(|v| v.as_array()) {
            if let Some(first) = all_of.first() {
                return self.resolve_type(field_name, first, parent_name);
            }
        }

        // Handle oneOf/anyOf as union (simplified: take the types)
        if let Some(one_of) = schema.get("oneOf").or_else(|| schema.get("anyOf")) {
            if let Some(arr) = one_of.as_array() {
                let mut types = Vec::new();
                for item in arr {
                    types.push(self.resolve_type(field_name, item, parent_name)?);
                }
                // Deduplicate
                types.dedup();
                if types.len() == 1 {
                    return Ok(types.into_iter().next().unwrap());
                }
                // Hone doesn't have union types in schemas, use the first non-null
                let non_null: Vec<_> = types.into_iter().filter(|t| t != "null").collect();
                return Ok(non_null
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "object".to_string()));
            }
        }

        // Handle enum (string enum -> string, with comment)
        if let Some(_enum_values) = schema.get("enum") {
            let base_type = schema
                .get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("string");
            return Ok(map_primitive_type(base_type));
        }

        // Get the type field
        let type_val = schema.get("type");

        match type_val.and_then(|t| t.as_str()) {
            Some("string") => Ok(self.resolve_string_type(schema)),
            Some("integer") => Ok(self.resolve_int_type(schema)),
            Some("number") => Ok(self.resolve_float_type(schema)),
            Some("boolean") => Ok("bool".to_string()),
            Some("null") => Ok("null".to_string()),
            Some("array") => self.resolve_array_type(field_name, schema, parent_name),
            Some("object") => {
                // Nested object - if it has properties, create a named sub-schema
                if schema.get("properties").is_some() {
                    let sub_name = format!("{}{}", parent_name, pascal_case(field_name));
                    self.process_object_schema(&sub_name, schema)?;
                    Ok(sub_name)
                } else {
                    Ok("object".to_string())
                }
            }
            Some(other) => Ok(other.to_string()),
            None => {
                // No type specified - check for properties (implicit object)
                if schema.get("properties").is_some() {
                    let sub_name = format!("{}{}", parent_name, pascal_case(field_name));
                    self.process_object_schema(&sub_name, schema)?;
                    Ok(sub_name)
                } else {
                    Ok("object".to_string())
                }
            }
        }
    }

    fn resolve_string_type(&self, schema: &Value) -> String {
        let min_len = schema.get("minLength").and_then(|v| v.as_u64());
        let max_len = schema.get("maxLength").and_then(|v| v.as_u64());
        let pattern = schema.get("pattern").and_then(|v| v.as_str());

        if let Some(pat) = pattern {
            return format!("string(\"{}\")", pat);
        }

        match (min_len, max_len) {
            (Some(min), Some(max)) => format!("string({}, {})", min, max),
            (Some(_), None) | (None, Some(_)) | (None, None) => "string".to_string(),
        }
    }

    fn resolve_int_type(&self, schema: &Value) -> String {
        let minimum = schema.get("minimum").and_then(|v| v.as_i64());
        let maximum = schema.get("maximum").and_then(|v| v.as_i64());

        match (minimum, maximum) {
            (Some(min), Some(max)) => format!("int({}, {})", min, max),
            (Some(_), None) | (None, Some(_)) | (None, None) => "int".to_string(),
        }
    }

    fn resolve_float_type(&self, schema: &Value) -> String {
        let minimum = schema.get("minimum").and_then(|v| v.as_f64());
        let maximum = schema.get("maximum").and_then(|v| v.as_f64());

        match (minimum, maximum) {
            (Some(min), Some(max)) => format!("float({}, {})", min, max),
            (Some(_), None) | (None, Some(_)) | (None, None) => "float".to_string(),
        }
    }

    fn resolve_array_type(
        &mut self,
        field_name: &str,
        schema: &Value,
        parent_name: &str,
    ) -> Result<String, String> {
        // Check items schema for array element type
        if let Some(items) = schema.get("items") {
            let item_type = self.resolve_type(field_name, items, parent_name)?;
            // Hone's array type doesn't carry element type in the type constraint
            // but we can add it as a comment in the output
            Ok(format!("array # {}", item_type))
        } else {
            Ok("array".to_string())
        }
    }

    fn resolve_ref(&self, ref_str: &str) -> String {
        // Handle local $ref like "#/$defs/Foo" or "#/definitions/Foo"
        if let Some(name) = ref_str
            .strip_prefix("#/$defs/")
            .or_else(|| ref_str.strip_prefix("#/definitions/"))
        {
            pascal_case(name)
        } else {
            // Remote refs not supported - return as object
            "object".to_string()
        }
    }
}

/// Convert a string to PascalCase for schema names
fn pascal_case(s: &str) -> String {
    s.split(['_', '-', ' ', '.'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(c) => {
                    let mut result = c.to_uppercase().to_string();
                    result.extend(chars);
                    result
                }
                None => String::new(),
            }
        })
        .collect()
}

/// Convert a JSON Schema primitive type name to Hone type name
fn map_primitive_type(json_type: &str) -> String {
    match json_type {
        "string" => "string".to_string(),
        "integer" => "int".to_string(),
        "number" => "float".to_string(),
        "boolean" => "bool".to_string(),
        "null" => "null".to_string(),
        "object" => "object".to_string(),
        "array" => "array".to_string(),
        other => other.to_string(),
    }
}

/// Make a field name safe for Hone (quote reserved words)
fn safe_field_name(name: &str) -> String {
    let reserved = [
        "let", "when", "else", "for", "import", "from", "true", "false", "null", "assert", "type",
        "schema", "variant", "expect", "secret", "policy", "deny", "warn", "use", "in", "as",
    ];
    if reserved.contains(&name) {
        format!("\"{}\"", name)
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_type_mapping() {
        let schema = serde_json::json!({
            "type": "object",
            "title": "Config",
            "properties": {
                "name": { "type": "string" },
                "port": { "type": "integer" },
                "rate": { "type": "number" },
                "debug": { "type": "boolean" }
            },
            "required": ["name", "port"],
            "additionalProperties": false
        });

        let result = generate_from_schema(&schema).unwrap();
        assert!(result.contains("schema Config"));
        assert!(result.contains("name: string"));
        assert!(result.contains("port: int"));
        assert!(result.contains("rate?: float"));
        assert!(result.contains("debug?: bool"));
    }

    #[test]
    fn test_constrained_types() {
        let schema = serde_json::json!({
            "type": "object",
            "title": "Server",
            "properties": {
                "host": { "type": "string", "minLength": 1, "maxLength": 255 },
                "port": { "type": "integer", "minimum": 1, "maximum": 65535 },
                "load": { "type": "number", "minimum": 0.0, "maximum": 1.0 }
            },
            "required": ["host", "port"],
            "additionalProperties": false
        });

        let result = generate_from_schema(&schema).unwrap();
        assert!(result.contains("host: string(1, 255)"), "got: {}", result);
        assert!(result.contains("port: int(1, 65535)"), "got: {}", result);
        assert!(result.contains("load?: float(0, 1)"), "got: {}", result);
    }

    #[test]
    fn test_single_bound_constraint_becomes_unconstrained() {
        // When only min or only max is specified, emit unconstrained type
        // (Hone requires both bounds in constraint syntax)
        let schema = serde_json::json!({
            "type": "object",
            "title": "Bounds",
            "properties": {
                "min_only": { "type": "integer", "minimum": 0 },
                "max_only": { "type": "integer", "maximum": 100 },
                "both": { "type": "integer", "minimum": 1, "maximum": 65535 }
            },
            "required": ["min_only", "max_only", "both"],
            "additionalProperties": false
        });

        let result = generate_from_schema(&schema).unwrap();
        assert!(
            result.contains("min_only: int\n"),
            "single min should be plain int, got: {}",
            result
        );
        assert!(
            result.contains("max_only: int\n"),
            "single max should be plain int, got: {}",
            result
        );
        assert!(
            result.contains("both: int(1, 65535)"),
            "both bounds should be constrained, got: {}",
            result
        );
    }

    #[test]
    fn test_string_pattern() {
        let schema = serde_json::json!({
            "type": "object",
            "title": "Email",
            "properties": {
                "email": { "type": "string", "pattern": "^[a-z]+@[a-z]+\\.[a-z]+$" }
            },
            "required": ["email"],
            "additionalProperties": false
        });

        let result = generate_from_schema(&schema).unwrap();
        // The pattern is emitted as-is from JSON Schema
        assert!(
            result.contains(r#"string("^[a-z]+@[a-z]+\.[a-z]+$")"#),
            "got: {}",
            result
        );
    }

    #[test]
    fn test_required_and_optional() {
        let schema = serde_json::json!({
            "type": "object",
            "title": "App",
            "properties": {
                "name": { "type": "string" },
                "version": { "type": "string" },
                "debug": { "type": "boolean" }
            },
            "required": ["name"]
        });

        let result = generate_from_schema(&schema).unwrap();
        assert!(
            result.contains("  name: string\n"),
            "name should be required, got: {}",
            result
        );
        assert!(
            result.contains("  debug?: bool\n"),
            "debug should be optional, got: {}",
            result
        );
        assert!(
            result.contains("  version?: string\n"),
            "version should be optional, got: {}",
            result
        );
    }

    #[test]
    fn test_nested_object_ref() {
        let schema = serde_json::json!({
            "type": "object",
            "title": "Config",
            "$defs": {
                "Database": {
                    "type": "object",
                    "properties": {
                        "host": { "type": "string" },
                        "port": { "type": "integer" }
                    },
                    "required": ["host", "port"]
                }
            },
            "properties": {
                "db": { "$ref": "#/$defs/Database" }
            },
            "required": ["db"]
        });

        let result = generate_from_schema(&schema).unwrap();
        assert!(
            result.contains("schema Database"),
            "should have Database schema, got: {}",
            result
        );
        assert!(
            result.contains("db: Database"),
            "should reference Database, got: {}",
            result
        );
    }

    #[test]
    fn test_array_type() {
        let schema = serde_json::json!({
            "type": "object",
            "title": "List",
            "properties": {
                "items": { "type": "array", "items": { "type": "string" } },
                "tags": { "type": "array" }
            }
        });

        let result = generate_from_schema(&schema).unwrap();
        assert!(result.contains("items?: array # string"), "got: {}", result);
        assert!(result.contains("tags?: array"), "got: {}", result);
    }

    #[test]
    fn test_additional_properties_closed() {
        let schema = serde_json::json!({
            "type": "object",
            "title": "Strict",
            "properties": {
                "name": { "type": "string" }
            },
            "additionalProperties": false,
            "required": ["name"]
        });

        let result = generate_from_schema(&schema).unwrap();
        assert!(
            !result.contains("..."),
            "closed schema should not have spread, got: {}",
            result
        );
    }

    #[test]
    fn test_additional_properties_open() {
        let schema = serde_json::json!({
            "type": "object",
            "title": "Open",
            "properties": {
                "name": { "type": "string" }
            },
            "additionalProperties": true,
            "required": ["name"]
        });

        let result = generate_from_schema(&schema).unwrap();
        assert!(
            result.contains("..."),
            "open schema should have spread, got: {}",
            result
        );
    }

    #[test]
    fn test_nested_object_creates_sub_schema() {
        let schema = serde_json::json!({
            "type": "object",
            "title": "Config",
            "properties": {
                "server": {
                    "type": "object",
                    "properties": {
                        "host": { "type": "string" },
                        "port": { "type": "integer" }
                    },
                    "required": ["host"]
                }
            },
            "required": ["server"]
        });

        let result = generate_from_schema(&schema).unwrap();
        assert!(
            result.contains("schema ConfigServer"),
            "should create sub-schema, got: {}",
            result
        );
        assert!(
            result.contains("server: ConfigServer"),
            "should reference sub-schema, got: {}",
            result
        );
    }

    #[test]
    fn test_pascal_case() {
        assert_eq!(pascal_case("foo_bar"), "FooBar");
        assert_eq!(pascal_case("hello-world"), "HelloWorld");
        assert_eq!(pascal_case("simple"), "Simple");
        assert_eq!(pascal_case("already_Pascal"), "AlreadyPascal");
        assert_eq!(pascal_case("a.b.c"), "ABC");
    }

    #[test]
    fn test_safe_field_name_reserved() {
        assert_eq!(safe_field_name("type"), "\"type\"");
        assert_eq!(safe_field_name("import"), "\"import\"");
        assert_eq!(safe_field_name("name"), "name");
        assert_eq!(safe_field_name("port"), "port");
    }

    #[test]
    fn test_roundtrip_generated_schema_parses() {
        let schema = serde_json::json!({
            "type": "object",
            "title": "Test",
            "properties": {
                "name": { "type": "string" },
                "count": { "type": "integer", "minimum": 0 }
            },
            "required": ["name"],
            "additionalProperties": false
        });

        let hone_source = generate_from_schema(&schema).unwrap();

        // Verify the generated source can be parsed by Hone
        let mut lexer = crate::lexer::Lexer::new(&hone_source, None);
        let tokens = lexer.tokenize().expect("generated source should lex");
        let mut parser = crate::parser::Parser::new(tokens, &hone_source, None);
        let ast = parser.parse().expect("generated source should parse");

        // Verify schema was found
        let has_schema = ast.preamble.iter().any(|item| {
            if let crate::parser::ast::PreambleItem::Schema(s) = item {
                s.name == "Test"
            } else {
                false
            }
        });
        assert!(has_schema, "parsed AST should contain schema 'Test'");
    }

    #[test]
    fn test_enum_becomes_base_type() {
        let schema = serde_json::json!({
            "type": "object",
            "title": "Status",
            "properties": {
                "state": { "type": "string", "enum": ["active", "inactive", "pending"] }
            },
            "required": ["state"]
        });

        let result = generate_from_schema(&schema).unwrap();
        assert!(
            result.contains("state: string"),
            "enum should map to base type, got: {}",
            result
        );
    }
}
