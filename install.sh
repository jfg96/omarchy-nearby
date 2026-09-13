#!/usr/bin/env bash

# Recovery/non-interactive installer. Normal interactive installation is:
#   omarchy plugin add https://github.com/jfg96/omarchy-nearby --enable

set -euo pipefail

readonly plugin_id="oma.nearby"
readonly repository_url="https://github.com/jfg96/omarchy-nearby"
readonly plugins_dir="${HOME:?HOME is not set}/.config/omarchy/plugins"
readonly plugin_dir="$plugins_dir/$plugin_id"

fail() {
  printf 'nearby-install: %s\n' "$*" >&2
  exit 1
}

for command_name in git jq omarchy omarchy-shell; do
  command -v "$command_name" >/dev/null 2>&1 \
    || fail "required command not found: $command_name"
done

(( $# <= 1 )) || fail "usage: ./install.sh [vX.Y.Z]"
requested_tag="${1:-}"
[[ -z $requested_tag || $requested_tag =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] \
  || fail "release must look like v1.2.3"

installed_now=false
if [[ ! -d $plugin_dir ]]; then
  omarchy plugin add "$repository_url" --yes \
    || fail "Omarchy could not install Nearby"
  installed_now=true
fi
[[ -d $plugin_dir/.git ]] || fail "$plugin_dir is not a git-managed Nearby installation"

origin_url=$(git -C "$plugin_dir" remote get-url origin)
case "$origin_url" in
  https://github.com/jfg96/omarchy-nearby | https://github.com/jfg96/omarchy-nearby.git | git@github.com:jfg96/omarchy-nearby.git) ;;
  *) fail "installed plugin has an unexpected origin: $origin_url" ;;
esac

[[ -z $(git -C "$plugin_dir" status --porcelain --untracked-files=no) ]] \
  || fail "installed plugin has local changes; refusing to replace its checkout"

if [[ -n $requested_tag ]]; then
  git -C "$plugin_dir" fetch --quiet --force origin "refs/tags/$requested_tag:refs/tags/$requested_tag"
  git -C "$plugin_dir" rev-parse --verify --quiet "refs/tags/$requested_tag^{commit}" >/dev/null \
    || fail "release tag $requested_tag is not present in the plugin repository"
  git -C "$plugin_dir" checkout --quiet --detach "$requested_tag"
  manifest_version=$(jq -r '.version // empty' "$plugin_dir/manifest.json") \
    || fail "manifest.json is invalid"
  [[ $manifest_version == "${requested_tag#v}" ]] \
    || fail "release version mismatch: tag=${requested_tag#v} manifest=$manifest_version"
elif [[ $installed_now == false ]]; then
  omarchy plugin update "$plugin_id" --yes \
    || fail "Omarchy could not update Nearby"
fi

launcher="$plugin_dir/bin/nearby-helper-launcher"
[[ -x $launcher && ! -L $launcher ]] \
  || fail "this Nearby checkout has no safe helper launcher"
"$launcher" --prefetch || fail "could not prepare the verified Nearby helper"

omarchy-shell shell rescanPlugins >/dev/null
omarchy plugin enable "$plugin_id"
printf 'Nearby %s installed with its verified helper.\n' \
  "$(jq -r '.version' "$plugin_dir/manifest.json")"
