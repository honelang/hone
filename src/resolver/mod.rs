//! Import Resolver for Hone configuration language
//!
//! This module handles:
//! - Resolving file paths (relative to importing file)
//! - Detecting circular imports
//! - Parsing and caching imported files
//! - Building dependency graphs

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::errors::{HoneError, HoneResult};
use crate::lexer::Lexer;
use crate::parser::ast::{
    File, FromStatement, ImportKind, ImportStatement, PreambleItem, StringPart,
};
use crate::parser::Parser;

/// Normalize a path by resolving `.` and `..` components
fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();

    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                // Pop the last component if possible
                if !components.is_empty() {
                    components.pop();
                }
            }
            std::path::Component::CurDir => {
                // Skip `.`
            }
            c => {
                components.push(c);
            }
        }
    }

    components.iter().collect()
}

/// A resolved and parsed file with its dependencies
#[derive(Debug)]
pub struct ResolvedFile {
    /// Canonical path to this file
    pub path: PathBuf,
    /// Parsed AST
    pub ast: File,
    /// Source code (needed for error reporting)
    pub source: String,
    /// Files this file inherits from (via `from`)
    pub from_path: Option<PathBuf>,
    /// Files this file imports
    pub import_paths: Vec<PathBuf>,
}

/// Import resolver that handles file loading and circular import detection
pub struct ImportResolver {
    /// Cache of already-resolved files
    cache: HashMap<PathBuf, ResolvedFile>,
    /// Stack of files currently being resolved (for cycle detection)
    resolution_stack: Vec<PathBuf>,
    /// Base directory for resolving paths (if not absolute)
    base_dir: PathBuf,
}

