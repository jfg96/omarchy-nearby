#!/usr/bin/env bash

# bin/nearby-clipboard-image, with wl-paste stubbed so the real clipboard is
# never read.

set -uo pipefail

tests_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly clipboard_image="$tests_dir/../bin/nearby-clipboard-image"
failures=0
current=""

announce() { current="$1"; }
report() { printf '  FAIL: %s\n        %s\n' "$current" "$1" >&2; failures=$((failures + 1)); }
assert_eq() { [[ $1 == "$2" ]] || report "expected '$2', got '$1'"; }

# The offered types are one per line in FAKE_TYPES; asking for a type prints
# FAKE_DATA, and every call is recorded.
setup() {
  sandbox=$(mktemp -d "${TMPDIR:-/tmp}/nearby-clipboard-test.XXXXXX")
  stub_bin="$sandbox/stub-bin"
  mkdir -p "$stub_bin" "$sandbox/runtime"
  chmod 0700 "$sandbox/runtime"
  export FAKE_TYPES="" FAKE_DATA="" FAKE_PASTE_EXIT=0 PASTE_LOG="$sandbox/paste"
  cat >"$stub_bin/wl-paste" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$PASTE_LOG"
if [[ $1 == --list-types ]]; then
  [[ -n $FAKE_TYPES ]] || { echo 'Nothing is copied' >&2; exit 1; }
  printf '%s\n' "$FAKE_TYPES"
  exit 0
fi
printf '%s' "$FAKE_DATA"
exit "$FAKE_PASTE_EXIT"
STUB
  chmod 0755 "$stub_bin/wl-paste"
}
run_clipboard() { XDG_RUNTIME_DIR="$sandbox/runtime" PATH="$stub_bin:$PATH" "$clipboard_image" "$@" 2>/dev/null; }
teardown() { [[ -n ${sandbox:-} && -d $sandbox ]] && rm -rf -- "$sandbox"; unset FAKE_TYPES FAKE_DATA FAKE_PASTE_EXIT PASTE_LOG; }
saved() { find "$sandbox/runtime/omarchy-nearby/clipboard" -mindepth 1 2>/dev/null | wc -l; }

announce "a screenshot is saved as a PNG named after the moment it was sent"
setup
output=$(FAKE_TYPES=image/png FAKE_DATA=png-bytes run_clipboard); status=$?
assert_eq "$status" "0"
[[ $output =~ ^$sandbox/runtime/omarchy-nearby/clipboard/clipboard-[0-9]{8}-[0-9]{6}\.png$ ]] \
  || report "unexpected path: $output"
assert_eq "$(<"$output")" "png-bytes"
grep -qxF -- "--type image/png" "$PASTE_LOG" || report "expected the PNG to be requested: $(<"$PASTE_LOG")"
assert_eq "$(stat -c %a "$sandbox/runtime/omarchy-nearby/clipboard")" "700"
assert_eq "$(saved)" "1"
teardown

announce "PNG is preferred, and other picture types keep their own extension"
setup
output=$(FAKE_TYPES=$'text/html\nimage/jpeg\nimage/png' FAKE_DATA=x run_clipboard)
[[ $output == *.png ]] || report "PNG must win over JPEG: $output"
output=$(FAKE_TYPES=$'text/html\nimage/jpeg' FAKE_DATA=x run_clipboard)
[[ $output == *.jpg ]] || report "a JPEG must keep a .jpg name: $output"
assert_eq "$(saved)" "1"
teardown

announce "text on the clipboard wins, so the panel sends it as text"
for types in $'text/plain;charset=utf-8\nimage/png' $'image/png\ntext/plain' $'UTF8_STRING\nimage/bmp' 'TEXT'; do
  setup
  output=$(FAKE_TYPES=$types FAKE_DATA=x run_clipboard); status=$?
  assert_eq "$status" "3"
  assert_eq "$output" ""
  assert_eq "$(saved)" "0"
  teardown
done

announce "an empty clipboard or one without a picture is left to the text read"
for types in "" "text/html" "application/x-thing"; do
  setup
  FAKE_TYPES=$types run_clipboard >/dev/null; assert_eq "$?" "3"
  teardown
done

announce "a picture type offered by a client is never used as a file name"
setup
output=$(FAKE_TYPES='image/../../evil' FAKE_DATA=x run_clipboard); status=$?
assert_eq "$status" "3"
assert_eq "$(find "$sandbox" -name 'evil*' | wc -l)" "0"
teardown

announce "a failed or empty read leaves nothing behind"
setup
FAKE_TYPES=image/png FAKE_DATA=partial FAKE_PASTE_EXIT=1 run_clipboard >/dev/null; assert_eq "$?" "1"
assert_eq "$(saved)" "0"
FAKE_TYPES=image/png FAKE_DATA="" run_clipboard >/dev/null; assert_eq "$?" "1"
assert_eq "$(saved)" "0"
teardown

announce "a picture over the size limit is refused"
setup
big="$sandbox/big"
head -c $((100 * 1024 * 1024 + 1)) /dev/zero >"$big"
cat >"$stub_bin/wl-paste" <<STUB
#!/usr/bin/env bash
[[ \$1 == --list-types ]] && { echo image/png; exit 0; }
cat "$big"
STUB
run_clipboard >/dev/null; assert_eq "$?" "1"
assert_eq "$(saved)" "0"
teardown

announce "a new picture replaces the previous one"
setup
first=$(FAKE_TYPES=image/png FAKE_DATA=one run_clipboard)
mv -- "$first" "${first%/*}/clipboard-19990101-000000.png"
second=$(FAKE_TYPES=image/png FAKE_DATA=two run_clipboard)
assert_eq "$(saved)" "1"
assert_eq "$(<"$second")" "two"
teardown

announce "a runtime folder that is a symlink is refused"
setup
mkdir "$sandbox/elsewhere"
ln -s "$sandbox/elsewhere" "$sandbox/runtime/omarchy-nearby"
FAKE_TYPES=image/png FAKE_DATA=x run_clipboard >/dev/null; assert_eq "$?" "1"
assert_eq "$(find "$sandbox/elsewhere" -mindepth 1 | wc -l)" "0"
teardown

announce "without wl-paste the text read reports what is missing"
setup
rm -- "$stub_bin/wl-paste"
mkdir "$sandbox/tools"
for tool in env bash; do ln -s "$(command -v "$tool")" "$sandbox/tools/$tool"; done
XDG_RUNTIME_DIR="$sandbox/runtime" PATH="$sandbox/tools" "$clipboard_image" >/dev/null 2>&1
assert_eq "$?" "3"
teardown

announce "arguments and a missing runtime folder are refused"
setup
FAKE_TYPES=image/png run_clipboard extra >/dev/null; assert_eq "$?" "1"
FAKE_TYPES=image/png FAKE_DATA=x XDG_RUNTIME_DIR="" PATH="$stub_bin:$PATH" "$clipboard_image" >/dev/null 2>&1
assert_eq "$?" "1"
teardown

if (( failures )); then
  printf 'clipboard image tests failed: %s\n' "$failures" >&2
  exit 1
fi
echo "clipboard image tests passed"
