//! Dependency graph visualization for Hone
//!
//! Generates DOT, JSON, or text representations of the import dependency graph.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::errors::{HoneError, HoneResult};
use crate::resolver::ImportResolver;

/// Output format for graph visualization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphFormat {
    /// DOT format for Graphviz
    Dot,
    /// JSON format for programmatic consumption
    Json,
    /// Text tree format (like the `tree` command)
    Text,
}

impl GraphFormat {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "dot" | "graphviz" => Some(GraphFormat::Dot),
            "json" => Some(GraphFormat::Json),
            "text" | "tree" => Some(GraphFormat::Text),
            _ => None,
        }
    }
}

/// A node in the dependency graph
#[derive(Debug, Clone)]
struct GraphNode {
    path: PathBuf,
    label: String,
}

/// An edge in the dependency graph
#[derive(Debug, Clone)]
struct GraphEdge {
    from: PathBuf,
    to: PathBuf,
    kind: EdgeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EdgeKind {
    Import,
    From,
}

/// Generate a dependency graph for a file and all its imports
pub fn generate_graph(path: impl AsRef<Path>, format: GraphFormat) -> HoneResult<String> {
    let path = path.as_ref();
    let canonical = path.canonicalize().map_err(|e| {
        HoneError::io_error(format!("failed to resolve path {}: {}", path.display(), e))
    })?;

    let base_dir = canonical.parent().unwrap_or(Path::new("."));
    let mut resolver = ImportResolver::new(base_dir);

    // Resolve the root file (this recursively resolves all deps)
    resolver.resolve(&canonical)?;

    // Get topological order
    let order = resolver.topological_order(&canonical)?;

    // Build graph data
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let root_dir = base_dir;

    for resolved in &order {
        let label = make_label(&resolved.path, root_dir);
        nodes.push(GraphNode {
            path: resolved.path.clone(),
            label,
        });

        if let Some(ref from) = resolved.from_path {
            edges.push(GraphEdge {
                from: resolved.path.clone(),
                to: from.clone(),
                kind: EdgeKind::From,
            });
        }

        for import in &resolved.import_paths {
            edges.push(GraphEdge {
                from: resolved.path.clone(),
                to: import.clone(),
                kind: EdgeKind::Import,
            });
        }
    }

    match format {
        GraphFormat::Dot => Ok(format_dot(&nodes, &edges, &canonical)),
        GraphFormat::Json => Ok(format_json(&nodes, &edges, root_dir)),
        GraphFormat::Text => Ok(format_text(&nodes, &edges, &canonical, root_dir)),
    }
}

/// Create a short label from a file path relative to root
fn make_label(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        })
}

/// Generate DOT format output
fn format_dot(nodes: &[GraphNode], edges: &[GraphEdge], root: &PathBuf) -> String {
    let mut out = String::from("digraph dependencies {\n");
    out.push_str("  rankdir=LR;\n");
    out.push_str("  node [shape=box, fontname=\"monospace\", fontsize=10];\n");
    out.push_str("  edge [fontname=\"monospace\", fontsize=8];\n\n");

    // Nodes
    for node in nodes {
        let id = node_id(&node.path);
        let style = if node.path == *root {
            ", style=filled, fillcolor=\"#89b4fa\", fontcolor=\"#1e1e2e\""
        } else {
            ""
        };
        out.push_str(&format!("  {} [label=\"{}\"{}];\n", id, node.label, style));
    }

    out.push('\n');

    // Edges
    for edge in edges {
        let from_id = node_id(&edge.from);
        let to_id = node_id(&edge.to);
        let style = match edge.kind {
            EdgeKind::Import => "",
            EdgeKind::From => " [style=dashed, label=\"from\"]",
        };
        out.push_str(&format!("  {} -> {}{};\n", from_id, to_id, style));
    }

    out.push_str("}\n");
    out
}

/// Generate JSON format output
fn format_json(nodes: &[GraphNode], edges: &[GraphEdge], root: &Path) -> String {
    let mut json = String::from("{\n  \"nodes\": [\n");

    for (i, node) in nodes.iter().enumerate() {
        let path = make_label(&node.path, root);
        json.push_str(&format!(
            "    {{\"path\": \"{}\", \"label\": \"{}\"}}",
            json_escape(&path),
            json_escape(&node.label)
        ));
        if i < nodes.len() - 1 {
            json.push(',');
        }
        json.push('\n');
    }

    json.push_str("  ],\n  \"edges\": [\n");

    for (i, edge) in edges.iter().enumerate() {
        let from = make_label(&edge.from, root);
        let to = make_label(&edge.to, root);
        let kind = match edge.kind {
            EdgeKind::Import => "import",
            EdgeKind::From => "from",
        };
        json.push_str(&format!(
            "    {{\"from\": \"{}\", \"to\": \"{}\", \"kind\": \"{}\"}}",
            json_escape(&from),
            json_escape(&to),
            kind
        ));
        if i < edges.len() - 1 {
            json.push(',');
        }
        json.push('\n');
    }

    json.push_str("  ]\n}\n");
    json
}

