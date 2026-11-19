#!/usr/bin/env bash
# Build and install Asteria for the current user (~/.local), or update an existing install.
# Run it from a clone (builds that checkout), or on its own (builds the latest release):
#   curl -fsSL https://raw.githubusercontent.com/star-ges/asteria/main/install.sh | bash
#   ... | bash -s -- --main    build the latest main instead of the latest release
set -euo pipefail

REPO=https://github.com/star-ges/asteria
MIN_RUST=1.95
BIN_DIR="$HOME/.local/bin"
SHARE_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"

say() { printf '\033[1m%s\033[0m\n' "$*"; }
fail() { printf 'error: %s\n' "$*" >&2; exit 1; }

main=false
case "${1:-}" in
    "") ;;
    --main) main=true ;;
    *) echo "usage: $0 [--main]" >&2; exit 2 ;;
esac

[ "$(uname -s)" = Linux ] || fail "Asteria runs on Linux only."
[ "$(uname -m)" = x86_64 ] || fail "Asteria launches games on x86_64 only (this is $(uname -m))."

export PATH="$HOME/.cargo/bin:$PATH"
command -v cargo >/dev/null || fail "Rust isn't installed. Get it from https://rustup.rs, then run this again."
rust=$(rustc --version | awk '{print $2}')
if [ "$(printf '%s\n%s\n' "$MIN_RUST" "$rust" | sort -V | head -n1)" != "$MIN_RUST" ]; then
    fail "Asteria needs Rust $MIN_RUST or newer (you have $rust). Run 'rustup update', then this again."
fi

# Use the clone this script sits in, else fetch a fresh copy.
src=""
here=$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" 2>/dev/null && pwd || true)
if [ -n "$here" ] && [ -f "$here/Cargo.toml" ] && grep -q '^name = "asteria"' "$here/Cargo.toml"; then
    src=$here
    $main && echo "Note: from a clone, this builds what's checked out; run 'git switch main && git pull' first."
else
    command -v git >/dev/null || fail "git isn't installed."
    ref=main
    if ! $main; then
        # Release tags only (vX.Y.Z), newest by version.
        ref=$(git ls-remote --tags --refs "$REPO" 'v*' | sed 's#.*refs/tags/##' | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | sort -V | tail -n1 || true)
        [ -n "$ref" ] || { echo "No release yet: building main."; ref=main; }
    fi
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    say "Downloading Asteria ($ref)…"
    # Fetch rather than clone --branch: a shallow clone of an annotated tag warns "is not a commit".
    git init --quiet "$tmp/asteria"
    if [ "$ref" = main ]; then spec=refs/heads/main; else spec=refs/tags/$ref; fi
    git -C "$tmp/asteria" fetch --quiet --depth 1 "$REPO" "$spec"
    git -C "$tmp/asteria" -c advice.detachedHead=false checkout --quiet FETCH_HEAD
    src="$tmp/asteria"
fi

say "Building (this takes a few minutes the first time)…"
# An explicit target dir, so a CARGO_TARGET_DIR in the environment doesn't put the binary elsewhere.
cargo build --release --manifest-path "$src/Cargo.toml" --target-dir "$src/target"

say "Installing…"
# Copy beside the old binary and rename over it, so a running Asteria keeps working.
install -Dm755 "$src/target/release/asteria" "$BIN_DIR/.asteria.new"
mv -f "$BIN_DIR/.asteria.new" "$BIN_DIR/asteria"
install -Dm644 "$src/packaging/asteria.desktop" "$SHARE_DIR/applications/asteria.desktop"
install -Dm644 "$src/packaging/asteria.svg" "$SHARE_DIR/icons/hicolor/scalable/apps/asteria.svg"
# Asteria continues Omacraft: replace its app (never its data, which Asteria moves over
# on its first start and links back, so an Omacraft still running keeps working).
if [ -e "$BIN_DIR/omacraft" ] || [ -e "$SHARE_DIR/applications/omacraft.desktop" ]; then
    rm -f "$BIN_DIR/omacraft" "$SHARE_DIR/applications/omacraft.desktop" "$SHARE_DIR/icons/hicolor/scalable/apps/omacraft.svg"
    echo "Removed the old Omacraft app. Asteria moves its instances over when it first starts."
fi
command -v update-desktop-database >/dev/null && update-desktop-database -q "$SHARE_DIR/applications" || true

say "Asteria is installed. Start it from your app launcher, or run 'asteria'."
case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) echo "Note: $BIN_DIR isn't on your PATH, so 'asteria' won't run from a terminal until you add it." ;;
esac
if pgrep -x asteria >/dev/null; then
    echo "Asteria is running: restart it to use the new version."
fi
