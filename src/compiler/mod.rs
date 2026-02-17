//! Compiler for Hone configuration language
//!
//! The compiler orchestrates the full compilation pipeline:
//! 1. Resolve imports and dependencies
//! 2. Evaluate files in topological order
//! 3. Handle `import` statements (inject exports into scope)
//! 4. Handle `from` inheritance (overlay on parent output)

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;

use crate::errors::{HoneError, HoneResult, Warning};
use crate::evaluator::{merge_values, Evaluator, MergeStrategy, Value};
use crate::lexer::token::SourceLocation;
use crate::parser::ast::{File, ImportKind, PreambleItem};
use crate::resolver::ImportResolver;
use crate::typechecker::{Type, TypeChecker};

/// Result of compiling a single file
#[derive(Debug, Clone)]
pub struct CompiledFile {
    /// The output value (object)
    pub value: Value,
    /// Exported variables (from let bindings)
    pub exports: HashMap<String, Value>,
}

/// Compiler that handles multi-file compilation
pub struct Compiler {
    /// Import resolver
    resolver: ImportResolver,
    /// Cache of compiled files
    compiled: HashMap<PathBuf, CompiledFile>,
    /// CLI args to inject as `args` variable
    args: Option<Value>,
    /// Whether env() and file() are allowed
    allow_env: bool,
    /// Warnings collected during compilation
    warnings: Vec<Warning>,
    /// Variant selections (variant_name -> case_name)
    variants: HashMap<String, String>,
    /// Whether to skip policy checks
    ignore_policies: bool,
}

