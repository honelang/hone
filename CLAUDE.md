# Hone Configuration Language - AI Agent Context

This document provides comprehensive context for AI agents working with the Hone codebase.

## What is Hone?

Hone is a **configuration language** that compiles to JSON, YAML, TOML, and .env. It sits between raw YAML/JSON and full programming languages like Python or TypeScript, providing:

- Variables and string interpolation
- Conditionals and loops
- Functions and type checking
- Multi-file imports and composition
- Deep merging and inheritance

**Target users**: DevOps engineers, platform teams, anyone managing complex YAML configurations (Kubernetes, Helm, Ansible, etc.)

## Project Structure

```
hone/
├── src/
│   ├── main.rs          # CLI entry point
│   ├── lib.rs           # Library exports
│   ├── lexer/           # Tokenizer
│   ├── parser/          # AST generation
│   │   ├── mod.rs       # Parser implementation
│   │   └── ast.rs       # AST node definitions
│   ├── evaluator/       # Runtime evaluation
│   │   ├── mod.rs       # Main evaluator
│   │   ├── builtins.rs  # Built-in functions
│   │   ├── merge.rs     # Deep merge logic
│   │   ├── scope.rs     # Variable scoping
│   │   └── value.rs     # Runtime values
│   ├── compiler/        # Multi-file compilation
│   ├── resolver/        # Import resolution
│   ├── typechecker/     # Type system
│   ├── emitter/         # JSON/YAML/TOML/.env output
│   ├── errors/          # Error types
│   ├── cache/           # Content-addressed build cache
│   ├── graph/           # Dependency graph visualization
│   ├── differ/          # Structural diff with move detection
│   ├── typeprovider/    # JSON Schema -> Hone type generation
│   └── lsp/             # Language Server Protocol
├── lib/
│   └── k8s/v1.30/       # Kubernetes schema library (78 schemas)
│       ├── _types.hone   # IntOrString, Quantity, K8sName type aliases
│       ├── _meta.hone    # ObjectMeta, LabelSelector
│       ├── core.hone     # Pod, Service, ConfigMap, Container, etc.
│       ├── apps.hone     # Deployment, StatefulSet, DaemonSet
│       ├── batch.hone    # Job, CronJob
│       ├── networking.hone # Ingress, NetworkPolicy
│       └── rbac.hone     # Role, ClusterRole, RoleBinding
├── examples/
│   ├── microservices/   # Multi-file K8s stack (API, worker, Redis, Postgres)
│   ├── k8s-validated/   # K8s manifests with schema validation
│   └── ci-pipeline/     # Multi-file GitHub Actions CI/CD workflow
├── editors/
│   └── vscode/          # VS Code/Cursor extension
└── tests/
    └── integration_tests.rs
```

## Language Syntax

### Basic Structure

```hone
# Comments start with #

# Variables
let name = "value"
let count = 42
let enabled = true

# Output is the last expression or key-value pairs
key: value
nested {
  child: "value"
}
```

### Data Types

| Type | Examples |
|------|----------|
| Null | `null` |
| Boolean | `true`, `false` |
| Integer | `42`, `-17` |
| Float | `3.14`, `-0.5`, `1e10` |
| String | `"hello"`, `'literal'`, `"""multiline"""` |
| Array | `[1, 2, 3]` |
| Object | `{ key: "value" }` |

### String Interpolation

```hone
let env = "prod"
let name = "api-${env}"           # "api-prod"
let port = 8080
let url = "http://localhost:${port}"
```

### Conditionals

```hone
let env = "production"
let replicas = env == "production" ? 3 : 1

# Using when/else blocks
when env == "production" {
  replicas: 5
} else when env == "staging" {
  replicas: 2
} else {
  replicas: 1
}
```

### Loops

```hone
# Array comprehension
let doubled = for x in [1, 2, 3] { x * 2 }  # [2, 4, 6]

# Block body: let bindings + trailing expression → array
let squares = for i in range(0, 5) {
  let sq = i * i
  sq
}
# [0, 1, 4, 9, 16]

# Object generation
let ports = for name in ["http", "https"] {
  "${name}Port": name == "http" ? 80 : 443
}
# { httpPort: 80, httpsPort: 443 }

# Destructuring
let items = for (key, value) in { a: 1, b: 2 } {
  "${key}_doubled": value * 2
}
```

### User-Defined Functions

