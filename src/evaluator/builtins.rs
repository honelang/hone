//! Built-in functions for Hone
//!
//! These functions are available in all Hone programs without import.
//! The function set is intentionally minimal.

use base64::Engine;
use indexmap::IndexMap;
use sha2::{Digest, Sha256};

use crate::errors::{HoneError, HoneResult};
use crate::lexer::token::SourceLocation;

use super::value::Value;

/// Evaluate a built-in function call
pub fn call_builtin(
    name: &str,
    args: Vec<Value>,
    location: &SourceLocation,
    source: &str,
) -> HoneResult<Value> {
    match name {
        "len" => builtin_len(args, location, source),
        "keys" => builtin_keys(args, location, source),
        "values" => builtin_values(args, location, source),
        "contains" => builtin_contains(args, location, source),
        "concat" => builtin_concat(args, location, source),
        "merge" => builtin_merge(args, location, source),
        "range" => builtin_range(args, location, source),
        "flatten" => builtin_flatten(args, location, source),
        "map" | "filter" | "reduce" => Err(HoneError::undefined_variable(
            source.to_string(),
            location,
            name,
            format!(
                "'{}' is not a built-in function. Use a for comprehension instead: for x in items {{ x * 2 }}",
                name
            ),
        )),
        "to_str" => builtin_to_str(args, location, source),
        "to_int" => builtin_to_int(args, location, source),
        "to_float" => builtin_to_float(args, location, source),
        "to_bool" => builtin_to_bool(args, location, source),
        "default" => builtin_default(args, location, source),
        "upper" => builtin_upper(args, location, source),
        "lower" => builtin_lower(args, location, source),
        "trim" => builtin_trim(args, location, source),
        "split" => builtin_split(args, location, source),
        "join" => builtin_join(args, location, source),
        "replace" => builtin_replace(args, location, source),
        "base64_encode" => builtin_base64_encode(args, location, source),
        "base64_decode" => builtin_base64_decode(args, location, source),
        "to_json" => builtin_to_json(args, location, source),
        "from_json" => builtin_from_json(args, location, source),
        "env" => builtin_env(args, location, source),
        "file" => builtin_file(args, location, source),
        // P0: core missing builtins
        "sort" => builtin_sort(args, location, source),
        "starts_with" => builtin_starts_with(args, location, source),
        "ends_with" => builtin_ends_with(args, location, source),
        "min" => builtin_min(args, location, source),
        "max" => builtin_max(args, location, source),
        "abs" => builtin_abs(args, location, source),
        // P1: important utilities
        "unique" => builtin_unique(args, location, source),
        "sha256" => builtin_sha256(args, location, source),
        "type_of" => builtin_type_of(args, location, source),
        "substring" => builtin_substring(args, location, source),
        // P2: object/array manipulation
        "entries" => builtin_entries(args, location, source),
        "from_entries" => builtin_from_entries(args, location, source),
        "clamp" => builtin_clamp(args, location, source),
        "reverse" => builtin_reverse(args, location, source),
        "slice" => builtin_slice(args, location, source),
        _ => Err(HoneError::undefined_variable(
            source.to_string(),
            location,
            name,
            format!("'{}' is not a built-in function", name),
        )),
    }
}

/// Check if a name is a built-in function.
/// IMPORTANT: This list must be kept in sync with the match arms in `call_builtin` above.
/// If you add a new builtin to `call_builtin`, add it here too, otherwise the evaluator
/// won't recognize it as a function call and will treat it as an undefined variable.
pub fn is_builtin(name: &str) -> bool {
    matches!(
        name,
        "len"
            | "keys"
            | "values"
            | "contains"
            | "concat"
            | "merge"
            | "range"
            | "flatten"
            | "to_str"
            | "to_int"
            | "to_float"
            | "to_bool"
            | "default"
            | "upper"
            | "lower"
            | "trim"
            | "split"
            | "join"
            | "replace"
            | "base64_encode"
            | "base64_decode"
            | "to_json"
            | "from_json"
            | "env"
            | "file"
            | "sort"
            | "starts_with"
            | "ends_with"
            | "min"
            | "max"
            | "abs"
            | "unique"
            | "sha256"
            | "type_of"
            | "substring"
            | "entries"
            | "from_entries"
            | "clamp"
            | "reverse"
            | "slice"
    )
}

/// len(array) -> int, len(string) -> int, len(object) -> int
fn builtin_len(args: Vec<Value>, location: &SourceLocation, source: &str) -> HoneResult<Value> {
    check_arity("len", &args, 1, location, source)?;

    match &args[0] {
        Value::Array(arr) => Ok(Value::Int(arr.len() as i64)),
        Value::String(s) => Ok(Value::Int(s.chars().count() as i64)),
        Value::Object(obj) => Ok(Value::Int(obj.len() as i64)),
        other => Err(type_error(
            "len",
            "array, string, or object",
            other.type_name(),
            location,
            source,
        )),
    }
}

/// keys(object) -> [string]
fn builtin_keys(args: Vec<Value>, location: &SourceLocation, source: &str) -> HoneResult<Value> {
    check_arity("keys", &args, 1, location, source)?;

    match &args[0] {
        Value::Object(obj) => {
            let keys: Vec<Value> = obj.keys().map(|k| Value::String(k.clone())).collect();
            Ok(Value::Array(keys))
        }
        other => Err(type_error(
            "keys",
            "object",
            other.type_name(),
            location,
            source,
        )),
    }
}

/// values(object) -> [any]
fn builtin_values(args: Vec<Value>, location: &SourceLocation, source: &str) -> HoneResult<Value> {
    check_arity("values", &args, 1, location, source)?;

    match &args[0] {
        Value::Object(obj) => {
            let values: Vec<Value> = obj.values().cloned().collect();
            Ok(Value::Array(values))
        }
        other => Err(type_error(
            "values",
            "object",
            other.type_name(),
            location,
            source,
        )),
    }
}

/// contains(array, value) -> bool, contains(string, substring) -> bool, contains(object, key) -> bool
fn builtin_contains(
    args: Vec<Value>,
    location: &SourceLocation,
    source: &str,
) -> HoneResult<Value> {
    check_arity("contains", &args, 2, location, source)?;

    match &args[0] {
        Value::Array(arr) => {
            let found = arr.iter().any(|v| v.equals(&args[1]));
            Ok(Value::Bool(found))
        }
        Value::String(s) => {
            if let Value::String(substr) = &args[1] {
                Ok(Value::Bool(s.contains(substr.as_str())))
            } else {
                Err(type_error(
                    "contains",
                    "string (for second argument)",
                    args[1].type_name(),
                    location,
                    source,
                ))
            }
        }
        Value::Object(obj) => {
            if let Value::String(key) = &args[1] {
                Ok(Value::Bool(obj.contains_key(key)))
            } else {
                Err(type_error(
                    "contains",
                    "string (for key)",
                    args[1].type_name(),
                    location,
                    source,
                ))
            }
        }
        other => Err(type_error(
            "contains",
            "array, string, or object",
            other.type_name(),
            location,
            source,
        )),
    }
}

/// concat(array, array, ...) -> array, concat(string, string, ...) -> string
fn builtin_concat(args: Vec<Value>, location: &SourceLocation, source: &str) -> HoneResult<Value> {
    if args.is_empty() {
        return Err(arity_error("concat", "at least 1", 0, location, source));
    }

    match &args[0] {
        Value::Array(_) => {
            let mut result = Vec::new();
            for arg in args {
                if let Value::Array(arr) = arg {
                    result.extend(arr);
                } else {
                    return Err(type_error(
                        "concat",
                        "array",
                        arg.type_name(),
                        location,
                        source,
                    ));
                }
            }
            Ok(Value::Array(result))
        }
        Value::String(_) => {
            let mut result = String::new();
            for arg in args {
                if let Value::String(s) = arg {
                    result.push_str(&s);
                } else {
                    return Err(type_error(
                        "concat",
                        "string",
                        arg.type_name(),
                        location,
                        source,
                    ));
                }
            }
            Ok(Value::String(result))
        }
        other => Err(type_error(
            "concat",
            "array or string",
            other.type_name(),
            location,
            source,
        )),
    }
}

