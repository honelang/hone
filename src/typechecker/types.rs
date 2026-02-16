//! Type representations for the Hone type system
//!
//! Types are used for validation and inference.

use std::collections::HashMap;
use std::fmt;

/// Constraints for integer types
#[derive(Debug, Clone, PartialEq, Default)]
pub struct IntConstraints {
    pub min: Option<i64>,
    pub max: Option<i64>,
}

/// Constraints for string types
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StringConstraints {
    pub min_len: Option<usize>,
    pub max_len: Option<usize>,
    pub pattern: Option<String>,
}

/// Constraints for float types
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FloatConstraints {
    pub min: Option<f64>,
    pub max: Option<f64>,
}

/// A type in the Hone type system
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    /// Any type (matches everything)
    Any,
    /// Null type
    Null,
    /// Boolean type
    Bool,
    /// Integer type (64-bit signed)
    Int,
    /// Integer type with constraints (min, max)
    IntConstrained(IntConstraints),
    /// Float type (64-bit)
    Float,
    /// Float type with constraints
    FloatConstrained(FloatConstraints),
    /// Number type (int or float)
    Number,
    /// String type
    String,
    /// String type with constraints
    StringConstrained(StringConstraints),
    /// A specific string literal (for union types)
    StringLiteral(std::string::String),
    /// Array type with element type
    Array(Box<Type>),
    /// Object type (optionally with a schema)
    Object(Option<Box<Type>>),
    /// Reference to a named schema
    Schema(std::string::String),
    /// Union of multiple types
    Union(Vec<Type>),
    /// Optional type (T | null)
    Optional(Box<Type>),
    /// Map type (object with string keys and typed values)
    Map(Box<Type>),
}

impl Type {
    /// Check if this is the Any type
    pub fn is_any(&self) -> bool {
        matches!(self, Type::Any)
    }

    /// Check if this type is optional
    pub fn is_optional(&self) -> bool {
        matches!(self, Type::Optional(_))
    }

    /// Make a type optional
    pub fn optional(self) -> Type {
        match self {
            Type::Optional(_) => self, // Already optional
            Type::Null => Type::Null,  // Null is already nullable
            t => Type::Optional(Box::new(t)),
        }
    }

    /// Create an array type
    pub fn array(elem_type: Type) -> Type {
        Type::Array(Box::new(elem_type))
    }

    /// Create a union type
    pub fn union(types: Vec<Type>) -> Type {
        if types.len() == 1 {
            types.into_iter().next().unwrap()
        } else {
            Type::Union(types)
        }
    }

    /// Parse a type from a string (simple types only)
    pub fn from_name(name: &str) -> Option<Type> {
        match name {
            "any" => Some(Type::Any),
            "null" => Some(Type::Null),
            "bool" | "boolean" => Some(Type::Bool),
            "int" | "integer" => Some(Type::Int),
            "float" | "double" => Some(Type::Float),
            "number" => Some(Type::Number),
            "string" | "str" => Some(Type::String),
            "array" => Some(Type::Array(Box::new(Type::Any))),
            "object" => Some(Type::Object(None)),
            _ => None,
        }
    }

    /// Check if a type is a subtype of another
    pub fn is_subtype_of(&self, other: &Type) -> bool {
        match (self, other) {
            // Any is a supertype of everything
            (_, Type::Any) => true,

            // Same types
            (Type::Bool, Type::Bool)
            | (Type::Int, Type::Int)
            | (Type::Float, Type::Float)
            | (Type::String, Type::String)
            | (Type::Null, Type::Null) => true,

            // Constrained int is subtype of int
            (Type::IntConstrained(_), Type::Int) => true,
            (Type::IntConstrained(_), Type::Number) => true,

            // Constrained float is subtype of float
            (Type::FloatConstrained(_), Type::Float) => true,
            (Type::FloatConstrained(_), Type::Number) => true,

            // Constrained string is subtype of string
            (Type::StringConstrained(_), Type::String) => true,

            // String literal is subtype of string
            (Type::StringLiteral(_), Type::String) => true,

            // Int and Float are subtypes of Number
            (Type::Int, Type::Number) | (Type::Float, Type::Number) => true,

            // Arrays are covariant
            (Type::Array(a), Type::Array(b)) => a.is_subtype_of(b),

            // Optional types
            (Type::Null, Type::Optional(_)) => true,
            (t, Type::Optional(inner)) => t.is_subtype_of(inner),

            // Union types
            (t, Type::Union(types)) => types.iter().any(|u| t.is_subtype_of(u)),
            (Type::Union(types), t) => types.iter().all(|u| u.is_subtype_of(t)),

            // Schema types are nominal
            (Type::Schema(a), Type::Schema(b)) => a == b,

            _ => false,
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Any => write!(f, "any"),
            Type::Null => write!(f, "null"),
            Type::Bool => write!(f, "bool"),
            Type::Int => write!(f, "int"),
            Type::IntConstrained(c) => match (c.min, c.max) {
                (Some(min), Some(max)) => write!(f, "int({}, {})", min, max),
                (Some(min), None) => write!(f, "int({}, _)", min),
                (None, Some(max)) => write!(f, "int(_, {})", max),
                (None, None) => write!(f, "int"),
            },
            Type::Float => write!(f, "float"),
            Type::FloatConstrained(c) => match (c.min, c.max) {
                (Some(min), Some(max)) => write!(f, "float({}, {})", min, max),
                (Some(min), None) => write!(f, "float({}, _)", min),
                (None, Some(max)) => write!(f, "float(_, {})", max),
                (None, None) => write!(f, "float"),
            },
            Type::Number => write!(f, "number"),
            Type::String => write!(f, "string"),
            Type::StringConstrained(c) => {
                if let Some(ref pattern) = c.pattern {
                    write!(f, "string(pattern: \"{}\")", pattern)
                } else {
                    match (c.min_len, c.max_len) {
                        (Some(min), Some(max)) => write!(f, "string({}, {})", min, max),
                        (Some(min), None) => write!(f, "string({}, _)", min),
                        (None, Some(max)) => write!(f, "string(_, {})", max),
                        (None, None) => write!(f, "string"),
                    }
                }
            }
            Type::StringLiteral(s) => write!(f, "\"{}\"", s),
            Type::Array(elem) => write!(f, "array<{}>", elem),
            Type::Object(None) => write!(f, "object"),
            Type::Object(Some(val)) => write!(f, "object<{}>", val),
            Type::Schema(name) => write!(f, "{}", name),
            Type::Union(types) => {
                let types_str: Vec<_> = types.iter().map(|t| t.to_string()).collect();
                write!(f, "{}", types_str.join(" | "))
            }
            Type::Optional(inner) => write!(f, "{}?", inner),
            Type::Map(val) => write!(f, "map<{}>", val),
        }
    }
}

