use std::collections::HashMap;
use std::path::PathBuf;
use wasm_bindgen::prelude::*;

use hone::ast::PolicyLevel;
use hone::ast::{BodyItem, ImportKind, PreambleItem};
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
    multi_doc: bool,
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

    #[wasm_bindgen(getter)]
    pub fn multi_doc(&self) -> bool {
        self.multi_doc
    }
}

fn ok_result(output: String) -> CompileResult {
    CompileResult {
        output,
        error: String::new(),
        success: true,
        multi_doc: false,
    }
}

fn err_result(error: String) -> CompileResult {
    CompileResult {
        output: String::new(),
        error,
        success: false,
        multi_doc: false,
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
        Err(e) => return err_result(e.message()),
    };

    // Parse
    let mut parser = Parser::new(tokens, source, None);
    let ast = match parser.parse() {
        Ok(a) => a,
        Err(e) => return err_result(e.message()),
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
        Err(e) => return err_result(e.message()),
    };

    // Schema validation (same as Compiler::compile_source)
    let unchecked_paths = evaluator.unchecked_paths().clone();
    if let Err(e) = validate_schemas(&ast, &value, source, &unchecked_paths) {
        return err_result(e.message());
    }

    // Emit
    match emit(&value, output_format) {
        Ok(output) => ok_result(output),
        Err(e) => err_result(e.message()),
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
        Ok((output, multi_doc)) => CompileResult {
            output,
            error: String::new(),
            success: true,
            multi_doc,
        },
        Err(e) => err_result(e),
    }
}

fn compile_project_inner(
    files_json: &str,
    entry_point: &str,
    format: &str,
    variant_json: &str,
    args_json: &str,
) -> Result<(String, bool), String> {
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

            // Emit each non-empty document as a JSON array of {name, content}
            let mut doc_entries = Vec::new();
            for (name, value) in &documents {
                if name.is_none() && value.is_empty_object() {
                    continue;
                }
                let emitted = emit(value, output_format).map_err(|e| e.message())?;
                let doc_name = name.clone().unwrap_or_default();
                doc_entries.push(serde_json::json!({
                    "name": doc_name,
                    "content": emitted,
                }));
            }
            let output = serde_json::to_string(&doc_entries)
                .map_err(|e| format!("JSON serialization error: {}", e))?;
            return Ok((output, true));
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

    let output = emit(value, output_format).map_err(|e| e.message())?;
    Ok((output, false))
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
        Ok(formatted) => ok_result(formatted),
        Err(e) => err_result(e.message()),
    }
}

// ---------------------------------------------------------------------------
// LSP-like intelligence exports for the playground (Monaco Editor)
// ---------------------------------------------------------------------------

