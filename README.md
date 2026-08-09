# Omarchy Nearby

Native nearby sharing for Omarchy, compatible with the LocalSend protocol.

Nearby is not a launcher for the LocalSend application. It implements the network
peer inside an Omarchy/Quickshell widget: discovery, sending, receiving, approval,
progress, clipboard text and transfer lifecycle are handled by the widget and its
Rust helper.

> Independent project. Not affiliated with or endorsed by LocalSend or Omarchy.

## Features

- Native Omarchy bar widget and popup
- Discover compatible devices on the local network
- Send one or more files
- Send clipboard text
- Receive files and text
- Accept/decline incoming transfers
- Transfer progress, completion, cancellation and error states
- HTTPS LocalSend interoperability with certificate fingerprint handling
- Passive discovery while Nearby is enabled
- Active discovery only while the popup is open
- Cache-first HTTP fallback when multicast discovery does not confirm a known peer
- Incremental subnet fallback with a global concurrency limit
- Files saved safely using temporary partial files and atomic finalization

## Discovery model

Nearby deliberately uses a cheap-first discovery hierarchy:

1. Passive listener while Nearby is enabled.
2. Immediate multicast announcement when the popup opens.
3. Short grace period for multicast/register responses.
4. Direct HTTP probes of recently known peer IPs as hints, never as proof of availability.
5. Full subnet HTTP scan only as a last resort.

Cached peers are revalidated over the network before being treated as confirmed.

### iOS note

During testing, the official LocalSend Android client responded to active discovery
immediately. The iOS client can enter a state where it remains visibly open but does
not confirm the multicast discovery round. In that case Nearby falls back to a cached
IP probe, or to a subnet scan when no usable cache entry exists. This is documented as
an interoperability limitation rather than worked around by making the normal path
more aggressive.

## Requirements

Runtime:

- Omarchy with the Quickshell-based shell/plugin system
- `zenity` for file selection
- `wl-clipboard` (`wl-copy` / `wl-paste`) for text sharing
- `libnotify` (`notify-send`) for desktop notifications
- Local network access to TCP/UDP port `53317`

Build:

- Rust toolchain with Cargo

## Build

The repository intentionally does **not** track a machine-built helper binary.
Build it locally:

```sh
./build.sh
```

This creates:

```text
bin/omarchy-nearby-helper
```

## Install

Install the repository with Omarchy's plugin manager without enabling it yet:

```sh
omarchy plugin add https://github.com/jfg96/omarchy-nearby.git --yes
```

Nearby deliberately does not distribute a machine-built helper binary. Build the
helper inside the managed plugin checkout, then enable the widget:

```sh
cd ~/.config/omarchy/plugins/oma.nearby
./build.sh
omarchy plugin enable oma.nearby
```

The widget is placed in the right section of the bar by default. Omarchy manages
updates and removal with `omarchy plugin update oma.nearby` and
`omarchy plugin remove oma.nearby`.

Received files are written to the user's Downloads directory according to the
backend's current save-path logic.

## Lifecycle

- **Nearby off:** helper/backend stopped; no receiver or discovery activity.
- **Nearby on, popup closed:** receiver and passive discovery remain available.
- **Nearby on, popup open:** active discovery is added.
- **Popup closed:** active discovery/fallback work is cancelled without disabling receiving.

## Architecture

```text
Quickshell UI (Panel.qml)
        │
        ├── Model.js
        │
        └── JSON over stdin/stdout
                 │
                 ▼
       omarchy-nearby-helper
              (Rust)
                 │
                 ▼
        LocalSend-compatible LAN peer
```

The Rust helper vendors a modified `localsend-rs` library. See
`THIRD_PARTY_NOTICES.md` for attribution and licensing information.

## Tests

Frontend model tests:

```sh
node tests/model.test.js
```

Backend tests:

```sh
cargo test --manifest-path backend/Cargo.toml
cargo test --manifest-path backend/vendor/localsend-rs/Cargo.toml --features https
```

A larger manual interoperability and robustness checklist is kept in
`ROBUSTNESS.md`.

## Security notes

Nearby validates transfer paths and writes incoming files through temporary partial
files before finalization. TLS fingerprints are used for peer connections where the
LocalSend protocol exposes them.

LocalSend discovery itself is LAN discovery and should not be treated as an
authenticated pairing mechanism. Use Nearby on networks you trust.

## License

Nearby is MIT licensed. See `LICENSE`.

The vendored `localsend-rs` code is separately MIT licensed and carries its own
notices. See `THIRD_PARTY_NOTICES.md`.
