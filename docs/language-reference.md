# Language Reference

Complete reference for the Hone configuration language.

## File structure

A Hone file has two sections:

1. **Preamble** -- declarations at the top: `let`, `import`, `from`, `schema`, `type`, `use`, `variant`, `expect`, `assert`, `secret`, `policy`
2. **Body** -- key-value pairs and blocks that produce output

```hone
# Preamble
let port = 8080
schema Config { port: int }
use Config

# Body
port: port
host: "localhost"
```

## Comments

```hone
# This is a line comment
key: value  # Inline comment
```

## Data types

| Type | Examples |
|---|---|
| Null | `null` |
| Boolean | `true`, `false` |
| Integer | `42`, `-17`, `0` |
| Float | `3.14`, `-0.5`, `1e10` |
| String | `"hello"`, `'literal'`, `"""multiline"""` |
| Array | `[1, 2, 3]` |
| Object | `{ key: "value" }` |

## Strings

### Double-quoted strings

Support escape sequences and `${}` interpolation:

```hone
let name = "world"
greeting: "Hello, ${name}!\n"
```

Escape sequences: `\\`, `\"`, `\n`, `\t`, `\r`.

### Single-quoted strings

Literal strings with no interpolation and no escapes (except `\\` and `\'`):

```hone
pattern: 'no ${interpolation} here'
```

### Multiline strings

Triple-quoted strings support interpolation and emit with YAML `|` block style:

```hone
let app = "myapp"
script: """
  #!/bin/bash
  echo "Deploying ${app}"
  kubectl apply -f manifests/
  """
```

## Variables

### `let` bindings

```hone
let name = "api"
let port = 8080
let enabled = true
let items = [1, 2, 3]
let config = { host: "localhost", port: 8080 }
```

All bindings are immutable. Variables are scoped to the file or block where they are defined.

### `expect` declarations

Self-documenting argument requirements. Values are injected via `--set` on the CLI:

```hone
expect args.env: string              # required, no default
expect args.port: int = 8080         # optional with default
expect args.debug: bool = false      # optional with default
```

```bash
hone compile config.hone --set env=production
```

If a required arg is missing, the error tells the user what to provide.

## Operators

### Arithmetic

`+`, `-`, `*`, `/`, `%`

```hone
let total = base_port + offset
let half = count / 2
```

### Comparison

`==`, `!=`, `<`, `<=`, `>`, `>=`

### Logical

`&&`, `||`, `!`

```hone
let ready = enabled && port > 0
let fallback = !ready || override
```

### Null coalescing

`??` returns the right operand if the left is null:

```hone
let host = custom_host ?? "localhost"
```

### Ternary

```hone
let replicas = env == "production" ? 5 : 1
```

### Operator precedence

From highest to lowest:

| Level | Operators | Associativity |
|---|---|---|
| 1 | `()` `[]` `.` | Left-to-right |
| 2 | `!` `-` (unary) | Right-to-left |
| 3 | `*` `/` `%` | Left-to-right |
| 4 | `+` `-` | Left-to-right |
| 5 | `<` `<=` `>` `>=` | Left-to-right |
| 6 | `==` `!=` | Left-to-right |
| 7 | `??` | Left-to-right |
| 8 | `&&` | Left-to-right |
| 9 | `\|\|` | Left-to-right |
| 10 | `? :` | Right-to-left |

## Output keys

### Simple assignment

```hone
key: value
```

### Block syntax

Newline-separated, no commas:

```hone
server {
  host: "localhost"
  port: 8080
}
```

### Inline syntax

Comma-separated:

```hone
server: { host: "localhost", port: 8080 }
```

### Assignment operators

| Operator | Behavior |
|---|---|
| `:` | Normal assign. Objects deep-merge, scalars/arrays replace. |
| `+:` | Append to array. |
| `!:` | Force replace (no merge). |

```hone
items +: ["extra"]               # append
config !: { completely: "new" }  # replace, don't merge
```

### Reserved words as keys

Keywords cannot be used as bare keys. Quote them:

```hone
"type": "Deployment"
"import": "some-module"
```

