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
at rest. Do not share the settings file.

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
