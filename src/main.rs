#![allow(unused_assignments)]

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// Hone Configuration Language Compiler
///
/// A configuration language that compiles to JSON and YAML.
#[derive(Parser)]
#[command(name = "hone")]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile Hone source to JSON or YAML
    Compile {
        /// Source file to compile
        file: PathBuf,

        /// Output file (extension determines format: .yaml, .json)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Force output format: yaml, json
        #[arg(short, long)]
        format: Option<String>,

        /// Inject variable into args.* namespace (can be used multiple times)
        #[arg(long = "set", value_parser = parse_key_value)]
        set: Vec<(String, String)>,

        /// Read value from file
        #[arg(long = "set-file", value_parser = parse_key_value)]
        set_file: Vec<(String, String)>,

        /// Force value as string (no type inference)
        #[arg(long = "set-string", value_parser = parse_key_value)]
        set_string: Vec<(String, String)>,

        /// Print output to stdout, don't write files
        #[arg(long)]
        dry_run: bool,

        /// Treat warnings as errors
        #[arg(long)]
        strict: bool,

        /// Suppress warnings
        #[arg(long)]
        quiet: bool,

        /// Output each ---name document to a separate file in this directory
        #[arg(long)]
        output_dir: Option<PathBuf>,

        /// Allow env() and file() builtins (non-deterministic)
        #[arg(long)]
        allow_env: bool,

        /// Select variant case (can be used multiple times, format: name=case)
        #[arg(long = "variant", value_parser = parse_key_value)]
        variants: Vec<(String, String)>,

        /// Disable build cache
        #[arg(long)]
        no_cache: bool,

        /// Secret handling mode: placeholder (default), error, env
        #[arg(long, default_value = "placeholder")]
        secrets_mode: String,

        /// Skip all policy checks
        #[arg(long)]
        ignore_policy: bool,
    },

    /// Validate source without emitting output
    Check {
        /// Source file to check
        file: PathBuf,

        /// Inject variable (required if file uses args.*)
        #[arg(long = "set", value_parser = parse_key_value)]
        set: Vec<(String, String)>,

        /// Validate against specific schema
        #[arg(long)]
        schema: Option<String>,

        /// Allow env() and file() builtins (non-deterministic)
        #[arg(long)]
        allow_env: bool,

        /// Select variant case (can be used multiple times, format: name=case)
        #[arg(long = "variant", value_parser = parse_key_value)]
        variants: Vec<(String, String)>,
    },

    /// Format source files
    Fmt {
        /// Files to format
        files: Vec<PathBuf>,

        /// Check if files are formatted (exit 1 if not)
        #[arg(long)]
        check: bool,

        /// Show diff of formatting changes
        #[arg(long)]
        diff: bool,

        /// Write formatted output back to files
        #[arg(short, long)]
        write: bool,
    },

    /// Compare compilation outputs (different args or git refs)
    Diff {
        /// Source file
        file: PathBuf,

        /// Arguments for left side ("key=val,key=val")
        #[arg(long)]
        left: Option<String>,

        /// Arguments for right side ("key=val,key=val")
        #[arg(long)]
        right: Option<String>,

        /// Compare against a git ref (branch, tag, or commit)
        #[arg(long)]
        base: Option<String>,

        /// Compare current file vs version at a git ref
        #[arg(long)]
        since: Option<String>,

        /// Detect moved keys (same value at different paths)
        #[arg(long)]
        detect_moves: bool,

        /// Annotate diffs with git blame information
        #[arg(long)]
        blame: bool,

        /// Output format: text (default), json
        #[arg(long, default_value = "text")]
        format: String,
    },

    /// Convert YAML/JSON to Hone source
    Import {
        /// YAML or JSON file to convert
        file: PathBuf,

        /// Output Hone file
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Attempt to identify repeated values as variables
        #[arg(long)]
        extract_vars: bool,

        /// Split multi-doc YAML into separate files
        #[arg(long)]
        split_docs: bool,
    },

    /// Start Language Server Protocol server
    Lsp {
        /// Use stdio transport (default)
        #[arg(long)]
        stdio: bool,

        /// Use TCP socket transport
        #[arg(long)]
        socket: Option<u16>,
    },

    /// Internal: Lex a file and print tokens (for debugging)
    #[command(hide = true)]
    Lex {
        /// Source file to lex
        file: PathBuf,
    },

    /// Internal: Parse a file and print AST (for debugging)
    #[command(hide = true)]
    Parse {
        /// Source file to parse
        file: PathBuf,
    },

    /// Internal: Resolve imports and print dependency graph (for debugging)
    #[command(hide = true)]
    Resolve {
        /// Source file to resolve
        file: PathBuf,
    },

    /// Visualize import dependency graph
    Graph {
        /// Source file to analyze
        file: PathBuf,

        /// Output format: text (default), dot, json
        #[arg(short, long, default_value = "text")]
        format: String,

        /// Output file (default: stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Manage the build cache
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },

    /// Generate Hone schema definitions from JSON Schema
    Typegen {
        /// JSON Schema file to convert
        file: PathBuf,

        /// Output file (default: stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Internal: Evaluate inline source (for debugging/testing)
    #[command(hide = true)]
    Eval {
        /// Hone source code to evaluate
        source: String,

        /// Output format: json, yaml
        #[arg(short, long, default_value = "json")]
        format: String,
    },
}

