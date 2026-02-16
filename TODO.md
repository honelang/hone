# Hone TODO

Tracked tasks for the Hone configuration language.

**Current state:** 535 tests, zero clippy warnings, all examples compile. Launch sprint complete.

---

## Backlog

### Package Manager
Import from URLs or a registry instead of file paths only.

### Sourcemaps
Map compiled output back to Hone source for debugging.

### Language Libraries
Native `.hone` parsing libraries for popular languages (Python, TypeScript, Go).

### Incremental Compilation
For large projects with many imports, only recompile changed files.

### Plugins
Custom functions via WASM modules.

---

## Completed

### Launch Sprint (Feb 2026)
All 9 tasks completed. No new Rust code -- writing, packaging, and connecting existing pieces.

1. **Binary Distribution** - GitHub Actions release workflow (multi-platform), `scripts/install.sh` curl installer
2. **CI Workflow** - `.github/workflows/ci.yml` with test, clippy, fmt, example verification, WASM build
3. **User Documentation** - 9 docs: getting-started, cli-reference, language-reference, editor-setup, errors, advanced/{secrets, policies, cache, typegen}
4. **CONTRIBUTING.md** - Build, test, lint, project structure, how-to guides
5. **Playground Deployment Docs** - `playground/README.md` with WASM build instructions, deployment guides
6. **README Rewrite** - Updated for cold audience, added TOML/.env, secrets, policies, full CLI, docs index
7. **Examples** - New `app-config.hone`, `scripts/verify-examples.sh` (15 checks)
8. **VS Code Extension Prep** - Updated publisher to `honelang`, added categories/keywords/license
9. **Audit Script** - `scripts/audit.sh` (38 checks), `LICENSE` file

### Vision Sprint (Feb 2026)
8 phases implemented, growing the codebase from 448 to 535 tests.

1. **Multi-Target Emission** - TOML (`--format toml`) and .env (`--format dotenv`) output formats
2. **Content-Addressed Caching** - SHA256-based build cache at `~/.cache/hone/v1/`, `--no-cache`, `hone cache clean`
3. **Dependency Graph** - `hone graph` with text, DOT, and JSON output formats
4. **Secret Declarations** - `secret name from "provider:path"`, `--secrets-mode` (placeholder/error/env)
5. **Policy Engine** - `policy name deny/warn when condition { "message" }`, `--ignore-policy`
6. **Enhanced Diff** - `--since`, `--detect-moves`, `--blame` flags for structural diff
7. **Teaching LSP** - Rich hover (evaluated values, builtin docs, schema tables), background compilation diagnostics
8. **Type Provider** - `hone typegen schema.json` generates Hone schemas from JSON Schema

### Hardening Sprint (Jan 2026)
7 tasks establishing the quality baseline.

1. **Enforce Type Constraints** - `int(min,max)`, `float(min,max)`, `string(min,max)`, `string("regex")` validated at compile time
2. **Type Aliases** - `type Port = int(1, 65535)` unified constraint syntax
3. **Error Message Quality** - "Did you mean?" suggestions via Levenshtein distance
4. **Escape Hatches** - Closed: YAML emitter already quotes bool-like and number-like strings
5. **hone fmt** - Source formatter with comment preservation, `--write`/`--check`/`--diff`
6. **hone diff** - Structural value diff with `--base` and `--left`/`--right` modes
7. **Documentation Sync** - README examples verified, CLAUDE.md updated

## Completion Contract

For any task to be marked complete:
- [ ] Tests written for every change
- [ ] All tests pass (`cargo test`)
- [ ] DESIGN.md updated if architecture changed
- [ ] TODO.md updated
- [ ] CLAUDE.md updated if syntax/behavior changed
- [ ] No dead code (features must be implemented or removed)
- [ ] Error messages match the quality spec
