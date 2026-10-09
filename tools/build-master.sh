#!/usr/bin/env bash
# Build the UNPATCHED Slint master variant into target-master/, pinned to the same commit as
# the vendored (patched) copy, and prove it is unpatched by checking the binary for the
# patch's environment-variable string. Run from the repo root in Git Bash.
#
# Why this exists: the [patch] section in Cargo.toml plus the committed Cargo.lock means a
# plain `cargo build` always uses vendor/slint. Stripping the section is not enough while the
# lockfile still records the path sources, and a build without a lockfile drifts to whatever
# master is today. This script strips, pins, builds with the lockfile set aside, restores
# everything, and refuses to succeed if the patch string is present.
set -euo pipefail
cd "$(dirname "$0")/.."
REV=$(git -C vendor/slint rev-parse HEAD)
TMP=$(mktemp -d)
trap 'cp "$TMP/Cargo.toml" Cargo.toml; cp "$TMP/Cargo.lock" Cargo.lock; rm -rf "$TMP"' EXIT
cp Cargo.toml Cargo.lock "$TMP/"
sed -e '/^# Day-2 experiment/,$d' \
    -e "s|git = \"https://github.com/slint-ui/slint\", branch = \"master\"|git = \"https://github.com/slint-ui/slint\", rev = \"$REV\"|g" \
    Cargo.toml > "$TMP/Cargo.master.toml"
grep -q '\[patch' "$TMP/Cargo.master.toml" && { echo "strip failed"; exit 1; }
cp "$TMP/Cargo.master.toml" Cargo.toml
rm -f Cargo.lock
cargo build --release --target-dir target-master
echo "resolved: $(cargo tree -p slint --depth 0 | tail -1)"
if grep -q "SLINT_ACCESSKIT_UPDATE_DELAY_MS" target-master/release/rustfm-proto.exe; then
  echo "ERROR: target-master binary contains the patch"; exit 1
fi
echo "ok: target-master/release/rustfm-proto.exe is unpatched Slint master at $REV"
