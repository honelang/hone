#!/usr/bin/env bash
set -euo pipefail

# Hone installer
# Usage: curl -fsSL https://raw.githubusercontent.com/honelang/hone/main/scripts/install.sh | bash

REPO="honelang/hone"
INSTALL_DIR="${HONE_INSTALL_DIR:-/usr/local/bin}"

main() {
  local os arch target version url tmpdir

  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Linux)  os="unknown-linux-gnu" ;;
    Darwin) os="apple-darwin" ;;
    *)      echo "Unsupported OS: $os" >&2; exit 1 ;;
  esac

  case "$arch" in
    x86_64|amd64) arch="x86_64" ;;
    arm64|aarch64) arch="aarch64" ;;
    *)             echo "Unsupported architecture: $arch" >&2; exit 1 ;;
  esac

  target="${arch}-${os}"

  if [ -n "${HONE_VERSION:-}" ]; then
    version="$HONE_VERSION"
  else
    version="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')"
  fi

  if [ -z "$version" ]; then
    echo "Failed to determine latest version" >&2
    exit 1
  fi

  url="https://github.com/${REPO}/releases/download/${version}/hone-${target}.tar.gz"
  echo "Downloading hone ${version} for ${target}..."

  tmpdir="$(mktemp -d)"
  trap 'rm -rf "$tmpdir"' EXIT

  curl -fsSL "$url" -o "${tmpdir}/hone.tar.gz"
  tar xzf "${tmpdir}/hone.tar.gz" -C "$tmpdir"

  if [ -w "$INSTALL_DIR" ]; then
    mv "${tmpdir}/hone" "${INSTALL_DIR}/hone"
  else
    echo "Installing to ${INSTALL_DIR} (requires sudo)..."
    sudo mv "${tmpdir}/hone" "${INSTALL_DIR}/hone"
  fi

  chmod +x "${INSTALL_DIR}/hone"
  echo "Installed hone ${version} to ${INSTALL_DIR}/hone"
  echo ""
  "${INSTALL_DIR}/hone" --version

  # Install Claude Code skill for Hone (if Claude Code is present)
  if [ -d "${HOME}/.claude" ]; then
    mkdir -p "${HOME}/.claude/skills/hone"
    if curl -fsSL "https://raw.githubusercontent.com/${REPO}/main/.claude/skills/hone/SKILL.md" -o "${HOME}/.claude/skills/hone/SKILL.md" 2>/dev/null; then
      echo "Installed Claude Code skill for Hone"
    fi
  fi
}

main "$@"
