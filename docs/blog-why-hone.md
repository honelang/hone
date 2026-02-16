# Why Hone?

You have 14,000 lines of YAML spread across three environments, four clusters, and a growing number of microservices. Somebody needs to change the storage class. They open the staging values file, make the edit, forget to update production, and the next deploy fails at 2 AM. This is not a hypothetical. This is Tuesday.

YAML was never meant for this. It was designed as a human-readable serialization format, and it does that job fine when your configuration fits on a single screen. But configurations grow. Teams grow. Environments multiply. And suddenly you are managing a sprawling estate of copy-pasted files where a single typo can take down a cluster. YAML has no variables, no conditionals, no way to express "this value depends on that one." It has anchors and aliases, technically, but anyone who has tried to debug a `<<: *merge-key` three levels deep knows that path leads nowhere good.

This is the problem Hone was built to solve.

## The YAML problem at scale

The core issues with raw YAML/JSON at scale are well-understood:

**Repetition.** The same image registry URL appears in 47 places. The same resource limits block is duplicated across every service. When something changes, you play find-and-replace roulette and hope you caught every instance.

**No abstraction.** You cannot define a variable, reference it elsewhere, or compute a value from other values. Every relationship between configuration values is implicit, living in the heads of the engineers who wrote it.

**No validation.** YAML will happily accept `replicas: "three"` and you will not find out until the API server rejects it. There is no way to say "this field must be an integer between 1 and 100" at the configuration layer.

**Environment drift.** Dev, staging, and production configs start as copies and immediately begin diverging. Three months later, nobody can confidently tell you the differences between them because the diff is 800 lines of noise.

These are not minor annoyances. At scale, they are the primary source of deployment failures.

## Existing solutions and their trade-offs

The industry has not ignored this problem. But every existing solution asks you to make significant compromises.

**Helm templates** graft Go's `text/template` syntax onto YAML. The result is a format where `{{ if .Values.ingress.enabled }}` floats in the middle of whitespace-sensitive markup. Helm charts are powerful and ubiquitous, but reading a complex Helm template requires simultaneously parsing YAML structure, Go template logic, and the implicit data flow through `values.yaml`. The error messages when something goes wrong reference the rendered output, not your source, making debugging an exercise in reverse engineering.

**Kustomize** takes a different approach: start with valid YAML and layer patches on top. This works well for simple overrides but breaks down when you need actual logic. You cannot express "if the environment is production, add these tolerations." You cannot loop over a list to generate resources. Kustomize deliberately avoids being a programming language, which is admirable in principle but limiting in practice.

**Jsonnet** is genuinely powerful. It has functions, imports, conditionals, and a well-defined evaluation model. But it chose an object-oriented paradigm with inheritance, mixins, and late binding. For infrastructure engineers who think in key-value pairs, the learning curve is steep. Writing Jsonnet feels like programming, which is exactly what you wanted to avoid when you chose YAML in the first place.

**CUE** is theoretically elegant. Its constraint-based type system can express sophisticated validation rules, and its unification semantics are mathematically sound. In practice, CUE's error messages are notoriously opaque, its documentation assumes familiarity with type theory, and simple tasks often require understanding concepts like lattice unification. Most teams that adopt CUE end up with a small number of experts and everyone else afraid to touch the configs.

**Dhall** brings total functional programming to configuration. It has a sound type system, guaranteed termination, and principled imports. It is also written in a Haskell-inspired syntax that immediately alienates its target audience. Platform engineers should not need to understand algebraic data types to change a port number.

Each of these tools solves the YAML problem. Each of them also introduces a new problem: they are too far from the mental model of the people who actually write configuration.

## The Hone approach: configuration, not programming

Hone occupies a deliberate position in this design space. It is more than YAML but less than a programming language. The guiding principle is that configuration authors should be able to read Hone files without a tutorial.

Here is what a Hone file looks like:

```hone
let env = "production"
let replicas = env == "production" ? 3 : 1
let registry = "registry.yeetops.io"

apiVersion: "apps/v1"
kind: "Deployment"
metadata {
  name: "api-${env}"
  labels {
    app: "api"
    environment: env
  }
}
spec {
  replicas: replicas
  template {
    spec {
      containers: [
        {
          name: "api"
          image: "${registry}/api:1.4.2"
          resources {
            limits { cpu: "2000m", memory: "4Gi" }
            requests { cpu: "500m", memory: "2Gi" }
          }
        }
      ]
    }
  }
}
```

