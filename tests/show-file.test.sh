#!/usr/bin/env bash

# bin/nearby-show-file, without starting a real file manager.

set -uo pipefail

tests_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly show_file="$tests_dir/../bin/nearby-show-file"
failures=0
current=""

announce() { current="$1"; }
report() { printf '  FAIL: %s\n        %s\n' "$current" "$1" >&2; failures=$((failures + 1)); }
assert_eq() { [[ $1 == "$2" ]] || report "expected '$2', got '$1'"; }

# `gdbus` and `xdg-open` are stubbed: each records its arguments one per line,
# and the file manager can be made unavailable.
setup() {
  sandbox=$(mktemp -d "${TMPDIR:-/tmp}/nearby-show-test.XXXXXX")
  stub_bin="$sandbox/stub-bin"
  mkdir -p "$stub_bin" "$sandbox/files"
  export FAKE_GDBUS_EXIT=0 CALLS="$sandbox/calls"
  cat >"$stub_bin/gdbus" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' gdbus "$@" >>"$CALLS"
exit "$FAKE_GDBUS_EXIT"
STUB
  cat >"$stub_bin/xdg-open" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' xdg-open "$@" >>"$CALLS"
STUB
  chmod 0755 "$stub_bin/"*
}
run_show() { PATH="$stub_bin:$PATH" "$show_file" "$@" 2>/dev/null; }
teardown() { [[ -n ${sandbox:-} && -d $sandbox ]] && rm -rf -- "$sandbox"; unset FAKE_GDBUS_EXIT CALLS; }
gdbus_method() { sed -n 's/^org\.freedesktop\.FileManager1\.//p' "$CALLS"; }
gdbus_uri() { grep "^\['file://" "$CALLS"; }

announce "a received file is selected in the file manager, not opened"
setup
printf 'x' >"$sandbox/files/photo.jpg"
run_show item "$sandbox/files/photo.jpg"; status=$?
assert_eq "$status" "0"
assert_eq "$(gdbus_method)" "ShowItems"
assert_eq "$(gdbus_uri)" "['file://$sandbox/files/photo.jpg']"
grep -q '^xdg-open' "$CALLS" && report "a reachable file manager must be the only thing started"
teardown

announce "several loose files open the save folder itself"
setup
run_show folder "$sandbox/files"; status=$?
assert_eq "$status" "0"
assert_eq "$(gdbus_method)" "ShowFolders"
assert_eq "$(gdbus_uri)" "['file://$sandbox/files']"
teardown

announce "a hostile name is escaped and cannot leave the D-Bus argument"
setup
name=$'it\'s "$(touch pwned)" ñ %#.jpg'
printf 'x' >"$sandbox/files/$name"
run_show item "$sandbox/files/$name"; status=$?
assert_eq "$status" "0"
assert_eq "$(gdbus_uri)" "['file://$sandbox/files/it%27s%20%22%24%28touch%20pwned%29%22%20%C3%B1%20%25%23.jpg']"
[[ ! -e pwned && ! -e $sandbox/files/pwned ]] || report "the name must never be run"
teardown

announce "without a file manager, a file's folder is opened with xdg-open"
setup
printf 'x' >"$sandbox/files/photo.jpg"
FAKE_GDBUS_EXIT=1 run_show item "$sandbox/files/photo.jpg"; status=$?
assert_eq "$status" "0"
assert_eq "$(sed -n '/^xdg-open$/,$p' "$CALLS" | tail -n +2)" "$sandbox/files"
teardown

announce "without a file manager, a received folder is opened as itself"
setup
mkdir "$sandbox/files/Trip"
FAKE_GDBUS_EXIT=1 run_show item "$sandbox/files/Trip"; status=$?
assert_eq "$status" "0"
assert_eq "$(sed -n '/^xdg-open$/,$p' "$CALLS" | tail -n +2)" "$sandbox/files/Trip"
teardown

announce "missing, relative, mistyped and extra arguments start nothing"
setup
printf 'x' >"$sandbox/files/here.jpg"
run_show item "$sandbox/files/absent.jpg"; assert_eq "$?" "1"
( cd "$sandbox/files" && run_show item here.jpg ); assert_eq "$?" "1"
run_show folder "$sandbox/files/here.jpg"; assert_eq "$?" "1"
run_show open "$sandbox/files/here.jpg"; assert_eq "$?" "1"
run_show item; assert_eq "$?" "1"
run_show item "$sandbox/files/here.jpg" extra; assert_eq "$?" "1"
[[ ! -e $CALLS ]] || report "a refused request must not reach the file manager"
teardown

if (( failures )); then
  printf 'show file tests failed: %s\n' "$failures" >&2
  exit 1
fi
echo "show file tests passed"
