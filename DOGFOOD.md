# Hone Dogfooding Notes

Status as of Pre-Dogfood Cleanup Sprint.

## Ready for Use

### Core Features Working

1. **Type Aliases with Unified Syntax**
   ```hone
   type Port = int(1, 65535)
   type Name = string(1, 100)
   type Percentage = float(0.0, 1.0)
   ```

2. **Schema Validation**
   ```hone
   schema ServerConfig {
       name: string(1, 100)
       port: int(1, 65535)
       debug?: bool
   }

   use ServerConfig

   name: "api-server"
   port: 8080
   ```

3. **YAML Output** - Properly quoted for Kubernetes safety
   - Bool-like strings (`"yes"`, `"no"`, `"on"`, `"off"`) → quoted
   - Number-like strings → quoted
   - Special characters in keys/values → quoted

4. **Import System**
   ```hone
   from "./base.hone"
   import { ports, config } from "./shared.hone"
   ```

5. **Conditional Blocks**
   ```hone
   when env == "production" {
       replicas: 3
       resources {
           cpu: "2"
           memory: "4Gi"
       }
   }
   ```

6. **For Loops**
   ```hone
   services: [
       for name in ["api", "web", "worker"] {
           name: name
           port: 8080
       }
   ]
   ```

7. **Error Messages** - "Did you mean?" suggestions for typos

## Test Commands

```bash
# Build
cargo build --release

# Compile to YAML
hone compile --format yaml config.hone

# Compile to JSON
hone compile config.hone

# Check syntax only
hone check config.hone

# Format source files
hone fmt --write config.hone   # Format in place
hone fmt --check .             # CI: check all files are formatted
```

## Example: Kubernetes Deployment

```hone
# deployment.hone
type Replicas = int(1, 100)
type Port = int(1, 65535)
type Memory = string(1, 20)

schema Deployment {
    name: string
    replicas: Replicas
    image: string
    port: Port
    memory?: Memory
}

use Deployment

name: "api-server"
replicas: 3
image: "myapp:v1.2.3"
port: 8080
memory: "512Mi"
```

Output:
```yaml
name: api-server
replicas: 3
image: myapp:v1.2.3
port: 8080
memory: 512Mi
```

## Known Limitations

1. **No watch mode** - Use external tools (`entr`, `watchexec`) for file watching

## Recommended Workflow

1. Create a base schema file with your types
2. Import schemas into config files
3. Run `hone fmt --check .` in CI to enforce formatting
4. Use `hone check` before committing
5. Compile to YAML for Kubernetes
6. Use conditional blocks for environment-specific config

## Files

- `hone compile --format yaml` for Kubernetes manifests
- `hone compile` (JSON) for other tools
- `.hone` extension recommended

## Feedback Requested

When dogfooding, note:
- Pain points in syntax
- Missing built-in functions
- Error messages that aren't helpful
- Features that would save time
