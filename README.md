# Omarchy Nearby

Native nearby sharing for Omarchy Quattro, compatible with the LocalSend protocol.

Open Nearby from the bar, choose a device, and send files or clipboard text.
Receive transfers with explicit approval, follow their progress, and optionally
protect incoming requests with a PIN. Nearby runs its own Rust helper; the
LocalSend application is not required on your Omarchy computer.

> Independent project. Not affiliated with or endorsed by LocalSend or Omarchy.

## Screenshots

### Discover devices

![Nearby discovering compatible devices on the local network](assets/screenshots/nearby-discovery.png)

### Send files or clipboard text

![Nearby actions for sending files or clipboard text to a selected device](assets/screenshots/nearby-send-actions.png)

## Requirements

- Omarchy Quattro with its Quickshell-based shell and `omarchy plugin` manager.
- Linux x86_64 for the published helper.
- A compatible peer on the local network, such as a device running LocalSend.
- TCP and UDP port `53317` available for LocalSend traffic.
- Omarchy's `omarchy-file-select` and `omarchy-notification-send` commands,
  plus `wl-copy` and `wl-paste` from `wl-clipboard`.
- Internet access for the first helper download, or when an update selects a
  helper that is not cached. The launcher uses Bash, curl and standard GNU
  utilities, including `sha256sum` and `stat`.

The published installation does not require Rust, compile code, install packages
or request administrator privileges. For a local build, see
[Contributing](CONTRIBUTING.md).

## Install

Install and enable through Omarchy's plugin manager:

```sh
omarchy plugin add https://github.com/jfg96/omarchy-nearby --enable
```

Add `--yes` for unattended installation. The widget appears in the right section
of the bar by default.

When the receiver starts, the launcher downloads the exact helper selected by
the checkout, verifies its size and SHA256, and caches it in the user's XDG data
directory. Later starts reuse verified cached bytes without internet access.
While the receiver is off, normal startup does not download the helper.

## First transfer

1. Put both devices on the same local network and open the receiving app on the
   other device.
2. Open Nearby from the bar and turn its receiver on if it is off.
3. Select the device, then choose **Send files** or **Send clipboard**.
4. Enter the destination's PIN if requested, and approve the transfer there.

For incoming transfers, leave Nearby on and use **Accept** or **Decline** when a
request arrives. Files go to your configured Downloads directory. Received text
can be copied from the panel.

Closing the popup keeps receiving and passive discovery available. Turning
Nearby off stops the helper, receiver and discovery. Active discovery runs while
a Nearby popup is open.

See the [user guide](docs/USAGE.md) for PIN settings, storage locations and shell
commands, or [troubleshooting](docs/TROUBLESHOOTING.md) if a device does not appear.

## Update or remove

```sh
omarchy plugin update oma.nearby
```

Add `--yes` for unattended updates. The next receiver start resolves the helper
selected by the updated checkout. Deliberate local builds take priority until
their [developer override is disabled](CONTRIBUTING.md#return-to-the-published-helper).

To remove the plugin:

```sh
omarchy plugin remove oma.nearby
```

## Documentation

| Guide | Contents |
| --- | --- |
| [Using Nearby](docs/USAGE.md) | Sending, receiving, PINs, storage and shell commands |
| [Troubleshooting](docs/TROUBLESHOOTING.md) | Discovery, startup, helper repair and reporting problems |
| [Contributing](CONTRIBUTING.md) | Local builds, repository map and validation |
| [Architecture](docs/ARCHITECTURE.md) | Service ownership, discovery and helper distribution |
| [Releasing](docs/RELEASING.md) | Plugin/helper versions, publication and verification |
| [Robustness validation](ROBUSTNESS.md) | Manual regression scenarios and result records |
| [Security](docs/SECURITY.md) | Trust model, PIN limits and persistent identity |
| [Changelog](CHANGELOG.md) | Release history |
| [Vendored dependency](VENDORED_LOCALSEND_RS.md) | Provenance, local patches and maintenance policy |

## Security and license

Use Nearby on networks you trust. Discovery is not authenticated pairing, and
incoming PIN protection complements local approval rather than replacing it.
Read the [security notes](docs/SECURITY.md) for the scope of those protections.

Nearby is [MIT licensed](LICENSE). The modified `localsend-rs` dependency is
separately MIT licensed; see [third-party notices](THIRD_PARTY_NOTICES.md).