#[derive(Subcommand)]
enum CacheAction {
    /// Remove all cached build results
    Clean {
        /// Only remove entries older than duration (e.g., 7d, 24h, 30m)
        #[arg(long)]
        older_than: Option<String>,
    },
}

/// Parse a key=value pair
fn parse_key_value(s: &str) -> Result<(String, String), String> {
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid KEY=value: no '=' found in '{}'", s))?;
    Ok((s[..pos].to_string(), s[pos + 1..].to_string()))
}

fn main() -> ExitCode {
    // Set up miette for nice error output
    miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .terminal_links(true)
                .unicode(true)
                .context_lines(2)
                .tab_width(4)
                .build(),
        )
    }))
    .ok();

    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Compile {
            file,
            output,
            format,
            set,
            set_file,
            set_string,
            dry_run,
            strict,
            quiet,
            output_dir,
            allow_env,
            variants,
            no_cache,
            secrets_mode,
            ignore_policy,
        } => cmd_compile(
            file,
            output,
            format,
            set,
            set_file,
            set_string,
            dry_run,
            strict,
            quiet,
            output_dir,
            allow_env,
            variants,
            no_cache,
            secrets_mode,
            ignore_policy,
        ),
        Commands::Check {
            file,
            set,
            schema,
            allow_env,
            variants,
        } => cmd_check(file, set, schema, allow_env, variants),
        Commands::Fmt {
            files,
            check,
            diff,
            write,
        } => cmd_fmt(files, check, diff, write),
        Commands::Diff {
            file,
            left,
            right,
            base,
            since,
            detect_moves,
            blame,
            format,
        } => cmd_diff(file, left, right, base, since, detect_moves, blame, format),
        Commands::Import {
            file,
            output,
            extract_vars,
            split_docs,
        } => cmd_import(file, output, extract_vars, split_docs),
        Commands::Graph {
            file,
            format,
            output,
        } => cmd_graph(file, format, output),
        Commands::Cache { action } => cmd_cache(action),
        Commands::Lsp { stdio, socket } => cmd_lsp(stdio, socket),
        Commands::Lex { file } => cmd_lex(file),
        Commands::Parse { file } => cmd_parse(file),
        Commands::Resolve { file } => cmd_resolve(file),
        Commands::Typegen { file, output } => cmd_typegen(file, output),
        Commands::Eval { source, format } => cmd_eval(source, format),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            let exit_code = match &e {
                // I/O errors
                hone::HoneError::IoError { .. } => ExitCode::from(3),
                // All compilation errors
                _ => ExitCode::from(1),
            };
            eprintln!("{:?}", miette::Report::new(e));
            exit_code
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn cmd_compile(
    file: PathBuf,
    output: Option<PathBuf>,
    format: Option<String>,
    set: Vec<(String, String)>,
    set_file: Vec<(String, String)>,
    set_string: Vec<(String, String)>,
    dry_run: bool,
    strict: bool,
    quiet: bool,
    output_dir: Option<PathBuf>,
    allow_env: bool,
    variants: Vec<(String, String)>,
    no_cache: bool,
    secrets_mode: String,
    ignore_policy: bool,
) -> hone::HoneResult<()> {
    // Determine output format
    let output_format = if let Some(ref fmt) = format {
        hone::OutputFormat::parse(fmt).ok_or_else(|| {
            hone::HoneError::io_error(format!(
                "unknown output format '{}'. Use: json, yaml, toml, dotenv",
                fmt
            ))
        })?
    } else if let Some(ref out) = output {
        match out.extension().and_then(|e| e.to_str()) {
            Some("yaml") | Some("yml") => hone::OutputFormat::Yaml,
            Some("json") => hone::OutputFormat::JsonPretty,
            Some("toml") => hone::OutputFormat::Toml,
            Some("env") => hone::OutputFormat::Dotenv,
            _ => hone::OutputFormat::JsonPretty,
        }
    } else if output_dir.is_some() {
        // Default to YAML for multi-file output (common for K8s)
        hone::OutputFormat::Yaml
    } else {
        hone::OutputFormat::JsonPretty
    };

    // If output_dir is specified, do multi-file output (no caching for multi-file)
    if let Some(ref dir) = output_dir {
        return cmd_compile_multi(
            &file,
            dir,
            output_format,
            dry_run,
            quiet,
            strict,
            &set,
            &set_file,
            &set_string,
            allow_env,
            &variants,
            &secrets_mode,
            ignore_policy,
        );
    }

    // Check for stdin
    let is_stdin = file.to_str() == Some("-") || file.to_str() == Some("/dev/stdin");

    // Set up base_dir early (needed for import resolution during cache hashing)
    let base_dir = if is_stdin {
        std::env::current_dir()
            .map_err(|e| hone::HoneError::io_error(format!("failed to get cwd: {}", e)))?
    } else {
        let canonical = file.canonicalize().map_err(|e| {
            hone::HoneError::io_error(format!("failed to resolve path {}: {}", file.display(), e))
        })?;
        canonical
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf()
    };

    // Try cache for non-stdin, non-env builds
    let use_cache = !no_cache && !is_stdin && !allow_env;
    let cache = if use_cache {
        hone::cache::BuildCache::new()
    } else {
        None
    };

    // Compute cache key if caching is enabled
    let format_str = match output_format {
        hone::OutputFormat::Json => "json",
        hone::OutputFormat::JsonPretty => "json-pretty",
        hone::OutputFormat::Yaml => "yaml",
        hone::OutputFormat::Toml => "toml",
        hone::OutputFormat::Dotenv => "dotenv",
    };

    // Collect source hashes from ALL files in the import closure (not just root)
    let source_hashes: Vec<String> = if use_cache && !is_stdin {
        let mut resolver = hone::ImportResolver::new(&base_dir);
        let canonical = file.canonicalize().map_err(|e| {
            hone::HoneError::io_error(format!("failed to resolve path {}: {}", file.display(), e))
        })?;
        match resolver.resolve(&canonical) {
            Ok(_) => {
                match resolver.topological_order(&canonical) {
                    Ok(files) => files
                        .iter()
                        .filter_map(|f| {
                            std::fs::read_to_string(&f.path)
                                .ok()
                                .map(|content| hone::cache::CacheKey::hash_string(&content))
                        })
                        .collect(),
                    Err(_) => {
                        // If we can't resolve imports, just hash the root file
                        match std::fs::read_to_string(&file) {
                            Ok(source) => vec![hone::cache::CacheKey::hash_string(&source)],
                            Err(_) => vec![],
                        }
                    }
                }
            }
            Err(_) => {
                // If resolve fails, just hash root file (compilation will fail too)
                match std::fs::read_to_string(&file) {
                    Ok(source) => vec![hone::cache::CacheKey::hash_string(&source)],
                    Err(_) => vec![],
                }
            }
        }
    } else if !is_stdin {
        match std::fs::read_to_string(&file) {
            Ok(source) => vec![hone::cache::CacheKey::hash_string(&source)],
            Err(_) => vec![],
        }
    } else {
        vec![]
    };

    let cache_key = if let Some(ref _cache) = cache {
        if !source_hashes.is_empty() {
            let variant_map: std::collections::HashMap<String, String> =
                variants.iter().cloned().collect();
            let args_hash = if has_args(&set, &set_file, &set_string) {
                let args_str = format!("{:?}{:?}{:?}", set, set_file, set_string);
                Some(hone::cache::CacheKey::hash_string(&args_str))
            } else {
                None
            };
            Some(hone::cache::CacheKey::compute(
                &source_hashes,
                &variant_map,
                args_hash.as_deref(),
                format_str,
                env!("CARGO_PKG_VERSION"),
            ))
        } else {
            None
        }
    } else {
        None
    };

    // Check cache
    if let (Some(ref cache), Some(ref key)) = (&cache, &cache_key) {
        if let Some(cached) = cache.get(key) {
            if dry_run || output.is_none() {
                println!("{}", cached.output);
            } else if let Some(out_path) = output.as_ref() {
                std::fs::write(out_path, &cached.output).map_err(|e| {
                    hone::HoneError::io_error(format!(
                        "failed to write {}: {}",
                        out_path.display(),
                        e
                    ))
                })?;
                eprintln!("Wrote {}", out_path.display());
            }
            return Ok(());
        }
    }

    let mut compiler = hone::Compiler::new(&base_dir);
    compiler.set_allow_env(allow_env);
    compiler.set_ignore_policies(ignore_policy);
    if !variants.is_empty() {
        let variant_map: std::collections::HashMap<String, String> = variants.into_iter().collect();
        compiler.set_variants(variant_map);
    }
    if has_args(&set, &set_file, &set_string) {
        let args = hone::build_args_object(&set, &set_file, &set_string)?;
        compiler.set_args(args);
    }

    let value = if is_stdin {
        use std::io::Read;
        let mut source = String::new();
        std::io::stdin()
            .read_to_string(&mut source)
            .map_err(|e| hone::HoneError::io_error(format!("failed to read stdin: {}", e)))?;
        compiler.compile_source(&source)?
    } else {
        let canonical = file.canonicalize().map_err(|e| {
            hone::HoneError::io_error(format!("failed to resolve path {}: {}", file.display(), e))
        })?;
        compiler.compile(&canonical)?
    };

    // Handle warnings
    let warnings = compiler.warnings();
    if !warnings.is_empty() {
        if strict {
            for w in warnings {
                eprintln!("warning: {}", w.message);
            }
            return Err(hone::HoneError::compilation_error(format!(
                "{} warning(s) treated as errors (--strict)",
                warnings.len()
            )));
        }
        if !quiet {
            for w in warnings {
                eprintln!("warning: {}", w.message);
            }
        }
    }

    // Handle secrets mode
    let value = match secrets_mode.as_str() {
        "placeholder" => value, // default: leave <SECRET:...> placeholders
        "error" => {
            // Check if any secret placeholders remain in output
            let secrets = find_secret_placeholders(&value, "");
            if !secrets.is_empty() {
                return Err(hone::HoneError::io_error(format!(
                    "secret placeholders found in output (--secrets-mode=error): {}",
                    secrets.join(", ")
                )));
            }
            value
        }
        "env" => {
            if !allow_env {
                return Err(hone::HoneError::io_error(
                    "--secrets-mode=env requires --allow-env flag".to_string(),
                ));
            }
            resolve_env_secrets(value)
        }
        other => {
            return Err(hone::HoneError::io_error(format!(
                "unknown secrets mode '{}': expected placeholder, error, or env",
                other
            )));
        }
    };

    let result = hone::emit(&value, output_format)?;

    // Store in cache
    if let (Some(ref cache), Some(ref key)) = (&cache, &cache_key) {
        let cached = hone::cache::CachedResult::new(result.clone(), format_str, file.to_str());
        // Ignore cache write failures
        let _ = cache.put(key, &cached);
    }

    if dry_run || output.is_none() {
        println!("{}", result);
    } else if let Some(out_path) = output {
        std::fs::write(&out_path, &result).map_err(|e| {
            hone::HoneError::io_error(format!("failed to write {}: {}", out_path.display(), e))
        })?;
        eprintln!("Wrote {}", out_path.display());
    }

    Ok(())
}

