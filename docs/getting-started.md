# Getting Started with Hone

This guide takes you from zero to compiling your first Hone file in under five minutes.

## Install

### From source (requires Rust toolchain)

```bash
git clone https://github.com/honelang/hone.git
cd hone
cargo build --release
export PATH="$PATH:$(pwd)/target/release"
```

Verify the installation:

```bash
hone --version
```

## Your first file

Create a file called `hello.hone`:

```hone
let greeting = "Hello"
let target = "World"

message: "${greeting}, ${target}"
version: 1
enabled: true
tags: ["intro", "example"]
```

Compile it:

```bash
hone compile hello.hone
```

Output (JSON):

```json
{
  "enabled": true,
  "message": "Hello, World",
  "tags": [
    "intro",
    "example"
  ],
  "version": 1
}
```

For YAML output:

```bash
hone compile hello.hone --format yaml
```

```yaml
enabled: true
message: Hello, World
tags:
  - intro
  - example
version: 1
```

## Key concepts

### Variables

Use `let` to define variables. Reference them by name or inside `${}` interpolation:

```hone
let env = "production"
let port = 8080

url: "http://api-${env}:${port}"
computed: port + 1000
```

### Nested objects

Use block syntax with braces:

```hone
server {
  host: "localhost"
  port: 8080
  logging {
    level: "info"
    format: "json"
  }
}
```

### Conditionals

Ternary expressions work inline:

```hone
let env = "production"
replicas: env == "production" ? 3 : 1
```

`when` blocks merge conditional sections into the parent:

```hone
server {
  host: "localhost"

  when env == "production" {
    host: "prod.example.com"
    tls: true
  }
}
```

### Loops

Generate arrays or objects with `for..in`:

```hone
ports: for p in [80, 443, 8080] { p }

endpoints: for name in ["api", "web"] {
  "${name}_url": "https://${name}.example.com"
}
```

### Schemas

Add compile-time validation with `schema` and `use`:

```hone
schema Config {
  host: string
  port: int(1, 65535)
  debug?: bool
}

use Config

host: "localhost"
port: 8080
```

If `port` were `99999`, compilation fails:

```
TypeMismatch: expected int(1, 65535), found int (value: 99999)
  help: value 99999 is greater than maximum 65535
```

## Multi-environment configs

Use `variant` blocks to define environment-specific values:

```hone
variant env {
  default dev {
    let replicas = 1
    let log_level = "debug"
  }
  production {
    let replicas = 5
    let log_level = "warn"
  }
}

name: "my-app"
replicas: replicas
log_level: log_level
```

Compile with a specific variant:

```bash
hone compile config.hone --format yaml --variant env=production
```

## Importing other files

Split large configs into modules:

```hone
# base.hone
let default_port = 8080
let default_host = "localhost"
```

```hone
# main.hone
import "./base.hone" as base

server {
  host: base.default_host
  port: base.default_port
}
```

Or use overlay inheritance:

```hone
# production.hone
from "./base.hone"

server {
  host: "prod.example.com"   # overrides base
  # port: 8080 inherited from base
}
```

## Output formats

Hone compiles to multiple formats:

```bash
hone compile config.hone                  # JSON (pretty)
hone compile config.hone --format yaml    # YAML
hone compile config.hone --format json    # JSON
hone compile config.hone --format toml    # TOML
hone compile config.hone --format dotenv  # .env
```

## Editor support

Install the VS Code / Cursor extension for syntax highlighting, error diagnostics, hover info, autocompletion, and go-to-definition. See [Editor Setup](editor-setup.md).

## Next steps

- [Language Reference](language-reference.md) -- complete syntax and semantics
- [CLI Reference](cli-reference.md) -- all commands and flags
- [Advanced: Secrets](advanced/secrets.md) -- secret placeholder management
- [Advanced: Policies](advanced/policies.md) -- output validation rules
- [Advanced: Build Cache](advanced/cache.md) -- content-addressed caching
- [Advanced: Type Generation](advanced/typegen.md) -- generate schemas from JSON Schema
- [Error Catalog](errors.md) -- every error code explained
