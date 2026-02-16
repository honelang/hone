# Hone

## Hero

**Configuration, without the pain.**

Hone is a configuration language that compiles to JSON and YAML. Variables, type checking, multi-file composition, and 30+ built-in functions -- in a syntax your team already knows how to read.

[Try in Playground](#playground) | [Install](#installation)

---

## The Problem

YAML was designed for simple data serialization. Teams adopted it for Kubernetes manifests, Helm values, Ansible playbooks, CI pipelines, and infrastructure-as-code. Then the complexity grew.

**Duplication everywhere.** The same labels, annotations, tolerations, and resource limits copy-pasted across dozens of files. Change a container registry URL? Find and replace across 40 manifests and hope you didn't miss one.

**No variables.** YAML has no concept of a variable. Helm invented `{{ .Values.x }}` as a workaround. Kustomize patches around it. Ansible layers Jinja2 on top. Every tool reinvents the same missing feature with its own templating dialect.

**No validation until deploy time.** A typo in a port number, an invalid replica count, a missing required field -- you find out when `kubectl apply` fails, or worse, when production breaks at 2am. YAML has no type system. The feedback loop is measured in minutes, sometimes hours.

**Anchors are unreadable.** YAML's built-in reuse mechanism (`&anchor` / `*alias`) is cryptic, limited to a single file, and breaks the moment you need to override a nested field.

**The result:** teams either suffer through raw YAML, or bolt on a templating engine that turns their configuration into a programming language they never signed up for. Neither option is good.

---

## The Solution

Hone gives you the missing features YAML should have had, without the complexity of a general-purpose language.

**Before: 70 lines of YAML with duplication**

```yaml
# dev-deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: api-server-dev
  namespace: myapp-dev
  labels:
    app: api-server
    env: dev
    team: platform
spec:
  replicas: 1
  selector:
    matchLabels:
      app: api-server
      env: dev
  template:
    metadata:
      labels:
        app: api-server
        env: dev
        team: platform
    spec:
      containers:
        - name: api-server
          image: registry.internal/api-server:1.4.2
          ports:
            - containerPort: 8080
          env:
            - name: LOG_LEVEL
              value: "debug"
            - name: DB_HOST
              value: "postgres-dev.internal"
          resources:
            requests:
              cpu: "250m"
              memory: "512Mi"
            limits:
              cpu: "500m"
              memory: "1Gi"

# prod-deployment.yaml (nearly identical, with scattered differences)
apiVersion: apps/v1
kind: Deployment
metadata:
  name: api-server-production
  namespace: myapp-production
  labels:
    app: api-server
    env: production
    team: platform
spec:
  replicas: 5
  selector:
    matchLabels:
      app: api-server
      env: production
  template:
    metadata:
      labels:
        app: api-server
        env: production
        team: platform
    spec:
      containers:
        - name: api-server
          image: registry.internal/api-server:1.4.2
          ports:
            - containerPort: 8080
          env:
            - name: LOG_LEVEL
              value: "warn"
            - name: DB_HOST
              value: "postgres-prod.internal"
          resources:
            requests:
              cpu: "1000m"
              memory: "2Gi"
            limits:
              cpu: "4000m"
              memory: "8Gi"
```

**After: 30 lines of Hone, both environments from one source**

```hone
let app = "api-server"
let image_tag = "1.4.2"

variant env {
  default dev {
    replicas: 1
    log_level: "debug"
    db_host: "postgres-dev.internal"
    resources: { requests: { cpu: "250m", memory: "512Mi" }, limits: { cpu: "500m", memory: "1Gi" } }
  }
  production {
    replicas: 5
    log_level: "warn"
    db_host: "postgres-prod.internal"
    resources: { requests: { cpu: "1000m", memory: "2Gi" }, limits: { cpu: "4000m", memory: "8Gi" } }
  }
}

apiVersion: "apps/v1"
kind: "Deployment"
metadata {
  name: "${app}-${env}"
  namespace: "myapp-${env}"
  labels { app: app, env: env, team: "platform" }
}
spec {
  replicas: replicas
  selector { matchLabels { app: app, env: env } }
  template {
    metadata { labels { app: app, env: env, team: "platform" } }
    spec {
      containers: [{
        name: app
        image: "registry.internal/${app}:${image_tag}"
        ports: [{ containerPort: 8080 }]
        env: [
          { name: "LOG_LEVEL", value: log_level },
          { name: "DB_HOST", value: db_host },
        ]
        resources: resources
      }]
    }
  }
}
```

```bash
hone compile deploy.hone --variant env=dev --format yaml
hone compile deploy.hone --variant env=production --format yaml
```

One source file. Two environments. Zero duplication. Change the image tag once and it propagates everywhere.

---

## Feature Highlights

### Variables and String Interpolation

Define values once, reference them everywhere. String interpolation with `${}` eliminates string concatenation and the need for templating engines.

```hone
let registry = "registry.internal"
let app = "payment-service"
let version = "2.1.0"

image: "${registry}/${app}:${version}"
name: "${app}-deployment"
```

No `{{ }}` escaping. No Jinja2 filters. No gotpl pipelines. Just straightforward variable substitution.

### Environment Variants

Define environment-specific configuration in a single file using `variant` blocks. Select which variant to compile with `--variant` on the command line.

```hone
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

# The selected variant's values merge into the output
deployment {
  replicas: replicas
  debug: debug
}
```

```bash
hone compile config.hone --variant env=production --format yaml
```

Multiple variant dimensions are supported. Use `--variant env=prod --variant region=eu` for matrix configurations.

### Schema Validation with Type Constraints

Catch configuration errors at compile time, not at deploy time. Define schemas with type constraints and Hone validates your output before it ever reaches your cluster.

```hone
type Port = int(1, 65535)

schema ServiceConfig {
  name: string(1, 63)        # 1-63 characters (K8s name limit)
  port: Port
  replicas: int(1, 100)
  debug?: bool               # optional field
}

use ServiceConfig

name: "api-gateway"
port: 8080
replicas: 3
```

If someone sets `port: 99999`, the compiler rejects it immediately:

```
TypeMismatch: expected int(1, 65535), found int (value: 99999)
  help: value 99999 is greater than maximum 65535
```

No more discovering invalid configuration in production.

### Multi-File Composition

Split large configurations across files. Import modules, inherit from base files, and let deep merge combine them. No more 2,000-line monolithic values files.

```hone
# base.hone
metadata {
  labels {
    managed-by: "hone"
    team: "platform"
  }
}
resources {
  requests { cpu: "250m", memory: "512Mi" }
  limits { cpu: "500m", memory: "1Gi" }
}
```

```hone
# production.hone
from "./base.hone"

metadata {
  labels {
    env: "production"   # merged with base labels
  }
}
resources {
  requests { cpu: "2000m", memory: "4Gi" }  # overrides base
  limits { cpu: "4000m", memory: "8Gi" }
}
```

The `from` keyword deep-merges the child into the base. Override specific fields while inheriting everything else. No copy-paste. No patch files. No strategic merge gymnastics.

### 30+ Built-in Functions

Base64 encoding, JSON serialization, string manipulation, array operations, and more -- all available without importing external libraries.

```hone
# Base64-encode a secret
let raw_password = "s3cret-value"
password: base64_encode(raw_password)

# Generate port mappings from a list
let services = ["http", "grpc", "metrics"]
let port_base = 8080
ports: for (i, name) in services {
  "${name}": port_base + i
}
# { http: 8080, grpc: 8081, metrics: 8082 }

# Filter with a for comprehension
let all_namespaces = ["yeetops", "yeetops-kafka", "monitoring", "kube-system"]
let app_namespaces = for ns in all_namespaces { when contains(ns, "yeetops") { ns } }
# ["yeetops", "yeetops-kafka"]
```

Full list includes: `len`, `keys`, `values`, `contains`, `upper`, `lower`, `trim`, `split`, `join`, `replace`, `range`, `flatten`, `concat`, `base64_encode`, `base64_decode`, `to_json`, `from_json`, `default`, `to_int`, `to_str`, `env`, `file`, and more. For transforming collections, use for comprehensions: `for x in items { x * 2 }`.

### Hermetic Builds

By default, Hone compilation is deterministic. The `env()` and `file()` functions are gated behind the `--allow-env` flag, so builds are reproducible unless you explicitly opt in to external inputs.

```hone
# This fails without --allow-env
let cloud_key = env("CLOUD_API_KEY")
```

```bash
# Deterministic (default)
hone compile config.hone --format yaml

# Explicitly allow environment access
hone compile config.hone --format yaml --allow-env
```

This means `hone compile` on the same source always produces the same output -- critical for GitOps workflows, CI pipelines, and auditable infrastructure.

---

## How It Compares

| Capability | Hone | Jsonnet | CUE | Helm | Kustomize |
|---|---|---|---|---|---|
| **Variables** | Native `let` bindings | Local/top-level | Definitions | Go templates | N/A |
| **Type checking** | Schemas with constraints | None | Comprehensive | None | None |
| **String interpolation** | `"${var}"` | `"%s" % var` | `"\(var)"` | `{{ .Values.var }}` | N/A |
| **Multi-file imports** | `import` / `from` | `import` | Packages | Subcharts | Overlays |
| **Deep merge** | Built-in | `+:` operator | Unification | `--merge` | Strategic merge patches |
| **Conditionals** | `when` blocks, ternary | `if/else` | Guards | `{{ if }}` | N/A |
| **Loops** | `for` comprehensions | Array/object comp | Comprehensions | `{{ range }}` | N/A |
| **Environment variants** | `variant` blocks | N/A | N/A | Multiple values files | Overlays |
| **Schema validation** | Compile-time | N/A | Compile-time | JSON Schema (external) | N/A |
| **IDE support** | LSP (VS Code) | LSP | LSP | YAML schema | YAML schema |
| **Output format** | JSON, YAML | JSON | JSON, YAML | YAML | YAML |
| **Learning curve** | Low (YAML-like syntax) | Medium (functional) | High (lattice theory) | Medium (Go templates) | Low (patches only) |
| **Multi-doc output** | `---name` documents | Multiple files | Packages | Multiple templates | Overlays |
| **Existing config import** | `hone import` | Manual | Manual | N/A | N/A |
| **Hermetic builds** | Default (opt-in env) | Default | Default | Requires discipline | Default |

**When to choose Hone over the alternatives:**

- Over **Helm**: When you want a real language instead of Go templates inside YAML. When you need type validation. When you're tired of debugging `{{ indent 8 }}`.
- Over **Kustomize**: When patches aren't expressive enough. When you need variables, loops, or conditional logic.
- Over **Jsonnet**: When you want a lower learning curve and YAML-native output. When you want compile-time type checking.
- Over **CUE**: When you want a simpler mental model. CUE's lattice-based type system is powerful but has a steep learning curve. Hone offers 80% of the validation benefit with 20% of the complexity.

---

## IDE Support

Hone ships with a Language Server Protocol (LSP) implementation and a VS Code / Cursor extension.

**Features:**

- Real-time diagnostics -- syntax errors and type mismatches as you type
- Go to Definition -- jump to any variable or import (F12)
- Find All References -- see every usage of a variable (Shift+F12)
- Rename Symbol -- rename across files (F2)
- Hover information -- type and value details on hover
- Completions -- variables, keywords, and all built-in functions
- Format on save -- automatic source formatting

**Installation:**

Install the `hone-lang` extension from the VS Code marketplace, or build from source:

```
editors/vscode/    # Extension source
```

The extension automatically starts the LSP server (`hone lsp --stdio`) and connects to it.

---

## Installation

### From source (Rust toolchain required)

```bash
git clone https://github.com/honelang/hone.git
cd hone
cargo install --path .
```

### Verify installation

```bash
hone --version
```

### Quick start

```bash
# Create a file
cat > hello.hone << 'EOF'
let greeting = "hello"
let target = "world"

message: "${greeting}, ${target}"
count: 42
tags: ["config", "demo"]
EOF

# Compile to JSON
hone compile hello.hone

# Compile to YAML
hone compile hello.hone --format yaml

# Import an existing YAML file
hone import existing-config.yaml -o config.hone

# Validate without output
hone check hello.hone
```

### Migrate existing YAML

Hone can convert your existing YAML and JSON files into Hone source, automatically detecting repeated values and extracting them as variables:

```bash
hone import values.yaml -o values.hone --extract-vars
```

---

## Start Shipping Configuration You Can Trust

Hone eliminates the gap between "configuration data" and "configuration logic" without dragging you into a full programming language. Define your infrastructure once, validate it at compile time, and generate correct YAML for every environment.

[Try in Playground](#playground) | [Read the Documentation](#docs) | [View on GitHub](https://github.com/honelang/hone)