fn has_args(
    set: &[(String, String)],
    set_file: &[(String, String)],
    set_string: &[(String, String)],
) -> bool {
    !set.is_empty() || !set_file.is_empty() || !set_string.is_empty()
}

/// Find all secret placeholders in a value tree, returning their paths
fn find_secret_placeholders(value: &hone::Value, prefix: &str) -> Vec<String> {
    let mut found = Vec::new();
    match value {
        hone::Value::String(s) if s.starts_with("<SECRET:") && s.ends_with('>') => {
            found.push(if prefix.is_empty() {
                s.clone()
            } else {
                format!("{} ({})", prefix, s)
            });
        }
        hone::Value::Object(obj) => {
            for (k, v) in obj {
                let path = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{}.{}", prefix, k)
                };
                found.extend(find_secret_placeholders(v, &path));
            }
        }
        hone::Value::Array(arr) => {
            for (i, v) in arr.iter().enumerate() {
                let path = format!("{}[{}]", prefix, i);
                found.extend(find_secret_placeholders(v, &path));
            }
        }
        _ => {}
    }
    found
}

/// Resolve env:-prefixed secrets from environment variables
fn resolve_env_secrets(value: hone::Value) -> hone::Value {
    match value {
        hone::Value::String(s) if s.starts_with("<SECRET:env:") && s.ends_with('>') => {
            let env_name = &s[12..s.len() - 1]; // strip "<SECRET:env:" and ">"
            match std::env::var(env_name) {
                Ok(val) => hone::Value::String(val),
                Err(_) => hone::Value::String(s), // leave placeholder if env var not found
            }
        }
        hone::Value::Object(obj) => {
            let resolved: indexmap::IndexMap<String, hone::Value> = obj
                .into_iter()
                .map(|(k, v)| (k, resolve_env_secrets(v)))
                .collect();
            hone::Value::Object(resolved)
        }
        hone::Value::Array(arr) => {
            let resolved: Vec<hone::Value> = arr.into_iter().map(resolve_env_secrets).collect();
            hone::Value::Array(resolved)
        }
        other => other,
    }
}

