use wasm_bindgen::prelude::*;
use std::collections::HashMap;

use hone::ast::PreambleItem;
use hone::{emit, infer_value, Evaluator, Lexer, OutputFormat, Parser, Type, TypeChecker, Value};
use hone::lexer::token::SourceLocation;
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
    let use_statements: Vec<_> = ast.preamble.iter()
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

        checker.check_type(value, &Type::Schema(use_stmt.schema_name.clone()), &location)?;
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