impl Compiler {
    /// Create a new compiler with the given base directory
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            resolver: ImportResolver::new(base_dir),
            compiled: HashMap::new(),
            args: None,
            allow_env: false,
            warnings: Vec::new(),
            variants: HashMap::new(),
            ignore_policies: false,
        }
    }

    /// Get warnings collected during compilation
    pub fn warnings(&self) -> &[Warning] {
        &self.warnings
    }

    /// Set CLI args to inject into the evaluator scope
    pub fn set_args(&mut self, args: Value) {
        self.args = Some(args);
    }

    /// Set whether env() and file() are allowed
    pub fn set_allow_env(&mut self, allow: bool) {
        self.allow_env = allow;
    }

    /// Set variant selections (variant_name -> case_name)
    pub fn set_variants(&mut self, variants: HashMap<String, String>) {
        self.variants = variants;
    }

    /// Set whether to ignore policy checks
    pub fn set_ignore_policies(&mut self, ignore: bool) {
        self.ignore_policies = ignore;
    }

    /// Compile source code directly (for stdin/inline input)
    /// Imports resolve relative to the compiler's base directory.
    pub fn compile_source(&mut self, source: &str) -> HoneResult<Value> {
        let mut lexer = crate::Lexer::new(source, None);
        let tokens = lexer.tokenize()?;

        let mut parser = crate::Parser::new(tokens, source, None);
        let ast = parser.parse()?;

        let mut evaluator = Evaluator::new(source);
        evaluator.set_allow_env(self.allow_env);
        if !self.variants.is_empty() {
            evaluator.set_variant_selections(self.variants.clone());
        }
        if let Some(ref args) = self.args {
            evaluator.define("args", args.clone());
        }

        let value = evaluator.evaluate(&ast)?;

        // Collect unchecked paths
        let unchecked_paths = evaluator.unchecked_paths().clone();

        // Generate warnings for unchecked paths
        for path in &unchecked_paths {
            self.warnings.push(Warning {
                message: format!("@unchecked used at {}", path),
                file: None,
                line: 0,
                column: 0,
            });
        }

        // Type check against use statements if any (no imports for stdin)
        self.validate_against_schemas(&ast, &value, source, &[], &unchecked_paths)?;

        // Check policies
        if !self.ignore_policies {
            self.check_policies(
                &mut evaluator,
                &ast,
                &value,
                source,
                std::path::Path::new("<stdin>"),
            )?;
        }

        Ok(value)
    }

    /// Compile a file and all its dependencies
    pub fn compile(&mut self, path: impl AsRef<Path>) -> HoneResult<Value> {
        let path = path.as_ref();

        // Resolve all dependencies
        self.resolve_all(path)?;

        // Get topological order (dependencies first)
        let canonical = path.canonicalize().map_err(|e| {
            HoneError::io_error(format!("failed to resolve path {}: {}", path.display(), e))
        })?;

        // Collect paths first to avoid borrow issues
        let order: Vec<PathBuf> = self
            .resolver
            .topological_order(&canonical)?
            .iter()
            .map(|r| r.path.clone())
            .collect();

        // Compile in order
        for file_path in order {
            self.compile_file_by_path(&file_path)?;
        }

        // Return the main file's output
        self.compiled
            .get(&canonical)
            .map(|c| c.value.clone())
            .ok_or_else(|| HoneError::io_error("compilation produced no output".to_string()))
    }

    /// Compile a file and return multiple documents (for `---name` multi-doc output).
    /// Works like `compile` but calls `evaluate_multi` on the root file.
    pub fn compile_multi(
        &mut self,
        path: impl AsRef<Path>,
    ) -> HoneResult<Vec<(Option<String>, Value)>> {
        let path = path.as_ref();

        // Resolve all dependencies
        self.resolve_all(path)?;

        // Get topological order (dependencies first)
        let canonical = path.canonicalize().map_err(|e| {
            HoneError::io_error(format!("failed to resolve path {}: {}", path.display(), e))
        })?;

        // Collect paths first to avoid borrow issues
        let order: Vec<PathBuf> = self
            .resolver
            .topological_order(&canonical)?
            .iter()
            .map(|r| r.path.clone())
            .collect();

        // Compile all dependency files (non-root) first
        for file_path in &order {
            if *file_path != canonical {
                self.compile_file_by_path(file_path)?;
            }
        }

        // For the root file, set up the evaluator and call evaluate_multi
        let resolved = self.resolver.get(&canonical).ok_or_else(|| {
            HoneError::io_error(format!("file not resolved: {}", canonical.display()))
        })?;

        let source = resolved.source.clone();
        let ast = resolved.ast.clone();
        let from_path = resolved.from_path.clone();
        let import_paths = resolved.import_paths.clone();

        // Create evaluator with full configuration
        let mut evaluator = Evaluator::new(&source);
        evaluator.set_allow_env(self.allow_env);
        if !self.variants.is_empty() {
            evaluator.set_variant_selections(self.variants.clone());
        }
        if let Some(ref args) = self.args {
            evaluator.define("args", args.clone());
        }
        self.inject_imports(&mut evaluator, &ast, &import_paths)?;

        // Get base value from `from` if present
        let base_value = if let Some(ref from) = from_path {
            self.compiled.get(from).map(|c| c.value.clone())
        } else {
            None
        };

        // Evaluate as multi-document
        let mut documents = evaluator.evaluate_multi(&ast)?;

        // Merge main document with base if present
        if let Some(base) = base_value {
            if let Some((_, ref mut main_value)) = documents.first_mut() {
                *main_value = merge_values(base, main_value.clone(), MergeStrategy::Normal);
            }
        }

        // Get unchecked paths from evaluator
        let unchecked_paths = evaluator.unchecked_paths().clone();

        // Generate warnings for unchecked paths
        for path_str in &unchecked_paths {
            self.warnings.push(Warning {
                message: format!("type check skipped for '{}' (@unchecked)", path_str),
                file: Some(canonical.clone()),
                line: 0,
                column: 0,
            });
        }

        // Type check the main document against use statements
        if let Some((_, ref main_value)) = documents.first() {
            self.validate_against_schemas(
                &ast,
                main_value,
                &source,
                &import_paths,
                &unchecked_paths,
            )?;
        }

        // Check policies against each document
        if !self.ignore_policies {
            for (_, ref doc_value) in &documents {
                self.check_policies(&mut evaluator, &ast, doc_value, &source, &canonical)?;
            }
        }

        Ok(documents)
    }

    /// Resolve a file and all its dependencies recursively
    fn resolve_all(&mut self, path: &Path) -> HoneResult<()> {
        let resolved = self.resolver.resolve(path)?;

        // Collect dependencies to resolve
        let from_path = resolved.from_path.clone();
        let import_paths = resolved.import_paths.clone();

        // Resolve from dependency
        if let Some(ref from) = from_path {
            self.resolve_all(from)?;
        }

        // Resolve import dependencies
        for import in &import_paths {
            self.resolve_all(import)?;
        }

        Ok(())
    }

    /// Compile a single file by path
    fn compile_file_by_path(&mut self, file_path: &Path) -> HoneResult<()> {
        // Skip if already compiled
        if self.compiled.contains_key(file_path) {
            return Ok(());
        }

        // Get resolved file from cache (must exist since we resolved it)
        let resolved = self.resolver.get(file_path).ok_or_else(|| {
            HoneError::io_error(format!("file not resolved: {}", file_path.display()))
        })?;

        // Extract data we need (to avoid holding borrow)
        let source = resolved.source.clone();
        let ast = resolved.ast.clone();
        let from_path = resolved.from_path.clone();
        let import_paths = resolved.import_paths.clone();

        // Create evaluator
        let mut evaluator = Evaluator::new(&source);
        evaluator.set_allow_env(self.allow_env);
        if !self.variants.is_empty() {
            evaluator.set_variant_selections(self.variants.clone());
        }

        // Inject CLI args if provided
        if let Some(ref args) = self.args {
            evaluator.define("args", args.clone());
        }

        // Inject imports into scope (use already-resolved paths from resolver)
        self.inject_imports(&mut evaluator, &ast, &import_paths)?;

        // Get base value from `from` if present
        let base_value = if let Some(ref from) = from_path {
            self.compiled.get(from).map(|c| c.value.clone())
        } else {
            None
        };

        // Evaluate the file
        let (value, exports) = self.evaluate_with_exports(&mut evaluator, &ast)?;

        // Get unchecked paths from evaluator
        let unchecked_paths = evaluator.unchecked_paths().clone();

        // Merge with base if present
        let final_value = if let Some(base) = base_value {
            merge_values(base, value, MergeStrategy::Normal)
        } else {
            value
        };

        // Generate warnings for unchecked paths
        for path in &unchecked_paths {
            self.warnings.push(Warning {
                message: format!("type check skipped for '{}' (@unchecked)", path),
                file: Some(file_path.to_path_buf()),
                line: 0,
                column: 0,
            });
        }

        // Type check against use statements if any
        self.validate_against_schemas(
            &ast,
            &final_value,
            &source,
            &import_paths,
            &unchecked_paths,
        )?;

        // Check policies
        if !self.ignore_policies {
            self.check_policies(&mut evaluator, &ast, &final_value, &source, file_path)?;
        }

        // Cache result
        self.compiled.insert(
            file_path.to_path_buf(),
            CompiledFile {
                value: final_value,
                exports,
            },
        );

        Ok(())
    }

    /// Inject imported values into the evaluator's scope
    fn inject_imports(
        &self,
        evaluator: &mut Evaluator,
        ast: &File,
        resolved_import_paths: &[PathBuf],
    ) -> HoneResult<()> {
        // Import paths in resolved_import_paths are ordered to match the import
        // statements in the AST preamble (by counting PreambleItem::Import items).
        for (idx, item) in ast.preamble.iter().enumerate() {
            if let PreambleItem::Import(import) = item {
                // Get the resolved path for this import (by index in import_paths)
                let import_path = resolved_import_paths.get(
                    ast.preamble
                        .iter()
                        .take(idx + 1)
                        .filter(|p| matches!(p, PreambleItem::Import(_)))
                        .count()
                        - 1,
                );

                let import_path = match import_path {
                    Some(p) => p,
                    None => continue,
                };

                match &import.kind {
                    ImportKind::Whole { alias, .. } => {
                        if let Some(compiled) = self.compiled.get(import_path) {
                            // Get alias name
                            let alias_name = alias.clone().unwrap_or_else(|| {
                                import_path
                                    .file_stem()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or("imported")
                                    .to_string()
                            });

                            // Create an object containing all exports
                            let mut exports_obj = IndexMap::new();
                            for (name, value) in &compiled.exports {
                                exports_obj.insert(name.clone(), value.clone());
                            }

                            // Also include the output value if it's an object
                            if let Value::Object(ref obj) = compiled.value {
                                for (k, v) in obj {
                                    exports_obj.insert(k.clone(), v.clone());
                                }
                            }

                            evaluator.add_import(&alias_name, Value::Object(exports_obj));
                        }
                    }
                    ImportKind::Named { names, .. } => {
                        if let Some(compiled) = self.compiled.get(import_path) {
                            for name_import in names {
                                let local_name =
                                    name_import.alias.as_ref().unwrap_or(&name_import.name);

                                // Look for the name in exports first, then in output
                                let value = compiled
                                    .exports
                                    .get(&name_import.name)
                                    .cloned()
                                    .or_else(|| {
                                        if let Value::Object(ref obj) = compiled.value {
                                            obj.get(&name_import.name).cloned()
                                        } else {
                                            None
                                        }
                                    })
                                    .unwrap_or(Value::Null);

                                evaluator.define(local_name, value);
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Validate output against schemas specified by `use` statements
    fn validate_against_schemas(
        &self,
        ast: &File,
        value: &Value,
        source: &str,
        import_paths: &[PathBuf],
        unchecked_paths: &std::collections::HashSet<String>,
    ) -> HoneResult<()> {
        // Collect use statements
        let use_statements: Vec<_> = ast
            .preamble
            .iter()
            .filter_map(|item| {
                if let PreambleItem::Use(use_stmt) = item {
                    Some(use_stmt)
                } else {
                    None
                }
            })
            .collect();

        // No use statements means no validation
        if use_statements.is_empty() {
            return Ok(());
        }

        // Create type checker and collect schemas from current file
        let mut checker = TypeChecker::new(source.to_string());
        checker.set_unchecked_paths(unchecked_paths.clone());
        checker.collect_schemas(ast)?;

        // Also collect schemas from all imported files
        for import_path in import_paths {
            if let Some(resolved) = self.resolver.get(import_path) {
                checker.collect_schemas(&resolved.ast)?;
            }
        }

        // Validate against each schema in use statements
        for use_stmt in use_statements {
            let location = SourceLocation {
                file: None,
                line: use_stmt.location.line,
                column: use_stmt.location.column,
                offset: use_stmt.location.offset,
                length: use_stmt.location.length,
            };

            // Check that the schema exists
            if checker.get_schema(&use_stmt.schema_name).is_none() {
                return Err(HoneError::UndefinedVariable {
                    src: source.to_string(),
                    span: (location.offset, location.length).into(),
                    name: use_stmt.schema_name.clone(),
                    help: format!(
                        "define schema '{}' before using it, or import it from another file",
                        use_stmt.schema_name
                    ),
                });
            }

            // Validate the output value against the schema
            checker.check_type(
                value,
                &Type::Schema(use_stmt.schema_name.clone()),
                &location,
            )?;
        }

        Ok(())
    }

    /// Check policy declarations against the output value
    fn check_policies(
        &mut self,
        evaluator: &mut Evaluator,
        ast: &File,
        value: &Value,
        source: &str,
        file_path: &Path,
    ) -> HoneResult<()> {
        use crate::parser::ast::PolicyLevel;

        let policies: Vec<_> = ast
            .preamble
            .iter()
            .filter_map(|item| {
                if let PreambleItem::Policy(p) = item {
                    Some(p.clone())
                } else {
                    None
                }
            })
            .collect();

        if policies.is_empty() {
            return Ok(());
        }

        let violations = evaluator.check_policies(&policies, value)?;

        for (name, level, message) in violations {
            match level {
                PolicyLevel::Deny => {
                    // Find the policy's location for error reporting
                    let policy = policies.iter().find(|p| p.name == name);
                    let loc = policy.map(|p| &p.location);
                    if let Some(loc) = loc {
                        return Err(HoneError::unexpected_token(
                            source,
                            loc,
                            "policy condition to be false",
                            format!("policy '{}' violated", name),
                            &message,
                        ));
                    } else {
                        return Err(HoneError::compilation_error(format!(
                            "policy '{}' violated: {}",
                            name, message
                        )));
                    }
                }
                PolicyLevel::Warn => {
                    let policy = policies.iter().find(|p| p.name == name);
                    let (line, column) = policy
                        .map(|p| (p.location.line, p.location.column))
                        .unwrap_or((0, 0));
                    self.warnings.push(Warning {
                        message: format!("policy '{}': {}", name, message),
                        file: Some(file_path.to_path_buf()),
                        line,
                        column,
                    });
                }
            }
        }

        Ok(())
    }

    /// Evaluate a file and extract both the output value and exports
    fn evaluate_with_exports(
        &self,
        evaluator: &mut Evaluator,
        ast: &File,
    ) -> HoneResult<(Value, HashMap<String, Value>)> {
        // Collect export names from let bindings
        let export_names: Vec<String> = ast
            .preamble
            .iter()
            .filter_map(|item| {
                if let PreambleItem::Let(binding) = item {
                    Some(binding.name.clone())
                } else {
                    None
                }
            })
            .collect();

        // Evaluate the file normally
        let value = evaluator.evaluate(ast)?;

        // Extract exports by looking up the defined variables
        let mut exports = HashMap::new();
        for name in export_names {
            if let Some(val) = evaluator.lookup(&name) {
                exports.insert(name, val);
            }
        }

        Ok((value, exports))
    }
}

/// Convenience function to compile a file
pub fn compile_file(path: impl AsRef<Path>) -> HoneResult<Value> {
    let path = path.as_ref();

    // Canonicalize the path first to get absolute path
    let canonical = path.canonicalize().map_err(|e| {
        HoneError::io_error(format!("failed to resolve path {}: {}", path.display(), e))
    })?;

    let base_dir = canonical.parent().unwrap_or(Path::new("."));
    let mut compiler = Compiler::new(base_dir);
    compiler.compile(&canonical)
}

/// Compile a file with CLI args injected as `args` variable
pub fn compile_file_with_args(path: impl AsRef<Path>, args: Value) -> HoneResult<Value> {
    let path = path.as_ref();

    let canonical = path.canonicalize().map_err(|e| {
        HoneError::io_error(format!("failed to resolve path {}: {}", path.display(), e))
    })?;

    let base_dir = canonical.parent().unwrap_or(Path::new("."));
    let mut compiler = Compiler::new(base_dir);
    compiler.set_args(args);
    compiler.compile(&canonical)
}

/// Infer the type of a string value from CLI --set flags.
///
/// - `"null"` -> `Value::Null`
/// - `"true"` / `"false"` -> `Value::Bool`
/// - Parseable as i64 -> `Value::Int`
/// - Parseable as f64 -> `Value::Float`
/// - Everything else -> `Value::String`
pub fn infer_value(s: &str) -> Value {
    if s == "null" {
        return Value::Null;
    }
    if s == "true" {
        return Value::Bool(true);
    }
    if s == "false" {
        return Value::Bool(false);
    }
    if let Ok(n) = s.parse::<i64>() {
        return Value::Int(n);
    }
    if let Ok(n) = s.parse::<f64>() {
        return Value::Float(n);
    }
    Value::String(s.to_string())
}

/// Set a nested value in an object using a dotted key path.
///
/// `set_nested(obj, "server.port", value)` creates `obj.server.port = value`,
/// creating intermediate objects as needed.
fn set_nested(obj: &mut IndexMap<String, Value>, key: &str, value: Value) {
    let parts: Vec<&str> = key.split('.').collect();
    if parts.len() == 1 {
        obj.insert(key.to_string(), value);
        return;
    }

    // Navigate/create intermediate objects
    let mut current = obj;
    for part in &parts[..parts.len() - 1] {
        // Ensure an object exists at this key
        if !current.contains_key(*part) || !matches!(current.get(*part), Some(Value::Object(_))) {
            current.insert(part.to_string(), Value::Object(IndexMap::new()));
        }
        current = match current.get_mut(*part) {
            Some(Value::Object(inner)) => inner,
            _ => unreachable!(),
        };
    }

    let last = parts.last().unwrap();
    current.insert(last.to_string(), value);
}

/// Build an args object from CLI --set, --set-file, and --set-string flags.
pub fn build_args_object(
    set: &[(String, String)],
    set_file: &[(String, String)],
    set_string: &[(String, String)],
) -> HoneResult<Value> {
    let mut obj = IndexMap::new();

    // --set: type inference
    for (key, val) in set {
        set_nested(&mut obj, key, infer_value(val));
    }

    // --set-file: read file contents as string
    for (key, path) in set_file {
        let contents = std::fs::read_to_string(path)
            .map_err(|e| HoneError::io_error(format!("--set-file {}: {}", path, e)))?;
        set_nested(&mut obj, key, Value::String(contents));
    }

    // --set-string: forced string (no type inference)
    for (key, val) in set_string {
        set_nested(&mut obj, key, Value::String(val.clone()));
    }

    Ok(Value::Object(obj))
}

/// Validate a compiled value against a named schema from the source file.
///
/// Re-parses the file to collect schemas and type aliases, then validates.
pub fn validate_against_schema(
    path: impl AsRef<Path>,
    value: &Value,
    schema_name: &str,
) -> HoneResult<()> {
    let path = path.as_ref();
    let source = std::fs::read_to_string(path)
        .map_err(|e| HoneError::io_error(format!("failed to read {}: {}", path.display(), e)))?;

    let mut lexer = crate::lexer::Lexer::new(&source, Some(path.to_path_buf()));
    let tokens = lexer.tokenize()?;

    let mut parser = crate::parser::Parser::new(tokens, &source, Some(path.to_path_buf()));
    let ast = parser.parse()?;

    let mut checker = TypeChecker::new(source.clone());
    checker.collect_schemas(&ast)?;

    if checker.get_schema(schema_name).is_none() {
        return Err(HoneError::UndefinedVariable {
            src: source,
            span: (0, 1).into(),
            name: schema_name.to_string(),
            help: format!(
                "schema '{}' is not defined in {}",
                schema_name,
                path.display()
            ),
        });
    }

    let location = SourceLocation {
        file: Some(path.to_path_buf()),
        line: 1,
        column: 1,
        offset: 0,
        length: 1,
    };

    checker.check_type(value, &Type::Schema(schema_name.to_string()), &location)
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
    fn test_compile_single_file() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
let name = "test"
greeting: "Hello, ${name}!"
"#,
            )],
        );

        let result = compile_file(dir.path().join("main.hone")).unwrap();

        if let Value::Object(obj) = result {
            assert_eq!(
                obj.get("greeting"),
                Some(&Value::String("Hello, test!".into()))
            );
        } else {
            panic!("Expected object");
        }
    }

    #[test]
    fn test_compile_with_import() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[
                (
                    "config.hone",
                    r#"
let version = "1.0.0"
let port = 8080
"#,
                ),
                (
                    "main.hone",
                    r#"
import "./config.hone" as config

app {
    version: config.version
    port: config.port
}
"#,
                ),
            ],
        );

        let result = compile_file(dir.path().join("main.hone")).unwrap();

        if let Value::Object(obj) = result {
            if let Some(Value::Object(app)) = obj.get("app") {
                assert_eq!(app.get("version"), Some(&Value::String("1.0.0".into())));
                assert_eq!(app.get("port"), Some(&Value::Int(8080)));
            } else {
                panic!("Expected app object");
            }
        } else {
            panic!("Expected object");
        }
    }

    #[test]
    fn test_compile_with_named_import() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[
                (
                    "config.hone",
                    r#"
let version = "1.0.0"
let port = 8080
let unused = "ignored"
"#,
                ),
                (
                    "main.hone",
                    r#"
import { version, port } from "./config.hone"

app {
    version: version
    port: port
}
"#,
                ),
            ],
        );

        let result = compile_file(dir.path().join("main.hone")).unwrap();

        if let Value::Object(obj) = result {
            if let Some(Value::Object(app)) = obj.get("app") {
                assert_eq!(app.get("version"), Some(&Value::String("1.0.0".into())));
                assert_eq!(app.get("port"), Some(&Value::Int(8080)));
            } else {
                panic!("Expected app object");
            }
        } else {
            panic!("Expected object");
        }
    }

    #[test]
    fn test_compile_with_from_inheritance() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[
                (
                    "base.hone",
                    r#"
server {
    host: "localhost"
    port: 8080
}
"#,
                ),
                (
                    "prod.hone",
                    r#"
from "./base.hone"

server {
    host: "prod.example.com"
}
"#,
                ),
            ],
        );

        let result = compile_file(dir.path().join("prod.hone")).unwrap();

        if let Value::Object(obj) = result {
            if let Some(Value::Object(server)) = obj.get("server") {
                // host should be overridden
                assert_eq!(
                    server.get("host"),
                    Some(&Value::String("prod.example.com".into()))
                );
                // port should be inherited
                assert_eq!(server.get("port"), Some(&Value::Int(8080)));
            } else {
                panic!("Expected server object");
            }
        } else {
            panic!("Expected object");
        }
    }

    #[test]
    fn test_compile_chain_of_imports() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[
                (
                    "a.hone",
                    r#"
let a_val = "from_a"
"#,
                ),
                (
                    "b.hone",
                    r#"
import "./a.hone" as a
let b_val = a.a_val
"#,
                ),
                (
                    "c.hone",
                    r#"
import "./b.hone" as b
result: b.b_val
"#,
                ),
            ],
        );

        let result = compile_file(dir.path().join("c.hone")).unwrap();

        if let Value::Object(obj) = result {
            assert_eq!(obj.get("result"), Some(&Value::String("from_a".into())));
        } else {
            panic!("Expected object");
        }
    }

    #[test]
    fn test_schema_validation_passes() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