/// Apply secrets mode to a value (shared by single and multi-file output)
fn apply_secrets_mode(value: &hone::Value, secrets_mode: &str) -> hone::HoneResult<hone::Value> {
    match secrets_mode {
        "placeholder" => Ok(value.clone()),
        "error" => {
            let secrets = find_secret_placeholders(value, "");
            if !secrets.is_empty() {
                return Err(hone::HoneError::io_error(format!(
                    "secret placeholders found in output (--secrets-mode=error): {}",
                    secrets.join(", ")
                )));
            }
            Ok(value.clone())
        }
        "env" => Ok(resolve_env_secrets(value.clone())),
        other => Err(hone::HoneError::io_error(format!(
            "unknown secrets mode '{}': expected placeholder, error, or env",
            other
        ))),
    }
}

fn cmd_graph(file: PathBuf, format: String, output: Option<PathBuf>) -> hone::HoneResult<()> {
    let graph_format = hone::graph::GraphFormat::parse(&format).ok_or_else(|| {
        hone::HoneError::io_error(format!(
            "unknown graph format '{}'. Use: text, dot, json",
            format
        ))
    })?;

    let result = hone::graph::generate_graph(&file, graph_format)?;

    if let Some(out_path) = output {
        std::fs::write(&out_path, &result).map_err(|e| {
            hone::HoneError::io_error(format!("failed to write {}: {}", out_path.display(), e))
        })?;
        eprintln!("Wrote {}", out_path.display());
    } else {
        print!("{}", result);
    }

    Ok(())
}