Reserved words: `let`, `import`, `from`, `schema`, `type`, `use`, `for`, `in`, `when`, `else`, `assert`, `variant`, `default`, `expect`, `secret`, `policy`, `deny`, `warn`, `true`, `false`, `null`.

## Conditionals

### Ternary expressions

```hone
replicas: env == "production" ? 5 : 1
```

### when/else blocks

Content merges into the parent scope. Exactly one branch is taken:

```hone
server {
  host: "localhost"
  port: 8080

  when env == "production" {
    host: "prod.example.com"
    tls: true
  } else when env == "staging" {
    host: "staging.example.com"
  } else {
    debug: true
  }
}
```

## Loops

### Array comprehensions

```hone
let doubled = for x in [1, 2, 3] { x * 2 }
# [2, 4, 6]
```

### Object comprehensions

When the body produces key-value pairs, a `for` loop generates an object:

```hone
let endpoints = for name in ["api", "web"] {
  "${name}_url": "https://${name}.example.com"
}
# { api_url: "https://api.example.com", web_url: "https://web.example.com" }
```

### Destructuring

Iterate over object key-value pairs:

```hone
let items = for (key, value) in { cpu: 2, memory: 4 } {
  "${key}_doubled": value * 2
}
```

### range()

Generate numeric sequences:

```hone
let ids = for i in range(0, 5) { "worker-${i}" }
# ["worker-0", "worker-1", "worker-2", "worker-3", "worker-4"]
```

`range(end)`, `range(start, end)`, and `range(start, end, step)` are all supported.

## Spread operator

Merge objects or concatenate arrays inline:

```hone
let base = { a: 1, b: 2 }
let extended = { ...base, c: 3 }   # { a: 1, b: 2, c: 3 }

let arr1 = [1, 2]
let arr2 = [...arr1, 3, 4]         # [1, 2, 3, 4]
```

## Deep merge

When the same key appears twice in the same scope, objects merge recursively. Scalars and arrays are replaced:

```hone
config {
  server {
    port: 8080
    host: "localhost"
  }
}

config {
  server {
    port: 9090
    # host remains "localhost"
  }
}
# Result: config.server = { port: 9090, host: "localhost" }
```

## Imports

### Module import

```hone
import "./config.hone" as config
port: config.default_port
```

### Named import

```hone
import { port, host } from "./settings.hone"
port: port
```

### Overlay (from)

Inherit all content from another file. Anything in the current file merges with or overrides the base:

```hone
from "./base.hone"
server {
  port: 9090  # overrides base value
}
```

## Variants

Define compile-time alternatives selected with `--variant`:

```hone
variant env {
  default dev {
    let replicas = 1
    let domain = "localhost"
  }
  staging {
    let replicas = 2
    let domain = "staging.example.com"
  }
  production {
    let replicas = 5
    let domain = "prod.example.com"
  }
}

replicas: replicas
url: "https://${domain}/api"
```

- `default` marks the case used when `--variant` is not specified
- Without a default, the `--variant` flag is required
- Multiple dimensions: `--variant env=prod --variant region=eu`
- `let` bindings inside variant cases are visible in the enclosing scope

## Type system

### Type aliases

```hone
type Port = int(1, 65535)
type Name = string(1, 100)
type Percentage = float(0.0, 1.0)
```

### Schemas

Define structural types:

```hone
schema Server {
  host: string
  port: Port
  name: string(1, 100)
  debug?: bool            # optional
}
```

Schemas are **closed by default**. Extra fields cause a compilation error. Use `...` to allow additional fields:

```hone
schema Flexible {
  name: string
  ...
}
```

### `use` statement

Apply a schema to validate the output:

```hone
use Server
```

### Supported types

| Type | Meaning |
|---|---|
| `int` | Any integer |
| `int(min, max)` | Integer in range (inclusive) |
| `float` | Any float |
| `float(min, max)` | Float in range |
| `string` | Any string |
| `string(min, max)` | String with length bounds |
| `string("regex")` | String matching regex |
| `bool` | Boolean |
| `object` | Any object |
| `array` | Any array |
| `SchemaName` | Reference to a named schema |

