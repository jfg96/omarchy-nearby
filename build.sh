#!/usr/bin/env bash
set -euo pipefail
plugin_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cargo build --release --manifest-path "$plugin_dir/backend/Cargo.toml"
install -Dm755 "$plugin_dir/backend/target/release/omarchy-nearby-helper" "$plugin_dir/bin/omarchy-nearby-helper"
