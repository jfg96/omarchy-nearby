# Changelog

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
