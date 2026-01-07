#!/usr/bin/env bash
set -euo pipefail

REPO="madhavajay/ralph"
BIN_NAME="ralph"

fail() {
  echo "Error: $1" >&2
  echo "If no prebuilt binary is available, use: cargo install ralph" >&2
  exit 1
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    fail "missing required command: $1"
  fi
}

detect_os() {
  case "$(uname -s)" in
    Linux) echo "linux" ;;
    Darwin) echo "darwin" ;;
    *) fail "unsupported OS: $(uname -s)" ;;
  esac
}

detect_arch() {
  case "$(uname -m)" in
    x86_64|amd64) echo "x86_64" ;;
    arm64|aarch64) echo "aarch64" ;;
    *) fail "unsupported arch: $(uname -m)" ;;
  esac
}

install_dir() {
  if [ -w "/usr/local/bin" ]; then
    echo "/usr/local/bin"
  else
    echo "${HOME}/.local/bin"
  fi
}

main() {
  require_cmd uname
  require_cmd tar

  local os arch target asset url tmp_dir archive bin_path dest_dir
  os="$(detect_os)"
  arch="$(detect_arch)"

  case "${os}" in
    linux) target="${arch}-unknown-linux-gnu" ;;
    darwin) target="${arch}-apple-darwin" ;;
  esac

  asset="${BIN_NAME}-${target}.tar.gz"
  url="https://github.com/${REPO}/releases/latest/download/${asset}"

  tmp_dir="$(mktemp -d)"
  trap 'rm -rf "${tmp_dir}"' EXIT
  archive="${tmp_dir}/${asset}"

  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "${url}" -o "${archive}"
  elif command -v wget >/dev/null 2>&1; then
    wget -qO "${archive}" "${url}"
  else
    fail "curl or wget is required to download releases"
  fi

  tar -xzf "${archive}" -C "${tmp_dir}"
  bin_path="$(find "${tmp_dir}" -type f -name "${BIN_NAME}" | head -n 1)"
  if [ -z "${bin_path}" ]; then
    fail "download succeeded but ${BIN_NAME} binary not found in archive"
  fi

  dest_dir="$(install_dir)"
  mkdir -p "${dest_dir}"
  install -m 0755 "${bin_path}" "${dest_dir}/${BIN_NAME}"

  echo "Installed ${BIN_NAME} to ${dest_dir}/${BIN_NAME}"
  if ! command -v "${BIN_NAME}" >/dev/null 2>&1; then
    echo "Note: ${dest_dir} is not on your PATH. Add it to use '${BIN_NAME}' directly."
  fi
}

main "$@"
