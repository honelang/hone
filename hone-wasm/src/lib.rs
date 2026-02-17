use std::collections::HashMap;
use std::path::PathBuf;
use wasm_bindgen::prelude::*;

use hone::ast::{ImportKind, PreambleItem};
use hone::evaluator::{merge_values, MergeStrategy};
use hone::lexer::token::SourceLocation;
use hone::{
    emit, infer_value, Evaluator, Lexer, OutputFormat, Parser, Type, TypeChecker, Value,
    VirtualResolver,
};
use indexmap::IndexMap;

#[wasm_bindgen]
pub struct CompileResult {
    output: String,
    error: String,
    success: bool,
}

#[wasm_bindgen]
impl CompileResult {
    #[wasm_bindgen(getter)]
    pub fn output(&self) -> String {
        self.output.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn error(&self) -> String {
        self.error.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn success(&self) -> bool {
        self.success
    }
}

/// Compile Hone source to JSON or YAML.
///
/// - `source`: Hone source code
/// - `format`: "json" or "yaml"
/// - `variant_json`: JSON object of variant selections, e.g. `{"env": "production"}`
/// - `args_json`: JSON object of args, e.g. `{"port": "8080", "env": "prod"}`
#[wasm_bindgen]
pub fn compile(source: &str, format: &str, variant_json: &str, args_json: &str) -> CompileResult {
    let output_format = match format {
        "yaml" | "YAML" => OutputFormat::Yaml,
        "toml" | "TOML" => OutputFormat::Toml,
        "dotenv" | "env" => OutputFormat::Dotenv,
        "json-pretty" => OutputFormat::JsonPretty,
        _ => OutputFormat::Json,
    };

    // Parse variant selections from JSON
    let variants: HashMap<String, String> = if variant_json.is_empty() {
        HashMap::new()
    } else {
        serde_json::from_str(variant_json).unwrap_or_default()
    };

    // Parse args from JSON
    let args: Option<Value> = if args_json.is_empty() {
        None
    } else {
        let raw: HashMap<String, String> = serde_json::from_str(args_json).unwrap_or_default();
        if raw.is_empty() {
            None
        } else {
            let mut obj = IndexMap::new();
            for (key, val) in &raw {
                obj.insert(key.clone(), infer_value(val));
            }
            Some(Value::Object(obj))
        }
    };

    // Lex
    let mut lexer = Lexer::new(source, None);
    let tokens = match lexer.tokenize() {
        Ok(t) => t,
        Err(e) => {
            return CompileResult {
                output: String::new(),
                error: e.message(),
                success: false,
            };
        }
    };

    // Parse
    let mut parser = Parser::new(tokens, source, None);
    let ast = match parser.parse() {
        Ok(a) => a,
        Err(e) => {
            return CompileResult {
                output: String::new(),
                error: e.message(),
                success: false,
            };
        }
    };

    // Evaluate
    let mut evaluator = Evaluator::new(source);
    if !variants.is_empty() {
        evaluator.set_variant_selections(variants);
    }
    if let Some(ref args_val) = args {
        evaluator.define("args", args_val.clone());
    }
    let value = match evaluator.evaluate(&ast) {
        Ok(v) => v,
        Err(e) => {
            return CompileResult {
                output: String::new(),
                error: e.message(),
                success: false,
            };
        }
    };

    // Schema validation (same as Compiler::compile_source)
    let unchecked_paths = evaluator.unchecked_paths().clone();
    if let Err(e) = validate_schemas(&ast, &value, source, &unchecked_paths) {
        return CompileResult {
            output: String::new(),
            error: e.message(),
            success: false,
        };
    }

    // Emit
    match emit(&value, output_format) {
        Ok(output) => CompileResult {
            output,
            error: String::new(),
            success: true,
        },
        Err(e) => CompileResult {
            output: String::new(),
            error: e.message(),
            success: false,
        },
    }
}

/// Validate output against `use` schemas in the AST.
fn validate_schemas(
    ast: &hone::ast::File,
    value: &Value,
    source: &str,
    unchecked_paths: &std::collections::HashSet<String>,
) -> hone::HoneResult<()> {
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

    if use_statements.is_empty() {
        return Ok(());
    }

    let mut checker = TypeChecker::new(source.to_string());
    checker.set_unchecked_paths(unchecked_paths.clone());
    checker.collect_schemas(ast)?;

    for use_stmt in use_statements {
        let location = SourceLocation {
            file: None,
            line: use_stmt.location.line,
            column: use_stmt.location.column,
            offset: use_stmt.location.offset,
            length: use_stmt.location.length,
        };

        if checker.get_schema(&use_stmt.schema_name).is_none() {
            return Err(hone::HoneError::UndefinedVariable {
                src: source.to_string(),
                span: (location.offset, location.length).into(),
                name: use_stmt.schema_name.clone(),
                help: format!("define schema '{}' before using it", use_stmt.schema_name),
            });
        }

        checker.check_type(
            value,
            &Type::Schema(use_stmt.schema_name.clone()),
            &location,
        )?;
    }

    Ok(())
}

/// Compile a multi-file Hone project using virtual (in-memory) files.
///
/// - `files_json`: JSON object mapping filenames to source, e.g. `{"./main.hone": "...", "./config.hone": "..."}`
/// - `entry_point`: the entry file path, e.g. `"./main.hone"`
/// - `format`: output format ("json", "yaml", "toml", "dotenv", "json-pretty")
/// - `variant_json`: JSON object of variant selections
/// - `args_json`: JSON object of args
#[wasm_bindgen]
pub fn compile_project(
    files_json: &str,
    entry_point: &str,
    format: &str,
    variant_json: &str,
    args_json: &str,
) -> CompileResult {
    match compile_project_inner(files_json, entry_point, format, variant_json, args_json) {
        Ok(output) => CompileResult {
            output,
            error: String::new(),
            success: true,
        },
        Err(e) => CompileResult {
            output: String::new(),
            error: e,
            success: false,
        },
    }
}

fn compile_project_inner(
    files_json: &str,
    entry_point: &str,
    format: &str,
    variant_json: &str,
    args_json: &str,
) -> Result<String, String> {
    let output_format = match format {
        "yaml" | "YAML" => OutputFormat::Yaml,
        "toml" | "TOML" => OutputFormat::Toml,
        "dotenv" | "env" => OutputFormat::Dotenv,
        "json-pretty" => OutputFormat::JsonPretty,
        _ => OutputFormat::Json,
    };

    // Parse variant selections
    let variants: HashMap<String, String> = if variant_json.is_empty() {
        HashMap::new()
    } else {
        serde_json::from_str(variant_json).unwrap_or_default()
    };

    // Parse args
    let args: Option<Value> = if args_json.is_empty() {
        None
    } else {
        let raw: HashMap<String, String> = serde_json::from_str(args_json).unwrap_or_default();
        if raw.is_empty() {
            None
        } else {
            let mut obj = IndexMap::new();
            for (key, val) in &raw {
                obj.insert(key.clone(), infer_value(val));
            }
            Some(Value::Object(obj))
        }
    };

    // Build virtual file map
    let files_map: HashMap<String, String> =
        serde_json::from_str(files_json).map_err(|e| format!("invalid files_json: {}", e))?;

    let mut virtual_files: HashMap<PathBuf, String> = HashMap::new();
    for (name, source) in &files_map {
        virtual_files.insert(PathBuf::from(name), source.clone());
    }

    // Create resolver and resolve entry point (recursively resolves imports)
    let mut resolver = VirtualResolver::new(virtual_files);
    let entry_path = PathBuf::from(entry_point);
    resolver.resolve(&entry_path).map_err(|e| e.message())?;

    // Get topological order — use resolved paths (normalized by VirtualResolver)
    let topo_files = resolver
        .topological_order(&entry_path)
        .map_err(|e| e.message())?;
    // The last entry in topological order is the entry point (dependencies come first)
    let entry_path_normalized = topo_files
        .last()
        .map(|r| r.path.clone())
        .ok_or_else(|| "no files resolved".to_string())?;
    let order: Vec<PathBuf> = topo_files.iter().map(|r| r.path.clone()).collect();

    // Compile each file in topological order
    // Store compiled results: (output value, exports map)
    let mut compiled: HashMap<PathBuf, (Value, HashMap<String, Value>)> = HashMap::new();

    for file_path in &order {
        let resolved = resolver
            .get(file_path)
            .ok_or_else(|| format!("file not resolved: {}", file_path.display()))?;

        let source = resolved.source.clone();
        let ast = resolved.ast.clone();
        let from_path = resolved.from_path.clone();
        let import_paths = resolved.import_paths.clone();

        // Create evaluator
        let mut evaluator = Evaluator::new(&source);
        if !variants.is_empty() {
            evaluator.set_variant_selections(variants.clone());
        }
        if let Some(ref args_val) = args {
            evaluator.define("args", args_val.clone());
        }

        // Inject imports from already-compiled files
        inject_imports_virtual(&mut evaluator, &ast, &import_paths, &compiled);

        // Get base value from `from` if present
        let base_value = from_path
            .as_ref()
            .and_then(|p| compiled.get(p))
            .map(|(v, _)| v.clone());

        // Evaluate and extract exports
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

        // For the entry point, use evaluate_multi to handle ---name documents
        let is_entry = *file_path == entry_path_normalized;
        let has_documents = !ast.documents.is_empty();

        if is_entry && has_documents {
            // Multi-document entry point: evaluate_multi and emit all docs
            let mut documents = evaluator.evaluate_multi(&ast).map_err(|e| e.message())?;

            // Merge main document with base if present
            if let Some(base) = base_value {
                if let Some((_, ref mut main_value)) = documents.first_mut() {
                    *main_value = merge_values(base, main_value.clone(), MergeStrategy::Normal);
                }
            }

            // Schema validation on main document
            let unchecked_paths = evaluator.unchecked_paths().clone();
            if let Some((_, ref main_value)) = documents.first() {
                validate_schemas_with_imports(
                    &ast,
                    main_value,
                    &source,
                    &import_paths,
                    &resolver,
                    &unchecked_paths,
                )
                .map_err(|e| e.message())?;
            }

            // Emit all non-empty documents
            let mut parts = Vec::new();
            for (name, value) in &documents {
                if name.is_none() && value.is_empty_object() {
                    continue;
                }
                let emitted = emit(value, output_format).map_err(|e| e.message())?;
                if let Some(doc_name) = name {
                    match output_format {
                        OutputFormat::Yaml => {
                            parts.push(format!("# {}\n{}", doc_name, emitted));
                        }
                        _ => {
                            parts.push(format!("// {}\n{}", doc_name, emitted));
                        }
                    }
                } else {
                    parts.push(emitted);
                }
            }

            let separator = match output_format {
                OutputFormat::Yaml => "\n---\n",
                _ => "\n\n",
            };
            return Ok(parts.join(separator));
        }

        let value = evaluator.evaluate(&ast).map_err(|e| e.message())?;

        let mut exports = HashMap::new();
        for name in export_names {
            if let Some(val) = evaluator.lookup(&name) {
                exports.insert(name, val);
            }
        }

        // Schema validation
        let unchecked_paths = evaluator.unchecked_paths().clone();
        validate_schemas_with_imports(
            &ast,
            &value,
            &source,
            &import_paths,
            &resolver,
            &unchecked_paths,
        )
        .map_err(|e| e.message())?;

        // Merge with base if present
        let final_value = if let Some(base) = base_value {
            merge_values(base, value, MergeStrategy::Normal)
        } else {
            value
        };

        compiled.insert(file_path.clone(), (final_value, exports));
    }

    // Get the entry point's output (use normalized path)
    let (value, _) = compiled
        .get(&entry_path_normalized)
        .ok_or_else(|| "compilation produced no output".to_string())?;

    emit(value, output_format).map_err(|e| e.message())
}

/// Inject imports from compiled files into the evaluator scope.
/// Mirrors Compiler::inject_imports but uses our local compiled map.
fn inject_imports_virtual(
    evaluator: &mut Evaluator,
    ast: &hone::ast::File,
    resolved_import_paths: &[PathBuf],
    compiled: &HashMap<PathBuf, (Value, HashMap<String, Value>)>,
) {
    for (idx, item) in ast.preamble.iter().enumerate() {
        if let PreambleItem::Import(import) = item {
            let import_idx = ast
                .preamble
                .iter()
                .take(idx + 1)
                .filter(|p| matches!(p, PreambleItem::Import(_)))
                .count()
                - 1;

            let import_path = match resolved_import_paths.get(import_idx) {
                Some(p) => p,
                None => continue,
            };

            let (compiled_value, compiled_exports) = match compiled.get(import_path) {
                Some(c) => c,
                None => continue,
            };

            match &import.kind {
                ImportKind::Whole { alias, .. } => {
                    let alias_name = alias.clone().unwrap_or_else(|| {
                        import_path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("imported")
                            .to_string()
                    });

                    let mut exports_obj = IndexMap::new();
                    for (name, value) in compiled_exports {
                        exports_obj.insert(name.clone(), value.clone());
                    }
                    if let Value::Object(ref obj) = compiled_value {
                        for (k, v) in obj {
                            exports_obj.insert(k.clone(), v.clone());
                        }
                    }

                    evaluator.add_import(&alias_name, Value::Object(exports_obj));
                }
                ImportKind::Named { names, .. } => {
                    for name_import in names {
                        let local_name = name_import.alias.as_ref().unwrap_or(&name_import.name);

                        let value = compiled_exports
                            .get(&name_import.name)
                            .cloned()
                            .or_else(|| {
                                if let Value::Object(ref obj) = compiled_value {
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

/// Validate schemas, also collecting schemas from imported files.
fn validate_schemas_with_imports(
    ast: &hone::ast::File,
    value: &Value,
    source: &str,
    import_paths: &[PathBuf],
    resolver: &VirtualResolver,
    unchecked_paths: &std::collections::HashSet<String>,
) -> hone::HoneResult<()> {
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

    if use_statements.is_empty() {
        return Ok(());
    }

    let mut checker = TypeChecker::new(source.to_string());
    checker.set_unchecked_paths(unchecked_paths.clone());
    checker.collect_schemas(ast)?;

    // Also collect schemas from imported files
    for import_path in import_paths {
        if let Some(resolved) = resolver.get(import_path) {
            checker.collect_schemas(&resolved.ast)?;
        }
    }

    for use_stmt in use_statements {
        let location = SourceLocation {
            file: None,
            line: use_stmt.location.line,
            column: use_stmt.location.column,
            offset: use_stmt.location.offset,
            length: use_stmt.location.length,
        };

        if checker.get_schema(&use_stmt.schema_name).is_none() {
            return Err(hone::HoneError::UndefinedVariable {
                src: source.to_string(),
                span: (location.offset, location.length).into(),
                name: use_stmt.schema_name.clone(),
                help: format!("define schema '{}' before using it", use_stmt.schema_name),
            });
        }

        checker.check_type(
            value,
            &Type::Schema(use_stmt.schema_name.clone()),
            &location,
        )?;
    }

    Ok(())
}

#[wasm_bindgen]
pub fn format_source(source: &str) -> CompileResult {
    match hone::format_source(source) {
        Ok(formatted) => CompileResult {
            output: formatted,
            error: String::new(),
            success: true,
        },
        Err(e) => CompileResult {
            output: String::new(),
            error: e.message(),
            success: false,
        },
    }
}
