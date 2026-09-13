# Repository guidance

These instructions apply to the entire repository. Prioritize correctness,
security, and user-visible regressions over stylistic preferences. Keep patches
minimal and do not mix unrelated refactors into bug fixes.

## Start here

- Check `git status --short` and the existing diff before editing; preserve
  unrelated work. Trace the affected behavior and its tests before choosing
  which layer to change.
- Read `README.md` for installation, supported runtime, lifecycle, and helper
  compatibility. Use `ROBUSTNESS.md` for the relevant manual regression cases.
- Before editing `backend/vendor/localsend-rs/`, read
  `VENDORED_LOCALSEND_RS.md`. The dependency is intentionally frozen; preserve
  local patches and document intentional divergences there in the same change.
  Keep Nearby-specific orchestration in the helper, not in the vendor.
- Consult `.github/workflows/ci.yml` for automated checks and
  `.github/workflows/release.yml` when working on releases. Keep the validation
  commands below aligned with those workflows.

## Architecture and compatibility

- `Service.qml` owns the helper process, the transfer state machine, and the
  `oma.nearby` IPC target. The shell loads a `service` kind once per session.
- `Panel.qml` is a per-monitor view: cursor, focus, and popup state belong
  there; shared transfer state and processes belong in `Service.qml`.
  `Model.js` contains shared JavaScript state helpers, also exercised by Node
  tests; keep it usable from both QML and Node.
- `backend/src/main.rs` owns the LocalSend integration and helper command/event
  loop; `backend/src/settings.rs` handles persistent receiver settings.
- The service and helper exchange newline-delimited JSON over stdin/stdout.
  Check both producers and consumers when changing commands or events. Preserve
  request/transfer correlation so late events cannot overwrite a newer transfer.
  Send helper diagnostics to stderr, keeping stdout for protocol messages.
- `bin/nearby-helper-launcher` strictly parses `helper-release.env`, verifies
  the exact helper size and SHA256, and stores published helpers under the
  user's XDG data directory. Keep downloads HTTPS-only, bounded, same-filesystem
  staged and atomically installed; never write a published ELF into the plugin
  checkout. `bin/nearby-repair-helper` only asks the launcher to prefetch.
- Installation and source updates belong exclusively to Omarchy's plugin
  manager. Do not add a curl-pipe installer or make plugin code modify its git
  checkout. `build.sh` installs an ignored local developer override into `bin/`;
  generated helper binaries must not be committed.
- Source updates can select a different helper through committed release
  metadata. Raise
  `manifest.json`'s `minHelperVersion` when a change requires a newer helper
  command or behavior; it is a compatibility floor, not the release version.
- Preserve compatibility with Omarchy Quattro and the LocalSend protocol.
- Keep receiving and passive discovery alive while enabled with no popup open.
  Active discovery follows the open views; closing one monitor's popup must not
  stop discovery needed by another. Disabling Nearby must stop the helper.
- Keep HTTP and HTTPS behavior distinct. Never weaken TLS certificate
  verification or fingerprint pinning to improve interoperability.
- Reuse the persistent Nearby TLS identity where client authentication is
  required; do not create unrelated transfer identities.
- Treat Quickshell `Process` startup, exit, stdin, and repeated-use behavior as
  separate lifecycle cases. Missing commands may fail without an exit code.
- Avoid new runtime dependencies unless they are necessary and available on a
  stock Omarchy installation.

## Validation

Run commands from the repository root. Select checks by the changed behavior:

- QML or shared JavaScript: run all three Node suites below. They exercise
  model logic and inspect/evaluate QML source; they do not launch Quickshell.
- Rust helper: run formatting, build checks, and helper tests. Vendor changes
  also require the separate vendored crate suite; helper tests do not replace it.
- Launcher or helper repair: run both Bash suites below. They
  use stubbed external commands to test immutable metadata, download policy,
  checksum verification, XDG storage, offline reuse, races and cleanup.
- Shell scripts: also run `bash -n` on the changed scripts. Syntax checks alone
  do not validate installation or updates on a live system.
- Documentation only: check referenced paths and commands and run
  `git diff --check`; runtime suites are unnecessary unless behavior also changes.

Before proposing a merge of runtime changes, run the complete supported suite
when practical:

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
```

Regression tests must exercise the reported failure and should fail against the
pre-fix behavior for the expected reason. Do not claim that Omarchy, Quickshell,
portals, peer discovery, or real-device transfers were tested when they were not.
Those runtime paths require explicit manual smoke testing on Omarchy and, for
interoperability, a real LocalSend peer.

Before handing off, inspect the final diff for unintended files and whitespace
errors. Report what changed, which checks actually ran and their results, and
any unverified runtime behavior or blockers. Do not present skipped checks as
passing.

## Reviews and releases

- Flag correctness bugs, security regressions, unnecessary dependencies, and
  unrelated scope growth. Do not request style-only refactors without a concrete
  maintenance or correctness benefit.
- Preserve contributor authorship and keep release preparation separate from a
  contributor's functional commits.
- Merge stacked pull requests in dependency order. After each merge, update the
  next branch against the current `main`, prefer rebasing when it is safe, and
  re-review the effective diff before merging it.
- Do not merge `main` into a contributor branch merely to refresh a stacked
  pull request's merge base. Avoid synchronization commits unless a concrete
  technical reason makes one necessary, and preserve contributor authorship
  when updating or rebasing the branch.
- Do not rewrite published `main` solely to make its history look cleaner.
- Stable plugin releases require matching `manifest.json`, changelog heading
  and `vX.Y.Z` tag. The helper package follows its own `helper-vX.Y.Z` cycle;
  its Cargo files, committed release metadata and published artifact must agree.
  Publish the helper first, independently verify its size, SHA256, attestation
  and version, then commit those exact values in `helper-release.env`. A plugin
  release must verify that reference and must not rebuild or republish the ELF.
- Runtime source changes after a stable release must advance the manifest to
  the next `-dev` version. Changes to helper behavior must also advance the
  helper package version and `minHelperVersion`; publish that helper first,
  then commit its exact release metadata. Repository-only documentation or CI
  changes that cannot alter the installed plugin do not require a version bump.
- Never create or move a release tag until the exact target commit has passed
  CI. Release helpers must come from the tagged GitHub Actions workflow, not a
  locally built binary. Helper releases stay prereleases so they cannot replace
  the latest stable plugin release in GitHub's default release view.
