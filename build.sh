#!/usr/bin/env bash
set -euo pipefail
plugin_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cargo build --release --manifest-path "$plugin_dir/backend/Cargo.toml"
install -Dm755 "$plugin_dir/backend/target/release/omarchy-nearby-helper" "$plugin_dir/bin/omarchy-nearby-helper"
# Only a deliberate local build is a developer override. The launcher refuses
# to run bin/omarchy-nearby-helper without this marker, so a binary left behind
# by an older published version is treated as legacy residue instead. The file
# is ignored by Git and is never created by any other flow.
touch "$plugin_dir/bin/.nearby-local-helper"