impl ImportResolver {
    /// Create a new import resolver
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            cache: HashMap::new(),
            resolution_stack: Vec::new(),
            base_dir: base_dir.into(),
        }
    }

    /// Resolve a file and all its dependencies
    pub fn resolve(&mut self, path: impl AsRef<Path>) -> HoneResult<&ResolvedFile> {
        let path = self.canonicalize_path(path.as_ref())?;

        // Check if already resolved
        if self.cache.contains_key(&path) {
            return Ok(self.cache.get(&path).unwrap());
        }

        // Check for circular import
        if self.resolution_stack.contains(&path) {
            let cycle = self.format_cycle(&path);
            return Err(HoneError::CircularImport {
                src: String::new(),
                span: (0, 0).into(),
                chain: cycle,
            });
        }

        // Push onto resolution stack
        self.resolution_stack.push(path.clone());

        // Read and parse the file
        let source = std::fs::read_to_string(&path).map_err(|e| {
            HoneError::io_error(format!("failed to read {}: {}", path.display(), e))
        })?;

        let mut lexer = Lexer::new(&source, Some(path.clone()));
        let tokens = lexer.tokenize()?;
        let mut parser = Parser::new(tokens, &source, Some(path.clone()));
        let ast = parser.parse()?;

        // Extract dependencies
        let (from_path, import_paths) = self.extract_dependencies(&ast, &path)?;

        // Recursively resolve dependencies
        if let Some(ref from) = from_path {
            self.resolve(from)?;
        }
        for import in &import_paths {
            self.resolve(import)?;
        }

        // Pop from resolution stack
        self.resolution_stack.pop();

        // Cache the resolved file
        let resolved = ResolvedFile {
            path: path.clone(),
            ast,
            source,
            from_path,
            import_paths,
        };

        self.cache.insert(path.clone(), resolved);
        Ok(self.cache.get(&path).unwrap())
    }

    /// Resolve a file from source string (for testing or embedded sources)
    pub fn resolve_source(
        &mut self,
        name: impl Into<PathBuf>,
        source: impl Into<String>,
    ) -> HoneResult<&ResolvedFile> {
        let path = name.into();
        let source = source.into();

        // Check if already resolved
        if self.cache.contains_key(&path) {
            return Ok(self.cache.get(&path).unwrap());
        }

        // Parse the source
        let mut lexer = Lexer::new(&source, Some(path.clone()));
        let tokens = lexer.tokenize()?;
        let mut parser = Parser::new(tokens, &source, Some(path.clone()));
        let ast = parser.parse()?;

        // Extract dependencies (but don't resolve them - caller is responsible)
        let (from_path, import_paths) = self.extract_dependencies(&ast, &path)?;

        // Cache the resolved file
        let resolved = ResolvedFile {
            path: path.clone(),
            ast,
            source,
            from_path,
            import_paths,
        };

        self.cache.insert(path.clone(), resolved);
        Ok(self.cache.get(&path).unwrap())
    }

    /// Get a previously resolved file from cache
    pub fn get(&self, path: &Path) -> Option<&ResolvedFile> {
        self.cache.get(path)
    }

    /// Get all resolved files
    pub fn files(&self) -> impl Iterator<Item = &ResolvedFile> {
        self.cache.values()
    }

    /// Get topologically sorted files (dependencies first)
    pub fn topological_order(&self, root: &Path) -> HoneResult<Vec<&ResolvedFile>> {
        let mut visited = HashSet::new();
        let mut result = Vec::new();

        self.visit_topological(root, &mut visited, &mut result)?;

        Ok(result)
    }

    fn visit_topological<'a>(
        &'a self,
        path: &Path,
        visited: &mut HashSet<PathBuf>,
        result: &mut Vec<&'a ResolvedFile>,
    ) -> HoneResult<()> {
        if visited.contains(path) {
            return Ok(());
        }

        let resolved = self
            .cache
            .get(path)
            .ok_or_else(|| HoneError::io_error(format!("file not resolved: {}", path.display())))?;

        visited.insert(path.to_path_buf());

        // Visit dependencies first
        if let Some(ref from) = resolved.from_path {
            self.visit_topological(from, visited, result)?;
        }
        for import in &resolved.import_paths {
            self.visit_topological(import, visited, result)?;
        }

        result.push(resolved);
        Ok(())
    }

    /// Extract from and import paths from AST
    fn extract_dependencies(
        &self,
        ast: &File,
        current_file: &Path,
    ) -> HoneResult<(Option<PathBuf>, Vec<PathBuf>)> {
        let mut from_path = None;
        let mut import_paths = Vec::new();

        let parent_dir = current_file.parent().unwrap_or(Path::new("."));

        // Process main document preamble
        for item in &ast.preamble {
            match item {
                PreambleItem::From(from) => {
                    let path = self.resolve_import_path(from, parent_dir)?;
                    if from_path.is_some() {
                        // This should be caught by the parser, but double-check
                        return Err(HoneError::MultipleFrom {
                            src: String::new(),
                            span: (from.location.offset, from.location.length).into(),
                            first_span: (0, 0).into(),
                        });
                    }
                    from_path = Some(path);
                }
                PreambleItem::Import(import) => {
                    let path = self.resolve_import_path_from_import(import, parent_dir)?;
                    import_paths.push(path);
                }
                _ => {}
            }
        }

        // Process sub-documents
        for doc in &ast.documents {
            for item in &doc.preamble {
                match item {
                    PreambleItem::From(from) => {
                        let path = self.resolve_import_path(from, parent_dir)?;
                        // Each document can have its own `from`
                        // We track all of them as dependencies
                        if !import_paths.contains(&path) && from_path.as_ref() != Some(&path) {
                            import_paths.push(path);
                        }
                    }
                    PreambleItem::Import(import) => {
                        let path = self.resolve_import_path_from_import(import, parent_dir)?;
                        if !import_paths.contains(&path) {
                            import_paths.push(path);
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok((from_path, import_paths))
    }

    /// Resolve a path from a `from` statement
    fn resolve_import_path(&self, from: &FromStatement, parent_dir: &Path) -> HoneResult<PathBuf> {
        let path_str = self.string_expr_to_string(&from.path)?;
        self.resolve_path_string(&path_str, parent_dir, &from.location)
    }

    /// Resolve a path from an `import` statement
    fn resolve_import_path_from_import(
        &self,
        import: &ImportStatement,
        parent_dir: &Path,
    ) -> HoneResult<PathBuf> {
        let path_expr = match &import.kind {
            ImportKind::Whole { path, .. } => path,
            ImportKind::Named { path, .. } => path,
        };
        let path_str = self.string_expr_to_string(path_expr)?;
        self.resolve_path_string(&path_str, parent_dir, &import.location)
    }

    /// Convert a StringExpr to a plain string (error if interpolation present)
    fn string_expr_to_string(&self, expr: &crate::parser::ast::StringExpr) -> HoneResult<String> {
        let mut result = String::new();
        for part in &expr.parts {
            match part {
                StringPart::Literal(s) => result.push_str(s),
                StringPart::Interpolation(_) => {
                    return Err(HoneError::unexpected_token(
                        String::new(),
                        &expr.location,
                        "literal string path",
                        "string interpolation",
                        "import paths cannot contain interpolations - use a literal string",
                    ));
                }
            }
        }
        Ok(result)
    }

    /// Resolve a path string relative to a parent directory
    fn resolve_path_string(
        &self,
        path_str: &str,
        parent_dir: &Path,
        location: &crate::lexer::token::SourceLocation,
    ) -> HoneResult<PathBuf> {
        let path = Path::new(path_str);

        // If absolute, use as-is
        if path.is_absolute() {
            return self.canonicalize_path(path);
        }

        // Relative path - resolve from parent directory
        let resolved = parent_dir.join(path);

        // Try to canonicalize, but if file doesn't exist, return normalized path
        match resolved.canonicalize() {
            Ok(canonical) => Ok(canonical),
            Err(_) => {
                // File doesn't exist
                Err(HoneError::ImportNotFound {
                    src: String::new(),
                    span: (location.offset, location.length).into(),
                    path: path_str.to_string(),
                })
            }
        }
    }

    /// Canonicalize a path, handling errors appropriately
    fn canonicalize_path(&self, path: &Path) -> HoneResult<PathBuf> {
        if path.is_absolute() {
            path.canonicalize().map_err(|e| {
                HoneError::io_error(format!("failed to resolve path {}: {}", path.display(), e))
            })
        } else {
            let full = self.base_dir.join(path);
            full.canonicalize().map_err(|e| {
                HoneError::io_error(format!("failed to resolve path {}: {}", full.display(), e))
            })
        }
    }

    /// Format a cycle for error reporting
    fn format_cycle(&self, target: &Path) -> String {
        let mut cycle_parts: Vec<String> = self
            .resolution_stack
            .iter()
            .skip_while(|p| *p != target)
            .map(|p| p.display().to_string())
            .collect();

        cycle_parts.push(target.display().to_string());
        cycle_parts.join(" -> ")
    }
}

/// Builder for creating test fixtures
#[derive(Default)]
pub struct TestFixtureBuilder {
    files: HashMap<PathBuf, String>,
}

impl TestFixtureBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_file(mut self, path: impl Into<PathBuf>, content: impl Into<String>) -> Self {
        self.files.insert(path.into(), content.into());
        self
    }

    pub fn build(self) -> TestFixture {
        TestFixture { files: self.files }
    }
}

/// Test fixture that provides virtual files
pub struct TestFixture {
    files: HashMap<PathBuf, String>,
}

impl TestFixture {
    /// Create a resolver that uses these virtual files
    pub fn resolver(&self) -> VirtualResolver {
        VirtualResolver::new(self.files.clone())
    }
}

/// A resolver that works with virtual (in-memory) files
pub struct VirtualResolver {
    files: HashMap<PathBuf, String>,
    cache: HashMap<PathBuf, ResolvedFile>,
    resolution_stack: Vec<PathBuf>,
}

impl VirtualResolver {
    pub fn new(files: HashMap<PathBuf, String>) -> Self {
        // Normalize all file keys so lookups match normalized import paths
        let normalized_files = files
            .into_iter()
            .map(|(k, v)| (normalize_path(&k), v))
            .collect();
        Self {
            files: normalized_files,
            cache: HashMap::new(),
            resolution_stack: Vec::new(),
        }
    }

    /// Get a previously resolved file from cache
    pub fn get(&self, path: &Path) -> Option<&ResolvedFile> {
        self.cache.get(&normalize_path(path))
    }

    /// Add a virtual file
    pub fn add_file(&mut self, path: impl Into<PathBuf>, content: impl Into<String>) {
        self.files
            .insert(normalize_path(&path.into()), content.into());
    }

    /// Resolve a virtual file
    pub fn resolve(&mut self, path: impl AsRef<Path>) -> HoneResult<&ResolvedFile> {
        let path = normalize_path(path.as_ref());

        // Check if already resolved
        if self.cache.contains_key(&path) {
            return Ok(self.cache.get(&path).unwrap());
        }

        // Check for circular import
        if self.resolution_stack.contains(&path) {
            let cycle = self.format_cycle(&path);
            return Err(HoneError::CircularImport {
                src: String::new(),
                span: (0, 0).into(),
                chain: cycle,
            });
        }

        // Get the virtual file content
        let source = self
            .files
            .get(&path)
            .ok_or_else(|| HoneError::ImportNotFound {
                src: String::new(),
                span: (0, 0).into(),
                path: path.display().to_string(),
            })?
            .clone();

        // Push onto resolution stack
        self.resolution_stack.push(path.clone());

        // Parse the source
        let mut lexer = Lexer::new(&source, Some(path.clone()));
        let tokens = lexer.tokenize()?;
        let mut parser = Parser::new(tokens, &source, Some(path.clone()));
        let ast = parser.parse()?;

        // Extract dependencies
        let (from_path, import_paths) = self.extract_dependencies(&ast, &path)?;

        // Recursively resolve dependencies
        if let Some(ref from) = from_path {
            self.resolve(from)?;
        }
        for import in &import_paths {
            self.resolve(import)?;
        }

        // Pop from resolution stack
        self.resolution_stack.pop();

        // Cache the resolved file
        let resolved = ResolvedFile {
            path: path.clone(),
            ast,
            source,
            from_path,
            import_paths,
        };

        self.cache.insert(path.clone(), resolved);
        Ok(self.cache.get(&path).unwrap())
    }

    /// Get topologically sorted files
    pub fn topological_order(&self, root: &Path) -> HoneResult<Vec<&ResolvedFile>> {
        let mut visited = HashSet::new();
        let mut result = Vec::new();

        self.visit_topological(&normalize_path(root), &mut visited, &mut result)?;

        Ok(result)
    }

    fn visit_topological<'a>(
        &'a self,
        path: &Path,
        visited: &mut HashSet<PathBuf>,
        result: &mut Vec<&'a ResolvedFile>,
    ) -> HoneResult<()> {
        if visited.contains(path) {
            return Ok(());
        }

        let resolved = self
            .cache
            .get(path)
            .ok_or_else(|| HoneError::io_error(format!("file not resolved: {}", path.display())))?;

        visited.insert(path.to_path_buf());

        // Visit dependencies first
        if let Some(ref from) = resolved.from_path {
            self.visit_topological(from, visited, result)?;
        }
        for import in &resolved.import_paths {
            self.visit_topological(import, visited, result)?;
        }

        result.push(resolved);
        Ok(())
    }

    fn extract_dependencies(
        &self,
        ast: &File,
        current_file: &Path,
    ) -> HoneResult<(Option<PathBuf>, Vec<PathBuf>)> {
        let mut from_path = None;
        let mut import_paths = Vec::new();

        let parent_dir = current_file.parent().unwrap_or(Path::new(""));

        for item in &ast.preamble {
            match item {
                PreambleItem::From(from) => {
                    let path = self.resolve_import_path(from, parent_dir)?;
                    from_path = Some(path);
                }
                PreambleItem::Import(import) => {
                    let path = self.resolve_import_path_from_import(import, parent_dir)?;
                    import_paths.push(path);
                }
                _ => {}
            }
        }

        for doc in &ast.documents {
            for item in &doc.preamble {
                match item {
                    PreambleItem::From(from) => {
                        let path = self.resolve_import_path(from, parent_dir)?;
                        if !import_paths.contains(&path) && from_path.as_ref() != Some(&path) {
                            import_paths.push(path);
                        }
                    }
                    PreambleItem::Import(import) => {
                        let path = self.resolve_import_path_from_import(import, parent_dir)?;
                        if !import_paths.contains(&path) {
                            import_paths.push(path);
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok((from_path, import_paths))
    }

    fn resolve_import_path(&self, from: &FromStatement, parent_dir: &Path) -> HoneResult<PathBuf> {
        let path_str = self.string_expr_to_string(&from.path)?;
        self.resolve_path_string(&path_str, parent_dir)
    }

    fn resolve_import_path_from_import(
        &self,
        import: &ImportStatement,
        parent_dir: &Path,
    ) -> HoneResult<PathBuf> {
        let path_expr = match &import.kind {
            ImportKind::Whole { path, .. } => path,
            ImportKind::Named { path, .. } => path,
        };
        let path_str = self.string_expr_to_string(path_expr)?;
        self.resolve_path_string(&path_str, parent_dir)
    }

    fn string_expr_to_string(&self, expr: &crate::parser::ast::StringExpr) -> HoneResult<String> {
        let mut result = String::new();
        for part in &expr.parts {
            match part {
                StringPart::Literal(s) => result.push_str(s),
                StringPart::Interpolation(_) => {
                    return Err(HoneError::unexpected_token(
                        String::new(),
                        &expr.location,
                        "literal string path",
                        "string interpolation",
                        "import paths cannot contain interpolations",
                    ));
                }
            }
        }
        Ok(result)
    }

    fn resolve_path_string(&self, path_str: &str, parent_dir: &Path) -> HoneResult<PathBuf> {
        let path = Path::new(path_str);

        // For virtual files, normalize the path
        let full_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            parent_dir.join(path)
        };

        // Normalize the path (handle .. and .)
        Ok(normalize_path(&full_path))
    }

    fn format_cycle(&self, target: &Path) -> String {
        let mut cycle_parts: Vec<String> = self
            .resolution_stack
            .iter()
            .skip_while(|p| *p != target)
            .map(|p| p.display().to_string())
            .collect();

        cycle_parts.push(target.display().to_string());
        cycle_parts.join(" -> ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_virtual_resolver_simple() {
        let mut resolver = VirtualResolver::new(HashMap::new());
        resolver.add_file(PathBuf::from("/test.hone"), "name: \"test\"");

        let resolved = resolver.resolve("/test.hone").unwrap();
        assert_eq!(resolved.path, PathBuf::from("/test.hone"));
        assert!(resolved.from_path.is_none());
        assert!(resolved.import_paths.is_empty());
    }

    #[test]
    fn test_virtual_resolver_with_from() {
        let mut resolver = VirtualResolver::new(HashMap::new());
        resolver.add_file(PathBuf::from("/base.hone"), "name: \"base\"\nport: 8080");
        resolver.add_file(
            PathBuf::from("/child.hone"),
            "from \"/base.hone\"\nname: \"child\"",
        );

        let resolved = resolver.resolve("/child.hone").unwrap();
        assert_eq!(resolved.from_path, Some(PathBuf::from("/base.hone")));
    }

    #[test]
    fn test_virtual_resolver_with_import() {
        let mut resolver = VirtualResolver::new(HashMap::new());
        resolver.add_file(PathBuf::from("/utils.hone"), "let default_port = 8080");
        resolver.add_file(
            PathBuf::from("/main.hone"),
            "import \"/utils.hone\" as utils\nport: utils.default_port",
        );

        let resolved = resolver.resolve("/main.hone").unwrap();
        assert_eq!(resolved.import_paths, vec![PathBuf::from("/utils.hone")]);
    }

    #[test]
    fn test_virtual_resolver_named_import() {
        let mut resolver = VirtualResolver::new(HashMap::new());
        resolver.add_file(
            PathBuf::from("/utils.hone"),
            "let port = 8080\nlet host = \"localhost\"",
        );
        resolver.add_file(
            PathBuf::from("/main.hone"),
            "import { port, host } from \"/utils.hone\"\nserver_port: port",
        );

        let resolved = resolver.resolve("/main.hone").unwrap();
        assert_eq!(resolved.import_paths, vec![PathBuf::from("/utils.hone")]);
    }

    #[test]
    fn test_circular_import_detection() {
        let mut resolver = VirtualResolver::new(HashMap::new());
        resolver.add_file(PathBuf::from("/a.hone"), "from \"/b.hone\"\na: 1");
        resolver.add_file(PathBuf::from("/b.hone"), "from \"/a.hone\"\nb: 2");

        let result = resolver.resolve("/a.hone");
        assert!(result.is_err());

        let err = result.unwrap_err();
        match err {
            HoneError::CircularImport { chain, .. } => {
                assert!(chain.contains("/a.hone"));
                assert!(chain.contains("/b.hone"));
            }
            _ => panic!("expected CircularImport error, got {:?}", err),
        }
    }

    #[test]
    fn test_circular_import_three_files() {
        let mut resolver = VirtualResolver::new(HashMap::new());
        resolver.add_file(PathBuf::from("/a.hone"), "from \"/b.hone\"\na: 1");
        resolver.add_file(PathBuf::from("/b.hone"), "from \"/c.hone\"\nb: 2");
        resolver.add_file(PathBuf::from("/c.hone"), "from \"/a.hone\"\nc: 3");

        let result = resolver.resolve("/a.hone");
        assert!(result.is_err());

        match result.unwrap_err() {
            HoneError::CircularImport { chain, .. } => {
                assert!(chain.contains("/a.hone"));
            }
            e => panic!("expected CircularImport, got {:?}", e),
        }
    }

    #[test]
    fn test_relative_path_resolution() {
        let mut resolver = VirtualResolver::new(HashMap::new());
        resolver.add_file(PathBuf::from("/project/lib/utils.hone"), "let port = 8080");
        resolver.add_file(
            PathBuf::from("/project/src/main.hone"),
            "import \"../lib/utils.hone\" as utils\nport: utils.port",
        );

        let resolved = resolver.resolve("/project/src/main.hone").unwrap();
        assert_eq!(
            resolved.import_paths,
            vec![PathBuf::from("/project/lib/utils.hone")]
        );
    }

    #[test]
    fn test_topological_order() {
        let mut resolver = VirtualResolver::new(HashMap::new());
        resolver.add_file(PathBuf::from("/base.hone"), "x: 1");
        resolver.add_file(PathBuf::from("/mid.hone"), "from \"/base.hone\"\ny: 2");
        resolver.add_file(PathBuf::from("/top.hone"), "from \"/mid.hone\"\nz: 3");

        resolver.resolve("/top.hone").unwrap();

        let order = resolver.topological_order(Path::new("/top.hone")).unwrap();

        // Should be: base, mid, top (dependencies first)
        assert_eq!(order.len(), 3);
        assert_eq!(order[0].path, PathBuf::from("/base.hone"));
        assert_eq!(order[1].path, PathBuf::from("/mid.hone"));
        assert_eq!(order[2].path, PathBuf::from("/top.hone"));
    }

    #[test]
    fn test_diamond_dependency() {
        // Test diamond pattern: D depends on B and C, both depend on A
        let mut resolver = VirtualResolver::new(HashMap::new());
        resolver.add_file(PathBuf::from("/a.hone"), "a: 1");
        resolver.add_file(PathBuf::from("/b.hone"), "from \"/a.hone\"\nb: 2");
        resolver.add_file(PathBuf::from("/c.hone"), "from \"/a.hone\"\nc: 3");
        resolver.add_file(
            PathBuf::from("/d.hone"),
            "import \"/b.hone\" as b\nimport \"/c.hone\" as c\nd: 4",
        );

        // Should resolve without errors (diamond is not circular)
        let resolved = resolver.resolve("/d.hone").unwrap();
        assert_eq!(resolved.import_paths.len(), 2);
    }

    #[test]
    fn test_file_not_found() {
        let mut resolver = VirtualResolver::new(HashMap::new());
        resolver.add_file(
            PathBuf::from("/main.hone"),
            "from \"/nonexistent.hone\"\nx: 1",
        );

        let result = resolver.resolve("/main.hone");
        assert!(result.is_err());

        match result.unwrap_err() {
            HoneError::ImportNotFound { path, .. } => {
                assert!(path.contains("nonexistent"));
            }
            e => panic!("expected ImportNotFound, got {:?}", e),
        }
    }

    #[test]
    fn test_interpolation_in_path_rejected() {
        let mut resolver = VirtualResolver::new(HashMap::new());
        resolver.add_file(
            PathBuf::from("/main.hone"),
            "let name = \"base\"\nfrom \"./${name}.hone\"\nx: 1",
        );

        let result = resolver.resolve("/main.hone");
        assert!(result.is_err());
        // Should fail because interpolation in import path is not allowed
    }

    #[test]
    fn test_cache_hit() {
        let mut resolver = VirtualResolver::new(HashMap::new());
        resolver.add_file(PathBuf::from("/test.hone"), "x: 1");

        // First resolution
        let first = resolver.resolve("/test.hone").unwrap();
        let first_ptr = first as *const _;

        // Second resolution should return cached
        let second = resolver.resolve("/test.hone").unwrap();
        let second_ptr = second as *const _;

        // Should be the same object
        assert_eq!(first_ptr, second_ptr);
    }

    #[test]
    fn test_multi_document_from() {
        let mut resolver = VirtualResolver::new(HashMap::new());
        resolver.add_file(PathBuf::from("/base-deploy.hone"), "kind: \"Deployment\"");
        resolver.add_file(PathBuf::from("/base-svc.hone"), "kind: \"Service\"");
        resolver.add_file(
            PathBuf::from("/app.hone"),
            r#"
name: "app"
---deployment
from "/base-deploy.hone"
replicas: 3
---service
from "/base-svc.hone"
port: 80
"#,
        );

        let resolved = resolver.resolve("/app.hone").unwrap();
        // Sub-documents' from statements should be tracked as dependencies
        assert!(
            resolved
                .import_paths
                .contains(&PathBuf::from("/base-deploy.hone"))
                || resolved
                    .import_paths
                    .contains(&PathBuf::from("/base-svc.hone"))
                || resolved.from_path == Some(PathBuf::from("/base-deploy.hone"))
        );
    }

    #[test]
    fn test_fixture_builder() {
        let fixture = TestFixtureBuilder::new()
            .add_file("/base.hone", "x: 1")
            .add_file("/child.hone", "from \"/base.hone\"\ny: 2")
            .build();

        let mut resolver = fixture.resolver();
        let resolved = resolver.resolve("/child.hone").unwrap();
        assert_eq!(resolved.from_path, Some(PathBuf::from("/base.hone")));
    }

    #[test]
    fn test_relative_dot_slash_paths() {
        // Reproduces the playground scenario: files keyed with ./ prefixes
        let mut resolver = VirtualResolver::new(HashMap::new());
        resolver.add_file(
            PathBuf::from("./config.hone"),
            "let app_name = \"catapult\"\nlet version = \"1.0.0\"",
        );
        resolver.add_file(
            PathBuf::from("./main.hone"),
            "import \"./config.hone\" as config\nname: config.app_name",
        );

        let resolved = resolver.resolve("./main.hone").unwrap();
        assert_eq!(resolved.import_paths, vec![PathBuf::from("config.hone")]);

        // The imported file should also be resolved
        let config = resolver.get(Path::new("config.hone"));
        assert!(
            config.is_some(),
            "config.hone should be resolved via import"
        );

        // Topological order should work
        let order = resolver
            .topological_order(Path::new("./main.hone"))
            .unwrap();
        assert_eq!(order.len(), 2);
        assert_eq!(order[0].path, PathBuf::from("config.hone"));
        assert_eq!(order[1].path, PathBuf::from("main.hone"));
    }

    #[test]
    fn test_relative_dot_slash_multi_import() {
        // Mirrors the microservices example: main imports config, resources, schemas;
        // schemas also imports config
        let mut resolver = VirtualResolver::new(HashMap::new());
        resolver.add_file(PathBuf::from("./config.hone"), "let port = 8080");
        resolver.add_file(
            PathBuf::from("./resources.hone"),
            "let api = { cpu: \"100m\" }",
        );
        resolver.add_file(
            PathBuf::from("./schemas.hone"),
            "import \"./config.hone\" as config\nschema Stack {\n  ...\n}",
        );
        resolver.add_file(
            PathBuf::from("./main.hone"),
            "import \"./config.hone\" as config\nimport \"./resources.hone\" as res\nimport \"./schemas.hone\" as schemas\nport: config.port",
        );

        let resolved = resolver.resolve("./main.hone");
        assert!(
            resolved.is_ok(),
            "should resolve all imports: {:?}",
            resolved.err()
        );

        let order = resolver
            .topological_order(Path::new("./main.hone"))
            .unwrap();
        assert_eq!(order.len(), 4);
    }
}
