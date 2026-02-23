# Hone

**A configuration language that compiles to JSON, YAML, TOML, and .env** -- variables, loops, imports, schemas, and deep merge for the configs you already write.

[Getting Started](docs/getting-started.md) | [Language Reference](docs/language-reference.md) | [CLI Reference](docs/cli-reference.md) | [Playground](https://honelang.github.io/hone/)

```hcl
let app = "api"
let env = "production"
let replicas = env == "production" ? 3 : 1

apiVersion: "apps/v1"
kind: "Deployment"
metadata {
  name: "${app}-${env}"
  labels { app: app, env: env }
}
spec {
  replicas: replicas
  template {
    spec {
      containers: [{
        name: app
        image: "registry.example.com/${app}:latest"
        ports: [{ containerPort: 8080 }]

        when env == "production" {
          resources {
            limits: { cpu: "2", memory: "4Gi" }
            requests: { cpu: "500m", memory: "1Gi" }
          }
        }
      }]
    }
  }
}
```

```bash
hone compile deploy.hone --format yaml --variant env=production
```

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: api-production
  labels:
    app: api
    env: production
spec:
  replicas: 3
  template:
    spec:
      containers:
        - name: api
          image: registry.example.com/api:latest
          ports:
            - containerPort: 8080
          resources:
            limits:
              cpu: "2"
              memory: 4Gi
            requests:
              cpu: 500m
              memory: 1Gi
```

---

## Why Hone?

Hone sits between raw YAML and full programming languages. It gives you just enough power to eliminate duplication and enforce structure, without turning your config into a codebase of its own.

| | Raw YAML | Helm Templates | Jsonnet | CUE | Dhall | **Hone** |
|---|---|---|---|---|---|---|
| Learning curve | None | Moderate (Go templates) | Steep | Steep | Steep | **Low** (reads like YAML) |
| Variables | Anchors/aliases | `{{ .Values }}` | `local` | `let` | `let` | **`let`** |
| Conditionals | No | `{{ if }}` | `if` | `if` | `if` | **`when` blocks, ternary** |
| Loops | No | `{{ range }}` | Comprehensions | Comprehensions | List ops | **`for..in` comprehensions** |
| Schema validation | No | JSON Schema (external) | No | Built-in (types) | Built-in (types) | **`schema` + `use` (compile-time)** |
| Multi-env configs | Copy files | values-dev.yaml + override | Mixins | Unification | No | **`variant` blocks** |
| Deep merge | Anchors (fragile) | No | `+:` | Unification | `//` operator | **Built-in, `!:` to replace** |
| Imports | No | Subcharts | `import` | Packages | `import` | **`import`, `from` (overlay)** |
| Hermetic builds | N/A | No | No | No | Yes | **Yes (`--allow-env` opt-in)** |
| Output formats | YAML only | YAML only | JSON only | JSON, YAML | JSON, YAML | **JSON, YAML, TOML, .env** |
| Editor support | Generic | Generic | LSP | LSP | LSP | **LSP (VS Code/Cursor)** |

---

## Quickstart

### Install from source

```bash
git clone https://github.com/honelang/hone.git
cd hone
cargo build --release
export PATH="$PATH:$(pwd)/target/release"
```

### Create a file

```bash
cat > hello.hone << 'EOF'
let name = "world"

greeting: "Hello, ${name}!"
items: for i in range(1, 4) { "item-${i}" }
EOF
```

### Compile

```bash
# To JSON (default)
hone compile hello.hone

# To YAML
hone compile hello.hone --format yaml
```

Output:

```yaml
greeting: Hello, world!
items:
  - item-1
  - item-2
  - item-3
```

---

## Core Features

### Variables and String Interpolation

```hcl
let env = "prod"
let port = 8080

url: "http://api-${env}:${port}"
computed: port + 1000
```

Multiline strings use triple quotes and support interpolation. The YAML emitter renders them with `|` block style:

```hcl
let app = "myapp"

script: """
  #!/bin/bash
  echo "Deploying ${app}"
  kubectl apply -f manifests/
  """
```

### Conditionals

Ternary expressions for inline values:

```hcl
let env = "production"
replicas: env == "production" ? 3 : 1
```

`when` blocks for conditional sections that merge into the parent:

```hcl
server {
  host: "localhost"
  port: 8080

  when env == "production" {
    host: "prod.example.com"    # overrides host above
    tls: true                   # adds new key
  }
}
```

### Loops

Array and object comprehensions with `for..in`:

```hcl
# Generate an array
ports: for p in [80, 443, 8080] { p }

# Generate object keys
endpoints: for name in ["api", "web", "admin"] {
  "${name}Url": "https://${name}.example.com"
}

# Destructure key-value pairs
doubled: for (key, value) in { cpu: 2, memory: 4 } {
  "${key}": value * 2
}

# Use range() for numeric sequences
ids: for i in range(0, 5) { "worker-${i}" }
```

### Imports and Composition

```hcl
# Import a module
import "./config.hone" as config
port: config.default_port

# Import specific names
import { replicas, image_tag } from "./env.hone"

# Overlay pattern: inherit and override
from "./base.hone"
server {
  port: 9090  # overrides base, keeps everything else
}
```

### Deep Merge

When the same key appears twice, objects are recursively merged. Scalars and arrays are replaced:

```hcl
# base.hone
config {
  server {
    port: 8080
    host: "localhost"
    logging { level: "info" }
  }
}

# overlay.hone
from "./base.hone"
config {
  server {
    port: 9090                   # overrides
    # host remains "localhost"
    logging { format: "json" }   # merges into logging
  }
}

# Result: config.server = { port: 9090, host: "localhost", logging: { level: "info", format: "json" } }
```

Use `+:` to append to arrays and `!:` to force-replace instead of merging:

```hcl
items +: ["extra"]               # appends to existing array
config !: { completely: "new" }  # replaces entire object, no merge
```

### Variants

Define environment-specific blocks selected at compile time with `--variant`:

```hcl
variant env {
  default dev {
    replicas: 1
    debug: true
    log_level: "debug"
  }

  staging {
    replicas: 2
    debug: false
    log_level: "info"
  }

  production {
    replicas: 5
    debug: false
    log_level: "warn"
  }
}

name: "api-server"
version: "2.1.0"
```

```bash
hone compile config.hone --variant env=production --format yaml
```

```yaml
replicas: 5
debug: false
log_level: warn
name: api-server
version: 2.1.0
```

The `default` keyword marks the case used when `--variant` is not specified. Without a default, the flag is required. Multiple variant dimensions are supported: `--variant env=prod --variant region=eu`.

### Schemas

Define schemas to validate output at compile time:

```hcl
type Port = int(1, 65535)

schema Server {
  host: string
  port: Port
  name: string(1, 100)   # string with length constraints
  debug?: bool            # optional field
}

use Server

host: "api.example.com"
port: 8080
name: "production-api"
```

Schemas are **closed by default** -- extra fields are rejected. Use `...` to allow additional fields:

```hcl
schema Flexible {
  name: string
  port: int
  ...           # allow extra fields
}
```

If the output violates the schema, compilation fails with a clear error:

```
TypeMismatch: expected int(1, 65535), found int (value: 99999)
  help: value 99999 is greater than maximum 65535
```

#### Kubernetes Schema Library

Hone ships with a pre-built schema library for common Kubernetes resource types in `lib/k8s/`. Import the schemas to get compile-time validation of your K8s manifests:

```hcl
import "../lib/k8s/v1.30/apps.hone" as apps
import "../lib/k8s/v1.30/core.hone" as core

use apps.DeploymentSpec

replicas: 3
selector {
  matchLabels { app: "api" }
}
template {
  metadata { labels { app: "api" } }
  spec {
    containers: [{
      name: "api"
      image: "registry.example.com/api:v1.0"
      ports: [{ containerPort: 8080 }]
    }]
  }
}
```

The library covers Deployments, Services, ConfigMaps, Ingresses, Jobs, RBAC, and more -- 78 schemas across 7 API groups. All schemas are open (extra fields allowed), so you get validation on the fields Hone knows about without blocking fields it doesn't. See `examples/k8s-validated/` for working examples.

Generate schemas for other K8s versions with:

```bash
python3 scripts/generate-k8s-schemas.py --version 1.31
```

### Assertions

Runtime constraints that fail the build if violated:

```hcl
let port = 8080
assert port > 0 && port < 65536 : "invalid port: ${port}"

let env = "staging"
assert contains(["dev", "staging", "production"], env) : "unknown environment: ${env}"
```

### Multi-Document Output

Use `---name` separators to produce multiple output files from a single source:

```hcl
let app = "myapp"
let env = "production"

---deployment
apiVersion: "apps/v1"
kind: "Deployment"
metadata { name: "${app}-${env}" }

---service
apiVersion: "v1"
kind: "Service"
metadata { name: "${app}-svc" }
```

```bash
hone compile k8s.hone --output-dir ./manifests --format yaml
# Creates: manifests/deployment.yaml, manifests/service.yaml
```

### Secrets

First-class secret placeholders that never leak into compiled output:

```hcl
secret db_password from "vault:secret/data/db#password"
secret api_key from "env:API_KEY"

database {
  host: "postgres.internal"
  password: db_password     # emits: <SECRET:vault:secret/data/db#password>
}
```

Control resolution with `--secrets-mode`: `placeholder` (default), `error` (fail if unresolved), or `env` (resolve `env:*` from environment).

### Policies

Output validation rules checked after compilation:

```hcl
policy no_debug deny when output.debug == true {
  "debug must be disabled in production"
}

policy port_range warn when output.port < 1024 {
  "privileged ports require elevated permissions"
}
```

---

## Built-in Functions

| Function | Description | Example |
|---|---|---|
| `len(x)` | Length of string, array, or object | `len("hello")` --> `5` |
| `keys(obj)` | Object keys as array | `keys({a: 1})` --> `["a"]` |
| `values(obj)` | Object values as array | `values({a: 1})` --> `[1]` |
| `contains(x, y)` | Check if x contains y | `contains([1, 2], 2)` --> `true` |
| `upper(s)` | Uppercase string | `upper("hi")` --> `"HI"` |
| `lower(s)` | Lowercase string | `lower("HI")` --> `"hi"` |
| `trim(s)` | Trim whitespace | `trim("  x  ")` --> `"x"` |
| `split(s, d)` | Split string by delimiter | `split("a,b", ",")` --> `["a", "b"]` |
| `join(arr, d)` | Join array with delimiter | `join(["a", "b"], "-")` --> `"a-b"` |
| `replace(s, from, to)` | Replace in string | `replace("ab", "b", "c")` --> `"ac"` |
| `range(start, end)` | Generate integer range (exclusive end) | `range(0, 3)` --> `[0, 1, 2]` |
| `base64_encode(s)` | Encode string to base64 | `base64_encode("hi")` --> `"aGk="` |
| `base64_decode(s)` | Decode base64 to string | `base64_decode("aGk=")` --> `"hi"` |
| `to_json(v)` | Serialize value to JSON string | `to_json({a: 1})` --> `"{\"a\":1}"` |
| `from_json(s)` | Parse JSON string to value | `from_json("{\"a\":1}")` --> `{a: 1}` |
| `env(name, default?)` | Read environment variable | `env("HOME")` |
| `file(path)` | Read file contents as string | `file("./data.txt")` |
| `concat(arrays...)` | Concatenate arrays | `concat([1], [2])` --> `[1, 2]` |
| `flatten(arr)` | Flatten nested arrays | `flatten([[1], [2]])` --> `[1, 2]` |
| `default(v, fallback)` | Return fallback if v is null | `default(null, "x")` --> `"x"` |
| `to_int(v)` | Convert to integer | `to_int("42")` --> `42` |
| `to_float(v)` | Convert to float | `to_float("3.14")` --> `3.14` |
| `to_str(v)` | Convert to string | `to_str(42)` --> `"42"` |
| `to_bool(v)` | Convert to bool (truthiness) | `to_bool(1)` --> `true` |
| `merge(objs...)` | Shallow merge objects (right wins) | `merge({a: 1}, {b: 2})` --> `{a: 1, b: 2}` |

**Note:** `env()` and `file()` require the `--allow-env` flag. Builds are hermetic by default.

---

## CLI Reference

```bash
hone compile file.hone                          # JSON to stdout
hone compile file.hone --format yaml            # YAML to stdout
hone compile file.hone --format toml            # TOML to stdout
hone compile file.hone --format dotenv          # .env to stdout
hone compile file.hone -o output.yaml           # Write to file (format from extension)
hone compile file.hone --output-dir ./manifests # Multi-document to separate files
hone compile file.hone --variant env=production # Select variant
hone compile file.hone --set replicas=5         # Inject args.replicas
hone compile file.hone --set-file ca=./ca.pem   # Inject args.ca from file
hone compile file.hone --allow-env              # Allow env() and file()
hone compile file.hone --no-cache               # Skip build cache
hone compile file.hone --secrets-mode error     # Fail if unresolved secrets
hone compile file.hone --ignore-policy          # Skip policy checks
hone compile file.hone --strict                 # Treat warnings as errors

hone check file.hone                            # Validate syntax and types
hone check file.hone --variant env=production   # Validate specific variant

hone fmt file.hone                              # Print formatted to stdout
hone fmt --write file.hone                      # Format in place
hone fmt --check .                              # CI: exit 1 if unformatted

hone diff file.hone --base main                 # Current vs git ref
hone diff file.hone --left "env=dev" --right "env=production"
hone diff file.hone --base main --detect-moves --blame

hone import config.yaml -o config.hone          # Convert YAML to Hone
hone import config.yaml --extract-vars          # Detect repeated values

hone graph main.hone                            # Text dependency tree
hone graph main.hone --format dot               # Graphviz DOT

hone typegen schema.json -o types.hone          # JSON Schema to Hone schemas

hone cache clean                                # Clear build cache
hone cache clean --older-than 7d                # Clear old entries

hone lsp --stdio                                # Start language server
```

See [full CLI reference](docs/cli-reference.md) for all options.

---

## Editor Support

### VS Code / Cursor

An extension is included in the `editors/vscode/` directory, providing:

- Syntax highlighting
- Real-time error diagnostics
- Hover information (types and docs)
- Autocompletion (variables, keywords, built-in functions)
- Go to Definition (Ctrl+Click or F12)
- Find All References (Shift+F12)
- Rename Symbol (F2)
- Format on Save

To install:

```bash
cd editors/vscode
npm install && npm run compile
ln -s "$(pwd)" ~/.vscode/extensions/hone-lang-0.1.0
```

See [editor setup guide](docs/editor-setup.md) for Neovim, Helix, and Sublime Text.

### Claude Code

Install the Hone skill so Claude Code writes correct `.hone` files in your projects:

```bash
mkdir -p ~/.claude/skills/hone && curl -fsSL https://raw.githubusercontent.com/honelang/hone/main/.claude/skills/hone/SKILL.md -o ~/.claude/skills/hone/SKILL.md
```

This gives Claude full knowledge of Hone syntax, patterns, and common pitfalls. The skill activates automatically when working with `.hone` files.

---

## Playground

Try Hone in your browser at **[honelang.github.io/hone](https://honelang.github.io/hone/)** -- edit source on the left, see compiled output on the right in real time. No installation required.

The playground is powered by a WebAssembly build of the full Hone compiler. Source is in `playground/`.

---

## Documentation

- [Getting Started](docs/getting-started.md) -- zero to first file in 5 minutes
- [Language Reference](docs/language-reference.md) -- complete syntax and semantics
- [CLI Reference](docs/cli-reference.md) -- all commands and flags
- [Editor Setup](docs/editor-setup.md) -- VS Code, Neovim, Helix, Sublime Text
- [Error Catalog](docs/errors.md) -- every error code explained
- **Libraries:**
  - [Kubernetes Schemas](lib/k8s/) -- pre-built schemas for common K8s resource types (v1.30)
- **Advanced:**
  - [Secrets](docs/advanced/secrets.md) -- secret placeholder management
  - [Policies](docs/advanced/policies.md) -- output validation rules
  - [Build Cache](docs/advanced/cache.md) -- content-addressed caching
  - [Type Generation](docs/advanced/typegen.md) -- generate schemas from JSON Schema

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, project structure, and contribution guidelines.

## License

MIT