Define reusable functions in the preamble with `fn`. Functions are not first-class values -- they can only be defined at the top level and called by name.

```hone
fn double(x) {
  x * 2
}

fn greet(name, title) {
  "Hello, ${title} ${name}!"
}

result: double(21)              # 42
message: greet("Alice", "Dr.")  # "Hello, Dr. Alice!"
```

Functions can call builtins and other user-defined functions:

```hone
fn slugify(s) {
  lower(replace(trim(s), " ", "-"))
}

fn make_label(app, env) {
  "${slugify(app)}-${env}"
}

label: make_label("My App", "prod")  # "my-app-prod"
```

Functions can be exported across files via named imports:

```hone
# utils.hone
fn double(x) { x * 2 }
fn triple(x) { x * 3 }

# main.hone
import { double } from "./utils.hone"
result: double(5)  # 10
```

Key details:
- Functions are defined in the preamble (before body key-value pairs)
- The body is a single expression (the return value)
- Parameters are scoped -- they don't leak into the caller
- A user function with the same name as a builtin overrides it
- `fn` is a reserved keyword and cannot be used as a bare key

### Imports

```hone
# Import entire module
import "./config.hone" as config
value: config.some_variable

# Import specific names
import { port, host } from "./settings.hone"

# Inheritance (overlay pattern)
from "./base.hone"
# All content here merges with/overrides base
override_key: "new value"
```

### Assignment Operators

```hone
# Normal assignment (deep merge for objects)
key: value

# Append to array
items +: ["new_item"]

# Force replace (no merge)
config !: { completely: "replaced" }
```

### Spread Operator

```hone
let base = { a: 1, b: 2 }
let extended = { ...base, c: 3 }  # { a: 1, b: 2, c: 3 }

let arr1 = [1, 2]
let arr2 = [...arr1, 3, 4]  # [1, 2, 3, 4]
```

### Variants

Environment-specific configuration blocks, selected at compile time via `--variant`:

```hone
variant env {
  default dev {
    replicas: 1
    debug: true
  }

  staging {
    replicas: 2
    debug: false
  }

  production {
    replicas: 5
    debug: false
  }
}

name: "my-app"
```

Compile with: `hone compile config.hone --variant env=production`

- The `default` keyword marks a case used when no `--variant` is specified
- Without a default, `--variant` is required (error otherwise)
- Multiple variant blocks are supported: `--variant env=prod --variant region=eu`
- Variant body items merge with the main output (deep merge)

### Assertions

```hone
let port = 8080
assert port > 0 : "port must be positive"
assert port < 65536 : "port must be valid"
```

### Expect Declarations

Self-documenting argument requirements in the preamble:

```hone
expect args.env: string              # required, no default
expect args.port: int = 8080         # optional with default
expect args.debug: bool = false      # optional with default

host: "api-${args.env}"
port: args.port
```

Compiles with: `hone compile config.hone --set env=prod`

If a required arg is missing, the error tells the user exactly what to provide.

### Secret Declarations

First-class secret placeholders that never leak into output:

```hone
secret db_password from "vault:secret/data/db#password"
secret api_key from "env:API_KEY"

database {
  password: db_password
}
```

- By default, secrets emit `<SECRET:provider>` placeholders
- `--secrets-mode error` fails if any secret placeholder appears in output
- `--secrets-mode env` resolves `env:NAME` secrets from environment variables
- Secrets work in string interpolation: `"prefix-${db_password}"`

### Policy Declarations

Post-compilation rules that validate the output:

```hone
policy no_debug deny when output.debug == true {
  "debug must be disabled in production configs"
}

policy port_range warn when output.port < 1024 {
  "privileged ports require elevated permissions"
}

debug: false
port: 8080
```

- `deny` policies cause compilation failure
- `warn` policies emit warnings but compilation succeeds
- `output` refers to the final compiled value
- `--ignore-policy` flag skips all policy checks
- Policies are named for clear error messages

### Type Aliases

Define reusable constrained types using the same syntax as schema fields:

```hone
type Port = int(1, 65535)          # int with range
type Name = string(1, 100)         # string with length constraints
type Percentage = float(0.0, 1.0)  # float with range
```

Use type aliases in schemas:

```hone
type Port = int(1, 65535)

schema Server {
  host: string
  port: Port          # Uses the type alias
  debug?: bool
}
```

### Type Annotations and Schema Validation