/// merge(object, object, ...) -> object (shallow merge, right wins)
fn builtin_merge(args: Vec<Value>, location: &SourceLocation, source: &str) -> HoneResult<Value> {
    if args.is_empty() {
        return Err(arity_error("merge", "at least 1", 0, location, source));
    }

    let mut result = IndexMap::new();

    for arg in args {
        if let Value::Object(obj) = arg {
            for (k, v) in obj {
                result.insert(k, v);
            }
        } else {
            return Err(type_error(
                "merge",
                "object",
                arg.type_name(),
                location,
                source,
            ));
        }
    }

    Ok(Value::Object(result))
}

/// range(end) -> [0, 1, ..., end-1], range(start, end) -> [start, ..., end-1], range(start, end, step)
fn builtin_range(args: Vec<Value>, location: &SourceLocation, source: &str) -> HoneResult<Value> {
    if args.is_empty() || args.len() > 3 {
        return Err(arity_error(
            "range",
            "1, 2, or 3",
            args.len(),
            location,
            source,
        ));
    }

    let (start, end, step) = match args.len() {
        1 => {
            let end = expect_int("range", &args[0], location, source)?;
            (0, end, 1)
        }
        2 => {
            let start = expect_int("range", &args[0], location, source)?;
            let end = expect_int("range", &args[1], location, source)?;
            (start, end, 1)
        }
        3 => {
            let start = expect_int("range", &args[0], location, source)?;
            let end = expect_int("range", &args[1], location, source)?;
            let step = expect_int("range", &args[2], location, source)?;
            if step == 0 {
                return Err(HoneError::TypeMismatch {
                    src: source.to_string(),
                    span: (location.offset, location.length).into(),
                    expected: "non-zero step".to_string(),
                    found: "0".to_string(),
                    help: "range step cannot be zero".to_string(),
                });
            }
            (start, end, step)
        }
        _ => unreachable!(),
    };

    let mut result = Vec::new();
    if step > 0 {
        let mut i = start;
        while i < end {
            result.push(Value::Int(i));
            i += step;
        }
    } else {
        let mut i = start;
        while i > end {
            result.push(Value::Int(i));
            i += step;
        }
    }

    Ok(Value::Array(result))
}

/// flatten(array) -> array (flattens one level)
fn builtin_flatten(args: Vec<Value>, location: &SourceLocation, source: &str) -> HoneResult<Value> {
    check_arity("flatten", &args, 1, location, source)?;

    match &args[0] {
        Value::Array(arr) => {
            let mut result = Vec::new();
            for item in arr {
                if let Value::Array(inner) = item {
                    result.extend(inner.clone());
                } else {
                    result.push(item.clone());
                }
            }
            Ok(Value::Array(result))
        }
        other => Err(type_error(
            "flatten",
            "array",
            other.type_name(),
            location,
            source,
        )),
    }
}

/// to_str(value) -> string
fn builtin_to_str(args: Vec<Value>, location: &SourceLocation, source: &str) -> HoneResult<Value> {
    check_arity("to_str", &args, 1, location, source)?;

    let s = match &args[0] {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(n) => n.to_string(),
        Value::Float(n) => n.to_string(),
        Value::String(s) => s.clone(),
        Value::Array(_) | Value::Object(_) => {
            return Err(type_error(
                "to_str",
                "scalar value",
                args[0].type_name(),
                location,
                source,
            ))
        }
    };

    Ok(Value::String(s))
}

/// to_int(value) -> int
fn builtin_to_int(args: Vec<Value>, location: &SourceLocation, source: &str) -> HoneResult<Value> {
    check_arity("to_int", &args, 1, location, source)?;

    let n = match &args[0] {
        Value::Int(n) => *n,
        Value::Float(n) => *n as i64,
        Value::String(s) => s.parse::<i64>().map_err(|_| HoneError::TypeMismatch {
            src: source.to_string(),
            span: (location.offset, location.length).into(),
            expected: "integer string".to_string(),
            found: format!("'{}'", s),
            help: "string must be a valid integer".to_string(),
        })?,
        Value::Bool(b) => {
            if *b {
                1
            } else {
                0
            }
        }
        other => {
            return Err(type_error(
                "to_int",
                "int, float, string, or bool",
                other.type_name(),
                location,
                source,
            ))
        }
    };

    Ok(Value::Int(n))
}

/// to_float(value) -> float
fn builtin_to_float(
    args: Vec<Value>,
    location: &SourceLocation,
    source: &str,
) -> HoneResult<Value> {
    check_arity("to_float", &args, 1, location, source)?;

    let n = match &args[0] {
        Value::Int(n) => *n as f64,
        Value::Float(n) => *n,
        Value::String(s) => s.parse::<f64>().map_err(|_| HoneError::TypeMismatch {
            src: source.to_string(),
            span: (location.offset, location.length).into(),
            expected: "numeric string".to_string(),
            found: format!("'{}'", s),
            help: "string must be a valid number".to_string(),
        })?,
        other => {
            return Err(type_error(
                "to_float",
                "int, float, or string",
                other.type_name(),
                location,
                source,
            ))
        }
    };

    Ok(Value::Float(n))
}

/// to_bool(value) -> bool (uses truthiness)
fn builtin_to_bool(args: Vec<Value>, location: &SourceLocation, source: &str) -> HoneResult<Value> {
    check_arity("to_bool", &args, 1, location, source)?;
    Ok(Value::Bool(args[0].is_truthy()))
}

/// default(value, fallback) -> value if not null, else fallback
fn builtin_default(args: Vec<Value>, location: &SourceLocation, source: &str) -> HoneResult<Value> {
    check_arity("default", &args, 2, location, source)?;

    if args[0].is_null() {
        Ok(args[1].clone())
    } else {
        Ok(args[0].clone())
    }
}

/// upper(string) -> string
fn builtin_upper(args: Vec<Value>, location: &SourceLocation, source: &str) -> HoneResult<Value> {
    check_arity("upper", &args, 1, location, source)?;
    let s = expect_string("upper", &args[0], location, source)?;
    Ok(Value::String(s.to_uppercase()))
}

/// lower(string) -> string
fn builtin_lower(args: Vec<Value>, location: &SourceLocation, source: &str) -> HoneResult<Value> {
    check_arity("lower", &args, 1, location, source)?;
    let s = expect_string("lower", &args[0], location, source)?;
    Ok(Value::String(s.to_lowercase()))
}

/// trim(string) -> string
fn builtin_trim(args: Vec<Value>, location: &SourceLocation, source: &str) -> HoneResult<Value> {
    check_arity("trim", &args, 1, location, source)?;
    let s = expect_string("trim", &args[0], location, source)?;
    Ok(Value::String(s.trim().to_string()))
}

/// split(string, delimiter) -> [string]
fn builtin_split(args: Vec<Value>, location: &SourceLocation, source: &str) -> HoneResult<Value> {
    check_arity("split", &args, 2, location, source)?;
    let s = expect_string("split", &args[0], location, source)?;
    let delimiter = expect_string("split", &args[1], location, source)?;
    let parts: Vec<Value> = s
        .split(delimiter)
        .map(|p| Value::String(p.to_string()))
        .collect();
    Ok(Value::Array(parts))
}

