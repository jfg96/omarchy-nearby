# Using Nearby

[Documentation index](../README.md#documentation)

## Send files, folders or text

Open the peer's LocalSend-compatible app and keep both devices on the same LAN.
Open Nearby, turn the receiver on, and select the destination.

- **Send files** opens Omarchy's file chooser and supports multiple files.
- **Send folder** opens the directory chooser. Files are sent with the selected
  folder name and their relative hierarchy, in alphabetical path order.
- **Send clipboard** sends the current clipboard. Text is sent as text, limited
  to 1 MiB of UTF-8 data. A picture with no text alongside it, such as a
  screenshot or an image copied from a browser, is sent as a file named
  `clipboard-<date>-<time>` with its type's extension, up to 100 MiB. When the
  clipboard offers both, as copied spreadsheet cells do, the text is sent. The
  picture is saved in a private folder under `$XDG_RUNTIME_DIR`, which keeps
  only the latest one and is emptied when the session ends.
- If the destination requires a PIN, enter that destination's PIN when asked.
  It is independent of your own Incoming PIN setting.

Folder transfers allow at most 32 directory levels and 10,000 files. Each
relative file path is limited to 4096 UTF-8 bytes and each component to 255
bytes. The complete offer must also fit within 2 MiB of JSON; long or escaped
names can reach that limit with fewer files. Split the selection if needed.
Hidden components (names starting with `.`), empty segments, traversal, Windows
separators and invalid UTF-8 names are refused. Unreadable files or directories
and special files fail the whole preparation rather than reporting a partial
selection as successfully sent.

A symlink explicitly selected at the top level is resolved and uses its selected
name. Symlinks inside a selected folder are skipped. Empty directories are not
represented by the protocol; a selection with no transferable files fails.

Approve the transfer on the destination. Nearby displays progress and offers
**Cancel** for outgoing transfers. After a couple of seconds, a line under the
percentage shows the speed and the time left, averaged over the last few
seconds; it does the same while receiving. Wait for completion before disconnecting.

If the peer is missing, use **Rescan**. See
[discovery troubleshooting](TROUBLESHOOTING.md#devices-do-not-appear).

## Receive

Leave Nearby on, choose its device entry on the sender and send files or text.
Use **Accept** or **Decline** for the incoming request. A text message goes
through the same approval: its request shows up to four lines of the message,
so you can see what you are accepting, while its notification only says that
a message is waiting. Approval requests expire; ask the sender to retry if
the request has expired.

Files go to your configured Downloads directory unless you
[choose another folder](#folder-for-received-files). Existing files are preserved
by assigning collision suffixes. Folder hierarchies are created inside that
destination; an existing directory is merged with the incoming directory and
individual colliding files receive suffixes, for example
`Photos/sub/image (1).jpg`. Incoming paths follow the same path limits and
hidden-component policy as outgoing paths and are checked before approval.
Existing symlinks along the destination path are refused before writing and
again at final publication. Received text appears in the panel; choose
**Copy** to put it on the clipboard, or **Done** to dismiss it.

When a transfer delivers exactly one file, the result offers **Copy** for it,
so it can be pasted without opening a file manager. A PNG, JPEG, WebP, GIF,
BMP, TIFF, HEIC/HEIF, AVIF or JPEG XL picture is also previewed, and is copied
as a PNG image, which is the image
type every paste target accepts; the copy carries no metadata such as the
location a photo was taken at or a comment stored in a PNG. Copying a large
picture can take a few seconds. Any other file, and a picture that cannot be
converted, is copied as a file reference, the way a file manager copies it.
BMP, TIFF, HEIC, AVIF and JPEG XL are previewed from a small PNG that
ImageMagick converts under `$XDG_RUNTIME_DIR`: a stock shell cannot decode the
formats phones often save photos in, and Nearby keeps the others out of the
shell's process. The file in Downloads is never changed. A transfer of several
files shows the plain result.

Incoming notifications use Omarchy's notification system and respect Do Not
Disturb. Clicking an incoming-transfer notification opens the panel on the
focused monitor with the request shown; it never accepts or declines it. Once
the request has expired or been answered, the click does nothing, including
from the notification history. On a shell whose notification helper predates
click actions, the notification appears without one. A request already
displayed in the open panel does not also need a duplicate notification;
requests waiting behind another view can still notify.

## Receiver and popup

| State | Behavior |
| --- | --- |
| Nearby off | Helper stopped; no receiving or discovery |
| Nearby on, all popups closed | Receiver and passive discovery available |
| Nearby on, a popup open | Active discovery also runs |

The popup can be used from the keyboard: the arrow keys show and move the
cursor, Enter or Space activates the highlighted item, and Escape goes back.
Enter and Space do nothing until an arrow key has shown the cursor, so typing
meant for another window when the popup opens cannot turn Nearby off or accept
a transfer.

On multiple monitors, the views share one receiver. Closing one popup does not
stop active discovery needed by another open popup.

## Folder for received files

The **Save to** row under Nearby's device list names the folder the receiver
is saving to. Choose it to pick another folder with Omarchy's file chooser.
Nearby restarts its receiver to apply the change, which takes about a second;
if a transfer or approval request starts before the chooser returns, its result
is discarded. Choose the folder again once the operation finishes.

The choice is stored as `downloadDir` in Nearby's entry in
`~/.config/omarchy/shell.json`, beside the receiver switch, and is handed to
the helper as `XDG_DOWNLOAD_DIR`. While a folder is chosen, **Use Downloads**
under that row removes the choice, which goes back to the Downloads lookup
described under [storage](#storage). Your home folder and the folders above it
cannot be chosen: received files keep the names their sender gave them, so
they belong in a folder of their own.

A folder that is missing is created when the receiver starts. If it cannot be
created, for example because it is on a drive that is no longer connected, the
receiver does not start and the panel names the folder. **Use Downloads**
stays in the panel then and starts the receiver again. So does reconnecting
the drive and turning Nearby off and on.

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
| Received files | The [chosen folder](#folder-for-received-files) if there is one; otherwise `XDG_DOWNLOAD_DIR`, then the setting in `$XDG_CONFIG_HOME/user-dirs.dirs` (default `~/.config/user-dirs.dirs`), then `~/Downloads`. Relative locations are ignored, as are `user-dirs.dirs` files that are not regular, are owned by another user or exceed 16 KiB |

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
| `omarchy-shell oma.nearby showRequest <id>` | Open the popup if incoming request `<id>` is still pending, otherwise report `expired`; used by the incoming notification |

Popup commands do not toggle the receiver. An `ok` response acknowledges a
command; it does not prove that startup, repair or a transfer has completed.
Check the panel or `status` afterward.
