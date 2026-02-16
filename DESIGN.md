# Hone Design Document

This document captures the design philosophy, architectural decisions, and rationale for the Hone configuration language.

## Design Philosophy

### 1. Configuration, Not Programming

Hone is a configuration language, not a general-purpose programming language. This means:

- **No side effects**: Compilation is pure; reading a file twice produces identical output
- **No unbounded loops**: `for` iterates over known collections, never infinite
- **No networking**: No HTTP calls, no sockets (except `env()` for environment variables)
- **Deterministic**: Same input always produces same output

### 2. Fail Fast, Fail Clearly

Errors should be caught as early as possible with actionable messages:

- **Parse-time**: Syntax errors caught immediately
- **Resolve-time**: Missing imports caught before evaluation
- **Evaluate-time**: Type mismatches, assertion failures, undefined variables
- **Never at deploy-time**: If Hone compiles, the output is structurally valid

### 3. Gradual Typing

Users can start with no types and add them incrementally:

```hone
# No types - works fine
port: 8080

# Add a schema later
schema Config { port: int }
use Config
port: 8080
```

### 4. Composition Over Inheritance

Hone favors explicit composition:

- `import` brings in modules explicitly
- `from` does inheritance but the mechanism is visible
- No implicit parent classes or prototype chains
- Spread operator `...` makes merging explicit

## Architecture

```
Source File (.hone)
       │
       ▼
   ┌────────┐
   │ Lexer  │  Tokenizes source into tokens
   └────────┘
       │
       ▼
   ┌────────┐
   │ Parser │  Builds Abstract Syntax Tree
   └────────┘
       │
       ▼
   ┌──────────┐
   │ Resolver │  Resolves imports, detects cycles
   └──────────┘
       │
       ▼
   ┌───────────┐
   │ Evaluator │  Executes AST, produces Value
   └───────────┘
       │
       ▼
   ┌────────────┐
   │ TypeChecker│  Validates against schemas
   └────────────┘
       │
       ▼
   ┌───────────────┐
   │ PolicyChecker │  Evaluates deny/warn rules
   └───────────────┘
       │
       ▼
   ┌─────────┐
   │ Emitter │  Serializes to JSON/YAML/TOML/.env
   └─────────┘
       │
       ▼
   Output (JSON/YAML/TOML/.env)
```

The pipeline order matters: evaluation happens before type checking (the evaluator produces a concrete Value tree), and policy checking happens after type checking (policies validate the final, typed output).

### Key Components

#### Lexer (`src/lexer/`)
- Handles string interpolation by emitting `StringPart` tokens
- Tracks source locations for error reporting
- Supports `#` comments

#### Parser (`src/parser/`)
- Recursive descent parser
- Produces strongly-typed AST (`ast.rs`)
- Handles operator precedence via Pratt parsing

#### Resolver (`src/resolver/`)
- Builds import graph
- Detects circular imports
- Caches parsed files for reuse

#### Evaluator (`src/evaluator/`)
- Tree-walking interpreter
- Lexical scoping with `Scope` struct
- Deep merge logic in `merge.rs`
- Built-in functions in `builtins.rs`

#### TypeChecker (`src/typechecker/`)
- Schema collection and validation
- Type inference for expressions
- Constraint checking (int/float range, string length, regex patterns)

#### Compiler (`src/compiler/`)
- Orchestrates the full pipeline
- Handles multi-file compilation
- Manages import resolution

## Key Decisions

### Decision 1: Indentation-Insensitive

**Choice**: Braces `{}` for structure, not indentation.

**Rationale**:
- YAML's significant whitespace causes real-world bugs
- Braces are explicit and tooling-friendly
- Copy-paste doesn't break structure

### Decision 2: Deep Merge by Default

**Choice**: Nested objects merge recursively by default.

```hone
# These merge, not replace
config { a: 1 }
config { b: 2 }
# Result: config { a: 1, b: 2 }
```

**Rationale**:
- Matches common overlay pattern (base + environment)
- `!:` provides explicit replace when needed
- Arrays don't merge (append with `+:`)

### Decision 3: No Implicit Type Coercion

**Choice**: `"8080"` is a string, `8080` is an integer. No automatic conversion.

**Rationale**:
- YAML's implicit typing causes bugs ("yes" becoming `true`)
- Explicit is better than implicit
- Type functions available: `to_int()`, `to_str()`

### Decision 4: Schemas are Optional

**Choice**: Schemas validate at compile time but aren't required.

**Rationale**:
- Low barrier to entry for simple configs
- Gradual adoption path
- Some configs genuinely don't need types

### Decision 5: Multi-Document Support

**Choice**: `---name` syntax for multiple output documents.

```hone
---deployment
kind: "Deployment"

---service
kind: "Service"
```

**Rationale**:
- Common pattern in Kubernetes (multiple resources)
- Single source file, multiple outputs
- Shared preamble reduces duplication

## Type System

### Primitive Types
- `null` - The null value
- `bool` - `true` or `false`
- `int` - Integer numbers
- `float` - Floating point numbers
- `string` - Text values
- `array` - Ordered lists
- `object` - Key-value maps

### Schema Types
```hone
schema Config {
  port: int
  host: string
  enabled?: bool  # Optional field
}
```

### Type Aliases
```hone
type Port = int(1, 65535)
type Name = string(1, 100)
type Percentage = float(0.0, 1.0)
```

## Error Handling

Errors use the `miette` crate for rich formatting:

```
error[E0002]: undefined variable `prot`
  --> config.hone:12:5
   |
12 |     port: prot
   |           ^^^^ did you mean `port`?
```

### Error Categories

| Range | Category |
|-------|----------|
| E00xx | Syntax/Lexer errors |
| E01xx | Import/Resolver errors |
| E02xx | Type errors |
| E03xx | Evaluation errors |
| E04xx | Runtime errors |
| E07xx | Assertion errors |

## File Format

### Extension
`.hone` is the standard extension.

### Encoding
UTF-8 required. No BOM.

### Line Endings
LF preferred, CRLF tolerated.

## Future Considerations

### Not Planned
- **Macros**: Too complex, violates "configuration not programming"
- **Async/Await**: No I/O means no need
- **Classes/Methods**: Objects are data, not behavior
- **Mutable variables**: All bindings are immutable
- **Watch mode**: Use external tools (`entr`, `watchexec`) for file watching
- **REPL**: Use `hone eval` for quick expressions

### Under Consideration
- **Language libraries**: Native parsing of `.hone` files in popular languages
- **Package manager**: Import from URLs or registry
- **Sourcemaps**: Map output back to Hone source
- **Incremental compilation**: For large projects
- **Plugins**: Custom functions via WASM

## Glossary

| Term | Definition |
|------|------------|
| Preamble | Top section of file with imports, schemas, lets, asserts |
| Body | Main section producing output values |
| Overlay | Pattern where one config merges onto another |
| Schema | Type definition for validating structure |
| Deep Merge | Recursive merging of nested objects |
