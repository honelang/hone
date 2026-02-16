# CLI Reference

Complete reference for the `hone` command-line tool.

## Global

```
hone [COMMAND] [OPTIONS]
hone --version
hone --help
```

## Commands

### `hone compile` -- Compile source to output

Compiles a `.hone` file (or stdin) and writes JSON, YAML, TOML, or .env output.

```bash
hone compile <FILE> [OPTIONS]
```

| Option | Description |
|---|---|
| `<FILE>` | Source file. Use `-` or `/dev/stdin` for stdin. |
| `-o, --output <PATH>` | Write output to file. Format inferred from extension (.yaml, .json, .toml). |
| `-f, --format <FMT>` | Force output format: `json`, `yaml`, `toml`, `dotenv`. |
| `--output-dir <DIR>` | Write each `---name` document to a separate file in this directory. |
| `--variant <NAME=CASE>` | Select a variant case. Repeatable for multiple variant dimensions. |
| `--set <KEY=VAL>` | Inject a value into the `args.*` namespace. Repeatable. |
| `--set-file <KEY=PATH>` | Read the value from a file. Repeatable. |
| `--set-string <KEY=VAL>` | Inject as string (no type inference). Repeatable. |
| `--allow-env` | Enable `env()` and `file()` builtins. |
| `--no-cache` | Disable the build cache. |
| `--secrets-mode <MODE>` | Secret handling: `placeholder` (default), `error`, `env`. |
| `--ignore-policy` | Skip all policy checks. |
| `--strict` | Treat warnings as errors. |
| `--quiet` | Suppress warnings. |
| `--dry-run` | Print output to stdout instead of writing files. |

**Output format resolution order:**
1. `--format` flag (explicit)
2. Output file extension (`-o output.yaml` implies YAML)
3. `--output-dir` present implies YAML
4. Default: JSON pretty

**Examples:**

```bash
# Basic compilation
hone compile config.hone --format yaml

# With variants and args
hone compile config.hone --variant env=production --set db_host=db.internal

# Multi-document output
hone compile k8s.hone --output-dir ./manifests --format yaml

# Stdin
echo 'name: "test"' | hone compile - --format yaml

# Strict mode (warnings become errors)
hone compile config.hone --strict
```

---

### `hone check` -- Validate without output

Parses, resolves imports, evaluates, and type-checks the file without emitting output.

```bash
hone check <FILE> [OPTIONS]
```

| Option | Description |
|---|---|
| `<FILE>` | Source file. Supports `-` for stdin. |
| `--variant <NAME=CASE>` | Select variant case. Repeatable. |
| `--set <KEY=VAL>` | Inject args. Repeatable. |
| `--schema <NAME>` | Validate against a specific named schema. |
| `--allow-env` | Enable `env()` and `file()` builtins. |

**Examples:**

```bash
hone check config.hone
hone check config.hone --variant env=production
hone check config.hone --schema Server
```

---

### `hone fmt` -- Format source files

Formats `.hone` source files with consistent style (2-space indent, canonical brace placement). Preserves comments.

```bash
hone fmt <FILES...> [OPTIONS]
```

| Option | Description |
|---|---|
| `<FILES...>` | Files or directories. Directories are scanned recursively for `.hone` files. |
| `-w, --write` | Write formatted output back to the source files. |
| `--check` | Exit with code 1 if any file is not formatted. For CI. |
| `--diff` | Print a diff of changes that would be made. |

Without flags, formatted output is printed to stdout.

**Examples:**

```bash
hone fmt config.hone              # print formatted to stdout
hone fmt --write config.hone      # format in place
hone fmt --check .                # CI check: all .hone files formatted?
hone fmt --diff config.hone       # preview changes
```

---

### `hone diff` -- Compare compilation outputs

Compiles a file under two different conditions and shows a structural diff.

```bash
hone diff <FILE> [OPTIONS]
```

| Option | Description |
|---|---|
| `<FILE>` | Source file. |
| `--base <GIT-REF>` | Compare current file against the version at a git ref. |
| `--since <GIT-REF>` | Alias for `--base`. |
| `--left <ARGS>` | Arguments for the left side (`"key=val,key=val"`). |
| `--right <ARGS>` | Arguments for the right side. |
| `-f, --format <FMT>` | Output format: `text` (default) or `json`. |
| `--detect-moves` | Detect keys that moved (same value at different paths). |
| `--blame` | Annotate diff entries with git blame info. |

Must specify at least one of `--base`/`--since` or `--left`/`--right`. Exit code 1 when differences are found, 0 when identical.

**Examples:**

```bash
# Compare current vs main branch
hone diff config.hone --base main

# Compare two environments
hone diff config.hone --left "env=dev" --right "env=production"

# With move detection and blame
hone diff config.hone --base main --detect-moves --blame

# JSON output for programmatic consumption
hone diff config.hone --left "env=dev" --right "env=prod" --format json
```

---

### `hone import` -- Convert YAML/JSON to Hone

Converts existing YAML or JSON files into Hone source.

```bash
hone import <FILE> [OPTIONS]
```

| Option | Description |
|---|---|
| `<FILE>` | YAML or JSON file to convert. |
| `-o, --output <PATH>` | Output file. |
| `--extract-vars` | Detect repeated values and extract them as `let` variables. |
| `--split-docs` | Split multi-document YAML into separate files. |

**Examples:**

```bash
hone import values.yaml -o values.hone
hone import config.json --extract-vars
```

---

### `hone graph` -- Visualize import dependencies

Analyzes a file's import graph and outputs it in text, DOT, or JSON format.

```bash
hone graph <FILE> [OPTIONS]
```

| Option | Description |
|---|---|
| `<FILE>` | Source file to analyze. |
| `-f, --format <FMT>` | Output format: `text` (default), `dot`/`graphviz`, `json`. |
| `-o, --output <PATH>` | Output file. |

**Examples:**

```bash
# Text tree
hone graph main.hone

# Graphviz diagram
hone graph main.hone --format dot | dot -Tpng > deps.png

# JSON for tooling
hone graph main.hone --format json
```

---

### `hone cache` -- Manage build cache

```bash
hone cache clean [OPTIONS]
```

| Option | Description |
|---|---|
| `--older-than <DURATION>` | Only remove entries older than this. Units: `d`, `h`, `m`, `s`. |

**Examples:**

```bash
hone cache clean                  # remove all cached entries
hone cache clean --older-than 7d  # remove entries older than 7 days
```

---

### `hone typegen` -- Generate schemas from JSON Schema

Reads a JSON Schema file and produces Hone `schema` definitions.

```bash
hone typegen <FILE> [OPTIONS]
```

| Option | Description |
|---|---|
| `<FILE>` | JSON Schema file. |
| `-o, --output <PATH>` | Output file. |

**Examples:**

```bash
hone typegen schema.json
hone typegen kubernetes-deployment.json -o k8s-types.hone
```

---

### `hone lsp` -- Start Language Server

Starts the Hone language server for editor integration.

```bash
hone lsp [OPTIONS]
```

| Option | Description |
|---|---|
| `--stdio` | Use stdio transport (default). |

---

### Debug commands (hidden)

These are hidden from `--help` but available for development:

```bash
hone lex file.hone        # Print tokens
hone parse file.hone      # Print AST
hone resolve file.hone    # Print import graph
hone eval 'let x = 1 + 2' # Evaluate inline expression
```

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Success |
| 1 | Compilation error, diff found differences, or format check failed |
| 3 | I/O error (file not found, permission denied) |
