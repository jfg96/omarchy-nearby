#!/usr/bin/env bash

# bin/nearby-preview-image, with `file` and `magick` stubbed.

set -uo pipefail

tests_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly preview_image="$tests_dir/../bin/nearby-preview-image"
failures=0
current=""

announce() { current="$1"; }
report() { printf '  FAIL: %s\n        %s\n' "$current" "$1" >&2; failures=$((failures + 1)); }
assert_eq() { [[ $1 == "$2" ]] || report "expected '$2', got '$1'"; }

# The type is chosen by the test and the decoder records how it was called,
# then writes its stdin, prefixed, to the PNG named by its last argument.
setup() {
  sandbox=$(mktemp -d "${TMPDIR:-/tmp}/nearby-preview-test.XXXXXX")
  stub_bin="$sandbox/stub-bin"
  mkdir -p "$stub_bin" "$sandbox/files" "$sandbox/runtime"
  chmod 0700 "$sandbox/runtime"
  export FAKE_MIME="" FAKE_MAGICK_EXIT=0 MAGICK_LOG="$sandbox/magick"
  cat >"$stub_bin/file" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$FAKE_MIME"
STUB
  cat >"$stub_bin/magick" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$*" >"$MAGICK_LOG"
(( FAKE_MAGICK_EXIT == 0 )) || exit "$FAKE_MAGICK_EXIT"
output=""
for argument in "$@"; do output="$argument"; done
{ printf 'png:'; cat; } >"${output#png:}"
STUB
  chmod 0755 "$stub_bin/"*
}
run_preview() { XDG_RUNTIME_DIR="$sandbox/runtime" PATH="$stub_bin:$PATH" "$preview_image" "$@" 2>/dev/null; }
teardown() { [[ -n ${sandbox:-} && -d $sandbox ]] && rm -rf -- "$sandbox"; unset FAKE_MIME FAKE_MAGICK_EXIT MAGICK_LOG; }
previews() { find "$sandbox/runtime/omarchy-nearby/previews" -mindepth 1 2>/dev/null | wc -l; }

announce "a HEIC photo becomes a small upright PNG in a private runtime folder"
setup
printf 'heic-bytes' >"$sandbox/files/IMG_0001.HEIC"
output=$(FAKE_MIME=image/heic run_preview "$sandbox/files/IMG_0001.HEIC"); status=$?
assert_eq "$status" "0"
[[ $output == "$sandbox/runtime/omarchy-nearby/previews/preview-"*.png ]] || report "unexpected preview path: $output"
assert_eq "$(<"$output")" "png:heic-bytes"
arguments=$(<"$MAGICK_LOG")
[[ $arguments == *" heic:-[0] -auto-orient -thumbnail 1024x1024> -strip "* ]] \
  || report "expected a bounded, oriented heic thumbnail in: $arguments"
[[ $arguments == *"-limit area 128MP"*"-limit disk 0"* ]] || report "expected decoder limits in: $arguments"
assert_eq "$(stat -c %a "$sandbox/runtime/omarchy-nearby")" "700"
assert_eq "$(stat -c %a "$sandbox/runtime/omarchy-nearby/previews")" "700"
teardown

announce "every converted type names its own decoder and never the file name"
for case in image/heif:heic image/avif:avif image/jxl:jxl; do
  setup
  odd="$sandbox/files/msl:odd [1] %d.png"
  printf 'bytes' >"$odd"
  FAKE_MIME=${case%%:*} run_preview "$odd" >/dev/null; status=$?
  assert_eq "$status" "0"
  arguments=$(<"$MAGICK_LOG")
  [[ $arguments == *" ${case#*:}:-[0] "* ]] || report "expected the ${case#*:} decoder for ${case%%:*} in: $arguments"
  [[ $arguments != *"odd"* ]] || report "the file name reached ImageMagick: $arguments"
  teardown
done

announce "a new preview replaces the previous one"
setup
printf 'one' >"$sandbox/files/a.heic"
printf 'two' >"$sandbox/files/b.heic"
first=$(FAKE_MIME=image/heic run_preview "$sandbox/files/a.heic")
second=$(FAKE_MIME=image/heic run_preview "$sandbox/files/b.heic")
[[ $first != "$second" ]] || report "each preview needs its own name so the panel reloads it"
[[ ! -e $first ]] || report "the previous preview must be removed"
assert_eq "$(<"$second")" "png:two"
assert_eq "$(previews)" "1"
teardown

announce "a type the shell draws itself, or that is not a picture, is never decoded"
for type in image/png image/jpeg image/svg+xml application/pdf; do
  setup
  printf 'x' >"$sandbox/files/thing"
  FAKE_MIME=$type run_preview "$sandbox/files/thing" >/dev/null; status=$?
  assert_eq "$status" "1"
  [[ ! -e $MAGICK_LOG ]] || report "$type must not reach ImageMagick"
  teardown
done

announce "a picture that cannot be decoded leaves no preview behind"
setup
printf 'broken' >"$sandbox/files/broken.heic"
output=$(FAKE_MIME=image/heic FAKE_MAGICK_EXIT=1 run_preview "$sandbox/files/broken.heic"); status=$?
assert_eq "$status" "1"
assert_eq "$output" ""
assert_eq "$(previews)" "0"
teardown

announce "a runtime folder that is a symlink is refused"
setup
mkdir "$sandbox/elsewhere"
ln -s "$sandbox/elsewhere" "$sandbox/runtime/omarchy-nearby"
printf 'x' >"$sandbox/files/a.heic"
FAKE_MIME=image/heic run_preview "$sandbox/files/a.heic" >/dev/null; status=$?
assert_eq "$status" "1"
assert_eq "$(find "$sandbox/elsewhere" -mindepth 1 | wc -l)" "0"
teardown

announce "missing, relative and extra arguments, or no runtime folder, are refused"
setup
run_preview "$sandbox/files/absent.heic"; assert_eq "$?" "1"
( cd "$sandbox/files" && printf 'x' >here.heic && FAKE_MIME=image/heic run_preview here.heic ); assert_eq "$?" "1"
run_preview; assert_eq "$?" "1"
FAKE_MIME=image/heic run_preview "$sandbox/files/here.heic" extra; assert_eq "$?" "1"
FAKE_MIME=image/heic XDG_RUNTIME_DIR="" PATH="$stub_bin:$PATH" "$preview_image" "$sandbox/files/here.heic" >/dev/null 2>&1
assert_eq "$?" "1"
[[ ! -e $MAGICK_LOG ]] || report "a refused request must not decode anything"
teardown

if (( failures )); then
  printf 'preview image tests failed: %s\n' "$failures" >&2
  exit 1
fi
echo "preview image tests passed"
