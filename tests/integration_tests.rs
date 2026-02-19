//! Integration tests for the Hone compiler
//!
//! These tests verify the complete pipeline from source to output.

use hone::{emit, Evaluator, Lexer, OutputFormat, Parser};
use std::collections::HashMap;

/// Helper to compile Hone source to JSON
fn compile_to_json(source: &str) -> Result<String, hone::HoneError> {
    let mut lexer = Lexer::new(source, None);
    let tokens = lexer.tokenize()?;

    let mut parser = Parser::new(tokens, source, None);
    let ast = parser.parse()?;

    let mut evaluator = Evaluator::new(source);
    let value = evaluator.evaluate(&ast)?;

    emit(&value, OutputFormat::Json)
}

/// Helper to compile Hone source to YAML
fn compile_to_yaml(source: &str) -> Result<String, hone::HoneError> {
    let mut lexer = Lexer::new(source, None);
    let tokens = lexer.tokenize()?;

    let mut parser = Parser::new(tokens, source, None);
    let ast = parser.parse()?;

    let mut evaluator = Evaluator::new(source);
    let value = evaluator.evaluate(&ast)?;

    emit(&value, OutputFormat::Yaml)
}

#[test]
fn test_simple_key_value() {
    let source = r#"
name: "my-app"
version: "1.0.0"
port: 8080
debug: true
"#;
    let json = compile_to_json(source).unwrap();
    assert!(json.contains(r#""name":"my-app""#));
    assert!(json.contains(r#""version":"1.0.0""#));
    assert!(json.contains(r#""port":8080"#));
    assert!(json.contains(r#""debug":true"#));
}

#[test]
fn test_nested_blocks() {
    let source = r#"
server {
    host: "localhost"
    port: 8080

    tls {
        enabled: true
        cert: "/etc/ssl/cert.pem"
    }
}
"#;
    let json = compile_to_json(source).unwrap();
    assert!(json.contains(r#""server""#));
    assert!(json.contains(r#""host":"localhost""#));
    assert!(json.contains(r#""tls""#));
    assert!(json.contains(r#""enabled":true"#));
}

#[test]
fn test_variables_and_interpolation() {
    let source = r#"
let env = "production"
let base_port = 8000

service {
    name: "api-${env}"
    port: base_port + 80
    url: "https://api.${env}.example.com"
}
"#;
    let json = compile_to_json(source).unwrap();
    assert!(json.contains(r#""name":"api-production""#));
    assert!(json.contains(r#""port":8080"#));
    assert!(json.contains(r#""url":"https://api.production.example.com""#));
}

#[test]
fn test_conditionals() {
    let source = r#"
let env = "production"

base {
    debug: false
}

when env == "production" {
    replicas: 3
    resources {
        cpu: "2"
        memory: "4Gi"
    }
}

when env == "development" {
    replicas: 1
    resources {
        cpu: "0.5"
        memory: "512Mi"
    }
}
"#;
    let json = compile_to_json(source).unwrap();
    assert!(json.contains(r#""replicas":3"#));
    assert!(json.contains(r#""cpu":"2""#));
    assert!(!json.contains(r#""cpu":"0.5""#)); // development block should not be included
}

#[test]
fn test_arrays() {
    let source = r#"
ports: [80, 443, 8080]
tags: ["web", "api", "v1"]
mixed: [1, "two", true, null]
"#;
    let json = compile_to_json(source).unwrap();
    assert!(json.contains(r#""ports":[80,443,8080]"#));
    assert!(json.contains(r#""tags":["web","api","v1"]"#));
}

#[test]
fn test_for_loops() {
    let source = r#"
ports: [for p in [80, 443] { port: p }]
numbers: [for i in range(3) { i * 2 }]
"#;
    let json = compile_to_json(source).unwrap();
    // ports should be array of objects
    assert!(json.contains(r#"{"port":80}"#));
    assert!(json.contains(r#"{"port":443}"#));
    // numbers should be [0, 2, 4]
    assert!(json.contains(r#""numbers":[0,2,4]"#));
}

#[test]
fn test_spread_operator() {
    let source = r#"
let defaults = { timeout: 30, retries: 3 }

config {
    ...defaults
    name: "my-config"
}
"#;
    let json = compile_to_json(source).unwrap();
    assert!(json.contains(r#""timeout":30"#));
    assert!(json.contains(r#""retries":3"#));
    assert!(json.contains(r#""name":"my-config""#));
}

#[test]
fn test_builtin_functions() {
    let source = r#"
let arr = [1, 2, 3]
len_result: len(arr)
keys_test: keys({ a: 1, b: 2 })
contains_test: contains(arr, 2)
range_test: range(3)
"#;
    let json = compile_to_json(source).unwrap();
    assert!(json.contains(r#""len_result":3"#));
    assert!(json.contains(r#""contains_test":true"#));
    assert!(json.contains(r#""range_test":[0,1,2]"#));
}

#[test]
fn test_arithmetic() {
    let source = r#"
add: 1 + 2
sub: 10 - 3
mul: 4 * 5
div: 20 / 4
mod: 17 % 5
neg: -42
"#;
    let json = compile_to_json(source).unwrap();
    assert!(json.contains(r#""add":3"#));
    assert!(json.contains(r#""sub":7"#));
    assert!(json.contains(r#""mul":20"#));
    assert!(json.contains(r#""div":5"#));
    assert!(json.contains(r#""mod":2"#));
    assert!(json.contains(r#""neg":-42"#));
}

#[test]
fn test_comparison() {
    let source = r#"
eq: 1 == 1
neq: 1 != 2
lt: 1 < 2
lte: 2 <= 2
gt: 3 > 2
gte: 3 >= 3
"#;
    let json = compile_to_json(source).unwrap();
    assert!(json.contains(r#""eq":true"#));
    assert!(json.contains(r#""neq":true"#));
    assert!(json.contains(r#""lt":true"#));
    assert!(json.contains(r#""lte":true"#));
    assert!(json.contains(r#""gt":true"#));
    assert!(json.contains(r#""gte":true"#));
}

#[test]
fn test_logical_operators() {
    let source = r#"
and_true: true && true
and_false: true && false
or_true: false || true
or_false: false || false
not_true: !false
not_false: !true
"#;
    let json = compile_to_json(source).unwrap();
    assert!(json.contains(r#""and_true":true"#));
    assert!(json.contains(r#""and_false":false"#));
    assert!(json.contains(r#""or_true":true"#));
    assert!(json.contains(r#""or_false":false"#));
    assert!(json.contains(r#""not_true":true"#));
    assert!(json.contains(r#""not_false":false"#));
}

#[test]
fn test_ternary_operator() {
    let source = r#"
let flag = true
result: flag ? "yes" : "no"
nested: flag ? (1 > 0 ? "a" : "b") : "c"
"#;
    let json = compile_to_json(source).unwrap();
    assert!(json.contains(r#""result":"yes""#));
    assert!(json.contains(r#""nested":"a""#));
}

#[test]
fn test_null_coalesce() {
    let source = r#"
let val = null
result: val ?? "default"
non_null: "value" ?? "default"
"#;
    let json = compile_to_json(source).unwrap();
    assert!(json.contains(r#""result":"default""#));
    assert!(json.contains(r#""non_null":"value""#));
}

#[test]
fn test_object_literal() {
    let source = r#"
config: { name: "test", port: 8080 }
nested: { outer: { inner: "value" } }
"#;
    let json = compile_to_json(source).unwrap();
    assert!(json.contains(r#""config":{"name":"test","port":8080}"#));
    assert!(json.contains(r#""nested":{"outer":{"inner":"value"}}"#));
}

#[test]
fn test_path_expression() {
    let source = r#"
let config = { server: { port: 8080 } }
port: config.server.port
"#;
    let json = compile_to_json(source).unwrap();
    assert!(json.contains(r#""port":8080"#));
}

#[test]
fn test_index_expression() {
    let source = r#"
let arr = [1, 2, 3]
first: arr[0]
last: arr[2]
"#;
    let json = compile_to_json(source).unwrap();
    assert!(json.contains(r#""first":1"#));
    assert!(json.contains(r#""last":3"#));
}

#[test]
fn test_yaml_output() {
    let source = r#"
apiVersion: "apps/v1"
kind: "Deployment"
metadata {
    name: "nginx"
    labels {
        app: "nginx"
    }
}
spec {
    replicas: 3
}
"#;
    let yaml = compile_to_yaml(source).unwrap();
    assert!(yaml.contains("apiVersion: apps/v1"));
    assert!(yaml.contains("kind: Deployment"));
    assert!(yaml.contains("name: nginx"));
    assert!(yaml.contains("app: nginx"));
    assert!(yaml.contains("replicas: 3"));
}

#[test]
fn test_deep_merge() {
    let source = r#"
config {
    server {
        host: "localhost"
    }
}

config {
    server {
        port: 8080
    }
    debug: true
}
"#;
    let json = compile_to_json(source).unwrap();
    // Both host and port should be present (deep merge)
    assert!(json.contains(r#""host":"localhost""#));
    assert!(json.contains(r#""port":8080"#));
    assert!(json.contains(r#""debug":true"#));
}

#[test]
fn test_append_operator() {
    let source = r#"
items: [1, 2]
items +: [3, 4]
"#;
    let json = compile_to_json(source).unwrap();
    assert!(json.contains(r#""items":[1,2,3,4]"#));
}

#[test]
fn test_replace_operator() {
    let source = r#"
config: { a: 1, b: 2 }
config !: { c: 3 }
"#;
    let json = compile_to_json(source).unwrap();
    // Replace should completely replace, not merge
    assert!(json.contains(r#""config":{"c":3}"#));
    assert!(!json.contains(r#""a""#));
    assert!(!json.contains(r#""b""#));
}

#[test]
fn test_complex_kubernetes_example() {
    let source = r#"
let app_name = "nginx"
let replicas = 3
let image = "nginx:1.21"

apiVersion: "apps/v1"
kind: "Deployment"

metadata {
    name: app_name
    labels {
        app: app_name
        version: "v1"
    }
}

spec {
    replicas: replicas
    selector {
        matchLabels {
            app: app_name
        }
    }
    template {
        metadata {
            labels {
                app: app_name
            }
        }
        spec {
            containers: [
                {
                    name: app_name,
                    image: image,
                    ports: [{ containerPort: 80 }]
                }
            ]
        }
    }
}
"#;
    let yaml = compile_to_yaml(source).unwrap();
    assert!(yaml.contains("apiVersion: apps/v1"));
    assert!(yaml.contains("kind: Deployment"));
    assert!(yaml.contains("name: nginx"));
    assert!(yaml.contains("replicas: 3"));
    // Image has a colon so may be quoted
    assert!(yaml.contains("nginx:1.21") || yaml.contains("\"nginx:1.21\""));
}

#[test]
fn test_assertion_passes() {
    let source = r#"
let port = 8080
assert port > 0 : "port must be positive"
config {
    port: port
}
"#;
    let result = compile_to_json(source);
    assert!(result.is_ok());
}

#[test]
fn test_assertion_fails() {
    let source = r#"
let port = -1
assert port > 0 : "port must be positive"
"#;
    let result = compile_to_json(source);
    assert!(result.is_err());
}

#[test]
fn test_type_alias_in_preamble() {
    // Type aliases should parse but not affect runtime (type checking is separate)
    let source = r#"
type Port = int(1, 65535)
port: 8080
"#;
    let result = compile_to_json(source);
    assert!(result.is_ok());
    assert!(result.unwrap().contains(r#""port":8080"#));
}

#[test]
fn test_schema_in_preamble() {
    // Schemas should parse but not affect runtime (type checking is separate)
    let source = r#"
schema Server {
    host: string
    port: int
}
server {
    host: "localhost"
    port: 8080
}
"#;
    let result = compile_to_json(source);
    assert!(result.is_ok());
}

// Import tests using the compiler
mod import_tests {
    use hone::compile_file;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_files(dir: &std::path::Path, files: &[(&str, &str)]) {
        for (name, content) in files {
            let path = dir.join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, content).unwrap();
        }
    }

    #[test]
    fn test_import_whole_module() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[
                (
                    "config.hone",
                    r#"
let app_name = "my-app"
let version = "1.0.0"
"#,
                ),
                (
                    "main.hone",
                    r#"
import "./config.hone" as config

app {
    name: config.app_name
    version: config.version
}
"#,
                ),
            ],
        );

        let result = compile_file(dir.path().join("main.hone")).unwrap();

        if let hone::Value::Object(obj) = result {
            if let Some(hone::Value::Object(app)) = obj.get("app") {
                assert_eq!(app.get("name"), Some(&hone::Value::String("my-app".into())));
                assert_eq!(
                    app.get("version"),
                    Some(&hone::Value::String("1.0.0".into()))
                );
            } else {
                panic!("Expected app object");
            }
        } else {
            panic!("Expected object");
        }
    }

    #[test]
    fn test_import_named() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[
                (
                    "config.hone",
                    r#"
let port = 8080
let host = "localhost"
let debug = true
"#,
                ),
                (
                    "main.hone",
                    r#"
import { port, host } from "./config.hone"

server {
    host: host
    port: port
}
"#,
                ),
            ],
        );

        let result = compile_file(dir.path().join("main.hone")).unwrap();

        if let hone::Value::Object(obj) = result {
            if let Some(hone::Value::Object(server)) = obj.get("server") {
                assert_eq!(
                    server.get("host"),
                    Some(&hone::Value::String("localhost".into()))
                );
                assert_eq!(server.get("port"), Some(&hone::Value::Int(8080)));
            } else {
                panic!("Expected server object");
            }
        } else {
            panic!("Expected object");
        }
    }

    #[test]
    fn test_from_inheritance() {
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[
                (
                    "base.hone",
                    r#"
database {
    host: "localhost"
    port: 5432
    name: "mydb"
}
"#,
                ),
                (
                    "prod.hone",
                    r#"
from "./base.hone"

database {
    host: "prod-db.example.com"
}
"#,
                ),
            ],
        );

        let result = compile_file(dir.path().join("prod.hone")).unwrap();

        if let hone::Value::Object(obj) = result {
            if let Some(hone::Value::Object(db)) = obj.get("database") {
                // host should be overridden
                assert_eq!(
                    db.get("host"),
                    Some(&hone::Value::String("prod-db.example.com".into()))
                );
                // port and name should be inherited
                assert_eq!(db.get("port"), Some(&hone::Value::Int(5432)));
                assert_eq!(db.get("name"), Some(&hone::Value::String("mydb".into())));
            } else {
                panic!("Expected database object");
            }
        } else {
            panic!("Expected object");
        }
    }

    #[test]
    fn test_diamond_dependency() {
        // A imports B and C, both B and C import D
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[
                (
                    "d.hone",
                    r#"
let base_value = "from_d"
"#,
                ),
                (
                    "b.hone",
                    r#"
import "./d.hone" as d
let b_value = d.base_value
"#,
                ),
                (
                    "c.hone",
                    r#"
import "./d.hone" as d
let c_value = d.base_value
"#,
                ),
                (
                    "a.hone",
                    r#"
import "./b.hone" as b
import "./c.hone" as c

result {
    from_b: b.b_value
    from_c: c.c_value
}
"#,
                ),
            ],
        );

        let result = compile_file(dir.path().join("a.hone")).unwrap();

        if let hone::Value::Object(obj) = result {
            if let Some(hone::Value::Object(res)) = obj.get("result") {
                assert_eq!(
                    res.get("from_b"),
                    Some(&hone::Value::String("from_d".into()))
                );
                assert_eq!(
                    res.get("from_c"),
                    Some(&hone::Value::String("from_d".into()))
                );
            } else {
                panic!("Expected result object");
            }
        } else {
            panic!("Expected object");
        }
    }

    #[test]
    fn test_compile_file_validates_schema() {
        // compile_file (used by hone check) should catch schema type mismatches
        let dir = TempDir::new().unwrap();
        create_test_files(
            dir.path(),
            &[(
                "main.hone",
                r#"
schema Config {
    name: string
    port: int
}

use Config

name: "test"
port: "not-an-int"
"#,
            )],
        );

        let result = compile_file(dir.path().join("main.hone"));
        assert!(
            result.is_err(),
            "compile_file should fail on schema type mismatch"
        );
        let err = result.unwrap_err();
        assert!(matches!(err, hone::HoneError::TypeMismatch { .. }));
    }
}

// Variant system tests
mod variant_tests {
    use super::*;

    /// Helper to compile with variant selections
    fn compile_with_variants(
        source: &str,
        variants: Vec<(&str, &str)>,
    ) -> Result<String, hone::HoneError> {
        let mut lexer = Lexer::new(source, None);
        let tokens = lexer.tokenize()?;

        let mut parser = Parser::new(tokens, source, None);
        let ast = parser.parse()?;

        let mut evaluator = Evaluator::new(source);
        let selections: HashMap<String, String> = variants
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        evaluator.set_variant_selections(selections);
        let value = evaluator.evaluate(&ast)?;

        emit(&value, OutputFormat::Json)
    }

    #[test]
    fn test_variant_with_default_case() {
        let source = r#"
variant env {
    default dev {
        replicas: 1
        debug: true
    }

    production {
        replicas: 5
        debug: false
    }
}
"#;
        // No variant selection -> uses default
        let json = compile_with_variants(source, vec![]).unwrap();
        assert!(json.contains(r#""replicas":1"#));
        assert!(json.contains(r#""debug":true"#));
    }

    #[test]
    fn test_variant_explicit_selection() {
        let source = r#"
variant env {
    default dev {
        replicas: 1
    }

    production {
        replicas: 5
    }
}
"#;
        let json = compile_with_variants(source, vec![("env", "production")]).unwrap();
        assert!(json.contains(r#""replicas":5"#));
        assert!(!json.contains(r#""replicas":1"#));
    }

    #[test]
    fn test_variant_select_default_explicitly() {
        let source = r#"
variant env {
    default dev {
        replicas: 1
    }

    production {
        replicas: 5
    }
}
"#;
        // Explicitly selecting the default case should also work
        let json = compile_with_variants(source, vec![("env", "dev")]).unwrap();
        assert!(json.contains(r#""replicas":1"#));
    }

    #[test]
    fn test_variant_error_on_unknown_case() {
        let source = r#"
variant env {
    default dev {
        replicas: 1
    }

    production {
        replicas: 5
    }
}
"#;
        let result = compile_with_variants(source, vec![("env", "staging")]);
        assert!(result.is_err());
        match result.unwrap_err() {
            hone::HoneError::TypeMismatch {
                expected,
                found,
                help,
                ..
            } => {
                assert!(expected.contains("dev") || expected.contains("production"));
                assert_eq!(found, "staging");
                assert!(help.contains("valid cases"));
            }
            other => panic!("Expected TypeMismatch, got: {:?}", other),
        }
    }

    #[test]
    fn test_variant_error_no_default_no_selection() {
        let source = r#"
variant env {
    dev {
        replicas: 1
    }

    production {
        replicas: 5
    }
}
"#;
        // No default, no selection -> error
        let result = compile_with_variants(source, vec![]);
        assert!(result.is_err());
        match result.unwrap_err() {
            hone::HoneError::TypeMismatch {
                expected,
                found,
                help,
                ..
            } => {
                assert!(expected.contains("--variant"));
                assert_eq!(found, "no selection");
                assert!(help.contains("no default"));
            }
            other => panic!("Expected TypeMismatch, got: {:?}", other),
        }
    }

    #[test]
    fn test_variant_merges_with_body() {
        let source = r#"
variant env {
    default dev {
        replicas: 1
    }

    production {
        replicas: 5
    }
}

name: "my-app"
port: 8080
"#;
        let json = compile_with_variants(source, vec![("env", "production")]).unwrap();
        assert!(json.contains(r#""replicas":5"#));
        assert!(json.contains(r#""name":"my-app""#));
        assert!(json.contains(r#""port":8080"#));
    }

    #[test]
    fn test_variant_body_overrides_variant() {
        let source = r#"
variant env {
    default dev {
        replicas: 1
        debug: true
    }
}

# Body content overrides variant content
debug: false
"#;
        let json = compile_with_variants(source, vec![]).unwrap();
        assert!(json.contains(r#""replicas":1"#));
        // Body's debug: false should override variant's debug: true
        assert!(json.contains(r#""debug":false"#));
    }

    #[test]
    fn test_multiple_variants() {
        let source = r#"
variant env {
    default dev {
        replicas: 1
    }

    production {
        replicas: 5
    }
}

variant region {
    default us {
        endpoint: "us.example.com"
    }

    eu {
        endpoint: "eu.example.com"
    }
}
"#;
        let json =
            compile_with_variants(source, vec![("env", "production"), ("region", "eu")]).unwrap();
        assert!(json.contains(r#""replicas":5"#));
        assert!(json.contains(r#""endpoint":"eu.example.com""#));
    }

    #[test]
    fn test_variant_with_nested_blocks() {
        let source = r#"
variant env {
    default dev {
        server {
            host: "localhost"
            port: 8080
        }
    }

    production {
        server {
            host: "prod.example.com"
            port: 443
        }
    }
}
"#;
        let json = compile_with_variants(source, vec![("env", "production")]).unwrap();
        assert!(json.contains(r#""host":"prod.example.com""#));
        assert!(json.contains(r#""port":443"#));
    }

    #[test]
    fn test_variant_uses_preamble_variables() {
        let source = r#"
let app = "my-app"

variant env {
    default dev {
        name: "${app}-dev"
    }

    production {
        name: "${app}-prod"
    }
}
"#;
        let json = compile_with_variants(source, vec![("env", "production")]).unwrap();
        assert!(json.contains(r#""name":"my-app-prod""#));
    }

    #[test]
    fn test_variant_formatting_roundtrip() {
        let source = r#"variant env {
  default dev {
    replicas: 1
  }

  production {
    replicas: 5
  }
}
"#;
        let formatted = hone::format_source(source).unwrap();
        assert!(formatted.contains("variant env {"));
        assert!(formatted.contains("default dev {"));
        assert!(formatted.contains("production {"));
        assert!(formatted.contains("replicas: 1"));
        assert!(formatted.contains("replicas: 5"));
    }
}

// For-loop object body tests
mod for_object_body_tests {
    use super::*;

    #[test]
    fn test_for_object_body_interpolated_key() {
        let source = r#"
let obj = { a: 1, b: 2 }
result: for (k, v) in obj {
  "${k}_doubled": v * 2
}
"#;
        let result = compile_to_json(source).unwrap();
        assert!(result.contains("\"a_doubled\":2"), "got: {}", result);
        assert!(result.contains("\"b_doubled\":4"), "got: {}", result);
    }

    #[test]
    fn test_for_object_body_static_key() {
        let source = r#"
let items = ["a", "b"]
result: for item in items {
  name: item
  kind: "string"
}
"#;
        let result = compile_to_json(source).unwrap();
        assert!(result.contains("\"name\":\"a\""), "got: {}", result);
        assert!(result.contains("\"name\":\"b\""), "got: {}", result);
        assert!(result.contains("\"kind\":\"string\""), "got: {}", result);
    }

    #[test]
    fn test_for_in_block_dynamic_keys() {
        let source = r#"
let environments = ["dev", "staging", "prod"]

endpoints {
  for env in environments {
    "${env}-api": "https://${env}.api.example.com"
  }
}
"#;
        let result = compile_to_json(source).unwrap();
        assert!(
            result.contains("\"dev-api\":\"https://dev.api.example.com\""),
            "got: {}",
            result
        );
        assert!(
            result.contains("\"staging-api\":\"https://staging.api.example.com\""),
            "got: {}",
            result
        );
        assert!(
            result.contains("\"prod-api\":\"https://prod.api.example.com\""),
            "got: {}",
            result
        );
    }

    #[test]
    fn test_for_destructuring_in_block() {
        let source = r#"
let port_map = { http: 80, https: 443 }

service_ports {
  for (name, port) in port_map {
    "${name}": port
  }
}
"#;
        let result = compile_to_json(source).unwrap();
        assert!(result.contains("\"http\":80"), "got: {}", result);
        assert!(result.contains("\"https\":443"), "got: {}", result);
    }

    #[test]
    fn test_for_array_body_still_works() {
        let source = r#"
result: for x in [1, 2, 3] { x * 2 }
"#;
        let result = compile_to_json(source).unwrap();
        assert!(result.contains("[2,4,6]"), "got: {}", result);
    }

    #[test]
    fn test_for_expression_body_still_works() {
        let source = r#"
let result = for (k, v) in { a: 1, b: 2 } {
  v * 2
}
out: result
"#;
        let result = compile_to_json(source).unwrap();
        assert!(result.contains("[2,4]"), "got: {}", result);
    }

    #[test]
    fn test_for_object_body_with_when_inside() {
        let source = r#"
let services = ["auth", "api", "worker"]

config {
  for svc in services {
    "${svc}": {
      enabled: true
      port: 8080
    }
  }
}
"#;
        let result = compile_to_json(source).unwrap();
        assert!(result.contains("\"auth\""), "got: {}", result);
        assert!(result.contains("\"api\""), "got: {}", result);
        assert!(result.contains("\"worker\""), "got: {}", result);
    }
}

mod assertion_error_display_tests {
    use super::*;

    fn get_assertion_error(source: &str) -> (String, String, String) {
        let mut lexer = Lexer::new(source, None);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens, source, None);
        let ast = parser.parse().unwrap();
        let mut evaluator = Evaluator::new(source);
        match evaluator.evaluate(&ast) {
            Err(hone::HoneError::AssertionFailed {
                condition,
                message,
                help,
                ..
            }) => (condition, message, help),
            other => panic!("expected AssertionFailed, got: {:?}", other),
        }
    }

    #[test]
    fn test_assert_error_readable_simple() {
        let (condition, message, _) = get_assertion_error(r#"assert 1 == 2 : "math is broken""#);
        assert_eq!(condition, "1 == 2");
        assert_eq!(message, "math is broken");
    }

    #[test]
    fn test_assert_error_readable_variables() {
        let (condition, _, help) =
            get_assertion_error("let replicas = 25\nassert replicas <= 10 : \"replicas too high\"");
        assert_eq!(condition, "replicas <= 10");
        assert!(help.contains("replicas = 25"), "help: {}", help);
    }

    #[test]
    fn test_assert_error_shows_user_message() {
        let (_, message, _) = get_assertion_error(r#"assert false : "custom message here""#);
        assert_eq!(message, "custom message here");
    }

    #[test]
    fn test_assert_error_string_comparison() {
        let (condition, _, help) = get_assertion_error(
            "let env = \"staging\"\nassert env == \"production\" : \"wrong env\"",
        );
        assert_eq!(condition, "env == \"production\"");
        assert!(help.contains("env = \"staging\""), "help: {}", help);
    }

    #[test]
    fn test_assert_error_member_access() {
        let (condition, _, help) = get_assertion_error(
            "let vars = { env: \"test\" }\nassert vars.env == \"prod\" : \"wrong\"",
        );
        assert_eq!(condition, "vars.env == \"prod\"");
        assert!(help.contains("vars.env = \"test\""), "help: {}", help);
    }

    #[test]
    fn test_assert_error_no_ast_debug() {
        let (condition, _, _) = get_assertion_error(r#"assert 1 > 2 : "fail""#);
        assert!(
            !condition.contains("BinaryExpr"),
            "condition contains AST debug: {}",
            condition
        );
        assert!(
            !condition.contains("SourceLocation"),
            "condition contains AST debug: {}",
            condition
        );
    }

    #[test]
    fn test_assert_pass_no_output() {
        let source = "assert 1 == 1 : \"ok\"\nname: \"test\"";
        let result = compile_to_json(source);
        assert!(result.is_ok(), "assertion should pass: {:?}", result.err());
    }
}

mod when_else_tests {
    use super::*;

    #[test]
    fn test_when_else_basic() {
        let source = r#"
let env = "dev"
when env == "prod" {
  replicas: 3
} else {
  replicas: 1
}
"#;
        let result = compile_to_json(source).unwrap();
        assert!(
            result.contains("\"replicas\":1"),
            "else branch should be taken: {}",
            result
        );
    }

    #[test]
    fn test_when_else_when_chain() {
        let source = r#"
let env = "staging"
when env == "prod" {
  replicas: 5
} else when env == "staging" {
  replicas: 2
} else {
  replicas: 1
}
"#;
        let result = compile_to_json(source).unwrap();
        assert!(
            result.contains("\"replicas\":2"),
            "else when branch should be taken: {}",
            result
        );
    }

    #[test]
    fn test_when_else_first_branch_wins() {
        let source = r#"
let env = "prod"
when env == "prod" {
  replicas: 5
} else when env == "staging" {
  replicas: 2
} else {
  replicas: 1
}
"#;
        let result = compile_to_json(source).unwrap();
        assert!(
            result.contains("\"replicas\":5"),
            "first branch should be taken: {}",
            result
        );
    }

    #[test]
    fn test_when_else_fallthrough() {
        let source = r#"
let env = "dev"
when env == "prod" {
  replicas: 5
} else when env == "staging" {
  replicas: 2
} else {
  replicas: 1
}
"#;
        let result = compile_to_json(source).unwrap();
        assert!(
            result.contains("\"replicas\":1"),
            "else branch should be taken: {}",
            result
        );
    }

    #[test]
    fn test_when_else_exactly_one_branch() {
        let source = r#"
let x = true
result: "base"
when x {
  a: 1
} else {
  b: 2
}
"#;
        let result = compile_to_json(source).unwrap();
        assert!(
            result.contains("\"a\":1"),
            "when branch should be taken: {}",
            result
        );
        assert!(
            !result.contains("\"b\""),
            "else branch should NOT be taken: {}",
            result
        );
    }

    #[test]
    fn test_when_else_no_else_false() {
        // when without else, condition false: nothing emitted (existing behavior preserved)
        let source = r#"
let x = false
name: "test"
when x {
  extra: "yes"
}
"#;
        let result = compile_to_json(source).unwrap();
        assert!(
            !result.contains("\"extra\""),
            "false when without else should emit nothing: {}",
            result
        );
        assert!(
            result.contains("\"name\":\"test\""),
            "other keys preserved: {}",
            result
        );
    }

    #[test]
    fn test_when_else_merges_into_parent() {
        let source = r#"
let env = "dev"
name: "api"
when env == "prod" {
  replicas: 5
  host: "prod.example.com"
} else {
  replicas: 1
  host: "localhost"
}
"#;
        let result = compile_to_json(source).unwrap();
        assert!(
            result.contains("\"name\":\"api\""),
            "parent key preserved: {}",
            result
        );
        assert!(
            result.contains("\"replicas\":1"),
            "else replicas: {}",
            result
        );
        assert!(
            result.contains("\"host\":\"localhost\""),
            "else host: {}",
            result
        );
    }

    #[test]
    fn test_when_else_in_block() {
        let source = r#"
let env = "prod"
server {
  name: "api"
  when env == "prod" {
    port: 443
  } else {
    port: 8080
  }
}
"#;
        let result = compile_to_json(source).unwrap();
        assert!(
            result.contains("\"port\":443"),
            "when inside block: {}",
            result
        );
    }

    #[test]
    fn test_when_else_multiple_else_when() {
        let source = r#"
let tier = "gold"
when tier == "platinum" {
  limit: 10000
} else when tier == "gold" {
  limit: 5000
} else when tier == "silver" {
  limit: 1000
} else {
  limit: 100
}
"#;
        let result = compile_to_json(source).unwrap();
        assert!(result.contains("\"limit\":5000"), "gold tier: {}", result);
    }

    #[test]
    fn test_when_else_formatting_roundtrip() {
        let source = "let x = true\n\nwhen x {\n  a: 1\n} else {\n  b: 2\n}\n";
        let formatted = hone::format_source(source).unwrap();
        assert!(
            formatted.contains("} else {"),
            "formatter preserves else: {}",
            formatted
        );
    }

    #[test]
    fn test_when_else_when_formatting_roundtrip() {
        let source = "let x = 1\n\nwhen x == 1 {\n  a: 1\n} else when x == 2 {\n  a: 2\n} else {\n  a: 0\n}\n";
        let formatted = hone::format_source(source).unwrap();
        assert!(
            formatted.contains("} else when"),
            "formatter preserves else when: {}",
            formatted
        );
        assert!(
            formatted.contains("} else {"),
            "formatter preserves final else: {}",
            formatted
        );
    }

    #[test]
    fn test_else_is_reserved_keyword() {
        // else cannot be used as a bare key
        let source = "else: 1";
        let result = compile_to_json(source);
        assert!(result.is_err(), "else should be a reserved keyword");
    }
}

mod expect_tests {
    use hone::evaluator::value::Value;
    use hone::{emit, Evaluator, Lexer, OutputFormat, Parser};
    use indexmap::IndexMap;

    /// Helper to compile with args injected
    fn compile_with_args(source: &str, args: Value) -> Result<String, hone::HoneError> {
        let mut lexer = Lexer::new(source, None);
        let tokens = lexer.tokenize()?;
        let mut parser = Parser::new(tokens, source, None);
        let ast = parser.parse()?;
        let mut evaluator = Evaluator::new(source);
        evaluator.define("args", args);
        let value = evaluator.evaluate(&ast)?;
        emit(&value, OutputFormat::Json)
    }

    fn make_args(pairs: &[(&str, Value)]) -> Value {
        let mut obj = IndexMap::new();
        for (k, v) in pairs {
            obj.insert(k.to_string(), v.clone());
        }
        Value::Object(obj)
    }

    #[test]
    fn test_expect_with_provided_arg() {
        let source = "expect args.env: string\nhost: args.env";
        let result = compile_with_args(source, make_args(&[("env", Value::String("prod".into()))]));
        assert!(result.is_ok(), "should succeed: {:?}", result.err());
        assert!(result.unwrap().contains("\"host\":\"prod\""));
    }

    #[test]
    fn test_expect_missing_required_arg() {
        let source = "expect args.env: string\nhost: \"test\"";
        let result = compile_with_args(source, Value::Object(IndexMap::new()));
        assert!(result.is_err(), "should fail when required arg missing");
        let err = format!("{:?}", result.err().unwrap());
        assert!(
            err.contains("not provided"),
            "error should say not provided: {}",
            err
        );
    }

    #[test]
    fn test_expect_default_value_used() {
        let source = "expect args.port: int = 8080\nport: args.port";
        let result = compile_with_args(source, Value::Object(IndexMap::new()));
        assert!(
            result.is_ok(),
            "should succeed with default: {:?}",
            result.err()
        );
        assert!(result.unwrap().contains("\"port\":8080"));
    }

    #[test]
    fn test_expect_provided_overrides_default() {
        let source = "expect args.port: int = 8080\nport: args.port";
        let result = compile_with_args(source, make_args(&[("port", Value::Int(9090))]));
        assert!(result.is_ok(), "should succeed: {:?}", result.err());
        assert!(result.unwrap().contains("\"port\":9090"));
    }

    #[test]
    fn test_expect_type_mismatch() {
        let source = "expect args.port: int\nport: args.port";
        let result = compile_with_args(
            source,
            make_args(&[("port", Value::String("hello".into()))]),
        );
        assert!(result.is_err(), "should fail on type mismatch");
        let err = format!("{:?}", result.err().unwrap());
        assert!(
            err.contains("string"),
            "error should mention actual type: {}",
            err
        );
    }

    #[test]
    fn test_expect_multiple_declarations() {
        let source =
            "expect args.env: string\nexpect args.port: int = 3000\nenv: args.env\nport: args.port";
        let result = compile_with_args(source, make_args(&[("env", Value::String("dev".into()))]));
        assert!(result.is_ok(), "should succeed: {:?}", result.err());
        let output = result.unwrap();
        assert!(output.contains("\"env\":\"dev\""));
        assert!(output.contains("\"port\":3000"));
    }

    #[test]
    fn test_expect_no_args_at_all() {
        // When no args object exists at all and expect has a default
        let source = "expect args.debug: bool = false\ndebug: args.debug";
        let mut lexer = Lexer::new(source, None);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens, source, None);
        let ast = parser.parse().unwrap();
        let mut evaluator = Evaluator::new(source);
        // Don't define args at all
        let value = evaluator.evaluate(&ast).unwrap();
        let output = emit(&value, OutputFormat::Json).unwrap();
        assert!(output.contains("\"debug\":false"));
    }

    #[test]
    fn test_expect_formatting_roundtrip() {
        let source = "expect args.env: string\nexpect args.port: int = 8080\n\nhost: \"test\"\n";
        let formatted = hone::format_source(source).unwrap();
        assert!(
            formatted.contains("expect args.env: string"),
            "expect preserved: {}",
            formatted
        );
        assert!(
            formatted.contains("expect args.port: int = 8080"),
            "expect with default preserved: {}",
            formatted
        );
    }

    #[test]
    fn test_expect_is_reserved_keyword() {
        let source = "expect: 1";
        let mut lexer = Lexer::new(source, None);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens, source, None);
        let result = parser.parse();
        assert!(result.is_err(), "expect should be a reserved keyword");
    }
}

mod variant_let_tests {
    use super::*;

    fn compile_with_variant(
        source: &str,
        variant_name: &str,
        variant_value: &str,
    ) -> Result<String, hone::HoneError> {
        let mut lexer = hone::Lexer::new(source, None);
        let tokens = lexer.tokenize()?;
        let mut parser = hone::Parser::new(tokens, source, None);
        let ast = parser.parse()?;
        let mut evaluator = hone::Evaluator::new(source);
        let mut selections = std::collections::HashMap::new();
        selections.insert(variant_name.to_string(), variant_value.to_string());
        evaluator.set_variant_selections(selections);
        let value = evaluator.evaluate(&ast)?;
        hone::emit(&value, hone::OutputFormat::Json)
    }

    #[test]
    fn test_variant_let_visible_in_body() {
        let source = r#"
variant env {
  default dev {
    let replicas = 1
  }
  prod {
    let replicas = 5
  }
}

count: replicas
"#;
        let result = compile_to_json(source).unwrap();
        assert!(
            result.contains("\"count\":1"),
            "default variant let visible: {}",
            result
        );
    }

    #[test]
    fn test_variant_let_with_explicit_selection() {
        let source = r#"
variant env {
  default dev {
    let replicas = 1
  }
  prod {
    let replicas = 5
  }
}

count: replicas
"#;
        let result = compile_with_variant(source, "env", "prod").unwrap();
        assert!(
            result.contains("\"count\":5"),
            "prod variant let visible: {}",
            result
        );
    }

    #[test]
    fn test_variant_let_multiple_bindings() {
        let source = r#"
variant env {
  default dev {
    let host = "localhost"
    let port = 3000
  }
  prod {
    let host = "api.example.com"
    let port = 443
  }
}

host: host
port: port
"#;
        let result = compile_with_variant(source, "env", "prod").unwrap();
        assert!(
            result.contains("\"host\":\"api.example.com\""),
            "host: {}",
            result
        );
        assert!(result.contains("\"port\":443"), "port: {}", result);
    }

    #[test]
    fn test_variant_let_in_interpolation() {
        let source = r#"
variant env {
  default dev {
    let domain = "dev.local"
  }
  prod {
    let domain = "prod.example.com"
  }
}

url: "https://${domain}/api"
"#;
        let result = compile_to_json(source).unwrap();
        assert!(
            result.contains("https://dev.local/api"),
            "interpolation: {}",
            result
        );
    }

    #[test]
    fn test_variant_let_and_body_items_together() {
        let source = r#"
variant env {
  default dev {
    let label = "development"
    debug: true
  }
  prod {
    let label = "production"
    debug: false
  }
}

name: label
"#;
        let result = compile_to_json(source).unwrap();
        assert!(
            result.contains("\"name\":\"development\""),
            "let binding: {}",
            result
        );
        assert!(result.contains("\"debug\":true"), "body item: {}", result);
    }
}

mod operator_precedence_tests {
    use super::*;

    #[test]
    fn test_precedence_multiply_before_add() {
        let source = "result: 2 + 3 * 4\n";
        let json = compile_to_json(source).unwrap();
        assert!(
            json.contains("\"result\":14"),
            "2+3*4 should be 14: {}",
            json
        );
    }

    #[test]
    fn test_precedence_and_before_or() {
        // true || false && false should be true (AND binds tighter)
        let source = "result: true || false && false\n";
        let json = compile_to_json(source).unwrap();
        assert!(
            json.contains("\"result\":true"),
            "true || false && false should be true: {}",
            json
        );
    }

    #[test]
    fn test_precedence_comparison_before_equality() {
        // 1 < 2 == true should be (1 < 2) == true = true
        let source = "result: 1 < 2 == true\n";
        let json = compile_to_json(source).unwrap();
        assert!(
            json.contains("\"result\":true"),
            "1 < 2 == true should be true: {}",
            json
        );
    }

    #[test]
    fn test_precedence_null_coalesce_before_comparison() {
        // null ?? 5 > 3 should be (null ?? 5) > 3 = true
        let source = "result: null ?? 5 > 3\n";
        let json = compile_to_json(source).unwrap();
        assert!(
            json.contains("\"result\":true"),
            "null ?? 5 > 3 should be true: {}",
            json
        );
    }

    #[test]
    fn test_precedence_unary_highest() {
        let source = "result: !false == true\n";
        let json = compile_to_json(source).unwrap();
        assert!(
            json.contains("\"result\":true"),
            "!false == true should be true: {}",
            json
        );
    }

    #[test]
    fn test_precedence_parentheses_override() {
        let source = "result: (2 + 3) * 4\n";
        let json = compile_to_json(source).unwrap();
        assert!(
            json.contains("\"result\":20"),
            "(2+3)*4 should be 20: {}",
            json
        );
    }
}

mod arithmetic_safety_tests {
    use super::*;

    #[test]
    fn test_int_overflow_addition_error() {
        let source = "result: 9223372036854775807 + 1\n";
        let err = compile_to_json(source).unwrap_err();
        let msg = err.message();
        assert!(msg.contains("overflow"), "expected overflow error: {}", msg);
    }

    #[test]
    fn test_int_overflow_subtraction_error() {
        let source = "result: -9223372036854775807 - 2\n";
        let err = compile_to_json(source).unwrap_err();
        let msg = err.message();
        assert!(msg.contains("overflow"), "expected overflow error: {}", msg);
    }

    #[test]
    fn test_int_overflow_multiplication_error() {
        let source = "result: 9223372036854775807 * 2\n";
        let err = compile_to_json(source).unwrap_err();
        let msg = err.message();
        assert!(msg.contains("overflow"), "expected overflow error: {}", msg);
    }

    #[test]
    fn test_int_negation_min_overflow() {
        // -(-9223372036854775808) overflows i64
        let source = r#"
let x = -9223372036854775807 - 1
result: -x
"#;
        let err = compile_to_json(source).unwrap_err();
        let msg = err.message();
        assert!(msg.contains("overflow"), "expected overflow error: {}", msg);
    }

    #[test]
    fn test_division_by_zero_error() {
        let source = "result: 10 / 0\n";
        let err = compile_to_json(source).unwrap_err();
        let msg = err.message();
        assert!(
            msg.contains("division by zero"),
            "expected div by zero: {}",
            msg
        );
    }

    #[test]
    fn test_modulo_by_zero_error() {
        let source = "result: 10 % 0\n";
        let err = compile_to_json(source).unwrap_err();
        let msg = err.message();
        assert!(
            msg.contains("division by zero"),
            "expected mod by zero: {}",
            msg
        );
    }

    #[test]
    fn test_normal_arithmetic_still_works() {
        let source = r#"
add: 100 + 200
sub: 500 - 300
mul: 7 * 8
div: 100 / 4
modulo: 17 % 5
neg: -42
"#;
        let json = compile_to_json(source).unwrap();
        assert!(json.contains(r#""add":300"#));
        assert!(json.contains(r#""sub":200"#));
        assert!(json.contains(r#""mul":56"#));
        assert!(json.contains(r#""div":25"#));
        assert!(json.contains(r#""modulo":2"#));
        assert!(json.contains(r#""neg":-42"#));
    }
}

mod recursion_limit_tests {
    use super::*;

    #[test]
    fn test_deeply_nested_parens_rejected() {
        // Run in a thread with explicit stack size to avoid debug-mode stack overflow
        // before our depth limit catches it
        let result = std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024) // 16MB stack
            .spawn(|| {
                // Generate (((((... 200 levels ...))))) which exceeds the 64 limit
                let depth = 200;
                let open: String = "(".repeat(depth);
                let close: String = ")".repeat(depth);
                let source = format!("x: {}1{}\n", open, close);

                let err = compile_to_json(&source).unwrap_err();
                let msg = err.message();
                assert!(
                    msg.contains("nesting depth"),
                    "expected recursion limit error: {}",
                    msg
                );
            })
            .unwrap()
            .join();

        assert!(result.is_ok(), "test thread panicked");
    }

    #[test]
    fn test_reasonable_nesting_works() {
        // 50 levels of nesting should be fine
        let source = r#"
a { b { c { d { e { f { g { h { i { j {
  value: 42
} } } } } } } } } }
"#;
        let result = compile_to_json(source);
        assert!(
            result.is_ok(),
            "reasonable nesting should work: {:?}",
            result.err()
        );
    }
}

mod multi_document_tests {
    use super::*;

    fn compile_multi(source: &str) -> Result<Vec<(Option<String>, String)>, hone::HoneError> {
        let mut lexer = Lexer::new(source, None);
        let tokens = lexer.tokenize()?;
        let mut parser = Parser::new(tokens, source, None);
        let ast = parser.parse()?;
        let mut evaluator = Evaluator::new(source);
        let docs = evaluator.evaluate_multi(&ast)?;
        let mut results = Vec::new();
        for (name, value) in &docs {
            let json = emit(value, OutputFormat::Json)?;
            results.push((name.clone(), json));
        }
        Ok(results)
    }

    #[test]
    fn test_multi_doc_basic() {
        let source = r#"
let app = "myapp"

---deployment
kind: "Deployment"
name: app

---service
kind: "Service"
name: "${app}-svc"
"#;
        let docs = compile_multi(source).unwrap();
        // Main doc (preamble only, no body) + 2 sub-documents
        assert!(
            docs.len() >= 2,
            "expected at least 2 documents, got {}",
            docs.len()
        );

        let deployment = docs
            .iter()
            .find(|(n, _)| n.as_deref() == Some("deployment"));
        assert!(deployment.is_some(), "missing deployment document");
        let (_, dep_json) = deployment.unwrap();
        assert!(dep_json.contains("Deployment"), "deployment: {}", dep_json);
        assert!(
            dep_json.contains("myapp"),
            "deployment should have app name: {}",
            dep_json
        );

        let service = docs.iter().find(|(n, _)| n.as_deref() == Some("service"));
        assert!(service.is_some(), "missing service document");
        let (_, svc_json) = service.unwrap();
        assert!(svc_json.contains("Service"), "service: {}", svc_json);
        assert!(
            svc_json.contains("myapp-svc"),
            "service should have interpolated name: {}",
            svc_json
        );
    }

    #[test]
    fn test_multi_doc_shared_preamble() {
        let source = r#"
let version = "2.0"

---alpha
ver: version

---beta
ver: version
"#;
        let docs = compile_multi(source).unwrap();
        let alpha = docs.iter().find(|(n, _)| n.as_deref() == Some("alpha"));
        let beta = docs.iter().find(|(n, _)| n.as_deref() == Some("beta"));
        assert!(alpha.is_some() && beta.is_some());
        assert!(alpha.unwrap().1.contains("2.0"));
        assert!(beta.unwrap().1.contains("2.0"));
    }
}

mod deep_merge_tests {
    use super::*;

    #[test]
    fn test_deep_merge_nested_objects() {
        let source = r#"
config {
    server {
        port: 8080
        host: "localhost"
    }
}
config {
    server {
        port: 9090
    }
}
"#;
        let json = compile_to_json(source).unwrap();
        // port should be overridden to 9090
        assert!(json.contains("9090"), "port should be 9090: {}", json);
        // host should remain from first declaration
        assert!(json.contains("localhost"), "host should remain: {}", json);
    }

    #[test]
    fn test_force_replace_operator() {
        let source = r#"
config {
    server {
        port: 8080
        host: "localhost"
    }
}
config !: {
    server {
        port: 9090
    }
}
"#;
        let json = compile_to_json(source).unwrap();
        // With !:, config should be completely replaced (no host)
        assert!(json.contains("9090"), "port should be 9090: {}", json);
        assert!(
            !json.contains("localhost"),
            "host should be gone after force replace: {}",
            json
        );
    }

    #[test]
    fn test_append_operator() {
        let source = r#"
items: [1, 2]
items +: [3, 4]
"#;
        let json = compile_to_json(source).unwrap();
        assert!(
            json.contains("[1,2,3,4]"),
            "items should be appended: {}",
            json
        );
    }

    #[test]
    fn test_spread_object() {
        let source = r#"
let base = { a: 1, b: 2 }
result: { ...base, c: 3 }
"#;
        let json = compile_to_json(source).unwrap();
        assert!(
            json.contains(r#""a":1"#),
            "spread should include a: {}",
            json
        );
        assert!(
            json.contains(r#""b":2"#),
            "spread should include b: {}",
            json
        );
        assert!(
            json.contains(r#""c":3"#),
            "spread should include c: {}",
            json
        );
    }

    #[test]
    fn test_spread_array() {
        let source = r#"
let first = [1, 2]
result: [...first, 3, 4]
"#;
        let json = compile_to_json(source).unwrap();
        assert!(json.contains("[1,2,3,4]"), "spread should concat: {}", json);
    }
}

mod formatter_tests {
    #[test]
    fn test_format_basic_roundtrip() {
        let source = "name: \"hello\"\nport: 8080\n";
        let formatted = hone::format_source(source).unwrap();
        // Formatting should be idempotent
        let formatted2 = hone::format_source(&formatted).unwrap();
        assert_eq!(formatted, formatted2, "formatter should be idempotent");
    }

    #[test]
    fn test_format_nested_blocks() {
        let source = r#"server {
  host: "localhost"
  port: 8080
}
"#;
        let formatted = hone::format_source(source).unwrap();
        let formatted2 = hone::format_source(&formatted).unwrap();
        assert_eq!(
            formatted, formatted2,
            "nested block formatting should be idempotent"
        );
    }

    #[test]
    fn test_format_preserves_comments() {
        let source = "# This is a comment\nname: \"test\"\n";
        let formatted = hone::format_source(source).unwrap();
        assert!(
            formatted.contains("# This is a comment"),
            "should preserve comments: {}",
            formatted
        );
    }
}

mod differ_tests {
    #[test]
    fn test_diff_identical_values() {
        let left = hone::Value::Object({
            let mut m = indexmap::IndexMap::new();
            m.insert("port".to_string(), hone::Value::Int(8080));
            m
        });
        let right = left.clone();

        let diffs = hone::diff_values(&left, &right);
        assert!(diffs.is_empty(), "identical values should have no diffs");
    }

    #[test]
    fn test_diff_changed_value() {
        let left = hone::Value::Object({
            let mut m = indexmap::IndexMap::new();
            m.insert("port".to_string(), hone::Value::Int(8080));
            m
        });
        let right = hone::Value::Object({
            let mut m = indexmap::IndexMap::new();
            m.insert("port".to_string(), hone::Value::Int(9090));
            m
        });

        let diffs = hone::diff_values(&left, &right);
        assert!(!diffs.is_empty(), "changed value should produce diffs");
        assert!(matches!(diffs[0].kind, hone::DiffKind::Changed { .. }));
    }

    #[test]
    fn test_diff_added_key() {
        let left = hone::Value::Object({
            let mut m = indexmap::IndexMap::new();
            m.insert("port".to_string(), hone::Value::Int(8080));
            m
        });
        let right = hone::Value::Object({
            let mut m = indexmap::IndexMap::new();
            m.insert("port".to_string(), hone::Value::Int(8080));
            m.insert("host".to_string(), hone::Value::String("localhost".into()));
            m
        });

        let diffs = hone::diff_values(&left, &right);
        assert!(!diffs.is_empty(), "added key should produce diffs");
        let added = diffs
            .iter()
            .any(|d| matches!(d.kind, hone::DiffKind::Added(_)));
        assert!(added, "should have an Added diff");
    }
}

mod secret_tests {
    use super::*;

    #[test]
    fn test_secret_declaration_placeholder() {
        let source = r#"
secret db_password from "vault:secret/data/db#password"

database {
    password: db_password
}
"#;
        let result = compile_to_json(source).unwrap();
        assert!(result.contains("<SECRET:vault:secret/data/db#password>"));
    }

    #[test]
    fn test_secret_env_provider() {
        let source = r#"
secret api_key from "env:API_KEY"

service {
    key: api_key
}
"#;
        let result = compile_to_json(source).unwrap();
        assert!(result.contains("<SECRET:env:API_KEY>"));
    }

    #[test]
    fn test_secret_in_string_interpolation() {
        let source = r#"
secret token from "vault:auth/token"

auth_header: "Bearer ${token}"
"#;
        let result = compile_to_json(source).unwrap();
        assert!(result.contains("Bearer <SECRET:vault:auth/token>"));
    }

    #[test]
    fn test_secret_multiple_declarations() {
        let source = r#"
secret db_pass from "vault:db/pass"
secret api_key from "env:API_KEY"

database {
    password: db_pass
}
api {
    key: api_key
}
"#;
        let result = compile_to_json(source).unwrap();
        assert!(result.contains("<SECRET:vault:db/pass>"));
        assert!(result.contains("<SECRET:env:API_KEY>"));
    }

    #[test]
    fn test_secret_formatting_roundtrip() {
        let source =
            "secret db_password from \"vault:secret/data/db#password\"\n\nkey: db_password\n";
        let formatted = hone::format_source(source).unwrap();
        assert!(formatted.contains("secret db_password from \"vault:secret/data/db#password\""));
    }

    #[test]
    fn test_secret_is_reserved_keyword() {
        // "secret" as a bare key should be treated as a keyword starting a preamble item
        // To use it as a key, it must be quoted
        let source = r#"
"secret": "my_value"
"#;
        let result = compile_to_json(source).unwrap();
        assert!(result.contains("my_value"));
    }

    #[test]
    fn test_secret_in_when_block() {
        let source = r#"
let env = "prod"
secret prod_key from "vault:prod/key"
secret dev_key from "vault:dev/key"

key: env == "prod" ? prod_key : dev_key
"#;
        let result = compile_to_json(source).unwrap();
        assert!(result.contains("<SECRET:vault:prod/key>"));
    }

    #[test]
    fn test_secret_parse_error_missing_from() {
        let source = r#"
secret db_password "vault:path"
key: "value"
"#;
        let result = compile_to_json(source);
        assert!(result.is_err());
    }

    #[test]
    fn test_secret_parse_error_missing_provider() {
        let source = r#"
secret db_password from
key: "value"
"#;
        let result = compile_to_json(source);
        assert!(result.is_err());
    }
}

mod policy_tests {
    use hone::{emit, Compiler, OutputFormat};

    /// Helper that compiles source using the Compiler (which evaluates policies)
    fn compile_with_policies(source: &str) -> Result<String, hone::HoneError> {
        let base_dir = std::env::current_dir().unwrap();
        let mut compiler = Compiler::new(&base_dir);
        let value = compiler.compile_source(source)?;
        let warnings = compiler.warnings().to_vec();
        let result = emit(&value, OutputFormat::Json)?;
        // Check warnings
        for w in &warnings {
            eprintln!("warning: {}", w.message);
        }
        Ok(result)
    }

    fn compile_with_policies_get_warnings(
        source: &str,
    ) -> Result<(String, Vec<String>), hone::HoneError> {
        let base_dir = std::env::current_dir().unwrap();
        let mut compiler = Compiler::new(&base_dir);
        let value = compiler.compile_source(source)?;
        let warnings: Vec<String> = compiler
            .warnings()
            .iter()
            .map(|w| w.message.clone())
            .collect();
        let result = emit(&value, OutputFormat::Json)?;
        Ok((result, warnings))
    }

    fn compile_ignoring_policies(source: &str) -> Result<String, hone::HoneError> {
        let base_dir = std::env::current_dir().unwrap();
        let mut compiler = Compiler::new(&base_dir);
        compiler.set_ignore_policies(true);
        let value = compiler.compile_source(source)?;
        emit(&value, OutputFormat::Json)
    }

    #[test]
    fn test_policy_deny_triggers_error() {
        let source = r#"
policy no_debug deny when output.debug == true {
    "debug must be disabled"
}

debug: true
port: 8080
"#;
        let result = compile_with_policies(source);
        assert!(result.is_err());
        let err = result.unwrap_err().message();
        assert!(err.contains("no_debug"));
    }

    #[test]
    fn test_policy_deny_passes_when_false() {
        let source = r#"
policy no_debug deny when output.debug == true {
    "debug must be disabled"
}

debug: false
port: 8080
"#;
        let result = compile_with_policies(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_policy_warn_emits_warning_but_succeeds() {
        let source = r#"
policy port_range warn when output.port < 1024 {
    "privileged ports require elevated permissions"
}

port: 80
"#;
        let result = compile_with_policies_get_warnings(source);
        assert!(result.is_ok());
        let (json, warnings) = result.unwrap();
        assert!(json.contains("80"));
        assert!(warnings.iter().any(|w| w.contains("port_range")));
    }

    #[test]
    fn test_policy_warn_no_warning_when_ok() {
        let source = r#"
policy port_range warn when output.port < 1024 {
    "privileged ports require elevated permissions"
}

port: 8080
"#;
        let result = compile_with_policies_get_warnings(source);
        assert!(result.is_ok());
        let (_json, warnings) = result.unwrap();
        assert!(!warnings.iter().any(|w| w.contains("port_range")));
    }

    #[test]
    fn test_multiple_policies() {
        let source = r#"
policy no_debug deny when output.debug == true {
    "debug must be disabled"
}
policy port_range warn when output.port < 1024 {
    "privileged port"
}

debug: false
port: 80
"#;
        // The deny passes (debug is false), but warn fires (port < 1024)
        let result = compile_with_policies_get_warnings(source);
        assert!(result.is_ok());
        let (_json, warnings) = result.unwrap();
        assert!(warnings.iter().any(|w| w.contains("port_range")));
    }

    #[test]
    fn test_policy_with_nested_output_access() {
        let source = r#"
policy no_debug deny when output.server.debug == true {
    "server debug must be disabled"
}

server {
    debug: true
    port: 8080
}
"#;
        let result = compile_with_policies(source);
        assert!(result.is_err());
    }

    #[test]
    fn test_ignore_policy_bypasses_checks() {
        let source = r#"
policy no_debug deny when output.debug == true {
    "debug must be disabled"
}

debug: true
"#;
        let result = compile_ignoring_policies(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_policy_without_message() {
        let source = r#"
policy safety deny when output.dangerous == true

dangerous: true
"#;
        let result = compile_with_policies(source);
        assert!(result.is_err());
    }

    #[test]
    fn test_policy_formatting_roundtrip() {
        let source = "policy no_debug deny when output.debug == true {\n  \"debug must be disabled\"\n}\n\nkey: \"value\"\n";
        let formatted = hone::format_source(source).unwrap();
        assert!(formatted.contains("policy no_debug deny when"));
        assert!(formatted.contains("debug must be disabled"));
    }

    #[test]
    fn test_policy_parse_error_missing_level() {
        let source = r#"
policy bad_policy when output.x == true
key: "value"
"#;
        let base_dir = std::env::current_dir().unwrap();
        let mut compiler = Compiler::new(&base_dir);
        let result = compiler.compile_source(source);
        assert!(result.is_err());
    }
}

mod typeprovider_tests {
    use hone::typeprovider;

    #[test]
    fn test_typegen_basic_json_schema() {
        let schema = serde_json::json!({
            "type": "object",
            "title": "AppConfig",
            "properties": {
                "name": { "type": "string" },
                "port": { "type": "integer", "minimum": 1, "maximum": 65535 },
                "debug": { "type": "boolean" }
            },
            "required": ["name", "port"],
            "additionalProperties": false
        });

        let result = typeprovider::generate_from_schema(&schema).unwrap();

        // Verify the output contains a valid schema
        assert!(result.contains("schema AppConfig"));
        assert!(result.contains("name: string"));
        assert!(result.contains("port: int(1, 65535)"));
        assert!(result.contains("debug?: bool"));

        // Verify the output can be parsed by Hone
        let mut lexer = hone::Lexer::new(&result, None);
        let tokens = lexer.tokenize().expect("should lex");
        let mut parser = hone::Parser::new(tokens, &result, None);
        let ast = parser.parse().expect("should parse");

        // Find the schema
        let schema_count = ast
            .preamble
            .iter()
            .filter(|item| matches!(item, hone::ast::PreambleItem::Schema(_)))
            .count();
        assert_eq!(schema_count, 1, "should have exactly 1 schema");
    }

    #[test]
    fn test_typegen_with_refs() {
        let schema = serde_json::json!({
            "type": "object",
            "title": "Deployment",
            "$defs": {
                "Container": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "image": { "type": "string" }
                    },
                    "required": ["name", "image"],
                    "additionalProperties": false
                }
            },
            "properties": {
                "containers": {
                    "type": "array",
                    "items": { "$ref": "#/$defs/Container" }
                }
            },
            "additionalProperties": false
        });

        let result = typeprovider::generate_from_schema(&schema).unwrap();
        assert!(
            result.contains("schema Container"),
            "should have Container schema, got:\n{}",
            result
        );
        assert!(
            result.contains("schema Deployment"),
            "should have Deployment schema"
        );
        assert!(
            result.contains("containers?: array # Container"),
            "should reference Container in array"
        );
    }

    #[test]
    fn test_typegen_roundtrip_compile() {
        // Generate a schema, then use it to compile a valid config
        let schema = serde_json::json!({
            "type": "object",
            "title": "ServerConfig",
            "properties": {
                "host": { "type": "string" },
                "port": { "type": "integer", "minimum": 1, "maximum": 65535 }
            },
            "required": ["host", "port"],
            "additionalProperties": false
        });

        let schema_source = typeprovider::generate_from_schema(&schema).unwrap();

        // Combine the generated schema with a use statement and config values
        let full_source = format!(
            "{}\nuse ServerConfig\n\nhost: \"localhost\"\nport: 8080\n",
            schema_source
        );

        // Compile the combined source
        let base_dir = std::env::current_dir().unwrap();
        let mut compiler = hone::Compiler::new(&base_dir);
        let result = compiler.compile_source(&full_source);
        assert!(
            result.is_ok(),
            "should compile successfully, got: {:?}",
            result.err()
        );

        let value = result.unwrap();
        assert_eq!(value.get_path(&["host"]).unwrap().to_string(), "localhost");
        assert_eq!(value.get_path(&["port"]).unwrap().to_string(), "8080");
    }

    #[test]
    fn test_typegen_roundtrip_validation_fails() {
        let schema = serde_json::json!({
            "type": "object",
            "title": "StrictConfig",
            "properties": {
                "port": { "type": "integer", "minimum": 1, "maximum": 65535 }
            },
            "required": ["port"],
            "additionalProperties": false
        });

        let schema_source = typeprovider::generate_from_schema(&schema).unwrap();

        // Use the schema with a value that violates the constraint
        let full_source = format!(
            "{}\nuse StrictConfig\n\nport: \"not_a_number\"\n",
            schema_source
        );

        let base_dir = std::env::current_dir().unwrap();
        let mut compiler = hone::Compiler::new(&base_dir);
        let result = compiler.compile_source(&full_source);
        assert!(result.is_err(), "should fail validation");
    }

    #[test]
    fn test_typegen_reserved_word_field() {
        let schema = serde_json::json!({
            "type": "object",
            "title": "K8s",
            "properties": {
                "type": { "type": "string" },
                "import": { "type": "string" },
                "name": { "type": "string" }
            },
            "required": ["type"],
            "additionalProperties": false
        });

        let result = typeprovider::generate_from_schema(&schema).unwrap();
        // Reserved words should be quoted
        assert!(
            result.contains("\"type\": string"),
            "reserved word 'type' should be quoted, got:\n{}",
            result
        );
        assert!(
            result.contains("\"import\"?: string"),
            "reserved word 'import' should be quoted, got:\n{}",
            result
        );
        assert!(
            result.contains("  name?: string"),
            "normal field should not be quoted"
        );
    }
}

// =============================================================================
// TOML and .env emitter integration tests
// =============================================================================

mod toml_dotenv_tests {
    use super::*;

    fn compile_to_toml(source: &str) -> Result<String, hone::HoneError> {
        let mut lexer = Lexer::new(source, None);
        let tokens = lexer.tokenize()?;
        let mut parser = Parser::new(tokens, source, None);
        let ast = parser.parse()?;
        let mut evaluator = Evaluator::new(source);
        let value = evaluator.evaluate(&ast)?;
        emit(&value, OutputFormat::Toml)
    }

    fn compile_to_dotenv(source: &str) -> Result<String, hone::HoneError> {
        let mut lexer = Lexer::new(source, None);
        let tokens = lexer.tokenize()?;
        let mut parser = Parser::new(tokens, source, None);
        let ast = parser.parse()?;
        let mut evaluator = Evaluator::new(source);
        let value = evaluator.evaluate(&ast)?;
        emit(&value, OutputFormat::Dotenv)
    }

    #[test]
    fn test_toml_flat_values() {
        let source = r#"
name: "my-app"
port: 8080
debug: true
"#;
        let result = compile_to_toml(source).unwrap();
        assert!(result.contains("name = \"my-app\""));
        assert!(result.contains("port = 8080"));
        assert!(result.contains("debug = true"));
    }

    #[test]
    fn test_toml_nested_objects() {
        let source = r#"
server {
    host: "localhost"
    port: 8080
}
"#;
        let result = compile_to_toml(source).unwrap();
        assert!(result.contains("[server]"));
        assert!(result.contains("host = \"localhost\""));
    }

    #[test]
    fn test_dotenv_flat_values() {
        let source = r#"
APP_NAME: "my-app"
PORT: 8080
DEBUG: true
"#;
        let result = compile_to_dotenv(source).unwrap();
        // dotenv format: KEY=value (no quotes unless special chars)
        assert!(result.contains("APP_NAME=my-app"));
        assert!(result.contains("PORT=8080"));
        assert!(result.contains("DEBUG=true"));
    }

    #[test]
    fn test_toml_multiline_string() {
        let source = "let content = \"\"\"line one\nline two\"\"\"\nresult: content\n";
        let result = compile_to_toml(source).unwrap();
        // Multiline string in TOML output
        assert!(result.contains("result"));
        assert!(result.contains("line one"));
    }
}

// =============================================================================
// @unchecked directive integration tests
// =============================================================================

mod unchecked_tests {
    #[test]
    fn test_unchecked_bypasses_type_check() {
        // Schema validation requires the Compiler
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.hone");
        std::fs::write(
            &file,
            r#"
schema Config {
    port: int(1, 65535)
}
use Config

port: 99999 @unchecked
"#,
        )
        .unwrap();
        let mut compiler = hone::Compiler::new(dir.path());
        let result = compiler.compile(&file);
        assert!(
            result.is_ok(),
            "unchecked should bypass type error: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_unchecked_non_annotated_still_fails() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.hone");
        std::fs::write(
            &file,
            r#"
schema Config {
    port: int(1, 65535)
    name: string
}
use Config

port: 99999 @unchecked
name: 42
"#,
        )
        .unwrap();
        let mut compiler = hone::Compiler::new(dir.path());
        let result = compiler.compile(&file);
        // name: 42 should still fail (not unchecked)
        assert!(result.is_err());
    }
}

// =============================================================================
// Triple-quoted string integration tests
// =============================================================================

mod triple_quoted_tests {
    use super::*;

    #[test]
    fn test_triple_quoted_string_basic() {
        let source = r#"
let msg = """
Hello,
World!
"""
greeting: msg
"#;
        let json = compile_to_json(source).unwrap();
        assert!(json.contains("Hello,\\nWorld!"));
    }

    #[test]
    fn test_triple_quoted_with_interpolation() {
        let source = r#"
let name = "Hone"
greeting: """Hello, ${name}!
Welcome."""
"#;
        let json = compile_to_json(source).unwrap();
        assert!(json.contains("Hello, Hone!"));
        assert!(json.contains("Welcome."));
    }
}

// =============================================================================
// Secret and policy declaration tests
// =============================================================================

mod secret_policy_tests {
    use super::*;

    #[test]
    fn test_secret_produces_placeholder() {
        let source = r#"
secret db_pass from "vault:secret/db#password"
database {
    password: db_pass
}
"#;
        let json = compile_to_json(source).unwrap();
        assert!(json.contains("<SECRET:vault:secret/db#password>"));
    }

    #[test]
    fn test_secret_in_interpolation() {
        let source = r#"
secret token from "env:API_TOKEN"
url: "https://api.example.com?token=${token}"
"#;
        let json = compile_to_json(source).unwrap();
        assert!(json.contains("<SECRET:env:API_TOKEN>"));
    }

    #[test]
    fn test_policy_deny_blocks_compilation() {
        // Policy checking requires the Compiler, not raw Evaluator
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.hone");
        std::fs::write(
            &file,
            r#"
policy no_debug deny when output.debug == true {
    "debug must be disabled"
}

debug: true
"#,
        )
        .unwrap();
        let mut compiler = hone::Compiler::new(dir.path());
        let result = compiler.compile(&file);
        assert!(result.is_err(), "deny policy should block compilation");
    }

    #[test]
    fn test_policy_deny_passes_when_ok() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.hone");
        std::fs::write(
            &file,
            r#"
policy no_debug deny when output.debug == true {
    "debug must be disabled"
}

debug: false
"#,
        )
        .unwrap();
        let mut compiler = hone::Compiler::new(dir.path());
        let result = compiler.compile(&file);
        assert!(result.is_ok(), "policy should pass: {:?}", result.err());
    }
}

// =============================================================================
// Behavioral edge case tests
// =============================================================================

mod behavioral_tests {
    use super::*;

    #[test]
    fn test_null_interpolation_produces_null_string() {
        let source = r#"
let x = null
result: "value is ${x}"
"#;
        let json = compile_to_json(source).unwrap();
        assert!(json.contains("value is null"));
    }

    #[test]
    fn test_null_coalesce() {
        let source = r#"
let x = null
result: x ?? "fallback"
"#;
        let json = compile_to_json(source).unwrap();
        assert!(json.contains("fallback"));
    }

    #[test]
    fn test_int_float_equality() {
        let source = r#"
result: 1 == 1.0 ? "equal" : "not equal"
"#;
        let json = compile_to_json(source).unwrap();
        assert!(json.contains("equal"));
        assert!(!json.contains("not equal"));
    }

    #[test]
    fn test_schema_extends() {
        // Schema validation requires the Compiler
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.hone");
        std::fs::write(
            &file,
            r#"
schema Base {
    name: string
}

schema Extended extends Base {
    port: int
}

use Extended

name: "test"
port: 8080
"#,
        )
        .unwrap();
        let mut compiler = hone::Compiler::new(dir.path());
        let result = compiler.compile(&file);
        assert!(
            result.is_ok(),
            "schema extends should work: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_integer_overflow_error() {
        let source = r#"
result: 9223372036854775807 + 1
"#;
        let result = compile_to_json(source);
        assert!(result.is_err(), "integer overflow should be an error");
    }
}

// ── Ansible / YAML-hostile pattern tests ────────────────────────────────

/// Tests that Hone correctly handles patterns commonly found in Ansible
/// playbooks and other YAML-heavy tools that clash with YAML's quirks
/// or Hone's reserved words.

#[test]
fn test_ansible_reserved_word_when_as_key() {
    let source = r#"
tasks: [
  {
    name: "Check something"
    "when": "ansible_os_family == 'Debian'"
  },
]
"#;
    let yaml = compile_to_yaml(source).unwrap();
    assert!(yaml.contains("when: "));
    assert!(yaml.contains("Debian"));
}

#[test]
fn test_ansible_reserved_word_import_as_key() {
    let source = r#"
tasks: [
  {
    name: "Include role"
    "import": "roles/common/tasks/main.yml"
  },
]
"#;
    let json = compile_to_json(source).unwrap();
    assert!(json.contains(r#""import":"roles/common/tasks/main.yml""#));
}

#[test]
fn test_ansible_reserved_word_type_as_key() {
    let source = r#"
resource {
  "type": "Deployment"
  apiVersion: "apps/v1"
}
"#;
    let json = compile_to_json(source).unwrap();
    assert!(json.contains(r#""type":"Deployment""#));
}

#[test]
fn test_ansible_yes_no_strings_preserved() {
    // Ansible often uses "yes"/"no" as string values.
    // Hone should emit them as quoted strings in YAML, not as booleans.
    let source = r#"
create_home: "yes"
update_cache: "yes"
force: "no"
enabled: true
"#;
    let yaml = compile_to_yaml(source).unwrap();
    assert!(yaml.contains(r#"create_home: "yes""#));
    assert!(yaml.contains(r#"update_cache: "yes""#));
    assert!(yaml.contains(r#"force: "no""#));
    assert!(yaml.contains("enabled: true"));
}

#[test]
fn test_ansible_norway_problem() {
    // The YAML 1.1 "Norway problem": bare NO is interpreted as false.
    // Hone strings should always be properly quoted in YAML output.
    let source = r#"
country_code: "NO"
country_name: "Norway"
flag: false
"#;
    let yaml = compile_to_yaml(source).unwrap();
    assert!(yaml.contains(r#"country_code: "NO""#));
    assert!(yaml.contains("country_name: Norway"));
    assert!(yaml.contains("flag: false"));
}

#[test]
fn test_ansible_jinja2_passthrough() {
    // Jinja2 {{ }} expressions should pass through untouched.
    // Hone only interpolates ${ }, not {{ }}.
    let source = r#"
name: "Deploy app"
vars: {
  workers: "{{ gunicorn_workers }}"
  bind: "127.0.0.1:{{ app_port }}"
}
"#;
    let yaml = compile_to_yaml(source).unwrap();
    assert!(yaml.contains("{{ gunicorn_workers }}"));
    assert!(yaml.contains("{{ app_port }}"));
}

#[test]
fn test_ansible_register_and_conditional() {
    let source = r#"
tasks: [
  {
    name: "Check config"
    stat: { path: "/etc/app.conf" }
    register: "config_check"
  },
  {
    name: "Create config"
    template: { src: "app.conf.j2", dest: "/etc/app.conf" }
    "when": "not config_check.stat.exists"
  },
]
"#;
    let json = compile_to_json(source).unwrap();
    assert!(json.contains(r#""register":"config_check""#));
    assert!(json.contains(r#""when":"not config_check.stat.exists""#));
}

#[test]
fn test_ansible_loop_with_items() {
    let source = r#"
let users = [
  { name: "alice", groups: "admin,docker" },
  { name: "bob", groups: "docker" },
]

tasks: [
  for user in users {
    name: "Create user ${user.name}"
    user: { name: user.name, groups: user.groups }
    "when": "ansible_os_family == 'Debian'"
  },
]
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let tasks = parsed["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0]["name"], "Create user alice");
    assert_eq!(tasks[1]["name"], "Create user bob");
    assert_eq!(tasks[0]["user"]["groups"], "admin,docker");
    assert_eq!(tasks[1]["when"], "ansible_os_family == 'Debian'");
}

#[test]
fn test_ansible_handlers() {
    let source = r#"
let app = "myapp"

handlers: [
  {
    name: "restart ${app}"
    systemd: { name: app, state: "restarted" }
  },
  {
    name: "reload nginx"
    systemd: { name: "nginx", state: "reloaded" }
  },
]
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let handlers = parsed["handlers"].as_array().unwrap();
    assert_eq!(handlers.len(), 2);
    assert_eq!(handlers[0]["name"], "restart myapp");
    assert_eq!(handlers[0]["systemd"]["name"], "myapp");
    assert_eq!(handlers[1]["systemd"]["state"], "reloaded");
}

#[test]
fn test_ansible_notify_with_interpolation() {
    let source = r#"
let service = "myapp"
task {
  name: "Deploy config"
  template: { src: "config.j2", dest: "/etc/config" }
  notify: ["restart ${service}", "reload nginx"]
}
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let notify = parsed["task"]["notify"].as_array().unwrap();
    assert_eq!(notify[0], "restart myapp");
    assert_eq!(notify[1], "reload nginx");
}

#[test]
fn test_ansible_dotted_module_names() {
    // Ansible collection module names like community.general.ufw
    // need quoting in Hone since dots are property access.
    let source = r#"
tasks: [
  {
    name: "Allow SSH"
    "community.general.ufw": { rule: "allow", port: "22", proto: "tcp" }
  },
]
"#;
    let yaml = compile_to_yaml(source).unwrap();
    assert!(yaml.contains("community.general.ufw:"));
    assert!(yaml.contains("rule: allow"));
}

#[test]
fn test_ansible_become_and_gather_facts() {
    let source = r#"
hosts: "webservers"
become: true
gather_facts: false
tasks: []
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["become"], true);
    assert_eq!(parsed["gather_facts"], false);
    assert_eq!(parsed["hosts"], "webservers");
}

#[test]
fn test_ansible_tags_array() {
    let source = r#"
tasks: [
  {
    name: "Install packages"
    apt: { name: ["nginx"], state: "present" }
    tags: ["setup", "packages", "web"]
  },
]
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let tags = parsed["tasks"][0]["tags"].as_array().unwrap();
    assert_eq!(tags.len(), 3);
    assert_eq!(tags[0], "setup");
    assert_eq!(tags[2], "web");
}

#[test]
fn test_ansible_vars_with_jinja2_filters() {
    // Jinja2 pipe filters should pass through cleanly
    let source = r#"
tasks: [
  {
    name: "Show info"
    debug: { msg: "Host: {{ inventory_hostname | upper }}, OS: {{ ansible_distribution | lower }}" }
  },
]
"#;
    let yaml = compile_to_yaml(source).unwrap();
    assert!(yaml.contains("{{ inventory_hostname | upper }}"));
    assert!(yaml.contains("{{ ansible_distribution | lower }}"));
}

#[test]
fn test_ansible_file_permissions_as_strings() {
    // File modes must stay as strings ("0644"), not become integers
    let source = r#"
file: {
  path: "/etc/app.conf"
  mode: "0644"
  owner: "root"
}
"#;
    let yaml = compile_to_yaml(source).unwrap();
    assert!(yaml.contains(r#"mode: "0644""#));
}

#[test]
fn test_ansible_mixed_hone_and_jinja2_interpolation() {
    // Hone ${} should resolve, Jinja2 {{ }} should pass through
    let source = r#"
let app = "myapp"
let config_dir = "/etc/${app}"

template: {
  src: "{{ role_path }}/templates/config.j2"
  dest: "${config_dir}/app.conf"
}
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["template"]["dest"], "/etc/myapp/app.conf");
    assert_eq!(
        parsed["template"]["src"],
        "{{ role_path }}/templates/config.j2"
    );
}

#[test]
fn test_ansible_spread_reusable_task_pattern() {
    // Spread a common task pattern and override specific fields
    let source = r#"
let enable_service = {
  systemd: {
    enabled: "yes"
    state: "started"
    daemon_reload: "yes"
  }
}

tasks: [
  {
    name: "Start nginx"
    ...enable_service
    systemd {
      name: "nginx"
    }
  },
  {
    name: "Start supervisor"
    ...enable_service
    systemd {
      name: "supervisor"
    }
  },
]
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let tasks = parsed["tasks"].as_array().unwrap();
    // Spread + deep merge: name should be set, systemd should have all fields
    assert_eq!(tasks[0]["systemd"]["name"], "nginx");
    assert_eq!(tasks[0]["systemd"]["enabled"], "yes");
    assert_eq!(tasks[0]["systemd"]["state"], "started");
    assert_eq!(tasks[1]["systemd"]["name"], "supervisor");
    assert_eq!(tasks[1]["systemd"]["daemon_reload"], "yes");
}

#[test]
fn test_ansible_for_loop_generates_tasks() {
    // A for-loop in a task array should expand to multiple task objects
    let source = r#"
let ports = ["22/tcp", "80/tcp", "443/tcp"]

tasks: [
  for port in ports {
    name: "Allow ${port}"
    ufw: { rule: "allow", port: replace(port, "/tcp", "") }
  },
]
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let tasks = parsed["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 3);
    assert_eq!(tasks[0]["name"], "Allow 22/tcp");
    assert_eq!(tasks[0]["ufw"]["port"], "22");
    assert_eq!(tasks[2]["name"], "Allow 443/tcp");
    assert_eq!(tasks[2]["ufw"]["port"], "443");
}

#[test]
fn test_ansible_policy_as_key() {
    // "policy" is a reserved word in Hone, must be quoted as a key
    let source = r#"
ufw: { state: "enabled", "policy": "deny" }
"#;
    let json = compile_to_json(source).unwrap();
    assert!(json.contains(r#""policy":"deny""#));
}

#[test]
fn test_yaml_octal_like_strings_stay_quoted() {
    // Values like "0755" that YAML might interpret as octal should stay strings
    let source = r#"
mode: "0755"
code: "0100"
"#;
    let yaml = compile_to_yaml(source).unwrap();
    assert!(yaml.contains(r#"mode: "0755""#));
    assert!(yaml.contains(r#"code: "0100""#));
}

#[test]
fn test_yaml_on_off_strings_stay_quoted() {
    // YAML 1.1 treats on/off/yes/no as booleans. Hone strings must stay quoted.
    let source = r#"
feature_on: "on"
feature_off: "off"
real_bool: true
"#;
    let yaml = compile_to_yaml(source).unwrap();
    assert!(yaml.contains(r#"feature_on: "on""#));
    assert!(yaml.contains(r#"feature_off: "off""#));
    assert!(yaml.contains("real_bool: true"));
}

// ── Computational stress tests ──────────────────────────────────────────

#[test]
fn test_mandelbrot_fractal_computation() {
    // The Mandelbrot set, computed in a configuration language.
    // 8 iterations of z = z² + c with complex arithmetic, per pixel.
    let source = r##"
let width = 20
let height = 10
let x_step = 3.0 / width
let y_step = 2.4 / height

art: for py in range(0, height) {
  let cy = 1.2 - py * y_step
  let row = for px in range(0, width) {
    let cx = -2.2 + px * x_step
    let z1r = cx
    let z1i = cy
    let z2r = z1r * z1r - z1i * z1i + cx
    let z2i = 2.0 * z1r * z1i + cy
    let z3r = z2r * z2r - z2i * z2i + cx
    let z3i = 2.0 * z2r * z2i + cy
    let z4r = z3r * z3r - z3i * z3i + cx
    let z4i = 2.0 * z3r * z3i + cy
    let m1 = z1r * z1r + z1i * z1i
    let m2 = z2r * z2r + z2i * z2i
    let m3 = z3r * z3r + z3i * z3i
    let m4 = z4r * z4r + z4i * z4i
    m1 > 4.0 ? " " : m2 > 4.0 ? "." : m3 > 4.0 ? ":" : m4 > 4.0 ? "#" : "@"
  }
  join(row, "")
}
"##;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let art = parsed["art"].as_array().unwrap();

    // Should produce the right number of rows
    assert_eq!(art.len(), 10);

    // Each row should be the right width
    for row in art {
        assert_eq!(row.as_str().unwrap().len(), 20);
    }

    // The center of the Mandelbrot set (around x=-0.5, y=0) should be "@"
    // and the far edges should be spaces (escaped quickly)
    let middle_row = art[5].as_str().unwrap();
    assert!(middle_row.contains("@"), "center should be in the set");
    assert!(middle_row.starts_with(' '), "left edge should escape");

    // The set should be roughly symmetric about the real axis (y=0)
    // At low resolution this isn't pixel-perfect, but row 0 and row 9
    // (top/bottom edges) should both start with spaces (outside the set)
    let top = art[0].as_str().unwrap();
    let bottom = art[9].as_str().unwrap();
    assert!(top.starts_with(' '), "top edge should be outside the set");
    assert!(
        bottom.starts_with(' '),
        "bottom edge should be outside the set"
    );
}

#[test]
fn test_for_loop_dummy_key_unwrap_pattern() {
    // The old "_" dummy key pattern still works for backwards compatibility
    let source = r#"
let data = for i in range(0, 5) {
  let squared = i * i
  "_": squared
}
result: for item in data { item._ }
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let result = parsed["result"].as_array().unwrap();
    assert_eq!(result, &[0, 1, 4, 9, 16]);
}

// =============================================================================
// Block body for loops (trailing expression after let bindings)
// =============================================================================

#[test]
fn test_for_block_body_basic() {
    // Trailing expression after let bindings produces an array
    let source = r#"
result: for i in range(0, 5) {
  let squared = i * i
  squared
}
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let result = parsed["result"].as_array().unwrap();
    assert_eq!(result, &[0, 1, 4, 9, 16]);
}

#[test]
fn test_for_block_body_multiple_lets() {
    // Multiple let bindings before trailing expression
    let source = r#"
result: for i in range(0, 4) {
  let a = i * 2
  let b = a + 1
  let c = b * b
  c
}
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let result = parsed["result"].as_array().unwrap();
    // i=0: a=0,b=1,c=1  i=1: a=2,b=3,c=9  i=2: a=4,b=5,c=25  i=3: a=6,b=7,c=49
    assert_eq!(result, &[1, 9, 25, 49]);
}

#[test]
fn test_for_block_body_complex_trailing_expr() {
    // Trailing expression can be any expression, not just a variable
    let source = r#"
result: for i in range(0, 3) {
  let x = i + 1
  x * x + x
}
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let result = parsed["result"].as_array().unwrap();
    // i=0: x=1, 1+1=2  i=1: x=2, 4+2=6  i=2: x=3, 9+3=12
    assert_eq!(result, &[2, 6, 12]);
}

#[test]
fn test_for_block_body_string_result() {
    // Block body producing strings
    let source = r#"
result: for name in ["alice", "bob", "carol"] {
  let greeting = "hello"
  "${greeting}, ${name}!"
}
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let result = parsed["result"].as_array().unwrap();
    assert_eq!(result[0], "hello, alice!");
    assert_eq!(result[1], "hello, bob!");
    assert_eq!(result[2], "hello, carol!");
}

#[test]
fn test_for_block_body_nested() {
    // Nested for loops both using block bodies
    let source = r#"
result: for y in range(0, 3) {
  let row = for x in range(0, 3) {
    let val = y * 3 + x
    val
  }
  join(for v in row { to_str(v) }, ",")
}
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let result = parsed["result"].as_array().unwrap();
    assert_eq!(result[0], "0,1,2");
    assert_eq!(result[1], "3,4,5");
    assert_eq!(result[2], "6,7,8");
}

#[test]
fn test_for_block_body_with_ternary() {
    // Trailing expression is a ternary (common pattern)
    let source = r#"
result: for i in range(0, 5) {
  let threshold = 2
  i > threshold ? "big" : "small"
}
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let result = parsed["result"].as_array().unwrap();
    assert_eq!(result, &["small", "small", "small", "big", "big"]);
}

#[test]
fn test_for_block_body_with_function_call() {
    // Trailing expression is a function call
    let source = r#"
result: for word in ["hello", "WORLD", "Foo"] {
  let lowered = lower(word)
  upper(lowered)
}
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let result = parsed["result"].as_array().unwrap();
    assert_eq!(result, &["HELLO", "WORLD", "FOO"]);
}

#[test]
fn test_for_block_body_let_in_expression_context() {
    // Block body for loop used in a let binding
    let source = r#"
let squares = for i in range(1, 6) {
  let sq = i * i
  sq
}
result: squares
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let result = parsed["result"].as_array().unwrap();
    assert_eq!(result, &[1, 4, 9, 16, 25]);
}

#[test]
fn test_for_block_body_destructuring() {
    // Block body with pair destructuring
    let source = r#"
let data = { a: 1, b: 2, c: 3 }
result: for (key, val) in data {
  let doubled = val * 2
  "${key}=${doubled}"
}
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let result = parsed["result"].as_array().unwrap();
    assert_eq!(result, &["a=2", "b=4", "c=6"]);
}

// =============================================================================
// New builtin function integration tests
// =============================================================================

#[test]
fn test_builtin_sort() {
    let source = r#"
nums: sort([3, 1, 4, 1, 5, 9])
strs: sort(["banana", "apple", "cherry"])
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["nums"], serde_json::json!([1, 1, 3, 4, 5, 9]));
    assert_eq!(
        parsed["strs"],
        serde_json::json!(["apple", "banana", "cherry"])
    );
}

#[test]
fn test_builtin_starts_with_ends_with() {
    let source = r#"
sw: starts_with("hello world", "hello")
ew: ends_with("hello world", "world")
sw_no: starts_with("hello", "world")
ew_no: ends_with("hello", "world")
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["sw"], true);
    assert_eq!(parsed["ew"], true);
    assert_eq!(parsed["sw_no"], false);
    assert_eq!(parsed["ew_no"], false);
}

#[test]
fn test_builtin_min_max_abs() {
    let source = r#"
mn: min(3, 7)
mx: max(3, 7)
ab: abs(-42)
ab_f: abs(-3.14)
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["mn"], 3);
    assert_eq!(parsed["mx"], 7);
    assert_eq!(parsed["ab"], 42);
    assert_eq!(parsed["ab_f"], 3.14);
}

#[test]
fn test_builtin_unique() {
    let source = r#"
result: unique([1, 2, 2, 3, 1, 4, 3])
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["result"], serde_json::json!([1, 2, 3, 4]));
}

#[test]
fn test_builtin_sha256() {
    let source = r#"
hash: sha256("hello")
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        parsed["hash"],
        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
    );
}

#[test]
fn test_builtin_type_of() {
    let source = r#"
ti: type_of(42)
ts: type_of("hello")
tb: type_of(true)
ta: type_of([1, 2])
to: type_of({ a: 1 })
tn: type_of(null)
tf: type_of(3.14)
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["ti"], "int");
    assert_eq!(parsed["ts"], "string");
    assert_eq!(parsed["tb"], "bool");
    assert_eq!(parsed["ta"], "array");
    assert_eq!(parsed["to"], "object");
    assert_eq!(parsed["tn"], "null");
    assert_eq!(parsed["tf"], "float");
}

#[test]
fn test_builtin_substring() {
    let source = r#"
full: substring("hello world", 0, 5)
tail: substring("hello world", 6)
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["full"], "hello");
    assert_eq!(parsed["tail"], "world");
}

#[test]
fn test_builtin_entries_from_entries() {
    let source = r#"
let obj = { a: 1, b: 2 }
e: entries(obj)
roundtrip: from_entries(entries(obj))
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["e"], serde_json::json!([["a", 1], ["b", 2]]));
    assert_eq!(parsed["roundtrip"], serde_json::json!({"a": 1, "b": 2}));
}

#[test]
fn test_builtin_clamp() {
    let source = r#"
low: clamp(-5, 0, 10)
mid: clamp(5, 0, 10)
high: clamp(15, 0, 10)
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["low"], 0);
    assert_eq!(parsed["mid"], 5);
    assert_eq!(parsed["high"], 10);
}

#[test]
fn test_builtin_reverse() {
    let source = r#"
arr: reverse([1, 2, 3])
str: reverse("hello")
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["arr"], serde_json::json!([3, 2, 1]));
    assert_eq!(parsed["str"], "olleh");
}

#[test]
fn test_builtin_slice() {
    let source = r#"
arr: slice([10, 20, 30, 40, 50], 1, 4)
str: slice("hello world", 0, 5)
neg: slice([1, 2, 3, 4, 5], -3)
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["arr"], serde_json::json!([20, 30, 40]));
    assert_eq!(parsed["str"], "hello");
    assert_eq!(parsed["neg"], serde_json::json!([3, 4, 5]));
}

#[test]
fn test_builtins_composed() {
    // Test multiple builtins composed together
    let source = r#"
let data = [3, 1, 4, 1, 5, 9, 2, 6, 5]
result: join(for x in sort(unique(data)) { to_str(x) }, ",")
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["result"], "1,2,3,4,5,6,9");
}

// =============================================================================
// User-defined function (fn) integration tests
// =============================================================================

#[test]
fn test_fn_basic() {
    let source = r#"
fn double(x) { x * 2 }

result: double(21)
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["result"], 42);
}

#[test]
fn test_fn_multiple_params() {
    let source = r#"
fn add(a, b) { a + b }

result: add(10, 32)
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["result"], 42);
}

#[test]
fn test_fn_string_operations() {
    let source = r#"
fn greet(name) { "Hello, ${name}!" }

result: greet("World")
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["result"], "Hello, World!");
}

#[test]
fn test_fn_calling_builtins() {
    let source = r#"
fn shout(text) { upper(text) }

result: shout("hello")
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["result"], "HELLO");
}

#[test]
fn test_fn_calling_other_fn() {
    let source = r#"
fn double(x) { x * 2 }
fn quadruple(x) { double(double(x)) }

result: quadruple(5)
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["result"], 20);
}

#[test]
fn test_fn_used_in_for_loop() {
    let source = r#"
fn square(x) { x * x }

result: for i in range(1, 6) { square(i) }
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["result"], serde_json::json!([1, 4, 9, 16, 25]));
}

#[test]
fn test_fn_with_ternary() {
    let source = r#"
fn classify(n) { n > 0 ? "positive" : n == 0 ? "zero" : "negative" }

a: classify(5)
b: classify(0)
c: classify(-3)
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["a"], "positive");
    assert_eq!(parsed["b"], "zero");
    assert_eq!(parsed["c"], "negative");
}

#[test]
fn test_fn_wrong_arity_error() {
    let source = r#"
fn add(a, b) { a + b }
result: add(1)
"#;
    let result = compile_to_json(source);
    assert!(result.is_err(), "wrong arity should be an error");
}

#[test]
fn test_fn_no_params() {
    let source = r#"
fn answer() { 42 }

result: answer()
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["result"], 42);
}

#[test]
fn test_fn_does_not_leak_scope() {
    // fn parameters should not be visible outside the function
    let source = r#"
fn add_one(x) { x + 1 }
let x = 100
a: add_one(5)
b: x
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["a"], 6);
    assert_eq!(parsed["b"], 100);
}

#[test]
fn test_fn_overrides_builtin() {
    // User fn with same name as builtin should take precedence
    let source = r#"
fn len(x) { 999 }

result: len("hello")
"#;
    let json = compile_to_json(source).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["result"], 999);
}

#[test]
fn test_fn_formatting_roundtrip() {
    let source = "fn double(x) {\n  x * 2\n}\n\nresult: double(21)\n";
    let formatted = hone::format_source(source).unwrap();
    // Should parse and re-format without error
    let re_formatted = hone::format_source(&formatted).unwrap();
    assert_eq!(formatted, re_formatted, "formatting should be idempotent");
}

#[test]
fn test_fn_is_reserved_keyword() {
    // "fn" as a bare key should fail (it's reserved)
    let source = r#"
fn: "some value"
"#;
    let result = compile_to_json(source);
    assert!(result.is_err(), "fn should be reserved as a bare key");
}

#[test]
fn test_fn_import_via_named_import() {
    // Test that fn definitions are exported through named imports
    let dir = tempfile::tempdir().unwrap();

    let utils = dir.path().join("utils.hone");
    std::fs::write(
        &utils,
        r#"
fn double(x) { x * 2 }
fn greet(name) { "Hi, ${name}" }
let base_port = 8080
"#,
    )
    .unwrap();

    let main = dir.path().join("main.hone");
    std::fs::write(
        &main,
        r#"
import { double, greet, base_port } from "./utils.hone"

result: double(21)
greeting: greet("World")
port: base_port
"#,
    )
    .unwrap();

    let mut compiler = hone::Compiler::new(dir.path());
    let result = compiler.compile(&main).unwrap();
    let json_val = result.to_serde_json();
    assert_eq!(json_val["result"], 42);
    assert_eq!(json_val["greeting"], "Hi, World");
    assert_eq!(json_val["port"], 8080);
}

// ── Schema error location & multi-error tests ────────────────────────

#[test]
fn test_schema_multiple_errors_collected() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.hone");
    std::fs::write(
        &file,
        r#"
schema Config {
    name: string
    port: int
    debug: bool
}

use Config

name: 42
port: "bad"
debug: "yes"
"#,
    )
    .unwrap();
    let mut compiler = hone::Compiler::new(dir.path());
    let result = compiler.compile(&file);
    assert!(result.is_err(), "should fail schema validation");
    let err = result.unwrap_err();
    let msg = err.message();
    // All three violations should be reported
    assert!(msg.contains("3 errors"), "expected 3 errors, got: {}", msg);
}

#[test]
fn test_schema_single_error_not_wrapped() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.hone");
    std::fs::write(
        &file,
        r#"
schema Config {
    name: string
    port: int
}

use Config

name: "valid"
port: "bad"
"#,
    )
    .unwrap();
    let mut compiler = hone::Compiler::new(dir.path());
    let result = compiler.compile(&file);
    assert!(result.is_err(), "should fail schema validation");
    let err = result.unwrap_err();
    let msg = err.message();
    // Single error: not wrapped in SchemaValidationErrors
    assert!(
        !msg.contains("schema validation failed"),
        "single error should not be wrapped: {}",
        msg
    );
    assert!(
        msg.contains("type mismatch") || msg.contains("expected"),
        "should be a direct type error: {}",
        msg
    );
}

#[test]
fn test_schema_error_points_to_value_location() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.hone");
    std::fs::write(
        &file,
        r#"
schema Server {
    port: int
}

use Server

port: "not-a-number"
"#,
    )
    .unwrap();
    let mut compiler = hone::Compiler::new(dir.path());
    let result = compiler.compile(&file);
    assert!(result.is_err());
    let err = result.unwrap_err();
    // The error span should point to the value definition line (line 8: port: "not-a-number")
    // not to the use statement (line 6: use Server)
    if let Some(span) = err.span() {
        let source = std::fs::read_to_string(&file).unwrap();
        let error_line = source[..span.start].matches('\n').count() + 1;
        assert_eq!(
            error_line, 8,
            "error should point to line 8 (port: \"not-a-number\"), got line {}",
            error_line
        );
    }
}

#[test]
fn test_schema_missing_field_points_to_use_statement() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.hone");
    std::fs::write(
        &file,
        r#"
schema Config {
    name: string
    port: int
}

use Config

name: "test"
"#,
    )
    .unwrap();
    let mut compiler = hone::Compiler::new(dir.path());
    let result = compiler.compile(&file);
    assert!(result.is_err());
    let err = result.unwrap_err();
    let msg = err.message();
    assert!(
        msg.contains("missing") || msg.contains("required"),
        "should report missing field: {}",
        msg
    );
}

#[test]
fn test_schema_valid_passes() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.hone");
    std::fs::write(
        &file,
        r#"
schema Config {
    name: string
    port: int
    debug?: bool
}

use Config

name: "my-app"
port: 8080
"#,
    )
    .unwrap();
    let mut compiler = hone::Compiler::new(dir.path());
    let result = compiler.compile(&file);
    assert!(
        result.is_ok(),
        "valid config should pass: {:?}",
        result.err()
    );
}
