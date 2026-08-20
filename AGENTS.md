# Repository guidance

These instructions apply to the entire repository. Prioritize correctness,
security, and user-visible regressions over stylistic preferences. Keep patches
minimal and do not mix unrelated refactors into bug fixes.

## Architecture and compatibility

- `Service.qml` owns the helper process, the transfer state machine, and the
  `oma.nearby` IPC target. The shell loads a `service` kind once per session.
- `Panel.qml` and `Model.js` implement the Quickshell UI and its state model.
  The bar builds one `Panel.qml` per monitor, so it must stay a view: anything
  there can only be one of belongs in `Service.qml`.
- `backend/` contains the Rust helper. `backend/vendor/localsend-rs/` is a
  locally modified dependency and has its own independently runnable tests.
- Preserve compatibility with Omarchy Quattro and the LocalSend protocol.
- Keep HTTP and HTTPS behavior distinct. Never weaken TLS certificate
  verification or fingerprint pinning to improve interoperability.
- Reuse the persistent Nearby TLS identity where client authentication is
  required; do not create unrelated transfer identities.
- Treat Quickshell `Process` startup, exit, stdin, and repeated-use behavior as
  separate lifecycle cases. Missing commands may fail without an exit code.
- Avoid new runtime dependencies unless they are necessary and available on a
  stock Omarchy installation.

## Validation

Run the checks relevant to every changed area. Before proposing a merge, run the
complete supported suite when practical:

```sh
node tests/model.test.js
node tests/panel-state.test.js
node tests/service-state.test.js
cargo fmt --manifest-path backend/Cargo.toml --all -- --check
cargo check --locked --manifest-path backend/Cargo.toml
cargo test --locked --manifest-path backend/Cargo.toml
cargo test --locked --manifest-path backend/vendor/localsend-rs/Cargo.toml --features https
```

Regression tests must exercise the reported failure and should fail against the
pre-fix behavior for the expected reason. Do not claim that Omarchy, Quickshell,
portals, peer discovery, or real-device transfers were tested when they were not.
Those runtime paths require explicit manual smoke testing on Omarchy and, for
interoperability, a real LocalSend peer.

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
- Stable releases require matching versions in `manifest.json`,
  `backend/Cargo.toml`, and `backend/Cargo.lock`, plus a matching changelog
  heading and `vX.Y.Z` tag.
- Runtime source changes after a stable release must advance the manifest and
  helper to the next `-dev` version together. Repository-only documentation or
  CI changes that cannot alter the installed plugin do not require a version
  bump.
- Never create or move a release tag until the exact target commit has passed
  CI. Release helpers must come from the tagged GitHub Actions workflow, not a
  locally built binary.
