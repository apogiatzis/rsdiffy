#!/bin/sh
set -eu

REPO="apogiatzis/rsdiffy"
BINARY="rsdiffy"

main() {
    need_cmd curl
    need_cmd tar

    os="$(detect_os)"
    arch="$(detect_arch)"
    target="$(build_target "$os" "$arch")"

    printf "Detected platform: %s\n" "$target"

    tag="$(fetch_latest_tag)"
    printf "Latest release: %s\n" "$tag"

    url="https://github.com/${REPO}/releases/download/${tag}/${BINARY}-${target}.tar.gz"

    tmpdir="$(mktemp -d)"
    trap 'rm -rf "$tmpdir"' EXIT

    printf "Downloading %s...\n" "$url"
    curl -fsSL "$url" -o "${tmpdir}/${BINARY}.tar.gz"
    tar xzf "${tmpdir}/${BINARY}.tar.gz" -C "$tmpdir"

    install_dir="$(find_install_dir)"
    mv "${tmpdir}/${BINARY}" "${install_dir}/${BINARY}"
    chmod +x "${install_dir}/${BINARY}"

    printf "\nInstalled %s to %s/%s\n" "$tag" "$install_dir" "$BINARY"

    if ! echo "$PATH" | tr ':' '\n' | grep -qx "$install_dir"; then
        printf "\n  NOTE: %s is not in your PATH.\n" "$install_dir"
        printf "  Add it with: export PATH=\"%s:\$PATH\"\n" "$install_dir"
    fi
}

detect_os() {
    case "$(uname -s)" in
        Linux*)  echo "linux" ;;
        Darwin*) echo "darwin" ;;
        *)       err "Unsupported OS: $(uname -s). Download manually from https://github.com/${REPO}/releases" ;;
    esac
}

detect_arch() {
    case "$(uname -m)" in
        x86_64|amd64)   echo "x86_64" ;;
        aarch64|arm64)   echo "aarch64" ;;
        *)               err "Unsupported architecture: $(uname -m)" ;;
    esac
}

build_target() {
    os="$1"
    arch="$2"
    case "$os" in
        linux)  echo "${arch}-unknown-linux-musl" ;;
        darwin) echo "${arch}-apple-darwin" ;;
    esac
}

fetch_latest_tag() {
    tag="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
        | grep '"tag_name"' \
        | head -1 \
        | sed 's/.*"tag_name": *"//;s/".*//')"

    if [ -z "$tag" ]; then
        err "Could not determine latest release. Check https://github.com/${REPO}/releases"
    fi
    echo "$tag"
}

find_install_dir() {
    if [ -d "/usr/local/bin" ] && [ -w "/usr/local/bin" ]; then
        echo "/usr/local/bin"
    else
        dir="${HOME}/.local/bin"
        mkdir -p "$dir"
        echo "$dir"
    fi
}

need_cmd() {
    if ! command -v "$1" > /dev/null 2>&1; then
        err "Required command not found: $1"
    fi
}

err() {
    printf "error: %s\n" "$1" >&2
    exit 1
}

main "$@"
