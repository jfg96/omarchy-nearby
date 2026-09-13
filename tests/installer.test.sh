#!/usr/bin/env bash

# install.sh, offline. Omarchy, git and curl are stubbed.

set -uo pipefail

tests_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly installer_source="$tests_dir/../install.sh"
readonly launcher_source="$tests_dir/../bin/nearby-helper-launcher"
failures=0
current=""

announce() { current="$1"; }
report() { printf '  FAIL: %s\n        %s\n' "$current" "$1" >&2; failures=$((failures + 1)); }
assert_eq() { [[ $1 == "$2" ]] || report "expected '$2', got '$1'"; }
assert_contains() { [[ $1 == *"$2"* ]] || report "expected '$2' in: $1"; }

setup() {
  sandbox=$(mktemp -d "${TMPDIR:-/tmp}/nearby-installer-test.XXXXXX")
  fake_home="$sandbox/home"
  plugin="$fake_home/.config/omarchy/plugins/oma.nearby"
  stub_bin="$sandbox/stub-bin"
  mkdir -p "$fake_home" "$stub_bin" "$sandbox/source/bin" "$sandbox/served" "$sandbox/data"
  cp "$launcher_source" "$sandbox/source/bin/nearby-helper-launcher"
  chmod 0755 "$sandbox/source/bin/nearby-helper-launcher"
  asset="omarchy-nearby-helper-v1.2.0-linux-x86_64"
  served="$sandbox/served/$asset"
  printf '#!/usr/bin/env bash\nprintf "omarchy-nearby-helper 1.2.0\\n"\n' >"$served"
  chmod 0755 "$served"
  size=$(stat -c %s -- "$served")
  hash=$(sha256sum "$served"); hash=${hash%% *}
  printf '{"id":"oma.nearby","version":"1.2.0"}\n' >"$sandbox/source/manifest.json"
  printf '%s\n' \
    'NEARBY_HELPER_TAG=helper-v1.2.0' \
    "NEARBY_HELPER_ASSET=$asset" \
    "NEARBY_HELPER_SIZE=$size" \
    "NEARBY_HELPER_SHA256=$hash" \
    'NEARBY_HELPER_SOURCE_SHA=0378062e504dee9775da84f00384800c4ce9b55d' \
    'NEARBY_HELPER_RELEASE_WORKFLOW=jfg96/omarchy-nearby/.github/workflows/helper-release.yml' \
    >"$sandbox/source/helper-release.env"
  export FAKE_PLUGIN_DIR="$plugin" FAKE_SOURCE="$sandbox/source" FAKE_ASSET_FILE="$served"
  export FAKE_OMARCHY_LOG="$sandbox/omarchy.log"

  cat >"$stub_bin/omarchy" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$FAKE_OMARCHY_LOG"
if [[ ${1:-} == plugin && ${2:-} == add ]]; then
  mkdir -p "$FAKE_PLUGIN_DIR/.git"
  cp -R "$FAKE_SOURCE/." "$FAKE_PLUGIN_DIR/"
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
  printf '#!/usr/bin/env bash\nexit 0\n' >"$stub_bin/omarchy-shell"
  chmod 0755 "$stub_bin"/*
}

run_installer() {
  HOME="$fake_home" XDG_DATA_HOME="$sandbox/data" PATH="$stub_bin:$PATH" \
    bash "$installer_source" "$@" 2>&1
}
teardown() { [[ -n ${sandbox:-} && -d $sandbox ]] && rm -rf -- "$sandbox"; unset FAKE_PLUGIN_DIR FAKE_SOURCE FAKE_ASSET_FILE FAKE_OMARCHY_LOG FAKE_CURL_EXIT; }

announce "default recovery install uses Omarchy and prefetches outside checkout"
setup
output=$(run_installer); status=$?
assert_eq "$status" "0"
assert_contains "$output" "Nearby 1.2.0 installed"
assert_contains "$(<"$FAKE_OMARCHY_LOG")" "plugin add https://github.com/jfg96/omarchy-nearby --yes"
assert_contains "$(<"$FAKE_OMARCHY_LOG")" "plugin enable oma.nearby"
[[ ! -e $plugin/bin/omarchy-nearby-helper ]] || report "installer wrote an ELF into the checkout"
[[ -f $sandbox/data/omarchy-nearby/helpers/helper-v1.2.0/$asset ]] || report "helper was not prefetched"
teardown

announce "an existing clean installation is updated through Omarchy"
setup
mkdir -p "$plugin/.git"
cp -R "$FAKE_SOURCE/." "$plugin/"
output=$(run_installer); status=$?
assert_eq "$status" "0"
assert_contains "$(<"$FAKE_OMARCHY_LOG")" "plugin update oma.nearby --yes"
teardown

announce "specific stable tag is checked out and version checked"
setup
output=$(run_installer v1.2.0); status=$?
assert_eq "$status" "0"
assert_contains "$output" "Nearby 1.2.0 installed"
teardown

announce "download failure leaves the source checkout intact"
setup
export FAKE_CURL_EXIT=6
output=$(run_installer); status=$?
assert_eq "$status" "1"
assert_contains "$output" "could not prepare"
[[ -f $plugin/manifest.json ]] || report "source checkout disappeared"
teardown

announce "invalid release argument fails before installation"
setup
output=$(run_installer main); status=$?
assert_eq "$status" "1"
assert_contains "$output" "release must look like"
[[ ! -e $plugin ]] || report "invalid request changed installation"
teardown

if (( failures )); then
  printf 'installer tests failed: %s\n' "$failures" >&2
  exit 1
fi
echo "installer tests passed"
