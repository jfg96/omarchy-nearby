# Architecture

[Documentation index](../README.md#documentation)

## Ownership

```text
Bar widget (Panel.qml)      one per monitor, view only
        │
        │  reads state, calls methods
        ▼
Nearby engine (Service.qml) one per shell session
        │
        ├── Model.js
        │
        └── tracked launcher → JSON over stdin/stdout
                 │
                 ▼
       verified XDG-data helper (Rust)
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
[third-party notices](../THIRD_PARTY_NOTICES.md) for attribution and licensing information, and
[vendor maintenance record](../VENDORED_LOCALSEND_RS.md) for the local patch and maintenance record.

## Helper protocol and state

The service sends newline-delimited JSON commands over the helper's stdin and
consumes JSON events from stdout. Diagnostics go to stderr. Command and event
definitions live in [the helper](../backend/src/main.rs); producers and consumers
on the shell side live in [Service.qml](../Service.qml).

Request IDs and transfer IDs correlate decisions, progress and terminal events.
Preserve them when extending the protocol so a late event cannot overwrite a
new transfer. Incoming and outgoing transfers have separate state.

The helper bounds command lines to 2 MiB, outgoing text to 1 MiB, and its peer
registry to 256 entries. Shared state helpers in [Model.js](../Model.js) must
remain usable by both QML and Node tests.

## Discovery model

Nearby starts with passive discovery and escalates to broader probes:

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

The following observation is retained from the earlier README. Its original
record did not specify client versions, so it is historical context rather
than a current compatibility result. Record new observations in
[the manual validation log](../ROBUSTNESS.md#result-record).

During testing, the official LocalSend Android client responded to Nearby's active
discovery immediately and consistently. LocalSend on iOS was also discovered normally
when the app was opened or brought to the foreground while Nearby was already
scanning. A specific edge case was observed when the iOS app had already been open
before Nearby started scanning: it could remain visibly open yet stop responding to
the multicast discovery round. Fully closing and reopening LocalSend on iOS restored
immediate discovery. In that stale-open state, Nearby also falls back to a cached IP
probe, or to a bounded subnet scan when no usable cache entry exists.

## Helper selection and lifecycle

The tracked [launcher](../bin/nearby-helper-launcher) reads
[helper-release.env](../helper-release.env) as strict metadata, never as shell
code. The checkout selects one exact Linux x86_64 asset; it does not resolve
"latest" at runtime.

Normal startup first checks for a deliberately built local helper and its marker.
Otherwise it verifies the selected cached helper's executable status, byte size
and SHA256. Missing or invalid bytes are downloaded over HTTPS, bounded to
32 MiB, staged on the destination filesystem and installed atomically.
Published executables are stored under XDG data, outside the plugin checkout.

The repair command invokes prefetch through the same launcher. Prefetch resolves
the published helper even when a developer override exists; it does not disable
that override. See [local builds](../CONTRIBUTING.md).

The service checks the helper's reported version against `minHelperVersion`.
An incompatible helper is stopped and the UI offers recovery. The runtime floor
is distinct from the stricter [stable release checks](RELEASING.md).

## Persistent state

[settings.rs](../backend/src/settings.rs) loads receiver security settings before
the listener starts and persists changes atomically.
[identity.rs](../backend/src/identity.rs) validates and preserves the TLS identity,
which is reused for outgoing HTTPS client authentication.

See [storage locations](USAGE.md#storage) and [security](SECURITY.md).
