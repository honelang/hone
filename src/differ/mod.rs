//! Structural diff engine for Hone compiled values
//!
//! Compares two Value trees recursively and produces a list of differences
//! at specific paths within the structure.

use crate::evaluator::Value;

/// A single difference between two value trees
#[derive(Debug, Clone, PartialEq)]
pub struct DiffEntry {
    /// Dot-separated path to the changed value (e.g., "server.port")
    pub path: String,
    /// The kind of change
    pub kind: DiffKind,
}

/// The kind of difference found
#[derive(Debug, Clone, PartialEq)]
pub enum DiffKind {
    /// Key/index exists only in the left value
    Removed(Value),
    /// Key/index exists only in the right value
    Added(Value),
    /// Value changed between left and right
    Changed { left: Value, right: Value },
    /// Key was moved from one path to another (same value)
    Moved {
        from: String,
        to: String,
        value: Value,
    },
}

/// Compare two Value trees and return a list of differences.
///
/// Returns an empty vec if the values are structurally identical.
pub fn diff_values(left: &Value, right: &Value) -> Vec<DiffEntry> {
    let mut entries = Vec::new();
    diff_recursive(left, right, String::new(), &mut entries);
    entries
}

fn diff_recursive(left: &Value, right: &Value, path: String, entries: &mut Vec<DiffEntry>) {
    if left == right {
        return;
    }

    match (left, right) {
        (Value::Object(left_map), Value::Object(right_map)) => {
            // Check keys in left
            for (key, left_val) in left_map {
                let child_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", path, key)
                };

                match right_map.get(key) {
                    Some(right_val) => {
                        diff_recursive(left_val, right_val, child_path, entries);
                    }
                    None => {
                        entries.push(DiffEntry {
                            path: child_path,
                            kind: DiffKind::Removed(left_val.clone()),
                        });
                    }
                }
            }

            // Check keys only in right
            for (key, right_val) in right_map {
                if !left_map.contains_key(key) {
                    let child_path = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", path, key)
                    };
                    entries.push(DiffEntry {
                        path: child_path,
                        kind: DiffKind::Added(right_val.clone()),
                    });
                }
            }
        }

        (Value::Array(left_arr), Value::Array(right_arr)) => {
            let max_len = left_arr.len().max(right_arr.len());
            for i in 0..max_len {
                let child_path = if path.is_empty() {
                    format!("[{}]", i)
                } else {
                    format!("{}[{}]", path, i)
                };

                match (left_arr.get(i), right_arr.get(i)) {
                    (Some(l), Some(r)) => {
                        diff_recursive(l, r, child_path, entries);
                    }
                    (Some(l), None) => {
                        entries.push(DiffEntry {
                            path: child_path,
                            kind: DiffKind::Removed(l.clone()),
                        });
                    }
                    (None, Some(r)) => {
                        entries.push(DiffEntry {
                            path: child_path,
                            kind: DiffKind::Added(r.clone()),
                        });
                    }
                    (None, None) => unreachable!(),
                }
            }
        }

        // Different types or different scalar values
        _ => {
            entries.push(DiffEntry {
                path: if path.is_empty() {
                    "(root)".to_string()
                } else {
                    path
                },
                kind: DiffKind::Changed {
                    left: left.clone(),
                    right: right.clone(),
                },
            });
        }
    }
}

