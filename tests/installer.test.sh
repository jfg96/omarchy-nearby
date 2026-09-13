#!/usr/bin/env bash

# install.sh, offline. External commands are stubbed so release resolution,
# download policy and replacement can be exercised without touching a real
# Omarchy installation or network.

set -uo pipefail

tests_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly installer_source="$tests_dir/../install.sh"
failures=0
current=""

announce() { current="$1"; }
report() {
  echo "  FAIL: $current" >&2
  echo "        $1" >&2
  failures=$((failures + 1))
}
assert_eq() { [[ $1 == "$2" ]] || report "expected '$2', got '$1'"; }
assert_contains() { [[ $1 == *"$2"* ]] || report "expected output to contain '$2', got: $1"; }

setup() {
  sandbox=$(mktemp -d "${TMPDIR:-/tmp}/nearby-installer-test.XXXXXX")
  fake_home="$sandbox/home"
  plugin="$fake_home/.config/omarchy/plugins/oma.nearby"
  stub_bin="$sandbox/stub-bin"
  mkdir -p "$fake_home" "$stub_bin" "$sandbox/cache" "$sandbox/served"

  platform="linux-$(uname -m)"
  [[ $platform == "linux-amd64" ]] && platform="linux-x86_64"
  asset_name="omarchy-nearby-helper-v1.1.0-${platform}"
  printf 'new helper 1.1.0\n' >"$sandbox/served/$asset_name"
  (cd "$sandbox/served" && sha256sum "$asset_name" >"$asset_name.sha256")

  export FAKE_PLUGIN_DIR="$plugin"
  export FAKE_ASSET_FILE="$sandbox/served/$asset_name"
  export FAKE_CHECKSUM_FILE="$sandbox/served/$asset_name.sha256"
  export FAKE_RELEASE_JSON="$sandbox/release.json"
  release_json "https://example.invalid/$asset_name"

  cat >"$stub_bin/curl" <<'STUB'
#!/usr/bin/env bash
output=""
url=""
previous=""
for argument in "$@"; do
  [[ $previous == --output ]] && output="$argument"
  [[ $argument == http* ]] && url="$argument"
  previous="$argument"
done
if [[ $url == *api.github.com* ]]; then
  cat "$FAKE_RELEASE_JSON"
elif [[ $url == *.sha256 ]]; then
  cp "$FAKE_CHECKSUM_FILE" "$output"
else
  cp "$FAKE_ASSET_FILE" "$output"
fi
STUB

  cat >"$stub_bin/omarchy" <<'STUB'
#!/usr/bin/env bash
if [[ ${1:-} == plugin && ${2:-} == add ]]; then
  mkdir -p "$FAKE_PLUGIN_DIR/.git" "$FAKE_PLUGIN_DIR/bin" "$FAKE_PLUGIN_DIR/backend"
  printf '{"id":"oma.nearby","version":"1.1.0"}\n' >"$FAKE_PLUGIN_DIR/manifest.json"
  printf '[package]\nname = "omarchy-nearby-helper"\nversion = "1.1.0"\n' >"$FAKE_PLUGIN_DIR/backend/Cargo.toml"
  printf 'old helper\n' >"$FAKE_PLUGIN_DIR/bin/omarchy-nearby-helper"
fi
exit 0
STUB

  cat >"$stub_bin/git" <<'STUB'
#!/usr/bin/env bash
case " $* " in
  *" remote get-url origin "*) printf 'https://github.com/jfg96/omarchy-nearby.git\n' ;;
  *" status --porcelain "*) ;;
  *" rev-parse "*) printf 'test-commit\n' ;;
esac
exit 0
STUB

  printf '#!/usr/bin/env bash\nexit 0\n' >"$stub_bin/omarchy-shell"
  chmod 0755 "$stub_bin"/*
}

release_json() {
  local asset_url="$1"
  jq -n --arg asset "$asset_name" --arg url "$asset_url" '{
    tag_name: "v1.1.0", draft: false, prerelease: false,
    assets: [
      {name: $asset, browser_download_url: $url},
      {name: ($asset + ".sha256"), browser_download_url: ($url + ".sha256")}
    ]
  }' >"$FAKE_RELEASE_JSON"
}

run_installer() {
  HOME="$fake_home" XDG_CACHE_HOME="$sandbox/cache" PATH="$stub_bin:$PATH" \
    bash "$installer_source" v1.1.0 2>&1
}

installed_helper() { cat "$plugin/bin/omarchy-nearby-helper"; }
teardown() { [[ -n ${sandbox:-} && -d $sandbox ]] && rm -rf -- "$sandbox"; }

announce "success installs the verified helper from staging outside the plugin"
setup
output=$(run_installer); status=$?
assert_eq "$status" "0"
assert_contains "$output" "Nearby 1.1.0 installed"
assert_eq "$(installed_helper)" "new helper 1.1.0"
assert_eq "$(find "$sandbox/cache" -name 'install.*' | wc -l)" "0"
assert_eq "$(find "$plugin/bin" -name '.install.*' | wc -l)" "0"
teardown

announce "an oversized helper leaves the old helper intact"
setup
truncate -s $((32 * 1024 * 1024 + 1)) "$FAKE_ASSET_FILE"
output=$(run_installer); status=$?
assert_eq "$status" "1"
assert_contains "$output" "exceeds its download limit"
assert_eq "$(installed_helper)" "old helper"
teardown

announce "a non-HTTPS asset URL is refused"
setup
release_json "http://example.invalid/$asset_name"
output=$(run_installer); status=$?
assert_eq "$status" "1"
assert_contains "$output" "non-HTTPS asset URL"
[[ ! -e $plugin/bin/omarchy-nearby-helper ]] \
  || report "an invalid release must fail before changing the installation"
teardown

announce "a checksum naming another asset is refused"
setup
hash=$(sha256sum "$FAKE_ASSET_FILE"); hash=${hash%% *}
printf '%s  another-helper\n' "$hash" >"$FAKE_CHECKSUM_FILE"
output=$(run_installer); status=$?
assert_eq "$status" "1"
assert_contains "$output" "checksum is malformed"
assert_eq "$(installed_helper)" "old helper"
teardown

if (( failures )); then
  echo "installer tests failed: $failures" >&2
  exit 1
fi
echo "installer tests passed"
