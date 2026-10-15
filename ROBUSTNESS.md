# Nearby robustness validation

[Documentation index](README.md#documentation) ·
[Development checks](CONTRIBUTING.md#validation)

This is a validation plan, not evidence that every scenario has passed.
Record real-device outcomes in the result template below.

## Automated coverage

Run:

```sh
cargo test --locked --manifest-path backend/Cargo.toml
cargo test --locked --manifest-path backend/vendor/localsend-rs/Cargo.toml --features https
node tests/model.test.js
node tests/panel-state.test.js
node tests/service-state.test.js
bash tests/launcher.test.sh
bash tests/helper-repair.test.sh
bash tests/copy-file.test.sh
```

The suites cover peer registry retention/expiry, command correlation, request decisions,
cancel/completed/failed event separation, oversized, truncated and checksum-mismatched uploads,
repeated and concurrent uploads of one file, sender-supplied file ids in partial paths,
the incoming file-count limit, cancellation stopping an upload in progress,
connection limits, request head/body timeouts and upload idle timeouts,
outgoing request timeouts and response size limits, bounded multicast replies,
rate-limited peer updates, HTTPS peers refusing plain HTTP takeover,
notification bodies that start with fixed text and escape remote text,
bounded, non-blocking `user-dirs.dirs` reads and ignored relative download locations,
atomic equal-name commits, traversal rejection, progress backpressure, TLS pinning,
HTTP `/register` fallback, and a session whose activity is older than five minutes while
an upload is still active. Incoming-PIN coverage includes secure startup,
atomic persistence and rollback, exact `401`/`429` behavior, bounded per-IP
failure state, text/file authorization before events, live PIN changes and an
already authorized upload continuing after a change.
Settings tests also cover rejected symlinks, non-regular files and mismatched
ownership, a FIFO without a writer under a subprocess deadline, the 16 KiB
boundary and a bounded read when the input exceeds the inspected size. A helper
process test checks fail-closed startup before `ready` with unsafe settings.
The 1.2.2 development tests also cover symlinked state ancestors, private
directory ownership and modes, rejected writable non-sticky ancestors,
descriptor binding after path substitution, exact `0600` publication under a
restrictive umask, random exclusive temporary files, collision retries,
failed publication cleanup, concurrent settings saves, and identity FIFO
rejection under a subprocess deadline.

The distribution suites additionally cover immutable helper metadata, exact
size/SHA256 enforcement, XDG data storage, offline reuse, corrupt and symlinked
cache repair, rejected symlinked or other-user-writable cache ancestors,
concurrent launches, failed/oversized downloads, and the rule
that published helpers never write into the watched plugin checkout.

The received-file suites cover offering exactly one received file once its
result is on screen, per-session file counts, results that never reached the
screen, a type probe that answers late, a file that arrives while the probe is
still busy with the previous one, keyboard reachability of Copy and Done, and,
for the copy script, conversion of every previewable picture, PNG included,
with the decoder named by the detected type and the file name kept away from
ImageMagick, fallback to a file reference when the conversion fails or
ImageMagick is absent, without clearing the clipboard or copying the file's
own bytes, URI escaping, temporary file cleanup and refused arguments.

### Local automated result, 2026-09-22

On `fix/secure-settings-load`, the three Node suites, two Bash suites, Rust
formatting check, `cargo check --locked`, Clippy with warnings denied, helper
tests and vendored `localsend-rs` tests all passed. Local Markdown link paths
and `git diff --check` also passed. This records automated checks only; the
manual scenarios below remain **Not run** for this change.

### 1.2.1 release preparation, 2026-09-23

The `helper-v1.2.1` GitHub Actions workflow passed on source commit
`04ab359a8ff850578beddca482d638bbac516e62`. Its published Linux x86_64
artifact was independently checked for byte size, SHA256, attestation signer,
source digest, tag and reported version before pinning it. The isolated launcher
prefetch check also passed. Android and iOS peer interoperability, live Omarchy
settings recovery and the manual scenarios below remain **Not run** for 1.2.1;
no peer app versions or manual outcomes are recorded.

### 1.2.2 development validation, 2026-09-23

On local branch `fix/filesystem-hardening`, all three Node suites, both Bash
suites, Rust formatting, `cargo check --locked`, Clippy with warnings denied,
helper tests, vendored `localsend-rs` tests and `git diff --check` passed.
This is automated validation only. The manual scenarios below remain **Not run**
for this change.

### 1.2.2 helper preparation, 2026-09-23

The pull-request CI passed on helper source commit
`5905ecd4e666e7d98e853c1f8e6f05890e4ebe30`. The `helper-v1.2.2`
GitHub Actions workflow built and published its Linux x86_64 prerelease.
The downloaded executable independently matched the release checksum and
9,040,128-byte size; `gh attestation verify` matched the helper-release workflow,
source digest and tag, and `--version` reported `omarchy-nearby-helper 1.2.2`.
Android/iOS interoperability and live Omarchy scenarios remain **Not run**.

### Upload-size development validation, 2026-10-01

On `fix/receive-hardening`, the incoming upload writer checks the negotiated
size before each chunk is written. New regression coverage checks immediate
rejection without EOF, no excess bytes or progress, zero-byte limits and checked
arithmetic overflow. The HTTP regression additionally checks partial cleanup,
terminal failure, old-token rejection and receiver recovery through a fresh
session. Existing truncation and checksum tests now assert that no `.part` file
remains either.

The nonterminating-stream regression failed against the pre-fix writer with
`must reject excess bytes without waiting for EOF: Elapsed(())`. The raw HTTP
regression was also run against pre-fix commit
`5b7c5b6d8f8726c4ee7e9d888757cea1201d5470` in an isolated worktree: it failed
with `oversized upload must be rejected before EOF: Elapsed(())`, while keeping
the sending half of the socket open. Both regressions pass with the fix.

An initial sandboxed run prevented localhost sockets and remapped filesystem
owners, blocking network and protected-state tests. After enabling full access,
the complete supported suite passed with Rust 1.97.1:

- All three Node suites and both Bash suites.
- Helper and vendor formatting checks, `cargo check --locked`, and helper
  Clippy with warnings denied.
- Helper tests: 71 unit tests and 1 startup integration test passed.
- Vendor tests: 76 library tests and 38 integration tests passed; 1 pre-existing
  library test remains ignored. The three `conformance_upload` tests also
  passed in a separate focused run, including the raw HTTP regression.
- `git diff --check`.

The HTTP regression was validated over a real localhost TCP socket, with real
file writes and partial cleanup. Android/iOS interoperability and live Omarchy
scenarios remain **Not run**. No release or tag has been created for this change.

The development checkout advances to plugin `1.2.3-dev` and helper/floor `1.2.3`.
`helper-release.env` still pins the verified 1.2.2 artifact; it must only change
after publishing and independently verifying a matching 1.2.3 helper. Until
then this checkout requires a deliberately built local helper to meet its new
compatibility floor.

### Single-use upload development validation, 2026-10-01

On `fix/receive-hardening`, each accepted file can be uploaded once per
session. New HTTP regressions check that a received file's token cannot store a
second copy while the session is still open, that a second request for a file
in progress is rejected without disturbing the first upload or its content, and
that a sender-supplied file id containing `/` and `..` no longer shapes the
partial path. A session unit test covers foreign, exclusive, released and
received reservations.

All three HTTP regressions failed against the size-fix commit `e602a80` for the
expected reason: the repeated and concurrent uploads returned `200` instead of
`409`, and the slash-containing file id returned `500` instead of `200`. They
pass with the fix. A temporary experiment, not committed, confirmed that an
abrupt sender disconnect mid-upload still runs the existing failure path:
progress rolls back, `SessionFailed` is emitted and no partial file remains.
Android/iOS interoperability and live Omarchy scenarios remain **Not run**.

### Incoming file-count development validation, 2026-10-01

On `fix/receive-hardening`, a prepare-upload request offering more than 10,000
files receives `413` after the PIN check and before a session is reserved or a
decision is requested. A regression offers 10,001 small entries, well under the
2 MB JSON limit, to a receiver without auto-accept: it must answer within five
seconds and emit no event. Against commit `7c08592` it failed because the
request waited for a decision. A boundary test confirms that exactly 10,000
files are still accepted. Live interoperability remains **Not run**.

### Upload cancellation development validation, 2026-10-01

On `fix/receive-hardening`, an upload writer stops before its next chunk once
its session has ended. An HTTP regression starts a 1 MiB upload, cancels the
session and sends one more byte while keeping the advertised body open: the
upload must answer within five seconds with a non-success status and leave no
file. Against commit `ec1b460` it failed because the writer kept waiting for
the rest of the body. A writer unit test checks that the chunk arriving after
the session ends is not written. The rollback progress event emitted after a
cancel was already possible on other failure paths; `Service.qml` ignores it
once the transfer has finished. Live interoperability remains **Not run**.

### Connection and timeout limits development validation, 2026-10-01

Before the change, an experiment against the HTTPS receiver showed no timeout
after the TLS handshake: an idle connection, a partial request head and a
partial `/register` body all stayed open past 40 seconds, and 100 connections
holding 1.9 MB partial bodies raised the process from 13 MB to 248 MB
indefinitely. The receiver also negotiated HTTP/2 with a client offering it.

`conformance_limits` adds nine HTTP tests with one-second limits. Seven
regressions failed with the limits configured but not enforced: idle and
partial-head connections stayed open, a trickled body and a stalled upload got
no answer, connections over the per-IP and global limits stayed open, and
HTTPS negotiated `h2`. Two guards passed before and after: an accept decision
taking longer than the body timeout still succeeds, and an upload sending one
byte every 400 ms completes although it outlasts both timeouts. A unit test
covers slot accounting.

With the default limits, an experiment opening 100 connections from distinct
loopback addresses, each holding a 1.9 MB partial prepare-upload body, admitted
64 and refused 36; memory peaked at 135 MB and returned to 26 MB once all held
connections were answered or closed within 32 seconds. Both experiments were
temporary and not committed; their numbers include the in-process test client.

Not covered automatically: hashing a multi-gigabyte upload, which happens after
the body is read and outside the idle timeout by construction. Interoperability
of the HTTP/1.1-only receiver with official Android/iOS clients, including
reconnection after an idle keep-alive connection closes, and Nearby-to-Nearby
transfers between live installations remain **Not run** and must be checked
manually before release.

### iPhone manual session, 2026-10-01

| Session field | Value |
| --- | --- |
| Date and tester | 2026-10-01, maintainer |
| Plugin version and exact commit | `1.2.3-dev`, `fix/receive-hardening` at `5cc66c0` |
| Helper version and published/local build | `1.2.3`, local `build.sh` override |
| Omarchy version | `4.0.0.r6691.g8b4eae6-1` |
| Android device, OS and LocalSend version | Not run |
| iOS device, OS and LocalSend version | iPhone 15, iOS 27.0.1, LocalSend 1.18.2 |
| Network, VPN/firewall and monitor arrangement | Not recorded |

Nearby 1.2.2 was removed with `omarchy plugin remove`, reinstalled through the
plugin manager, switched to the branch and built locally. The receiver reused
the existing identity and settings and negotiated only `http/1.1`.

| Check | Peer/platform | Result | Evidence or deviation |
| --- | --- | --- | --- |
| Discovery in both directions | iOS | Pass | Reported by tester |
| iPhone → Nearby, one file and several files | iOS | Pass | Reported by tester |
| Accept after about 40 seconds | iOS | Pass | Longer than the 30-second body timeout |
| Second transfer after more than 30 idle seconds | iOS | Pass | Reconnects after keep-alive close |
| Nearby → iPhone, file and clipboard | iOS | Pass | Reported by tester |
| Incoming PIN | iOS | Pass | Reported by tester |
| Large-file cancel from iPhone (matrix 4) | iOS | Not run | No large file available |
| Cancel while awaiting approval, then resend | iOS | Fail at `5cc66c0` | Next offer refused as busy; also in 1.2.2 |
| Same retest after the fix | iOS | Pass at `b6546d0` | Prompt withdrawn; immediate resend asks for approval |

The failure was reproduced with an automated sender that drops its connection
while awaiting a decision, against both this branch and `v1.2.2`; the new offer
received `409` for at least 25 seconds, beyond the accept timeout. It is fixed
by the pending-reservation guard recorded in `VENDORED_LOCALSEND_RS.md` and
passed the manual iPhone retest at `b6546d0`. Android remains **Not run**.

### 1.2.3 helper preparation, 2026-10-01

The pull-request CI passed on helper source commit
`96bcd3025ca5c31cc06df483d4b0968ee460c94a`. The `helper-v1.2.3`
GitHub Actions workflow built and published its Linux x86_64 prerelease.
The downloaded executable independently matched the release checksum and
9,071,448-byte size; `gh attestation verify` matched the helper-release workflow,
source digest and tag on a GitHub-hosted runner, and `--version` reported
`omarchy-nearby-helper 1.2.3`. Android interoperability remains **Not run**.

### LAN peer hardening development validation, 2026-10-02

The development checkout advances to plugin `1.2.4-dev` and helper/floor `1.2.4`.
`helper-release.env` still pins the verified 1.2.3 artifact; it must only change
after publishing and independently verifying a matching 1.2.4 helper. Until
then this checkout requires a deliberately built local helper to meet its new
compatibility floor.

The panel test now parses every text element in `Panel.qml` and `Service.qml`
and requires `textFormat: Text.PlainText`; before the fix it failed on the
transfer progress line reported in the marketplace review.

`conformance_client_limits` adds five regressions with a fake peer that never
answers or streams an endless chunked body. Before the fix `register` and
`cancel` were still waiting after 20 seconds, and `register`, prepare-upload
and a discovery probe read the entire 256 MiB flood. They now fail within the
one-second test timeout or after reading at most the response limit.

`conformance_multicast_replies` sends announcements over loopback to the
wildcard-bound discovery socket and counts reply connections at a silent peer.
Before the fix 50 announcements from one address opened 50 connections and 40
addresses opened 40; now they open one and at most eight. The tests skip when
no IPv4 interface can join the multicast group.

Helper unit tests cover the peer gate. With the gate made permissive, as the
previous `record_peer` behaved, five failed for the expected reason: 1,000 new
identities produced 1,000 frontend events, an unchanged peer was re-sent on
every announcement, the event budget never ran out, and a plain HTTP
announcement replaced a known HTTPS peer's protocol and address, also after
the peer had expired. A guard confirms that an HTTP peer can still upgrade to
HTTPS.

A service-state test sends hostile aliases and file names (`--image=…`,
`--urgency=…`, `--exec`, `-g` and markup) through incoming requests and text.
Before the fix it failed because the body began with the alias. Separately,
on Omarchy, `omarchy-notification-send` given the old body
`--image=… wants to send …` stored the notification with an empty body; given
the new body it kept the full, escaped text and no image. The latter check is
manual and not part of CI.

Six helper tests cover the download location. With the previous unbounded
`read_to_string` and no absolute-path check, three failed: a FIFO
`user-dirs.dirs` blocked until the five-second subprocess deadline, a file
larger than 16 KiB was read whole, and a relative location was accepted.

On 2026-10-02 the merged source `1930ed7` was built locally and checked on
Omarchy against an iPhone 15 (iOS 27.0.1, LocalSend 1.18.2). The iPhone
appeared in the device list, disappeared after LocalSend had been closed for
more than 90 seconds and reappeared when reopened. An iPhone → Nearby file was
accepted, saved to the download location and announced by the incoming
notification, and a Nearby → iPhone file was received. All passed; Android
remains **Not run**.

### 1.2.4 helper preparation, 2026-10-02

The CI passed on helper source commit
`1930ed762c7f67ec71f828a0a405ef377544ca12`. The `helper-v1.2.4`
GitHub Actions workflow built and published its Linux x86_64 prerelease.
The downloaded executable independently matched the release checksum and
9,077,944-byte size; `gh attestation verify` matched the helper-release workflow,
source digest and tag on a GitHub-hosted runner, and `--version` reported
`omarchy-nearby-helper 1.2.4`. Android interoperability remains **Not run**.

### Folder transfer hardening development validation, 2026-10-13

The PR #22's four original commits were rebased onto `main` at `758e319`,
preserving Rizmi's authorship. The rebased feature baseline is `3d5f67e`;
maintainer hardening is recorded at `b85376f` on
`feat/folder-transfer-hardened`. Development
versions advance to plugin `1.2.6-dev` and helper/floor `1.2.5`. The committed
`helper-release.env` deliberately still selects the verified 1.2.4 artifact.
This development branch requires a local developer helper until a 1.2.5 helper
is published through Actions, independently verified and selected by exact
metadata. No helper or plugin release has been published by this validation.

All three Node suites, both Bash distribution suites, Cargo formatting, locked
check, Clippy with warnings denied, helper tests (92 unit plus the startup
integration test) and the separate vendor HTTPS suite passed locally. The
vendor suite passed 151 tests in total and retains its existing ignored test; an ignored test is not a pass.
Formatting of the vendored crate and `git diff --check` also passed.

New coverage includes:

- Raw path spelling and UTF-8 byte/depth boundaries, validation of every
  offered path before prompts/reservations, a valid retry after rejection, and
  unchanged PIN-first authorization and lockout.
- Missing directories and unreadable files/folders, invalid UTF-8, special
  files, hidden names and cooperative scan cancellation.
- Explicit top-level symlink naming and skipping links inside selected trees.
- The 10,000-file limit for regular-file selections as well as folders; escaped
  JSON counting during collection and the exact identity/offer envelope bound.
- Full helper folder transfers over HTTP and pinned HTTPS, in sorted order,
  with nested collision suffixes, exact saved bytes and omitted empty folders.
- Existing and late-created parent symlinks refused over HTTP before disk
  writes, and transfer-ID protection for the active outgoing file name.

Three regressions were also run against the rebased feature before hardening:
`unsafe_paths_are_rejected_before_reservation_or_prompt` failed because the
invalid offer waited for approval; `rejects_unsafe_wire_spellings_without_normalizing`
failed on `./file`; and a direct missing/unreadable-directory preparation check
failed because traversal returned success. All three pass after hardening.

The effective diff was also checked against the post-1.2.0 guarantees:

| Baseline | Guarantees retained and exercised |
| --- | --- |
| 1.2.0 | Bounded command/text input and peer registry; persistent TLS identity and outgoing client authentication; immutable HTTPS-only helper selection and distribution tests |
| 1.2.1–1.2.2 | Bounded, no-follow settings/identity state and atomic descriptor-based persistence; unsafe helper-cache ancestor rejection; source files and launcher unchanged |
| 1.2.3 | Pre-write upload size bounds, one upload per offered file, random partial names, cancellation/progress rollback, 10,000-file cap, connection/body limits and abandoned-prompt cleanup; vendor conformance suites pass |
| 1.2.4 | Bounded outgoing requests/responses and multicast replies, peer event coalescing and HTTPS protection, plain-text sinks, escaped notification bodies and bounded Downloads config reads; regression suites pass |
| 1.2.5 | Nearby icon and Commons.Color fix retained by the rebase; QML plain-text and state suites pass |

A live helper-only check on the maintainer workstation reached `ready` with
helper `1.2.5` on port 53317, using isolated temporary state and Downloads.
Active multicast and the bounded subnet fallback completed; all peer snapshots
were empty during the 20-second observation. The process was shut down and the
installed receiver was still disabled afterwards. The installed plugin was
not replaced. `omarchy plugin validate` accepted the development checkout.
This confirms local helper startup, not an official-client transfer or loading
the changed QML inside the live shell.

These are local automated tests using Rust peers, not official LocalSend
interoperability or live Quickshell tests. At this initial stage the manual folder scenario was **Not run** with official
clients; the subsequent iPhone session below supplies real transfer results.
Android and desktop interoperability remain **Not run**.

### Official iPhone folder session, 2026-10-13

- Source: `d682a9c` on `feat/folder-transfer-hardened`, plugin
  `1.2.6-dev`, local debug helper `1.2.5`; the changed QML is not installed.
- Peer: the maintainer's iPhone advertised as `Patient Avocado`, protocol
  `2.2`, HTTPS. The repo's prior session records iPhone 15 / iOS 27.0.1 /
  LocalSend 1.18.2; those OS/application versions have not been rechecked
  during this session.
- Nearby → iPhone: **Pass at the protocol level**. The three-file
  `Nearby-PR22-folder-test` offer was accepted, the helper uploaded
  `a.txt`, `sub/b.txt`, `sub/deep/c.bin` in order (4,158 bytes total)
  and emitted `outgoing_done` over fingerprint-pinned HTTPS.
- iPhone hierarchy/content: **Pass by round-trip verification**. The maintainer
  returned the received folder with the original nested wire paths.
- iPhone → Nearby folder round trip: **Pass**. Approval produced
  `incoming_accepted`, three `file_received` events at the expected relative
  paths and `incoming_done`. All three sizes and SHA256 hashes exactly match
  the original fixture; no `.part` remains.
- Repeated Nearby → iPhone send: **Pass**. On request, the same three-file
  fixture was sent again and emitted `outgoing_done`.
- Incoming PIN and repeated-root collisions: **Pass**. The temporary receiver
  enabled a PIN, and the official sender reached approval with it. The second
  return offered six files (8,316 bytes), including the phone's renamed copies.
  After explicit approval the helper completed all six files. Nine files
  remain in the destination: the original three plus six new files. The
  colliding originals became `a (1).txt`, `sub/b (1).txt` and
  `sub/deep/c (1).bin`; every original and copy has the expected size and
  SHA256, and no `.part` remains.
- Decline, cancellation, wrong-PIN lockout and live QML: **Not run in this session**.

The helper used temporary state, identity, fixture and download directories,
without replacing the installed plugin or changing its PIN/identity files.
It was shut down at the end. The installed receiver was left disabled in
accordance with the maintainer's latest instruction.

### Quickshell QML smoke, 2026-10-13

Source `1a98a45` was loaded in an isolated Quickshell 0.3.2 configuration
against the workstation's real Omarchy `Ui` and `Commons` components. The
harness instantiated `Service.qml` and `Panel.qml`, including the eager popup
content and the **Send folder** button. It verified that a disabled receiver
stays stopped, the helper floor is `1.2.5`, selection switches the live panel
to the target view, keyboard navigation stops at the fourth action, outgoing
file-name/progress bindings update, and a stale transfer event cannot replace
them. The final run loaded without warnings or errors and emitted
`QML_SMOKE_PASS`. The installed shell/plugin and receiver setting were not
changed by this check.

This is an actual QML runtime smoke with a small host fixture, not a visual
review of a popup on the real bar or an interactive file-chooser test. Those
manual UI actions remain **Not run**. The official iPhone transfers above
used the real helper separately from this QML harness.

### 1.2.5 helper preparation, 2026-10-13

The exact main merge commit
`3ffca9342890d71c66f1e7346cd88feb6c2307f2` passed CI in run
[38383631972](https://github.com/jfg96/omarchy-nearby/actions/runs/38383631972)
before the `helper-v1.2.5` tag was created. The helper-release workflow
[38384135757](https://github.com/jfg96/omarchy-nearby/actions/runs/38384135757)
passed and published the Linux x86_64 helper as a prerelease.

The downloaded executable independently matched its published checksum and
GitHub asset digest: 9,132,128-byte size and SHA256
`f07286bb96ad845fa755e3ebd4b468fc22b505a7a9bc7c22b95f7caf34f6c113`. `gh attestation verify`
verified the expected repository, helper-release workflow, exact source commit
and tag and refused self-hosted runners. Executing the verified published bytes
with `--version` returned `omarchy-nearby-helper 1.2.5`.

The initial plugin release preparation selected those values in
`helper-release.env`; at that stage its backend tree matched the tagged helper
source. This preparation is superseded by the rustls update below before
stable publication.
Both launcher/repair Bash suites passed. A fresh `--prefetch` through the
tracked launcher downloaded the selected artifact into an isolated XDG cache,
verified it and returned version `1.2.5`; the cached executable reported the
same version. The
official iPhone and Quickshell smoke results above remain the applicable manual
records. Android, desktop and interactive popup/file-chooser validation remain
**Not run**.

### Rustls handshake boundary security update, 2026-10-13

GitHub's runtime dependency alert identified rustls 0.23.43 in the helper
lockfile and 0.23.36 in the standalone vendor lockfile as affected by
[GHSA-2mjx-qc3c-rqvc](https://github.com/rustls/rustls/security/advisories/GHSA-2mjx-qc3c-rqvc).
The manifests now require rustls 0.23.45 and both locked dependency trees resolve
exactly that version for the client, server and helper. Required transitive
updates are recorded in vendor maintenance entry 17.

The helper package and minimum helper floor advance to `1.2.6`; the plugin
remains `1.2.6-dev` until release preparation. The already published
`helper-v1.2.5` tag is unchanged. A new independently built and verified
`helper-v1.2.6` is required before the stable plugin can select this fix.

All three Node suites, both distribution Bash suites, Cargo formatting, locked
check, Clippy with warnings denied, the 92 helper unit tests plus startup
integration test, and all 151 vendor tests passed locally (one existing vendor
test remains ignored). The helper's folder transfer regression exercises both
HTTP and pinned HTTPS with nested collisions, and its client-authentication
test requires the persistent sender identity. The vendor suite retains
fingerprint rejection coverage. No new local exploit reproduction was added;
the handshake boundary correction comes from the upstream rustls patch.

Official iPhone round trips were tested with the preceding helper source as
recorded above; that manual session has not yet been repeated with rustls
0.23.45.

### 1.2.6 helper preparation, 2026-10-13

The rustls security fix was merged at
`b187afc73d09cad50c7dfd309d7af80c47b7c963`. That exact main commit passed CI in
[38386346462](https://github.com/jfg96/omarchy-nearby/actions/runs/38386346462)
before creating `helper-v1.2.6`. Helper release workflow
[38386808233](https://github.com/jfg96/omarchy-nearby/actions/runs/38386808233)
passed and published the Linux x86_64 prerelease.

The published executable was downloaded independently and verified: byte size
9,141,056, SHA256 `3ba5ba44d3380b2d0e3ed6d1a42d6d1b8abb9c5acd11d0dd415a936e094a02ed`,
matching the checksum file and GitHub asset digest. Attestation verification
bound it to the expected repository, helper-release workflow, source commit
and tag on a GitHub-hosted runner. The verified bytes reported
`omarchy-nearby-helper 1.2.6`.

The final plugin 1.2.6 preparation selects this corrected helper, with matching
Cargo version, minimum helper floor and backend tree. All three Node suites and
both distribution Bash suites passed with the final metadata. A fresh prefetch
through the tracked launcher downloaded and verified the 1.2.6 artifact into an
isolated XDG cache; both prefetch and the cached executable reported 1.2.6. GitHub marks the rustls
dependency alert fixed on main. The previously published helper-v1.2.5 tag
is retained and is superseded by this selection before plugin publication.

### Download folder development validation, 2026-10-12

The development checkout advances to plugin `1.2.7-dev`; the helper is
unchanged and already resolves `XDG_DOWNLOAD_DIR` first. The Node suites
passed.

After review the panel offers **Use Downloads** and refuses the home folder.
On Omarchy's shell with Quickshell 0.3.1, `downloadDir` was set by hand to a
path that cannot be created: the helper reported "download destination
unavailable" and the receiver gave up after its retries. Removing `downloadDir`
from the entry, which is the write **Use Downloads** makes, started the
receiver again within a second without touching the receiver switch, and the
helper had no `XDG_DOWNLOAD_DIR` in its environment. Writing a folder back
restarted it with that folder. Pressing **Use Downloads** in the panel, the
refusal of the home folder on screen and the rest of scenario 23 (choosing a
folder in the panel and receiving into it) are **Not run**.

### Received file preview and copy development validation, 2026-10-12

The development checkout advances to plugin `1.2.7-dev`; the helper is
unchanged. The Node suites and `tests/copy-file.test.sh` passed.
`bin/nearby-copy-file` was also run by hand with the real `file` 5.48 and
ImageMagick 7.1.2-31 and a stand-in for `wl-copy`: a 4000×3000 JPEG, a WebP and
a three-frame GIF each produced PNG data, including a JPEG whose name contains
brackets, a percent sign and spaces; a JPEG with EXIF orientation 6 and a GPS
tag came out upright with no metadata; a text file and text renamed to `.jpg`
or `.png` produced a file reference.

After review a PNG is converted like the other types, and the script was run
again the same way. An 800×600 and a 4000×3000 PNG carrying a comment, date
text chunks and, in the larger one, a GPS property each produced a PNG whose
only chunks are `IHDR`, `PLTE` where indexed, ImageMagick's `orNT`, `IDAT` and
`IEND`; the larger one took 2.7 s. JPEGs of 4000×3000 and 8160×6120 with EXIF
orientation 6 came out upright within the 2 GiB memory bound in under 2 s
each; with a 256 MiB bound the first of them was refused, which is why the
bound is not lower. In a throwaway Quickshell 0.3.1 configuration, a `Process`
started again from its own `onExited` ran and reported the second file's type.

At this development stage, scenario 24 (a real transfer, the preview in the
panel, the real clipboard and pasting into other applications) was **Not run**.
The subsequent session below records the parts now exercised with an iPhone.

### Official iPhone received-file actions session, 2026-10-15

| Session field | Value |
| --- | --- |
| Tester | jfg96, with assisted protocol, hash and clipboard checks |
| Source | PR #32, `4f839f87cb02423655decedb8b6886e7c339c55e`, plugin `1.2.7-dev` |
| Helper | Published `omarchy-nearby-helper 1.2.6`, selected by the PR's unchanged release metadata |
| Host | Omarchy dev `4.0.0.r7017.g84cfb93-1`, Quickshell `0.3.2-4`, Qt `6.12.0-2` |
| Image and clipboard tools | ImageMagick `7.1.2.33-3` Q16-HDRI, file `5.48-1`, wl-clipboard `2.3.0-1` |
| Peer | Official LocalSend on an iPhone, protocol `2.2`, HTTPS; device model, iOS and app versions not rechecked |
| Network and display | Same Wi-Fi LAN, one monitor; VPN/firewall configuration not rechecked |

The unmodified PR service, panel and copy script ran in a temporary Quickshell
host using Omarchy's real `Ui` and `Commons` components. The host supplied a
separate configuration API and isolated helper state and receive directories;
it did not replace the installed plugin. Test files were sent to the iPhone
and returned through its official LocalSend client. Incoming approval and the
visible panel were exercised by the tester; clipboard checks used the real
Wayland clipboard, with the panel's Copy action also callable by the harness.

| Scenario 24 component | Result | Evidence or deviation |
| --- | --- | --- |
| Single PNG receive, preview and Copy | Pass | The 800x600 gradient arrived byte-for-byte intact, its preview was confirmed on screen, and the PNG copied from the result was pasted successfully into Codex |
| PNG metadata removal | Pass | The input carried a private test comment and text chunks; the copied PNG retained only `IHDR`, `orNT`, `IDAT` and `IEND`, with identical decoded pixels |
| JPEG receive and oriented image copy | Pass | A 4000x3000 JPEG with EXIF orientation 6 arrived unchanged; Copy produced a 3000x4000 PNG whose decoded RGBA hash matched the original after orientation, and it was pasted into Codex |
| JPEG preview orientation on screen | Not run | Orientation and pixels of the copied image were verified; the JPEG preview's orientation was not separately confirmed visually |
| Non-image receive, Copy and file-manager paste | Pass | A 23-byte TXT arrived unchanged, was copied as `text/uri-list`, and pasted into Flea; the pasted file's SHA256 matched the original. This used TXT rather than the scenario's PDF |
| Text renamed to `.png` | Pass | Detected as `text/plain`, no image preview was shown, and Copy produced a file reference rather than image data |
| Multiple files in one receive | Pass | PNG and TXT were returned together; both hashes matched, and the result had no single-file preview or Copy action. This used mixed files rather than two pictures |
| Paste into a separately tested Chromium browser, GTK app and image editor | Not run | Successful image and file pastes in Codex and file paste in Flea do not claim those additional targets were exercised |

The JPEG source and received SHA256 was
`6981739476206363862e880c1d3dc0a75399efeb14cda322682d2fe25fc634ea`.
The copied and expected oriented RGBA SHA256 was
`1d494804045e6afb963f1b2667b563860c06dbef9a460fe2944b88f3a43ed283`.
The PNG source and received SHA256 was
`da5f466ebfaa217608d5ecf2fd184d9b24c02f8a198bb69ad825824c98335b40`.
The TXT, its pasted copy and the text renamed to PNG matched
`2613cbb051ddc41d0ddddad7ab2d213acae2ed24f598e6c984c9ad58414ff7d8`.

The iPhone initially received without a manual Accept. The tester checked that
LocalSend Quick Save was enabled; this observation does not demonstrate an
authorization bypass. After the initial harness setup, PC auto-accept was disabled for the
recorded return transfers. One TXT request expired while awaiting approval;
its retry completed after acceptance. The temporary panel had to be opened
manually for some requests, so this session does not validate the installed
shell's complete notification-routing behavior.

Cancellation, a live Copy completion after another received result, multiple
monitors, Android interoperability and loading this PR as the normally
installed plugin remain **Not run** in this session. The scenario above is a
partial manual pass, not completion of the entire regression matrix. The
temporary receiver and monitor were stopped afterwards and the previously
enabled normal receiver was restored to ready. No contributor code was changed
for the test.

### Combined PR integration regression validation, 2026-10-15

PR #32 was rebased onto main after PR #33 was merged. The contributor's
commits and authorship are retained. Maintainer changes discard late folder
choices while a transfer, approval request or PIN update is active and keep a
file-copy completion from annotating a different file or a dismissed result.
Scenarios 23 (folder selection) and 24 (received file actions) now have distinct
numbers; the earlier iPhone evidence above refers to the pre-rebase PR #32 code.

Both new Node regression cases failed against the pre-fix behavior: the late
folder choice persisted new settings during reception, and Copy for file A
reported success under file B. They pass with the fixes, along with all three
Node suites and the three Bash suites. Rust formatting, locked build checks,
Clippy with warnings denied, helper tests and the standalone HTTPS vendor suite
also passed; the vendor suite retains one upstream ignored test. A separate Quickshell 0.3.2 instance
loaded the combined Service.qml with a simulated helper: returning a folder
choice after entering `receiving` left the setting unchanged, kept the helper
ready with a single launch, and preserved the receiving view. This used no
network peer and does not count as a real-device race test. Full live folder
selection/receiving and the copy race with a real peer remain **Not run**.

## Manual interoperability matrix

Run applicable transfer and PIN scenarios with both Android and iOS LocalSend
peers. Record exact app and OS versions rather than "latest." The discovery
timing scenario below specifically describes iOS; record Android discovery
separately. Keep devices on the same non-guest LAN and confirm TCP and UDP
53317 are allowed. Capture helper stderr through the test session's shell logging
when checking discovery messages; stdout is reserved for protocol events.

1. Enable Nearby, leave its popup closed, open LocalSend on iPhone, then open Nearby.
   The phone must already be in the snapshot. Repeat with LocalSend open first: the helper log
   should show the initial multicast at approximately `+0 ms`, the phone within `+1000 ms`,
   and `HTTP fallback skipped`. If that iOS state does not answer multicast, `HTTP fallback
   started` should appear at approximately `+1000 ms`, not after the retry sequence.
   Close and reopen Nearby within 90 seconds: if multicast still does not confirm the phone,
   the log must show its IP under `cached candidates`, then `cached peer confirmed` and
   `CACHE HIT`; it must not show `full subnet scan started`. Change the phone's IP and repeat:
   the cached probe must fail, the full scan must run without probing the old IP twice, and the
   registry must update to the new address when the phone is found.
2. Close LocalSend and wait longer than 90 seconds. Reopen Nearby: the stale phone must
   be absent. Reopen LocalSend: it must reappear from a new event.
3. Send iPhone → Nearby, leave the approval unanswered for over 60 seconds, then try
   Accept. Nearby must report expiration and the phone must see rejection/timeout.
4. Accept a large file, cancel from iPhone halfway, and repeat by disabling Wi-Fi.
   Nearby must show Cancelled/Failed, never Received, and Downloads must contain neither
   the final name nor a `.part` file.
5. Throttle the sender enough to exceed five minutes. It must complete without the
   receiver sweeper terminating it.
6. Send two files with the same name in close succession. Both validated files must
   exist with collision suffixes and correct contents.
7. Transfer a large file while sending another file in the opposite direction. Incoming
   progress must not replace outgoing UI state, and Cancel must affect only the visible
   outgoing transfer.
8. Kill `omarchy-nearby-helper` during send and receive. The UI must show a coherent
   backend-stopped error. With the popup still open, a successful bounded restart must
   resume active discovery. A permanent port conflict must stop after four retries.
9. Toggle Nearby ON→OFF→ON repeatedly and verify with `ss -lntup` that OFF leaves no
   TCP/UDP 53317 listener or helper process.
10. Test aliases, filenames and text containing `<b>`, quotes, `$()`, newlines and emoji.
    They must render literally and must not execute commands or markup.
11. Enable Nearby incoming PIN `123456`. From official LocalSend, verify missing
    and incorrect PINs do not surface a request, while the correct PIN reaches
    the normal Accept/Decline flow for both text and files.
12. Repeat with incoming PIN `Abc-_.~09`, then change it while discovery remains
    active. The old value must fail, the new value must work and the receiver
    must keep the same listening port.
13. Begin and accept a file transfer, change the incoming PIN while it is in
    progress and confirm that the transfer completes byte-for-byte.
14. Restart Nearby and confirm the saved PIN is required on the first request.
    Disable it and confirm a subsequent request no longer prompts for a PIN.
15. Configure official LocalSend receiver PINs containing a space, Unicode and
    each of `+`, `&`, `#` and `%`; verify Nearby can send text and files using
    each exact value.
16. On a clean installation with the receiver off, confirm no helper is
    downloaded. Turn it on with network access, confirm the pinned helper lands
    under `$XDG_DATA_HOME/omarchy-nearby/helpers`, then disconnect the network
    and restart Nearby; the verified cached helper must start offline.

Before publishing a stable release, record the exact Android and iOS LocalSend
versions used for steps 11–15 here. These real-device checks are not considered
complete until those versions and results are written down.

## Additional release scenarios

17. Open Nearby on two monitors. Close one popup and confirm discovery continues
    for the other. Close both and confirm receiving remains available. Disable
    Nearby and confirm the single helper stops.
18. Repeat helper recovery with an unavailable network and an invalid cached
    executable in an isolated test installation. Confirm useful errors and
    successful recovery when connectivity returns; never change committed hashes.
19. Build a local helper, confirm its marked override takes priority, then follow
    [the return-to-published procedure](CONTRIBUTING.md#return-to-the-published-helper).
    Confirm the selected published helper runs after restarting the receiver.
20. Exercise file selection and received-text copying. Check incoming notifications
    with the panel closed, with the request visible, and with Do Not Disturb on.
21. In an isolated test installation, start with unsafe `settings.json` and
    confirm a visible error without retries or a listener. Restore a regular,
    valid file while preserving its incoming PIN; enable Nearby again and
    confirm that the PIN still applies. Never replace personal settings with
    a FIFO or delete them as a generic recovery step.
22. With an official LocalSend client, send and receive a folder containing
    several nested files and verify exact contents, paths, active file names,
    progress and completion. Receive it again: directories merge and colliding
    files receive suffixes inside their original parents. Verify PIN flows,
    decline, cancel while awaiting approval, cancel during a large upload and
    immediate retry. Repeat with the panel closed and, where available, on
    multiple monitors. Check unreadable folders fail preparation, inner
    symlinks are skipped and hidden components are refused. Record peer OS and
    LocalSend version; Rust loopback tests do not count as this manual pass.
23. Choose **Save to**, cancel the chooser and confirm nothing changes. Choose a
    folder: the receiver must restart once, the row must name the folder, and a
    received file must land there. Restart the shell and confirm the folder is
    still used. Set `downloadDir` to a path that cannot be created and confirm
    the panel names it after the retries and still offers **Use Downloads**;
    choose it and confirm the receiver starts, `downloadDir` is gone from the
    entry and files go to Downloads again. Choose your home folder and confirm
    it is refused with the reason. Open the chooser, start receiving or sending
    before choosing a folder, then finish the chooser: the transfer must stay
    active and the folder setting must remain unchanged. Repeat with a pending
    approval or outgoing PIN request.

24. Receive one PNG and one JPEG photo taken in portrait orientation, in
    separate transfers. Each must show an upright preview with **Copy** and
    **Done**. Copy each and paste it into a Chromium-based application, a GTK
    application and an image editor: the picture must arrive upright. Receive a
    PDF: there must be no preview, and Copy must paste the file into a file
    manager. Send two pictures in one transfer: the result must stay plain.
    Receive a text file renamed to `.png`: there must be no preview.

25. In an agreed development installation, keep the panel open and switch
    between English and Spanish under **Setup > Language**. Verify buttons,
    rotating phrases, tooltips and chooser titles update without restarting.
    Repeat with German, Finnish, Bulgarian and Urdu and inspect long
    half-width buttons. Switch to Arabic and inspect mirrored rows, Back
    chevrons and horizontal keyboard navigation. Check queued-request plurals
    in Russian (2, 5, 21), and confirm translated notifications still escape
    remote markup and preserve literal `%N` inside peer names. Restore the
    original language. Exercise English fallback without shell translations.

26. On an aarch64 machine, install a release whose metadata selects an aarch64
    helper and enable Nearby with no local build: the launcher must download
    the `linux-aarch64` asset into the XDG helper cache and the receiver must
    become ready. Send and receive a file with a LocalSend peer. On the same
    release, confirm an x86_64 machine still downloads only the x86_64 asset.

## Result record

Copy this template for each validation session. Leave unexecuted cases marked
**Not run**; a plan, source inspection or automated test is not a manual pass.

| Session field | Value |
| --- | --- |
| Date and tester | Not recorded |
| Plugin version and exact commit | Not recorded |
| Helper version and published/local build | Not recorded |
| Omarchy version | Not recorded |
| Android device, OS and LocalSend version | Not recorded |
| iOS device, OS and LocalSend version | Not recorded |
| Network, VPN/firewall and monitor arrangement | Not recorded |

| Scenarios | Peer/platform | Result | Evidence or deviation |
| --- | --- | --- | --- |
| 1–2: discovery and expiry | Android / iOS, separate rows when run | Not run | |
| 3–7: transfer lifecycle | Android / iOS, separate rows when run | Not run | |
| 8–10: restart, toggles and literal rendering | Omarchy + peer | Not run | |
| 11–15: PIN interoperability | Android / iOS, separate rows when run | Not run | |
| 16, 18–19: helper distribution | Omarchy | Not run | |
| 17: multiple monitors | Omarchy | Not run | |
| 20: desktop integration | Omarchy + peer | Not run | |
| 21: unsafe settings and recovery | Isolated Omarchy installation | Not run | |
| 22: folder transfers and lifecycle | Official Android / iOS / desktop client, separate rows | Not run | |
| 23: folder for received files | Omarchy + peer | Not run | |
| 24: received file preview and copy | Omarchy + peer | Not run | |
| 25: display language and right-to-left layout | Omarchy | Not run | |

For failures, identify the exact scenario and attach redacted logs or a
reproduction. Split grouped rows whenever individual outcomes differ.
This template introduces no claim about past releases' manual test results.

## Protocol and trust scope

Keep compatibility claims tied to recorded peer versions and test outcomes.
The vendor tests include a declared SHA-256 mismatch returning `422`.
Discovery is not authenticated pairing; TLS fingerprint handling does not
remove that discovery trust limitation. See [security](docs/SECURITY.md).

## Translation development check — 2026-10-15

The working `feat/i18n` tree based on `4dbb866` was tested in an isolated
Quickshell process, with a private `OMARCHY_LANGUAGE_FILE` and the receiver
disabled. It compiled Panel.qml under Wayland, loaded Service.qml and Strings.qml,
and changed English → es-MX → Arabic without restarting. English and Spanish
labels and plural strings updated, remote `%2` remained literal, and the Arabic
language setting enabled the RTL flag with English catalogue fallback. Startup
showed readable English without binding loops. This does not validate the visual
layout or Arabic translations. The desktop's own language/plugin settings were
not changed. Only existing shell deprecation warnings for FloatingWindow width
and height remained.

The four Node suites and three Bash suites passed. Rust is unchanged and its
suites were not run for this frontend-only change. Scenario 25 on the actual
installed panel, long translated labels, other language catalogues, real peers
and an actual older Omarchy installation remain untested.

A separate isolated component with an intentionally unavailable translations
type exercised the fallback loader: English was present from the initial
component completion, plural strings resolved, remote placeholders stayed
literal, and no binding loop occurred. This simulates the missing type; it
does not replace validation on an actual older Omarchy release.

## Complete catalogue validation — 2026-10-15

The working tree based on `b3a7453` adds the remaining 48 catalogues, covering
all 50 codes in the installed Omarchy registry, with 145 keys per catalogue
(144 after the panel layout change removed the unused `phrase.findingDevices`).
Each was validated after generation. A cross-language sample review corrected
Italian/Finnish plurals, Arabic dual/zero forms, Romanian PIN grammar, Irish
receive/retry labels and several chooser titles. These are AI-assisted
translations without comprehensive native-speaker review.

With strict completeness enabled, the four Node suites, three Bash suites and
`git diff --check` pass. An isolated Wayland Quickshell process switched through
all 50 languages and performed 8,000 comparisons of strings, selected plural
forms, literal remote placeholders and RTL flags, with zero mismatches or binding
loops. Metrics for the actual shell Button component found no sampled half-width
control exceeding its label room at the component's minimum font size. This is
a measurement check, not a visual review of every panel state. Qt logged font
OpenType-support messages for some South Asian scripts, so this run does not
establish correct glyph rendering for all languages.

The Wayland harness also opened an unintended blank window while loading shell
components. Its processes were terminated and confirmed absent. It did not
change the desktop language or the managed Nearby installation. Further checks
for this work use non-windowed tools. Full visual review, native-language review
and real-peer transfer testing remain unperformed. Rust/helper code is unchanged.

## Release check — v1.2.7, 2026-10-15

Plugin `1.2.7` from `main` at `775e0a5` (#32, #33, #38, #39), reusing the
published `helper-v1.2.6`. `backend/` is unchanged since its source commit
`b187afc`, so no helper is rebuilt or republished.

The complete supported suite passed locally on that tree: the four Node suites
(50/50 catalogues, 144 keys), the three Bash suites, `cargo fmt --check`,
`cargo check`, `cargo clippy -D warnings`, the helper tests and the vendored
`localsend-rs` tests with `https`. CI passed on `775e0a5`.

The panel layout from #38 was previewed on Omarchy (dev) by hot-reloading the
installed plugin, with one discovered peer and with an empty list. Scenario 25,
real-peer transfers with a translated panel, and an actual older Omarchy
release remain **Not run**.

## aarch64 helper development check — 2026-10-15

Plugin `1.2.8-dev` adds optional aarch64 metadata, launcher selection by
`uname -m`, native aarch64 helper builds in Helper release and aarch64 CI.
No aarch64 helper has been published, so `helper-release.env` still selects
only `helper-v1.2.6` for x86_64.

On an Apple MacBook Pro (M1 Max, Asahi kernel with 16 KiB pages) running
Omarchy 4.0.3, the unchanged launcher failed with "no prebuilt helper is
available for Linux/aarch64". `build.sh` produced a native aarch64 helper that
reported `omarchy-nearby-helper 1.2.6`; with that local override the receiver
reached ready. The complete supported suite passed on that machine, using Rust
1.99.0 rather than the pinned 1.97.1. The new launcher cases failed against
the previous launcher, which rejected the aarch64 fields as unknown.
actionlint 1.7.7 reported no workflow errors; the workflows themselves,
including the `ubuntu-24.04-arm` jobs, have not run. Scenario 26 and transfers
with a real peer from the aarch64 machine remain **Not run**.

## Helper 1.2.7 preparation — 2026-10-15

Prepare a new helper tag for the native x86_64/aarch64 distribution from #41.
Only the helper package version and lockfile advance from 1.2.6 to 1.2.7;
helper behavior and vendored source are unchanged. The plugin remains
1.2.8-dev and requires helper 1.2.7. Published metadata remains at 1.2.6
until the new workflow artifacts have been independently verified.

The complete supported suite passed locally with Rust 1.97.1: four Node
suites, three Bash suites, formatting, cargo check, Clippy with warnings
denied, helper tests and the vendored localsend-rs HTTPS suite. The plugin
validator and git diff --check passed. ARM64 compilation and execution are
validated by native GitHub runners; this machine is x86_64. Real-device ARM64
transfers and scenario 26 remain Not run.

## Release check — v1.2.8, 2026-10-15

Plugin 1.2.8 selects the published helper-v1.2.7 for Linux x86_64 and
aarch64, from source f2dec01994010a52df511f72ae687d516be3a8f9. Helper
preparation #42 and its merge commit passed CI on both native architectures.
Helper release run 38748042831 built, tested and attested both executables,
checked each --version natively, and published both binaries, their checksums
and the license page in one prerelease.

Both published executables were downloaded independently. Their checksum files,
32 MiB size bound, ELF architecture and GitHub provenance passed verification
against the exact repository, source digest, helper tag and release workflow,
with self-hosted runners denied. The downloaded x86_64 helper reports
omarchy-nearby-helper 1.2.7 locally; the ARM64 executable's native version check
is recorded by the helper build, and independent native verification of the
published download will run in Verify release before stable advances.

The full supported suite passed locally during helper preparation with Rust
1.97.1. The release metadata is derived from the published bytes; backend/
remains identical to the helper source. Scenario 26 (real installation and
peer transfers on ARM64) remains Not run; there is no local ARM machine.
No real-peer transfer or installed desktop smoke test is claimed for this
release.

On the final plugin release tree, all four Node and three Bash suites, the
plugin validator, whitespace checks and the helper-source tree comparison
passed. The launcher prefetched the real published x86_64 helper into an
isolated XDG cache and reported 1.2.7.
