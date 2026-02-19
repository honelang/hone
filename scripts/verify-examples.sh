#!/usr/bin/env bash
set -euo pipefail

# Verify all Hone examples compile without error.
# Usage: scripts/verify-examples.sh [path-to-hone-binary]

HONE="${1:-cargo run --release --}"
FAIL=0
PASS=0

compile() {
  local label="$1"
  shift
  if $HONE "$@" > /dev/null 2>&1; then
    echo "  PASS  $label"
    PASS=$((PASS + 1))
  else
    echo "  FAIL  $label"
    FAIL=$((FAIL + 1))
  fi
}

echo "=== Verifying Hone examples ==="
echo ""

# Single-file examples
for f in examples/*.hone; do
  compile "$f (yaml)" compile "$f" --format yaml
done

# Multi-file example
if [ -d examples/microservices ]; then
  tmpdir=$(mktemp -d)
  compile "examples/microservices/main.hone (yaml)" compile examples/microservices/main.hone --format yaml --output-dir "$tmpdir"
  rm -rf "$tmpdir"
  tmpdir=$(mktemp -d)
  compile "examples/microservices/main.hone (variant env=production)" compile examples/microservices/main.hone --format yaml --variant env=production --output-dir "$tmpdir"
  rm -rf "$tmpdir"
fi

# K8s validated examples
if [ -d examples/k8s-validated ]; then
  compile "examples/k8s-validated/deployment.hone (yaml)" compile examples/k8s-validated/deployment.hone --format yaml
  compile "examples/k8s-validated/service.hone (yaml)" compile examples/k8s-validated/service.hone --format yaml
  tmpdir=$(mktemp -d)
  compile "examples/k8s-validated/full-stack.hone (yaml, output-dir)" compile examples/k8s-validated/full-stack.hone --format yaml --output-dir "$tmpdir"
  rm -rf "$tmpdir"
fi

# Multi-file CI pipeline
if [ -d examples/ci-pipeline ]; then
  compile "examples/ci-pipeline/main.hone (yaml)" compile examples/ci-pipeline/main.hone --format yaml
  compile "examples/ci-pipeline/main.hone (variant deploy=production)" compile examples/ci-pipeline/main.hone --format yaml --variant deploy=production
fi

# Variant compilations
compile "app-config.hone (variant env=staging)" compile examples/app-config.hone --format yaml --variant env=staging
compile "app-config.hone (variant env=production)" compile examples/app-config.hone --format yaml --variant env=production
compile "app-config.hone (--set db_host=db.prod)" compile examples/app-config.hone --format yaml --variant env=production --set db_host=db.prod.internal
compile "kubernetes.hone (--set env=production)" compile examples/kubernetes.hone --format yaml --set env=production
compile "docker-compose.hone (--set env=production)" compile examples/docker-compose.hone --format yaml --set env=production

# TOML and dotenv format checks
compile "hello.hone (json)" compile examples/hello.hone --format json
compile "hello.hone (toml)" compile examples/hello.hone --format toml
compile "hello.hone (dotenv)" compile examples/hello.hone --format dotenv

# Stdin compilation
echo 'name: "stdin-test"' | compile "stdin (yaml)" compile - --format yaml

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="

if [ "$FAIL" -gt 0 ]; then
  exit 1
fi
