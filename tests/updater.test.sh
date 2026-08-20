#!/usr/bin/env bash

# bin/nearby-update-helper, offline.
#
# The updater is the part of this plugin that reaches the network, verifies a
# checksum and replaces a running binary, and every bug that reached a user so
# far lived in it: a 404 reported as a dead connection, nine megabytes staged
# inside the watched plugin directory, and a sweep loop that returned 1 from a
# successful run. None of that is reachable from the QML tests, so it is
# covered here with a curl stub on PATH -- the script needs no seam of its own.
#
# Run: bash tests/updater.test.sh

set -uo pipefail

tests_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly updater_source="$tests_dir/../bin/nearby-update-helper"
[[ -f $updater_source ]] || { echo "updater not found: $updater_source" >&2; exit 1; }

failures=0
current=""

announce() { current="$1"; }
report() {
  echo "  FAIL: $current" >&2
  echo "        $1" >&2
  failures=$((failures + 1))
}
assert_eq() {
  [[ $1 == "$2" ]] || report "expected '$2', got '$1'${3:+ ($3)}"
}
assert_contains() {
  [[ $1 == *"$2"* ]] || report "expected output to contain '$2', got: $1"
}
assert_not_contains() {
  [[ $1 != *"$2"* ]] || report "expected output NOT to contain '$2', got: $1"
}

# A sandbox per case: a plugin directory holding only what the updater reads,
# a cache root for its staging, and a stub curl ahead of the real one.
setup() {
  sandbox=$(mktemp -d "${TMPDIR:-/tmp}/nearby-updater-test.XXXXXX")
  plugin="$sandbox/plugin"
  stub_bin="$sandbox/stub-bin"
  mkdir -p "$plugin/bin" "$stub_bin" "$sandbox/cache" "$sandbox/served"
  cp "$updater_source" "$plugin/bin/nearby-update-helper"
  chmod 0755 "$plugin/bin/nearby-update-helper"
  printf 'old helper\n' >"$plugin/bin/omarchy-nearby-helper"
  chmod 0755 "$plugin/bin/omarchy-nearby-helper"
  manifest "1.1.0"

  # The stub answers the three calls the updater makes, told apart by URL, and
  # is steered entirely by environment so each case stays declarative.
  cat >"$stub_bin/curl" <<'STUB'
#!/usr/bin/env bash
output=""
url=""
previous=""
for argument in "$@"; do
  case "$previous" in
  --output) output="$argument" ;;
  esac
  case "$argument" in
  http*) url="$argument" ;;
  esac
  previous="$argument"
done

if [[ $url == *api.github.com* ]]; then
  (( ${FAKE_LOOKUP_EXIT:-0} == 0 )) || exit "$FAKE_LOOKUP_EXIT"
  [[ -n $output ]] && cp "${FAKE_RELEASE_JSON:-/dev/null}" "$output"
  printf '%s' "${FAKE_HTTP_STATUS:-200}"
  exit 0
fi

if [[ $url == *.sha256 ]]; then
  (( ${FAKE_CHECKSUM_EXIT:-0} == 0 )) || exit "$FAKE_CHECKSUM_EXIT"
  cp "$FAKE_CHECKSUM_FILE" "$output"
  exit 0
fi

sleep "${FAKE_ASSET_DELAY:-0}"
(( ${FAKE_ASSET_EXIT:-0} == 0 )) || exit "$FAKE_ASSET_EXIT"
cp "$FAKE_ASSET_FILE" "$output"
exit 0
STUB
  chmod 0755 "$stub_bin/curl"

  platform="linux-$(uname -m)"
  [[ $platform == "linux-amd64" ]] && platform="linux-x86_64"
  asset_name="omarchy-nearby-helper-v1.1.0-${platform}"

  printf 'new helper 1.1.0\n' >"$sandbox/served/$asset_name"
  ( cd "$sandbox/served" && sha256sum "$asset_name" >"$asset_name.sha256" )

  export FAKE_RELEASE_JSON="$sandbox/release.json"
  export FAKE_ASSET_FILE="$sandbox/served/$asset_name"
  export FAKE_CHECKSUM_FILE="$sandbox/served/$asset_name.sha256"
  release_json "$asset_name" false false
}

