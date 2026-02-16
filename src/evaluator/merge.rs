//! Merge engine for Hone configuration values
//!
//! Implements the three assignment operators:
//! - `:` (Colon) - Normal assignment with deep merge for objects
//! - `+:` (Append) - Array append, object merge
//! - `!:` (Replace) - Force replace, no merging

use super::value::Value;
use indexmap::IndexMap;

/// Merge strategy determined by assignment operator
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeStrategy {
    /// Normal merge (`:`) - deep merge objects, last value wins for scalars
    Normal,
    /// Append (`+:`) - concatenate arrays, merge objects
    Append,
    /// Replace (`!:`) - completely replace, no merging
    Replace,
}

/// Merge two values according to the given strategy
pub fn merge_values(base: Value, overlay: Value, strategy: MergeStrategy) -> Value {
    match strategy {
        MergeStrategy::Replace => overlay,
        MergeStrategy::Append => merge_append(base, overlay),
        MergeStrategy::Normal => merge_normal(base, overlay),
    }
}

/// Normal merge - deep merge for objects, overlay wins for other types
fn merge_normal(base: Value, overlay: Value) -> Value {
    match (base, overlay) {
        (Value::Object(mut base_obj), Value::Object(overlay_obj)) => {
            deep_merge_objects(&mut base_obj, overlay_obj, MergeStrategy::Normal);
            Value::Object(base_obj)
        }
        // For non-objects, overlay wins
        (_, overlay) => overlay,
    }
}

/// Append merge - concatenate arrays, merge objects
fn merge_append(base: Value, overlay: Value) -> Value {
    match (base, overlay) {
        (Value::Array(mut base_arr), Value::Array(overlay_arr)) => {
            base_arr.extend(overlay_arr);
            Value::Array(base_arr)
        }
        (Value::Object(mut base_obj), Value::Object(overlay_obj)) => {
            deep_merge_objects(&mut base_obj, overlay_obj, MergeStrategy::Append);
            Value::Object(base_obj)
        }
        // For mismatched types with append, overlay wins (with warning in real usage)
        (_, overlay) => overlay,
    }
}

/// Deep merge objects, recursively applying strategy
fn deep_merge_objects(
    base: &mut IndexMap<String, Value>,
    overlay: IndexMap<String, Value>,
    strategy: MergeStrategy,
) {
    for (key, overlay_value) in overlay {
        match base.get(&key).cloned() {
            Some(base_value) => {
                let merged = merge_values(base_value, overlay_value, strategy);
                base.insert(key, merged);
            }
            None => {
                base.insert(key, overlay_value);
            }
        }
    }
}

/// Merge multiple documents in order (later documents overlay earlier ones)
pub fn merge_documents(documents: Vec<Value>) -> Value {
    if documents.is_empty() {
        return Value::Object(IndexMap::new());
    }

    let mut iter = documents.into_iter();
    let mut result = iter.next().unwrap();

    for doc in iter {
        result = merge_values(result, doc, MergeStrategy::Normal);
    }

    result
}

/// Merge a base value with overlays, where each overlay has its own strategy
pub fn merge_with_strategies(base: Value, overlays: Vec<(Value, MergeStrategy)>) -> Value {
    let mut result = base;
    for (overlay, strategy) in overlays {
        result = merge_values(result, overlay, strategy);
    }
    result
}

/// Tracked value that remembers its merge strategy
#[derive(Debug, Clone)]
pub struct TrackedValue {
    pub value: Value,
    pub strategy: MergeStrategy,
}

impl TrackedValue {
    pub fn new(value: Value, strategy: MergeStrategy) -> Self {
        Self { value, strategy }
    }

    pub fn normal(value: Value) -> Self {
        Self::new(value, MergeStrategy::Normal)
    }

    pub fn append(value: Value) -> Self {
        Self::new(value, MergeStrategy::Append)
    }

    pub fn replace(value: Value) -> Self {
        Self::new(value, MergeStrategy::Replace)
    }
}

/// A builder for constructing merged configurations
#[derive(Debug, Default)]
pub struct MergeBuilder {
    layers: Vec<(Value, MergeStrategy)>,
}

impl MergeBuilder {
    pub fn new() -> Self {
        Self { layers: Vec::new() }
    }

    /// Add a base layer
    pub fn base(mut self, value: Value) -> Self {
        self.layers.push((value, MergeStrategy::Normal));
        self
    }

    /// Add an overlay layer with normal merge
    pub fn overlay(mut self, value: Value) -> Self {
        self.layers.push((value, MergeStrategy::Normal));
        self
    }

    /// Add an overlay layer with append merge
    pub fn append(mut self, value: Value) -> Self {
        self.layers.push((value, MergeStrategy::Append));
        self
    }

    /// Add an overlay layer with replace
    pub fn replace(mut self, value: Value) -> Self {
        self.layers.push((value, MergeStrategy::Replace));
        self
    }

