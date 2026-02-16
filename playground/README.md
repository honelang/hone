# Hone Playground

A browser-based playground for the Hone configuration language, powered by a WebAssembly build of the compiler.

## Local usage

Open `index.html` directly in a browser:

```bash
open playground/index.html
# or
python3 -m http.server 8000 -d playground
# then visit http://localhost:8000
```

Edit Hone source on the left, see compiled output on the right in real time.

## Features

- Real-time compilation as you type
- Output format selection: JSON, YAML, TOML, .env
- Variant selection via UI
- Args injection via UI
- Source formatting (via `format_source()` WASM binding)
- Error display with line/column information
- Example templates to get started

## Architecture

The playground consists of:

- `index.html` -- Single-page app with editor and output panels
- `pkg/` -- Pre-built WASM package (compiled from `hone-wasm/`)

The WASM package exposes two functions:

- `compile(source, format, variant_json, args_json)` -- Returns `CompileResult` with `output`, `error`, and `success` fields
- `format_source(source)` -- Returns `CompileResult` with formatted source

## Rebuilding the WASM package

Prerequisites:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
```

Build:

```bash
wasm-pack build hone-wasm --target web --out-dir ../playground/pkg
```

This replaces the `playground/pkg/` directory with fresh WASM bindings.

## Deployment

The playground is a static site with no server dependencies. Deploy it anywhere that serves static files.

### GitHub Pages

Add a GitHub Actions workflow to build and deploy:

```yaml
name: Deploy Playground
on:
  push:
    branches: [main]

jobs:
  deploy:
    runs-on: ubuntu-latest
    permissions:
      pages: write
      id-token: write
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: wasm32-unknown-unknown
      - name: Install wasm-pack
        run: curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
      - name: Build WASM
        run: wasm-pack build hone-wasm --target web --out-dir ../playground/pkg
      - name: Upload Pages artifact
        uses: actions/upload-pages-artifact@v3
        with:
          path: playground
      - name: Deploy to GitHub Pages
        id: deployment
        uses: actions/deploy-pages@v4
```

### Netlify / Vercel / Cloudflare Pages

Point the build to the `playground/` directory. No build step needed if the pre-built `pkg/` is committed; otherwise, add the `wasm-pack build` step above.

### Self-hosted

Copy the `playground/` directory to any static file server:

```bash
cp -r playground/ /var/www/hone-playground/
```

## WASM package details

The `hone-wasm` crate (`hone-wasm/`) is a thin wrapper around the Hone library. It:

- Uses `hone-lang` with `default-features = false` (no CLI, no LSP, no tokio)
- Exposes `compile()` and `format_source()` via `wasm-bindgen`
- Runs the full pipeline: lex, parse, evaluate, type-check, emit
- Supports all output formats (JSON, YAML, TOML, .env)
- Handles variant selections and args via JSON string parameters