/// join(array, delimiter) -> string
fn builtin_join(args: Vec<Value>, location: &SourceLocation, source: &str) -> HoneResult<Value> {
    check_arity("join", &args, 2, location, source)?;
    let arr = match &args[0] {
        Value::Array(a) => a,
        other => {
            return Err(type_error(
                "join",
                "array",
                other.type_name(),
                location,
                source,
            ))
        }
    };
    let delimiter = match &args[1] {
        Value::String(d) => d,
        other => {
            return Err(type_error(
                "join",
                "string (delimiter)",
                other.type_name(),
                location,
                source,
            ))
        }
    };
    let mut strings = Vec::with_capacity(arr.len());
    for item in arr {
        match item {
            Value::String(s) => strings.push(s.clone()),
            other => {
                return Err(type_error(
                    "join",
                    "array of strings",
                    other.type_name(),
                    location,
                    source,
                ))
            }
        }
    }
    Ok(Value::String(strings.join(delimiter.as_str())))
}

/// replace(string, from, to) -> string
fn builtin_replace(args: Vec<Value>, location: &SourceLocation, source: &str) -> HoneResult<Value> {
    check_arity("replace", &args, 3, location, source)?;
    let s = expect_string("replace", &args[0], location, source)?;
    let from = expect_string("replace", &args[1], location, source)?;
    let to = expect_string("replace", &args[2], location, source)?;
    Ok(Value::String(s.replace(from, to)))
}

/// base64_encode(string) -> string
fn builtin_base64_encode(
    args: Vec<Value>,
    location: &SourceLocation,
    source: &str,
) -> HoneResult<Value> {
    check_arity("base64_encode", &args, 1, location, source)?;
    let s = expect_string("base64_encode", &args[0], location, source)?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(s.as_bytes());
    Ok(Value::String(encoded))
}

/// base64_decode(string) -> string
fn builtin_base64_decode(
    args: Vec<Value>,
    location: &SourceLocation,
    source: &str,
) -> HoneResult<Value> {
    check_arity("base64_decode", &args, 1, location, source)?;
    let s = expect_string("base64_decode", &args[0], location, source)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(s.as_bytes())
        .map_err(|e| HoneError::TypeMismatch {
            src: source.to_string(),
            span: (location.offset, location.length).into(),
            expected: "valid base64 string".to_string(),
            found: format!("invalid base64: {}", e),
            help: "the input string is not valid base64".to_string(),
        })?;
    let decoded = String::from_utf8(bytes).map_err(|_| HoneError::TypeMismatch {
        src: source.to_string(),
        span: (location.offset, location.length).into(),
        expected: "valid UTF-8 after decoding".to_string(),
        found: "invalid UTF-8".to_string(),
        help: "the decoded base64 data is not valid UTF-8".to_string(),
    })?;
    Ok(Value::String(decoded))
}

/// to_json(value) -> string
fn builtin_to_json(args: Vec<Value>, location: &SourceLocation, source: &str) -> HoneResult<Value> {
    check_arity("to_json", &args, 1, location, source)?;
    let json_value = args[0].to_serde_json();
    let json_string = serde_json::to_string(&json_value).map_err(|e| HoneError::TypeMismatch {
        src: source.to_string(),
        span: (location.offset, location.length).into(),
        expected: "serializable value".to_string(),
        found: format!("serialization error: {}", e),
        help: "value could not be serialized to JSON".to_string(),
    })?;
    Ok(Value::String(json_string))
}

/// from_json(string) -> value
fn builtin_from_json(
    args: Vec<Value>,
    location: &SourceLocation,
    source: &str,
) -> HoneResult<Value> {
    check_arity("from_json", &args, 1, location, source)?;
    match &args[0] {
        Value::String(s) => {
            let json_value: serde_json::Value =
                serde_json::from_str(s).map_err(|e| HoneError::TypeMismatch {
                    src: source.to_string(),
                    span: (location.offset, location.length).into(),
                    expected: "valid JSON string".to_string(),
                    found: format!("parse error: {}", e),
                    help: "the input string is not valid JSON".to_string(),
                })?;
            Ok(Value::from_serde_json(json_value))
        }
        other => Err(type_error(
            "from_json",
            "string",
            other.type_name(),
            location,
            source,
        )),
    }
}

/// env(name, default?) -> string
fn builtin_env(args: Vec<Value>, location: &SourceLocation, source: &str) -> HoneResult<Value> {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("env", "1 or 2", args.len(), location, source));
    }
    let name = match &args[0] {
        Value::String(s) => s,
        other => {
            return Err(type_error(
                "env",
                "string",
                other.type_name(),
                location,
                source,
            ))
        }
    };
    match std::env::var(name) {
        Ok(val) => Ok(Value::String(val)),
        Err(_) => {
            if args.len() == 2 {
                Ok(args[1].clone())
            } else {
                Err(HoneError::TypeMismatch {
                    src: source.to_string(),
                    span: (location.offset, location.length).into(),
                    expected: format!("environment variable '{}' to be set", name),
                    found: "undefined".to_string(),
                    help: format!("set the environment variable or provide a default: env(\"{}\", \"default\")", name),
                })
            }
        }
    }
}

/// file(path) -> string
fn builtin_file(args: Vec<Value>, location: &SourceLocation, source: &str) -> HoneResult<Value> {
    check_arity("file", &args, 1, location, source)?;
    let path = match &args[0] {
        Value::String(s) => s,
        other => {
            return Err(type_error(
                "file",
                "string",
                other.type_name(),
                location,
                source,
            ))
        }
    };
    let contents = std::fs::read_to_string(path).map_err(|e| HoneError::TypeMismatch {
        src: source.to_string(),
        span: (location.offset, location.length).into(),
        expected: format!("readable file at '{}'", path),
        found: format!("I/O error: {}", e),
        help: "check that the file path exists and is readable".to_string(),
    })?;
    Ok(Value::String(contents))
}

// ── P0 builtins ────────────────────────────────────────────────────────

/// sort(array) -> array (ascending, stable)
fn builtin_sort(args: Vec<Value>, location: &SourceLocation, source: &str) -> HoneResult<Value> {
    check_arity("sort", &args, 1, location, source)?;
    match &args[0] {
        Value::Array(arr) => {
            let mut sorted = arr.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            Ok(Value::Array(sorted))
        }
        other => Err(type_error(
            "sort",
            "array",
            other.type_name(),
            location,
            source,
        )),
    }
}

/// starts_with(string, prefix) -> bool
fn builtin_starts_with(
    args: Vec<Value>,
    location: &SourceLocation,
    source: &str,
) -> HoneResult<Value> {
    check_arity("starts_with", &args, 2, location, source)?;
    let s = expect_string("starts_with", &args[0], location, source)?;
    let prefix = expect_string("starts_with", &args[1], location, source)?;
    Ok(Value::Bool(s.starts_with(prefix)))
}

/// ends_with(string, suffix) -> bool
fn builtin_ends_with(
    args: Vec<Value>,
    location: &SourceLocation,
    source: &str,
) -> HoneResult<Value> {
    check_arity("ends_with", &args, 2, location, source)?;
    let s = expect_string("ends_with", &args[0], location, source)?;
    let suffix = expect_string("ends_with", &args[1], location, source)?;
    Ok(Value::Bool(s.ends_with(suffix)))
}

/// min(a, b) -> number
fn builtin_min(args: Vec<Value>, location: &SourceLocation, source: &str) -> HoneResult<Value> {
    check_arity("min", &args, 2, location, source)?;
    match (&args[0], &args[1]) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(*a.min(b))),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a.min(*b))),
        (Value::Int(a), Value::Float(b)) => Ok(Value::Float((*a as f64).min(*b))),
        (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a.min(*b as f64))),
        _ => Err(type_error(
            "min",
            "two numbers",
            &format!("{}, {}", args[0].type_name(), args[1].type_name()),
            location,
            source,
        )),
    }
}