    /// Build the final merged value
    pub fn build(self) -> Value {
        if self.layers.is_empty() {
            return Value::Object(IndexMap::new());
        }

        let mut iter = self.layers.into_iter();
        let (first, _) = iter.next().unwrap();
        let mut result = first;

        for (overlay, strategy) in iter {
            result = merge_values(result, overlay, strategy);
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(pairs: &[(&str, Value)]) -> Value {
        let mut map = IndexMap::new();
        for (k, v) in pairs {
            map.insert(k.to_string(), v.clone());
        }
        Value::Object(map)
    }

    fn arr(items: Vec<Value>) -> Value {
        Value::Array(items)
    }

    #[test]
    fn test_merge_normal_scalars() {
        // Overlay wins for scalars
        let base = Value::Int(1);
        let overlay = Value::Int(2);
        assert_eq!(
            merge_values(base, overlay, MergeStrategy::Normal),
            Value::Int(2)
        );
    }

    #[test]
    fn test_merge_normal_objects() {
        let base = obj(&[("a", Value::Int(1)), ("b", Value::Int(2))]);
        let overlay = obj(&[("b", Value::Int(3)), ("c", Value::Int(4))]);
        let result = merge_values(base, overlay, MergeStrategy::Normal);

        if let Value::Object(map) = result {
            assert_eq!(map.get("a"), Some(&Value::Int(1)));
            assert_eq!(map.get("b"), Some(&Value::Int(3))); // Overlay wins
            assert_eq!(map.get("c"), Some(&Value::Int(4)));
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn test_merge_deep_objects() {
        let base = obj(&[(
            "server",
            obj(&[
                ("host", Value::String("localhost".into())),
                ("port", Value::Int(8080)),
            ]),
        )]);
        let overlay = obj(&[(
            "server",
            obj(&[("port", Value::Int(9000)), ("debug", Value::Bool(true))]),
        )]);
        let result = merge_values(base, overlay, MergeStrategy::Normal);

        if let Value::Object(map) = result {
            if let Some(Value::Object(server)) = map.get("server") {
                assert_eq!(server.get("host"), Some(&Value::String("localhost".into())));
                assert_eq!(server.get("port"), Some(&Value::Int(9000))); // Overlay wins
                assert_eq!(server.get("debug"), Some(&Value::Bool(true)));
            } else {
                panic!("expected server object");
            }
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn test_merge_append_arrays() {
        let base = arr(vec![Value::Int(1), Value::Int(2)]);
        let overlay = arr(vec![Value::Int(3), Value::Int(4)]);
        let result = merge_values(base, overlay, MergeStrategy::Append);

        if let Value::Array(items) = result {
            assert_eq!(items.len(), 4);
            assert_eq!(items[0], Value::Int(1));
            assert_eq!(items[1], Value::Int(2));
            assert_eq!(items[2], Value::Int(3));
            assert_eq!(items[3], Value::Int(4));
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn test_merge_append_objects() {
        let base = obj(&[("a", Value::Int(1))]);
        let overlay = obj(&[("b", Value::Int(2))]);
        let result = merge_values(base, overlay, MergeStrategy::Append);

        if let Value::Object(map) = result {
            assert_eq!(map.get("a"), Some(&Value::Int(1)));
            assert_eq!(map.get("b"), Some(&Value::Int(2)));
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn test_merge_replace() {
        let base = obj(&[("a", Value::Int(1)), ("b", Value::Int(2))]);
        let overlay = obj(&[("c", Value::Int(3))]);
        let result = merge_values(base, overlay, MergeStrategy::Replace);

        // Replace completely ignores base
        if let Value::Object(map) = result {
            assert_eq!(map.get("a"), None);
            assert_eq!(map.get("c"), Some(&Value::Int(3)));
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn test_merge_builder() {
        let result = MergeBuilder::new()
            .base(obj(&[("a", Value::Int(1))]))
            .overlay(obj(&[("b", Value::Int(2))]))
            .append(obj(&[("c", Value::Int(3))]))
            .build();

        if let Value::Object(map) = result {
            assert_eq!(map.get("a"), Some(&Value::Int(1)));
            assert_eq!(map.get("b"), Some(&Value::Int(2)));
            assert_eq!(map.get("c"), Some(&Value::Int(3)));
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn test_merge_documents() {
        let docs = vec![
            obj(&[("env", Value::String("base".into()))]),
            obj(&[
                ("env", Value::String("prod".into())),
                ("debug", Value::Bool(false)),
            ]),
        ];
        let result = merge_documents(docs);

        if let Value::Object(map) = result {
            assert_eq!(map.get("env"), Some(&Value::String("prod".into())));
            assert_eq!(map.get("debug"), Some(&Value::Bool(false)));
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn test_nested_array_in_object_merge() {
        let base = obj(&[("servers", arr(vec![Value::String("server1".into())]))]);
        let overlay = obj(&[("servers", arr(vec![Value::String("server2".into())]))]);

        // Normal merge replaces arrays (doesn't concatenate)
        let result = merge_values(base.clone(), overlay.clone(), MergeStrategy::Normal);
        if let Value::Object(map) = &result {
            if let Some(Value::Array(servers)) = map.get("servers") {
                assert_eq!(servers.len(), 1);
                assert_eq!(servers[0], Value::String("server2".into()));
            }
        }

        // Append merge concatenates arrays
        let result = merge_values(base, overlay, MergeStrategy::Append);
        if let Value::Object(map) = result {
            if let Some(Value::Array(servers)) = map.get("servers") {
                assert_eq!(servers.len(), 2);
                assert_eq!(servers[0], Value::String("server1".into()));
                assert_eq!(servers[1], Value::String("server2".into()));
            }
        }
    }

    #[test]
    fn test_type_mismatch_overlay_wins() {
        // When types don't match, overlay always wins
        let base = Value::Int(42);
        let overlay = Value::String("hello".into());
        assert_eq!(
            merge_values(base, overlay, MergeStrategy::Normal),
            Value::String("hello".into())
        );

        let base = obj(&[("x", Value::Int(1))]);
        let overlay = Value::Array(vec![Value::Int(1)]);
        assert_eq!(
            merge_values(base, overlay, MergeStrategy::Normal),
            Value::Array(vec![Value::Int(1)])
        );
    }
}
