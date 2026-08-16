# Vendored `localsend-rs` maintenance record

## Purpose

Nearby vendors a locally modified copy of `localsend-rs` at
`backend/vendor/localsend-rs`. The helper consumes it as a path dependency with
the `https` feature and without the crate's default CLI features.

This document records engineering and maintenance history for the local copy.
Licensing attribution remains in `THIRD_PARTY_NOTICES.md` and in the notices
retained inside the vendored tree.

The entries below have a deliberately narrow meaning: Nearby's Git history
records these modifications to the vendored tree after its initial import. They
must not be read as a complete list of differences from current upstream.

## Upstream provenance

- Upstream project: <https://github.com/CrossCopy/localsend-rs>
- Local path: `backend/vendor/localsend-rs`
- Package metadata version in the vendored tree: `0.1.2`
- Exact upstream base commit of the original import: **unknown/unrecorded**

The original tree appears in Nearby commit `80fad5e` (`Initial release`). A
content comparison against commits reachable in the upstream repository on
2026-08-13 did not find an exact match for that imported tree. This does not
establish a base revision: the import may have contained changes already, or its
source revision may no longer be reachable in upstream history. No upstream SHA
should be inferred from the package version.

Commit `0ac2d7d` (`Support installation with Omarchy plugin manager`) later
moved the tree from `oma.nearby/backend/vendor/localsend-rs` to its current path.
Git records every vendor file in that commit as a 100% content-preserving rename,
so it is not a local `localsend-rs` patch.

After that relocation, Nearby's history records eight content-changing vendor
commits. Their cumulative diff is 621 insertions and 79 deletions across ten
files:

| File | Insertions | Deletions |
| --- | ---: | ---: |
| `Cargo.lock` | 3 | 2 |
| `Cargo.toml` | 1 | 0 |
| `src/client/client.rs` | 81 | 11 |
| `src/client/mod.rs` | 1 | 1 |
| `src/discovery/http.rs` | 142 | 33 |
| `src/discovery/multicast.rs` | 29 | 8 |
| `src/server/pin.rs` | 65 | 22 |
| `src/server/server.rs` | 16 | 0 |
| `tests/conformance_pin.rs` | 146 | 1 |
| `tests/interop_upload.rs` | 137 | 1 |

## Local modification policy

The vendored `localsend-rs` tree is intentionally frozen by default. Nearby
does not track upstream changes automatically. Upstream changes are evaluated
and ported only when they are relevant to security, LocalSend protocol
compatibility, a concrete Nearby bug, or functionality intentionally required by
Nearby. The tree is not routinely refreshed merely to follow upstream HEAD.

## Local modifications

### 1. Prefer `/register` for HTTP discovery

- Nearby commit: `13f43c4` — `Prefer register for HTTP peer discovery`
- Primary file: `src/discovery/http.rs`

The original HTTP/subnet fallback discovered peers with
`GET /api/localsend/v2/info`. Nearby changed this path to prefer
`POST /api/localsend/v2/register`, sending Nearby's local `DeviceInfo` in the
request body.

If `/register` returns a non-success response, returns malformed or unusable
JSON, or otherwise cannot provide usable peer information, discovery falls back
to the legacy `/info` request. The normal HTTP/HTTPS protocol fallback remains
in place. The discovered peer is populated with the IP, port, and protocol of
the connection that actually succeeded.

Nearby needs this behavior so fallback discovery participates in LocalSend's
registration mechanism instead of relying exclusively on the legacy `/info`
endpoint. Retaining `/info` preserves compatibility with older or unusual peers.

The HTTPS-to-HTTP discovery regression test was strengthened to wait for and
assert a `ServerEvent::PeerRegistered` event. Finding a peer alone is not enough
for that test: the event proves that discovery used `/register`.

If upstream adopts `/register`-first discovery, compare its semantics before
dropping this patch. In particular, do not remove the legacy `/info` fallback
merely because upstream adds `/register`.

### 2. Expose real IPv4 interface netmasks

- Nearby commit: `ed6cd3a` — `Respect interface netmasks in subnet discovery`
- Primary file: `src/discovery/multicast.rs`

This patch adds:

```rust
local_ipv4_interfaces_with_netmasks() -> Result<Vec<(Ipv4Addr, Ipv4Addr)>>
```

The helper obtains the interfaces used for LocalSend multicast, obtains their
real IPv4 netmasks through `if_addrs`, matches the netmask information by
interface name and address, and returns `(IPv4 address, IPv4 netmask)` pairs. It
continues to reject unsuitable addresses, including unspecified, loopback, and
link-local addresses. The existing address-only helper remains available and
derives its result from the richer representation.

Nearby needs the actual network mask so its fallback scanner does not have to
assume every LAN is a `/24`.

The scope boundary matters: the vendor patch exposes only the address and
netmask. Nearby's own backend calculates CIDRs and ranges, chooses interfaces,
bounds large scans, applies the `/22` and local-`/24` limits, and orchestrates
cache-first and fallback behavior. Those policies are not vendor modifications.

### 3. Preserve cached peer port and protocol during revalidation

- Nearby commit: `068fe7e` — `Preserve cached peer ports and protocols`
- Primary file: `src/discovery/http.rs`
- Regression test: `cached_probe_uses_the_peers_custom_port_and_protocol`

The generic explicit-host scanner starts from an IP address. Reducing an already
known and cached peer to only its IP loses its advertised or custom port and its
known HTTP/HTTPS protocol. Revalidation could therefore probe different
connection coordinates from those on which the peer was discovered.

