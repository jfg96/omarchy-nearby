#!/usr/bin/env bash

# bin/nearby-update-helper, offline.

set -uo pipefail

tests_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly updater_source="$tests_dir/../bin/nearby-update-helper"
readonly launcher_source="$tests_dir/../bin/nearby-helper-launcher"
failures=0
current=""

announce() { current="$1"; }
report() { printf '  FAIL: %s\n        %s\n' "$current" "$1" >&2; failures=$((failures + 1)); }
assert_eq() { [[ $1 == "$2" ]] || report "expected '$2', got '$1'"; }
assert_contains() { [[ $1 == *"$2"* ]] || report "expected '$2' in: $1"; }

setup() {
  sandbox=$(mktemp -d "${TMPDIR:-/tmp}/nearby-updater-test.XXXXXX")
  plugin="$sandbox/plugin"
  stub_bin="$sandbox/stub-bin"
  mkdir -p "$plugin/bin" "$stub_bin" "$sandbox/served" "$sandbox/data"
  cp "$updater_source" "$launcher_source" "$plugin/bin/"
  chmod 0755 "$plugin/bin/"*
  asset="omarchy-nearby-helper-v1.2.0-linux-x86_64"
  served="$sandbox/served/$asset"
  printf '#!/usr/bin/env bash\nprintf "omarchy-nearby-helper 1.2.0\\n"\n' >"$served"
  chmod 0755 "$served"
  size=$(stat -c %s -- "$served")
  hash=$(sha256sum "$served"); hash=${hash%% *}
  printf '%s\n' \
    'NEARBY_HELPER_TAG=helper-v1.2.0' \
    "NEARBY_HELPER_ASSET=$asset" \
    "NEARBY_HELPER_SIZE=$size" \
    "NEARBY_HELPER_SHA256=$hash" \
    'NEARBY_HELPER_SOURCE_SHA=0378062e504dee9775da84f00384800c4ce9b55d' \
    'NEARBY_HELPER_RELEASE_WORKFLOW=jfg96/omarchy-nearby/.github/workflows/helper-release.yml' \
    >"$plugin/helper-release.env"
  export FAKE_ASSET_FILE="$served"
  cat >"$stub_bin/curl" <<'STUB'
#!/usr/bin/env bash
output=""
previous=""
for argument in "$@"; do
  [[ $previous == --output ]] && output="$argument"
  previous="$argument"
done
(( ${FAKE_CURL_EXIT:-0} == 0 )) || exit "$FAKE_CURL_EXIT"
cp "$FAKE_ASSET_FILE" "$output"
STUB
  chmod 0755 "$stub_bin/curl"
}

run_updater() {
  HOME="$sandbox/home" XDG_DATA_HOME="$sandbox/data" PATH="$stub_bin:$PATH" \
    "$plugin/bin/nearby-update-helper" 2>/dev/null
}
teardown() { [[ -n ${sandbox:-} && -d $sandbox ]] && rm -rf -- "$sandbox"; unset FAKE_ASSET_FILE FAKE_CURL_EXIT; }

announce "success prefetches through the launcher and reports parseable NDJSON"
setup
output=$(run_updater); status=$?
assert_eq "$status" "0"
assert_contains "$output" '"event":"step"'
assert_contains "$output" '{"event":"done","version":"1.2.0"}'
while IFS= read -r line; do
  [[ -z $line ]] || jq -e . >/dev/null <<<"$line" || report "not JSON: $line"
done <<<"$output"
[[ ! -e $plugin/bin/omarchy-nearby-helper ]] || report "updater wrote an ELF into the checkout"
teardown

announce "an offline update reuses a previously verified helper"
setup
run_updater >/dev/null
export FAKE_CURL_EXIT=6
output=$(run_updater); status=$?
assert_eq "$status" "0"
assert_contains "$output" '"event":"done"'
teardown

announce "a launcher failure becomes one failed JSON event"
setup
export FAKE_CURL_EXIT=6
output=$(run_updater); status=$?
assert_eq "$status" "1"
last=$(tail -1 <<<"$output")
assert_eq "$(jq -r .event <<<"$last")" "failed"
assert_contains "$(jq -r .message <<<"$last")" "check your connection"
teardown

announce "an unsafe launcher is refused"
setup
rm -- "$plugin/bin/nearby-helper-launcher"
ln -s /bin/true "$plugin/bin/nearby-helper-launcher"
output=$(run_updater); status=$?
assert_eq "$status" "1"
assert_contains "$output" "missing or unsafe"
teardown

if (( failures )); then
  printf 'updater tests failed: %s\n' "$failures" >&2
  exit 1
fi
echo "updater tests passed"