/// Convert a byte offset to 0-based (line, column).
fn offset_to_position(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 0;
    let mut col = 0;
    let mut current_offset = 0;
    for ch in source.chars() {
        if current_offset >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
        current_offset += ch.len_utf8();
    }
    (line, col)
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn get_word_at_position(line: &str, char_idx: usize) -> Option<String> {
    let chars: Vec<char> = line.chars().collect();
    if char_idx >= chars.len() {
        return None;
    }
    let mut start = char_idx;
    while start > 0 && is_word_char(chars[start - 1]) {
        start -= 1;
    }
    let mut end = char_idx;
    while end < chars.len() && is_word_char(chars[end]) {
        end += 1;
    }
    if start == end {
        return None;
    }
    Some(chars[start..end].iter().collect())
}

/// Get diagnostics for Hone source code.
///
/// Returns a JSON array: `[{startLine, startCol, endLine, endCol, message, severity}]`
/// Severity: 8 = Error, 4 = Warning (Monaco values).
#[wasm_bindgen]
pub fn get_diagnostics(source: &str) -> String {
    let mut diagnostics: Vec<serde_json::Value> = Vec::new();

    let push_error =
        |diagnostics: &mut Vec<serde_json::Value>, error: &hone::HoneError, source: &str| {
            let (start_line, start_col) = if let Some(span) = error.span() {
                offset_to_position(source, span.start)
            } else {
                (0, 0)
            };
            let (end_line, end_col) = if let Some(span) = error.span() {
                offset_to_position(source, span.end)
            } else {
                (start_line, start_col + 1)
            };
            diagnostics.push(serde_json::json!({
                "startLine": start_line,
                "startCol": start_col,
                "endLine": end_line,
                "endCol": end_col,
                "message": error.message(),
                "severity": 8
            }));
        };

    // Lex
    let mut lexer = Lexer::new(source, None);
    let tokens = match lexer.tokenize() {
        Ok(t) => t,
        Err(e) => {
            push_error(&mut diagnostics, &e, source);
            return serde_json::to_string(&diagnostics).unwrap_or_else(|_| "[]".to_string());
        }
    };

    // Parse
    let mut parser = Parser::new(tokens, source, None);
    let ast = match parser.parse() {
        Ok(a) => a,
        Err(e) => {
            push_error(&mut diagnostics, &e, source);
            return serde_json::to_string(&diagnostics).unwrap_or_else(|_| "[]".to_string());
        }
    };

    // Evaluate
    let mut evaluator = Evaluator::new(source);
    match evaluator.evaluate(&ast) {
        Ok(value) => {
            // Type check against use statements
            let use_statements: Vec<_> = ast
                .preamble
                .iter()
                .filter_map(|item| {
                    if let PreambleItem::Use(u) = item {
                        Some(u)
                    } else {
                        None
                    }
                })
                .collect();

            if !use_statements.is_empty() {
                let mut checker = TypeChecker::new(source.to_string());
                let unchecked = evaluator.unchecked_paths().clone();
                checker.set_unchecked_paths(unchecked);
                if checker.collect_schemas(&ast).is_ok() {
                    for use_stmt in &use_statements {
                        if checker.get_schema(&use_stmt.schema_name).is_some() {
                            if let Err(e) = checker.check_type(
                                &value,
                                &Type::Schema(use_stmt.schema_name.clone()),
                                &use_stmt.location,
                            ) {
                                push_error(&mut diagnostics, &e, source);
                            }
                        }
                    }
                }
            }

            // Check policies
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

            if !policies.is_empty() {
                if let Ok(violations) = evaluator.check_policies(&policies, &value) {
                    for (name, level, msg) in &violations {
                        let severity = match level {
                            PolicyLevel::Deny => 8,
                            PolicyLevel::Warn => 4,
                        };
                        diagnostics.push(serde_json::json!({
                            "startLine": 0,
                            "startCol": 0,
                            "endLine": 0,
                            "endCol": 0,
                            "message": format!("Policy '{}': {}", name, msg),
                            "severity": severity
                        }));
                    }
                }
            }
        }
        Err(e) => {
            push_error(&mut diagnostics, &e, source);
        }
    }

    serde_json::to_string(&diagnostics).unwrap_or_else(|_| "[]".to_string())
}

/// Get completions at a given position.
///
/// Returns a JSON array: `[{label, kind, detail, insertText, insertTextFormat}]`
/// Kind values: 14=Keyword, 1=Function, 5=Variable, 3=Field.
/// insertTextFormat: 1=PlainText, 2=Snippet.
#[wasm_bindgen]
pub fn get_completions(source: &str, line: u32, col: u32) -> String {
    let mut items: Vec<serde_json::Value> = Vec::new();

    // Keywords
    let keywords = [
        ("let", "Variable binding", "let $1 = $2"),
        ("when", "Conditional block", "when $1 {\n\t$2\n}"),
        ("else", "Else branch", "else {\n\t$1\n}"),
        ("for", "For loop", "for $1 in $2 {\n\t$3\n}"),
        ("import", "Import module", "import \"$1\" as $2"),
        ("from", "Inherit from file", "from \"$1\""),
        ("true", "Boolean true", "true"),
        ("false", "Boolean false", "false"),
        ("null", "Null value", "null"),
        ("assert", "Assertion", "assert $1 : \"$2\""),
        ("type", "Type definition", "type $1 = $2"),
        ("schema", "Schema definition", "schema $1 {\n\t$2\n}"),
        (
            "variant",
            "Variant definition",
            "variant $1 {\n\tdefault $2 {\n\t\t$3\n\t}\n}",
        ),
        ("expect", "Argument declaration", "expect args.$1: $2"),
        ("secret", "Secret declaration", "secret $1 from \"$2\""),
        (
            "policy",
            "Policy declaration",
            "policy $1 deny when $2 {\n\t\"$3\"\n}",
        ),
        ("deny", "Policy deny level", "deny"),
        ("warn", "Policy warn level", "warn"),
    ];

    for (kw, detail, snippet) in keywords {
        items.push(serde_json::json!({
            "label": kw,
            "kind": 14,
            "detail": detail,
            "insertText": snippet,
            "insertTextFormat": 2
        }));
    }

    // Built-in functions
    let builtins = [
        ("len", "Get length of string, array, or object", "len($1)"),
        ("keys", "Get keys of an object", "keys($1)"),
        ("values", "Get values of an object", "values($1)"),
        (
            "contains",
            "Check if collection contains value",
            "contains($1, $2)",
        ),
        ("concat", "Concatenate arrays or strings", "concat($1, $2)"),
        ("merge", "Shallow merge objects", "merge($1, $2)"),
        ("flatten", "Flatten nested arrays", "flatten($1)"),
        ("default", "Null coalescing", "default($1, $2)"),
        ("upper", "Convert string to uppercase", "upper($1)"),
        ("lower", "Convert string to lowercase", "lower($1)"),
        ("trim", "Trim whitespace from string", "trim($1)"),
        ("split", "Split string by delimiter", "split($1, $2)"),
        ("join", "Join array with delimiter", "join($1, $2)"),
        ("replace", "Replace in string", "replace($1, $2, $3)"),
        ("range", "Generate range of numbers", "range($1, $2)"),
        ("base64_encode", "Encode to base64", "base64_encode($1)"),
        ("base64_decode", "Decode from base64", "base64_decode($1)"),
        ("to_json", "Convert to JSON string", "to_json($1)"),
        ("from_json", "Parse JSON string", "from_json($1)"),
        ("to_str", "Convert value to string", "to_str($1)"),
        ("to_int", "Convert value to integer", "to_int($1)"),
        ("to_float", "Convert value to float", "to_float($1)"),
        ("to_bool", "Convert value to boolean", "to_bool($1)"),
        ("env", "Get environment variable", "env(\"$1\")"),
        ("file", "Read file contents", "file(\"$1\")"),
    ];

    for (name, detail, snippet) in builtins {
        items.push(serde_json::json!({
            "label": name,
            "kind": 1,
            "detail": detail,
            "insertText": snippet,
            "insertTextFormat": 2
        }));
    }

    // Parse AST for variable and schema-aware completions
    let mut lexer = Lexer::new(source, None);
    if let Ok(tokens) = lexer.tokenize() {
        let mut parser = Parser::new(tokens, source, None);
        if let Ok(ast) = parser.parse() {
            // Let-bound variables from preamble
            for item in &ast.preamble {
                if let PreambleItem::Let(binding) = item {
                    items.push(serde_json::json!({
                        "label": binding.name,
                        "kind": 5,
                        "detail": "Local variable",
                        "insertText": binding.name,
                        "insertTextFormat": 1
                    }));
                }
            }
            // Let-bound variables from body
            for item in &ast.body {
                if let BodyItem::Let(binding) = item {
                    items.push(serde_json::json!({
                        "label": binding.name,
                        "kind": 5,
                        "detail": "Local variable",
                        "insertText": binding.name,
                        "insertTextFormat": 1
                    }));
                }
            }

            // Schema-aware completions
            add_schema_completions_json(&ast, line, col, &mut items);
        }
    }

    serde_json::to_string(&items).unwrap_or_else(|_| "[]".to_string())
}

/// Schema-aware field completions (ported from LSP)
fn add_schema_completions_json(
    ast: &hone::ast::File,
    line: u32,
    _col: u32,
    items: &mut Vec<serde_json::Value>,
) {
    let schemas: Vec<&hone::ast::SchemaDefinition> = ast
        .preamble
        .iter()
        .filter_map(|item| {
            if let PreambleItem::Schema(s) = item {
                Some(s)
            } else {
                None
            }
        })
        .collect();

    if schemas.is_empty() {
        return;
    }

    let used_schemas: Vec<&str> = ast
        .preamble
        .iter()
        .filter_map(|item| {
            if let PreambleItem::Use(u) = item {
                Some(u.schema_name.as_str())
            } else {
                None
            }
        })
        .collect();

    if used_schemas.is_empty() {
        return;
    }

    // Collect existing keys at cursor position
    let cursor_line = line as usize + 1; // AST lines are 1-based
    let existing_keys = keys_at_position_body(&ast.body, cursor_line);

    // Collect labels already added to avoid duplicates
    let existing_labels: Vec<String> = items
        .iter()
        .filter_map(|i| {
            i.get("label")
                .and_then(|l| l.as_str())
                .map(|s| s.to_string())
        })
        .collect();

    for schema_name in &used_schemas {
        if let Some(schema) = schemas.iter().find(|s| s.name == *schema_name) {
            add_fields_from_schema_json(schema, &schemas, &existing_keys, &existing_labels, items);
        }
    }
}

fn keys_at_position_body(body: &[BodyItem], cursor_line: usize) -> Vec<String> {
    // Check if cursor is inside a nested block
    for item in body {
        if let Some(keys) = keys_in_block_at_line_wasm(item, cursor_line) {
            return keys;
        }
    }
    // Top-level keys
    let mut keys = Vec::new();
    for item in body {
        match item {
            BodyItem::KeyValue(kv) => {
                if let hone::ast::Key::Ident(name) = &kv.key {
                    keys.push(name.clone());
                }
            }
            BodyItem::Block(block) => {
                keys.push(block.name.clone());
            }
            _ => {}
        }
    }
    keys
}

fn keys_in_block_at_line_wasm(item: &BodyItem, cursor_line: usize) -> Option<Vec<String>> {
    if let BodyItem::Block(block) = item {
        let block_start = block.location.line;
        let block_end = block_start + block.location.length.max(1);
        if cursor_line >= block_start && cursor_line <= block_end {
            for child in &block.items {
                if let Some(keys) = keys_in_block_at_line_wasm(child, cursor_line) {
                    return Some(keys);
                }
            }
            let mut keys = Vec::new();
            for child in &block.items {
                match child {
                    BodyItem::KeyValue(kv) => {
                        if let hone::ast::Key::Ident(name) = &kv.key {
                            keys.push(name.clone());
                        }
                    }
                    BodyItem::Block(b) => {
                        keys.push(b.name.clone());
                    }
                    _ => {}
                }
            }
            return Some(keys);
        }
    }
    None
}

fn format_type_constraint(constraint: &hone::ast::TypeConstraint) -> String {
    if constraint.args.is_empty() {
        constraint.name.clone()
    } else {
        format!("{}(...)", constraint.name)
    }
}

fn add_fields_from_schema_json(
    schema: &hone::ast::SchemaDefinition,
    all_schemas: &[&hone::ast::SchemaDefinition],
    existing_keys: &[String],
    existing_labels: &[String],
    items: &mut Vec<serde_json::Value>,
) {
    if let Some(ref parent_name) = schema.extends {
        if let Some(parent) = all_schemas.iter().find(|s| s.name == *parent_name) {
            add_fields_from_schema_json(parent, all_schemas, existing_keys, existing_labels, items);
        }
    }

    for field in &schema.fields {
        if existing_keys.iter().any(|k| k == &field.name) {
            continue;
        }
        // Check items already added (from parent schema or existing labels)
        if existing_labels.iter().any(|l| l == &field.name) {
            continue;
        }
        if items.iter().any(|i| {
            i.get("label")
                .and_then(|l| l.as_str())
                .is_some_and(|l| l == field.name)
        }) {
            continue;
        }

        let type_str = format_type_constraint(&field.constraint);
        let required = if field.optional {
            "optional"
        } else {
            "required"
        };
        let detail = format!("{}: {} ({})", field.name, type_str, required);
        let sort_prefix = if field.optional { "1" } else { "0" };

        items.push(serde_json::json!({
            "label": field.name,
            "kind": 3,
            "detail": detail,
            "insertText": format!("{}: $1", field.name),
            "insertTextFormat": 2,
            "sortText": format!("{}_{}", sort_prefix, field.name)
        }));
    }
}

/// Get hover information at a given position.
///
/// Returns a JSON object: `{contents: string, range?: {startLine, startCol, endLine, endCol}}`
/// or an empty string `""` if no hover is available.
/// The contents field is markdown.
#[wasm_bindgen]
pub fn get_hover(source: &str, line: u32, col: u32) -> String {
    let line_idx = line as usize;
    let char_idx = col as usize;

    let lines: Vec<&str> = source.lines().collect();
    if line_idx >= lines.len() {
        return String::new();
    }

    let line_str = lines[line_idx];
    let word = match get_word_at_position(line_str, char_idx) {
        Some(w) => w,
        None => return String::new(),
    };

    // Check keywords
    let keyword_docs: &[(&str, &str)] = &[
        ("let", "**let** - Variable binding\n\nDeclares a variable with the given name and value.\n\n```hone\nlet name = \"value\"\n```"),
        ("when", "**when** - Conditional block\n\nConditionally includes configuration. Supports else chains.\n\n```hone\nwhen env == \"prod\" {\n  replicas: 3\n} else {\n  replicas: 1\n}\n```"),
        ("else", "**else** - Else branch\n\nProvides an alternative branch for a when block.\n\n```hone\nwhen env == \"prod\" {\n  replicas: 3\n} else when env == \"staging\" {\n  replicas: 2\n} else {\n  replicas: 1\n}\n```"),
        ("for", "**for** - Iteration\n\nIterates over an array or object.\n\n```hone\nlet doubled = for x in [1, 2, 3] { x * 2 }\n```"),
        ("import", "**import** - Module import\n\nImports definitions from another Hone file.\n\n```hone\nimport \"./config.hone\" as config\nimport { a, b } from \"./utils.hone\"\n```"),
        ("from", "**from** - Inheritance\n\nInherits and extends from a base configuration.\n\n```hone\nfrom \"./base.hone\"\n\noverrides {\n  key: \"new value\"\n}\n```"),
        ("assert", "**assert** - Assertion\n\nValidates a condition and fails with message if false.\n\n```hone\nassert len(name) > 0 : \"name cannot be empty\"\n```"),
        ("type", "**type** - Type alias\n\nDefines a type alias for documentation.\n\n```hone\ntype Port = int\n```"),
        ("schema", "**schema** - Schema definition\n\nDefines a schema for validating object structure.\n\n```hone\nschema Person {\n  name: string\n  age: int\n}\n```"),
        ("expect", "**expect** - Argument declaration\n\nDeclares expected CLI arguments with type and optional default.\n\n```hone\nexpect args.env: string\nexpect args.port: int = 8080\n```"),
        ("secret", "**secret** - Secret declaration\n\nDeclares a secret placeholder that is never emitted as a real value.\n\n```hone\nsecret db_pass from \"vault:secret/data/db#password\"\nsecret api_key from \"env:API_KEY\"\n```\n\nUse `--secrets-mode env` to resolve `env:` secrets from environment variables."),
        ("policy", "**policy** - Policy declaration\n\nDeclares a policy rule that checks the output after compilation.\n\n```hone\npolicy no_debug deny when output.debug == true {\n  \"debug must be disabled in production\"\n}\n```\n\n- `deny` policies cause compilation failure\n- `warn` policies emit warnings but succeed"),
        ("variant", "**variant** - Environment-specific configuration\n\nDefines configuration variants selected at compile time.\n\n```hone\nvariant env {\n  default dev {\n    replicas: 1\n  }\n  production {\n    replicas: 5\n  }\n}\n```\n\nCompile with: `hone compile config.hone --variant env=production`"),
        ("use", "**use** - Schema validation\n\nApplies a schema to validate the output at compile time.\n\n```hone\nschema Server {\n  host: string\n  port: int(1, 65535)\n}\n\nuse Server\n\nhost: \"localhost\"\nport: 8080\n```"),
    ];

    for (kw, doc) in keyword_docs {
        if word == *kw {
            return serde_json::json!({ "contents": doc }).to_string();
        }
    }

    // Check builtin functions
    let builtin_docs: &[(&str, &str)] = &[
        ("len", "**len**(value) -> int\n\nReturns the length of a string, array, or object.\n\n```hone\nlen(\"hello\")  // 5\nlen([1, 2, 3])  // 3\n```"),
        ("keys", "**keys**(object) -> array\n\nReturns the keys of an object as an array.\n\n```hone\nkeys({ a: 1, b: 2 })  // [\"a\", \"b\"]\n```"),
        ("values", "**values**(object) -> array\n\nReturns the values of an object as an array.\n\n```hone\nvalues({ a: 1, b: 2 })  // [1, 2]\n```"),
        ("contains", "**contains**(collection, value) -> bool\n\nChecks if collection contains the value.\n\n```hone\ncontains([1, 2, 3], 2)  // true\ncontains(\"hello\", \"ell\")  // true\n```"),
        ("concat", "**concat**(arrays...) -> array | concat(strings...) -> string\n\nConcatenates arrays or strings.\n\n```hone\nconcat([1, 2], [3, 4])  // [1, 2, 3, 4]\n```"),
        ("merge", "**merge**(objects...) -> object\n\nShallow merges objects, right wins on conflicts.\n\n```hone\nmerge({ a: 1 }, { b: 2 })  // { a: 1, b: 2 }\n```"),
        ("flatten", "**flatten**(array) -> array\n\nFlattens one level of nesting.\n\n```hone\nflatten([[1, 2], [3]])  // [1, 2, 3]\n```"),
        ("default", "**default**(value, fallback) -> value\n\nReturns value if not null, otherwise fallback.\n\n```hone\ndefault(null, 42)  // 42\ndefault(1, 42)  // 1\n```"),
        ("upper", "**upper**(string) -> string\n\nConverts string to uppercase.\n\n```hone\nupper(\"hello\")  // \"HELLO\"\n```"),
        ("lower", "**lower**(string) -> string\n\nConverts string to lowercase.\n\n```hone\nlower(\"HELLO\")  // \"hello\"\n```"),
        ("trim", "**trim**(string) -> string\n\nRemoves leading and trailing whitespace.\n\n```hone\ntrim(\"  hello  \")  // \"hello\"\n```"),
        ("split", "**split**(string, delimiter) -> array\n\nSplits string by delimiter.\n\n```hone\nsplit(\"a,b,c\", \",\")  // [\"a\", \"b\", \"c\"]\n```"),
        ("join", "**join**(array, delimiter) -> string\n\nJoins array elements with delimiter.\n\n```hone\njoin([\"a\", \"b\", \"c\"], \"-\")  // \"a-b-c\"\n```"),
        ("replace", "**replace**(string, pattern, replacement) -> string\n\nReplaces occurrences of pattern.\n\n```hone\nreplace(\"hello\", \"l\", \"L\")  // \"heLLo\"\n```"),
        ("range", "**range**(start, end) -> array\n\nGenerates array of integers from start to end-1.\n\n```hone\nrange(0, 3)  // [0, 1, 2]\n```"),
        ("base64_encode", "**base64_encode**(string) -> string\n\nEncodes string to base64.\n\n```hone\nbase64_encode(\"hello\")  // \"aGVsbG8=\"\n```"),
        ("base64_decode", "**base64_decode**(string) -> string\n\nDecodes base64 string.\n\n```hone\nbase64_decode(\"aGVsbG8=\")  // \"hello\"\n```"),
        ("to_json", "**to_json**(value) -> string\n\nConverts value to JSON string.\n\n```hone\nto_json({ a: 1 })  // \"{\\\"a\\\":1}\"\n```"),
        ("from_json", "**from_json**(string) -> value\n\nParses JSON string to value.\n\n```hone\nfrom_json(\"{\\\"a\\\":1}\")  // { a: 1 }\n```"),
        ("to_str", "**to_str**(value) -> string\n\nConverts a scalar value to string.\n\n```hone\nto_str(42)  // \"42\"\nto_str(true)  // \"true\"\n```"),
        ("to_int", "**to_int**(value) -> int\n\nConverts value to integer.\n\n```hone\nto_int(\"42\")  // 42\nto_int(3.7)  // 3\n```"),
        ("to_float", "**to_float**(value) -> float\n\nConverts value to float.\n\n```hone\nto_float(\"3.14\")  // 3.14\nto_float(42)  // 42.0\n```"),
        ("to_bool", "**to_bool**(value) -> bool\n\nConverts value to boolean using truthiness.\n\n```hone\nto_bool(1)  // true\nto_bool(\"\")  // false\n```"),
        ("env", "**env**(name, default?) -> string\n\nReads environment variable.\n\n```hone\nenv(\"HOME\")\nenv(\"MISSING\", \"default\")\n```"),
        ("file", "**file**(path) -> string\n\nReads file contents as string.\n\n```hone\nfile(\"./config.txt\")\n```"),
    ];

    for (name, doc) in builtin_docs {
        if word == *name {
            return serde_json::json!({ "contents": doc }).to_string();
        }
    }

    // Parse AST for variable, schema, expect, secret hover info
    let mut lexer = Lexer::new(source, None);
    if let Ok(tokens) = lexer.tokenize() {
        let mut parser = Parser::new(tokens, source, None);
        if let Ok(ast) = parser.parse() {
            // Check preamble items
            for item in &ast.preamble {
                match item {
                    PreambleItem::Let(binding) if binding.name == word => {
                        let mut evaluator = Evaluator::new(source);
                        let display = match evaluator.eval_expr(&binding.value) {
                            Ok(val) => {
                                format!("**{}**: {} = `{}`", binding.name, val.type_name(), val)
                            }
                            Err(_) => format!(
                                "**{}** - Local variable\n\n```hone\nlet {} = ...\n```",
                                binding.name, binding.name
                            ),
                        };
                        return serde_json::json!({ "contents": display }).to_string();
                    }
                    PreambleItem::Schema(schema) if schema.name == word => {
                        let mut info = format!("**schema {}**", schema.name);
                        if let Some(ref ext) = schema.extends {
                            info.push_str(&format!(" extends {}", ext));
                        }
                        info.push_str(
                            "\n\n| Field | Type | Required |\n|-------|------|----------|\n",
                        );
                        for field in &schema.fields {
                            info.push_str(&format!(
                                "| {} | {} | {} |\n",
                                field.name,
                                field.constraint.name,
                                if field.optional {
                                    "optional"
                                } else {
                                    "required"
                                },
                            ));
                        }
                        if schema.open {
                            info.push_str("\n*Open schema - extra fields allowed*");
                        }
                        return serde_json::json!({ "contents": info }).to_string();
                    }
                    PreambleItem::Expect(expect) => {
                        let last = expect.path.last().map(|s| s.as_str()).unwrap_or("");
                        if last == word {
                            let mut info =
                                format!("**{}**: {}", expect.path.join("."), expect.type_name);
                            if let Some(ref default_val) = expect.default {
                                info.push_str(&format!(" = {}", default_val.display()));
                            }
                            info.push_str("\n\n*CLI argument declaration*");
                            return serde_json::json!({ "contents": info }).to_string();
                        }
                    }
                    PreambleItem::Secret(secret) if secret.name == word => {
                        let info = format!(
                            "**{}** - Secret\n\nProvider: `{}`\n\nEmits `<SECRET:{}>` placeholder in output.",
                            secret.name, secret.provider, secret.provider
                        );
                        return serde_json::json!({ "contents": info }).to_string();
                    }
                    _ => {}
                }
            }
            // Check body let bindings
            for item in &ast.body {
                if let BodyItem::Let(binding) = item {
                    if binding.name == word {
                        let mut evaluator = Evaluator::new(source);
                        let display = match evaluator.eval_expr(&binding.value) {
                            Ok(val) => {
                                format!("**{}**: {} = `{}`", binding.name, val.type_name(), val)
                            }
                            Err(_) => format!(
                                "**{}** - Local variable\n\n```hone\nlet {} = ...\n```",
                                binding.name, binding.name
                            ),
                        };
                        return serde_json::json!({ "contents": display }).to_string();
                    }
                }
            }
        }
    }

    String::new()
}
