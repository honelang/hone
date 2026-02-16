# Error Catalog

Every Hone error has a code in the format `E0xxx`. This page lists all error codes, what they mean, and how to fix them.

## Syntax errors (E00xx)

### E0001 -- Unexpected token

The parser encountered a token it did not expect at that position.

```
error[E0001]: unexpected token `}`
  --> config.hone:5:1
```

**Fix:** Check for mismatched braces, missing colons, or stray characters.

### E0002 -- Undefined variable

A variable name was used but never declared.

```
error[E0002]: undefined variable `prot`
  --> config.hone:12:5
   |
12 |     port: prot
   |           ^^^^ did you mean `port`?
```

**Fix:** Check for typos. The error includes a "did you mean?" suggestion when a similar variable exists.

### E0003 -- Reserved word as bare key

A Hone keyword was used as a bare key name. Keywords include: `let`, `import`, `from`, `schema`, `type`, `use`, `for`, `in`, `when`, `else`, `assert`, `variant`, `default`, `expect`, `secret`, `policy`, `deny`, `warn`, `true`, `false`, `null`.

```
error[E0003]: `type` is a reserved word and cannot be used as a bare key
  help: quote it: "type"
```

**Fix:** Wrap the key in double quotes: `"type": "Deployment"`.

### E0004 -- Unterminated string

A string literal was opened but never closed.

```
error[E0004]: unterminated string
  --> config.hone:3:10
```

**Fix:** Add the closing `"` or `'`. For multiline strings, use triple quotes `"""..."""`.

### E0005 -- Invalid escape sequence

An unrecognized escape sequence was found in a string.

```
error[E0005]: invalid escape sequence `\x`
```

**Fix:** Valid escapes are `\\`, `\"`, `\n`, `\t`, `\r`. Use single-quoted strings for literal content.

## Import errors (E01xx)

### E0101 -- Import not found

The specified file could not be found.

```
error[E0101]: import not found: ./missing.hone
  --> config.hone:1:1
```

**Fix:** Check the file path. Paths are relative to the importing file.

### E0102 -- Circular import

Two or more files import each other, creating a cycle.

```
error[E0102]: circular import detected: a.hone -> b.hone -> a.hone
```

**Fix:** Restructure imports to break the cycle. Extract shared definitions into a third file.

## Type errors (E02xx)

### E0201 -- Value out of range

A value violates a type constraint.

```
error[E0201]: expected int(1, 65535), found int (value: 99999)
  help: value 99999 is greater than maximum 65535
```

**Fix:** Adjust the value to be within the constraint range, or update the constraint.

### E0202 -- Type mismatch

A value has the wrong type for its schema field.

```
error[E0202]: type mismatch: expected string, found int
  --> config.hone:8:7
```

**Fix:** Change the value to match the expected type, or use a conversion function like `to_str()`.

### E0203 -- Pattern mismatch

A string does not match its regex constraint.

```
error[E0203]: string "invalid" does not match pattern "^[a-z]+-[0-9]+$"
```

**Fix:** Adjust the value to match the pattern.

### E0204 -- Missing required field

A required schema field is not present in the output.

```
error[E0204]: missing required field `host` in schema Server
```

**Fix:** Add the missing field to the output.

### E0205 -- Unknown field in closed schema

A field exists in the output that is not defined in the schema.

```
error[E0205]: unknown field `extra` in schema Server
  help: schema Server does not allow additional fields; add `...` to the schema to allow them
```

**Fix:** Either remove the extra field, add it to the schema definition, or add `...` to the schema to allow additional fields.

## Merge errors (E03xx)

### E0302 -- Multiple from declarations

A file contains more than one `from` statement.

```
error[E0302]: multiple `from` declarations; only one is allowed
```

**Fix:** Use only one `from` per file. For multiple bases, use `import` instead.

### E0304 -- From in multi-document preamble

A `from` statement was used in a file with `---name` document separators.

```
error[E0304]: `from` cannot be used in multi-document files
```

**Fix:** Use `import` instead of `from` in multi-document files.

## Evaluation errors (E04xx)

### E0402 -- Division by zero / arithmetic overflow

A division by zero or integer overflow occurred.

```
error[E0402]: division by zero
  --> config.hone:3:15
```

**Fix:** Add a guard condition or ensure the divisor is never zero.

### E0403 -- Recursion limit exceeded

Nesting depth exceeded the maximum (array index out of bounds).

**Fix:** Simplify the nesting structure.

## Dependency errors (E05xx)

### E0501 -- Circular dependency

A circular dependency was detected between values.

**Fix:** Restructure the values to break the cycle.

## Control flow errors (E07xx)

### E0701 -- For at top level

A `for` loop was used at the document top level where it is not allowed.

**Fix:** Use `for` loops inside blocks or as array/object expressions assigned to keys.

### E0702 -- Assertion failed

An `assert` statement's condition evaluated to false.

```
error[E0702]: assertion failed: port must be positive
  --> config.hone:5:1
   |
   | assert port > 0 : "port must be positive"
   |        ^^^^^^^^
   | where: port = -1
```

**Fix:** Adjust the values so the assertion condition is satisfied.

## Hermeticity errors (E08xx)

### E0801 -- env/file not allowed

The `env()` or `file()` function was called without the `--allow-env` flag.

```
error[E0801]: env() requires --allow-env flag
  help: add --allow-env to allow reading environment variables
```

**Fix:** Add `--allow-env` to the compile command. This flag is required to keep builds hermetic by default.

### E0802 -- Secret in output

Secret placeholders remain in the output when `--secrets-mode error` is active.

```
error[E0802]: secret placeholders found in output (--secrets-mode=error)
```

**Fix:** Either resolve the secrets (e.g., with `--secrets-mode env --allow-env`) or use `--secrets-mode placeholder` to allow placeholders.
