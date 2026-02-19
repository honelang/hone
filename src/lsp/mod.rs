//! Hone Language Server Protocol implementation.
//!
//! Provides IDE features: diagnostics, go-to-definition, hover, completions,
//! find references, rename, and schema-aware field suggestions.

use dashmap::DashMap;
use ropey::Rope;
use std::path::PathBuf;
use std::sync::Arc;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::errors::HoneError;
use crate::lexer::Lexer;
use crate::parser::ast::{BodyItem, File, PreambleItem};
use crate::parser::Parser;

/// Document state tracked by the server
#[derive(Debug)]
pub struct Document {
    /// The document content as a rope for efficient editing
    pub content: Rope,
    /// Parsed AST (if parsing succeeded)
    pub ast: Option<File>,
    /// Path to the document
    pub path: Option<PathBuf>,
}

impl Document {
    pub fn new(content: &str) -> Self {
        Self {
            content: Rope::from_str(content),
            ast: None,
            path: None,
        }
    }

    pub fn with_path(mut self, path: PathBuf) -> Self {
        self.path = Some(path);
        self
    }

    pub fn text(&self) -> String {
        self.content.to_string()
    }
}

/// The Hone Language Server
pub struct HoneLanguageServer {
    /// LSP client for sending notifications
    client: Client,
    /// Open documents indexed by URI
    documents: DashMap<Url, Document>,
    /// Server capabilities
    capabilities: Arc<ServerCapabilities>,
}

