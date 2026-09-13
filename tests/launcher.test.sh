#!/usr/bin/env bash

# bin/nearby-helper-launcher, offline. curl is stubbed; no case uses network.

set -uo pipefail

tests_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly launcher_source="$tests_dir/../bin/nearby-helper-launcher"
failures=0
current=""

announce() { current="$1"; }
report() { printf '  FAIL: %s\n        %s\n' "$current" "$1" >&2; failures=$((failures + 1)); }
assert_eq() { [[ $1 == "$2" ]] || report "expected '$2', got '$1'"; }
assert_contains() { [[ $1 == *"$2"* ]] || report "expected '$2' in: $1"; }

setup() {
  sandbox=$(mktemp -d "${TMPDIR:-/tmp}/nearby-launcher-test.XXXXXX")
  plugin="$sandbox/plugin"
  stub_bin="$sandbox/stub-bin"
  data_home="$sandbox/data"
  mkdir -p "$plugin/bin" "$stub_bin" "$sandbox/served" "$data_home"
  cp "$launcher_source" "$plugin/bin/nearby-helper-launcher"
  chmod 0755 "$plugin/bin/nearby-helper-launcher"

  asset="omarchy-nearby-helper-v1.2.0-linux-x86_64"
  served="$sandbox/served/$asset"
  printf '#!/usr/bin/env bash\nprintf "omarchy-nearby-helper 1.2.0\\n"\n' >"$served"
  chmod 0755 "$served"
  size=$(stat -c %s -- "$served")
  hash=$(sha256sum "$served"); hash=${hash%% *}
  metadata "$size" "$hash"

  export FAKE_ASSET_FILE="$served"
  export FAKE_CURL_COUNT="$sandbox/curl-count"
  export FAKE_CURL_URL="$sandbox/curl-url"
  printf '0\n' >"$FAKE_CURL_COUNT"
  cat >"$stub_bin/curl" <<'STUB'
#!/usr/bin/env bash
output=""
url=""
previous=""
for argument in "$@"; do
  [[ $previous == --output ]] && output="$argument"
  [[ $argument == https://* || $argument == http://* ]] && url="$argument"
  previous="$argument"
done
count=$(<"$FAKE_CURL_COUNT")
printf '%s\n' "$((count + 1))" >"$FAKE_CURL_COUNT"
printf '%s\n' "$url" >"$FAKE_CURL_URL"
sleep "${FAKE_CURL_DELAY:-0}"
(( ${FAKE_CURL_EXIT:-0} == 0 )) || exit "$FAKE_CURL_EXIT"
if [[ ${FAKE_CURL_SYMLINK:-0} == 1 ]]; then
  ln -s "$FAKE_ASSET_FILE" "$output"
else
  cp "$FAKE_ASSET_FILE" "$output"
fi
STUB
  chmod 0755 "$stub_bin/curl"
}

metadata() {
  local expected_size="$1" expected_hash="$2"
  printf '%s\n' \
    'NEARBY_HELPER_TAG=helper-v1.2.0' \
    'NEARBY_HELPER_ASSET=omarchy-nearby-helper-v1.2.0-linux-x86_64' \
    "NEARBY_HELPER_SIZE=$expected_size" \
    "NEARBY_HELPER_SHA256=$expected_hash" \
    'NEARBY_HELPER_SOURCE_SHA=0378062e504dee9775da84f00384800c4ce9b55d' \
    'NEARBY_HELPER_RELEASE_WORKFLOW=jfg96/omarchy-nearby/.github/workflows/helper-release.yml' \
    >"$plugin/helper-release.env"
}

run_launcher() {
  HOME="$sandbox/home" XDG_DATA_HOME="$data_home" PATH="$stub_bin:$PATH" \
    "$plugin/bin/nearby-helper-launcher" "$@" 2>&1
}

installed_path() { printf '%s/helper-v1.2.0/%s' "$data_home/omarchy-nearby/helpers" "$asset"; }
teardown() {
  [[ -n ${sandbox:-} && -d $sandbox ]] && rm -rf -- "$sandbox"
  unset FAKE_ASSET_FILE FAKE_CURL_COUNT FAKE_CURL_URL FAKE_CURL_DELAY FAKE_CURL_EXIT FAKE_CURL_SYMLINK
}

announce "first prefetch downloads exact immutable bytes outside the checkout"
setup
before=$(find "$plugin" -printf '%P\n' | sort)
output=$(run_launcher --prefetch); status=$?
assert_eq "$status" "0"
assert_eq "$output" "omarchy-nearby-helper 1.2.0"
assert_eq "$(sha256sum "$(installed_path)" | cut -d' ' -f1)" "$hash"
assert_eq "$(stat -c %a -- "$(installed_path)")" "755"
assert_eq "$(<"$FAKE_CURL_COUNT")" "1"
assert_eq "$(<"$FAKE_CURL_URL")" "https://github.com/jfg96/omarchy-nearby/releases/download/helper-v1.2.0/$asset"
assert_eq "$(find "$plugin" -printf '%P\n' | sort)" "$before"
teardown

announce "second prefetch works offline without calling curl"
setup
run_launcher --prefetch >/dev/null
export FAKE_CURL_EXIT=6
output=$(run_launcher --prefetch); status=$?
assert_eq "$status" "0"
assert_eq "$output" "omarchy-nearby-helper 1.2.0"
assert_eq "$(<"$FAKE_CURL_COUNT")" "1"
teardown

announce "normal launch executes the verified helper"
setup
output=$(run_launcher --version); status=$?
assert_eq "$status" "0"
assert_eq "$output" "omarchy-nearby-helper 1.2.0"
teardown

announce "normal launch prefers an intentional local build"
setup
printf '#!/usr/bin/env bash\nprintf "local developer helper\\n"\n' >"$plugin/bin/omarchy-nearby-helper"
chmod 0755 "$plugin/bin/omarchy-nearby-helper"
export FAKE_CURL_EXIT=6
output=$(run_launcher --version); status=$?
assert_eq "$status" "0"
assert_eq "$output" "local developer helper"
assert_eq "$(<"$FAKE_CURL_COUNT")" "0"
teardown

announce "a corrupt cached helper is replaced"
setup
run_launcher --prefetch >/dev/null
printf 'corrupt\n' >"$(installed_path)"
output=$(run_launcher --prefetch); status=$?
assert_eq "$status" "0"
assert_eq "$(<"$FAKE_CURL_COUNT")" "2"
assert_eq "$(sha256sum "$(installed_path)" | cut -d' ' -f1)" "$hash"
teardown

announce "a cached symlink is replaced without touching its target"
setup
mkdir -p "$(dirname "$(installed_path)")"
printf 'keep me\n' >"$sandbox/victim"
ln -s "$sandbox/victim" "$(installed_path)"
output=$(run_launcher --prefetch); status=$?
assert_eq "$status" "0"
[[ ! -L $(installed_path) ]] || report "installed helper remained a symlink"
assert_eq "$(<"$sandbox/victim")" "keep me"
teardown

announce "an oversized download is rejected"
setup
truncate -s $((32 * 1024 * 1024 + 1)) "$served"
output=$(run_launcher --prefetch); status=$?
assert_eq "$status" "1"
assert_contains "$output" "exceeds the safety limit"
[[ ! -e $(installed_path) ]] || report "oversized helper was installed"
teardown

announce "a network failure leaves no installed or staged helper"
setup
export FAKE_CURL_EXIT=6
output=$(run_launcher --prefetch); status=$?
assert_eq "$status" "1"
assert_contains "$output" "check your connection"
[[ ! -e $(installed_path) ]] || report "failed download was installed"
assert_eq "$(find "$data_home" -name '.stage.*' | wc -l)" "0"
teardown

announce "a downloaded symlink is rejected"
setup
export FAKE_CURL_SYMLINK=1
output=$(run_launcher --prefetch); status=$?
assert_eq "$status" "1"
assert_contains "$output" "not a regular file"
teardown

announce "concurrent prefetches converge on one verified helper"
setup
export FAKE_CURL_DELAY=0.2
run_launcher --prefetch >"$sandbox/one.out" & first=$!
run_launcher --prefetch >"$sandbox/two.out" & second=$!
wait "$first"; first_status=$?
wait "$second"; second_status=$?
assert_eq "$first_status" "0"
assert_eq "$second_status" "0"
assert_eq "$(sha256sum "$(installed_path)" | cut -d' ' -f1)" "$hash"
assert_eq "$(find "$data_home" -name '.stage.*' | wc -l)" "0"
teardown

announce "unknown metadata fields fail before network access"
setup
printf 'UNEXPECTED=value\n' >>"$plugin/helper-release.env"
output=$(run_launcher --prefetch); status=$?
assert_eq "$status" "1"
assert_contains "$output" "unknown field"
assert_eq "$(<"$FAKE_CURL_COUNT")" "0"
teardown

if (( failures )); then
  printf 'launcher tests failed: %s\n' "$failures" >&2
  exit 1
fi
echo "launcher tests passed"