schema Server {
    host: string
    port: int
    debug?: bool
}

use Server

host: "localhost"
port: 8080
"#,
            )],
        );

        let result = compile_file(dir.path().join("main.hone"));
        assert!(
            result.is_ok(),
            "Schema validation should pass for valid data"
        );

        if let Value::Object(obj) = result.unwrap() {
            assert_eq!(obj.get("host"), Some(&Value::String("localhost".into())));
            assert_eq!(obj.get("port"), Some(&Value::Int(8080)));
        } else {
            panic!("Expected object");
        }
    }

    #[test]
    fn test_schema_validation_type_mismatch() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
schema Server {
    host: string
    port: int
}

use Server

host: "localhost"
port: "8080"
"#,
            )],
        );

        let result = compile_file(dir.path().join("main.hone"));
        assert!(
            result.is_err(),
            "Schema validation should fail for type mismatch"
        );

        let err = result.unwrap_err();
        assert!(matches!(err, HoneError::TypeMismatch { .. }));
    }

    #[test]
    fn test_schema_validation_missing_field() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
schema Server {
    host: string
    port: int
}

use Server

host: "localhost"
"#,
            )],
        );

        let result = compile_file(dir.path().join("main.hone"));
        assert!(
            result.is_err(),
            "Schema validation should fail for missing field"
        );

        let err = result.unwrap_err();
        assert!(matches!(err, HoneError::MissingField { .. }));
    }

    #[test]
    fn test_schema_validation_optional_field() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
