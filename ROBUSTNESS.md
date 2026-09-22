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
cancel/completed/failed event separation, truncated and checksum-mismatched uploads,
atomic equal-name commits, traversal rejection, progress backpressure, TLS pinning,
HTTP `/register` fallback, and a session whose activity is older than five minutes while
an upload is still active. Incoming-PIN coverage includes secure startup,
atomic persistence and rollback, exact `401`/`429` behavior, bounded per-IP
failure state, text/file authorization before events, live PIN changes and an
already authorized upload continuing after a change.

The distribution suites additionally cover immutable helper metadata, exact
size/SHA256 enforcement, XDG data storage, offline reuse, corrupt and symlinked
cache repair, concurrent launches, failed/oversized downloads, and the rule
that published helpers never write into the watched plugin checkout.

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

For failures, identify the exact scenario and attach redacted logs or a
reproduction. Split grouped rows whenever individual outcomes differ.
This template introduces no claim about past releases' manual test results.

## Protocol and trust scope

Keep compatibility claims tied to recorded peer versions and test outcomes.
The vendor tests include a declared SHA-256 mismatch returning `422`.
Discovery is not authenticated pairing; TLS fingerprint handling does not
remove that discovery trust limitation. See [security](docs/SECURITY.md).
