#!/usr/bin/env bash
# Build the UNPATCHED Slint master variant into target-master/, pinned to the same upstream
# commit the patched fork branch is based on, and prove it is unpatched by checking the binary
# for the patch's environment-variable string. Run from the repo root in Git Bash.
#
# Why this exists: the manifest pins the jtklinger/slint fork (master a1ce0d8 plus the patches
# in docs/patches/). This script rewrites every fork reference to slint-ui/slint at a1ce0d8,
# builds with the lockfile set aside so the new source resolves, restores both files, and
# refuses to succeed if the patch string is present.
set -euo pipefail
cd "$(dirname "$0")/.."
UPSTREAM_REV=a1ce0d87a023c94894a604914e2cb498d6daef8f
TMP=$(mktemp -d)
trap 'cp "$TMP/Cargo.toml" Cargo.toml; cp "$TMP/Cargo.lock" Cargo.lock; rm -rf "$TMP"' EXIT
cp Cargo.toml Cargo.lock "$TMP/"
sed -E "s|git = \"https://github.com/jtklinger/slint\", rev = \"[0-9a-f]+\"|git = \"https://github.com/slint-ui/slint\", rev = \"$UPSTREAM_REV\"|g" \
    Cargo.toml > "$TMP/Cargo.master.toml"
grep -q 'git = "https://github.com/jtklinger/slint"' "$TMP/Cargo.master.toml" && { echo "rewrite failed"; exit 1; }
cp "$TMP/Cargo.master.toml" Cargo.toml
rm -f Cargo.lock
cargo build --release --target-dir target-master
echo "resolved: $(cargo tree -p slint --depth 0 | tail -1)"
if grep -q "SLINT_ACCESSKIT_UPDATE_DELAY_MS" target-master/release/rustfm-proto.exe; then
  echo "ERROR: target-master binary contains the patch"; exit 1
fi
echo "ok: target-master/release/rustfm-proto.exe is unpatched Slint master at $UPSTREAM_REV"
