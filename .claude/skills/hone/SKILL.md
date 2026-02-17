---
name: hone
description: Use when writing, editing, or generating Hone configuration language files (.hone). Automatically activates when the user asks to create configs, convert YAML/JSON to Hone, or work with .hone files.
---

# Hone Configuration Language — Writing Guide

You are writing **Hone**, a configuration language that compiles to JSON, YAML, TOML, and .env. Think of it as "what YAML should have been" — structured, typed, composable, with real variables and logic.

## Quick Reference

### File Structure

A Hone file has two sections: **preamble** (let/import/expect/secret/schema/type declarations) then **body** (output key-value pairs).

```hone
# === Preamble ===
let app = "myapp"
let port = 8080
import "./base.hone" as base

# === Body (this becomes the output) ===
name: app
port: port
url: "http://localhost:${port}"
```

### Data Types

| Type | Syntax |
|------|--------|
| Null | `null` |
| Bool | `true`, `false` |
| Int | `42`, `-17` |
| Float | `3.14`, `1e10` |
| String | `"interpolated ${var}"`, `'literal no interpolation'`, `"""multiline"""` |
| Array | `[1, 2, 3]` |
| Object | `{ key: "value" }` or block syntax below |

### Block vs Inline

```hone
# Block syntax — newline-separated, NO commas
server {
  host: "localhost"
  port: 8080
}

# Inline syntax — comma-separated
server: { host: "localhost", port: 8080 }
```

**CRITICAL**: Never use commas inside block `{ }` syntax. Commas are ONLY for inline objects and arrays.

### Variables

```hone
let name = "api"
let count = 3
let config = { host: "localhost", port: 8080 }
let items = [1, 2, 3]
```

### String Interpolation

Only works in double-quoted strings. Use `${expr}` syntax:

```hone
let env = "prod"
name: "${env}-api"            # "prod-api"
url: "http://localhost:${8000 + 80}"  # expressions work
```

Single-quoted strings are literal — no interpolation: `'${this} stays as-is'`

### Conditionals

```hone
# Ternary expression
replicas: env == "prod" ? 5 : 1

# When blocks (merge into parent)
when env == "production" {
  replicas: 5
  debug: false
} else when env == "staging" {
  replicas: 2
} else {
  replicas: 1
}
```

### Loops

```hone
# For-as-expression → array
let doubled = for x in [1, 2, 3] { x * 2 }   # [2, 4, 6]

# For-as-expression with string keys → array of objects
let list = for env in ["dev", "prod"] {
  "${env}": "https://${env}.example.com"
}
# Result: [ {"dev": "https://dev.example.com"}, {"prod": "https://prod.example.com"} ]

# For-in-body → flat dict (keys merge into parent object)
endpoints {
  for env in ["dev", "staging", "prod"] {
    "${env}": "https://${env}.example.com"
  }
}
# Result: endpoints = { dev: "https://...", staging: "https://...", prod: "https://..." }

# Destructuring objects
for (key, value) in { a: 1, b: 2 } {
  "${key}_doubled": value * 2
}
```

**KEY DISTINCTION**: `let x = for ...` produces an **array**. A `for` inside a block body produces a **flat dict** by merging.

### Imports

```hone
import "./config.hone" as config     # Module import
import { port, host } from "./net.hone"  # Named import
from "./base.hone"                   # Inheritance (deep merge on top of base)
```

### Dynamic Keys

Three forms:

```hone
# 1. Interpolated string key
let prefix = "app"
"${prefix}-name": "my-service"

# 2. Computed key [expression]
let key = "dynamic"
[key]: 42

# 3. Plain quoted key (for reserved words)
"type": "Deployment"
"import": "some-module"
```

### Spread Operator

```hone
let base = { a: 1, b: 2 }
extended: { ...base, c: 3 }

let arr = [1, 2]
more: [...arr, 3, 4]
```

### Assignment Operators

```hone
key: value       # Normal (deep merge for objects)
items +: ["new"]  # Append to array
config !: { x: 1 }  # Force replace (skip merge)
```

### Schemas & Validation

```hone
schema Server {
  host: string
  port: int(1, 65535)
  name: string(1, 100)      # string with length range
  debug?: bool              # optional field
  ...                       # allow extra fields (open schema)
}

use Server                   # activate validation

host: "api.example.com"
port: 8080
name: "prod-api"
```

Schemas are **closed by default** — extra fields cause errors. Add `...` to allow them.

### Variants

