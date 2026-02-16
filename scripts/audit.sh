#!/usr/bin/env bash
set -euo pipefail

# Hone launch audit script
# Checks that everything is in order before a release.

FAIL=0
PASS=0
WARN=0

pass() { echo "  PASS  $1"; PASS=$((PASS + 1)); }
fail() { echo "  FAIL  $1"; FAIL=$((FAIL + 1)); }
warn() { echo "  WARN  $1"; WARN=$((WARN + 1)); }

echo "=== Hone Launch Audit ==="
echo ""

# 1. Build
echo "--- Build ---"
if cargo build --release 2>&1 | tail -1; then
  pass "cargo build --release"
else
  fail "cargo build --release"
fi

# 2. Tests
echo ""
echo "--- Tests ---"
test_output=$(cargo test 2>&1)
test_count=$(echo "$test_output" | grep "^test result:" | awk '{sum += $4} END {print sum}')
if echo "$test_output" | grep -q "FAILED"; then
  fail "cargo test ($test_count tests)"
else
  pass "cargo test ($test_count tests)"
fi

# 3. Clippy
echo ""
echo "--- Lint ---"
if cargo clippy -- -D warnings 2>&1 | tail -1 | grep -q "Finished"; then
  pass "cargo clippy -- -D warnings"
else
  fail "cargo clippy -- -D warnings"
fi

# 4. Format
if cargo fmt -- --check 2>&1; then
  pass "cargo fmt -- --check"
else
  fail "cargo fmt -- --check"
fi

# 5. Examples
echo ""
echo "--- Examples ---"
for f in examples/*.hone; do
  if ./target/release/hone compile "$f" --format yaml > /dev/null 2>&1; then
    pass "$f"
  else
    fail "$f"
  fi
done
for dir in microservices ci-pipeline; do
  if [ -f "examples/$dir/main.hone" ]; then
    tmpdir=$(mktemp -d)
    if ./target/release/hone compile "examples/$dir/main.hone" --format yaml --output-dir "$tmpdir" > /dev/null 2>&1 || \
       ./target/release/hone compile "examples/$dir/main.hone" --format yaml > /dev/null 2>&1; then
      pass "examples/$dir/main.hone"
    else
      fail "examples/$dir/main.hone"
    fi
    rm -rf "$tmpdir"
  fi
done

# 6. WASM build
echo ""
echo "--- WASM ---"
if cargo build --release --target wasm32-unknown-unknown -p hone-wasm 2>&1 | tail -1; then
  pass "WASM build"
else
  warn "WASM build (may need: rustup target add wasm32-unknown-unknown)"
fi

# 7. Required files
echo ""
echo "--- Required Files ---"
for f in README.md CONTRIBUTING.md CLAUDE.md DESIGN.md LICENSE Cargo.toml; do
  if [ -f "$f" ]; then
    pass "$f exists"
  else
    fail "$f missing"
  fi
done

# 8. Documentation
echo ""
echo "--- Documentation ---"
for f in docs/getting-started.md docs/cli-reference.md docs/language-reference.md docs/editor-setup.md docs/errors.md; do
  if [ -f "$f" ]; then
    pass "$f"
  else
    fail "$f"
  fi
done
for f in docs/advanced/secrets.md docs/advanced/policies.md docs/advanced/cache.md docs/advanced/typegen.md; do
  if [ -f "$f" ]; then
    pass "$f"
  else
    fail "$f"
  fi
done

# 9. CI workflows
echo ""
echo "--- CI ---"
for f in .github/workflows/ci.yml .github/workflows/release.yml; do
  if [ -f "$f" ]; then
    pass "$f"
  else
    fail "$f"
  fi
done

# 10. Scripts
echo ""
echo "--- Scripts ---"
for f in scripts/install.sh scripts/verify-examples.sh scripts/audit.sh; do
  if [ -f "$f" ] && [ -x "$f" ]; then
    pass "$f (executable)"
  elif [ -f "$f" ]; then
    warn "$f (not executable)"
  else
    fail "$f missing"
  fi
done

# 11. Editor extension
echo ""
echo "--- Editor Extension ---"
if [ -f editors/vscode/package.json ]; then
  pass "editors/vscode/package.json"
  publisher=$(grep '"publisher"' editors/vscode/package.json | head -1 | sed 's/.*: *"\([^"]*\)".*/\1/')
  if [ "$publisher" = "honelang" ]; then
    pass "publisher is honelang"
  else
    warn "publisher is '$publisher' (expected 'honelang')"
  fi
else
  fail "editors/vscode/package.json missing"
fi

# 12. Playground
echo ""
echo "--- Playground ---"
if [ -f playground/index.html ]; then
  pass "playground/index.html"
else
  fail "playground/index.html missing"
fi
if [ -f playground/pkg/hone_wasm_bg.wasm ]; then
  pass "playground/pkg/hone_wasm_bg.wasm"
else
  warn "playground/pkg/hone_wasm_bg.wasm (needs wasm-pack build)"
fi
if [ -f playground/README.md ]; then
  pass "playground/README.md"
else
  fail "playground/README.md missing"
fi

# 13. Version consistency
echo ""
echo "--- Version ---"
cargo_version=$(grep '^version' Cargo.toml | head -1 | sed 's/.*= *"\([^"]*\)".*/\1/')
pass "Cargo.toml version: $cargo_version"
if ./target/release/hone --version 2>&1 | grep -q "$cargo_version"; then
  pass "binary --version matches Cargo.toml"
else
  warn "binary --version may not match Cargo.toml"
fi

# Summary
echo ""
echo "=========================================="
echo "  PASS: $PASS"
echo "  WARN: $WARN"
echo "  FAIL: $FAIL"
echo "=========================================="

if [ "$FAIL" -gt 0 ]; then
  echo ""
  echo "Audit FAILED. Fix the issues above before release."
  exit 1
else
  echo ""
  echo "Audit PASSED."
fi
