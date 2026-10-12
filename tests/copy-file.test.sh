#!/usr/bin/env bash

# bin/nearby-copy-file, without touching the real clipboard.

set -uo pipefail

tests_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly copy_file="$tests_dir/../bin/nearby-copy-file"
failures=0
current=""

announce() { current="$1"; }
report() { printf '  FAIL: %s\n        %s\n' "$current" "$1" >&2; failures=$((failures + 1)); }
assert_eq() { [[ $1 == "$2" ]] || report "expected '$2', got '$1'"; }

# `file`, `magick` and `wl-copy` are stubbed: the type is chosen by the test,
# the decoder records how it was called, and the clipboard is a pair of files.
setup() {
  sandbox=$(mktemp -d "${TMPDIR:-/tmp}/nearby-copy-test.XXXXXX")
  stub_bin="$sandbox/stub-bin"
  mkdir -p "$stub_bin" "$sandbox/files" "$sandbox/runtime"
  export FAKE_MIME="" FAKE_MAGICK_EXIT=0 CLIPBOARD="$sandbox/clipboard"
  cat >"$stub_bin/file" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$FAKE_MIME"
STUB
  cat >"$stub_bin/magick" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$*" >"$CLIPBOARD.magick"
(( FAKE_MAGICK_EXIT == 0 )) || exit "$FAKE_MAGICK_EXIT"
output=""
for argument in "$@"; do output="$argument"; done
{ printf 'png:'; cat; } >"${output#png:}"
STUB
  cat >"$stub_bin/wl-copy" <<'STUB'
#!/usr/bin/env bash
printf '%s' "$2" >"$CLIPBOARD.type"
cat >"$CLIPBOARD.data"
STUB
  chmod 0755 "$stub_bin/"*
}
run_copy() { XDG_RUNTIME_DIR="$sandbox/runtime" PATH="$stub_bin:$PATH" "$copy_file" "$@" 2>/dev/null; }
teardown() { [[ -n ${sandbox:-} && -d $sandbox ]] && rm -rf -- "$sandbox"; unset FAKE_MIME FAKE_MAGICK_EXIT CLIPBOARD; }
leftovers() { find "$sandbox/runtime" -mindepth 1 | wc -l; }

announce "a PNG is encoded afresh, so its metadata does not reach the clipboard"
setup
printf 'png-bytes' >"$sandbox/files/shot.png"
FAKE_MIME=image/png run_copy "$sandbox/files/shot.png"; status=$?
assert_eq "$status" "0"
assert_eq "$(<"$CLIPBOARD.type")" "image/png"
assert_eq "$(<"$CLIPBOARD.data")" "png:png-bytes"
arguments=$(<"$CLIPBOARD.magick")
[[ $arguments == *" png:-[0] "*" -strip "* ]] || report "expected the png decoder and -strip in: $arguments"
teardown

announce "a PNG that cannot be converted is copied as a file, never as its own bytes"
setup
printf 'png-bytes' >"$sandbox/files/shot.png"
FAKE_MIME=image/png FAKE_MAGICK_EXIT=1 run_copy "$sandbox/files/shot.png"; status=$?
assert_eq "$status" "0"
assert_eq "$(<"$CLIPBOARD.type")" "text/uri-list"
assert_eq "$(<"$CLIPBOARD.data")" "file://$sandbox/files/shot.png"$'\r'
teardown

announce "without ImageMagick a picture is copied as a file"
setup
rm -- "$stub_bin/magick"
mkdir "$sandbox/tools"
for tool in env bash cat mktemp rm timeout; do ln -s "$(command -v "$tool")" "$sandbox/tools/$tool"; done
printf 'png-bytes' >"$sandbox/files/shot.png"
FAKE_MIME=image/png XDG_RUNTIME_DIR="$sandbox/runtime" PATH="$stub_bin:$sandbox/tools" \
  "$copy_file" "$sandbox/files/shot.png" 2>/dev/null; status=$?
assert_eq "$status" "0"
assert_eq "$(<"$CLIPBOARD.type")" "text/uri-list"
assert_eq "$(<"$CLIPBOARD.data")" "file://$sandbox/files/shot.png"$'\r'
teardown

announce "a JPEG is converted and copied as PNG, with nothing left behind"
setup
printf 'jpeg-bytes' >"$sandbox/files/photo.jpg"
FAKE_MIME=image/jpeg run_copy "$sandbox/files/photo.jpg"; status=$?
assert_eq "$status" "0"
assert_eq "$(<"$CLIPBOARD.type")" "image/png"
assert_eq "$(<"$CLIPBOARD.data")" "png:jpeg-bytes"
assert_eq "$(leftovers)" "0"
teardown

announce "the decoder is named by the detected type and never sees the file name"
setup
odd="$sandbox/files/msl:odd [1] %d.png"
printf 'gif-bytes' >"$odd"
FAKE_MIME=image/gif run_copy "$odd"; status=$?
assert_eq "$status" "0"
arguments=$(<"$CLIPBOARD.magick")
[[ $arguments == *" gif:-[0] "* ]] || report "expected the gif decoder on stdin in: $arguments"
[[ $arguments != *"odd"* ]] || report "the file name reached ImageMagick: $arguments"
teardown

announce "a picture that cannot be decoded is copied as a file, not as an empty image"
setup
printf 'broken' >"$sandbox/files/broken.jpg"
FAKE_MIME=image/jpeg FAKE_MAGICK_EXIT=1 run_copy "$sandbox/files/broken.jpg"; status=$?
assert_eq "$status" "0"
assert_eq "$(<"$CLIPBOARD.type")" "text/uri-list"
assert_eq "$(<"$CLIPBOARD.data")" "file://$sandbox/files/broken.jpg"$'\r'
assert_eq "$(leftovers)" "0"
teardown

announce "any other file is copied as an escaped file reference"
setup
mkdir -p "$sandbox/files/a b"
other="$sandbox/files/a b/é#?%.pdf"
printf 'pdf' >"$other"
FAKE_MIME=application/pdf run_copy "$other"; status=$?
assert_eq "$status" "0"
assert_eq "$(<"$CLIPBOARD.type")" "text/uri-list"
assert_eq "$(<"$CLIPBOARD.data")" "file://$sandbox/files/a%20b/%C3%A9%23%3F%25.pdf"$'\r'
[[ ! -e $CLIPBOARD.magick ]] || report "a file that is not a picture must not reach ImageMagick"
teardown

announce "a type the preview does not cover is never decoded"
setup
printf '<svg/>' >"$sandbox/files/drawing.svg"
FAKE_MIME=image/svg+xml run_copy "$sandbox/files/drawing.svg"; status=$?
assert_eq "$status" "0"
assert_eq "$(<"$CLIPBOARD.type")" "text/uri-list"
[[ ! -e $CLIPBOARD.magick ]] || report "only PNG, JPEG, WebP and GIF may be decoded"
teardown

announce "missing, relative and extra arguments are refused before the clipboard changes"
setup
run_copy "$sandbox/files/absent.png"; assert_eq "$?" "1"
( cd "$sandbox/files" && printf 'x' >here.png && run_copy here.png ); assert_eq "$?" "1"
run_copy; assert_eq "$?" "1"
run_copy "$sandbox/files/here.png" extra; assert_eq "$?" "1"
[[ ! -e $CLIPBOARD.type ]] || report "a refused request must leave the clipboard alone"
teardown

if (( failures )); then
  printf 'copy file tests failed: %s\n' "$failures" >&2
  exit 1
fi
echo "copy file tests passed"