```hone
variant env {
  default dev {
    replicas: 1
  }
  production {
    replicas: 5
  }
}

name: "api"
# Compile: hone compile file.hone --variant env=production
```

### Expect (CLI Args)

```hone
expect args.env: string              # required
expect args.port: int = 8080         # optional with default
# Compile: hone compile file.hone --set env=prod
```

### Multi-Document Output

```hone
let app = "myapp"

---deployment
apiVersion: "apps/v1"
kind: "Deployment"
metadata {
  name: app
}

---service
apiVersion: "v1"
kind: "Service"
metadata {
  name: "${app}-svc"
}
# Compile: hone compile k8s.hone --output-dir ./manifests --format yaml
```

### Built-in Functions

`len`, `keys`, `values`, `contains`, `upper`, `lower`, `trim`, `split`, `join`, `replace`, `range`, `base64_encode`, `base64_decode`, `to_json`, `from_json`, `env`, `file`, `concat`, `flatten`, `default`, `to_int`, `to_float`, `to_str`, `to_bool`, `merge`

### Assertions & Policies

```hone
assert port > 0 : "port must be positive"

policy no_debug deny when output.debug == true {
  "debug must not be enabled"
}
```

## Common Pitfalls — Avoid These

### 1. Commas in blocks
```hone
# WRONG — commas in block syntax
server {
  host: "localhost",   # ← NO
  port: 8080,         # ← NO
}

# RIGHT
server {
  host: "localhost"
  port: 8080
}
```

### 2. Reserved words as bare keys
```hone
# WRONG
type: "Deployment"   # 'type' is reserved

# RIGHT
"type": "Deployment"
```

Reserved words that must be quoted as keys: `type`, `schema`, `import`, `from`, `for`, `when`, `else`, `let`, `expect`, `secret`, `policy`, `use`, `assert`, `variant`, `deny`, `warn`, `default`, `extends`, `in`, `as`

### 3. Confusing string types
```hone
# Double-quoted: interpolation ON
name: "hello ${world}"    # → "hello <value>"

# Single-quoted: interpolation OFF (literal)
pattern: 'hello ${world}'  # → "hello ${world}" literally

# Triple-quoted: multiline with interpolation
desc: """
  Line one
  Line two about ${thing}
"""
```

### 4. Null propagation
```hone
let x = null
"${x}"           # → "null" (string)
x + 1            # → ERROR
obj.missing_key  # → null (silent)
x ?? "fallback"  # → "fallback"
```

### 5. For loop body vs expression
```hone
# EXPRESSION — produces array of objects
let x = for i in items { "k": i }
# → [{k: item1}, {k: item2}, ...]

# BODY — merges into flat object
result {
  for i in items { "k_${i}": i }
}
# → {k_item1: item1, k_item2: item2, ...}
```

### 6. Boolean strings for target systems
```hone
# Some systems need "true" not true
enabled: to_str(true)  # → "true" (string)
```

## Patterns for Real Configs

### Kubernetes
```hone
let app = "myapp"
let env = "prod"

apiVersion: "apps/v1"
kind: "Deployment"
metadata {
  name: "${app}-${env}"
  labels {
    "app.kubernetes.io/name": app
    "app.kubernetes.io/instance": env
  }
}
spec {
  replicas: env == "prod" ? 3 : 1
  template {
    spec {
      containers: [{
        name: app,
        image: "registry.example.com/${app}:latest",
        ports: [{ containerPort: 8080 }]
      }]
    }
  }
}
```

### Environment-specific configs
```hone
variant env {
  default dev {
    let db_host = "localhost"
    let replicas = 1
  }
  production {
    let db_host = "db.prod.internal"
    let replicas = 5
  }
}

database {
  host: db_host
  port: 5432
}
replicas: replicas
```

### Reusable patterns with imports
```hone
# base.hone
let app = "myapp"
metadata {
  labels {
    app: app
    managed_by: "hone"
  }
}

# overlay.hone
from "./base.hone"
metadata {
  labels {
    env: "production"  # merges with base labels
  }
}
```

## Style Guidelines

1. **Use block syntax** for multi-key objects, inline for single-key or short objects
2. **Group related lets** at the top of the preamble
3. **Use descriptive variable names** — `let api_port = 8080` not `let p = 8080`
4. **Use variants** for environment differences, not nested ternaries
5. **Use schemas** for any config that will be consumed by others
6. **Prefer `from` inheritance** over manual merge for overlay patterns
7. **Use `expect`** for any values that should come from CLI args
