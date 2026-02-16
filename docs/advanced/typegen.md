# Type Generation

`hone typegen` generates Hone schema definitions from JSON Schema files. This lets you validate Hone output against existing schemas without rewriting type definitions by hand.

## Usage

```bash
hone typegen schema.json                  # print to stdout
hone typegen schema.json -o types.hone    # write to file
```

## Example

Given a JSON Schema file `server.json`:

```json
{
  "title": "Server",
  "type": "object",
  "properties": {
    "host": { "type": "string", "minLength": 1, "maxLength": 255 },
    "port": { "type": "integer", "minimum": 1, "maximum": 65535 },
    "debug": { "type": "boolean" }
  },
  "required": ["host", "port"],
  "additionalProperties": false
}
```

Running `hone typegen server.json` produces:

```hone
schema Server {
  debug?: bool
  host: string(1, 255)
  port: int(1, 65535)
}
```

Use the generated schemas in your Hone files:

```hone
import { Server } from "./types.hone"
# or copy the schema directly into your file

use Server

host: "api.example.com"
port: 8080
```

## Type mapping

| JSON Schema | Hone type |
|---|---|
| `"type": "string"` | `string` |
| `"type": "string"` + `minLength` + `maxLength` | `string(min, max)` |
| `"type": "string"` + `pattern` | `string("regex")` |
| `"type": "integer"` | `int` |
| `"type": "integer"` + `minimum` + `maximum` | `int(min, max)` |
| `"type": "number"` | `float` |
| `"type": "number"` + `minimum` + `maximum` | `float(min, max)` |
| `"type": "boolean"` | `bool` |
| `"type": "null"` | `null` |
| `"type": "object"` with properties | Named sub-schema |
| `"type": "object"` without properties | `object` |
| `"type": "array"` | `array` |
| `$ref` (local) | Schema reference by name |

## Handling constraints

Both bounds must be present for constraints to be emitted. A single `minimum` without `maximum` produces plain `int`, not `int(min, ...)`. This matches Hone's constraint syntax which requires both bounds.

## Nested objects

Nested objects with `properties` are extracted into separate named schemas:

```json
{
  "title": "Config",
  "type": "object",
  "properties": {
    "server": {
      "type": "object",
      "properties": {
        "host": { "type": "string" },
        "port": { "type": "integer" }
      },
      "required": ["host", "port"]
    }
  },
  "required": ["server"]
}
```

Produces:

```hone
schema ConfigServer {
  host: string
  port: int
}

schema Config {
  server: ConfigServer
}
```

## $ref resolution

Local `$ref` references (within the same file) are resolved:

```json
{
  "$defs": {
    "Port": {
      "type": "integer",
      "minimum": 1,
      "maximum": 65535
    }
  },
  "title": "Server",
  "type": "object",
  "properties": {
    "port": { "$ref": "#/$defs/Port" }
  }
}
```

Produces:

```hone
schema Port {
  # ... or referenced inline depending on structure
}

schema Server {
  port?: Port
}
```

Remote `$ref` (URLs or external files) are not supported.

## Open vs closed schemas

- `"additionalProperties": false` produces a closed schema (no `...`)
- `"additionalProperties": true` or absent produces an open schema (with `...`)

## Reserved words

Field names that are Hone reserved words are automatically quoted:

```json
{
  "properties": {
    "type": { "type": "string" }
  }
}
```

Produces:

```hone
schema Root {
  "type"?: string
  ...
}
```

## Round-trip verification

Generated schemas can be used immediately with `use` to validate output. The `typegen` command produces valid Hone source that compiles without modification.
