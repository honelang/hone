use wasm_bindgen_test::*;
use hone_wasm::*;

#[wasm_bindgen_test]
fn test_compile_json() {
    let result = compile(r#"name: "hello""#, "json", "", "");
    assert!(result.success());
    assert!(result.output().contains(r#""name":"hello""#));
    assert!(result.error().is_empty());
}

#[wasm_bindgen_test]
fn test_compile_yaml() {
    let result = compile(r#"name: "hello""#, "yaml", "", "");
    assert!(result.success());
    assert!(result.output().contains("name: hello"));
}

#[wasm_bindgen_test]
fn test_compile_error() {
    let result = compile(r#"name: undefined_var"#, "json", "", "");
    assert!(!result.success());
    assert!(!result.error().is_empty());
}

#[wasm_bindgen_test]
fn test_compile_with_variables() {
    let source = r#"
let env = "production"
let port = 8080
name: "api-${env}"
port: port
"#;
    let result = compile(source, "json", "", "");
    assert!(result.success());
    assert!(result.output().contains(r#""name":"api-production""#));
    assert!(result.output().contains(r#""port":8080"#));
}

#[wasm_bindgen_test]
fn test_compile_with_schema_validation() {
    let source = r#"
schema Server {
  host: string
  port: int(1, 65535)
}

use Server

host: "localhost"
port: 8080
"#;
    let result = compile(source, "json", "", "");
    assert!(result.success());
}

#[wasm_bindgen_test]
fn test_compile_schema_validation_failure() {
    let source = r#"
schema Server {
  host: string
  port: int(1, 65535)
}

use Server

host: "localhost"
port: 99999
"#;
    let result = compile(source, "json", "", "");
    assert!(!result.success());
    assert!(!result.error().is_empty());
}

#[wasm_bindgen_test]
fn test_compile_with_args() {
    let source = r#"
expect args.env: string
expect args.port: int = 8080
host: "api-${args.env}"
port: args.port
"#;
    let result = compile(source, "json", "", r#"{"env": "prod", "port": "9090"}"#);
    assert!(result.success());
    assert!(result.output().contains(r#""host":"api-prod""#));
    assert!(result.output().contains(r#""port":9090"#));
}

#[wasm_bindgen_test]
fn test_format_source() {
    let source = r#"name:    "hello"
port:  8080"#;
    let result = format_source(source);
    assert!(result.success());
    assert!(result.output().contains("name: \"hello\""));
    assert!(result.output().contains("port: 8080"));
}