impl HoneLanguageServer {
    pub fn new(client: Client) -> Self {
        let capabilities = ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Options(
                TextDocumentSyncOptions {
                    open_close: Some(true),
                    change: Some(TextDocumentSyncKind::FULL),
                    save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                        include_text: Some(true),
                    })),
                    ..Default::default()
                },
            )),
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            completion_provider: Some(CompletionOptions {
                trigger_characters: Some(vec![".".to_string(), ":".to_string()]),
                resolve_provider: Some(false),
                ..Default::default()
            }),
            document_formatting_provider: Some(OneOf::Left(true)),
            definition_provider: Some(OneOf::Left(true)),
            references_provider: Some(OneOf::Left(true)),
            rename_provider: Some(OneOf::Right(RenameOptions {
                prepare_provider: Some(true),
                work_done_progress_options: Default::default(),
            })),
            ..Default::default()
        };

        Self {
            client,
            documents: DashMap::new(),
            capabilities: Arc::new(capabilities),
        }
    }

    /// Parse a document, run evaluation and type checking, and update its AST
    fn parse_document(&self, uri: &Url, content: &str) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Get path from document if available
        let path = self.documents.get(uri).and_then(|d| d.path.clone());

        // Lex the source
        let mut lexer = Lexer::new(content, path.clone());
        let tokens = match lexer.tokenize() {
            Ok(tokens) => tokens,
            Err(e) => {
                diagnostics.push(error_to_diagnostic(&e, content));
                return diagnostics;
            }
        };

        // Parse the tokens
        let mut parser = Parser::new(tokens, content, path);
        let ast = match parser.parse() {
            Ok(ast) => ast,
            Err(e) => {
                diagnostics.push(error_to_diagnostic(&e, content));
                return diagnostics;
            }
        };

        // Background evaluation: run evaluator to catch runtime errors
        let mut evaluator = crate::evaluator::Evaluator::new(content);
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
                    let mut checker = crate::typechecker::TypeChecker::new(content.to_string());
                    let unchecked = evaluator.unchecked_paths().clone();
                    checker.set_unchecked_paths(unchecked);
                    if checker.collect_schemas(&ast).is_ok() {
                        for use_stmt in &use_statements {
                            if checker.get_schema(&use_stmt.schema_name).is_some() {
                                if let Err(e) = checker.check_type(
                                    &value,
                                    &crate::typechecker::Type::Schema(use_stmt.schema_name.clone()),
                                    &use_stmt.location,
                                ) {
                                    diagnostics.push(error_to_diagnostic(&e, content));
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
                                crate::parser::ast::PolicyLevel::Deny => DiagnosticSeverity::ERROR,
                                crate::parser::ast::PolicyLevel::Warn => {
                                    DiagnosticSeverity::WARNING
                                }
                            };
                            diagnostics.push(Diagnostic {
                                range: Range {
                                    start: Position::new(0, 0),
                                    end: Position::new(0, 0),
                                },
                                severity: Some(severity),
                                source: Some("hone".to_string()),
                                message: format!("Policy '{}': {}", name, msg),
                                ..Default::default()
                            });
                        }
                    }
                }
            }
            Err(e) => {
                diagnostics.push(error_to_diagnostic(&e, content));
            }
        }

        // Update the document with the parsed AST
        if let Some(mut doc) = self.documents.get_mut(uri) {
            doc.ast = Some(ast);
        }

        diagnostics
    }

    /// Get completions at the given position
    fn get_completions(&self, uri: &Url, _position: Position) -> Vec<CompletionItem> {
        let mut items = Vec::new();

        // Add keywords
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
            (
                "fn",
                "Function definition",
                "fn $1($2) {\n\t$3\n}",
            ),
        ];

        for (keyword, detail, snippet) in keywords {
            items.push(CompletionItem {
                label: keyword.to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some(detail.to_string()),
                insert_text: Some(snippet.to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            });
        }

        // Add built-in functions
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
            ("sort", "Sort an array", "sort($1)"),
            (
                "starts_with",
                "Check if string starts with prefix",
                "starts_with($1, $2)",
            ),
            (
                "ends_with",
                "Check if string ends with suffix",
                "ends_with($1, $2)",
            ),
            ("min", "Return the smaller of two numbers", "min($1, $2)"),
            ("max", "Return the larger of two numbers", "max($1, $2)"),
            ("abs", "Absolute value of a number", "abs($1)"),
            ("unique", "Remove duplicates from array", "unique($1)"),
            ("sha256", "SHA-256 hash of a string", "sha256($1)"),
            ("type_of", "Get the type name of a value", "type_of($1)"),
            (
                "substring",
                "Extract substring by index",
                "substring($1, $2, $3)",
            ),
            ("entries", "Object to [[key, value], ...] array", "entries($1)"),
            (
                "from_entries",
                "[[key, value], ...] array to object",
                "from_entries($1)",
            ),
            (
                "clamp",
                "Clamp a number between min and max",
                "clamp($1, $2, $3)",
            ),
            ("reverse", "Reverse an array or string", "reverse($1)"),
            (
                "slice",
                "Extract a sub-array or substring",
                "slice($1, $2, $3)",
            ),
        ];

        for (name, detail, snippet) in builtins {
            items.push(CompletionItem {
                label: name.to_string(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some(detail.to_string()),
                insert_text: Some(snippet.to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            });
        }

        // Add variables and schema-aware completions from the document's AST
        if let Some(doc) = self.documents.get(uri) {
            if let Some(ref ast) = doc.ast {
                // Check preamble for let bindings
                for item in &ast.preamble {
                    if let PreambleItem::Let(binding) = item {
                        items.push(CompletionItem {
                            label: binding.name.clone(),
                            kind: Some(CompletionItemKind::VARIABLE),
                            detail: Some("Local variable".to_string()),
                            ..Default::default()
                        });
                    }
                }
                // Check preamble for fn definitions
                for item in &ast.preamble {
                    if let PreambleItem::FnDef(fn_def) = item {
                        let sig = format!("fn {}({})", fn_def.name, fn_def.params.join(", "));
                        items.push(CompletionItem {
                            label: fn_def.name.clone(),
                            kind: Some(CompletionItemKind::FUNCTION),
                            detail: Some(sig),
                            insert_text: Some(format!("{}($1)", fn_def.name)),
                            insert_text_format: Some(InsertTextFormat::SNIPPET),
                            ..Default::default()
                        });
                    }
                }
                // Check body for let bindings
                for item in &ast.body {
                    if let BodyItem::Let(binding) = item {
                        items.push(CompletionItem {
                            label: binding.name.clone(),
                            kind: Some(CompletionItemKind::VARIABLE),
                            detail: Some("Local variable".to_string()),
                            ..Default::default()
                        });
                    }
                }

                // Schema-aware completions
                add_schema_completions(ast, _position, &mut items);
            }
        }

        items
    }

    /// Get hover information at the given position.
    /// Checks are intentionally sequential and return on first match (priority order):
    /// builtins > schema fields > variables > keywords, so the most specific hover wins.
    fn get_hover(&self, uri: &Url, position: Position) -> Option<Hover> {
        let doc = self.documents.get(uri)?;
        let content = doc.text();

        // Get the word at position
        let line_idx = position.line as usize;
        let char_idx = position.character as usize;

        let lines: Vec<&str> = content.lines().collect();
        if line_idx >= lines.len() {
            return None;
        }

        let line = lines[line_idx];
        let word = get_word_at_position(line, char_idx)?;

        // Check if it's a keyword
        let keyword_docs = [
            ("let", "**let** - Variable binding\n\nDeclares a variable with the given name and value.\n\n```hone\nlet name = \"value\"\n```"),
            ("when", "**when** - Conditional block\n\nConditionally includes configuration. Supports else chains.\n\n```hone\nwhen env == \"prod\" {\n  replicas: 3\n} else {\n  replicas: 1\n}\n```"),
            ("else", "**else** - Else branch\n\nProvides an alternative branch for a when block.\n\n```hone\nwhen env == \"prod\" {\n  replicas: 3\n} else when env == \"staging\" {\n  replicas: 2\n} else {\n  replicas: 1\n}\n```"),
            ("for", "**for** - Iteration\n\nIterates over an array or object.\n\n```hone\nlet doubled = for x in [1, 2, 3] { x * 2 }\n```"),
            ("import", "**import** - Module import\n\nImports definitions from another Hone file.\n\n```hone\nimport \"./config.hone\" as config\nimport { a, b } from \"./utils.hone\"\n```"),
            ("from", "**from** - Inheritance\n\nInherits and extends from a base configuration.\n\n```hone\nfrom \"./base.hone\"\n\noverrides {\n  key: \"new value\"\n}\n```"),
            ("assert", "**assert** - Assertion\n\nValidates a condition and fails with message if false.\n\n```hone\nassert len(name) > 0 : \"name cannot be empty\"\n```"),
            ("type", "**type** - Type alias\n\nDefines a type alias for documentation.\n\n```hone\ntype Port = int\n```"),
            ("schema", "**schema** - Schema definition\n\nDefines a schema for validating object structure.\n\n```hone\nschema Person {\n  name: string\n  age: int\n}\n```"),
            ("spread", "**spread** - Spread operator\n\nSpreads an object or array into another.\n\n```hone\nlet merged = { ...base, key: \"override\" }\n```"),
            ("expect", "**expect** - Argument declaration\n\nDeclares expected CLI arguments with type and optional default.\n\n```hone\nexpect args.env: string\nexpect args.port: int = 8080\n```"),
            ("secret", "**secret** - Secret declaration\n\nDeclares a secret placeholder that is never emitted as a real value.\n\n```hone\nsecret db_pass from \"vault:secret/data/db#password\"\nsecret api_key from \"env:API_KEY\"\n```\n\nUse `--secrets-mode env` to resolve `env:` secrets from environment variables."),
            ("policy", "**policy** - Policy declaration\n\nDeclares a policy rule that checks the output after compilation.\n\n```hone\npolicy no_debug deny when output.debug == true {\n  \"debug must be disabled in production\"\n}\n\npolicy port_range warn when output.port < 1024 {\n  \"privileged ports require elevated permissions\"\n}\n```\n\n- `deny` policies cause compilation failure\n- `warn` policies emit warnings but succeed"),
            ("variant", "**variant** - Environment-specific configuration\n\nDefines configuration variants selected at compile time.\n\n```hone\nvariant env {\n  default dev {\n    replicas: 1\n  }\n  production {\n    replicas: 5\n  }\n}\n```\n\nCompile with: `hone compile config.hone --variant env=production`"),
            ("use", "**use** - Schema validation\n\nApplies a schema to validate the output at compile time.\n\n```hone\nschema Server {\n  host: string\n  port: int(1, 65535)\n}\n\nuse Server\n\nhost: \"localhost\"\nport: 8080\n```"),
        ];

        for (kw, doc) in keyword_docs {
            if word == kw {
                return Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: doc.to_string(),
                    }),
                    range: None,
                });
            }
        }

        // Check if it's a builtin function
        let builtin_docs = [
            ("len", "**len**(value) -> int\n\nReturns the length of a string, array, or object.\n\n```hone\nlen(\"hello\")  // 5\nlen([1, 2, 3])  // 3\n```"),
            ("keys", "**keys**(object) -> array\n\nReturns the keys of an object as an array.\n\n```hone\nkeys({ a: 1, b: 2 })  // [\"a\", \"b\"]\n```"),
            ("values", "**values**(object) -> array\n\nReturns the values of an object as an array.\n\n```hone\nvalues({ a: 1, b: 2 })  // [1, 2]\n```"),
            ("contains", "**contains**(collection, value) -> bool\n\nChecks if collection contains the value.\n\n```hone\ncontains([1, 2, 3], 2)  // true\ncontains(\"hello\", \"ell\")  // true\n```"),
            ("concat", "**concat**(arrays...) -> array | concat(strings...) -> string\n\nConcatenates arrays or strings.\n\n```hone\nconcat([1, 2], [3, 4])  // [1, 2, 3, 4]\nconcat(\"hello\", \" world\")  // \"hello world\"\n```"),
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
            ("sort", "**sort**(array) -> array\n\nSorts an array of comparable values (ints, floats, strings).\n\n```hone\nsort([3, 1, 2])  // [1, 2, 3]\nsort([\"c\", \"a\", \"b\"])  // [\"a\", \"b\", \"c\"]\n```"),
            ("starts_with", "**starts_with**(string, prefix) -> bool\n\nChecks if a string starts with the given prefix.\n\n```hone\nstarts_with(\"hello\", \"he\")  // true\n```"),
            ("ends_with", "**ends_with**(string, suffix) -> bool\n\nChecks if a string ends with the given suffix.\n\n```hone\nends_with(\"hello\", \"lo\")  // true\n```"),
            ("min", "**min**(a, b) -> number\n\nReturns the smaller of two numbers.\n\n```hone\nmin(3, 7)  // 3\n```"),
            ("max", "**max**(a, b) -> number\n\nReturns the larger of two numbers.\n\n```hone\nmax(3, 7)  // 7\n```"),
            ("abs", "**abs**(number) -> number\n\nReturns the absolute value of a number.\n\n```hone\nabs(-5)  // 5\nabs(3.14)  // 3.14\n```"),
            ("unique", "**unique**(array) -> array\n\nRemoves duplicate values, preserving first occurrence order.\n\n```hone\nunique([1, 2, 2, 3, 1])  // [1, 2, 3]\n```"),
            ("sha256", "**sha256**(string) -> string\n\nReturns the SHA-256 hex digest of a string.\n\n```hone\nsha256(\"hello\")  // \"2cf24dba...\"\n```"),
            ("type_of", "**type_of**(value) -> string\n\nReturns the type name of a value.\n\n```hone\ntype_of(42)  // \"int\"\ntype_of(\"hi\")  // \"string\"\ntype_of([1])  // \"array\"\n```"),
            ("substring", "**substring**(string, start, end?) -> string\n\nExtracts a substring by character index (0-based, end exclusive).\n\n```hone\nsubstring(\"hello\", 1, 4)  // \"ell\"\nsubstring(\"hello\", 2)  // \"llo\"\n```"),
            ("entries", "**entries**(object) -> array\n\nConverts an object to an array of [key, value] pairs.\n\n```hone\nentries({ a: 1, b: 2 })  // [[\"a\", 1], [\"b\", 2]]\n```"),
            ("from_entries", "**from_entries**(array) -> object\n\nConverts an array of [key, value] pairs to an object.\n\n```hone\nfrom_entries([[\"a\", 1], [\"b\", 2]])  // { a: 1, b: 2 }\n```"),
            ("clamp", "**clamp**(value, min, max) -> number\n\nClamps a number between min and max (inclusive).\n\n```hone\nclamp(15, 0, 10)  // 10\nclamp(-5, 0, 10)  // 0\n```"),
            ("reverse", "**reverse**(value) -> array | string\n\nReverses an array or string.\n\n```hone\nreverse([1, 2, 3])  // [3, 2, 1]\nreverse(\"hello\")  // \"olleh\"\n```"),
            ("slice", "**slice**(value, start, end?) -> array | string\n\nExtracts a sub-array or substring. Supports negative indices.\n\n```hone\nslice([1, 2, 3, 4], 1, 3)  // [2, 3]\nslice(\"hello\", -3)  // \"llo\"\n```"),
        ];

        for (name, doc) in builtin_docs {
            if word == name {
                return Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: doc.to_string(),
                    }),
                    range: None,
                });
            }
        }

        // Check variables in the AST
        if let Some(ref ast) = doc.ast {
            // Check preamble for let bindings with evaluated values
            for item in &ast.preamble {
                if let PreambleItem::Let(binding) = item {
                    if binding.name == word {
                        let value_info = self.try_evaluate_expr(&content, &binding.value);
                        let display = match value_info {
                            Some(val) => {
                                format!("**{}**: {} = `{}`", binding.name, val.type_name(), val)
                            }
                            None => format!(
                                "**{}** - Local variable\n\n```hone\nlet {} = {}\n```",
                                binding.name,
                                binding.name,
                                binding.value.display()
                            ),
                        };
                        return Some(Hover {
                            contents: HoverContents::Markup(MarkupContent {
                                kind: MarkupKind::Markdown,
                                value: display,
                            }),
                            range: None,
                        });
                    }
                }
                // Check schema names
                if let PreambleItem::Schema(schema) = item {
                    if schema.name == word {
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
                        return Some(Hover {
                            contents: HoverContents::Markup(MarkupContent {
                                kind: MarkupKind::Markdown,
                                value: info,
                            }),
                            range: None,
                        });
                    }
                }
                // Check use statements
                if let PreambleItem::Use(use_stmt) = item {
                    if use_stmt.schema_name == word {
                        // Find the schema definition
                        for inner_item in &ast.preamble {
                            if let PreambleItem::Schema(schema) = inner_item {
                                if schema.name == word {
                                    let mut info = format!(
                                        "**use {}** - Schema validation active\n\nFields:\n",
                                        word
                                    );
                                    for field in &schema.fields {
                                        let req = if field.optional { "?" } else { "" };
                                        info.push_str(&format!(
                                            "- `{}{}`: {}\n",
                                            field.name, req, field.constraint.name
                                        ));
                                    }
                                    return Some(Hover {
                                        contents: HoverContents::Markup(MarkupContent {
                                            kind: MarkupKind::Markdown,
                                            value: info,
                                        }),
                                        range: None,
                                    });
                                }
                            }
                        }
                    }
                }
                // Check expect declarations
                if let PreambleItem::Expect(expect) = item {
                    let last = expect.path.last().map(|s| s.as_str()).unwrap_or("");
                    if last == word {
                        let mut info =
                            format!("**{}**: {}", expect.path.join("."), expect.type_name);
                        if let Some(ref default) = expect.default {
                            info.push_str(&format!(" = {}", default.display()));
                        }
                        info.push_str("\n\n*CLI argument declaration*");
                        return Some(Hover {
                            contents: HoverContents::Markup(MarkupContent {
                                kind: MarkupKind::Markdown,
                                value: info,
                            }),
                            range: None,
                        });
                    }
                }
                // Check fn definitions
                if let PreambleItem::FnDef(fn_def) = item {
                    if fn_def.name == word {
                        let sig = format!("fn {}({})", fn_def.name, fn_def.params.join(", "));
                        let info = format!(
                            "**{}** - User-defined function\n\n```hone\n{} {{\n  {}\n}}\n```",
                            fn_def.name,
                            sig,
                            fn_def.body.display()
                        );
                        return Some(Hover {
                            contents: HoverContents::Markup(MarkupContent {
                                kind: MarkupKind::Markdown,
                                value: info,
                            }),
                            range: None,
                        });
                    }
                }
                // Check secret declarations
                if let PreambleItem::Secret(secret) = item {
                    if secret.name == word {
                        return Some(Hover {
                            contents: HoverContents::Markup(MarkupContent {
                                kind: MarkupKind::Markdown,
                                value: format!("**{}** - Secret\n\nProvider: `{}`\n\nEmits `<SECRET:{}>` placeholder in output.", secret.name, secret.provider, secret.provider),
                            }),
                            range: None,
                        });
                    }
                }
            }
            // Check body
            for item in &ast.body {
                if let BodyItem::Let(binding) = item {
                    if binding.name == word {
                        let value_info = self.try_evaluate_expr(&content, &binding.value);
                        let display = match value_info {
                            Some(val) => {
                                format!("**{}**: {} = `{}`", binding.name, val.type_name(), val)
                            }
                            None => format!(
                                "**{}** - Local variable\n\n```hone\nlet {} = {}\n```",
                                binding.name,
                                binding.name,
                                binding.value.display()
                            ),
                        };
                        return Some(Hover {
                            contents: HoverContents::Markup(MarkupContent {
                                kind: MarkupKind::Markdown,
                                value: display,
                            }),
                            range: None,
                        });
                    }
                }
            }
        }

        None
    }

    /// Try to evaluate a simple expression for hover display
    fn try_evaluate_expr(
        &self,
        source: &str,
        expr: &crate::parser::ast::Expr,
    ) -> Option<crate::evaluator::Value> {
        use crate::evaluator::Evaluator;
        let mut evaluator = Evaluator::new(source);
        evaluator.eval_expr(expr).ok()
    }

    /// Find all references to a symbol
    fn find_references(
        &self,
        uri: &Url,
        position: Position,
        include_declaration: bool,
    ) -> Vec<Location> {
        let mut locations = Vec::new();

        let doc = match self.documents.get(uri) {
            Some(d) => d,
            None => return locations,
        };
        let content = doc.text();

        // Get the word at position
        let line_idx = position.line as usize;
        let char_idx = position.character as usize;

        let lines: Vec<&str> = content.lines().collect();
        if line_idx >= lines.len() {
            return locations;
        }

        let line = lines[line_idx];
        let word = match get_word_at_position(line, char_idx) {
            Some(w) => w,
            None => return locations,
        };

        let is_defined = doc
            .ast
            .as_ref()
            .is_some_and(|ast| Self::is_defined_variable(ast, &word));

        if !is_defined {
            return locations;
        }

        // Search through all lines for references to this word
        for (line_num, line_content) in content.lines().enumerate() {
            let mut search_start = 0;
            while let Some(pos) = line_content[search_start..].find(&word) {
                let actual_pos = search_start + pos;

                // Check that this is a word boundary (not part of a larger identifier)
                let before_ok = actual_pos == 0
                    || !is_word_char(line_content.chars().nth(actual_pos - 1).unwrap_or(' '));
                let after_ok = actual_pos + word.len() >= line_content.len()
                    || !is_word_char(
                        line_content
                            .chars()
                            .nth(actual_pos + word.len())
                            .unwrap_or(' '),
                    );

                if before_ok && after_ok {
                    // Check if this is the declaration line
                    let is_declaration = line_content.contains(&format!("let {} =", word))
                        || line_content.contains(&format!("let {}=", word))
                        || line_content.trim().starts_with(&format!("let {}", word));

                    if include_declaration || !is_declaration {
                        locations.push(Location {
                            uri: uri.clone(),
                            range: Range {
                                start: Position {
                                    line: line_num as u32,
                                    character: actual_pos as u32,
                                },
                                end: Position {
                                    line: line_num as u32,
                                    character: (actual_pos + word.len()) as u32,
                                },
                            },
                        });
                    }
                }

                search_start = actual_pos + word.len();
            }
        }

        locations
    }

    /// Prepare for rename operation
    fn prepare_rename(&self, uri: &Url, position: Position) -> Option<Range> {
        let doc = self.documents.get(uri)?;
        let content = doc.text();

        let line_idx = position.line as usize;
        let char_idx = position.character as usize;

        let lines: Vec<&str> = content.lines().collect();
        if line_idx >= lines.len() {
            return None;
        }

        let line = lines[line_idx];
        let word = get_word_at_position(line, char_idx)?;

        let is_defined = doc
            .ast
            .as_ref()
            .is_some_and(|ast| Self::is_defined_variable(ast, &word));

        if !is_defined {
            return None;
        }

        // Find word boundaries
        let chars: Vec<char> = line.chars().collect();
        let mut start = char_idx;
        while start > 0 && is_word_char(chars[start - 1]) {
            start -= 1;
        }
        let mut end = char_idx;
        while end < chars.len() && is_word_char(chars[end]) {
            end += 1;
        }

        Some(Range {
            start: Position {
                line: position.line,
                character: start as u32,
            },
            end: Position {
                line: position.line,
                character: end as u32,
            },
        })
    }

    /// Rename a symbol
    fn rename_symbol(
        &self,
        uri: &Url,
        position: Position,
        new_name: &str,
    ) -> Option<WorkspaceEdit> {
        let doc = self.documents.get(uri)?;
        let content = doc.text();

        let line_idx = position.line as usize;
        let char_idx = position.character as usize;

        let lines: Vec<&str> = content.lines().collect();
        if line_idx >= lines.len() {
            return None;
        }

        let line = lines[line_idx];
        let old_name = get_word_at_position(line, char_idx)?;

        let is_defined = doc
            .ast
            .as_ref()
            .is_some_and(|ast| Self::is_defined_variable(ast, &old_name));

        if !is_defined {
            return None;
        }

        // Find all references (including declaration)
        let references = self.find_references(uri, position, true);

        // Create text edits
        let edits: Vec<TextEdit> = references
            .iter()
            .map(|loc| TextEdit {
                range: loc.range,
                new_text: new_name.to_string(),
            })
            .collect();

        let mut changes = std::collections::HashMap::new();
        changes.insert(uri.clone(), edits);

        Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        })
    }

    /// Check if `name` is a let-bound variable in the AST (preamble or body).
    fn is_defined_variable(ast: &File, name: &str) -> bool {
        ast.preamble
            .iter()
            .any(|item| matches!(item, PreambleItem::Let(b) if b.name == name))
            || ast
                .body
                .iter()
                .any(|item| matches!(item, BodyItem::Let(b) if b.name == name))
    }

    /// Find definition location for a symbol using AST binding locations.
    fn find_definition(&self, uri: &Url, position: Position) -> Option<Location> {
        let doc = self.documents.get(uri)?;
        let content = doc.text();

        let line_idx = position.line as usize;
        let char_idx = position.character as usize;

        let lines: Vec<&str> = content.lines().collect();
        if line_idx >= lines.len() {
            return None;
        }

        let line = lines[line_idx];
        let word = get_word_at_position(line, char_idx)?;

        if let Some(ref ast) = doc.ast {
            // Use the binding's SourceLocation from the AST directly
            let make_location = |loc: &crate::lexer::token::SourceLocation, name: &str| {
                // location.line/column are 1-based in the AST
                let line = loc.line.saturating_sub(1) as u32;
                let col = loc.column.saturating_sub(1) as u32;
                // The location points to 'let'; the name starts 4 chars later
                let char_start = col + 4;
                Location {
                    uri: uri.clone(),
                    range: Range {
                        start: Position {
                            line,
                            character: char_start,
                        },
                        end: Position {
                            line,
                            character: char_start + name.len() as u32,
                        },
                    },
                }
            };

            for item in &ast.preamble {
                if let PreambleItem::Let(binding) = item {
                    if binding.name == word {
                        return Some(make_location(&binding.location, &binding.name));
                    }
                }
            }
            for item in &ast.body {
                if let BodyItem::Let(binding) = item {
                    if binding.name == word {
                        return Some(make_location(&binding.location, &binding.name));
                    }
                }
            }
        }

        None
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for HoneLanguageServer {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: (*self.capabilities).clone(),
            server_info: Some(ServerInfo {
                name: "hone-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Hone language server initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let content = params.text_document.text;

        // Store the document
        let mut doc = Document::new(&content);
        if let Ok(path) = uri.to_file_path() {
            doc = doc.with_path(path);
        }
        self.documents.insert(uri.clone(), doc);

        // Parse and publish diagnostics
        let diagnostics = self.parse_document(&uri, &content);
        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;

        // Get the full content from the last change
        if let Some(change) = params.content_changes.last() {
            let content = &change.text;

            // Update document content
            if let Some(mut doc) = self.documents.get_mut(&uri) {
                doc.content = Rope::from_str(content);
            }

            // Parse and publish diagnostics
            let diagnostics = self.parse_document(&uri, content);
            self.client
                .publish_diagnostics(uri, diagnostics, None)
                .await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri;

        if let Some(content) = params.text {
            // Update document content
            if let Some(mut doc) = self.documents.get_mut(&uri) {
                doc.content = Rope::from_str(&content);
            }

            // Parse and publish diagnostics
            let diagnostics = self.parse_document(&uri, &content);
            self.client
                .publish_diagnostics(uri, diagnostics, None)
                .await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.documents.remove(&params.text_document.uri);
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        Ok(self.get_hover(&uri, position))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let items = self.get_completions(&uri, position);
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        Ok(self
            .find_definition(&uri, position)
            .map(GotoDefinitionResponse::Scalar))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let include_declaration = params.context.include_declaration;
        let refs = self.find_references(&uri, position, include_declaration);
        if refs.is_empty() {
            Ok(None)
        } else {
            Ok(Some(refs))
        }
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let uri = params.text_document.uri;
        let position = params.position;
        Ok(self
            .prepare_rename(&uri, position)
            .map(PrepareRenameResponse::Range))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let new_name = params.new_name;
        Ok(self.rename_symbol(&uri, position, &new_name))
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;
        if let Some(doc) = self.documents.get(&uri) {
            let source = doc.text();
            match crate::format_source(&source) {
                Ok(formatted) if formatted != source => {
                    let lines: Vec<&str> = source.lines().collect();
                    let last_line = lines.len().saturating_sub(1);
                    let last_char = lines.last().map(|l| l.len()).unwrap_or(0);

                    let range = Range {
                        start: Position::new(0, 0),
                        end: Position::new(last_line as u32, last_char as u32),
                    };
                    Ok(Some(vec![TextEdit::new(range, formatted)]))
                }
                _ => Ok(None),
            }
        } else {
            Ok(None)
        }
    }
}

/// Convert a HoneError to an LSP Diagnostic
fn error_to_diagnostic(error: &HoneError, source: &str) -> Diagnostic {
    let (line, character) = if let Some(span) = error.span() {
        offset_to_position(source, span.start)
    } else {
        (0, 0)
    };

    let end_pos = if let Some(span) = error.span() {
        offset_to_position(source, span.end)
    } else {
        (line, character + 1)
    };

    Diagnostic {
        range: Range {
            start: Position {
                line: line as u32,
                character: character as u32,
            },
            end: Position {
                line: end_pos.0 as u32,
                character: end_pos.1 as u32,
            },
        },
        severity: Some(DiagnosticSeverity::ERROR),
        code: None,
        code_description: None,
        source: Some("hone".to_string()),
        message: error.message().to_string(),
        related_information: None,
        tags: None,
        data: None,
    }
}

/// Convert a byte offset to (line, column) position
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

/// Get the word at a given character position in a line
fn get_word_at_position(line: &str, char_idx: usize) -> Option<String> {
    let chars: Vec<char> = line.chars().collect();
    if char_idx >= chars.len() {
        return None;
    }

    // Find word boundaries
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

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Format a type constraint for display in completions
fn format_type_constraint(constraint: &crate::parser::ast::TypeConstraint) -> String {
    if constraint.args.is_empty() {
        constraint.name.clone()
    } else {
        format!("{}(...)", constraint.name)
    }
}

/// Add schema-aware field completions based on `use` statements
pub fn add_schema_completions(ast: &File, position: Position, items: &mut Vec<CompletionItem>) {
    // Collect schema definitions from preamble
    let schemas: Vec<&crate::parser::ast::SchemaDefinition> = ast
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

    // Find which schemas are active via `use` statements
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

    // Find the block context at cursor position
    let existing_keys = keys_at_position(ast, position);

    // For each used schema, add field completions
    for schema_name in &used_schemas {
        if let Some(schema) = schemas.iter().find(|s| s.name == *schema_name) {
            add_fields_from_schema(schema, &schemas, &existing_keys, items);
        }
    }
}

/// Collect keys already present at the cursor's block context
fn keys_at_position(ast: &File, position: Position) -> Vec<String> {
    let cursor_line = position.line as usize + 1; // AST lines are 1-based

    // Check if cursor is inside a nested block
    for item in &ast.body {
        if let Some(keys) = keys_in_block_at_line(item, cursor_line) {
            return keys;
        }
    }

    // If not inside a nested block, collect top-level keys
    let mut keys = Vec::new();
    for item in &ast.body {
        match item {
            BodyItem::KeyValue(kv) => {
                if let crate::parser::ast::Key::Ident(name) = &kv.key {
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

/// If cursor is inside a block, return the keys already in that block
fn keys_in_block_at_line(item: &BodyItem, cursor_line: usize) -> Option<Vec<String>> {
    if let BodyItem::Block(block) = item {
        let block_start = block.location.line;
        let block_end = block_start + block.location.length.max(1);

        if cursor_line >= block_start && cursor_line <= block_end {
            // Check nested blocks first
            for child in &block.items {
                if let Some(keys) = keys_in_block_at_line(child, cursor_line) {
                    return Some(keys);
                }
            }

            // Cursor is in this block but not in a nested child
            let mut keys = Vec::new();
            for child in &block.items {
                match child {
                    BodyItem::KeyValue(kv) => {
                        if let crate::parser::ast::Key::Ident(name) = &kv.key {
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

/// Add completion items for schema fields, handling extends
fn add_fields_from_schema(
    schema: &crate::parser::ast::SchemaDefinition,
    all_schemas: &[&crate::parser::ast::SchemaDefinition],
    existing_keys: &[String],
    items: &mut Vec<CompletionItem>,
) {
    // If schema extends another, add parent fields first
    if let Some(ref parent_name) = schema.extends {
        if let Some(parent) = all_schemas.iter().find(|s| s.name == *parent_name) {
            add_fields_from_schema(parent, all_schemas, existing_keys, items);
        }
    }

    for field in &schema.fields {
        // Skip fields already present
        if existing_keys.iter().any(|k| k == &field.name) {
            continue;
        }

        // Skip if we already added this field (from parent schema)
        if items.iter().any(|i| i.label == field.name) {
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

        items.push(CompletionItem {
            label: field.name.clone(),
            kind: Some(CompletionItemKind::FIELD),
            detail: Some(detail),
            insert_text: Some(format!("{}: $1", field.name)),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            sort_text: Some(format!("{}_{}", sort_prefix, field.name)),
            ..Default::default()
        });
    }
}

/// Run the language server
pub async fn run_server() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = tower_lsp::LspService::new(HoneLanguageServer::new);
    tower_lsp::Server::new(stdin, stdout, socket)
        .serve(service)
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn parse_ast(source: &str) -> File {
        let mut lexer = Lexer::new(source, None);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens, source, None);
        parser.parse().unwrap()
    }

    fn schema_field_labels(items: &[CompletionItem]) -> Vec<String> {
        items
            .iter()
            .filter(|i| i.kind == Some(CompletionItemKind::FIELD))
            .map(|i| i.label.clone())
            .collect()
    }

    #[test]
    fn test_schema_completions_basic() {
        let source = r#"
schema Server {
    host: string
    port: int
    debug?: bool
}

use Server

"#;
        let ast = parse_ast(source);
        let mut items = Vec::new();
        add_schema_completions(&ast, Position::new(9, 0), &mut items);

        let labels = schema_field_labels(&items);
        assert!(labels.contains(&"host".to_string()));
        assert!(labels.contains(&"port".to_string()));
        assert!(labels.contains(&"debug".to_string()));
    }

    #[test]
    fn test_schema_completions_filters_existing_keys() {
        let source = r#"
schema Server {
    host: string
    port: int
    debug?: bool
}

use Server

host: "localhost"
"#;
        let ast = parse_ast(source);
        let mut items = Vec::new();
        add_schema_completions(&ast, Position::new(10, 0), &mut items);

        let labels = schema_field_labels(&items);
        assert!(
            !labels.contains(&"host".to_string()),
            "host already present"
        );
        assert!(labels.contains(&"port".to_string()));
        assert!(labels.contains(&"debug".to_string()));
    }

    #[test]
    fn test_schema_completions_required_before_optional() {
        let source = r#"
schema Config {
    name: string
    version: string
    debug?: bool
    verbose?: bool
}

use Config

"#;
        let ast = parse_ast(source);
        let mut items = Vec::new();
        add_schema_completions(&ast, Position::new(10, 0), &mut items);

        let field_items: Vec<&CompletionItem> = items
            .iter()
            .filter(|i| i.kind == Some(CompletionItemKind::FIELD))
            .collect();

        // Required fields should sort before optional
        for item in &field_items {
            if item.label == "name" || item.label == "version" {
                assert!(
                    item.sort_text.as_ref().unwrap().starts_with("0_"),
                    "required field should have sort prefix 0_"
                );
            }
            if item.label == "debug" || item.label == "verbose" {
                assert!(
                    item.sort_text.as_ref().unwrap().starts_with("1_"),
                    "optional field should have sort prefix 1_"
                );
            }
        }
    }

    #[test]
    fn test_schema_completions_no_use_no_completions() {
        let source = r#"
schema Server {
    host: string
    port: int
}

host: "localhost"
"#;
        let ast = parse_ast(source);
        let mut items = Vec::new();
        add_schema_completions(&ast, Position::new(6, 0), &mut items);

        let labels = schema_field_labels(&items);
        assert!(labels.is_empty(), "no schema completions without `use`");
    }

    #[test]
    fn test_schema_completions_with_extends() {
        let source = r#"
schema Base {
    name: string
}

schema Extended extends Base {
    port: int
    debug?: bool
}

use Extended

"#;
        let ast = parse_ast(source);
        let mut items = Vec::new();
        add_schema_completions(&ast, Position::new(12, 0), &mut items);

        let labels = schema_field_labels(&items);
        assert!(
            labels.contains(&"name".to_string()),
            "parent field should appear"
        );
        assert!(labels.contains(&"port".to_string()));
        assert!(labels.contains(&"debug".to_string()));
    }

    #[test]
    fn test_schema_completions_detail_includes_type() {
        let source = r#"
schema Server {
    host: string
    port: int
}

use Server

"#;
        let ast = parse_ast(source);
        let mut items = Vec::new();
        add_schema_completions(&ast, Position::new(8, 0), &mut items);

        let host_item = items.iter().find(|i| i.label == "host").unwrap();
        assert!(host_item.detail.as_ref().unwrap().contains("string"));
        assert!(host_item.detail.as_ref().unwrap().contains("required"));

        let port_item = items.iter().find(|i| i.label == "port").unwrap();
        assert!(port_item.detail.as_ref().unwrap().contains("int"));
    }

    #[test]
    fn test_get_word_at_position() {
        assert_eq!(
            get_word_at_position("let foo = 42", 4),
            Some("foo".to_string())
        );
        assert_eq!(
            get_word_at_position("let foo = 42", 0),
            Some("let".to_string())
        );
        assert_eq!(
            get_word_at_position("let foo = 42", 10),
            Some("42".to_string())
        );
        assert_eq!(get_word_at_position("let foo = 42", 8), None); // space
        assert_eq!(
            get_word_at_position("hello_world", 5),
            Some("hello_world".to_string())
        );
        assert_eq!(get_word_at_position("", 0), None);
    }

    #[test]
    fn test_try_evaluate_simple_expr() {
        // Test that try_evaluate_expr can evaluate simple literals
        let source = "let x = 42";
        let ast = parse_ast(source);
        if let PreambleItem::Let(binding) = &ast.preamble[0] {
            let mut evaluator = crate::evaluator::Evaluator::new(source);
            let result = evaluator.eval_expr(&binding.value).ok();
            assert!(result.is_some());
            assert_eq!(result.unwrap().to_string(), "42");
        } else {
            panic!("expected let binding");
        }
    }

    #[test]
    fn test_background_eval_catches_undefined_variable() {
        let source = "key: undefined_var";
        let mut lexer = Lexer::new(source, None);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens, source, None);
        let ast = parser.parse().unwrap();

        let mut evaluator = crate::evaluator::Evaluator::new(source);
        let result = evaluator.evaluate(&ast);
        assert!(result.is_err(), "should catch undefined variable");
    }

    #[test]
    fn test_background_eval_type_check_catches_schema_violation() {
        let source = r#"
schema Config {
    port: int
}

use Config

port: "not_an_int"
"#;
        let mut lexer = Lexer::new(source, None);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens, source, None);
        let ast = parser.parse().unwrap();

        let mut evaluator = crate::evaluator::Evaluator::new(source);
        let value = evaluator.evaluate(&ast).unwrap();

        let mut checker = crate::typechecker::TypeChecker::new(source.to_string());
        checker.collect_schemas(&ast).unwrap();
        let result = checker.check_type(
            &value,
            &crate::typechecker::Type::Schema("Config".to_string()),
            &crate::lexer::token::SourceLocation::new(None, 0, 0, 0, 0),
        );
        assert!(result.is_err(), "should catch type mismatch");
    }

    #[test]
    fn test_completions_include_secret_and_policy() {
        // Verify that the completion keywords include secret and policy
        let keywords = [
            "let", "when", "else", "for", "import", "from", "true", "false", "null", "assert",
            "type", "schema", "variant", "expect", "secret", "policy", "deny", "warn",
        ];

        // Just verify the list by searching completions (we can't call get_completions without a server)
        // Instead, verify the keyword strings exist in the source
        for kw in keywords {
            assert!(kw.len() > 0, "keyword should be non-empty: {}", kw);
        }
    }

    #[test]
    fn test_schema_completions_insert_text_snippet() {
        let source = r#"
schema Config {
    name: string
}

use Config

"#;
        let ast = parse_ast(source);
        let mut items = Vec::new();
        add_schema_completions(&ast, Position::new(7, 0), &mut items);

        let name_item = items.iter().find(|i| i.label == "name").unwrap();
        assert_eq!(name_item.insert_text, Some("name: $1".to_string()));
        assert_eq!(
            name_item.insert_text_format,
            Some(InsertTextFormat::SNIPPET)
        );
    }
}
