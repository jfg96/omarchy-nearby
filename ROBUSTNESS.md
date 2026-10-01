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
```

The suites cover peer registry retention/expiry, command correlation, request decisions,
cancel/completed/failed event separation, oversized, truncated and checksum-mismatched uploads,
repeated and concurrent uploads of one file, sender-supplied file ids in partial paths,
the incoming file-count limit, cancellation stopping an upload in progress,
connection limits, request head/body timeouts and upload idle timeouts,
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
| iOS device, OS and LocalSend version | iPhone; exact model, iOS and LocalSend versions not recorded |
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
passed the manual iPhone retest at `b6546d0`. Exact iOS and LocalSend versions
must be recorded before a stable release.

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

For failures, identify the exact scenario and attach redacted logs or a
reproduction. Split grouped rows whenever individual outcomes differ.
This template introduces no claim about past releases' manual test results.

## Protocol and trust scope

Keep compatibility claims tied to recorded peer versions and test outcomes.
The vendor tests include a declared SHA-256 mismatch returning `422`.
Discovery is not authenticated pairing; TLS fingerprint handling does not
remove that discovery trust limitation. See [security](docs/SECURITY.md).