schema Config {
    name: string
    timeout?: int
}

use Config

name: "test"
"#,
            )],
        );

        let result = compile_file(dir.path().join("main.hone"));
        assert!(result.is_ok(), "Optional field should not be required");
    }

    #[test]
    fn test_int_constraint_valid() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
schema Config {
    port: int(1, 65535)
}

use Config

port: 8080
"#,
            )],
        );

        let result = compile_file(dir.path().join("main.hone"));
        assert!(result.is_ok(), "Valid port within range should pass");
    }

    #[test]
    fn test_int_constraint_too_high() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
schema Config {
    port: int(1, 65535)
}

use Config

port: 99999
"#,
            )],
        );

        let result = compile_file(dir.path().join("main.hone"));
        assert!(result.is_err(), "Port above max should fail");

        let err = result.unwrap_err();
        assert!(matches!(err, HoneError::ValueOutOfRange { .. }));
    }

    #[test]
    fn test_int_constraint_too_low() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
schema Config {
    port: int(1, 65535)
}

use Config

port: 0
"#,
            )],
        );

        let result = compile_file(dir.path().join("main.hone"));
        assert!(result.is_err(), "Port below min should fail");

        let err = result.unwrap_err();
        assert!(matches!(err, HoneError::ValueOutOfRange { .. }));
    }

    #[test]
    fn test_string_length_constraint_valid() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
