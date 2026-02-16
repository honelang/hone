# Hone Language Support for VS Code / Cursor

This extension provides syntax highlighting and language server support for Hone configuration files (`.hone`).

## Features

- **Syntax Highlighting** - Full TextMate grammar for Hone syntax
- **Diagnostics** - Real-time error reporting as you type
- **Hover Documentation** - Hover over keywords and functions for documentation
- **Completions** - Autocomplete for keywords, built-in functions, and variables
- **Go to Definition** - Jump to variable definitions
- **Find All References** - Find all usages of a variable
- **Rename Symbol** - Rename a variable across all usages
- **Format on Save** - Automatic source formatting

## Installation

### Prerequisites

First, install the Hone compiler:

```bash
# Build from source
cd /path/to/hone
cargo build --release

# Add to PATH
export PATH="$PATH:/path/to/hone/target/release"

# Verify
hone --version
```

### Development Install

1. Install dependencies and compile:

```bash
cd editors/vscode
npm install
npm run compile
```

2. Create a symlink to your VS Code extensions folder:

```bash
# macOS/Linux - VS Code
ln -s /path/to/hone/editors/vscode ~/.vscode/extensions/hone-lang

# macOS/Linux - Cursor
ln -s /path/to/hone/editors/vscode ~/.cursor/extensions/hone-lang
```

3. Restart VS Code / Cursor

4. Open any `.hone` file - you should see syntax highlighting and diagnostics

### Configuration

The extension can be configured in your VS Code settings:

```json
{
  "hone.serverPath": "/path/to/hone",
  "hone.trace.server": "off"
}
```

| Setting | Default | Description |
|---------|---------|-------------|
| `hone.serverPath` | `"hone"` | Path to the Hone binary |
| `hone.trace.server` | `"off"` | Trace LSP communication (`"off"`, `"messages"`, `"verbose"`) |

## What's Included

### Syntax Highlighting
- Keywords: `let`, `when`, `for`, `in`, `import`, `from`, `assert`, `schema`, `type`, `secret`, `policy`, `deny`, `warn`
- Types: `string`, `int`, `float`, `bool`, `null`, `any`
- String interpolation: `${variable}`
- Comments: `# comment`
- Numbers, booleans, operators
- Built-in functions: `len`, `keys`, `contains`, `range`, etc.

### Language Server Features
- **Diagnostics** - Syntax errors are underlined in red with helpful messages
- **Hover** - Hover over keywords and functions to see documentation
- **Completions** - Type to get suggestions for keywords, functions, and variables
- **Go to Definition** - Ctrl/Cmd+Click on a variable to jump to its definition
- **Find References** - Find all usages of a variable (Shift+F12)
- **Rename** - Rename a variable across the file (F2)
- **Format on Save** - Automatic source formatting via `hone fmt`

### Language Configuration
- Auto-closing brackets and quotes
- Comment toggling (Cmd+/)
- Code folding for blocks

## File Association

To associate `.hone` files with this language in your workspace, add to `.vscode/settings.json`:

```json
{
  "files.associations": {
    "*.hone": "hone"
  }
}
```

## Troubleshooting

### Language server fails to start

1. Check that `hone` is in your PATH:
   ```bash
   which hone
   hone --version
   ```

2. If using a custom path, configure `hone.serverPath`:
   ```json
   {
     "hone.serverPath": "/absolute/path/to/hone"
   }
   ```

3. Enable tracing to see LSP messages:
   ```json
   {
     "hone.trace.server": "verbose"
   }
   ```
   Then check the Output panel (View > Output > Hone Language Server)

### No syntax highlighting

Make sure the extension is installed:
1. Open the Extensions view (Cmd+Shift+X)
2. Search for "hone"
3. Check that it's enabled

## Publishing

To publish to the VS Code Marketplace:

```bash
npm install -g vsce
cd editors/vscode
npm install
npm run compile
vsce package   # Creates hone-lang-0.1.0.vsix
vsce publish   # Publishes to marketplace (needs PAT)
```