```hone
schema Server {
  host: string
  port: int(1, 65535)       # int with min/max constraints
  name: string(1, 100)      # string with length constraints
  debug?: bool              # optional field
}

# Apply schema to validate output at compile time
use Server

host: "localhost"
port: 8080
name: "api-server"
```

**Supported Constraints:**
- `int` - any integer
- `int(min, max)` - integer within range (inclusive)
- `float` - any float
- `float(min, max)` - float within range
- `string` - any string
- `string(min_len, max_len)` - string with length constraints
- `string("regex")` - string matching regex pattern
- `bool` - boolean
- `object` - any object
- `array` - any array
- `SchemaName` - nested schema reference

Schemas are **closed by default** -- extra fields not in the schema are rejected. Use `...` to make a schema open:

```hone
schema Strict {
  name: string
  port: int
}
# Output with extra field "debug" → error

schema Flexible {
  name: string
  port: int
  ...
}
# Output with extra field "debug" → allowed
```

If the output doesn't match the schema, compilation fails with a clear error:
- `TypeMismatch` - wrong type for a field or constraint violated
- `MissingField` - required field not present

Example constraint violation:
```
TypeMismatch: expected int(1, 65535), found int (value: 99999)
  help: value 99999 is greater than maximum 65535
```

## Built-in Functions

| Function | Description | Example |
|----------|-------------|---------|
| `len(x)` | Length of string/array/object | `len("hello")` → `5` |
| `keys(obj)` | Object keys as array | `keys({a:1})` → `["a"]` |
| `values(obj)` | Object values as array | `values({a:1})` → `[1]` |
| `contains(x, y)` | Check if x contains y | `contains([1,2], 2)` → `true` |
| `upper(s)` | Uppercase string | `upper("hi")` → `"HI"` |
| `lower(s)` | Lowercase string | `lower("HI")` → `"hi"` |
| `trim(s)` | Trim whitespace | `trim(" x ")` → `"x"` |
| `split(s, d)` | Split string | `split("a,b", ",")` → `["a","b"]` |
| `join(arr, d)` | Join array | `join(["a","b"], "-")` → `"a-b"` |
| `replace(s, from, to)` | Replace in string | `replace("ab", "b", "c")` → `"ac"` |
| `range(start, end)` | Generate int range | `range(0, 3)` → `[0, 1, 2]` |
| `base64_encode(s)` | Encode to base64 | `base64_encode("hi")` → `"aGk="` |
| `base64_decode(s)` | Decode from base64 | `base64_decode("aGk=")` → `"hi"` |
| `to_json(v)` | Convert to JSON string | `to_json({a:1})` → `"{\"a\":1}"` |
| `from_json(s)` | Parse JSON string | `from_json("{\"a\":1}")` → `{a:1}` |
| `env(name, default?)` | Read env variable | `env("HOME")` |
| `file(path)` | Read file contents | `file("./data.txt")` |
| `concat(arrays...)` | Concatenate arrays | `concat([1], [2])` → `[1,2]` |
| `flatten(arr)` | Flatten nested arrays | `flatten([[1],[2]])` → `[1,2]` |
| `default(v, fallback)` | Null coalescing | `default(null, "x")` → `"x"` |
| `to_int(v)` | Convert to integer | `to_int("42")` → `42` |
| `to_float(v)` | Convert to float | `to_float("3.14")` → `3.14` |
| `to_str(v)` | Convert to string | `to_str(42)` → `"42"` |
| `to_bool(v)` | Convert to bool (truthiness) | `to_bool(1)` → `true` |
| `merge(objs...)` | Shallow merge objects (right wins) | `merge({a:1}, {b:2})` → `{a:1, b:2}` |
| `sort(arr)` | Sort array (numbers, strings, mixed) | `sort([3,1,2])` → `[1,2,3]` |
| `reverse(arr)` | Reverse array | `reverse([1,2,3])` → `[3,2,1]` |
| `unique(arr)` | Remove duplicates from array | `unique([1,2,1])` → `[1,2]` |
| `slice(arr, start, end?)` | Slice array (negative indices supported) | `slice([1,2,3,4], 1, 3)` → `[2,3]` |
| `min(a, b)` | Minimum of two numbers | `min(3, 7)` → `3` |
| `max(a, b)` | Maximum of two numbers | `max(3, 7)` → `7` |
| `abs(n)` | Absolute value | `abs(-5)` → `5` |
| `clamp(n, lo, hi)` | Clamp number to range | `clamp(10, 0, 5)` → `5` |
| `starts_with(s, prefix)` | Check string prefix | `starts_with("hello", "he")` → `true` |
| `ends_with(s, suffix)` | Check string suffix | `ends_with("hello", "lo")` → `true` |
| `substring(s, start, end?)` | Extract substring | `substring("hello", 1, 3)` → `el` |
| `type_of(v)` | Get type name as string | `type_of(42)` → `"int"` |
| `entries(obj)` | Object to `[[key, value], ...]` | `entries({a:1})` → `[["a",1]]` |
| `from_entries(arr)` | `[[key, value], ...]` to object | `from_entries([["a",1]])` → `{a:1}` |
| `sha256(s)` | SHA256 hash of string | `sha256("hi")` → `"8f43..."` |