teardown() {
  [[ -n ${sandbox:-} && -d $sandbox ]] && rm -rf -- "$sandbox"
  unset FAKE_RELEASE_JSON FAKE_ASSET_FILE FAKE_CHECKSUM_FILE
  unset FAKE_HTTP_STATUS FAKE_LOOKUP_EXIT FAKE_ASSET_EXIT FAKE_CHECKSUM_EXIT FAKE_ASSET_DELAY
}

manifest() {
  printf '{"id":"oma.nearby","version":"%s","minHelperVersion":"%s"}\n' "$1" "$1" >"$plugin/manifest.json"
}

release_json() {
  jq -n --arg name "$1" --argjson draft "$2" --argjson prerelease "$3" '{
    tag_name: "v1.1.0", draft: $draft, prerelease: $prerelease,
    assets: [
      {name: $name, browser_download_url: ("https://example.invalid/" + $name)},
      {name: ($name + ".sha256"), browser_download_url: ("https://example.invalid/" + $name + ".sha256")}
    ]
  }' >"$FAKE_RELEASE_JSON"
}

run_updater() {
  PATH="$stub_bin:$PATH" XDG_CACHE_HOME="$sandbox/cache" \
    "$plugin/bin/nearby-update-helper" 2>/dev/null
}

installed_helper() { cat "$plugin/bin/omarchy-nearby-helper"; }
staging_in_plugin() { find "$plugin/bin" -maxdepth 1 -name '.update.*' | wc -l; }
staging_in_cache() { find "$sandbox/cache" -mindepth 1 -maxdepth 2 -name 'update.*' 2>/dev/null | wc -l; }

# --- a release that exists, with a checksum that matches -----------------

announce "success replaces the binary, reports done, and exits 0"
setup
output=$(run_updater); status=$?
assert_eq "$status" "0" "a verified install must not report failure"
assert_contains "$output" '{"event":"done","version":"1.1.0"}'
assert_eq "$(installed_helper)" "new helper 1.1.0"
assert_eq "$(staging_in_cache)" "0" "the staging directory must be cleaned up"
assert_eq "$(staging_in_plugin)" "0" "nothing may be left inside the watched plugin directory"
teardown

announce "progress is reported as one JSON object per line"
setup
output=$(run_updater)
assert_contains "$output" '"event":"step"'
assert_contains "$output" 'Looking up Nearby v1.1.0'
assert_contains "$output" 'Verifying checksum'
while IFS= read -r line; do
  [[ -n $line ]] || continue
  jq -e . >/dev/null 2>&1 <<<"$line" || report "not JSON: $line"
done <<<"$output"
teardown

# The sweep loop runs last and used to decide the exit status: with no orphan
# to match, the glob stayed literal, the test failed, and a successful update
# reported failure to the panel.
announce "an orphan staging directory is swept without changing the exit status"
setup
mkdir -p "$plugin/bin/.update.leftover"
printf 'junk\n' >"$plugin/bin/.update.leftover/partial"
output=$(run_updater); status=$?
assert_eq "$status" "0"
assert_eq "$(staging_in_plugin)" "0" "a leftover staging directory must be swept"
assert_contains "$output" '"event":"done"'
teardown

# --- the plugin directory is off limits until the binary is ready --------

# Writing into the plugin tree makes the shell reload the plugin, which tears
# down the service and kills this process. Staging there meant the download was
# killed partway through, every time, and left its directory behind.
announce "nothing is written into the plugin directory before the final rename"
setup
before=$(find "$plugin" | sort)
FAKE_ASSET_DELAY=3 PATH="$stub_bin:$PATH" XDG_CACHE_HOME="$sandbox/cache" \
  "$plugin/bin/nearby-update-helper" >/dev/null 2>&1 &
