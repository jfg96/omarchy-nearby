# Using Nearby

[Documentation index](../README.md#documentation)

## Send files or text

Open the peer's LocalSend-compatible app and keep both devices on the same LAN.
Open Nearby, turn the receiver on, and select the destination.

- **Send files** opens Omarchy's file chooser and supports multiple files.
- **Send clipboard** sends the current clipboard as text, not an image or file.
  Outgoing text is limited to 1 MiB of UTF-8 data.
- If the destination requires a PIN, enter that destination's PIN when asked.
  It is independent of your own Incoming PIN setting.

Approve the transfer on the destination. Nearby displays progress and offers
**Cancel** for outgoing transfers. Wait for completion before disconnecting.

If the peer is missing, use **Search for new devices**. See
[discovery troubleshooting](TROUBLESHOOTING.md#devices-do-not-appear).

## Receive

Leave Nearby on, choose its device entry on the sender and send files or text.
Use **Accept** or **Decline** for the incoming request. Approval requests expire;
ask the sender to retry if the request has expired.

Files go to your configured Downloads directory. Existing files are preserved
by assigning collision suffixes. Received text appears in the panel; choose
**Copy** to put it on the clipboard, or **Done** to dismiss it.

Incoming notifications use Omarchy's notification system and respect Do Not
Disturb. A request already displayed in the open panel does not also need a
duplicate notification; requests waiting behind another view can still notify.

## Receiver and popup

| State | Behavior |
| --- | --- |
| Nearby off | Helper stopped; no receiving or discovery |
| Nearby on, all popups closed | Receiver and passive discovery available |
| Nearby on, a popup open | Active discovery also runs |

On multiple monitors, the views share one receiver. Closing one popup does not
stop active discovery needed by another open popup.

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

A PIN does not replace local approval. Three supplied incorrect PINs from one IP
block further requests from that IP until the security state changes or the
receiver restarts. Missing PINs do not consume an attempt. See
[security](SECURITY.md) for the trust model.

## Storage

| Data | Location |
| --- | --- |
| Published helpers | `$XDG_DATA_HOME/omarchy-nearby/helpers`, default `~/.local/share/omarchy-nearby/helpers` |
| Incoming PIN settings | `$XDG_STATE_HOME/omarchy-nearby/settings.json`, default `~/.local/state/omarchy-nearby/settings.json` |
| Persistent TLS identity | `identity.json` alongside the PIN settings |
| Received files | `XDG_DOWNLOAD_DIR`, then the setting in `$XDG_CONFIG_HOME/user-dirs.dirs` (default `~/.config/user-dirs.dirs`), then `~/Downloads` |

Interrupted receives use `.nearby-*.part` files. At startup, Nearby removes only
its own partial files older than 24 hours. Treat settings and identity files as
private; do not include their contents in bug reports.

## Shell commands

These commands address the loaded `oma.nearby` service in the Omarchy shell:

| Command | Effect |
| --- | --- |
| `omarchy-shell oma.nearby open` | Open the popup on the focused monitor |
| `omarchy-shell oma.nearby close` | Close the routed popup |
| `omarchy-shell oma.nearby toggle` | Toggle the popup |
| `omarchy-shell oma.nearby receiverOn` | Enable receiving |
| `omarchy-shell oma.nearby receiverOff` | Disable receiving |
| `omarchy-shell oma.nearby receiverToggle` | Toggle receiving |
| `omarchy-shell oma.nearby status` | Report service state as JSON |
| `omarchy-shell oma.nearby retryHelper` | Request verification/repair of the selected published helper |

Popup commands do not toggle the receiver. An `ok` response acknowledges a
command; it does not prove that startup, repair or a transfer has completed.
Check the panel or `status` afterward.
