//! Type checker for Hone configuration language
//!
//! Validates values against schema definitions and type constraints.
//! Provides helpful error messages for type mismatches.

mod types;

pub use types::{FloatConstraints, IntConstraints, StringConstraints, Type, TypeEnv, TypeRegistry};

use crate::errors::{HoneError, HoneResult};
use crate::evaluator::Value;
use crate::lexer::token::SourceLocation;
use crate::parser::ast::{
    Expr, File, PreambleItem, SchemaDefinition, SchemaField, TypeAliasDefinition, TypeConstraint,
    TypeExpr,
};

use std::collections::{HashMap, HashSet};

/// Extract an integer value from a constraint expression, handling unary negation.
fn extract_int(expr: &crate::parser::ast::Expr) -> Option<i64> {
    use crate::parser::ast::{Expr, UnaryOp};
    match expr {
        Expr::Integer(n, _) => Some(*n),
        Expr::Unary(unary) if unary.op == UnaryOp::Neg => {
            if let Expr::Integer(n, _) = unary.operand.as_ref() {
                Some(-*n)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Extract a float value from a constraint expression, handling unary negation
/// and int-to-float promotion.
fn extract_float(expr: &crate::parser::ast::Expr) -> Option<f64> {
    use crate::parser::ast::{Expr, UnaryOp};
    match expr {
        Expr::Float(n, _) => Some(*n),
        Expr::Integer(n, _) => Some(*n as f64),
        Expr::Unary(unary) if unary.op == UnaryOp::Neg => {
            if let Expr::Float(n, _) = unary.operand.as_ref() {
                Some(-*n)
            } else if let Expr::Integer(n, _) = unary.operand.as_ref() {
                Some(-(*n as f64))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Type checker for Hone files
pub struct TypeChecker {
    /// Registry of defined schemas
    schemas: HashMap<String, Schema>,
    /// Registry of type aliases
    type_aliases: HashMap<String, Type>,
    /// Source code (for error messages)
    source: String,
    /// Paths marked with @unchecked (skip type checking)
    unchecked_paths: HashSet<String>,
    /// Cache of compiled regexes for string pattern constraints
    regex_cache: HashMap<String, regex::Regex>,
}

/// Compiled schema for type checking
#[derive(Debug, Clone)]
pub struct Schema {
    pub name: String,
    pub extends: Option<String>,
    pub fields: Vec<Field>,
    /// If true, extra fields are allowed (schema has `...`)
    pub open: bool,
}

/// Compiled field in a schema
#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub field_type: Type,
    pub optional: bool,
    pub default: Option<Value>,
}

impl TypeChecker {
    /// Create a new type checker
    pub fn new(source: String) -> Self {
        Self {
            schemas: HashMap::new(),
            type_aliases: HashMap::new(),
            source,
            unchecked_paths: HashSet::new(),
            regex_cache: HashMap::new(),
        }
    }

    /// Set paths that should skip type checking (@unchecked)
    pub fn set_unchecked_paths(&mut self, paths: HashSet<String>) {
        self.unchecked_paths = paths;
    }

    /// Collect and compile schema definitions and type aliases from a file.
    /// Regex patterns are pre-compiled and cached for O(1) lookups at check time.
    pub fn collect_schemas(&mut self, file: &File) -> HoneResult<()> {
        // First pass: collect type aliases (they may be referenced by schemas)
        for item in &file.preamble {
            if let PreambleItem::TypeAlias(alias_def) = item {
                let resolved_type = self.compile_type_alias(alias_def)?;
                self.cache_regex_from_type(&resolved_type);
                self.type_aliases
                    .insert(alias_def.name.clone(), resolved_type);
            }
        }

        // Second pass: collect schemas (which may reference type aliases)
        for item in &file.preamble {
            if let PreambleItem::Schema(schema_def) = item {
                let schema = self.compile_schema(schema_def)?;
                for field in &schema.fields {
                    self.cache_regex_from_type(&field.field_type);
                }
                self.schemas.insert(schema.name.clone(), schema);
            }
        }
        Ok(())
    }

    /// Pre-compile and cache any regex pattern found in a type.
    fn cache_regex_from_type(&mut self, ty: &Type) {
        if let Type::StringConstrained(c) = ty {
            if let Some(ref pat) = c.pattern {
                if !self.regex_cache.contains_key(pat) {
                    if let Ok(re) = regex::Regex::new(pat) {
                        self.regex_cache.insert(pat.clone(), re);
                    }
                }
            }
        }
    }

    /// Compile a type alias definition into a Type
    fn compile_type_alias(&self, def: &TypeAliasDefinition) -> HoneResult<Type> {
        self.compile_type_expr(&def.base_type)
    }

    /// Compile a type expression into a Type
    fn compile_type_expr(&self, expr: &TypeExpr) -> HoneResult<Type> {
        match expr {
            TypeExpr::Named { name, args } => {
                // Check if it's a type alias first (with no args)
                if args.is_empty() {
                    if let Some(t) = self.type_aliases.get(name) {
                        return Ok(t.clone());
                    }
                }

                // Handle built-in types with optional args
                match name.as_str() {
                    "string" => {
                        if args.is_empty() {
                            Ok(Type::String)
                        } else {
                            let mut constraints = StringConstraints::default();
                            let first_is_string = matches!(args.first(), Some(Expr::String(_)));

                            if first_is_string {
                                if let Some(Expr::String(s_expr)) = args.first() {
                                    if let Some(pat) = s_expr.as_literal() {
                                        if let Err(e) = regex::Regex::new(&pat) {
                                            return Err(HoneError::TypeMismatch {
                                                src: self.source.clone(),
                                                span: (0, 0).into(),
                                                expected: "valid regex pattern".to_string(),
                                                found: format!("\"{}\"", pat),
                                                help: format!("invalid regex: {}", e),
                                            });
                                        }
                                        constraints.pattern = Some(pat);
                                    }
                                }
                                if let Some(min) = args.get(1).and_then(extract_int) {
                                    constraints.min_len = Some(min as usize);
                                }
                                if let Some(max) = args.get(2).and_then(extract_int) {
                                    constraints.max_len = Some(max as usize);
                                }
                            } else {
                                if let Some(min) = args.first().and_then(extract_int) {
                                    constraints.min_len = Some(min as usize);
                                }
                                if let Some(max) = args.get(1).and_then(extract_int) {
                                    constraints.max_len = Some(max as usize);
                                }
                            }
                            Ok(Type::StringConstrained(constraints))
                        }
                    }
                    "int" => {
                        if args.is_empty() {
                            Ok(Type::Int)
                        } else {
                            let mut constraints = IntConstraints::default();
                            if let Some(min) = args.first().and_then(extract_int) {
                                constraints.min = Some(min);
                            }
                            if let Some(max) = args.get(1).and_then(extract_int) {
                                constraints.max = Some(max);
                            }
                            Ok(Type::IntConstrained(constraints))
                        }
                    }
                    "float" => {
                        if args.is_empty() {
                            Ok(Type::Float)
                        } else {
                            let mut constraints = FloatConstraints::default();
                            if let Some(min) = args.first().and_then(extract_float) {
                                constraints.min = Some(min);
                            }
                            if let Some(max) = args.get(1).and_then(extract_float) {
                                constraints.max = Some(max);
                            }
                            Ok(Type::FloatConstrained(constraints))
                        }
                    }
                    "bool" => Ok(Type::Bool),
                    "null" => Ok(Type::Null),
                    "any" => Ok(Type::Any),
                    "object" => Ok(Type::Object(None)),
                    "array" => Ok(Type::Array(Box::new(Type::Any))),
                    "number" => Ok(Type::Number),
                    _ => {
                        // Schema reference
                        Ok(Type::Schema(name.clone()))
                    }
                }
            }
            TypeExpr::Optional(inner) => {
                let inner_type = self.compile_type_expr(inner)?;
                Ok(Type::Optional(Box::new(inner_type)))
            }
            TypeExpr::Array(elem) => {
                let elem_type = self.compile_type_expr(elem)?;
                Ok(Type::Array(Box::new(elem_type)))
            }
            TypeExpr::Union(types) => {
                let compiled_types: Vec<Type> = types
                    .iter()
                    .map(|t| self.compile_type_expr(t))
                    .collect::<HoneResult<Vec<_>>>()?;
                Ok(Type::Union(compiled_types))
            }
        }
    }

    /// Compile a schema definition into a Schema
    fn compile_schema(&self, def: &SchemaDefinition) -> HoneResult<Schema> {
        let fields = def
            .fields
            .iter()
            .map(|f| self.compile_field(f))
            .collect::<HoneResult<Vec<_>>>()?;

        Ok(Schema {
            name: def.name.clone(),
            extends: def.extends.clone(),
            fields,
            open: def.open,
        })
    }

    /// Compile a schema field into a Field
    fn compile_field(&self, field: &SchemaField) -> HoneResult<Field> {
        let field_type = self.parse_type_constraint(&field.constraint)?;

        // If the type is a schema reference, check if it's actually a type alias
        let resolved_type = match &field_type {
            Type::Schema(name) => {
                if let Some(alias_type) = self.type_aliases.get(name) {
                    alias_type.clone()
                } else {
                    field_type
                }
            }
            _ => field_type,
        };

        Ok(Field {
            name: field.name.clone(),
            field_type: resolved_type,
            optional: field.optional,
            default: None, // Defaults are evaluated at runtime
        })
    }

    /// Parse a type constraint into a Type
    fn parse_type_constraint(&self, constraint: &TypeConstraint) -> HoneResult<Type> {
        let base_type = match constraint.name.as_str() {
            "string" => {
                if constraint.args.is_empty() {
                    Type::String
                } else {
                    let mut constraints = StringConstraints::default();

                    // Detect pattern: string("^[a-z]+$") - single string arg is a regex pattern
                    let first_is_string = matches!(constraint.args.first(), Some(Expr::String(_)));

                    if first_is_string {
                        // string("pattern") or string("pattern", min, max)
                        if let Some(Expr::String(s_expr)) = constraint.args.first() {
                            // Only plain string literals (no interpolation) are valid patterns
                            if let Some(pat) = s_expr.as_literal() {
                                if let Err(e) = regex::Regex::new(&pat) {
                                    return Err(HoneError::TypeMismatch {
                                        src: self.source.clone(),
                                        span: (
                                            constraint.location.offset,
                                            constraint.location.length,
                                        )
                                            .into(),
                                        expected: "valid regex pattern".to_string(),
                                        found: format!("\"{}\"", pat),
                                        help: format!("invalid regex: {}", e),
                                    });
                                }
                                constraints.pattern = Some(pat);
                            }
                        }
                        // Optional min/max after pattern: string("pattern", min, max)
                        if let Some(min) = constraint.args.get(1).and_then(extract_int) {
                            constraints.min_len = Some(min as usize);
                        }
                        if let Some(max) = constraint.args.get(2).and_then(extract_int) {
                            constraints.max_len = Some(max as usize);
                        }
                    } else {
                        // string(min, max) for length constraints
                        if let Some(min) = constraint.args.first().and_then(extract_int) {
                            constraints.min_len = Some(min as usize);
                        }
                        if let Some(max) = constraint.args.get(1).and_then(extract_int) {
                            constraints.max_len = Some(max as usize);
                        }
                    }

                    Type::StringConstrained(constraints)
                }
            }
            "int" => {
                if constraint.args.is_empty() {
                    Type::Int
                } else {
                    // int(min, max) for range constraints
                    let mut constraints = IntConstraints::default();
                    if let Some(min) = constraint.args.first().and_then(extract_int) {
                        constraints.min = Some(min);
                    }
                    if let Some(max) = constraint.args.get(1).and_then(extract_int) {
                        constraints.max = Some(max);
                    }
                    Type::IntConstrained(constraints)
                }
            }
            "float" => {
                if constraint.args.is_empty() {
                    Type::Float
                } else {
                    // float(min, max) for range constraints
                    let mut constraints = FloatConstraints::default();
                    if let Some(min) = constraint.args.first().and_then(extract_float) {
                        constraints.min = Some(min);
                    }
                    if let Some(max) = constraint.args.get(1).and_then(extract_float) {
                        constraints.max = Some(max);
                    }
                    Type::FloatConstrained(constraints)
                }
            }
            "bool" => Type::Bool,
            "null" => Type::Null,
            "any" => Type::Any,
            "array" => {
                if constraint.args.is_empty() {
                    Type::Array(Box::new(Type::Any))
                } else {
                    // For now, we don't parse nested type expressions
                    // This would need more work for array<string> syntax
                    Type::Array(Box::new(Type::Any))
                }
            }
            "object" => Type::Object(None),
            name => Type::Schema(name.to_string()),
        };

        Ok(base_type)
    }

    /// Check if a value matches the expected type
    pub fn check_type(
        &self,
        value: &Value,
        expected: &Type,
        location: &SourceLocation,
    ) -> HoneResult<()> {
        self.check_type_at_path(value, expected, location, "")
    }

    /// Check if a value matches the expected type, tracking the field path for @unchecked
    fn check_type_at_path(
        &self,
        value: &Value,
        expected: &Type,
        location: &SourceLocation,
        path: &str,
    ) -> HoneResult<()> {
        // Skip check if this path is marked @unchecked
        if !path.is_empty() && self.unchecked_paths.contains(path) {
            return Ok(());
        }

        match (value, expected) {
            // Any matches anything
            (_, Type::Any) => Ok(()),

            // Null type
            (Value::Null, Type::Null) => Ok(()),

            // Primitives
            (Value::Bool(_), Type::Bool) => Ok(()),
            (Value::Int(_), Type::Int) => Ok(()),
            (Value::Float(_), Type::Float) => Ok(()),
            (Value::String(_), Type::String) => Ok(()),

            // Constrained integer type
            (Value::Int(n), Type::IntConstrained(constraints)) => {
                if let Some(min) = constraints.min {
                    if *n < min {
                        return Err(HoneError::ValueOutOfRange {
                            src: self.source.clone(),
                            span: (location.offset, location.length).into(),
                            expected: format!("{}", expected),
                            value: format!("{}", n),
                            help: format!("value {} is less than minimum {}", n, min),
                        });
                    }
                }
                if let Some(max) = constraints.max {
                    if *n > max {
                        return Err(HoneError::ValueOutOfRange {
                            src: self.source.clone(),
                            span: (location.offset, location.length).into(),
                            expected: format!("{}", expected),
                            value: format!("{}", n),
                            help: format!("value {} is greater than maximum {}", n, max),
                        });
                    }
                }
                Ok(())
            }

            // Constrained float type
            (Value::Float(n), Type::FloatConstrained(constraints)) => {
                if let Some(min) = constraints.min {
                    if *n < min {
                        return Err(HoneError::ValueOutOfRange {
                            src: self.source.clone(),
                            span: (location.offset, location.length).into(),
                            expected: format!("{}", expected),
                            value: format!("{}", n),
                            help: format!("value {} is less than minimum {}", n, min),
                        });
                    }
                }
                if let Some(max) = constraints.max {
                    if *n > max {
                        return Err(HoneError::ValueOutOfRange {
                            src: self.source.clone(),
                            span: (location.offset, location.length).into(),
                            expected: format!("{}", expected),
                            value: format!("{}", n),
                            help: format!("value {} is greater than maximum {}", n, max),
                        });
                    }
                }
                Ok(())
            }

            // Constrained string type
            (Value::String(s), Type::StringConstrained(constraints)) => {
                let char_count = s.chars().count();
                if let Some(min_len) = constraints.min_len {
                    if char_count < min_len {
                        return Err(HoneError::ValueOutOfRange {
                            src: self.source.clone(),
                            span: (location.offset, location.length).into(),
                            expected: format!("{}", expected),
                            value: format!("string of length {}", char_count),
                            help: format!(
                                "string length {} is less than minimum {}",
                                char_count, min_len
                            ),
                        });
                    }
                }
                if let Some(max_len) = constraints.max_len {
                    if char_count > max_len {
                        return Err(HoneError::ValueOutOfRange {
                            src: self.source.clone(),
                            span: (location.offset, location.length).into(),
                            expected: format!("{}", expected),
                            value: format!("string of length {}", char_count),
                            help: format!(
                                "string length {} is greater than maximum {}",
                                char_count, max_len
                            ),
                        });
                    }
                }
                if let Some(ref pattern) = constraints.pattern {
                    // Use cached regex (pre-compiled in collect_schemas), fall back to compiling
                    let re = self
                        .regex_cache
                        .get(pattern)
                        .cloned()
                        .or_else(|| regex::Regex::new(pattern).ok());
                    match re {
                        Some(re) => {
                            if !re.is_match(s) {
                                return Err(HoneError::PatternMismatch {
                                    src: self.source.clone(),
                                    span: (location.offset, location.length).into(),
                                    pattern: pattern.clone(),
                                    value: s.clone(),
                                    help: format!(
                                        "string \"{}\" does not match pattern /{}/",
                                        s, pattern
                                    ),
                                });
                            }
                        }
                        None => {
                            return Err(HoneError::TypeMismatch {
                                src: self.source.clone(),
                                span: (location.offset, location.length).into(),
                                expected: "valid regex pattern".to_string(),
                                found: format!("\"{}\"", pattern),
                                help: format!("invalid regex pattern: \"{}\"", pattern),
                            });
                        }
                    }
                }
                Ok(())
            }

            // String literal type (for union of literals)
            (Value::String(s), Type::StringLiteral(expected_s)) => {
                if s == expected_s {
                    Ok(())
                } else {
                    Err(HoneError::TypeMismatch {
                        src: self.source.clone(),
                        span: (location.offset, location.length).into(),
                        expected: format!("\"{}\"", expected_s),
                        found: format!("\"{}\"", s),
                        help: format!("expected literal \"{}\" but got \"{}\"", expected_s, s),
                    })
                }
            }

            // Number matches int or float
            (Value::Int(_), Type::Number) | (Value::Float(_), Type::Number) => Ok(()),

            // Arrays
            (Value::Array(items), Type::Array(elem_type)) => {
                for (i, item) in items.iter().enumerate() {
                    self.check_type_at_path(item, elem_type, location, path)
                        .map_err(|e| self.annotate_array_error(e, i))?;
                }
                Ok(())
            }

            // Objects without schema
            (Value::Object(_), Type::Object(None)) => Ok(()),

            // Objects with schema
            (Value::Object(obj), Type::Schema(schema_name)) => {
                self.check_schema_at_path(obj, schema_name, location, path)
            }

            // Union types
            (value, Type::Union(types)) => {
                for t in types {
                    if self.check_type_at_path(value, t, location, path).is_ok() {
                        return Ok(());
                    }
                }
                Err(HoneError::TypeMismatch {
                    src: self.source.clone(),
                    span: (location.offset, location.length).into(),
                    expected: format!("{}", Type::Union(types.clone())),
                    found: value.type_name().to_string(),
                    help: "value does not match any type in the union".to_string(),
                })
            }

            // Optional type (can be null or the inner type)
            (Value::Null, Type::Optional(_)) => Ok(()),
            (value, Type::Optional(inner)) => self.check_type_at_path(value, inner, location, path),

            // Type mismatch
            (value, expected) => Err(HoneError::TypeMismatch {
                src: self.source.clone(),
                span: (location.offset, location.length).into(),
                expected: format!("{}", expected),
                found: value.type_name().to_string(),
                help: if path.is_empty() {
                    format!("expected {}, got {}", expected, value.type_name())
                } else {
                    format!(
                        "at {}: expected {}, got {}",
                        path,
                        expected,
                        value.type_name()
                    )
                },
            }),
        }
    }

    /// Check if an object matches a schema, tracking field paths for @unchecked
    fn check_schema_at_path(
        &self,
        obj: &indexmap::IndexMap<String, Value>,
        schema_name: &str,
        location: &SourceLocation,
        path: &str,
    ) -> HoneResult<()> {
        // Collect all known field names across the inheritance chain
        let mut known_fields: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let mut is_open = false;
        self.collect_schema_fields(schema_name, &mut known_fields, &mut is_open);

        // Validate all fields in the inheritance chain
        self.validate_schema_fields(obj, schema_name, location, path)?;

        // Reject unknown fields if schema is closed (only at top level)
        if !is_open {
            for key in obj.keys() {
                if !known_fields.contains(key.as_str()) {
                    let mut defined: Vec<_> = known_fields.iter().copied().collect();
                    defined.sort();
                    return Err(HoneError::UnknownField {
                        src: self.source.clone(),
                        span: (location.offset, location.length).into(),
                        field: key.clone(),
                        schema: schema_name.to_string(),
                        help: format!(
                            "defined fields: {}; add '...' to the schema to allow extra fields",
                            defined.join(", ")
                        ),
                    });
                }
            }
        }

        Ok(())
    }

    /// Validate that an object satisfies a schema's field requirements (recursive through inheritance)
    fn validate_schema_fields(
        &self,
        obj: &indexmap::IndexMap<String, Value>,
        schema_name: &str,
        location: &SourceLocation,
        path: &str,
    ) -> HoneResult<()> {
        let schema = self
            .schemas
            .get(schema_name)
            .ok_or_else(|| HoneError::UndefinedVariable {
                src: self.source.clone(),
                span: (location.offset, location.length).into(),
                name: schema_name.to_string(),
                help: format!("define schema '{}' before using it", schema_name),
            })?;

        // Check parent schema fields first
        if let Some(ref parent_name) = schema.extends {
            self.validate_schema_fields(obj, parent_name, location, path)?;
        }

        // Check this schema's fields
        for field in &schema.fields {
            let field_path = if path.is_empty() {
                field.name.clone()
            } else {
                format!("{}.{}", path, field.name)
            };

            match obj.get(&field.name) {
                Some(value) => {
                    self.check_type_at_path(value, &field.field_type, location, &field_path)?;
                }
                None if !field.optional => {
                    return Err(HoneError::MissingField {
                        src: self.source.clone(),
                        span: (location.offset, location.length).into(),
                        field: field.name.clone(),
                        schema: schema_name.to_string(),
                    });
                }
                None => {} // Optional field not present, OK
            }
        }

        Ok(())
    }

    /// Recursively collect all field names from a schema and its parents
    fn collect_schema_fields<'a>(
        &'a self,
        schema_name: &str,
        known: &mut std::collections::HashSet<&'a str>,
        is_open: &mut bool,
    ) {
        if let Some(schema) = self.schemas.get(schema_name) {
            for field in &schema.fields {
                known.insert(&field.name);
            }
            if schema.open {
                *is_open = true;
            }
            if let Some(ref parent) = schema.extends {
                self.collect_schema_fields(parent, known, is_open);
            }
        }
    }

    /// Get a schema by name
    pub fn get_schema(&self, name: &str) -> Option<&Schema> {
        self.schemas.get(name)
    }

    /// Annotate an error with array index context
    fn annotate_array_error(&self, err: HoneError, index: usize) -> HoneError {
        match err {
            HoneError::TypeMismatch {
                src,
                span,
                expected,
                found,
                help,
            } => HoneError::TypeMismatch {
                src,
                span,
                expected,
                found,
                help: format!("at array index {}: {}", index, help),
            },
            HoneError::ValueOutOfRange {
                src,
                span,
                expected,
                value,
                help,
            } => HoneError::ValueOutOfRange {
                src,
                span,
                expected: format!("at array index {}: {}", index, expected),
                value,
                help: format!("at array index {}: {}", index, help),
            },
            HoneError::PatternMismatch {
                src,
                span,
                pattern,
                value,
                help,
            } => HoneError::PatternMismatch {
                src,
                span,
                pattern,
                value,
                help: format!("at array index {}: {}", index, help),
            },
            e => e,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evaluator::Value;
    use indexmap::IndexMap;

    fn loc() -> SourceLocation {
        SourceLocation {
            file: None,
            line: 1,
            column: 1,
            offset: 0,
            length: 1,
        }
    }

    #[test]
    fn test_check_primitive_types() {
        let checker = TypeChecker::new("test".into());

        assert!(checker
            .check_type(&Value::Bool(true), &Type::Bool, &loc())
            .is_ok());
        assert!(checker
            .check_type(&Value::Int(42), &Type::Int, &loc())
            .is_ok());
        assert!(checker
            .check_type(&Value::Float(3.14), &Type::Float, &loc())
            .is_ok());
        assert!(checker
            .check_type(&Value::String("hello".into()), &Type::String, &loc())
            .is_ok());
        assert!(checker
            .check_type(&Value::Null, &Type::Null, &loc())
            .is_ok());
    }

    #[test]
    fn test_check_type_mismatch() {
        let checker = TypeChecker::new("test".into());

        assert!(checker
            .check_type(&Value::Int(42), &Type::String, &loc())
            .is_err());
        assert!(checker
            .check_type(&Value::String("hello".into()), &Type::Bool, &loc())
            .is_err());
    }

    #[test]
    fn test_check_any_type() {
        let checker = TypeChecker::new("test".into());

        assert!(checker
            .check_type(&Value::Int(42), &Type::Any, &loc())
            .is_ok());
        assert!(checker
            .check_type(&Value::String("hello".into()), &Type::Any, &loc())
            .is_ok());
        assert!(checker
            .check_type(&Value::Array(vec![]), &Type::Any, &loc())
            .is_ok());
    }

    #[test]
    fn test_check_array_type() {
        let checker = TypeChecker::new("test".into());

        let arr = Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        assert!(checker
            .check_type(&arr, &Type::Array(Box::new(Type::Int)), &loc())
            .is_ok());

        let mixed_arr = Value::Array(vec![Value::Int(1), Value::String("hello".into())]);
        assert!(checker
            .check_type(&mixed_arr, &Type::Array(Box::new(Type::Int)), &loc())
            .is_err());
    }

    #[test]
    fn test_check_number_type() {
        let checker = TypeChecker::new("test".into());

        assert!(checker
            .check_type(&Value::Int(42), &Type::Number, &loc())
            .is_ok());
        assert!(checker
            .check_type(&Value::Float(3.14), &Type::Number, &loc())
            .is_ok());
        assert!(checker
            .check_type(&Value::String("42".into()), &Type::Number, &loc())
            .is_err());
    }

    #[test]
    fn test_check_optional_type() {
        let checker = TypeChecker::new("test".into());

        let opt_int = Type::Optional(Box::new(Type::Int));
        assert!(checker
            .check_type(&Value::Int(42), &opt_int, &loc())
            .is_ok());
        assert!(checker.check_type(&Value::Null, &opt_int, &loc()).is_ok());
        assert!(checker
            .check_type(&Value::String("hello".into()), &opt_int, &loc())
            .is_err());
    }

    #[test]
    fn test_check_union_type() {
        let checker = TypeChecker::new("test".into());

        let str_or_int = Type::Union(vec![Type::String, Type::Int]);
        assert!(checker
            .check_type(&Value::Int(42), &str_or_int, &loc())
            .is_ok());
        assert!(checker
            .check_type(&Value::String("hello".into()), &str_or_int, &loc())
            .is_ok());
        assert!(checker
            .check_type(&Value::Bool(true), &str_or_int, &loc())
            .is_err());
    }

    #[test]
    fn test_check_schema() {
        let mut checker = TypeChecker::new("test".into());

        // Define a simple schema
        checker.schemas.insert(
            "Server".into(),
            Schema {
                name: "Server".into(),
                extends: None,
                fields: vec![
                    Field {
                        name: "host".into(),
                        field_type: Type::String,
                        optional: false,
                        default: None,
                    },
                    Field {
                        name: "port".into(),
                        field_type: Type::Int,
                        optional: false,
                        default: None,
                    },
                    Field {
                        name: "debug".into(),
                        field_type: Type::Bool,
                        optional: true,
                        default: None,
                    },
                ],
                open: false,
            },
        );

        // Valid object
        let mut valid_obj = IndexMap::new();
        valid_obj.insert("host".into(), Value::String("localhost".into()));
        valid_obj.insert("port".into(), Value::Int(8080));
        assert!(checker
            .check_type(
                &Value::Object(valid_obj),
                &Type::Schema("Server".into()),
                &loc()
            )
            .is_ok());

        // Missing required field
        let mut missing_field = IndexMap::new();
        missing_field.insert("host".into(), Value::String("localhost".into()));
        assert!(checker
            .check_type(
                &Value::Object(missing_field),
                &Type::Schema("Server".into()),
                &loc()
            )
            .is_err());

        // Wrong type for field
        let mut wrong_type = IndexMap::new();
        wrong_type.insert("host".into(), Value::String("localhost".into()));
        wrong_type.insert("port".into(), Value::String("8080".into()));
        assert!(checker
            .check_type(
                &Value::Object(wrong_type),
                &Type::Schema("Server".into()),
                &loc()
            )
            .is_err());
    }

    #[test]
    fn test_schema_extends() {
        let mut checker = TypeChecker::new("test".into());

        // Base schema
        checker.schemas.insert(
            "Base".into(),
            Schema {
                name: "Base".into(),
                extends: None,
                fields: vec![Field {
                    name: "name".into(),
                    field_type: Type::String,
                    optional: false,
                    default: None,
                }],
                open: false,
            },
        );

        // Extended schema
        checker.schemas.insert(
            "Extended".into(),
            Schema {
                name: "Extended".into(),
                extends: Some("Base".into()),
                fields: vec![Field {
                    name: "count".into(),
                    field_type: Type::Int,
                    optional: false,
                    default: None,
                }],
                open: false,
            },
        );

        // Object must have both name (from Base) and count (from Extended)
        let mut valid = IndexMap::new();
        valid.insert("name".into(), Value::String("test".into()));
        valid.insert("count".into(), Value::Int(42));
        assert!(checker
            .check_type(
                &Value::Object(valid),
                &Type::Schema("Extended".into()),
                &loc()
            )
            .is_ok());

        // Missing parent field
        let mut missing_parent = IndexMap::new();
        missing_parent.insert("count".into(), Value::Int(42));
        assert!(checker
            .check_type(
                &Value::Object(missing_parent),
                &Type::Schema("Extended".into()),
                &loc()
            )
            .is_err());
    }

    #[test]
    fn test_check_int_constrained() {
        let checker = TypeChecker::new("test".into());

        // int(1, 100) - min 1, max 100
        let port_type = Type::IntConstrained(IntConstraints {
            min: Some(1),
            max: Some(100),
        });

        // Valid values
        assert!(checker
            .check_type(&Value::Int(1), &port_type, &loc())
            .is_ok());
        assert!(checker
            .check_type(&Value::Int(50), &port_type, &loc())
            .is_ok());
        assert!(checker
            .check_type(&Value::Int(100), &port_type, &loc())
            .is_ok());

        // Below minimum
        assert!(checker
            .check_type(&Value::Int(0), &port_type, &loc())
            .is_err());
        assert!(checker
            .check_type(&Value::Int(-5), &port_type, &loc())
            .is_err());

        // Above maximum
        assert!(checker
            .check_type(&Value::Int(101), &port_type, &loc())
            .is_err());
        assert!(checker
            .check_type(&Value::Int(999), &port_type, &loc())
            .is_err());

        // Wrong type entirely
        assert!(checker
            .check_type(&Value::String("50".into()), &port_type, &loc())
            .is_err());
    }

    #[test]
    fn test_check_int_constrained_min_only() {
        let checker = TypeChecker::new("test".into());

        // int(0, _) - min 0, no max
        let positive_type = Type::IntConstrained(IntConstraints {
            min: Some(0),
            max: None,
        });

        assert!(checker
            .check_type(&Value::Int(0), &positive_type, &loc())
            .is_ok());
        assert!(checker
            .check_type(&Value::Int(999999), &positive_type, &loc())
            .is_ok());
        assert!(checker
            .check_type(&Value::Int(-1), &positive_type, &loc())
            .is_err());
    }

    #[test]
    fn test_check_int_constrained_max_only() {
        let checker = TypeChecker::new("test".into());

        // int(_, 10) - no min, max 10
        let capped_type = Type::IntConstrained(IntConstraints {
            min: None,
            max: Some(10),
        });

        assert!(checker
            .check_type(&Value::Int(-999), &capped_type, &loc())
            .is_ok());
        assert!(checker
            .check_type(&Value::Int(10), &capped_type, &loc())
            .is_ok());
        assert!(checker
            .check_type(&Value::Int(11), &capped_type, &loc())
            .is_err());
    }

    #[test]
    fn test_check_string_constrained() {
        let checker = TypeChecker::new("test".into());

        // string(1, 10) - min length 1, max length 10
        let name_type = Type::StringConstrained(StringConstraints {
            min_len: Some(1),
            max_len: Some(10),
            pattern: None,
        });

        // Valid values
        assert!(checker
            .check_type(&Value::String("a".into()), &name_type, &loc())
            .is_ok());
        assert!(checker
            .check_type(&Value::String("hello".into()), &name_type, &loc())
            .is_ok());
        assert!(checker
            .check_type(&Value::String("0123456789".into()), &name_type, &loc())
            .is_ok());

        // Too short
        assert!(checker
            .check_type(&Value::String("".into()), &name_type, &loc())
            .is_err());

        // Too long
        assert!(checker
            .check_type(&Value::String("12345678901".into()), &name_type, &loc())
            .is_err());

        // Wrong type
        assert!(checker
            .check_type(&Value::Int(42), &name_type, &loc())
            .is_err());
    }

    #[test]
    fn test_check_float_constrained() {
        let checker = TypeChecker::new("test".into());

        // float(0.0, 1.0) - percentage
        let percentage_type = Type::FloatConstrained(FloatConstraints {
            min: Some(0.0),
            max: Some(1.0),
        });

        // Valid values
        assert!(checker
            .check_type(&Value::Float(0.0), &percentage_type, &loc())
            .is_ok());
        assert!(checker
            .check_type(&Value::Float(0.5), &percentage_type, &loc())
            .is_ok());
        assert!(checker
            .check_type(&Value::Float(1.0), &percentage_type, &loc())
            .is_ok());

        // Out of range
        assert!(checker
            .check_type(&Value::Float(-0.1), &percentage_type, &loc())
            .is_err());
        assert!(checker
            .check_type(&Value::Float(1.1), &percentage_type, &loc())
            .is_err());
    }

    #[test]
    fn test_check_string_literal() {
        let checker = TypeChecker::new("test".into());

        let env_type = Type::StringLiteral("production".into());

        // Exact match
        assert!(checker
            .check_type(&Value::String("production".into()), &env_type, &loc())
            .is_ok());

        // Wrong value
        assert!(checker
            .check_type(&Value::String("staging".into()), &env_type, &loc())
            .is_err());
        assert!(checker
            .check_type(&Value::String("Production".into()), &env_type, &loc())
            .is_err());
    }

    #[test]
    fn test_check_union_with_string_literals() {
        let checker = TypeChecker::new("test".into());

        // "dev" | "staging" | "production"
        let env_type = Type::Union(vec![
            Type::StringLiteral("dev".into()),
            Type::StringLiteral("staging".into()),
            Type::StringLiteral("production".into()),
        ]);

        // Valid values
        assert!(checker
            .check_type(&Value::String("dev".into()), &env_type, &loc())
            .is_ok());
        assert!(checker
            .check_type(&Value::String("staging".into()), &env_type, &loc())
            .is_ok());
        assert!(checker
            .check_type(&Value::String("production".into()), &env_type, &loc())
            .is_ok());

        // Invalid values
        assert!(checker
            .check_type(&Value::String("test".into()), &env_type, &loc())
            .is_err());
        assert!(checker
            .check_type(&Value::String("prod".into()), &env_type, &loc())
            .is_err());
    }

    #[test]
    fn test_unchecked_skips_type_mismatch() {
        let mut checker = TypeChecker::new("test".into());

        checker.schemas.insert(
            "Config".into(),
            Schema {
                name: "Config".into(),
                extends: None,
                fields: vec![Field {
                    name: "port".into(),
                    field_type: Type::IntConstrained(IntConstraints {
                        min: Some(1),
                        max: Some(65535),
                    }),
                    optional: false,
                    default: None,
                }],
                open: false,
            },
        );

        // Mark "port" as unchecked
        let mut unchecked = HashSet::new();
        unchecked.insert("port".to_string());
        checker.set_unchecked_paths(unchecked);

        // Value that would normally fail constraint (99999 > 65535)
        let mut obj = IndexMap::new();
        obj.insert("port".into(), Value::Int(99999));
        assert!(checker
            .check_type(&Value::Object(obj), &Type::Schema("Config".into()), &loc())
            .is_ok());
    }

    #[test]
    fn test_unchecked_non_annotated_still_checked() {
        let mut checker = TypeChecker::new("test".into());

        checker.schemas.insert(
            "Config".into(),
            Schema {
                name: "Config".into(),
                extends: None,
                fields: vec![
                    Field {
                        name: "port".into(),
                        field_type: Type::IntConstrained(IntConstraints {
                            min: Some(1),
                            max: Some(65535),
                        }),
                        optional: false,
                        default: None,
                    },
                    Field {
                        name: "host".into(),
                        field_type: Type::String,
                        optional: false,
                        default: None,
                    },
                ],
                open: false,
            },
        );

        // Mark only "port" as unchecked, "host" is still checked
        let mut unchecked = HashSet::new();
        unchecked.insert("port".to_string());
        checker.set_unchecked_paths(unchecked);

        // host has wrong type (int instead of string) - should fail
        let mut obj = IndexMap::new();
        obj.insert("port".into(), Value::Int(99999));
        obj.insert("host".into(), Value::Int(42));
        assert!(checker
            .check_type(&Value::Object(obj), &Type::Schema("Config".into()), &loc())
            .is_err());
    }

    #[test]
    fn test_unchecked_nested_schema() {
        let mut checker = TypeChecker::new("test".into());

        checker.schemas.insert(
            "Server".into(),
            Schema {
                name: "Server".into(),
                extends: None,
                fields: vec![Field {
                    name: "port".into(),
                    field_type: Type::IntConstrained(IntConstraints {
                        min: Some(1),
                        max: Some(65535),
                    }),
                    optional: false,
                    default: None,
                }],
                open: false,
            },
        );

        checker.schemas.insert(
            "Config".into(),
            Schema {
                name: "Config".into(),
                extends: None,
                fields: vec![Field {
                    name: "server".into(),
                    field_type: Type::Schema("Server".into()),
                    optional: false,
                    default: None,
                }],
                open: false,
            },
        );

        // Mark "server.port" as unchecked
        let mut unchecked = HashSet::new();
        unchecked.insert("server.port".to_string());
        checker.set_unchecked_paths(unchecked);

        let mut server_obj = IndexMap::new();
        server_obj.insert("port".into(), Value::Int(99999));
        let mut config_obj = IndexMap::new();
        config_obj.insert("server".into(), Value::Object(server_obj));

        assert!(checker
            .check_type(
                &Value::Object(config_obj),
                &Type::Schema("Config".into()),
                &loc()
            )
            .is_ok());
    }

    #[test]
    fn test_unchecked_without_annotation_still_fails() {
        let mut checker = TypeChecker::new("test".into());

        checker.schemas.insert(
            "Config".into(),
            Schema {
                name: "Config".into(),
                extends: None,
                fields: vec![Field {
                    name: "port".into(),
                    field_type: Type::IntConstrained(IntConstraints {
                        min: Some(1),
                        max: Some(65535),
                    }),
                    optional: false,
                    default: None,
                }],
                open: false,
            },
        );

        // No unchecked paths set - constraint violation should fail
        let mut obj = IndexMap::new();
        obj.insert("port".into(), Value::Int(99999));
        assert!(checker
            .check_type(&Value::Object(obj), &Type::Schema("Config".into()), &loc())
            .is_err());
    }

    #[test]
    fn test_unchecked_multiple_fields() {
        let mut checker = TypeChecker::new("test".into());

        checker.schemas.insert(
            "Config".into(),
            Schema {
                name: "Config".into(),
                extends: None,
                fields: vec![
                    Field {
                        name: "port".into(),
                        field_type: Type::IntConstrained(IntConstraints {
                            min: Some(1),
                            max: Some(65535),
                        }),
                        optional: false,
                        default: None,
                    },
                    Field {
                        name: "name".into(),
                        field_type: Type::StringConstrained(StringConstraints {
                            min_len: Some(1),
                            max_len: Some(10),
                            pattern: None,
                        }),
                        optional: false,
                        default: None,
                    },
                ],
                open: false,
            },
        );

        // Mark both fields as unchecked
        let mut unchecked = HashSet::new();
        unchecked.insert("port".to_string());
        unchecked.insert("name".to_string());
        checker.set_unchecked_paths(unchecked);

        // Both values violate constraints but should pass
        let mut obj = IndexMap::new();
        obj.insert("port".into(), Value::Int(99999));
        obj.insert(
            "name".into(),
            Value::String("this name is way too long".into()),
        );
        assert!(checker
            .check_type(&Value::Object(obj), &Type::Schema("Config".into()), &loc())
            .is_ok());
    }
}