For transforming collections, use for comprehensions: `for x in items { x * 2 }`

## Common Patterns

### String Booleans

Some target systems (e.g., Helm values, Ansible vars) require boolean values as strings (`"true"` / `"false"`) rather than actual booleans. Use `to_str()`:

```hone
# Produces: enabled: "true" (as a string)
enabled: to_str(true)

# From a variable
let debug = false
debugMode: to_str(debug)
```

Hone's type system is honest: `true` is a boolean, `"true"` is a string. Target format quirks are handled by the user at the call site, not by the emitter.

### Reserved Words as Keys

Keywords like `type`, `schema`, `import`, `for`, `when`, `else`, `expect`, `fn` cannot be used as bare keys. Quote them:

```hone
# Wrong: type: "Deployment"
# Right:
"type": "Deployment"
"import": "some-module"
```

### Block vs Inline Syntax

```hone
# Block syntax: newline-separated, no commas
server {
  host: "localhost"
  port: 8080
}

# Inline syntax: comma-separated, colon assignment
server: { host: "localhost", port: 8080 }
```

## Variable Scoping Rules

Hone uses **lexical scoping** with the following resolution order:

1. **Local scope** - Variables defined in the current block (for loops, when blocks)
2. **File scope** - `let` bindings in the preamble
3. **Import scope** - Imported modules accessed via their alias
4. **Built-in scope** - Built-in functions

```hone
let x = "file scope"

for item in [1, 2, 3] {
  let x = "loop scope"  # Shadows file scope x
  result: x             # Uses "loop scope"
}

outer: x  # Uses "file scope" (loop x not visible here)
```

### Import Scoping

```hone
import "./config.hone" as config      # Access as config.var
import { port, host } from "./net.hone"  # Direct access as port, host

server {
  port: port           # Direct import
  name: config.name    # Module access
}
```

### When Block Scoping

`when` blocks **do not create a new scope** - their content merges into the parent:

```hone
let env = "prod"

server {
  host: "localhost"
  when env == "prod" {
    host: "prod.example.com"  # Overrides the host above
    replicas: 3               # Adds new key
  } else {
    replicas: 1               # Exactly one branch taken
  }
}
```

### Variant Let Bindings

`let` inside variant cases is visible in the enclosing scope:

```hone
variant env {
  default dev {
    let replicas = 1
    let domain = "localhost"
  }
  production {
    let replicas = 5
    let domain = "prod.example.com"
  }
}

# replicas and domain are accessible here
count: replicas
url: "https://${domain}/api"
```

## Operator Precedence

From highest to lowest precedence:

| Level | Operators | Associativity |
|-------|-----------|---------------|
| 1 | `()` `[]` `.` | Left-to-right |
| 2 | `!` `-` (unary) | Right-to-left |
| 3 | `*` `/` `%` | Left-to-right |
| 4 | `+` `-` | Left-to-right |
| 5 | `??` (null coalesce) | Left-to-right |
| 6 | `<` `<=` `>` `>=` | Left-to-right |
| 7 | `==` `!=` | Left-to-right |
| 8 | `&&` | Left-to-right |
| 9 | `\|\|` | Left-to-right |
| 10 | `? :` (ternary) | Right-to-left |

```hone
# Examples
let a = 1 + 2 * 3      # 7 (multiplication first)
let b = !true && false # false (! binds tighter than &&)
let c = x ?? y ?? z    # First non-null from left
let d = a > b ? 1 : 2  # Ternary has lowest precedence
```

## CLI Commands

### `hone compile`

