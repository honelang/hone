# Contributing to Hone

Thank you for your interest in contributing to Hone.

## Getting started

### Prerequisites

- Rust toolchain (stable) via [rustup](https://rustup.rs/)
- Node.js 18+ (for the VS Code extension)

### Build

```bash
git clone https://github.com/honelang/hone.git
cd hone
cargo build
```

### Test

```bash
cargo test
```

All 535+ tests must pass before submitting a PR.

### Lint

```bash
cargo clippy -- -D warnings
cargo fmt -- --check
```

Both must be clean. CI enforces this.

## Project structure

```
src/
  main.rs           CLI entry point
  lib.rs            Library exports
  lexer/            Tokenizer
  parser/           AST generation (mod.rs + ast.rs)
  resolver/         Import resolution
  evaluator/        Runtime evaluation, builtins, merge, scoping
  typechecker/      Schema validation and type checking
  compiler/         Orchestrates the full pipeline
  emitter/          JSON, YAML, TOML, dotenv output
  formatter/        Source code formatter
  differ/           Structural diff engine
  importer/         YAML/JSON to Hone converter
  graph/            Import dependency graph
  cache/            Content-addressed build cache
  typeprovider/     JSON Schema to Hone type generation
  errors/           Error types and codes
  lsp/              Language Server Protocol
editors/vscode/     VS Code/Cursor extension
hone-wasm/          WebAssembly bindings
playground/         Browser-based playground
tests/
  integration_tests.rs
```

## Compilation pipeline

```
Source -> Lexer -> Parser -> Resolver -> Evaluator -> TypeChecker -> PolicyChecker -> Emitter -> Output
```

## How to add a built-in function

1. Edit `src/evaluator/builtins.rs`
2. Add a match arm in `call_builtin()`:

```rust
"my_func" => {
    check_arity(name, &args, 1)?;
    match &args[0] {
        Value::String(s) => Ok(Value::String(s.to_uppercase())),
        _ => Err(type_error(name, "string", &args[0])),
    }
}
```

3. Add `"my_func"` to the `BUILTIN_FUNCTIONS` array in the same file
4. Add tests in `tests/integration_tests.rs`
5. Update `CLAUDE.md` with the function signature

## How to add an AST node

1. Add the variant to `src/parser/ast.rs`
2. Add lexer tokens if needed in `src/lexer/mod.rs`
3. Parse it in `src/parser/mod.rs`
4. Evaluate it in `src/evaluator/mod.rs`
5. Format it in `src/formatter/mod.rs`
6. Add tests
7. Update `CLAUDE.md`

## How to add an output format

1. Create `src/emitter/myformat.rs` implementing the `Emitter` trait
2. Add the variant to `OutputFormat` in `src/emitter/mod.rs`
3. Add dispatch in `emit()` and parsing in `OutputFormat::parse()`
4. Add `"myformat"` to CLI arg choices in `src/main.rs`
5. Add WASM support in `hone-wasm/src/lib.rs`
6. Add tests

## Error codes

Errors use the ranges documented in `src/errors/mod.rs`:

| Range | Category |
|---|---|
| E00xx | Syntax/lexer |
| E01xx | Import/resolver |
| E02xx | Type system |
| E03xx | Merge |
| E04xx | Evaluation/runtime |
| E05xx | Dependencies |
| E07xx | Assertions |
| E08xx | Hermeticity/secrets |

When adding new error variants, use the next available code in the appropriate range.

## Testing

Tests live in two places:

- **Unit tests**: `#[cfg(test)]` modules inside source files
- **Integration tests**: `tests/integration_tests.rs`

Integration tests compile `.hone` source strings and verify the output. Pattern:

```rust
#[test]
fn test_my_feature() {
    let source = r#"
let x = 42
value: x
"#;
    let result = compile_source(source);
    assert!(result.is_ok());
    let value = result.unwrap();
    assert_eq!(value.get_path(&["value"]), Some(&Value::Int(42)));
}
```

## Code style

- Zero compiler warnings (enforced by `cargo clippy -- -D warnings`)
- Format with `cargo fmt`
- No dead code -- features are implemented or removed, not stubbed
- Error messages include help text with actionable suggestions
- No unnecessary dependencies

## Submitting changes

1. Fork the repository
2. Create a branch from `main`
3. Make your changes
4. Ensure `cargo test`, `cargo clippy -- -D warnings`, and `cargo fmt -- --check` pass
5. Open a pull request against `main`

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
