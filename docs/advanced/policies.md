# Policies

Policies are output validation rules that run after compilation. They check conditions against the final compiled value and either fail the build (`deny`) or emit a warning (`warn`).

## Syntax

```hone
policy <name> <level> when <condition> {
  "message"
}
```

- `name` -- identifier for the policy (used in error messages)
- `level` -- `deny` (fails build) or `warn` (emits warning)
- `condition` -- expression evaluated against the output
- `message` -- optional string shown when the policy triggers

## The `output` variable

Inside a policy condition, `output` refers to the root object of the compiled result. Access fields with dot notation:

```hone
policy no_debug deny when output.debug == true {
  "debug must be disabled"
}

policy min_replicas warn when output.spec.replicas < 2 {
  "consider at least 2 replicas for availability"
}
```

## Examples

### Deny policy

Prevents compilation from succeeding when the condition is true:

```hone
variant env {
  default dev {
    let debug = true
  }
  production {
    let debug = false
  }
}

policy no_debug_in_prod deny when output.debug == true && output.env == "production" {
  "debug must be disabled in production"
}

env: "production"
debug: debug
```

Compiling with `--variant env=production` succeeds (debug is false). Removing the variant or using dev fails:

```
error: policy 'no_debug_in_prod' violated: debug must be disabled in production
```

### Warn policy

Emits a warning to stderr but allows compilation to succeed:

```hone
policy port_range warn when output.port < 1024 {
  "privileged ports require elevated permissions"
}

port: 80
```

Output compiles successfully, but stderr shows:

```
warning: policy 'port_range': privileged ports require elevated permissions
```

### Policy without message

The message block is optional. Without it, the error uses a default:

```hone
policy has_name deny when output.name == null
```

```
error: policy 'has_name' violated
```

## Evaluation order

Policies run after:
1. Parsing and import resolution
2. Evaluation (all variables resolved, all merges applied)
3. Schema validation (`use`)

This means policies see the fully compiled, type-checked output.

## Complex conditions

Policy conditions support any expression the language supports:

```hone
policy balanced_resources deny when
  output.resources.requests.cpu > output.resources.limits.cpu {
  "CPU request cannot exceed limit"
}

policy name_format deny when !contains(output.name, "-") {
  "name must contain a hyphen separator"
}
```

## Skipping policies

Use `--ignore-policy` to skip all policy checks:

```bash
hone compile config.hone --ignore-policy
```

This is useful during development or when you intentionally want to bypass policy enforcement.

## Importing policies

Policies can be defined in separate files and imported:

```hone
# policies.hone
policy no_debug deny when output.debug == true {
  "debug must be disabled"
}
```

```hone
# config.hone
import "./policies.hone" as _

debug: false
name: "my-app"
```

The policies from the imported file apply to the final output.
