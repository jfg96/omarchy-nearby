# Changelog

## 1.1.1-dev

## 1.1.0

- Add a user-configured, persistent PIN for incoming text and file transfers.
  PIN changes take effect without restarting the receiver or interrupting an
  already authorized transfer, while normal Accept/Decline remains required.
- Match LocalSend's three-attempt receiver lockout semantics and keep failure
  tracking bounded per source IP.
- Accept and correctly encode general PIN values when sending to LocalSend,
  instead of restricting the prompt to numeric values.
- Store incoming PIN configuration in a private atomic settings file, load it
  before receiver startup and fail closed if that security state is invalid.

## 1.0.7

- Keep the helper's running state bound to the receiver setting across startup
  retries, so toggling Nearby off and on can recover after a port conflict.

## 1.0.6

- Report a confirmed local port conflict from a structured helper event instead
  of treating every failure before receiver startup as port 53317 being busy.
- Run one helper per shell session instead of one per monitor. The bar builds a
  widget for every screen, so on a multi-monitor setup each extra copy started
  its own helper, lost the race for the LocalSend port, and reported "Nearby
  backend could not start" while the first copy held the port and worked. The
  helper, the transfer state, and the `oma.nearby` IPC target now live in a
  `service` entry point that the shell loads once, and each bar widget is a
  view onto it. The IPC target was registered once per monitor for the same
  reason, and the shell kept only the first registration.
- Keep a receiver the user turned off turned off. The setting is now read from
  `shell.json` rather than pushed in by a bar widget. Widgets are built once per
  monitor and receive their settings a tick after they are created, so the first
  value one could report was the default rather than the persisted entry, and a
  persisted `receiverEnabled: false` was replaced by the default on every start:
  the helper ran, bound port 53317 and announced itself on the network. Reading
  the entry directly also means the persisted setting and the effective setting
  are the same value and cannot drift apart.
- Retry a helper that exits before it is ready, on the same bounded schedule
  used for one that stops later. A port conflict skipped the retries entirely
  and reported a permanent failure on the first exit.
- Say what actually failed when the receiver cannot start. A taken port now
  names port 53317 rather than advising a reinstall, which never frees it, and
  the reinstall advice moved to the case it fixes: a helper binary that is
  missing and never launches at all.

## 1.0.5

- Present the Nearby TLS identity when sending. LocalSend peers ask for a client
  certificate during the handshake, so outgoing HTTPS transfers to peers that
  require one no longer abort with `certificate required` and surface as
  "Transfer failed".
- Choose files with `omarchy-file-select` instead of `zenity`, which Omarchy does
  not ship. Selecting files opened nothing at all on a stock system.
- Report a helper command that never launched. Quickshell signals that by
  returning `running` to false without an exit code, so the previous checks for
  exit code 127 could not run and the failure was silent.

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