schema Config {
    name: string(1, 50)
}

use Config

name: "myapp"
"#,
            )],
        );

        let result = compile_file(dir.path().join("main.hone"));
        assert!(result.is_ok(), "Valid string length should pass");
    }

    #[test]
    fn test_string_length_constraint_too_short() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
schema Config {
    name: string(1, 50)
}

use Config

name: ""
"#,
            )],
        );

        let result = compile_file(dir.path().join("main.hone"));
        assert!(result.is_err(), "Empty string should fail min length check");

        let err = result.unwrap_err();
        assert!(matches!(err, HoneError::ValueOutOfRange { .. }));
    }

    #[test]
    fn test_nested_schema_with_constraints() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
schema Server {
    host: string
    port: int(1, 65535)
}

schema Config {
    server: Server
}

use Config

server {
    host: "localhost"
    port: 8080
}
"#,
            )],
        );

        let result = compile_file(dir.path().join("main.hone"));
        assert!(
            result.is_ok(),
            "Nested schema with valid values should pass"
        );
    }

    #[test]
    fn test_nested_schema_constraint_violation() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
schema Server {
    host: string
    port: int(1, 65535)
}

schema Config {
    server: Server
}

use Config

server {
    host: "localhost"
    port: 99999
}
"#,
            )],
        );

        let result = compile_file(dir.path().join("main.hone"));
        assert!(
            result.is_err(),
            "Nested schema with invalid port should fail"
        );

        let err = result.unwrap_err();
        assert!(matches!(err, HoneError::ValueOutOfRange { .. }));
    }

    #[test]
    fn test_type_alias_valid() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
