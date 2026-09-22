# Troubleshooting

[Documentation index](../README.md#documentation)

## Check receiver state

Open Nearby and read the full error detail. For a compact state report:

```sh
omarchy-shell oma.nearby status
```

| Field | Meaning |
| --- | --- |
| `enabled` | Receiver is configured on |
| `running` | Helper process is running |
| `ready` | Backend has reached the service's ready state |
| `devices` | Number of devices in the current service list |
| `helper` | Reported helper version; may be empty before startup |
| `requires` | Minimum helper version required by the plugin |
| `updateOffered` | Whether the service offers helper recovery/update |

During startup these values can differ. `running: true` alone does not establish
readiness, and `devices: 0` does not prove the network is broken.
If the command cannot find the service, check that the plugin is installed and
enabled in Omarchy.

If `omarchy plugin` itself is unavailable, the installed Omarchy environment
does not provide the manager required by the documented installation flow.
Confirm that environment's plugin support before attempting installation.

## Devices do not appear

1. Confirm Nearby is on and the other device's app is open.
2. Put both devices on the same non-guest LAN. Check for Wi-Fi client isolation,
   VPN routing or firewall rules that prevent local TCP/UDP `53317` traffic.
3. Open the Nearby popup to start active discovery.
4. Use **Search for new devices** to force the bounded subnet fallback.
5. For an iOS app that was already open, try closing and reopening LocalSend.

Cached addresses are revalidated; they do not prove availability. Large networks
are deliberately bounded rather than scanned exhaustively. See
[the discovery model](ARCHITECTURE.md#discovery-model).

## Port 53317 is already in use

Another LocalSend instance or helper may already own the port. Inspect listeners:

```sh
ss -lntup 'sport = :53317'
```

Process details may be limited by permissions. Close the competing application
normally, then turn Nearby off and on. Nearby uses bounded startup retries; it
does not retry a permanent conflict indefinitely.

## Helper download or verification fails

Check internet access to GitHub and the panel's detailed message, then choose
**Retry helper** or run:

```sh
omarchy-shell oma.nearby retryHelper
omarchy-shell oma.nearby status
```

Repair is asynchronous: `ok` acknowledges the request and `busy` means a repair
is already in progress. Wait for the panel's result before interpreting status.

Repair selects the exact asset in the installed checkout's metadata. It verifies
and reuses a valid cache or replaces invalid bytes. Do not edit checksums or
disable verification to make a download pass. Offline startup requires that
exact helper to have been cached already.

For a missing or malformed `helper-release.env`, update the plugin through
Omarchy's manager; helper repair does not repair source files. Prebuilt helpers
support Linux x86_64. See [Contributing](../CONTRIBUTING.md) for local builds.

If an old helper still runs after repair, check whether you previously used
`build.sh`. A marked developer build takes priority until you
[disable its override](../CONTRIBUTING.md#return-to-the-published-helper).

## PIN rejected or sender blocked

Use the destination's PIN when sending. Nearby's own Incoming PIN only protects
requests coming to Nearby.

Three supplied incorrect values from one IP block that IP. After confirming the
correct credential, the receiver owner can change/disable its PIN or restart
the receiver to clear the failure state. Restarting interrupts active work.
Missing PINs do not consume attempts.

Nearby's incoming PIN allows 1–64 ASCII letters, numbers, dots, underscores,
tildes and hyphens. Outgoing PINs support other characters accepted by the peer.

## File selection or clipboard fails

Check that the session provides the required commands:

```sh
command -v omarchy-file-select wl-copy wl-paste omarchy-notification-send
```

Use **Send files** for file content. **Send clipboard** expects text and has a
1 MiB UTF-8 limit. Confirm that copying text works in the current Wayland session;
do not paste private clipboard contents into a bug report.

## Receiver security state or download destination is unavailable

Check the panel error and permissions on the
[documented storage locations](USAGE.md#storage). The download directory must be
accessible and writable. Invalid or unsafe PIN settings or TLS identity files
can stop startup. Preserve those files for diagnosis rather than deleting them
as a generic reset.

For `settings.json`, startup rejects a final symlink, a FIFO or other non-regular
file, a file owned by another user, JSON larger than 16 KiB, and malformed or
unsupported settings. Repair the file while preserving the intended incoming
PIN; do not share its contents in a report. **Retry helper** repairs only the
published executable and cannot repair invalid receiver settings.

## Report a reproducible problem

Include the Nearby plugin and helper versions, Omarchy version, peer app version,
operating systems, exact steps, expected result and actual result. Add the
`status` output and full panel error, noting whether a local helper override is
active. For discovery problems, describe the network arrangement and whether
the peer app was already open.

Redact personal paths, addresses and device names as needed. Never attach private
keys, PIN settings, clipboard content or unrelated received files. Distinguish
automated test results from real-device observations.