fn cmd_cache(action: CacheAction) -> hone::HoneResult<()> {
    match action {
        CacheAction::Clean { older_than } => {
            let cache = hone::cache::BuildCache::new().ok_or_else(|| {
                hone::HoneError::io_error("could not determine cache directory".to_string())
            })?;

            let count = if let Some(ref duration_str) = older_than {
                let duration = hone::cache::parse_duration(duration_str).ok_or_else(|| {
                    hone::HoneError::io_error(format!(
                        "invalid duration '{}'. Use format like 7d, 24h, 30m",
                        duration_str
                    ))
                })?;
                cache.clean_older_than(duration)?
            } else {
                cache.clean()?
            };

            eprintln!("Removed {} cached entries", count);
            Ok(())
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn cmd_compile_multi(
    file: &std::path::Path,
    output_dir: &std::path::Path,
    format: hone::OutputFormat,
    dry_run: bool,
    quiet: bool,
    strict: bool,
    set: &[(String, String)],
    set_file: &[(String, String)],
    set_string: &[(String, String)],
    allow_env: bool,
    variants: &[(String, String)],
    secrets_mode: &str,
    ignore_policy: bool,
) -> hone::HoneResult<()> {
    let canonical = file.canonicalize().map_err(|e| {
        hone::HoneError::io_error(format!("failed to resolve path {}: {}", file.display(), e))
    })?;
    let base_dir = canonical
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();

    // Set up compiler with all flags
    let mut compiler = hone::Compiler::new(&base_dir);
    compiler.set_allow_env(allow_env);
    compiler.set_ignore_policies(ignore_policy);
    if !variants.is_empty() {
        let variant_map: std::collections::HashMap<String, String> =
            variants.iter().cloned().collect();
        compiler.set_variants(variant_map);
    }
    if has_args(set, set_file, set_string) {
        let args = hone::build_args_object(set, set_file, set_string)?;
        compiler.set_args(args);
    }

    // Compile with full import resolution, variants, args, policies, etc.
    let documents = compiler.compile_multi(&canonical)?;

    // Handle warnings
    let warnings = compiler.warnings();
    if !warnings.is_empty() {
        if strict {
            for w in warnings {
                eprintln!("warning: {}", w.message);
            }
            return Err(hone::HoneError::compilation_error(format!(
                "{} warning(s) treated as errors (--strict)",
                warnings.len()
            )));
        }
        if !quiet {
            for w in warnings {
                eprintln!("warning: {}", w.message);
            }
        }
    }

    // Validate secrets mode prerequisites
    if secrets_mode == "env" && !allow_env {
        return Err(hone::HoneError::io_error(
            "--secrets-mode=env requires --allow-env flag".to_string(),
        ));
    }

    // Apply secrets mode to each document
    let documents: Vec<(Option<String>, hone::Value)> = documents
        .into_iter()
        .map(|(name, value)| {
            let value = apply_secrets_mode(&value, secrets_mode)?;
            Ok((name, value))
        })
        .collect::<hone::HoneResult<Vec<_>>>()?;

    let ext = match format {
        hone::OutputFormat::Yaml => "yaml",
        hone::OutputFormat::Toml => "toml",
        hone::OutputFormat::Dotenv => "env",
        _ => "json",
    };

    if dry_run {
        // Print all documents with separators
        let mut first = true;
        for (name, value) in documents.iter() {
            if name.is_none() && value.is_empty_object() {
                continue;
            }
            if !first {
                println!("---");
            }
            first = false;
            let result = hone::emit(value, format)?;
            if let Some(doc_name) = name {
                println!("# {}", doc_name);
            }
            println!("{}", result);
        }
    } else {
        // Create output directory
        std::fs::create_dir_all(output_dir).map_err(|e| {
            hone::HoneError::io_error(format!(
                "failed to create directory {}: {}",
                output_dir.display(),
                e
            ))
        })?;

        for (i, (name, value)) in documents.iter().enumerate() {
            if name.is_none() && value.is_empty_object() {
                continue;
            }

            let filename = match name {
                Some(n) => format!("{}.{}", n, ext),
                None if i == 0 => format!("main.{}", ext),
                None => format!("doc{}.{}", i, ext),
            };

            let out_path = output_dir.join(&filename);
            let result = hone::emit(value, format)?;

            std::fs::write(&out_path, &result).map_err(|e| {
                hone::HoneError::io_error(format!("failed to write {}: {}", out_path.display(), e))
            })?;
            if !quiet {
                eprintln!("Wrote {}", out_path.display());
            }
        }
    }

    Ok(())
}

fn cmd_check(
    file: PathBuf,
    set: Vec<(String, String)>,
    schema: Option<String>,
    allow_env: bool,
    variants: Vec<(String, String)>,
) -> hone::HoneResult<()> {
    // Check for stdin
    let is_stdin = file.to_str() == Some("-") || file.to_str() == Some("/dev/stdin");

    let base_dir = if is_stdin {
        std::env::current_dir()
            .map_err(|e| hone::HoneError::io_error(format!("failed to get cwd: {}", e)))?
    } else {
        let canonical = file.canonicalize().map_err(|e| {
            hone::HoneError::io_error(format!("failed to resolve path {}: {}", file.display(), e))
        })?;
        canonical
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf()
    };

    let mut compiler = hone::Compiler::new(&base_dir);
    compiler.set_allow_env(allow_env);
    if !variants.is_empty() {
        let variant_map: std::collections::HashMap<String, String> = variants.into_iter().collect();
        compiler.set_variants(variant_map);
    }

    let has_args = !set.is_empty();
    if has_args {
        let args = hone::build_args_object(&set, &[], &[])?;
        compiler.set_args(args);
    }

    let value = if is_stdin {
        use std::io::Read;
        let mut source = String::new();
        std::io::stdin()
            .read_to_string(&mut source)
            .map_err(|e| hone::HoneError::io_error(format!("failed to read stdin: {}", e)))?;
        compiler.compile_source(&source)?
    } else {
        let canonical = file.canonicalize().map_err(|e| {
            hone::HoneError::io_error(format!("failed to resolve path {}: {}", file.display(), e))
        })?;
        compiler.compile(&canonical)?
    };

    // If --schema is provided, validate against it explicitly
    if let Some(ref schema_name) = schema {
        if !is_stdin {
            hone::validate_against_schema(&file, &value, schema_name)?;
        }
    }

    if is_stdin {
        eprintln!("<stdin>: OK");
    } else {
        eprintln!("{}: OK", file.display());
    }
    Ok(())
}

fn cmd_fmt(files: Vec<PathBuf>, check: bool, diff: bool, write: bool) -> hone::HoneResult<()> {
    // Collect .hone files from arguments
    let mut all_files = Vec::new();
    for path in &files {
        if path.is_dir() {
            collect_hone_files(path, &mut all_files)?;
        } else {
            all_files.push(path.clone());
        }
    }

    if all_files.is_empty() {
        eprintln!("No .hone files found");
        return Ok(());
    }

    let mut any_unformatted = false;

    for file in &all_files {
        let source = std::fs::read_to_string(file).map_err(|e| {
            hone::HoneError::io_error(format!("failed to read {}: {}", file.display(), e))
        })?;

        let formatted = hone::format_source(&source)?;

        if check || diff || write {
            if source == formatted {
                continue;
            }
            any_unformatted = true;

            if check {
                eprintln!("{}: not formatted", file.display());
            } else if diff {
                eprintln!("--- {}", file.display());
                eprintln!("+++ {}", file.display());
                for change in simple_diff(&source, &formatted) {
                    eprintln!("{}", change);
                }
            } else {
                // write mode
                std::fs::write(file, &formatted).map_err(|e| {
                    hone::HoneError::io_error(format!("failed to write {}: {}", file.display(), e))
                })?;
                eprintln!("Formatted {}", file.display());
            }
        } else {
            // Default: print formatted output to stdout
            print!("{}", formatted);
        }
    }

    if check && any_unformatted {
        return Err(hone::HoneError::io_error(
            "some files are not formatted".to_string(),
        ));
    }

    Ok(())
}

/// Recursively collect all .hone files in a directory
fn collect_hone_files(dir: &PathBuf, files: &mut Vec<PathBuf>) -> hone::HoneResult<()> {
    let entries = std::fs::read_dir(dir).map_err(|e| {
        hone::HoneError::io_error(format!("failed to read directory {}: {}", dir.display(), e))
    })?;

    for entry in entries {
        let entry =
            entry.map_err(|e| hone::HoneError::io_error(format!("failed to read entry: {}", e)))?;
        let path = entry.path();
        if path.is_dir() {
            collect_hone_files(&path, files)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("hone") {
            files.push(path);
        }
    }
    Ok(())
}

/// Simple line-based diff for --diff mode
fn simple_diff(original: &str, formatted: &str) -> Vec<String> {
    let orig_lines: Vec<&str> = original.lines().collect();
    let fmt_lines: Vec<&str> = formatted.lines().collect();
    let mut changes = Vec::new();

    let max_len = orig_lines.len().max(fmt_lines.len());
    let mut i = 0;
    let mut j = 0;

    while i < orig_lines.len() || j < fmt_lines.len() {
        if i < orig_lines.len() && j < fmt_lines.len() && orig_lines[i] == fmt_lines[j] {
            i += 1;
            j += 1;
        } else if i < orig_lines.len()
            && (j >= fmt_lines.len()
                || (j + 1 < fmt_lines.len() && orig_lines.get(i) == fmt_lines.get(j + 1)))
        {
            // Line in original but not formatted (removed) - but actually in a formatter
            // this means the formatted version changed this line
            changes.push(format!("-{}", orig_lines[i]));
            i += 1;
        } else if j < fmt_lines.len() {
            changes.push(format!("+{}", fmt_lines[j]));
            j += 1;
            if i < orig_lines.len() && (i >= max_len || orig_lines.get(i) != fmt_lines.get(j)) {
                changes.push(format!("-{}", orig_lines[i]));
                i += 1;
            }
        } else {
            i += 1;
            j += 1;
        }
    }

    changes
}

#[allow(clippy::too_many_arguments)]
fn cmd_diff(
    file: PathBuf,
    left: Option<String>,
    right: Option<String>,
    base: Option<String>,
    since: Option<String>,
    detect_moves: bool,
    blame: bool,
    format: String,
) -> hone::HoneResult<()> {
    let (left_value, right_value) = if let Some(ref git_ref) = since {
        // Since mode: compile current file vs version at git ref
        let canonical = file.canonicalize().map_err(|e| {
            hone::HoneError::io_error(format!("failed to resolve path {}: {}", file.display(), e))
        })?;
        let old_value = hone::compile_at_ref(&canonical, git_ref)?;
        let new_value = hone::compile_file(&file)?;
        (old_value, new_value)
    } else if let Some(ref git_ref) = base {
        // Git mode: compare current file vs file at git ref
        let old_source = std::process::Command::new("git")
            .args(["show", &format!("{}:{}", git_ref, file.display())])
            .output()
            .map_err(|e| hone::HoneError::io_error(format!("failed to run git: {}", e)))?;

        if !old_source.status.success() {
            let stderr = String::from_utf8_lossy(&old_source.stderr);
            return Err(hone::HoneError::io_error(format!(
                "git show failed: {}",
                stderr.trim()
            )));
        }

        let old_src = String::from_utf8_lossy(&old_source.stdout).to_string();

        // Compile old version via in-memory eval (no file imports)
        let mut lexer = hone::Lexer::new(&old_src, Some(file.clone()));
        let tokens = lexer.tokenize()?;
        let mut parser = hone::Parser::new(tokens, &old_src, Some(file.clone()));
        let ast = parser.parse()?;
        let mut evaluator = hone::Evaluator::new(&old_src);
        let old_value = evaluator.evaluate(&ast)?;

        // Compile current version
        let new_value = hone::compile_file(&file)?;

        (old_value, new_value)
    } else if left.is_some() || right.is_some() {
        // Args mode: compare same file with two different arg sets
        let left_args = hone::parse_arg_string(left.as_deref().unwrap_or(""));
        let right_args = hone::parse_arg_string(right.as_deref().unwrap_or(""));

        let left_value = if left_args.is_empty() {
            hone::compile_file(&file)?
        } else {
            let args = hone::build_args_object(&left_args, &[], &[])?;
            hone::compile_file_with_args(&file, args)?
        };

        let right_value = if right_args.is_empty() {
            hone::compile_file(&file)?
        } else {
            let args = hone::build_args_object(&right_args, &[], &[])?;
            hone::compile_file_with_args(&file, args)?
        };

        (left_value, right_value)
    } else {
        return Err(hone::HoneError::io_error(
            "must specify either --base, --since, or --left/--right args".to_string(),
        ));
    };

    let entries = if detect_moves {
        hone::diff_with_moves(&left_value, &right_value)
    } else {
        hone::diff_values(&left_value, &right_value)
    };

    if entries.is_empty() {
        eprintln!("No differences found");
        return Ok(());
    }

    let output = if blame {
        let blamed = hone::blame_diff(&entries, &file);
        hone::format_blame_text(&blamed)
    } else if format == "json" {
        hone::format_diff_json(&entries)
    } else {
        hone::format_diff_text(&entries)
    };

    print!("{}", output);

    // Exit with code 1 to indicate differences exist
    std::process::exit(1);
}

fn cmd_import(
    file: PathBuf,
    output: Option<PathBuf>,
    extract_vars: bool,
    split_docs: bool,
) -> hone::HoneResult<()> {
    // Configure import options
    let options = hone::importer::ImportOptions::new()
        .with_extract_vars(extract_vars)
        .with_split_docs(split_docs);

    // Import the file
    let hone_source = hone::importer::import_file(&file, &options)?;

    // Output
    if let Some(out_path) = output {
        std::fs::write(&out_path, &hone_source).map_err(|e| {
            hone::HoneError::io_error(format!("failed to write {}: {}", out_path.display(), e))
        })?;
        eprintln!("Wrote {}", out_path.display());
    } else {
        println!("{}", hone_source);
    }

    Ok(())
}

fn cmd_lsp(_stdio: bool, socket: Option<u16>) -> hone::HoneResult<()> {
    // Only stdio is supported for now
    if socket.is_some() {
        eprintln!("TCP socket transport not yet implemented, using stdio");
    }

    // Run the LSP server
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| hone::HoneError::io_error(format!("failed to create runtime: {}", e)))?;

    rt.block_on(hone::lsp::run_server());
    Ok(())
}

fn cmd_lex(file: PathBuf) -> hone::HoneResult<()> {
    let source = std::fs::read_to_string(&file).map_err(|e| {
        hone::HoneError::io_error(format!("failed to read {}: {}", file.display(), e))
    })?;

    let mut lexer = hone::Lexer::new(&source, Some(file.clone()));
    let tokens = lexer.tokenize()?;

    println!("Tokens from {}:", file.display());
    println!("{:-<60}", "");

    for token in tokens {
        println!(
            "{:>4}:{:<3}  {:20} {}",
            token.location.line,
            token.location.column,
            format!("{:?}", std::mem::discriminant(&token.kind)),
            token.kind
        );
    }

    Ok(())
}

fn cmd_resolve(file: PathBuf) -> hone::HoneResult<()> {
    let base_dir = file
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();
    let mut resolver = hone::ImportResolver::new(base_dir);

    // Resolve and get the canonical path
    let resolved = resolver.resolve(&file)?;
    let root_path = resolved.path.clone();

    println!("Import Resolution for {}:", file.display());
    println!("{:-<60}", "");

    // Get topological order
    let order = resolver.topological_order(&root_path)?;

    println!("Files (in dependency order):");
    for (i, f) in order.iter().enumerate() {
        let marker = if f.path == root_path { " <-- root" } else { "" };
        println!("  {}. {}{}", i + 1, f.path.display(), marker);

        if let Some(ref from) = f.from_path {
            println!("      from: {}", from.display());
        }
        for import in &f.import_paths {
            println!("      import: {}", import.display());
        }
    }

    Ok(())
}

fn cmd_parse(file: PathBuf) -> hone::HoneResult<()> {
    let source = std::fs::read_to_string(&file).map_err(|e| {
        hone::HoneError::io_error(format!("failed to read {}: {}", file.display(), e))
    })?;

    let mut lexer = hone::Lexer::new(&source, Some(file.clone()));
    let tokens = lexer.tokenize()?;

    let mut parser = hone::Parser::new(tokens, &source, Some(file.clone()));
    let ast = parser.parse()?;

    println!("AST from {}:", file.display());
    println!("{:-<60}", "");

    // Print preamble
    if !ast.preamble.is_empty() {
        println!("Preamble ({} items):", ast.preamble.len());
        for item in &ast.preamble {
            println!("  {:?}", item);
        }
        println!();
    }

    // Print body
    if !ast.body.is_empty() {
        println!("Body ({} items):", ast.body.len());
        for item in &ast.body {
            println!("  {:?}", item);
        }
        println!();
    }

    // Print documents
    if !ast.documents.is_empty() {
        println!("Documents ({}):", ast.documents.len());
        for doc in &ast.documents {
            println!("  --- {}", doc.name.as_deref().unwrap_or("<unnamed>"));
            for item in &doc.preamble {
                println!("    [preamble] {:?}", item);
            }
            for item in &doc.body {
                println!("    [body] {:?}", item);
            }
        }
    }

    Ok(())
}

fn cmd_typegen(file: PathBuf, output: Option<PathBuf>) -> hone::HoneResult<()> {
    let result =
        hone::typeprovider::generate_from_file(&file).map_err(hone::HoneError::io_error)?;

    match output {
        Some(path) => {
            std::fs::write(&path, &result).map_err(|e| {
                hone::HoneError::io_error(format!("failed to write {}: {}", path.display(), e))
            })?;
            eprintln!("Wrote {}", path.display());
        }
        None => {
            print!("{}", result);
        }
    }

    Ok(())
}

fn cmd_eval(source: String, format: String) -> hone::HoneResult<()> {
    // Lex
    let mut lexer = hone::Lexer::new(&source, None);
    let tokens = lexer.tokenize()?;

    // Parse
    let mut parser = hone::Parser::new(tokens, &source, None);
    let ast = parser.parse()?;

    // Evaluate
    let mut evaluator = hone::Evaluator::new(&source);
    let value = evaluator.evaluate(&ast)?;

    // Determine output format
    let output_format = hone::OutputFormat::parse(&format).ok_or_else(|| {
        hone::HoneError::io_error(format!(
            "unknown output format '{}'. Use: json, yaml, toml, dotenv",
            format
        ))
    })?;

    // Emit
    let result = hone::emit(&value, output_format)?;
    println!("{}", result);

    Ok(())
}