This patch adds the device-aware incremental API
`scan_devices_incremental_with_limit(...)`, which accepts complete `DeviceInfo`
objects, and a known-device probe path that starts with the cached peer's IP,
port, and protocol. If the cached protocol fails, the alternate HTTP/HTTPS
scheme may still be tried on the same peer port. The lower-level HTTP probe now
accepts an explicit `(ip, port, protocol)`, and the generic incremental scanner
can carry targets other than strings so complete devices can flow through it.

The regression test runs a peer over HTTP on a non-default dynamic port while
the scanner itself is configured differently. It verifies that revalidation
finds exactly that peer, preserves the remote port, and records the protocol
that succeeded.

If upstream gains explicit-port probing, do not assume the entire patch is
obsolete. Check whether it also preserves the cached protocol preference and
provides the incremental, device-aware behavior Nearby consumes.

### 4. Upload clipboard and text payloads directly from memory

- Nearby commit: `22f39cf` — `fix: keep clipboard uploads in memory`
- Files: `src/client/client.rs`, `src/client/mod.rs`, and
  `tests/interop_upload.rs`
- Regression test: `uploads_in_memory_bytes_without_a_source_file`

This patch adds `LocalSendClient::upload_bytes(...)` for uploading an in-memory
`Vec<u8>` rather than requiring a source path. It constructs the normal
LocalSend `/upload` request, supplies the correct `Content-Length`, wraps the
payload as a streaming reqwest body, uses the existing progress callback, and
accepts both `200 OK` and `204 No Content`. Other HTTP responses continue through
the normal LocalSend error mapping. `ProgressCallback` is also re-exported from
`client/mod.rs` so Nearby can use the shared progress API.

Clipboard and text payloads already exist in memory. Without this API, Nearby
would need to create a temporary file, write the clipboard data to disk, upload
that file, and then clean it up. Normal file transfers continue to use the
existing streaming file-upload path.

The integration test prepares a normal upload session, uploads an in-memory
payload without a source file, and verifies the exact received bytes, completed
session, and final progress count.

### 5. Match official LocalSend receiver PIN enforcement and allow live changes

- Nearby commits: `3b842ca` and `e51e10d`
- Files: `Cargo.toml`, `Cargo.lock`, `src/server/pin.rs`,
  `src/server/server.rs`, and `tests/conformance_pin.rs`

The imported PIN gate counted a missing PIN as a failed attempt, kept an
unbounded IP map and applied a five-minute cooldown. LocalSend 1.18.1 instead
counts only supplied incorrect values, blocks after three failures without a
timer and bounds failure state to an LRU cache of 200 IPs. Nearby now follows
those semantics.

`PinGate::set_pin(...)` and `LocalSendServer::set_pin(...)` expose the existing
authentication state to Nearby's long-lived helper. Changing or disabling the
credential clears stale failure state and affects only future prepare-upload
checks; it does not restart the listener or invalidate an authorized session.
Conformance tests cover status codes, event suppression, counter reset and live
changes, while the upload integration test proves an authorized transfer
survives a credential change.

### 6. Encode outgoing PIN query values

- Nearby commit: `74bd876`
- Primary file: `src/client/client.rs`
- Regression test: `prepare_upload_url_round_trips_reserved_and_unicode_pin_characters`

The imported client interpolated a PIN directly into the prepare-upload URL.
Reserved query characters could therefore be parsed as delimiters or fragments
instead of arriving as part of the PIN. Nearby now builds the URL with
`reqwest::Url` and appends `pin` through its query serializer. Tests cover
spaces, Unicode, `+`, `&`, `#` and `%` round-tripping unchanged.

## Nearby behavior outside the vendor

The LocalSend 1.18 client-certificate compatibility hotfix is not one of the
vendor patches above. The vendored client already offered
`with_trust_policy_and_identity(...)`; Nearby changed its own caller to reuse the
persistent Nearby TLS identity for outgoing HTTPS connections.

Likewise, Nearby's bounded subnet scanning policy is implemented in Nearby's
backend. The vendor exposes interface and netmask information; the consumer
decides scan ranges, limits, priorities, and cache behavior.

These distinctions must be preserved when comparing the local tree with
upstream. A change in Nearby's integration is not automatically a vendor
modification.

## Upstream refresh procedure

Before replacing or rebasing the vendored tree:

1. Identify the exact upstream revision being considered.
2. Review every locally documented modification.
3. Determine for each modification whether upstream still lacks it, has an
   exact equivalent, has a partial equivalent, or solves the underlying problem
   differently.
4. Port only the patches that remain necessary.
5. Run the vendored crate's full test suite.
6. Run Nearby's backend and frontend CI checks.
7. Perform the relevant real-device LocalSend interoperability smoke tests.
8. Update this document in the same change.

Never replace the vendor and assume that a successful compile proves that local
behavior was preserved.

## Rules for future vendor changes

Every intentional divergence introduced under
`backend/vendor/localsend-rs/` must update this document in the same PR or
commit. Each entry should record:

- its purpose;
- affected files and APIs;
- the behavior changed;
- why Nearby requires it;
- regression coverage;
- the originating Nearby commit or PR when useful; and
- upstream status, if known.

Keep consumer-side Nearby behavior distinct from vendor changes, and distinguish
changes proven by Nearby history from differences observed against a particular
upstream revision.

If upstream later absorbs a local modification, do not erase its history. Mark
the entry as superseded or upstreamed and record the upstream revision or
replacement before removing the local patch.