/// Generate text tree format output
fn format_text(
    _nodes: &[GraphNode],
    edges: &[GraphEdge],
    root: &PathBuf,
    root_dir: &Path,
) -> String {
    // Build adjacency list
    let mut children: HashMap<PathBuf, Vec<(PathBuf, EdgeKind)>> = HashMap::new();
    for edge in edges {
        children
            .entry(edge.from.clone())
            .or_default()
            .push((edge.to.clone(), edge.kind));
    }

    let mut out = String::new();
    let label = make_label(root, root_dir);
    out.push_str(&label);
    out.push('\n');

    let mut visited = std::collections::HashSet::new();
    visited.insert(root.clone());
    print_tree(&mut out, root, &children, "", true, &mut visited, root_dir);

    out
}

fn print_tree(
    out: &mut String,
    node: &PathBuf,
    children: &HashMap<PathBuf, Vec<(PathBuf, EdgeKind)>>,
    prefix: &str,
    _is_root: bool,
    visited: &mut std::collections::HashSet<PathBuf>,
    root_dir: &Path,
) {
    if let Some(deps) = children.get(node) {
        for (i, (dep, kind)) in deps.iter().enumerate() {
            let is_last = i == deps.len() - 1;
            let connector = if is_last { "\\-- " } else { "|-- " };
            let label = make_label(dep, root_dir);
            let kind_label = match kind {
                EdgeKind::Import => "",
                EdgeKind::From => " (from)",
            };

            let circular = if visited.contains(dep) {
                " [circular]"
            } else {
                ""
            };

            out.push_str(&format!(
                "{}{}{}{}{}\n",
                prefix, connector, label, kind_label, circular
            ));

            if !visited.contains(dep) {
                visited.insert(dep.clone());
                let new_prefix = format!("{}{}", prefix, if is_last { "    " } else { "|   " });
                print_tree(out, dep, children, &new_prefix, false, visited, root_dir);
            }
        }
    }
}

/// Generate a DOT-safe node ID from a path
fn node_id(path: &Path) -> String {
    let s = path.to_string_lossy().to_string();
    format!(
        "n{}",
        s.chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect::<String>()
    )
}

/// Escape a string for JSON
fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_files(dir: &Path, files: &[(&str, &str)]) {
        for (name, content) in files {
            let path = dir.join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, content).unwrap();
        }
    }

    #[test]
    fn test_single_file_graph_text() {
        let dir = TempDir::new().unwrap();
        create_test_files(dir.path(), &[("main.hone", "key: \"value\"")]);

        let result = generate_graph(dir.path().join("main.hone"), GraphFormat::Text).unwrap();
        assert!(result.contains("main.hone"));
    }

    #[test]
    fn test_multi_file_graph_text() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[
                ("config.hone", "let port = 8080"),
                (
                    "main.hone",
                    "import \"./config.hone\" as config\nvalue: config.port",
                ),
            ],
        );

        let result = generate_graph(dir.path().join("main.hone"), GraphFormat::Text).unwrap();
        assert!(result.contains("main.hone"));
        assert!(result.contains("config.hone"));
    }

    #[test]
    fn test_from_graph_text() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[
                ("base.hone", "key: \"base\""),
                ("overlay.hone", "from \"./base.hone\"\nkey: \"overlay\""),
            ],
        );

        let result = generate_graph(dir.path().join("overlay.hone"), GraphFormat::Text).unwrap();
        assert!(result.contains("overlay.hone"));
        assert!(result.contains("base.hone"));
        assert!(result.contains("(from)"));
    }

    #[test]
    fn test_graph_dot_format() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[
                ("config.hone", "let port = 8080"),
                (
                    "main.hone",
                    "import \"./config.hone\" as config\nvalue: config.port",
                ),
            ],
        );

        let result = generate_graph(dir.path().join("main.hone"), GraphFormat::Dot).unwrap();
        assert!(result.contains("digraph dependencies"));
        assert!(result.contains("->"));
    }

    #[test]
    fn test_graph_json_format() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[
                ("config.hone", "let port = 8080"),
                (
                    "main.hone",
                    "import \"./config.hone\" as config\nvalue: config.port",
                ),
            ],
        );

        let result = generate_graph(dir.path().join("main.hone"), GraphFormat::Json).unwrap();
        assert!(result.contains("\"nodes\""));
        assert!(result.contains("\"edges\""));
        assert!(result.contains("\"kind\": \"import\""));
    }

    #[test]
    fn test_diamond_dependency() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[
                ("shared.hone", "let common = 42"),
                (
                    "a.hone",
                    "import \"./shared.hone\" as shared\nlet a_val = shared.common",
                ),
                (
                    "b.hone",
                    "import \"./shared.hone\" as shared\nlet b_val = shared.common",
                ),
                (
                    "main.hone",
                    "import \"./a.hone\" as a\nimport \"./b.hone\" as b\nresult: a.a_val",
                ),
            ],
        );

        let result = generate_graph(dir.path().join("main.hone"), GraphFormat::Text).unwrap();
        assert!(result.contains("main.hone"));
        assert!(result.contains("a.hone"));
        assert!(result.contains("b.hone"));
        assert!(result.contains("shared.hone"));
    }
}