/// Compare two Value trees with move detection.
///
/// When a key is removed from one path and an identical value appears at
/// another path, this is reported as a `Moved` instead of Remove + Add.
pub fn diff_with_moves(left: &Value, right: &Value) -> Vec<DiffEntry> {
    let mut entries = diff_values(left, right);

    // Find moves: matching (Removed, Added) pairs with equal values
    let mut removed: Vec<(usize, String, Value)> = Vec::new();
    let mut added: Vec<(usize, String, Value)> = Vec::new();

    for (i, entry) in entries.iter().enumerate() {
        match &entry.kind {
            DiffKind::Removed(val) => removed.push((i, entry.path.clone(), val.clone())),
            DiffKind::Added(val) => added.push((i, entry.path.clone(), val.clone())),
            _ => {}
        }
    }

    // Match removed items with added items that have the same value
    let mut move_pairs: Vec<(usize, usize, String, String, Value)> = Vec::new();
    let mut used_added: std::collections::HashSet<usize> = std::collections::HashSet::new();

    for (ri, rpath, rval) in &removed {
        for (ai, apath, aval) in &added {
            if !used_added.contains(ai) && rval == aval {
                move_pairs.push((*ri, *ai, rpath.clone(), apath.clone(), rval.clone()));
                used_added.insert(*ai);
                break;
            }
        }
    }

    // Replace Remove/Add pairs with Moved entries
    // Collect indices to remove (in reverse order for safe removal)
    let mut indices_to_remove: Vec<usize> = Vec::new();
    for (ri, ai, _, _, _) in &move_pairs {
        indices_to_remove.push(*ri);
        indices_to_remove.push(*ai);
    }
    indices_to_remove.sort_unstable();
    indices_to_remove.dedup();
    for idx in indices_to_remove.into_iter().rev() {
        entries.remove(idx);
    }

    // Add Moved entries
    for (_, _, from, to, value) in move_pairs {
        entries.push(DiffEntry {
            path: to.clone(),
            kind: DiffKind::Moved { from, to, value },
        });
    }

    entries
}

/// Compile a Hone file at a specific git ref and return the output value
pub fn compile_at_ref(
    file_path: &std::path::Path,
    git_ref: &str,
) -> Result<Value, crate::errors::HoneError> {
    let file_name = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| crate::errors::HoneError::io_error("invalid file path"))?;

    // Get the file content at the given git ref
    let output = std::process::Command::new("git")
        .args(["show", &format!("{}:{}", git_ref, file_name)])
        .current_dir(file_path.parent().unwrap_or(std::path::Path::new(".")))
        .output()
        .map_err(|e| crate::errors::HoneError::io_error(format!("failed to run git: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(crate::errors::HoneError::io_error(format!(
            "git show failed for {}:{}: {}",
            git_ref,
            file_name,
            stderr.trim()
        )));
    }

    let source = String::from_utf8_lossy(&output.stdout).to_string();

    // Compile the source
    let base_dir = file_path.parent().unwrap_or(std::path::Path::new("."));
    let mut compiler = crate::compiler::Compiler::new(base_dir);
    compiler.compile_source(&source)
}

/// Annotate diff entries with git blame information
pub fn blame_diff(
    entries: &[DiffEntry],
    file_path: &std::path::Path,
) -> Vec<(DiffEntry, Option<BlameInfo>)> {
    entries
        .iter()
        .map(|entry| {
            let blame = get_blame_for_path(file_path, &entry.path);
            (entry.clone(), blame)
        })
        .collect()
}

/// Git blame information for a diff entry
#[derive(Debug, Clone)]
pub struct BlameInfo {
    pub commit: String,
    pub author: String,
    pub date: String,
}

/// Try to get blame info for a specific path in a file
fn get_blame_for_path(file_path: &std::path::Path, _key_path: &str) -> Option<BlameInfo> {
    // Run git log to find the last commit that touched this file
    let output = std::process::Command::new("git")
        .args(["log", "-1", "--format=%H|%an|%ai", "--"])
        .arg(file_path)
        .current_dir(file_path.parent().unwrap_or(std::path::Path::new(".")))
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let line = String::from_utf8_lossy(&output.stdout);
    let line = line.trim();
    let parts: Vec<&str> = line.splitn(3, '|').collect();
    if parts.len() == 3 {
        Some(BlameInfo {
            commit: parts[0][..8.min(parts[0].len())].to_string(),
            author: parts[1].to_string(),
            date: parts[2].to_string(),
        })
    } else {
        None
    }
}