/// max(a, b) -> number
fn builtin_max(args: Vec<Value>, location: &SourceLocation, source: &str) -> HoneResult<Value> {
    check_arity("max", &args, 2, location, source)?;
    match (&args[0], &args[1]) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(*a.max(b))),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a.max(*b))),
        (Value::Int(a), Value::Float(b)) => Ok(Value::Float((*a as f64).max(*b))),
        (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a.max(*b as f64))),
        _ => Err(type_error(
            "max",
            "two numbers",
            &format!("{}, {}", args[0].type_name(), args[1].type_name()),
            location,
            source,
        )),
    }
}

/// abs(number) -> number
fn builtin_abs(args: Vec<Value>, location: &SourceLocation, source: &str) -> HoneResult<Value> {
    check_arity("abs", &args, 1, location, source)?;
    match &args[0] {
        Value::Int(n) => Ok(Value::Int(n.abs())),
        Value::Float(n) => Ok(Value::Float(n.abs())),
        other => Err(type_error(
            "abs",
            "number",
            other.type_name(),
            location,
            source,
        )),
    }
}

// ── P1 builtins ────────────────────────────────────────────────────────

/// unique(array) -> array (preserves first occurrence order)
fn builtin_unique(args: Vec<Value>, location: &SourceLocation, source: &str) -> HoneResult<Value> {
    check_arity("unique", &args, 1, location, source)?;
    match &args[0] {
        Value::Array(arr) => {
            let mut seen = Vec::new();
            let mut result = Vec::new();
            for item in arr {
                let json_key = format!("{}", item);
                if !seen.contains(&json_key) {
                    seen.push(json_key);
                    result.push(item.clone());
                }
            }
            Ok(Value::Array(result))
        }
        other => Err(type_error(
            "unique",
            "array",
            other.type_name(),
            location,
            source,
        )),
    }
}

/// sha256(string) -> string (hex digest)
fn builtin_sha256(args: Vec<Value>, location: &SourceLocation, source: &str) -> HoneResult<Value> {
    check_arity("sha256", &args, 1, location, source)?;
    let s = expect_string("sha256", &args[0], location, source)?;
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    let result = hasher.finalize();
    Ok(Value::String(format!("{:x}", result)))
}

/// type_of(value) -> string
fn builtin_type_of(
    args: Vec<Value>,
    location: &SourceLocation,
    source: &str,
) -> HoneResult<Value> {
    check_arity("type_of", &args, 1, location, source)?;
    Ok(Value::String(args[0].type_name().to_string()))
}

/// substring(string, start, end?) -> string
fn builtin_substring(
    args: Vec<Value>,
    location: &SourceLocation,
    source: &str,
) -> HoneResult<Value> {
    if args.len() < 2 || args.len() > 3 {
        return Err(arity_error(
            "substring",
            "2 or 3",
            args.len(),
            location,
            source,
        ));
    }
    let s = expect_string("substring", &args[0], location, source)?;
    let start = expect_int("substring", &args[1], location, source)? as usize;
    let chars: Vec<char> = s.chars().collect();
    let end = if args.len() == 3 {
        expect_int("substring", &args[2], location, source)? as usize
    } else {
        chars.len()
    };
    let start = start.min(chars.len());
    let end = end.min(chars.len());
    if start > end {
        return Ok(Value::String(String::new()));
    }
    Ok(Value::String(chars[start..end].iter().collect()))
}

// ── P2 builtins ────────────────────────────────────────────────────────

/// entries(object) -> [[key, value], ...]
fn builtin_entries(
    args: Vec<Value>,
    location: &SourceLocation,
    source: &str,
) -> HoneResult<Value> {
    check_arity("entries", &args, 1, location, source)?;
    match &args[0] {
        Value::Object(obj) => {
            let result: Vec<Value> = obj
                .iter()
                .map(|(k, v)| Value::Array(vec![Value::String(k.clone()), v.clone()]))
                .collect();
            Ok(Value::Array(result))
        }
        other => Err(type_error(
            "entries",
            "object",
            other.type_name(),
            location,
            source,
        )),
    }
}

/// from_entries([[key, value], ...]) -> object
fn builtin_from_entries(
    args: Vec<Value>,
    location: &SourceLocation,
    source: &str,
) -> HoneResult<Value> {
    check_arity("from_entries", &args, 1, location, source)?;
    match &args[0] {
        Value::Array(arr) => {
            let mut obj = IndexMap::new();
            for item in arr {
                match item {
                    Value::Array(pair) if pair.len() == 2 => {
                        if let Value::String(key) = &pair[0] {
                            obj.insert(key.clone(), pair[1].clone());
                        } else {
                            return Err(type_error(
                                "from_entries",
                                "string key in [key, value] pair",
                                pair[0].type_name(),
                                location,
                                source,
                            ));
                        }
                    }
                    _ => {
                        return Err(type_error(
                            "from_entries",
                            "[key, value] pair (2-element array)",
                            item.type_name(),
                            location,
                            source,
                        ))
                    }
                }
            }
            Ok(Value::Object(obj))
        }
        other => Err(type_error(
            "from_entries",
            "array of [key, value] pairs",
            other.type_name(),
            location,
            source,
        )),
    }
}

/// clamp(value, min, max) -> number
fn builtin_clamp(args: Vec<Value>, location: &SourceLocation, source: &str) -> HoneResult<Value> {
    check_arity("clamp", &args, 3, location, source)?;
    match (&args[0], &args[1], &args[2]) {
        (Value::Int(v), Value::Int(lo), Value::Int(hi)) => Ok(Value::Int(*v.max(lo).min(hi))),
        _ => {
            let v = expect_float_coerce("clamp", &args[0], location, source)?;
            let lo = expect_float_coerce("clamp", &args[1], location, source)?;
            let hi = expect_float_coerce("clamp", &args[2], location, source)?;
            Ok(Value::Float(v.max(lo).min(hi)))
        }
    }
}

/// reverse(array) -> array
fn builtin_reverse(
    args: Vec<Value>,
    location: &SourceLocation,
    source: &str,
) -> HoneResult<Value> {
    check_arity("reverse", &args, 1, location, source)?;
    match &args[0] {
        Value::Array(arr) => {
            let mut result = arr.clone();
            result.reverse();
            Ok(Value::Array(result))
        }
        Value::String(s) => Ok(Value::String(s.chars().rev().collect())),
        other => Err(type_error(
            "reverse",
            "array or string",
            other.type_name(),
            location,
            source,
        )),
    }
}

/// slice(array, start, end?) -> array, slice(string, start, end?) -> string
fn builtin_slice(args: Vec<Value>, location: &SourceLocation, source: &str) -> HoneResult<Value> {
    if args.len() < 2 || args.len() > 3 {
        return Err(arity_error(
            "slice",
            "2 or 3",
            args.len(),
            location,
            source,
        ));
    }
    let start = expect_int("slice", &args[1], location, source)?;
    match &args[0] {
        Value::Array(arr) => {
            let len = arr.len() as i64;
            let start = normalize_index(start, len) as usize;
            let end = if args.len() == 3 {
                normalize_index(
                    expect_int("slice", &args[2], location, source)?,
                    len,
                ) as usize
            } else {
                len as usize
            };
            let start = start.min(arr.len());
            let end = end.min(arr.len());
            if start >= end {
                return Ok(Value::Array(vec![]));
            }
            Ok(Value::Array(arr[start..end].to_vec()))
        }
        Value::String(s) => {
            let chars: Vec<char> = s.chars().collect();
            let len = chars.len() as i64;
            let start = normalize_index(start, len) as usize;
            let end = if args.len() == 3 {
                normalize_index(
                    expect_int("slice", &args[2], location, source)?,
                    len,
                ) as usize
            } else {
                len as usize
            };
            let start = start.min(chars.len());
            let end = end.min(chars.len());
            if start >= end {
                return Ok(Value::String(String::new()));
            }
            Ok(Value::String(chars[start..end].iter().collect()))
        }
        other => Err(type_error(
            "slice",
            "array or string",
            other.type_name(),
            location,
            source,
        )),
    }
}

