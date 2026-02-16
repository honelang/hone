# Editor Setup

Hone includes a Language Server Protocol (LSP) implementation and a VS Code / Cursor extension.

## VS Code / Cursor

### Install from source

```bash
cd editors/vscode
npm install
npm run compile
```

Then symlink the extension into your editor's extensions directory:

```bash
# VS Code
ln -s "$(pwd)" ~/.vscode/extensions/hone-lang-0.1.0

# Cursor
ln -s "$(pwd)" ~/.cursor/extensions/hone-lang-0.1.0
```

Restart the editor after installation.

### Features

The extension provides:

- **Syntax highlighting** -- Keywords, strings, numbers, comments, operators
- **Real-time diagnostics** -- Syntax errors, undefined variables, type mismatches, schema violations, and policy warnings shown as you type
- **Hover information** -- Variable types, evaluated values, builtin function signatures with examples, schema field constraints
- **Autocompletion** -- Variables in scope, keywords, built-in function names
- **Go to Definition** -- Ctrl+Click or F12 to jump to variable declarations
- **Find All References** -- Shift+F12 to find all usages of a variable
- **Rename Symbol** -- F2 to rename a variable across all usages
- **Format on Save** -- Automatically formats `.hone` files when saving

### Configuration

The extension uses `hone lsp --stdio` as the language server. Ensure the `hone` binary is on your `PATH`, or configure the path in the extension settings.

### Troubleshooting

**"hone" command not found**: The `hone` binary must be on your `PATH`. Either install it globally or add the build directory:

```bash
export PATH="$PATH:/path/to/hone/target/release"
```

**Diagnostics not updating**: The LSP runs background compilation on every change with a debounce. If diagnostics seem stale, save the file to trigger a fresh compilation.

**Extension not loading**: Check the Output panel (View > Output) and select "Hone Language Server" from the dropdown for server logs.

## Other editors

Any editor with LSP support can use the Hone language server:

```bash
hone lsp --stdio
```

The server communicates over stdin/stdout using the LSP protocol. Configure your editor to launch this command for `.hone` files.

### Supported LSP capabilities

| Capability | Method |
|---|---|
| Diagnostics | `textDocument/publishDiagnostics` |
| Hover | `textDocument/hover` |
| Completion | `textDocument/completion` |
| Go to Definition | `textDocument/definition` |
| Find References | `textDocument/references` |
| Rename | `textDocument/rename` |
| Formatting | `textDocument/formatting` |

### Neovim (nvim-lspconfig)

Add to your LSP configuration:

```lua
local lspconfig = require('lspconfig')
local configs = require('lspconfig.configs')

configs.hone = {
  default_config = {
    cmd = { 'hone', 'lsp', '--stdio' },
    filetypes = { 'hone' },
    root_dir = lspconfig.util.find_git_ancestor,
  },
}

lspconfig.hone.setup({})
```

You will also need a filetype detection autocmd:

```lua
vim.filetype.add({
  extension = {
    hone = 'hone',
  },
})
```

### Helix

Add to `~/.config/helix/languages.toml`:

```toml
[[language]]
name = "hone"
scope = "source.hone"
file-types = ["hone"]
language-servers = ["hone-lsp"]

[language-server.hone-lsp]
command = "hone"
args = ["lsp", "--stdio"]
```

### Sublime Text (LSP package)

Add to LSP settings (`Preferences > Package Settings > LSP > Settings`):

```json
{
  "clients": {
    "hone": {
      "enabled": true,
      "command": ["hone", "lsp", "--stdio"],
      "selector": "source.hone"
    }
  }
}
```