```bash
hone compile file.hone                          # Compile to pretty JSON (default)
hone compile file.hone --format yaml            # Output format: json, yaml, toml, dotenv
hone compile file.hone -o output.yaml           # Output to file (format inferred from ext)
hone compile file.hone --output-dir ./manifests # Multi-file output (split ---name docs)

# Variant selection
hone compile file.hone --variant env=production
hone compile file.hone --variant env=prod --variant region=eu

# CLI args injection (available as args.* in source)
hone compile file.hone --set env=prod                # Type-inferred value
hone compile file.hone --set-string port=8080        # Force string (no inference)
hone compile file.hone --set-file cert=./cert.pem    # Read value from file

# Build modes
hone compile file.hone --dry-run                # Print to stdout, don't write
hone compile file.hone --strict                 # Treat warnings as errors (exit 1)
hone compile file.hone --quiet                  # Suppress warnings
hone compile file.hone --allow-env              # Enable env() and file() builtins
hone compile file.hone --no-cache               # Skip build cache

# Secret and policy modes
hone compile file.hone --secrets-mode error     # Fail if secret placeholders in output
hone compile file.hone --secrets-mode env       # Resolve env: secrets (requires --allow-env)
hone compile file.hone --ignore-policy          # Skip all policy checks
```

### `hone check`

```bash
hone check file.hone                            # Validate without output
hone check file.hone --set env=prod             # With args
hone check file.hone --schema MySchema          # Validate against specific schema
hone check file.hone --allow-env                # Allow env()/file()
hone check file.hone --variant env=prod         # With variant selection
```

### `hone fmt`

```bash
hone fmt file.hone           # Print formatted to stdout
hone fmt --write file.hone   # Format in place
hone fmt --check file.hone   # Check only (exit 1 if unformatted)
hone fmt --diff file.hone    # Show diff of changes
hone fmt .                   # Format all .hone files in directory
```

### `hone diff`

```bash
hone diff file.hone --base main                              # vs git ref
hone diff file.hone --left "env=dev" --right "env=production" # two arg sets
hone diff file.hone --left "env=dev" --right "env=prod" --format json
hone diff file.hone --since main                             # vs git ref (time-travel)
hone diff file.hone --since main --detect-moves              # detect moved keys
hone diff file.hone --since main --blame                     # git blame annotations
```

### Other commands

```bash
# Import YAML/JSON to Hone
hone import config.yaml -o config.hone
hone import config.yaml --extract-vars  # Detect repeated values

# Generate Hone schemas from JSON Schema
hone typegen schema.json                # Print to stdout
hone typegen schema.json -o types.hone  # Write to file

# Visualize import dependency graph
hone graph main.hone                    # Text tree (default)
hone graph main.hone --format dot       # Graphviz DOT format
hone graph main.hone --format json      # JSON format

# Manage build cache
hone cache clean                        # Remove all cached results
hone cache clean --older-than 7d        # Remove stale entries

# Start LSP server
hone lsp --stdio

# Debug commands (hidden)
hone lex file.hone      # Show tokens
hone parse file.hone    # Show AST
hone resolve file.hone  # Show import graph
hone eval 'let x = 1 + 2'  # Evaluate inline
```

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Compilation error (syntax, type, evaluation, --strict) |
| 3 | I/O error (file not found, write failure) |

## Multi-Document Output

Use `---name` to create multiple output documents from a single source:

```hone
# Shared preamble (variables available to all documents)
let app = "myapp"
let env = "production"

---deployment
apiVersion: "apps/v1"
kind: "Deployment"
metadata {
  name: "${app}-${env}"
}

---service
apiVersion: "v1"
kind: "Service"
metadata {
  name: "${app}-svc"
}
```

Compile with `--output-dir` to create separate files:

```bash
hone compile k8s.hone --output-dir ./manifests --format yaml
# Creates: manifests/deployment.yaml, manifests/service.yaml
```

## Deep Merge Behavior

When objects are merged, Hone performs deep merging:

```hone
# Base
config {
  server {
    port: 8080
    host: "localhost"
  }
}

# Overlay (merges, doesn't replace)
config {
  server {
    port: 9090  # overrides
    # host remains "localhost"
  }
}

# Result:
# config.server = { port: 9090, host: "localhost" }
```

Use `!:` to force replacement instead of merge.

### `from` + `import` Interaction

When a file uses both `from` and `import`:

