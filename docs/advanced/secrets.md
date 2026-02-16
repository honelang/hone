# Secrets

Hone provides first-class secret placeholders so sensitive values never leak into compiled output.

## Declaring secrets

Use the `secret` keyword in the preamble:

```hone
secret db_password from "vault:secret/data/db#password"
secret api_key from "env:API_KEY"
secret token from "ssm:/prod/app/token"
```

After declaration, `db_password` is a regular variable you can use anywhere. Its compile-time value is the placeholder string `<SECRET:vault:secret/data/db#password>`.

## Using secrets

Secrets work like any other variable:

```hone
secret db_password from "vault:secret/data/db#password"
secret api_key from "env:API_KEY"

database {
  host: "postgres.internal"
  password: db_password
}

service {
  token: api_key
  connection_string: "postgres://user:${db_password}@db:5432/myapp"
}
```

In the default output, `password` will be `<SECRET:vault:secret/data/db#password>`.

## Secret modes

The `--secrets-mode` flag on `hone compile` controls how secrets appear in output.

### `placeholder` (default)

Leaves `<SECRET:...>` strings in the output. A downstream tool (CI pipeline, secret manager integration) resolves them:

```bash
hone compile config.hone --format yaml
```

```yaml
database:
  host: postgres.internal
  password: <SECRET:vault:secret/data/db#password>
```

### `error`

Compilation fails if any secret placeholder remains in the output. Use this to enforce that no unresolved secrets ship:

```bash
hone compile config.hone --secrets-mode error
```

```
error[E0802]: secret placeholders found in output (--secrets-mode=error)
```

### `env`

Resolves `env:`-prefixed secrets from environment variables at compile time. Requires `--allow-env`:

```bash
export API_KEY="abc123"
hone compile config.hone --secrets-mode env --allow-env
```

Only secrets declared as `secret name from "env:VAR_NAME"` are resolved. Non-`env:` providers (e.g., `vault:...`) are left as placeholders.

## Provider string

The provider string after `from` is opaque to Hone. It is metadata for external tooling. Common conventions:

| Provider | Meaning |
|---|---|
| `env:VAR_NAME` | Environment variable |
| `vault:path#key` | HashiCorp Vault |
| `ssm:/path/to/param` | AWS SSM Parameter Store |
| `gsm:projects/P/secrets/S` | Google Secret Manager |

Hone does not resolve any of these (except `env:` in `--secrets-mode env`). The placeholder format is designed for downstream tools to parse and replace.

## Secrets in string interpolation

Secrets participate in string interpolation:

```hone
secret password from "vault:db#pass"

url: "postgres://admin:${password}@db:5432/myapp"
```

Output: `postgres://admin:<SECRET:vault:db#pass>@db:5432/myapp`

## Secrets with schemas

Secrets are string values, so they validate against `string` fields:

```hone
schema Config {
  password: string
}

use Config

secret password from "vault:db#pass"
password: password
```

This compiles successfully because `<SECRET:vault:db#pass>` is a string.
