// Rust 1.93+ triggers false positives on thiserror/miette derive macro fields
#![allow(unused_assignments)]

//! Hone Configuration Language
//!
//! A configuration language that compiles to JSON and YAML.
//! Designed for infrastructure engineers who need more than YAML
//! but don't want a full programming language.
//!
//! # Example
//!
//! ```hone
//! let env = "production"
//!
//! server {
//!   host: "localhost"
//!   port: 8080
//!   name: "api-${env}"
//! }
//! ```

pub mod cache;
pub mod compiler;
pub mod differ;
pub mod emitter;
pub mod errors;
pub mod evaluator;
pub mod formatter;
pub mod graph;
pub mod importer;
pub mod lexer;
#[cfg(feature = "lsp")]
pub mod lsp;
pub mod parser;
pub mod resolver;
pub mod typechecker;
pub mod typeprovider;

pub use compiler::{
    build_args_object, compile_file, compile_file_with_args, infer_value, validate_against_schema,
    CompiledFile, Compiler,
};
pub use differ::{
    blame_diff, compile_at_ref, diff_values, diff_with_moves, format_blame_text, format_diff_json,
    format_diff_text, parse_arg_string, BlameInfo, DiffEntry, DiffKind,
};
pub use emitter::{
    emit, emit_multi, DotenvEmitter, Emitter, JsonEmitter, OutputFormat, TomlEmitter, YamlEmitter,
};
pub use errors::{HoneError, HoneResult, Warning};
pub use evaluator::{Evaluator, Value};
pub use formatter::format_source;
pub use lexer::token::{SourceLocation, Token, TokenKind};
pub use lexer::{Comment, Lexer};
pub use parser::ast;
pub use parser::Parser;
pub use resolver::{ImportResolver, ResolvedFile, VirtualResolver};
pub use typechecker::{Type, TypeChecker, TypeEnv, TypeRegistry};
pub use typeprovider::generate_from_file as typegen;