If you know YAML, you can read this. The additions -- `let` bindings, `${}` interpolation, ternary expressions -- are minimal and unsurprising. Hone does not ask you to learn a new paradigm. It gives you the tools that YAML should have had from the start.

**Variables and interpolation** eliminate repetition. Define a value once, reference it everywhere. Change it in one place, and the change propagates.

**Conditionals and loops** handle environment-specific logic without file duplication. A `when` block adds keys conditionally. A `for` expression generates arrays or objects from data.

**Schema validation** catches errors at compile time, not deploy time. Define a schema with constraints -- `port: int(1, 65535)`, `name: string(1, 100)` -- and Hone validates your output before it ever becomes YAML:

```hone
schema Service {
  host: string
  port: int(1, 65535)
  debug?: bool
}

use Service

host: "localhost"
port: 8080
```

If `port` is 99999, compilation fails with a clear message: `value 99999 is greater than maximum 65535`. No more discovering constraint violations from a Kubernetes API server rejection.

**Variant blocks** replace the copy-paste-and-diverge pattern for multi-environment configs:

```hone
variant env {
  default dev {
    replicas: 1
    debug: true
  }
  staging {
    replicas: 2
    debug: false
  }
  production {
    replicas: 5
    debug: false
  }
}

name: "my-app"
```

Compile with `--variant env=production` and get the production values merged into your output. One file, all environments, diffs that actually mean something.

**Multi-file composition** scales to real projects. Hone supports imports, selective imports, and an overlay pattern (`from`) with deep merging:

```hone
import "./variables.hone" as vars
import "./patterns.hone" as patterns

kafka {
  replicaCount: vars.is_production ? "3" : "1"
  resources: vars.is_production ? patterns.resources_large : patterns.resources_medium
  storageClass: patterns.storage_classes.standardRwo
}
```

Variables, patterns, and domain-specific modules each live in their own file. The entry point composes them. The import graph is explicit and acyclic -- Hone detects circular imports at compile time.

**Hermetic builds** ensure reproducibility. The `env()` and `file()` functions, which read environment variables and files at compile time, are gated behind an `--allow-env` flag. Without it, builds are fully deterministic: same input, same output, every time.

**IDE support is not an afterthought.** Hone ships with a Language Server Protocol implementation that provides real-time diagnostics, go-to-definition, find-references, rename-symbol, hover information, completions, and format-on-save. There is a VS Code and Cursor extension available today. When you hover over a variable, you see its type and value. When you make a typo, you see the error before you save.

## A real-world example

The Hone repository includes a multi-file example that generates a complete BYOC (Bring Your Own Cluster) configuration for Kubernetes. The project is structured across five files: variables, reusable patterns, Kafka config, schema definitions, and a main entry point that composes everything. Compile-time assertions enforce invariants (`assert len(vars.customer_name) > 0`), schemas validate the output structure, and environment-specific logic is expressed inline rather than duplicated across files.

A single `hone compile main.hone --format yaml` produces the complete, validated configuration. Change the environment from `"test"` to `"production"` and replica counts, storage sizes, tolerations, and resource limits all adjust automatically. The YAML that comes out is correct by construction.

## What is next

Hone is MIT licensed, written in Rust, and already usable for real work. Active development is focused on several fronts:

**WASM playground.** Try Hone in the browser without installing anything. Write a config, see the JSON/YAML output in real time.

**Package registry.** Today, imports are file-path based. A package registry will enable sharing reusable configuration modules across teams and organizations -- common Kubernetes patterns, cloud provider defaults, security baselines.

**CI integrations.** `hone check` already validates configs without producing output. Deeper CI integration will enable diff-based review of configuration changes: what actually changes in the rendered output when you modify a variable?

**Import from existing configs.** `hone import config.yaml` already converts YAML and JSON files to Hone, with optional variable extraction for repeated values. This is the migration path: you do not have to rewrite your configs from scratch.

## Conclusion

The configuration management space is not short on options. But most tools either give you too little (YAML, Kustomize) or ask for too much (Jsonnet, CUE, Dhall). Hone is an opinionated bet that there is a sweet spot: a language that looks like the configs you already write, with just enough power to eliminate the repetition, drift, and runtime errors that make YAML at scale so painful.

It compiles fast. It catches errors early. It fits in your editor. And when you hand a Hone file to a colleague who has never seen it before, they can read it.

That is the point.
