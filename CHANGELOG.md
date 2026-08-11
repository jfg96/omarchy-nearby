# Changelog

## 1.0.4

- Fix copying received text with the Quickshell `Process` API and keep repeated
  clipboard copies working.
- Queue incoming transfer requests so simultaneous approvals are handled in
  order instead of replacing one another.
- Support sending files and clipboard text to PIN-protected LocalSend receivers,
  including clear retry, cancellation, and rate-limit feedback.
- Keep outgoing clipboard contents in memory rather than writing plaintext to a
  temporary file.
- Isolate incoming, outgoing, PIN, and deferred-text state across cancellation,
  expiry, helper restarts, popup visibility changes, and late transfer events.
- Improve received-text and terminal-state navigation for both mouse and keyboard,
  with clearer wrapped status and error messages.
- Add comprehensive frontend state-transition coverage and run it in normal CI
  and release validation.

## 1.0.3

- Read the frontend version from the plugin manifest so strict helper version
  checks work with Omarchy/Quattro's curated runtime widget metadata.

## 1.0.2

- Correctly accept published, non-draft GitHub releases in the installer.

## 1.0.1

- Fix the release workflow installation of the third-party license generator.

## 1.0.0

- Initial public release.
- Native send/receive UI for Omarchy.
- LocalSend-compatible discovery and transfers.
- Multicast-first discovery with cache-first and subnet HTTP fallbacks.
- Incoming/outgoing progress, cancellation and robust file finalization.
- Optional GitHub Release helper for installing without a Rust toolchain.
- Strict plugin/helper version check with a clear recovery message.
- Backend restart attempts only after a helper has reached the ready state.