/// Format blame-annotated diff entries as text
pub fn format_blame_text(entries: &[(DiffEntry, Option<BlameInfo>)]) -> String {
    let mut output = String::new();
    for (entry, blame) in entries {
        let blame_prefix = match blame {
            Some(info) => format!("[{} {} {}] ", info.commit, info.author, info.date),
            None => String::new(),
        };
        match &entry.kind {
            DiffKind::Added(val) => {
                output.push_str(&format!(
                    "{}+ {}: {}\n",
                    blame_prefix,
                    entry.path,
                    format_value_short(val)
                ));
            }
            DiffKind::Removed(val) => {
                output.push_str(&format!(
                    "{}- {}: {}\n",
                    blame_prefix,
                    entry.path,
                    format_value_short(val)
                ));
            }
            DiffKind::Changed { left, right } => {
                output.push_str(&format!(
                    "{}~ {}: {} -> {}\n",
                    blame_prefix,
                    entry.path,
                    format_value_short(left),
                    format_value_short(right)
                ));
            }
            DiffKind::Moved { from, to, value } => {
                output.push_str(&format!(
                    "{}> {} -> {}: {}\n",
                    blame_prefix,
                    from,
                    to,
                    format_value_short(value)
                ));
            }
        }
    }
    output
}

/// Format diff entries as human-readable text
pub fn format_diff_text(entries: &[DiffEntry]) -> String {
    let mut output = String::new();
    for entry in entries {
        match &entry.kind {
            DiffKind::Added(val) => {
                output.push_str(&format!("+ {}: {}\n", entry.path, format_value_short(val)));
            }
            DiffKind::Removed(val) => {
                output.push_str(&format!("- {}: {}\n", entry.path, format_value_short(val)));
            }
            DiffKind::Changed { left, right } => {
                output.push_str(&format!(
                    "~ {}: {} -> {}\n",
                    entry.path,
                    format_value_short(left),
                    format_value_short(right)
                ));
            }
            DiffKind::Moved { from, to, value } => {
                output.push_str(&format!(
                    "> {} -> {}: {}\n",
                    from,
                    to,
                    format_value_short(value)
                ));
            }
        }
    }
    output
}

/// Format diff entries as JSON
pub fn format_diff_json(entries: &[DiffEntry]) -> String {
    let mut parts = Vec::new();
    for entry in entries {
        let (op, detail) = match &entry.kind {
            DiffKind::Added(val) => ("added", format!("\"value\": {}", value_to_json(val))),
            DiffKind::Removed(val) => ("removed", format!("\"value\": {}", value_to_json(val))),
            DiffKind::Changed { left, right } => (
                "changed",
                format!(
                    "\"left\": {}, \"right\": {}",
                    value_to_json(left),
                    value_to_json(right)
                ),
            ),
            DiffKind::Moved { from, to, value } => (
                "moved",
                format!(
                    "\"from\": \"{}\", \"to\": \"{}\", \"value\": {}",
                    from,
                    to,
                    value_to_json(value)
                ),
            ),
        };
        parts.push(format!(
            "  {{\"path\": \"{}\", \"op\": \"{}\", {}}}",
            entry.path, op, detail
        ));
    }
    format!("[\n{}\n]", parts.join(",\n"))
}

/// Format a Value as a short display string
fn format_value_short(val: &Value) -> String {
    match val {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(n) => n.to_string(),
        Value::Float(f) => format!("{}", f),
        Value::String(s) => format!("\"{}\"", s),
        Value::Array(a) => format!("[{} items]", a.len()),
        Value::Object(o) => format!("{{{} keys}}", o.len()),
    }
}

