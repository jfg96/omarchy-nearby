# Contributing

[Documentation index](README.md#documentation)

Read [repository guidance](AGENTS.md) before editing. Keep unrelated changes
separate and preserve existing work in the checkout.

## Development environment

Use Git, Bash, Node.js and a Rust toolchain with Cargo, rustfmt and Clippy.
[CI](.github/workflows/ci.yml) currently uses Rust `1.97.1` on Ubuntu 24.04;
use that version when reproducing CI results. A passing test run does not
establish that the UI works: live validation requires Omarchy Quattro and a
LocalSend-compatible peer.

## Build and run

For an installed development copy, use Omarchy's plugin manager, build the helper
in the installed checkout, then enable the plugin:

```sh
omarchy plugin add https://github.com/jfg96/omarchy-nearby.git --yes
cd ~/.config/omarchy/plugins/oma.nearby
./build.sh
omarchy plugin enable oma.nearby
```

If already installed, use the existing checkout instead of adding it again.
Check its branch and working changes before editing or updating it.

[build.sh](build.sh) runs a release Cargo build and installs
`bin/omarchy-nearby-helper` plus the marker `bin/.nearby-local-helper`.
Both are ignored by Git. The launcher gives this explicitly marked executable
priority over the published helper. Rebuild after helper source changes, then
turn the receiver off and back on to start the new executable.

The build script currently does not pass `--locked`; the validation commands
below do. Inspect any lockfile changes after building.

### Return to the published helper

From the installed plugin root, turn the receiver off and remove only the local
override marker:

```sh
omarchy-shell oma.nearby receiverOff
rm -- bin/.nearby-local-helper
omarchy-shell oma.nearby receiverOn
omarchy-shell oma.nearby status
```

Use the removal command only when the marker exists. The local executable can
remain on disk: without the marker, the launcher ignores it. Startup uses the
published helper pinned by the checkout, downloading it if needed.
A later `./build.sh` recreates the marker. **Retry helper** alone does not
disable a developer override.

## Repository map

| Path | Responsibility |
| --- | --- |
| [Panel.qml](Panel.qml) | Per-monitor popup, focus and selection |
| [Service.qml](Service.qml) | Shared state, helper lifecycle and shell commands |
| [Model.js](Model.js) | Shared model logic, usable in QML and Node |
| [backend/src/main.rs](backend/src/main.rs) | Network integration and JSON command/event loop |
| [backend/src/settings.rs](backend/src/settings.rs) | Persistent receiver security settings |
| [backend/src/identity.rs](backend/src/identity.rs) | Persistent TLS identity validation |
| [bin/nearby-helper-launcher](bin/nearby-helper-launcher) | Verify, cache and start the selected helper |
| [bin/nearby-repair-helper](bin/nearby-repair-helper) | Prefetch-based helper repair |
| [manifest.json](manifest.json) | Plugin metadata and helper compatibility floor |
| [helper-release.env](helper-release.env) | Immutable published helper reference |
| [tests](tests) | Frontend and distribution regression suites |

Read [architecture](docs/ARCHITECTURE.md) for ownership and protocol boundaries.
Before changing vendored code, read the
[vendor maintenance record](VENDORED_LOCALSEND_RS.md); update it with every
intentional divergence. Preserve the upstream documentation and license notices.

## Validation

Run from the repository root. Select checks by the changed behavior:

| Change | Required checks |
| --- | --- |
| QML or shared JavaScript | All three Node suites |
| Rust helper | Formatting, build check, Clippy and helper tests |
| Vendored Rust | Helper checks plus the separate vendor suite |
| Launcher or repair | Both Bash suites |
| Shell scripts | Relevant suites plus `bash -n` on changed scripts |
| Documentation only | Referenced paths/commands, links and `git diff --check` |

The complete supported automated suite is:

```sh
node tests/model.test.js
node tests/panel-state.test.js
node tests/service-state.test.js
bash tests/launcher.test.sh
bash tests/helper-repair.test.sh
cargo fmt --manifest-path backend/Cargo.toml --all -- --check
cargo check --locked --manifest-path backend/Cargo.toml
cargo clippy --all-targets --locked --manifest-path backend/Cargo.toml -- -D warnings
cargo test --locked --manifest-path backend/Cargo.toml
cargo test --locked --manifest-path backend/vendor/localsend-rs/Cargo.toml --features https
git diff --check
```

The Node suites inspect/evaluate QML and model logic; they do not launch
Quickshell. Distribution tests use stubbed external commands. Neither replaces
live installation, multi-monitor or real-device transfer testing.
Use [ROBUSTNESS.md](ROBUSTNESS.md) for manual cases and record exact results.

Regression tests should fail for the reported reason before the fix. Runtime
suites are unnecessary for a documentation-only change.

## Proposing changes

Describe the concrete problem, resulting behavior and checks actually performed.
Identify unverified runtime behavior rather than reporting skipped tests as
passing. Inspect the final diff for unintended files and whitespace errors.

Runtime changes after a stable release advance the plugin to the next `-dev`
version. Helper behavior changes also advance the helper package and compatibility
floor. Repository-only documentation or CI changes that cannot alter the installed
plugin do not require a version bump. Follow the
[release procedure](docs/RELEASING.md) before preparing tags or helper metadata.