/// Type environment for tracking types in scope
#[derive(Debug, Default)]
pub struct TypeEnv {
    /// Variable types
    bindings: HashMap<String, Type>,
    /// Parent environment
    parent: Option<Box<TypeEnv>>,
}

impl TypeEnv {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a child environment
    pub fn child(&self) -> Self {
        Self {
            bindings: HashMap::new(),
            parent: Some(Box::new(self.clone())),
        }
    }

    /// Define a variable type
    pub fn define(&mut self, name: impl Into<String>, typ: Type) {
        self.bindings.insert(name.into(), typ);
    }

    /// Look up a variable type
    pub fn get(&self, name: &str) -> Option<&Type> {
        self.bindings
            .get(name)
            .or_else(|| self.parent.as_ref().and_then(|p| p.get(name)))
    }
}

impl Clone for TypeEnv {
    fn clone(&self) -> Self {
        Self {
            bindings: self.bindings.clone(),
            parent: self.parent.clone(),
        }
    }
}

/// Registry for named types and schemas
#[derive(Debug, Default)]
pub struct TypeRegistry {
    /// Named types (schemas and aliases)
    types: HashMap<String, Type>,
}

impl TypeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a named type
    pub fn register(&mut self, name: impl Into<String>, typ: Type) {
        self.types.insert(name.into(), typ);
    }

    /// Look up a named type
    pub fn get(&self, name: &str) -> Option<&Type> {
        self.types.get(name)
    }

    /// Check if a type name is registered
    pub fn contains(&self, name: &str) -> bool {
        self.types.contains_key(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_display() {
        assert_eq!(format!("{}", Type::Int), "int");
        assert_eq!(format!("{}", Type::array(Type::String)), "array<string>");
        assert_eq!(format!("{}", Type::Int.optional()), "int?");
        assert_eq!(
            format!("{}", Type::union(vec![Type::String, Type::Int])),
            "string | int"
        );
    }

    #[test]
    fn test_subtype_primitives() {
        assert!(Type::Int.is_subtype_of(&Type::Int));
        assert!(Type::Int.is_subtype_of(&Type::Any));
        assert!(Type::Int.is_subtype_of(&Type::Number));
        assert!(Type::Float.is_subtype_of(&Type::Number));
        assert!(!Type::String.is_subtype_of(&Type::Number));
    }

    #[test]
    fn test_subtype_optional() {
        let opt_int = Type::Optional(Box::new(Type::Int));
        assert!(Type::Int.is_subtype_of(&opt_int));
        assert!(Type::Null.is_subtype_of(&opt_int));
        assert!(!Type::String.is_subtype_of(&opt_int));
    }

    #[test]
    fn test_subtype_union() {
        let str_or_int = Type::union(vec![Type::String, Type::Int]);
        assert!(Type::Int.is_subtype_of(&str_or_int));
        assert!(Type::String.is_subtype_of(&str_or_int));
        assert!(!Type::Bool.is_subtype_of(&str_or_int));
    }

    #[test]
    fn test_subtype_array() {
        let int_arr = Type::array(Type::Int);
        let any_arr = Type::array(Type::Any);
        assert!(int_arr.is_subtype_of(&any_arr));
        assert!(!any_arr.is_subtype_of(&int_arr));
    }

    #[test]
    fn test_from_name() {
        assert_eq!(Type::from_name("int"), Some(Type::Int));
        assert_eq!(Type::from_name("string"), Some(Type::String));
        assert_eq!(Type::from_name("bool"), Some(Type::Bool));
        assert_eq!(Type::from_name("unknown"), None);
    }

    #[test]
    fn test_type_env() {
        let mut env = TypeEnv::new();
        env.define("x", Type::Int);
        env.define("y", Type::String);

        assert_eq!(env.get("x"), Some(&Type::Int));
        assert_eq!(env.get("y"), Some(&Type::String));
        assert_eq!(env.get("z"), None);

        let child = env.child();
        assert_eq!(child.get("x"), Some(&Type::Int)); // Inherited from parent
    }

    #[test]
    fn test_type_registry() {
        let mut registry = TypeRegistry::new();
        registry.register("Port", Type::Int);
        registry.register("Host", Type::String);

        assert_eq!(registry.get("Port"), Some(&Type::Int));
        assert!(registry.contains("Host"));
        assert!(!registry.contains("Unknown"));
    }
}