/// Convert a Value to a JSON string (simple, for diff output)
fn value_to_json(val: &Value) -> String {
    match val {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(n) => n.to_string(),
        Value::Float(f) => format!("{}", f),
        Value::String(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
        Value::Array(a) => {
            let items: Vec<String> = a.iter().map(value_to_json).collect();
            format!("[{}]", items.join(", "))
        }
        Value::Object(o) => {
            let items: Vec<String> = o
                .iter()
                .map(|(k, v)| format!("\"{}\": {}", k, value_to_json(v)))
                .collect();
            format!("{{{}}}", items.join(", "))
        }
    }
}

/// Parse a comma-separated "key=val,key=val" string into key-value pairs
pub fn parse_arg_string(s: &str) -> Vec<(String, String)> {
    if s.is_empty() {
        return Vec::new();
    }
    s.split(',')
        .filter_map(|part| {
            let part = part.trim();
            let eq_pos = part.find('=')?;
            Some((part[..eq_pos].to_string(), part[eq_pos + 1..].to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    #[test]
    fn test_diff_identical() {
        let val = Value::Object({
            let mut m = IndexMap::new();
            m.insert("key".to_string(), Value::String("value".to_string()));
            m
        });
        let entries = diff_values(&val, &val);
        assert!(entries.is_empty());
    }

    #[test]
    fn test_diff_scalar_changed() {
        let left = Value::Int(42);
        let right = Value::Int(99);
        let entries = diff_values(&left, &right);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "(root)");
        assert!(matches!(&entries[0].kind, DiffKind::Changed { .. }));
    }

    #[test]
    fn test_diff_object_added_key() {
        let left = Value::Object({
            let mut m = IndexMap::new();
            m.insert("a".to_string(), Value::Int(1));
            m
        });
        let right = Value::Object({
            let mut m = IndexMap::new();
            m.insert("a".to_string(), Value::Int(1));
            m.insert("b".to_string(), Value::Int(2));
            m
        });
        let entries = diff_values(&left, &right);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "b");
        assert!(matches!(&entries[0].kind, DiffKind::Added(_)));
    }

    #[test]
    fn test_diff_object_removed_key() {
        let left = Value::Object({
            let mut m = IndexMap::new();
            m.insert("a".to_string(), Value::Int(1));
            m.insert("b".to_string(), Value::Int(2));
            m
        });
        let right = Value::Object({
            let mut m = IndexMap::new();
            m.insert("a".to_string(), Value::Int(1));
            m
        });
        let entries = diff_values(&left, &right);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "b");
        assert!(matches!(&entries[0].kind, DiffKind::Removed(_)));
    }

    #[test]
    fn test_diff_nested_change() {
        let left = Value::Object({
            let mut m = IndexMap::new();
            let mut inner = IndexMap::new();
            inner.insert("port".to_string(), Value::Int(8080));
            m.insert("server".to_string(), Value::Object(inner));
            m
        });
        let right = Value::Object({
            let mut m = IndexMap::new();
            let mut inner = IndexMap::new();
            inner.insert("port".to_string(), Value::Int(9090));
            m.insert("server".to_string(), Value::Object(inner));
            m
        });
        let entries = diff_values(&left, &right);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "server.port");
        assert!(matches!(
            &entries[0].kind,
            DiffKind::Changed {
                left: Value::Int(8080),
                right: Value::Int(9090)
            }
        ));
    }

    #[test]
    fn test_diff_array_length_change() {
        let left = Value::Array(vec![Value::Int(1), Value::Int(2)]);
        let right = Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let entries = diff_values(&left, &right);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "[2]");
        assert!(matches!(&entries[0].kind, DiffKind::Added(Value::Int(3))));
    }

    #[test]
    fn test_diff_array_element_change() {
        let left = Value::Array(vec![
            Value::String("a".to_string()),
            Value::String("b".to_string()),
        ]);
        let right = Value::Array(vec![
            Value::String("a".to_string()),
            Value::String("c".to_string()),
        ]);
        let entries = diff_values(&left, &right);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "[1]");
    }

    #[test]
    fn test_diff_type_change() {
        let left = Value::String("42".to_string());
        let right = Value::Int(42);
        let entries = diff_values(&left, &right);
        assert_eq!(entries.len(), 1);
        assert!(matches!(&entries[0].kind, DiffKind::Changed { .. }));
    }

    #[test]
    fn test_format_diff_text() {
        let entries = vec![
            DiffEntry {
                path: "port".to_string(),
                kind: DiffKind::Changed {
                    left: Value::Int(8080),
                    right: Value::Int(9090),
                },
            },
            DiffEntry {
                path: "debug".to_string(),
                kind: DiffKind::Added(Value::Bool(true)),
            },
        ];
        let text = format_diff_text(&entries);
        assert!(text.contains("~ port: 8080 -> 9090"));
        assert!(text.contains("+ debug: true"));
    }

    #[test]
    fn test_format_diff_json() {
        let entries = vec![DiffEntry {
            path: "port".to_string(),
            kind: DiffKind::Changed {
                left: Value::Int(8080),
                right: Value::Int(9090),
            },
        }];
        let json = format_diff_json(&entries);
        assert!(json.contains("\"path\": \"port\""));
        assert!(json.contains("\"op\": \"changed\""));
        assert!(json.contains("\"left\": 8080"));
        assert!(json.contains("\"right\": 9090"));
    }

    #[test]
    fn test_parse_arg_string() {
        let args = parse_arg_string("env=prod,port=8080");
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], ("env".to_string(), "prod".to_string()));
        assert_eq!(args[1], ("port".to_string(), "8080".to_string()));
    }

    #[test]
    fn test_parse_arg_string_empty() {
        let args = parse_arg_string("");
        assert!(args.is_empty());
    }

    #[test]
    fn test_diff_with_moves_detects_rename() {
        let left = Value::Object({
            let mut m = IndexMap::new();
            m.insert("old_name".to_string(), Value::String("hello".to_string()));
            m.insert("port".to_string(), Value::Int(8080));
            m
        });
        let right = Value::Object({
            let mut m = IndexMap::new();
            m.insert("new_name".to_string(), Value::String("hello".to_string()));
            m.insert("port".to_string(), Value::Int(8080));
            m
        });
        let entries = diff_with_moves(&left, &right);
        let has_moved = entries
            .iter()
            .any(|e| matches!(&e.kind, DiffKind::Moved { .. }));
        assert!(has_moved, "should detect moved key");
        // Should NOT have separate remove + add for the same value
        let has_removed = entries
            .iter()
            .any(|e| matches!(&e.kind, DiffKind::Removed(_)));
        let has_added = entries
            .iter()
            .any(|e| matches!(&e.kind, DiffKind::Added(_)));
        assert!(!has_removed, "removed should be replaced by moved");
        assert!(!has_added, "added should be replaced by moved");
    }

    #[test]
    fn test_diff_with_moves_no_false_positives() {
        let left = Value::Object({
            let mut m = IndexMap::new();
            m.insert("a".to_string(), Value::Int(1));
            m.insert("b".to_string(), Value::Int(2));
            m
        });
        let right = Value::Object({
            let mut m = IndexMap::new();
            m.insert("a".to_string(), Value::Int(1));
            m.insert("b".to_string(), Value::Int(3));
            m
        });
        let entries = diff_with_moves(&left, &right);
        let has_moved = entries
            .iter()
            .any(|e| matches!(&e.kind, DiffKind::Moved { .. }));
        assert!(!has_moved, "changed values should not be detected as moves");
    }

    #[test]
    fn test_diff_with_moves_changed_and_moved() {
        let left = Value::Object({
            let mut m = IndexMap::new();
            m.insert("x".to_string(), Value::String("moved_value".to_string()));
            m.insert("a".to_string(), Value::Int(1));
            m
        });
        let right = Value::Object({
            let mut m = IndexMap::new();
            m.insert("y".to_string(), Value::String("moved_value".to_string()));
            m.insert("a".to_string(), Value::Int(2));
            m
        });
        let entries = diff_with_moves(&left, &right);
        let has_moved = entries
            .iter()
            .any(|e| matches!(&e.kind, DiffKind::Moved { .. }));
        let has_changed = entries
            .iter()
            .any(|e| matches!(&e.kind, DiffKind::Changed { .. }));
        assert!(has_moved, "should detect moved key");
        assert!(has_changed, "should detect changed key");
    }

    #[test]
    fn test_format_diff_text_with_moved() {
        let entries = vec![DiffEntry {
            path: "new_key".to_string(),
            kind: DiffKind::Moved {
                from: "old_key".to_string(),
                to: "new_key".to_string(),
                value: Value::Int(42),
            },
        }];
        let text = format_diff_text(&entries);
        assert!(text.contains("> old_key -> new_key: 42"));
    }

    #[test]
    fn test_format_diff_json_with_moved() {
        let entries = vec![DiffEntry {
            path: "new_key".to_string(),
            kind: DiffKind::Moved {
                from: "old_key".to_string(),
                to: "new_key".to_string(),
                value: Value::Int(42),
            },
        }];
        let json = format_diff_json(&entries);
        assert!(json.contains("\"op\": \"moved\""));
        assert!(json.contains("\"from\": \"old_key\""));
        assert!(json.contains("\"to\": \"new_key\""));
    }
}
