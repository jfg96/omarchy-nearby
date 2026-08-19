# Omarchy Nearby

Native nearby sharing for Omarchy Quattro, compatible with the LocalSend protocol.

Nearby turns LocalSend-compatible sharing into a native shell feature: open it
from the bar, choose a device, and send files or clipboard text without launching
a separate application.

Nearby is not a launcher for the LocalSend application. It implements the network
peer inside a Quickshell widget: discovery, sending, receiving, approval, progress,
clipboard text and transfer lifecycle are handled by the widget and its Rust helper.

> Independent project. Not affiliated with or endorsed by LocalSend or Omarchy.

## Screenshots

### Discover devices

![Nearby discovering compatible devices on the local network](assets/screenshots/nearby-discovery.png)

### Send files or clipboard text

![Nearby actions for sending files or clipboard text to a selected device](assets/screenshots/nearby-send-actions.png)

## Install

Download and run the installer:

```sh
curl -fsSL https://raw.githubusercontent.com/jfg96/omarchy-nearby/main/install.sh \
  -o /tmp/omarchy-nearby-install.sh
bash /tmp/omarchy-nearby-install.sh
```

The installer selects a published release, installs the plugin at that exact tag,
downloads its matching Linux x86_64 helper, verifies the SHA256, and enables
`oma.nearby`. Nearby never requests administrator privileges. The installer does
not install packages, require Rust, or compile anything locally.

Install or reinstall a specific release with:

```sh
bash /tmp/omarchy-nearby-install.sh v1.0.4
```

The widget is placed in the right section of the bar by default. Remove it with
`omarchy plugin remove oma.nearby`.

## Features

- Discover compatible devices on the local network
- Send one or more files
- Send clipboard text
- Receive files and text with explicit accept/decline controls
- Optionally require a persistent PIN for incoming text and file transfers
- Transfer progress, completion, cancellation and error states
- Native Omarchy bar widget and popup
- HTTPS LocalSend interoperability with certificate fingerprint handling
- Receiver and passive discovery remain available while enabled; active discovery
  is limited to the open popup

## Requirements

Runtime:

- Omarchy Quattro (Quickshell-based shell)
- `omarchy-file-select` for file selection, which Omarchy ships
- `wl-clipboard` (`wl-copy` / `wl-paste`) for text sharing
- `libnotify` (`notify-send`) for desktop notifications
- Local network connectivity for LocalSend traffic (default TCP/UDP port `53317`)

The recommended prebuilt installation supports Linux x86_64. Building from source
requires a Rust toolchain with Cargo.

## Lifecycle

- **Nearby off:** helper/backend stopped; no receiver or discovery activity.
- **Nearby on, popup closed:** receiver and passive discovery remain available.
- **Nearby on, popup open:** active discovery is added.
- **Popup closed:** active discovery and fallback work are cancelled without
  disabling receiving.

Received files are written to the user's Downloads directory. Interrupted receives
use `.nearby-*.part` files. On startup, Nearby removes only its own partial files
that are older than 24 hours.

## Incoming PIN protection

Open **Incoming PIN** from Nearby's device list to enable, change or disable
receiver PIN protection. Nearby accepts 1–64 ASCII letters, numbers, dots,
underscores, tildes and hyphens for an incoming PIN so official LocalSend
senders can transmit it reliably.

The sender must provide the correct PIN before Nearby displays the transfer,
but the local user must still explicitly Accept or Decline it. Changes apply to
new requests immediately and do not restart discovery or interrupt an already
authorized transfer. Nearby stores the PIN in
`$XDG_STATE_HOME/omarchy-nearby/settings.json` (falling back to
`~/.local/state/omarchy-nearby/settings.json`) with user-only permissions and
never displays the saved value again.

## Architecture

