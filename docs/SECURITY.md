# Security model

[Documentation index](../README.md#documentation)

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

## Persistent security state

The incoming PIN is stored in a private settings file; it is not shown again in
the UI. This is local credential storage, not a claim that the PIN is encrypted
at rest. Do not share the settings file. The helper walks the absolute state
path from the filesystem root using directory descriptors and refuses symlinks
at every component. Root-owned system ancestors and ancestors owned by the
effective user are accepted; the final `omarchy-nearby` directory must belong
to the effective user and is set to mode `0700`. A relative `XDG_STATE_HOME`
therefore fails closed.

Settings and TLS identity files are opened relative to that trusted directory
without following symlinks. The opened file must be regular and owned by the
effective user. Nonblocking opens reject FIFOs without waiting for a writer.
Reads are capped at 16 KiB for settings and 128 KiB for identity. Updates use
random, exclusive mode-`0600` temporary files, sync their contents, rename
relative to the same directory descriptor, then sync the directory. Invalid or
unsafe settings stop startup without silently disabling the PIN or deleting the
file. A missing final settings file uses defaults only after the directory has
been validated.

These controls do not protect against a malicious process already running as
the same user. A storage failure during the final directory sync can also be
reported after an atomic rename has occurred; in that case durability is not
guaranteed.

The descriptor-based guarantees above apply to Nearby's private persistent
security state. Incoming downloads follow the configured Downloads location
and the vendored LocalSend receive path.

Nearby validates the saved TLS certificate and private key and preserves a valid
identity across restarts. The helper also reuses that identity when an HTTPS peer
requires client authentication. Unsafe or invalid security state can prevent
startup; do not delete it as a generic troubleshooting step, because removing
settings or identity can change the receiver's security behavior.

See [storage locations](USAGE.md#storage).

## Published helper verification

The launcher checks the exact size and SHA256 committed in the plugin checkout.
Downloads and redirects are restricted to HTTPS and artifacts are capped at
32 MiB. Published helpers are installed atomically outside the plugin directory.
The cache path must be absolute, owned by the user or root as appropriate,
and free of symlinked or other-user-writable ancestors. Nearby's own cache
directories are restricted to the current user.

Build-provenance attestation verification happens in the release workflows, not
on every user startup. The independent verification workflow additionally checks
the executable's reported version. These checks depend on trusting the source
checkout and its pinned metadata.

An explicitly marked local developer build bypasses published-asset selection
and hash verification. See [Contributing](../CONTRIBUTING.md).

## Reporting a concern

Include the affected version, a minimal reproduction and the observed impact.
Do not attach PIN settings, private keys, clipboard contents or personal files.
For a suspected vulnerability, arrange a private channel with the maintainer
before disclosing sensitive reproduction details publicly. This repository does
not currently document a dedicated private security contact.
