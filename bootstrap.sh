#!/bin/sh
# bootstrap.sh: from nothing to `cs trybox` on a Mac.
# Idempotent: rerun it any time. Asks before installing anything; `-y` says yes to all.
# Never uses sudo itself; Homebrew and the Xcode installer ask for a password on their own.
set -eu

REPO_URL="https://github.com/calculating-space/rust-binaries.git"
DEST="${RUST_BINARIES_DIR:-$HOME/rust-binaries}"
YES=0
[ "${1:-}" = "-y" ] && YES=1

status() { printf '%-12s %s\n' "$1" "$2"; }
ask() {
    [ "$YES" = 1 ] && return 0
    printf '%s [y/N] ' "$1"
    read -r answer </dev/tty
    [ "$answer" = y ] || [ "$answer" = Y ]
}

case "$(uname -s)-$(uname -m)" in
    Darwin-arm64) status platform "Apple Silicon Mac" ;;
    *) status platform "$(uname -s) $(uname -m): only the bare recipe runs here; mlx and whisper need an Apple Silicon Mac" ;;
esac

if xcode-select -p >/dev/null 2>&1; then
    status "xcode tools" ok
else
    status "xcode tools" "missing: a dialog is opening; run this script again when it has finished"
    xcode-select --install >/dev/null 2>&1 || true
    exit 1
fi

if command -v brew >/dev/null 2>&1; then
    status homebrew ok
else
    ask "Install Homebrew (it asks for your password)?" || exit 1
    /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
    [ -x /opt/homebrew/bin/brew ] && eval "$(/opt/homebrew/bin/brew shellenv)"
    status homebrew installed
fi

[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
if command -v cargo >/dev/null 2>&1; then
    status rust "ok   $(rustc --version)"
else
    ask "Install Rust with rustup (about 2 minutes)?" || exit 1
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path
    . "$HOME/.cargo/env"
    status rust "installed $(rustc --version)"
fi

for tool in uv ffmpeg; do
    if command -v "$tool" >/dev/null 2>&1; then
        status "$tool" ok
    else
        ask "brew install $tool?" || exit 1
        brew install "$tool"
        status "$tool" installed
    fi
done

here="$(cd "$(dirname "$0")" 2>/dev/null && pwd || true)"
if [ -n "$here" ] && [ -f "$here/cs/Cargo.toml" ]; then
    DEST="$here"
    status repo "$DEST"
elif [ -f "$DEST/cs/Cargo.toml" ]; then
    status repo "$DEST (already cloned)"
else
    git clone -q "$REPO_URL" "$DEST"
    status repo "cloned to $DEST"
fi

export PATH="$HOME/.cargo/bin:$PATH"
if command -v cs >/dev/null 2>&1 && cs --where cs 2>/dev/null | grep -q "^$DEST/"; then
    status cs ok
else
    status cs "building (about a minute)"
    cargo install -q --force --path "$DEST/cs"
fi

if [ -x "$DEST/trybox/target/release/trybox" ]; then
    status trybox ok
else
    status trybox "building (about a minute)"
    cs --build trybox >/dev/null
fi

echo
cs trybox doctor || true
echo
echo "ready: run  cs trybox"