type Port = int(1, 65535)

schema Config {
    port: Port
}

use Config

port: 8080
"#,
            )],
        );

        let result = compile_file(dir.path().join("main.hone"));
        assert!(result.is_ok(), "Type alias with valid value should pass");
    }

    #[test]
    fn test_type_alias_constraint_violation() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
type Port = int(1, 65535)

schema Config {
    port: Port
}

use Config

port: 99999
"#,
            )],
        );

        let result = compile_file(dir.path().join("main.hone"));
        assert!(result.is_err(), "Type alias with invalid value should fail");

        let err = result.unwrap_err();
        assert!(matches!(err, HoneError::ValueOutOfRange { .. }));
    }

    #[test]
    fn test_type_alias_string_constraints() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
type Name = string(1, 50)

schema Config {
    name: Name
}

use Config

name: "myapp"
"#,
            )],
        );

        let result = compile_file(dir.path().join("main.hone"));
        assert!(result.is_ok(), "Type alias with valid string should pass");
    }

    #[test]
    fn test_type_alias_string_constraint_violation() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
type Name = string(1, 50)

schema Config {
    name: Name
}

use Config

name: ""
"#,
            )],
        );

        let result = compile_file(dir.path().join("main.hone"));
        assert!(
            result.is_err(),
            "Type alias with empty string should fail minLen"
        );

        let err = result.unwrap_err();
        assert!(matches!(err, HoneError::ValueOutOfRange { .. }));
    }

    #[test]
    fn test_multiple_type_aliases() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
type Port = int(1, 65535)
type Name = string(1, 100)

schema ServerConfig {
    name: Name
    port: Port
}

use ServerConfig