```text
Bar widget (Panel.qml)      one per monitor, view only
        │
        │  reads state, calls methods
        ▼
Nearby engine (Service.qml) one per shell session
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

The bar builds one widget per screen, while the helper binds a single
LocalSend port and the `oma.nearby` IPC target can only be registered once. The
engine is therefore a `service` entry point, which the shell loads once per
session, and every bar widget is a view onto it holding nothing but its own
cursor and popup.

The Rust helper vendors a modified `localsend-rs` library. See
`THIRD_PARTY_NOTICES.md` for attribution and licensing information, and
`VENDORED_LOCALSEND_RS.md` for the local patch and maintenance record.

## Discovery model

Nearby deliberately uses a cheap-first discovery hierarchy:

1. Passive listener while Nearby is enabled.
2. Immediate multicast announcement when the popup opens.
3. Short grace period for multicast/register responses.
4. Direct HTTP probes of recently known peer IPs as hints, never as proof of availability.
5. Full subnet HTTP scan only as a last resort.

Cached peers are revalidated over the network before being treated as confirmed.
Subnet fallback uses each interface's real IPv4 netmask. Networks up to `/22` are
scanned completely; larger networks are bounded to the local `/24` of each selected
interface to avoid generating tens of thousands of probes. Subnet probing has a
global concurrency limit.

The panel also provides **Search for new devices**, which bypasses cache-hit
short-circuiting and forces the bounded subnet scan when a new peer is missing.

### iOS note

During testing, the official LocalSend Android client responded to Nearby's active
discovery immediately and consistently. LocalSend on iOS was also discovered normally
when the app was opened or brought to the foreground while Nearby was already
scanning. A specific edge case was observed when the iOS app had already been open
before Nearby started scanning: it could remain visibly open yet stop responding to
the multicast discovery round. Fully closing and reopening LocalSend on iOS restored
immediate discovery. In that stale-open state, Nearby also falls back to a cached IP
probe, or to a bounded subnet scan when no usable cache entry exists.

## Build from source

The repository and every release contain the complete source. Install the plugin
without enabling it, build the helper, then enable it:

```sh
omarchy plugin add https://github.com/jfg96/omarchy-nearby.git --yes
cd ~/.config/omarchy/plugins/oma.nearby
./build.sh
omarchy plugin enable oma.nearby
```

`build.sh` runs Cargo and installs the resulting helper at:

```text
bin/omarchy-nearby-helper
```

Generated helper binaries are not tracked in the repository.

## Updates

Prebuilt installations are updated with Nearby's installer, which moves the
plugin and its helper to the same release together:

```sh
~/.config/omarchy/plugins/oma.nearby/install.sh
```

`omarchy plugin update oma.nearby` updates source files but cannot update the
release helper: it fetches and fast-forwards the checkout, and `bin/` is not
tracked. Nearby therefore states the oldest helper it can drive in
`manifest.json`:

```json
"minHelperVersion": "1.1.0"
```

A helper at or above that version keeps working after a source-only update, so
a release that does not change what the plugin asks of the helper no longer
needs the installer run at all. Below it, Nearby stops the backend and reports
which version it needs and which one is installed. Raise `minHelperVersion` in
the same change that starts depending on a new helper.

## Releases

Stable releases use tags such as `v1.0.3`. The tag, `manifest.json`, Rust package,
and helper all carry the same version. Release helpers are built only by GitHub
Actions and stored as GitHub Release assets with a SHA256 file and generated
third-party license notices.

Development on `main` uses the next SemVer prerelease, such as `1.0.4-dev`, as soon
as it diverges from the preceding stable tag. `manifest.json` and
`backend/Cargo.toml` are advanced together so a source-only update cannot be
mistaken for the previous release.

## Tests

Frontend model tests:

```sh
node tests/model.test.js
node tests/panel-state.test.js
node tests/service-state.test.js
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
files before atomic finalization. TLS fingerprints are used for peer connections
where the LocalSend protocol exposes them.

Incoming PIN protection is an additional authorization gate, not authenticated
pairing and not a replacement for TLS or local Accept/Decline. Three incorrect
PIN submissions from one IP cause subsequent requests from that IP to receive
LocalSend's `429 Too Many Requests` response until the receiver security state
changes or the receiver restarts.

LocalSend discovery itself is LAN discovery and should not be treated as an
authenticated pairing mechanism. Use Nearby on networks you trust.

## License

Nearby is MIT licensed. See `LICENSE`.

The vendored `localsend-rs` code is separately MIT licensed and carries its own
notices. See `THIRD_PARTY_NOTICES.md`.
