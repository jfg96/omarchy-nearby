#!/usr/bin/env bash
set -euo pipefail

readonly plugin_id="oma.nearby"
readonly repository="jfg96/omarchy-nearby"
readonly repository_url="https://github.com/${repository}.git"
readonly plugins_dir="${HOME}/.config/omarchy/plugins"
readonly plugin_dir="${plugins_dir}/${plugin_id}"

fail() {
  echo "nearby-install: $*" >&2
  exit 1
}

for command_name in curl git jq omarchy omarchy-shell sha256sum; do
  command -v "$command_name" >/dev/null 2>&1 || fail "required command not found: $command_name"
done

[[ $(uname -s) == "Linux" ]] || fail "prebuilt helpers are supported only on Linux"
case "$(uname -m)" in
  x86_64 | amd64) platform="linux-x86_64" ;;
  *) fail "no prebuilt helper is available for architecture: $(uname -m)" ;;
esac

if (( $# > 1 )); then
  fail "usage: ./install.sh [vX.Y.Z]"
fi

requested_tag="${1:-}"
if [[ -n $requested_tag ]]; then
  [[ $requested_tag =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "release must look like v1.2.3"
  release_url="https://api.github.com/repos/${repository}/releases/tags/${requested_tag}"
else
  release_url="https://api.github.com/repos/${repository}/releases/latest"
fi

release_json=$(curl --fail --location --silent --show-error "$release_url") \
  || fail "could not resolve the requested GitHub release"
tag=$(jq -r '.tag_name // empty' <<<"$release_json")
[[ $tag =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "GitHub did not return a stable SemVer release"
[[ -z $requested_tag || $tag == "$requested_tag" ]] || fail "GitHub returned release $tag instead of $requested_tag"
[[ $(jq -r '.draft // true' <<<"$release_json") == "false" ]] || fail "refusing to install a draft release"
[[ $(jq -r '.prerelease // true' <<<"$release_json") == "false" ]] || fail "refusing to install a prerelease"

asset_name="omarchy-nearby-helper-${tag}-${platform}"
checksum_name="${asset_name}.sha256"
asset_url=$(jq -r --arg name "$asset_name" '.assets[] | select(.name == $name) | .browser_download_url' <<<"$release_json")
checksum_url=$(jq -r --arg name "$checksum_name" '.assets[] | select(.name == $name) | .browser_download_url' <<<"$release_json")
[[ -n $asset_url && -n $checksum_url ]] || fail "release $tag does not contain the expected $platform helper and checksum"

if [[ ! -d $plugin_dir ]]; then
  omarchy plugin add "$repository_url" --yes
fi
[[ -d $plugin_dir/.git ]] || fail "$plugin_dir is not a git-managed Nearby installation"

origin_url=$(git -C "$plugin_dir" remote get-url origin)
case "$origin_url" in
  https://github.com/jfg96/omarchy-nearby | https://github.com/jfg96/omarchy-nearby.git | git@github.com:jfg96/omarchy-nearby.git) ;;
  *) fail "installed plugin has an unexpected origin: $origin_url" ;;
esac

[[ -z $(git -C "$plugin_dir" status --porcelain --untracked-files=no) ]] \
  || fail "installed plugin has local changes; refusing to replace its checkout"

git -C "$plugin_dir" fetch --quiet --force origin "refs/tags/$tag:refs/tags/$tag"
git -C "$plugin_dir" rev-parse --verify --quiet "refs/tags/$tag^{commit}" >/dev/null \
  || fail "release tag $tag is not present in the plugin repository"
git -C "$plugin_dir" checkout --quiet --detach "$tag"

version="${tag#v}"
manifest_version=$(jq -r '.version // empty' "$plugin_dir/manifest.json")
cargo_version=$(awk '
  /^\[package\]$/ { package = 1; next }
  /^\[/ { package = 0 }
  package && /^version[[:space:]]*=/ {
    value = $0
    sub(/^[^=]*=[[:space:]]*"/, "", value)
    sub(/"[[:space:]]*$/, "", value)
    print value
    exit
  }
' "$plugin_dir/backend/Cargo.toml")
[[ $manifest_version == "$version" && $cargo_version == "$version" ]] \
  || fail "release version mismatch: tag=$version manifest=$manifest_version cargo=$cargo_version"

mkdir -p "$plugin_dir/bin"
stage_dir=$(mktemp -d "$plugin_dir/bin/.install.XXXXXX")
cleanup() { rm -rf -- "$stage_dir"; }
trap cleanup EXIT

curl --fail --location --silent --show-error --output "$stage_dir/$asset_name" "$asset_url"
curl --fail --location --silent --show-error --output "$stage_dir/$checksum_name" "$checksum_url"
(
  cd "$stage_dir"
  sha256sum --check --strict "$checksum_name"
)
chmod 0755 "$stage_dir/$asset_name"
mv -- "$stage_dir/$asset_name" "$plugin_dir/bin/omarchy-nearby-helper"

omarchy-shell shell rescanPlugins >/dev/null
omarchy plugin enable "$plugin_id"
echo "Nearby $version installed with its matching $platform helper."