name: "api-server"
port: 8080
"#,
            )],
        );

        let result = compile_file(dir.path().join("main.hone"));
        assert!(result.is_ok(), "Multiple type aliases should work together");
    }

    #[test]
    fn test_infer_value() {
        assert_eq!(infer_value("null"), Value::Null);
        assert_eq!(infer_value("true"), Value::Bool(true));
        assert_eq!(infer_value("false"), Value::Bool(false));
        assert_eq!(infer_value("42"), Value::Int(42));
        assert_eq!(infer_value("-17"), Value::Int(-17));
        assert_eq!(infer_value("0"), Value::Int(0));
        assert_eq!(infer_value("3.14"), Value::Float(3.14));
        assert_eq!(infer_value("hello"), Value::String("hello".into()));
        assert_eq!(infer_value(""), Value::String("".into()));
        assert_eq!(
            infer_value("not-a-number"),
            Value::String("not-a-number".into())
        );
    }

    #[test]
    fn test_build_args_object_basic() {
        let set = vec![
            ("env".to_string(), "prod".to_string()),
            ("port".to_string(), "8080".to_string()),
            ("debug".to_string(), "true".to_string()),
        ];
        let args = build_args_object(&set, &[], &[]).unwrap();

        if let Value::Object(obj) = args {
            assert_eq!(obj.get("env"), Some(&Value::String("prod".into())));
            assert_eq!(obj.get("port"), Some(&Value::Int(8080)));
            assert_eq!(obj.get("debug"), Some(&Value::Bool(true)));
        } else {
            panic!("Expected object");
        }
    }

    #[test]
    fn test_build_args_object_dotted_keys() {
        let set = vec![
            ("server.port".to_string(), "8080".to_string()),
            ("server.host".to_string(), "localhost".to_string()),
            ("db.name".to_string(), "mydb".to_string()),
        ];
        let args = build_args_object(&set, &[], &[]).unwrap();

        assert_eq!(args.get_path(&["server", "port"]), Some(&Value::Int(8080)));
        assert_eq!(
            args.get_path(&["server", "host"]),
            Some(&Value::String("localhost".into()))
        );
        assert_eq!(
            args.get_path(&["db", "name"]),
            Some(&Value::String("mydb".into()))
        );
    }

    #[test]
    fn test_build_args_object_set_string() {
        let set_string = vec![
            ("port".to_string(), "8080".to_string()),
            ("flag".to_string(), "true".to_string()),
        ];
        let args = build_args_object(&[], &[], &set_string).unwrap();

        if let Value::Object(obj) = args {
            // --set-string forces string type, no inference
            assert_eq!(obj.get("port"), Some(&Value::String("8080".into())));
            assert_eq!(obj.get("flag"), Some(&Value::String("true".into())));
        } else {
            panic!("Expected object");
        }
    }

    #[test]
    fn test_build_args_object_set_file() {
        let dir = TempDir::new().unwrap();
        let data_path = dir.path().join("data.txt");
        fs::write(&data_path, "file contents here").unwrap();

        let set_file = vec![(
            "config".to_string(),
            data_path.to_string_lossy().to_string(),
        )];
        let args = build_args_object(&[], &set_file, &[]).unwrap();

        if let Value::Object(obj) = args {
            assert_eq!(
                obj.get("config"),
                Some(&Value::String("file contents here".into()))
            );
        } else {
            panic!("Expected object");
        }
    }

    #[test]
    fn test_compile_with_args() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
env: args.env
port: args.port
"#,
            )],
        );

        let set = vec![
            ("env".to_string(), "prod".to_string()),
            ("port".to_string(), "8080".to_string()),
        ];
        let args = build_args_object(&set, &[], &[]).unwrap();
        let result = compile_file_with_args(dir.path().join("main.hone"), args).unwrap();

        if let Value::Object(obj) = result {
            assert_eq!(obj.get("env"), Some(&Value::String("prod".into())));
            assert_eq!(obj.get("port"), Some(&Value::Int(8080)));
        } else {
            panic!("Expected object");
        }
    }

    #[test]
    fn test_args_undefined_without_set() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
value: args.x
"#,
            )],
        );

        let result = compile_file(dir.path().join("main.hone"));
        assert!(result.is_err(), "Accessing args without --set should fail");
        let err = result.unwrap_err();
        assert!(matches!(err, HoneError::UndefinedVariable { .. }));
    }

    #[test]
    fn test_compile_with_nested_args() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
host: args.server.host
port: args.server.port
"#,
            )],
        );

        let set = vec![
            ("server.host".to_string(), "example.com".to_string()),
            ("server.port".to_string(), "443".to_string()),
        ];
        let args = build_args_object(&set, &[], &[]).unwrap();
        let result = compile_file_with_args(dir.path().join("main.hone"), args).unwrap();

        if let Value::Object(obj) = result {
            assert_eq!(obj.get("host"), Some(&Value::String("example.com".into())));
            assert_eq!(obj.get("port"), Some(&Value::Int(443)));
        } else {
            panic!("Expected object");
        }
    }

    #[test]
    fn test_validate_against_schema_pass() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
schema Server {
    host: string
    port: int
}

host: "localhost"
port: 8080
"#,
            )],
        );

        let value = compile_file(dir.path().join("main.hone")).unwrap();
        let result = validate_against_schema(dir.path().join("main.hone"), &value, "Server");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_against_schema_fail() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
schema Server {
    host: string
    port: int
}

host: "localhost"
port: "not-a-number"
"#,
            )],
        );

        let value = compile_file(dir.path().join("main.hone")).unwrap();
        let result = validate_against_schema(dir.path().join("main.hone"), &value, "Server");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_against_unknown_schema() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