### `@unchecked` escape hatch

Bypass type checking on a specific value (emits a warning):

```hone
port: 99999 @unchecked
```

## Assertions

Runtime constraints:

```hone
assert port > 0 : "port must be positive"
assert port <= 65535 : "port must be valid"
assert contains(["dev", "staging", "prod"], env) : "unknown env: ${env}"
```

If the condition is false, compilation fails with the message.

## Secrets

Declare secret placeholders that are never evaluated to real values at compile time:

```hone
secret db_password from "vault:secret/data/db#password"
secret api_key from "env:API_KEY"

database {
  password: db_password
}
```

The compile-time value is always `<SECRET:provider:path>`. See [Advanced: Secrets](advanced/secrets.md) for modes.

## Policies

Output validation rules checked after compilation:

```hone
policy no_debug deny when output.debug == true {
  "debug must be disabled"
}

policy low_replicas warn when output.replicas < 2 {
  "consider increasing replicas"
}
```

- `deny` policies fail the build
- `warn` policies emit to stderr but succeed

See [Advanced: Policies](advanced/policies.md) for details.

## Multi-document output

Use `---name` to produce multiple output documents:

```hone
let app = "myapp"

---deployment
apiVersion: "apps/v1"
kind: "Deployment"
metadata { name: app }

---service
apiVersion: "v1"
kind: "Service"
metadata { name: "${app}-svc" }
```

Variables in the preamble are shared across all documents.

```bash
hone compile k8s.hone --output-dir ./manifests --format yaml
# Creates: manifests/deployment.yaml, manifests/service.yaml
```

## Built-in functions

### String functions

| Function | Signature | Description |
|---|---|---|
| `upper(s)` | `string -> string` | Uppercase |
| `lower(s)` | `string -> string` | Lowercase |
| `trim(s)` | `string -> string` | Trim whitespace |
| `split(s, d)` | `string, string -> [string]` | Split by delimiter |
| `join(arr, d)` | `[string], string -> string` | Join with delimiter |
| `replace(s, from, to)` | `string, string, string -> string` | Replace all occurrences |

### Encoding functions

| Function | Signature | Description |
|---|---|---|
| `base64_encode(s)` | `string -> string` | Encode to base64 |
| `base64_decode(s)` | `string -> string` | Decode from base64 |
| `to_json(v)` | `any -> string` | Serialize to JSON |
| `from_json(s)` | `string -> any` | Parse JSON |

### Collection functions

| Function | Signature | Description |
|---|---|---|
| `len(x)` | `string\|array\|object -> int` | Length |
| `keys(obj)` | `object -> [string]` | Object keys |
| `values(obj)` | `object -> [any]` | Object values |
| `contains(x, y)` | `array\|string\|object, any -> bool` | Containment check |
| `concat(arrays...)` | `array... -> array` | Concatenate arrays |
| `flatten(arr)` | `array -> array` | Flatten one level |
| `merge(objs...)` | `object... -> object` | Shallow merge (right wins) |
| `range(start, end, step?)` | `int... -> [int]` | Generate range |

### Conversion functions

| Function | Signature | Description |
|---|---|---|
| `to_int(v)` | `int\|float\|string\|bool -> int` | Convert to integer |
| `to_float(v)` | `int\|float\|string -> float` | Convert to float |
| `to_str(v)` | `scalar -> string` | Convert to string |
| `to_bool(v)` | `any -> bool` | Truthiness |
| `default(v, fallback)` | `any, any -> any` | Null coalescing |

### Environment functions

| Function | Signature | Description |
|---|---|---|
| `env(name, default?)` | `string -> string` | Read environment variable. Requires `--allow-env`. |
| `file(path)` | `string -> string` | Read file contents. Requires `--allow-env`. |

## Scoping rules

Hone uses lexical scoping:

1. **Local scope** -- `for` loop variables, `when` block doesn't create a new scope
2. **File scope** -- `let` bindings in the preamble
3. **Import scope** -- modules accessed via `import ... as alias`
4. **Built-in scope** -- built-in functions

Variant `let` bindings are visible in the enclosing file scope.
