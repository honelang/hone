use std::io::Write;
use std::process::{Command, Stdio};

fn hone_binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_hone"))
}

fn write_temp_hone(content: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::Builder::new()
        .suffix(".hone")
        .tempfile()
        .expect("create temp file");
    f.write_all(content.as_bytes()).expect("write temp file");
    f
}

#[test]
fn test_cli_error_output_formatted() {
    let f = write_temp_hone("let x = undefined_var\nname: x\n");
    let output = hone_binary()
        .args(["compile", f.path().to_str().unwrap()])
        .output()
        .expect("run hone");

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Must contain miette-formatted output, not raw Debug
    assert!(
        stderr.contains("undefined variable"),
        "expected 'undefined variable' in stderr, got: {}",
        stderr
    );
    assert!(
        !stderr.contains("UndefinedVariable {"),
        "stderr contains raw Debug output: {}",
        stderr
    );
    assert!(
        stderr.contains("E0002"),
        "expected error code E0002 in stderr, got: {}",
        stderr
    );
    assert!(!output.status.success());
}

#[test]
fn test_cli_check_error_formatted() {
    let f = write_temp_hone("name: unknown_var\n");
    let output = hone_binary()
        .args(["check", f.path().to_str().unwrap()])
        .output()
        .expect("run hone");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("UndefinedVariable {"),
        "stderr contains raw Debug output: {}",
        stderr
    );
}

#[test]
fn test_cli_compile_success_no_error_output() {
    let f = write_temp_hone("name: \"hello\"\n");
    let output = hone_binary()
        .args(["compile", f.path().to_str().unwrap()])
        .output()
        .expect("run hone");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"name\""));
}

// --- Stdin support tests ---

fn run_stdin(args: &[&str], input: &str) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_hone"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn hone");

    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .expect("write stdin");

    child.wait_with_output().expect("wait for hone")
}

#[test]
fn test_stdin_compile_basic() {
    let output = run_stdin(&["compile", "-"], "name: \"hello\"\n");
    assert!(output.status.success(), "stdin compile should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"name\""), "stdout: {}", stdout);
    assert!(stdout.contains("\"hello\""), "stdout: {}", stdout);
}

#[test]
fn test_stdin_compile_with_set() {
    let output = run_stdin(
        &["compile", "-", "--set", "env=prod"],
        "environment: args.env\n",
    );
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"prod\""), "stdout: {}", stdout);
}

#[test]
fn test_stdin_compile_yaml_output() {
    let output = run_stdin(
        &["compile", "-", "--format", "yaml"],
        "name: \"hello\"\nport: 8080\n",
    );
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("name: hello"), "stdout: {}", stdout);
    assert!(stdout.contains("port: 8080"), "stdout: {}", stdout);
}

#[test]
fn test_stdin_compile_json_output() {
    let output = run_stdin(&["compile", "-", "--format", "json"], "count: 42\n");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"count\""), "stdout: {}", stdout);
    assert!(stdout.contains("42"), "stdout: {}", stdout);
}

#[test]
fn test_stdin_check_mode() {
    let output = run_stdin(&["check", "-"], "name: \"valid\"\n");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("OK"), "stderr: {}", stderr);
}

#[test]
fn test_stdin_check_mode_error() {
    let output = run_stdin(&["check", "-"], "name: undefined_var\n");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("undefined variable"), "stderr: {}", stderr);
}