host: "localhost"
"#,
            )],
        );

        let value = compile_file(dir.path().join("main.hone")).unwrap();
        let result = validate_against_schema(dir.path().join("main.hone"), &value, "NonExistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_unchecked_suppresses_type_error() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
schema Config {
    port: int(1, 65535)
}

use Config

port: 99999 @unchecked
"#,
            )],
        );

        let result = compile_file(dir.path().join("main.hone"));
        assert!(
            result.is_ok(),
            "unchecked should suppress constraint violation"
        );
    }

    #[test]
    fn test_unchecked_emits_warning() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
schema Config {
    port: int(1, 65535)
}

use Config

port: 99999 @unchecked
"#,
            )],
        );

        let canonical = dir.path().join("main.hone").canonicalize().unwrap();
        let base_dir = canonical.parent().unwrap();
        let mut compiler = Compiler::new(base_dir);
        let _value = compiler.compile(&canonical).unwrap();

        let warnings = compiler.warnings();
        assert!(!warnings.is_empty(), "should emit warning for @unchecked");
        assert!(
            warnings[0].message.contains("port"),
            "warning should mention the field name"
        );
    }

    #[test]
    fn test_unchecked_non_annotated_still_fails() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
schema Config {
    port: int(1, 65535)
    host: string
}

use Config

port: 99999 @unchecked
host: 42
"#,
            )],
        );

        let result = compile_file(dir.path().join("main.hone"));
        assert!(
            result.is_err(),
            "non-annotated field should still fail type check"
        );
    }

    #[test]
    fn test_unchecked_nested_schema() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
schema Server {
    port: int(1, 65535)
}

schema Config {
    server: Server
}

use Config

server {
    port: 99999 @unchecked
}
"#,
            )],
        );

        let result = compile_file(dir.path().join("main.hone"));
        assert!(result.is_ok(), "unchecked should work with nested schemas");
    }

    #[test]
    fn test_string_length_counts_characters_not_bytes() {
        let dir = TempDir::new().unwrap();
        // "cafe\u{0301}" is 5 bytes but 4 characters (e + combining accent = 2 bytes)
        // Emoji are 4 bytes each but 1 character
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                "
schema Config {
    label: string(1, 4)
}

use Config

label: \"\u{1F600}\u{1F601}\u{1F602}\u{1F603}\"
",
            )],
        );

        let result = compile_file(dir.path().join("main.hone"));
        assert!(
            result.is_ok(),
            "4 emoji chars should pass string(1,4): {:?}",
            result.err()
        );
    }

    #[test]
    fn test_string_length_rejects_by_char_count() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                "
schema Config {
    label: string(1, 3)
}

use Config

label: \"\u{1F600}\u{1F601}\u{1F602}\u{1F603}\"
",
            )],
        );

        let result = compile_file(dir.path().join("main.hone"));
        assert!(result.is_err(), "4 emoji chars should fail string(1,3)");
    }

    #[test]
    fn test_regex_pattern_match_valid() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
schema Config {
    name: string("^[a-z][a-z0-9-]*$")
}

use Config

name: "my-app-123"
"#,
            )],
        );

        let result = compile_file(dir.path().join("main.hone"));
        assert!(
            result.is_ok(),
            "valid pattern should pass: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_regex_pattern_match_invalid() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
schema Config {
    name: string("^[a-z][a-z0-9-]*$")
}

use Config

name: "INVALID"
"#,
            )],
        );

        let result = compile_file(dir.path().join("main.hone"));
        assert!(result.is_err(), "INVALID should fail pattern match");
        let err = result.unwrap_err();
        assert!(matches!(err, HoneError::PatternMismatch { .. }));
    }

    #[test]
    fn test_regex_pattern_in_type_alias() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
type K8sName = string("^[a-z][a-z0-9-]*$")

schema Config {
    name: K8sName
}

use Config

name: "my-app"
"#,
            )],
        );

        let result = compile_file(dir.path().join("main.hone"));
        assert!(
            result.is_ok(),
            "type alias with pattern should work: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_regex_invalid_pattern_compile_error() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
schema Config {
    name: string("[unclosed")
}

use Config

name: "test"
"#,
            )],
        );

        let result = compile_file(dir.path().join("main.hone"));
        assert!(result.is_err(), "invalid regex should fail at compile time");
        let err = result.unwrap_err();
        let msg = format!("{:?}", err);
        assert!(
            msg.contains("regex") || msg.contains("invalid"),
            "error should mention regex: {}",
            msg
        );
    }

    #[test]
    fn test_schema_closed_rejects_extra_field() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
schema Config {
    port: int
}

use Config

port: 8080
extra: "not allowed"
"#,
            )],
        );

        let result = compile_file(dir.path().join("main.hone"));
        assert!(result.is_err(), "closed schema should reject extra fields");
        let err = result.unwrap_err();
        assert!(matches!(err, HoneError::UnknownField { .. }));
    }

    #[test]
    fn test_schema_open_allows_extra_field() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
schema Config {
    port: int
    ...
}

use Config

port: 8080
extra: "allowed"
"#,
            )],
        );

        let result = compile_file(dir.path().join("main.hone"));
        assert!(
            result.is_ok(),
            "open schema should allow extra fields: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_schema_open_still_validates_defined_fields() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
schema Config {
    port: int
    ...
}

use Config

port: "not an int"
extra: "allowed"
"#,
            )],
        );

        let result = compile_file(dir.path().join("main.hone"));
        assert!(
            result.is_err(),
            "open schema should still validate defined fields"
        );
    }
}