/// Normalize a possibly-negative index: negative counts from end
fn normalize_index(idx: i64, len: i64) -> i64 {
    if idx < 0 {
        (len + idx).max(0)
    } else {
        idx.min(len)
    }
}

// Helper functions

fn check_arity(
    name: &str,
    args: &[Value],
    expected: usize,
    location: &SourceLocation,
    source: &str,
) -> HoneResult<()> {
    if args.len() != expected {
        Err(arity_error(
            name,
            &expected.to_string(),
            args.len(),
            location,
            source,
        ))
    } else {
        Ok(())
    }
}

fn arity_error(
    name: &str,
    expected: &str,
    got: usize,
    location: &SourceLocation,
    source: &str,
) -> HoneError {
    HoneError::TypeMismatch {
        src: source.to_string(),
        span: (location.offset, location.length).into(),
        expected: format!("{} argument(s)", expected),
        found: format!("{} argument(s)", got),
        help: format!("{}() expects {} argument(s)", name, expected),
    }
}

fn type_error(
    name: &str,
    expected: &str,
    got: &str,
    location: &SourceLocation,
    source: &str,
) -> HoneError {
    HoneError::TypeMismatch {
        src: source.to_string(),
        span: (location.offset, location.length).into(),
        expected: expected.to_string(),
        found: got.to_string(),
        help: format!("{}() requires {}", name, expected),
    }
}

fn expect_int(
    name: &str,
    value: &Value,
    location: &SourceLocation,
    source: &str,
) -> HoneResult<i64> {
    match value {
        Value::Int(n) => Ok(*n),
        other => Err(type_error(name, "int", other.type_name(), location, source)),
    }
}

fn expect_float_coerce(
    name: &str,
    value: &Value,
    location: &SourceLocation,
    source: &str,
) -> HoneResult<f64> {
    match value {
        Value::Int(n) => Ok(*n as f64),
        Value::Float(n) => Ok(*n),
        other => Err(type_error(
            name,
            "number",
            other.type_name(),
            location,
            source,
        )),
    }
}