1. `import`ed values are injected into scope as variables (for use in expressions)
2. The file body is evaluated with those variables in scope
3. The evaluated output is deep-merged **on top of** the `from` parent's output (child wins on conflict)

```hone
from "./base.hone"           # base has { port: 80, host: "base.com" }
import "./lib.hone" as lib   # lib exports url = "https://api.com"

host: lib.url                # Overrides base.host
# Result: { port: 80, host: "https://api.com" } — base.port preserved, host overridden
```

## Behavioral Notes

### Null Propagation
- **String interpolation**: `"prefix-${null_var}"` produces `"prefix-null"` (null renders as the string `"null"`)
- **Arithmetic**: `null + 1` is a `TypeMismatch` error
- **Missing keys**: `obj.nonexistent_key` silently returns `null` (no error)
- **Null coalescing**: `null ?? fallback` returns `fallback`

### Type Coercion
- **Int/Float equality**: `1 == 1.0` is `true` (automatic numeric coercion)
- **No string coercion**: `1 == "1"` is `false`
- **No bool coercion**: `true == 1` is `false`
- **Ordering**: `1 < 2.0` works (int/float mixed), `"a" < "b"` works (lexicographic), `true < 2` is a `TypeMismatch` error

### Float Arithmetic
Float operations follow IEEE 754. Overflow produces `Inf`, not an error. `NaN` propagates silently. Integer arithmetic uses checked operations and raises `ArithmeticOverflow` (E0402) on overflow.

### Inline Object Separators
In inline object syntax `{ }`, commas and newlines are interchangeable separators. Trailing commas, leading commas, and double commas are silently accepted. In block syntax, commas produce a helpful error message.

### @unchecked Escape Hatch

Bypass type checking for a specific value with a warning:

```hone
schema Config {
  port: int(1, 65535)
}
use Config

port: 99999 @unchecked   # Emits warning but compiles
```

### Schema Extends

Schemas can extend other schemas:

```hone
schema Base {
  name: string
  port: int
}

schema Extended extends Base {
  debug?: bool
}
```

## Error Codes

| Code | Category | Description |
|------|----------|-------------|
| E0001 | Parse | Unexpected token / character |
| E0002 | Parse | Undefined variable |
| E0003 | Parse | Reserved word as bare key (use quotes) |
| E0004 | Parse | Unterminated string |
| E0005 | Parse | Invalid escape sequence |
| E0101 | Import | Import file not found |
| E0102 | Import | Circular import detected |
| E0201 | Type | Value out of range (constraint violation) |
| E0202 | Type | Type mismatch |
| E0203 | Type | Pattern mismatch (regex constraint) |
| E0204 | Type | Missing required field |
| E0205 | Type | Unknown field in closed schema |
| E0302 | Merge | Multiple `from` declarations in one file |
| E0304 | Merge | `from` in preamble of multi-document file |
| E0402 | Eval | Division by zero / arithmetic overflow |
| E0403 | Eval | Maximum nesting depth exceeded |
| E0501 | Dep | Circular dependency |
| E0701 | Control | `for` not allowed at top level |
| E0702 | Control | Assertion failed |
| E0801 | Hermetic | env()/file() requires --allow-env |
| E0802 | Hermetic | Secret placeholder in output (--secrets-mode error) |

## Known Issues

### Missing Features

(none currently)

## Kubernetes Schema Library

The `lib/k8s/v1.30/` directory contains 78 Hone schemas generated from the official Kubernetes JSON Schema definitions. Usage:

```hone
import "../../lib/k8s/v1.30/apps.hone" as apps
import "../../lib/k8s/v1.30/core.hone" as core
import "../../lib/k8s/v1.30/_meta.hone" as meta

use Deployment

apiVersion: "apps/v1"
kind: "Deployment"
# ... validated at compile time
```

