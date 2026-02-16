# Build Cache

Hone uses a content-addressed SHA256 cache for hermetic, reproducible builds. When a file is compiled with the same source, variants, args, format, and compiler version, the cached result is returned instantly.

## How it works

On every `hone compile`:

1. A cache key is computed as SHA256 of: source content + sorted variant selections + sorted args + output format + compiler version
2. If a matching entry exists in the cache, the cached output is returned without recompilation
3. If no match, the file is compiled normally and the result is stored

## Cache location

```
~/.cache/hone/v1/<first-2-hex>/<full-hash>.json
```

Respects `XDG_CACHE_HOME` if set. On macOS/Linux, the default is `~/.cache/hone/v1/`. Each entry is a JSON file containing the compiled output string, format, source path, timestamp, and compiler version.

## When caching is active

The cache is **enabled by default** on `hone compile` and is automatically **disabled** when:

- `--no-cache` is passed
- Input is stdin (`-` or `/dev/stdin`)
- `--allow-env` is used (builds depending on environment variables are non-deterministic)
- `--output-dir` is used (multi-file output)

## Invalidation

The cache invalidates automatically when any input changes:

- Source file content changes (even whitespace)
- Different `--variant` selection
- Different `--set` values
- Different `--format`
- Different Hone compiler version

No manual invalidation is needed for normal use.

## CLI commands

### Disable cache for a single build

```bash
hone compile config.hone --no-cache
```

### Clear the cache

```bash
# Remove all cached entries
hone cache clean

# Remove entries older than 7 days
hone cache clean --older-than 7d

# Other duration units
hone cache clean --older-than 24h
hone cache clean --older-than 30m
hone cache clean --older-than 60s
```

## Concurrency

Cache writes use atomic operations (write to temporary file, then rename) to prevent corruption from concurrent access. Multiple `hone compile` processes can safely share the same cache directory.