fn expect_string<'a>(
    name: &str,
    value: &'a Value,
    location: &SourceLocation,
    source: &str,
) -> HoneResult<&'a str> {
    match value {
        Value::String(s) => Ok(s),
        other => Err(type_error(
            name,
            "string",
            other.type_name(),
            location,
            source,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loc() -> SourceLocation {
        SourceLocation::new(None, 1, 1, 0, 1)
    }

    #[test]
    fn test_len() {
        assert_eq!(
            call_builtin(
                "len",
                vec![Value::Array(vec![Value::Int(1), Value::Int(2)])],
                &loc(),
                ""
            )
            .unwrap(),
            Value::Int(2)
        );
        assert_eq!(
            call_builtin("len", vec![Value::String("hello".into())], &loc(), "").unwrap(),
            Value::Int(5)
        );
        assert_eq!(
            call_builtin("len", vec![Value::Object(IndexMap::new())], &loc(), "").unwrap(),
            Value::Int(0)
        );
    }

    #[test]
    fn test_keys() {
        let mut obj = IndexMap::new();
        obj.insert("a".to_string(), Value::Int(1));
        obj.insert("b".to_string(), Value::Int(2));

        let result = call_builtin("keys", vec![Value::Object(obj)], &loc(), "").unwrap();
        if let Value::Array(keys) = result {
            assert_eq!(keys.len(), 2);
            assert!(keys.contains(&Value::String("a".into())));
            assert!(keys.contains(&Value::String("b".into())));
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn test_contains() {
        assert_eq!(
            call_builtin(
                "contains",
                vec![
                    Value::Array(vec![Value::Int(1), Value::Int(2)]),
                    Value::Int(1)
                ],
                &loc(),
                ""
            )
            .unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            call_builtin(
                "contains",
                vec![Value::String("hello".into()), Value::String("ell".into())],
                &loc(),
                ""
            )
            .unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn test_concat() {
        assert_eq!(
            call_builtin(
                "concat",
                vec![
                    Value::Array(vec![Value::Int(1)]),
                    Value::Array(vec![Value::Int(2)])
                ],
                &loc(),
                ""
            )
            .unwrap(),
            Value::Array(vec![Value::Int(1), Value::Int(2)])
        );
        assert_eq!(
            call_builtin(
                "concat",
                vec![
                    Value::String("hello".into()),
                    Value::String(" world".into())
                ],
                &loc(),
                ""
            )
            .unwrap(),
            Value::String("hello world".into())
        );
    }

    #[test]
    fn test_range() {
        assert_eq!(
            call_builtin("range", vec![Value::Int(3)], &loc(), "").unwrap(),
            Value::Array(vec![Value::Int(0), Value::Int(1), Value::Int(2)])
        );
        assert_eq!(
            call_builtin("range", vec![Value::Int(1), Value::Int(4)], &loc(), "").unwrap(),
            Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
        );
        assert_eq!(
            call_builtin(
                "range",
                vec![Value::Int(0), Value::Int(6), Value::Int(2)],
                &loc(),
                ""
            )
            .unwrap(),
            Value::Array(vec![Value::Int(0), Value::Int(2), Value::Int(4)])
        );
    }

    #[test]
    fn test_flatten() {
        assert_eq!(
            call_builtin(
                "flatten",
                vec![Value::Array(vec![
                    Value::Array(vec![Value::Int(1), Value::Int(2)]),
                    Value::Array(vec![Value::Int(3)])
                ])],
                &loc(),
                ""
            )
            .unwrap(),
            Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
        );
    }

    #[test]
    fn test_to_str() {
        assert_eq!(
            call_builtin("to_str", vec![Value::Int(42)], &loc(), "").unwrap(),
            Value::String("42".into())
        );
        assert_eq!(
            call_builtin("to_str", vec![Value::Bool(true)], &loc(), "").unwrap(),
            Value::String("true".into())
        );
    }

    #[test]
    fn test_to_int() {
        assert_eq!(
            call_builtin("to_int", vec![Value::String("42".into())], &loc(), "").unwrap(),
            Value::Int(42)
        );
        assert_eq!(
            call_builtin("to_int", vec![Value::Float(3.7)], &loc(), "").unwrap(),
            Value::Int(3)
        );
    }

    #[test]
    fn test_default() {
        assert_eq!(
            call_builtin("default", vec![Value::Null, Value::Int(42)], &loc(), "").unwrap(),
            Value::Int(42)
        );
        assert_eq!(
            call_builtin("default", vec![Value::Int(1), Value::Int(42)], &loc(), "").unwrap(),
            Value::Int(1)
        );
    }

    #[test]
    fn test_unknown_builtin() {
        let result = call_builtin("unknown_func", vec![], &loc(), "");
        assert!(result.is_err());
    }

    #[test]
    fn test_upper() {
        assert_eq!(
            call_builtin("upper", vec![Value::String("hello".into())], &loc(), "").unwrap(),
            Value::String("HELLO".into())
        );
        assert_eq!(
            call_builtin(
                "upper",
                vec![Value::String("Hello World".into())],
                &loc(),
                ""
            )
            .unwrap(),
            Value::String("HELLO WORLD".into())
        );
        assert!(call_builtin("upper", vec![Value::Int(42)], &loc(), "").is_err());
    }

    #[test]
    fn test_lower() {
        assert_eq!(
            call_builtin("lower", vec![Value::String("HELLO".into())], &loc(), "").unwrap(),
            Value::String("hello".into())
        );
        assert_eq!(
            call_builtin(
                "lower",
                vec![Value::String("Hello World".into())],
                &loc(),
                ""
            )
            .unwrap(),
            Value::String("hello world".into())
        );
        assert!(call_builtin("lower", vec![Value::Int(42)], &loc(), "").is_err());
    }

    #[test]
    fn test_trim() {
        assert_eq!(
            call_builtin("trim", vec![Value::String("  hello  ".into())], &loc(), "").unwrap(),
            Value::String("hello".into())
        );
        assert_eq!(
            call_builtin(
                "trim",
                vec![Value::String("\n\thello\t\n".into())],
                &loc(),
                ""
            )
            .unwrap(),
            Value::String("hello".into())
        );
        assert!(call_builtin("trim", vec![Value::Int(42)], &loc(), "").is_err());
    }

    #[test]
    fn test_split() {
        assert_eq!(
            call_builtin(
                "split",
                vec![Value::String("a,b,c".into()), Value::String(",".into())],
                &loc(),
                ""
            )
            .unwrap(),
            Value::Array(vec![
                Value::String("a".into()),
                Value::String("b".into()),
                Value::String("c".into()),
            ])
        );
        assert_eq!(
            call_builtin(
                "split",
                vec![Value::String("hello".into()), Value::String(",".into())],
                &loc(),
                ""
            )
            .unwrap(),
            Value::Array(vec![Value::String("hello".into())])
        );
        assert!(call_builtin(
            "split",
            vec![Value::Int(42), Value::String(",".into())],
            &loc(),
            ""
        )
        .is_err());
    }

    #[test]
    fn test_join() {
        assert_eq!(
            call_builtin(
                "join",
                vec![
                    Value::Array(vec![
                        Value::String("a".into()),
                        Value::String("b".into()),
                        Value::String("c".into()),
                    ]),
                    Value::String("-".into()),
                ],
                &loc(),
                ""
            )
            .unwrap(),
            Value::String("a-b-c".into())
        );
        assert_eq!(
            call_builtin(
                "join",
                vec![Value::Array(vec![]), Value::String(",".into())],
                &loc(),
                ""
            )
            .unwrap(),
            Value::String("".into())
        );
        // Error: non-string elements
        assert!(call_builtin(
            "join",
            vec![Value::Array(vec![Value::Int(1)]), Value::String(",".into())],
            &loc(),
            ""
        )
        .is_err());
    }

    #[test]
    fn test_replace() {
        assert_eq!(
            call_builtin(
                "replace",
                vec![
                    Value::String("hello world".into()),
                    Value::String("world".into()),
                    Value::String("rust".into()),
                ],
                &loc(),
                ""
            )
            .unwrap(),
            Value::String("hello rust".into())
        );
        assert_eq!(
            call_builtin(
                "replace",
                vec![
                    Value::String("aaa".into()),
                    Value::String("a".into()),
                    Value::String("b".into()),
                ],
                &loc(),
                ""
            )
            .unwrap(),
            Value::String("bbb".into())
        );
    }

    #[test]
    fn test_base64_encode() {
        assert_eq!(
            call_builtin(
                "base64_encode",
                vec![Value::String("hello".into())],
                &loc(),
                ""
            )
            .unwrap(),
            Value::String("aGVsbG8=".into())
        );
        assert_eq!(
            call_builtin("base64_encode", vec![Value::String("".into())], &loc(), "").unwrap(),
            Value::String("".into())
        );
        assert!(call_builtin("base64_encode", vec![Value::Int(42)], &loc(), "").is_err());
    }

    #[test]
    fn test_base64_decode() {
        assert_eq!(
            call_builtin(
                "base64_decode",
                vec![Value::String("aGVsbG8=".into())],
                &loc(),
                ""
            )
            .unwrap(),
            Value::String("hello".into())
        );
        assert_eq!(
            call_builtin("base64_decode", vec![Value::String("".into())], &loc(), "").unwrap(),
            Value::String("".into())
        );
        // Invalid base64
        assert!(call_builtin(
            "base64_decode",
            vec![Value::String("!!!".into())],
            &loc(),
            ""
        )
        .is_err());
    }

    #[test]
    fn test_base64_roundtrip() {
        let original = Value::String("Hello, World! 123 !@#$%".into());
        let encoded = call_builtin("base64_encode", vec![original.clone()], &loc(), "").unwrap();
        let decoded = call_builtin("base64_decode", vec![encoded], &loc(), "").unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_to_json() {
        // Simple value
        assert_eq!(
            call_builtin("to_json", vec![Value::Int(42)], &loc(), "").unwrap(),
            Value::String("42".into())
        );
        // Object
        let mut obj = IndexMap::new();
        obj.insert("a".to_string(), Value::Int(1));
        let result = call_builtin("to_json", vec![Value::Object(obj)], &loc(), "").unwrap();
        assert_eq!(result, Value::String("{\"a\":1}".into()));
        // Null
        assert_eq!(
            call_builtin("to_json", vec![Value::Null], &loc(), "").unwrap(),
            Value::String("null".into())
        );
    }

    #[test]
    fn test_from_json() {
        assert_eq!(
            call_builtin("from_json", vec![Value::String("42".into())], &loc(), "").unwrap(),
            Value::Int(42)
        );
        assert_eq!(
            call_builtin(
                "from_json",
                vec![Value::String("{\"a\":1}".into())],
                &loc(),
                ""
            )
            .unwrap(),
            {
                let mut obj = IndexMap::new();
                obj.insert("a".to_string(), Value::Int(1));
                Value::Object(obj)
            }
        );
        assert_eq!(
            call_builtin(
                "from_json",
                vec![Value::String("[1,2,3]".into())],
                &loc(),
                ""
            )
            .unwrap(),
            Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
        );
        // Invalid JSON
        assert!(call_builtin(
            "from_json",
            vec![Value::String("{invalid".into())],
            &loc(),
            ""
        )
        .is_err());
        // Wrong type
        assert!(call_builtin("from_json", vec![Value::Int(42)], &loc(), "").is_err());
    }

    #[test]
    fn test_json_roundtrip() {
        let mut obj = IndexMap::new();
        obj.insert("name".to_string(), Value::String("test".into()));
        obj.insert("count".to_string(), Value::Int(42));
        obj.insert("enabled".to_string(), Value::Bool(true));
        let original = Value::Object(obj);

        let json = call_builtin("to_json", vec![original.clone()], &loc(), "").unwrap();
        let restored = call_builtin("from_json", vec![json], &loc(), "").unwrap();
        assert_eq!(restored, original);
    }

    #[test]
    fn test_map_removed_with_helpful_error() {
        let result = call_builtin(
            "map",
            vec![Value::Array(vec![]), Value::Array(vec![])],
            &loc(),
            "test",
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        match &err {
            HoneError::UndefinedVariable { help, .. } => {
                assert!(
                    help.contains("for comprehension"),
                    "error should suggest for comprehension, got: {}",
                    help
                );
            }
            other => panic!("expected UndefinedVariable, got: {:?}", other),
        }
    }

    #[test]
    fn test_filter_removed_with_helpful_error() {
        let result = call_builtin(
            "filter",
            vec![Value::Array(vec![]), Value::Array(vec![])],
            &loc(),
            "test",
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        match &err {
            HoneError::UndefinedVariable { help, .. } => {
                assert!(
                    help.contains("for comprehension"),
                    "error should suggest for comprehension, got: {}",
                    help
                );
            }
            other => panic!("expected UndefinedVariable, got: {:?}", other),
        }
    }

    #[test]
    fn test_reduce_removed_with_helpful_error() {
        let result = call_builtin(
            "reduce",
            vec![Value::Array(vec![]), Value::Int(0), Value::Array(vec![])],
            &loc(),
            "test",
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        match &err {
            HoneError::UndefinedVariable { help, .. } => {
                assert!(
                    help.contains("for comprehension"),
                    "error should suggest for comprehension, got: {}",
                    help
                );
            }
            other => panic!("expected UndefinedVariable, got: {:?}", other),
        }
    }

    #[test]
    fn test_env() {
        // Set a test env var
        std::env::set_var("HONE_TEST_VAR", "test_value");
        assert_eq!(
            call_builtin(
                "env",
                vec![Value::String("HONE_TEST_VAR".into())],
                &loc(),
                ""
            )
            .unwrap(),
            Value::String("test_value".into())
        );
        // With default for missing var
        assert_eq!(
            call_builtin(
                "env",
                vec![
                    Value::String("HONE_NONEXISTENT_VAR_12345".into()),
                    Value::String("fallback".into())
                ],
                &loc(),
                ""
            )
            .unwrap(),
            Value::String("fallback".into())
        );
        // Missing without default
        assert!(call_builtin(
            "env",
            vec![Value::String("HONE_NONEXISTENT_VAR_12345".into())],
            &loc(),
            ""
        )
        .is_err());
        std::env::remove_var("HONE_TEST_VAR");
    }

    #[test]
    fn test_is_builtin() {
        assert!(is_builtin("len"));
        assert!(is_builtin("upper"));
        assert!(is_builtin("lower"));
        assert!(is_builtin("trim"));
        assert!(is_builtin("split"));
        assert!(is_builtin("join"));
        assert!(is_builtin("replace"));
        assert!(is_builtin("base64_encode"));
        assert!(is_builtin("base64_decode"));
        assert!(is_builtin("to_json"));
        assert!(is_builtin("from_json"));
        assert!(is_builtin("env"));
        assert!(is_builtin("file"));
        // New builtins
        assert!(is_builtin("sort"));
        assert!(is_builtin("starts_with"));
        assert!(is_builtin("ends_with"));
        assert!(is_builtin("min"));
        assert!(is_builtin("max"));
        assert!(is_builtin("abs"));
        assert!(is_builtin("unique"));
        assert!(is_builtin("sha256"));
        assert!(is_builtin("type_of"));
        assert!(is_builtin("substring"));
        assert!(is_builtin("entries"));
        assert!(is_builtin("from_entries"));
        assert!(is_builtin("clamp"));
        assert!(is_builtin("reverse"));
        assert!(is_builtin("slice"));
        // Removed builtins
        assert!(!is_builtin("map"));
        assert!(!is_builtin("filter"));
        assert!(!is_builtin("reduce"));
        assert!(!is_builtin("nonexistent"));
    }

    // ── P0 builtin tests ──────────────────────────────────────────────

    #[test]
    fn test_sort_integers() {
        let arr = Value::Array(vec![Value::Int(3), Value::Int(1), Value::Int(2)]);
        let result = call_builtin("sort", vec![arr], &loc(), "").unwrap();
        assert_eq!(
            result,
            Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
        );
    }

    #[test]
    fn test_sort_strings() {
        let arr = Value::Array(vec![
            Value::String("banana".into()),
            Value::String("apple".into()),
            Value::String("cherry".into()),
        ]);
        let result = call_builtin("sort", vec![arr], &loc(), "").unwrap();
        assert_eq!(
            result,
            Value::Array(vec![
                Value::String("apple".into()),
                Value::String("banana".into()),
                Value::String("cherry".into()),
            ])
        );
    }

    #[test]
    fn test_sort_empty() {
        let result = call_builtin("sort", vec![Value::Array(vec![])], &loc(), "").unwrap();
        assert_eq!(result, Value::Array(vec![]));
    }

    #[test]
    fn test_sort_rejects_non_array() {
        assert!(call_builtin("sort", vec![Value::Int(1)], &loc(), "").is_err());
    }

    #[test]
    fn test_starts_with() {
        assert_eq!(
            call_builtin(
                "starts_with",
                vec![
                    Value::String("hello world".into()),
                    Value::String("hello".into())
                ],
                &loc(),
                ""
            )
            .unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            call_builtin(
                "starts_with",
                vec![
                    Value::String("hello".into()),
                    Value::String("world".into())
                ],
                &loc(),
                ""
            )
            .unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            call_builtin(
                "starts_with",
                vec![Value::String("".into()), Value::String("".into())],
                &loc(),
                ""
            )
            .unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn test_ends_with() {
        assert_eq!(
            call_builtin(
                "ends_with",
                vec![
                    Value::String("hello.yaml".into()),
                    Value::String(".yaml".into())
                ],
                &loc(),
                ""
            )
            .unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            call_builtin(
                "ends_with",
                vec![
                    Value::String("hello.yaml".into()),
                    Value::String(".json".into())
                ],
                &loc(),
                ""
            )
            .unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_min() {
        assert_eq!(
            call_builtin("min", vec![Value::Int(3), Value::Int(1)], &loc(), "").unwrap(),
            Value::Int(1)
        );
        assert_eq!(
            call_builtin("min", vec![Value::Float(3.5), Value::Float(1.2)], &loc(), "").unwrap(),
            Value::Float(1.2)
        );
        assert_eq!(
            call_builtin("min", vec![Value::Int(3), Value::Float(1.5)], &loc(), "").unwrap(),
            Value::Float(1.5)
        );
    }

    #[test]
    fn test_max() {
        assert_eq!(
            call_builtin("max", vec![Value::Int(3), Value::Int(1)], &loc(), "").unwrap(),
            Value::Int(3)
        );
        assert_eq!(
            call_builtin("max", vec![Value::Float(3.5), Value::Float(1.2)], &loc(), "").unwrap(),
            Value::Float(3.5)
        );
    }

    #[test]
    fn test_min_max_reject_non_numbers() {
        assert!(call_builtin(
            "min",
            vec![Value::String("a".into()), Value::String("b".into())],
            &loc(),
            ""
        )
        .is_err());
    }

    #[test]
    fn test_abs() {
        assert_eq!(
            call_builtin("abs", vec![Value::Int(-5)], &loc(), "").unwrap(),
            Value::Int(5)
        );
        assert_eq!(
            call_builtin("abs", vec![Value::Int(5)], &loc(), "").unwrap(),
            Value::Int(5)
        );
        assert_eq!(
            call_builtin("abs", vec![Value::Float(-3.14)], &loc(), "").unwrap(),
            Value::Float(3.14)
        );
        assert!(call_builtin("abs", vec![Value::String("x".into())], &loc(), "").is_err());
    }

    // ── P1 builtin tests ──────────────────────────────────────────────

    #[test]
    fn test_unique() {
        let arr = Value::Array(vec![
            Value::Int(1),
            Value::Int(2),
            Value::Int(1),
            Value::Int(3),
            Value::Int(2),
        ]);
        let result = call_builtin("unique", vec![arr], &loc(), "").unwrap();
        assert_eq!(
            result,
            Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
        );
    }

    #[test]
    fn test_unique_strings() {
        let arr = Value::Array(vec![
            Value::String("a".into()),
            Value::String("b".into()),
            Value::String("a".into()),
        ]);
        let result = call_builtin("unique", vec![arr], &loc(), "").unwrap();
        assert_eq!(
            result,
            Value::Array(vec![
                Value::String("a".into()),
                Value::String("b".into()),
            ])
        );
    }

    #[test]
    fn test_unique_preserves_order() {
        let arr = Value::Array(vec![
            Value::Int(3),
            Value::Int(1),
            Value::Int(3),
            Value::Int(2),
        ]);
        let result = call_builtin("unique", vec![arr], &loc(), "").unwrap();
        assert_eq!(
            result,
            Value::Array(vec![Value::Int(3), Value::Int(1), Value::Int(2)])
        );
    }

    #[test]
    fn test_sha256() {
        let result = call_builtin("sha256", vec![Value::String("hello".into())], &loc(), "").unwrap();
        assert_eq!(
            result,
            Value::String("2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".into())
        );
    }

    #[test]
    fn test_sha256_empty() {
        let result = call_builtin("sha256", vec![Value::String("".into())], &loc(), "").unwrap();
        assert_eq!(
            result,
            Value::String("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into())
        );
    }

    #[test]
    fn test_type_of() {
        assert_eq!(
            call_builtin("type_of", vec![Value::Null], &loc(), "").unwrap(),
            Value::String("null".into())
        );
        assert_eq!(
            call_builtin("type_of", vec![Value::Int(1)], &loc(), "").unwrap(),
            Value::String("int".into())
        );
        assert_eq!(
            call_builtin("type_of", vec![Value::Float(1.0)], &loc(), "").unwrap(),
            Value::String("float".into())
        );
        assert_eq!(
            call_builtin("type_of", vec![Value::String("x".into())], &loc(), "").unwrap(),
            Value::String("string".into())
        );
        assert_eq!(
            call_builtin("type_of", vec![Value::Bool(true)], &loc(), "").unwrap(),
            Value::String("bool".into())
        );
        assert_eq!(
            call_builtin("type_of", vec![Value::Array(vec![])], &loc(), "").unwrap(),
            Value::String("array".into())
        );
        assert_eq!(
            call_builtin("type_of", vec![Value::Object(IndexMap::new())], &loc(), "").unwrap(),
            Value::String("object".into())
        );
    }

    #[test]
    fn test_substring() {
        assert_eq!(
            call_builtin(
                "substring",
                vec![Value::String("hello world".into()), Value::Int(0), Value::Int(5)],
                &loc(),
                ""
            )
            .unwrap(),
            Value::String("hello".into())
        );
        assert_eq!(
            call_builtin(
                "substring",
                vec![Value::String("hello".into()), Value::Int(2)],
                &loc(),
                ""
            )
            .unwrap(),
            Value::String("llo".into())
        );
        // Out of bounds clamps
        assert_eq!(
            call_builtin(
                "substring",
                vec![Value::String("hi".into()), Value::Int(0), Value::Int(100)],
                &loc(),
                ""
            )
            .unwrap(),
            Value::String("hi".into())
        );
        // Start > end gives empty
        assert_eq!(
            call_builtin(
                "substring",
                vec![Value::String("hi".into()), Value::Int(5), Value::Int(2)],
                &loc(),
                ""
            )
            .unwrap(),
            Value::String("".into())
        );
    }

    // ── P2 builtin tests ──────────────────────────────────────────────

    #[test]
    fn test_entries() {
        let mut obj = IndexMap::new();
        obj.insert("a".to_string(), Value::Int(1));
        obj.insert("b".to_string(), Value::Int(2));
        let result = call_builtin("entries", vec![Value::Object(obj)], &loc(), "").unwrap();
        assert_eq!(
            result,
            Value::Array(vec![
                Value::Array(vec![Value::String("a".into()), Value::Int(1)]),
                Value::Array(vec![Value::String("b".into()), Value::Int(2)]),
            ])
        );
    }

    #[test]
    fn test_from_entries() {
        let pairs = Value::Array(vec![
            Value::Array(vec![Value::String("x".into()), Value::Int(10)]),
            Value::Array(vec![Value::String("y".into()), Value::Int(20)]),
        ]);
        let result = call_builtin("from_entries", vec![pairs], &loc(), "").unwrap();
        let mut expected = IndexMap::new();
        expected.insert("x".to_string(), Value::Int(10));
        expected.insert("y".to_string(), Value::Int(20));
        assert_eq!(result, Value::Object(expected));
    }

    #[test]
    fn test_entries_from_entries_roundtrip() {
        let mut obj = IndexMap::new();
        obj.insert("name".to_string(), Value::String("test".into()));
        obj.insert("port".to_string(), Value::Int(8080));
        let original = Value::Object(obj);
        let entries = call_builtin("entries", vec![original.clone()], &loc(), "").unwrap();
        let restored = call_builtin("from_entries", vec![entries], &loc(), "").unwrap();
        assert_eq!(restored, original);
    }

    #[test]
    fn test_from_entries_rejects_bad_pairs() {
        let bad = Value::Array(vec![Value::Int(1)]);
        assert!(call_builtin("from_entries", vec![bad], &loc(), "").is_err());
    }

    #[test]
    fn test_clamp_int() {
        assert_eq!(
            call_builtin(
                "clamp",
                vec![Value::Int(5), Value::Int(1), Value::Int(10)],
                &loc(),
                ""
            )
            .unwrap(),
            Value::Int(5)
        );
        assert_eq!(
            call_builtin(
                "clamp",
                vec![Value::Int(-5), Value::Int(0), Value::Int(10)],
                &loc(),
                ""
            )
            .unwrap(),
            Value::Int(0)
        );
        assert_eq!(
            call_builtin(
                "clamp",
                vec![Value::Int(100), Value::Int(0), Value::Int(10)],
                &loc(),
                ""
            )
            .unwrap(),
            Value::Int(10)
        );
    }

    #[test]
    fn test_clamp_float() {
        assert_eq!(
            call_builtin(
                "clamp",
                vec![Value::Float(3.14), Value::Float(0.0), Value::Float(1.0)],
                &loc(),
                ""
            )
            .unwrap(),
            Value::Float(1.0)
        );
    }

    #[test]
    fn test_reverse_array() {
        let arr = Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let result = call_builtin("reverse", vec![arr], &loc(), "").unwrap();
        assert_eq!(
            result,
            Value::Array(vec![Value::Int(3), Value::Int(2), Value::Int(1)])
        );
    }

    #[test]
    fn test_reverse_string() {
        assert_eq!(
            call_builtin("reverse", vec![Value::String("hello".into())], &loc(), "").unwrap(),
            Value::String("olleh".into())
        );
    }

    #[test]
    fn test_slice_array() {
        let arr = Value::Array(vec![
            Value::Int(10),
            Value::Int(20),
            Value::Int(30),
            Value::Int(40),
            Value::Int(50),
        ]);
        assert_eq!(
            call_builtin("slice", vec![arr.clone(), Value::Int(1), Value::Int(3)], &loc(), "")
                .unwrap(),
            Value::Array(vec![Value::Int(20), Value::Int(30)])
        );
        // Without end
        assert_eq!(
            call_builtin("slice", vec![arr.clone(), Value::Int(3)], &loc(), "").unwrap(),
            Value::Array(vec![Value::Int(40), Value::Int(50)])
        );
        // Negative index
        assert_eq!(
            call_builtin("slice", vec![arr, Value::Int(-2)], &loc(), "").unwrap(),
            Value::Array(vec![Value::Int(40), Value::Int(50)])
        );
    }

    #[test]
    fn test_slice_string() {
        assert_eq!(
            call_builtin(
                "slice",
                vec![Value::String("hello".into()), Value::Int(1), Value::Int(4)],
                &loc(),
                ""
            )
            .unwrap(),
            Value::String("ell".into())
        );
    }

    #[test]
    fn test_slice_empty_range() {
        let arr = Value::Array(vec![Value::Int(1), Value::Int(2)]);
        assert_eq!(
            call_builtin("slice", vec![arr, Value::Int(5), Value::Int(3)], &loc(), "").unwrap(),
            Value::Array(vec![])
        );
    }
}