updater_pid=$!
sleep 1.5
kill -KILL "$updater_pid" 2>/dev/null
wait "$updater_pid" 2>/dev/null
after=$(find "$plugin" | sort)
assert_eq "$after" "$before" "a download in flight must leave the plugin directory untouched"
assert_eq "$(installed_helper)" "old helper" "a killed update must not have replaced the binary"
teardown

# --- failures leave the old helper in place ------------------------------

announce "a checksum mismatch discards the download"
setup
printf 'tampered\n' >"$sandbox/served/$asset_name"
output=$(run_updater); status=$?
assert_eq "$status" "1"
assert_contains "$output" "failed its SHA256 check"
assert_eq "$(installed_helper)" "old helper" "an unverified binary must never be installed"
assert_eq "$(staging_in_cache)" "0"
assert_eq "$(staging_in_plugin)" "0"
teardown

# "no such release" and "no network" are different problems with different
# fixes. curl --fail reports both as a non-zero exit, and collapsing them told
# users to check a connection that was working.
announce "a missing release is not reported as a connection problem"
setup
export FAKE_HTTP_STATUS=404
output=$(run_updater); status=$?
assert_eq "$status" "1"
assert_contains "$output" "GitHub has no release v1.1.0"
assert_not_contains "$output" "Check your connection"
teardown

announce "an unreachable host is reported as a connection problem"
setup
export FAKE_LOOKUP_EXIT=6
output=$(run_updater); status=$?
assert_eq "$status" "1"
assert_contains "$output" "Could not reach GitHub"
assert_contains "$output" "Check your connection"
assert_not_contains "$output" "has no release"
teardown

announce "a rate limit says to wait rather than to retry now"
setup
export FAKE_HTTP_STATUS=403
output=$(run_updater)
assert_contains "$output" "rate-limiting"
teardown

announce "a server error is distinguished from a missing release"
setup
export FAKE_HTTP_STATUS=503
output=$(run_updater)
assert_contains "$output" "HTTP 503"
assert_not_contains "$output" "has no release"
teardown

announce "a development checkout is told to build, not to retry the network"
setup
manifest "1.1.1-dev"
output=$(run_updater); status=$?
assert_eq "$status" "1"
assert_contains "$output" "development build"
assert_contains "$output" "./build.sh"
assert_not_contains "$output" '"event":"step"' "a version with no release must fail before reaching the network"
teardown

announce "a release without a helper for this architecture says so"
setup
release_json "omarchy-nearby-helper-v1.1.0-linux-somethingelse" false false
output=$(run_updater); status=$?
assert_eq "$status" "1"
assert_contains "$output" "publishes no"
assert_eq "$(installed_helper)" "old helper"
teardown

announce "a draft release is refused"
setup
release_json "$asset_name" true false
output=$(run_updater); status=$?
assert_eq "$status" "1"
assert_contains "$output" "draft"
teardown

announce "a prerelease is refused"
setup
release_json "$asset_name" false true
output=$(run_updater); status=$?
assert_eq "$status" "1"
assert_contains "$output" "prerelease"
teardown

announce "a broken manifest fails before the network"
setup
printf 'not json\n' >"$plugin/manifest.json"
output=$(run_updater); status=$?
assert_eq "$status" "1"
assert_contains "$output" "not valid JSON"
teardown

announce "every failure is a single JSON object the panel can parse"
setup
export FAKE_HTTP_STATUS=404
output=$(run_updater)
last=$(tail -1 <<<"$output")
assert_eq "$(jq -r '.event' <<<"$last")" "failed"
[[ -n $(jq -r '.message' <<<"$last") ]] || report "a failure must carry a message"
teardown

if (( failures )); then
  echo "updater tests failed: $failures" >&2
  exit 1
fi
echo "updater tests passed"
