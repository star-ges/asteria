#!/usr/bin/env bash
# Remove Asteria. Instances, worlds, accounts and settings stay unless you pass --purge.
#   ./uninstall.sh            remove the app
#   ./uninstall.sh --purge    also delete every instance, world, backup and signed-in account
set -euo pipefail

BIN_DIR="$HOME/.local/bin"
SHARE_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
DATA_DIR="$SHARE_DIR/asteria"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/asteria"

purge=false
case "${1:-}" in
    "") ;;
    --purge) purge=true ;;
    *) echo "usage: $0 [--purge]" >&2; exit 2 ;;
esac

# Deleting data under a running game or launcher would leave it writing into nothing.
if $purge && pgrep -x asteria >/dev/null; then
    echo "Asteria is running. Quit it first." >&2
    exit 1
fi

rm -f "$BIN_DIR/asteria" "$SHARE_DIR/applications/asteria.desktop" "$SHARE_DIR/icons/hicolor/scalable/apps/asteria.svg"
command -v update-desktop-database >/dev/null && update-desktop-database -q "$SHARE_DIR/applications" || true
echo "Asteria is uninstalled."

if ! $purge; then
    [ -d "$DATA_DIR" ] && echo "Your instances and worlds are still in $DATA_DIR (run with --purge to delete them)."
    exit 0
fi

echo
echo "This permanently deletes:"
echo "  $DATA_DIR    every instance, world, backup and signed-in account"
echo "  $CONFIG_DIR  settings"
# Read from the terminal, so this also works when the script itself comes through a pipe.
read -r -p "Type 'delete' to go ahead: " answer </dev/tty
if [ "$answer" = delete ]; then
    rm -rf "$DATA_DIR" "$CONFIG_DIR"
    # The links Asteria left where Omacraft's folders were (only links: never a real folder).
    for old in "$SHARE_DIR/omacraft" "${XDG_CONFIG_HOME:-$HOME/.config}/omacraft"; do
        [ -L "$old" ] && rm -f "$old"
    done
    echo "Deleted."
else
    echo "Kept your data."
fi