**Design decisions:**
- All schemas are **open** (`...`) -- validates defined fields, allows extras
- No status fields (users don't write status blocks)
- Fields named with Hone reserved words (`type`, `secret`) are quoted in schemas (e.g., `"type"?: string`)
- `IntOrString` is aliased to `string` (Hone lacks union types)

**Regeneration:** `python3 scripts/generate-k8s-schemas.py [--version 1.30]`

## Current Limitations

1. **No package manager** - imports are file-path based only
2. **Schema field defaults parsed but not applied** - `port?: int = 8080` parses without error but the default value is not injected at runtime
3. **Regex patterns recompiled on each validation** - Schema `string("regex")` constraints recompile the regex on every check (negligible for typical configs)

## LSP Features

The language server provides:
- **Diagnostics** - Syntax errors, type mismatches, evaluation errors, schema violations, and policy warnings shown in real-time (background compilation on every change)
- **Go to Definition** - Jump to variable declarations (Ctrl+Click or F12)
- **Find References** - Find all usages of a variable (Shift+F12)
- **Rename Symbol** - Rename a variable across all usages (F2)
- **Hover Information** - Rich hover with evaluated values and types for variables, builtin function signatures with examples, schema field tables, expect/secret declaration details
- **Completions** - Variables, keywords (including secret/policy/deny/warn), built-in functions, and schema-aware field completions
- **Schema-Aware Completions** - When `use SchemaName` is active, completions suggest missing required fields first, then optional fields

## Key Code Patterns

### Adding a new built-in function

Edit `src/evaluator/builtins.rs`:

```rust
pub fn call_builtin(name: &str, args: Vec<Value>, source: &str) -> HoneResult<Value> {
    match name {
        "my_func" => {
            check_arity(name, &args, 1)?;
            // Implementation
        }
        // ...
    }
}
```

### Adding a new AST node

1. Add to `src/parser/ast.rs`
2. Parse in `src/parser/mod.rs`
3. Evaluate in `src/evaluator/mod.rs`

### Testing

```bash
cargo test                    # All tests
cargo test test_name          # Specific test
cargo test --test integration_tests  # Integration only
```

## Example: Multi-file Projects

### Kubernetes Microservices Stack

```
examples/microservices/
├── main.hone       # Entry point: 7 K8s manifests via multi-document output
├── config.hone     # App name, registry, namespace
├── resources.hone  # CPU/memory limits per component
└── schemas.hone    # Output validation schemas
```

```bash
hone compile examples/microservices/main.hone --format yaml --output-dir ./manifests
hone compile examples/microservices/main.hone --format yaml --variant env=production --set image_tag=v2.0.0 --output-dir ./manifests
```

### Kubernetes with Schema Validation

```
examples/k8s-validated/
├── deployment.hone  # Deployment with full schema validation
├── service.hone     # Service with schema validation
└── full-stack.hone  # Multi-doc Deployment + Service
```

```bash
hone compile examples/k8s-validated/deployment.hone --format yaml
hone compile examples/k8s-validated/full-stack.hone --format yaml --output-dir /tmp/k8s
```

Uses the K8s schema library at `lib/k8s/v1.30/`. Import schemas and add `use Deployment` to get compile-time validation. Catches: wrong types (`replicas: "3"`), missing required fields (`containers`), type mismatches in nested structures.

### GitHub Actions CI/CD Pipeline

```
examples/ci-pipeline/
├── main.hone       # Full workflow: lint, test matrix, build, Docker, deploy
└── actions.hone    # Reusable action step patterns (checkout, setup, login)
```

```bash
hone compile examples/ci-pipeline/main.hone --format yaml
hone compile examples/ci-pipeline/main.hone --format yaml --variant deploy=production
```

## Build and Test

```bash
cargo test                    # All tests must pass
cargo clippy -- -D warnings   # Zero warnings required
cargo fmt -- --check          # Formatting enforced
scripts/verify-examples.sh ./target/release/hone  # All example checks
scripts/audit.sh              # Full launch audit (38 checks)
```

**Rust 1.93+ compatibility:** `#![allow(unused_assignments)]` is set in `lib.rs` and `main.rs` due to false positives from `thiserror`/`miette` derive macros on Rust 1.93+. Do not remove.

## Dependencies

- `tower-lsp` - LSP server framework
- `tokio` - Async runtime (for LSP)
- `serde` / `serde_json` / `serde_yaml` - Serialization
- `miette` - Error reporting
- `clap` - CLI parsing
- `indexmap` - Ordered maps for deterministic output
- `sha2` - SHA256 hashing for build cache
- `regex` - Pattern matching in schema string constraints
- `base64` - Base64 encoding/decoding builtins

## CI/CD

- `.github/workflows/ci.yml` - Tests, clippy, fmt, example compilation, WASM build
- `.github/workflows/release.yml` - Multi-platform binary releases on tag push (linux x86_64, macOS x86_64/aarch64, Windows)
- `scripts/install.sh` - Curl-based installer for GitHub releases
