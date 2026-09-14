# Releasing Nearby

[Documentation index](../README.md#documentation)

This is the maintainer procedure. The checked-in workflows are the executable
source of truth; do not publish tags until the exact target commit has passed CI.

## Versions and release tracks

| Item | Meaning |
| --- | --- |
| `manifest.json` version | Plugin version, matching the stable changelog heading and `vX.Y.Z` tag |
| `minHelperVersion` | Runtime compatibility floor for the helper |
| Helper Cargo package version | Helper release version, reflected in Cargo metadata and lockfile |
| `helper-vX.Y.Z` | Independently published helper tag |
| `helper-release.env` | Exact asset, size, hash, source commit and workflow selected by a checkout |

Plugin and helper versions can advance independently. Runtime source changes
after a stable plugin release advance the manifest to the next `-dev` version.
Helper behavior changes also require advancing its package version and the
compatibility floor. Documentation-only and repository-only CI changes that
cannot alter the installed plugin do not require a version bump.

At runtime, a helper at or above the floor is accepted. The current
[stable release workflow](../.github/workflows/release.yml) imposes stricter
packaging checks: the selected helper version, Cargo package version and
`minHelperVersion` must be equal, and the entire `backend` tree must match
the recorded helper source commit. A plugin-only release may reuse the same
helper when those conditions still hold.

## Prepare and publish a changed helper

1. Finish helper changes, update Cargo version/lockfile and the plugin's required
   helper floor, and record any vendor changes in the
   [maintenance record](../VENDORED_LOCALSEND_RS.md).
2. Run the [supported checks](../CONTRIBUTING.md#validation) and relevant
   [manual scenarios](../ROBUSTNESS.md). Record peer versions and outcomes.
3. Ensure the exact source commit has passed CI before creating its
   `helper-vX.Y.Z` tag. Tags use stable SemVer without a `-dev` suffix.
4. Publish the tag to trigger
   [Helper release](../.github/workflows/helper-release.yml). It builds the
   executable in GitHub Actions, generates checksum/license assets, attests the
   executable, and publishes a GitHub prerelease.
5. Download and independently verify the published bytes before selecting them:
   check byte size, SHA256, provenance and `--version`. Never substitute a
   locally built executable.

Helper releases remain marked prerelease in GitHub so they do not replace the
latest stable plugin release. This is independent of their stable-format tags.

## Select the verified helper

Update all six fields of [helper-release.env](../helper-release.env) with the
verified release values:

| Field | Required value |
| --- | --- |
| `NEARBY_HELPER_TAG` | Exact `helper-vX.Y.Z` tag |
| `NEARBY_HELPER_ASSET` | `omarchy-nearby-helper-vX.Y.Z-linux-x86_64` |
| `NEARBY_HELPER_SIZE` | Exact byte size, at most 32 MiB |
| `NEARBY_HELPER_SHA256` | Lowercase SHA256 of the published bytes |
| `NEARBY_HELPER_SOURCE_SHA` | Full 40-character source commit |
| `NEARBY_HELPER_RELEASE_WORKFLOW` | `jfg96/omarchy-nearby/.github/workflows/helper-release.yml` |

The file accepts no extra fields or shell syntax. Never invent hashes or copy
values from a different build.

For provenance verification, use `gh attestation verify` with the expected
repository, `--signer-workflow`, `--source-digest`,
`--source-ref refs/tags/helper-vX.Y.Z` and `--deny-self-hosted-runners`.
The exact invocation is in the
[verification workflow](../.github/workflows/verify-release.yml).
Check that the verified executable reports
`omarchy-nearby-helper X.Y.Z`.

## Publish the stable plugin

1. Set the stable manifest version and matching `## X.Y.Z` changelog heading.
   Keep release preparation distinct from contributors' functional commits.
2. Check the helper reference and version/tree constraints above. Do not modify
   the backend after selecting its source without preparing a matching helper.
3. Run the supported checks, inspect the final diff and record manual results.
   Wait for CI to pass on the exact target commit.
4. Create and publish the matching `vX.Y.Z` tag.
5. Confirm the **Release** workflow succeeds. It verifies metadata and source,
   runs checks, downloads the pinned helper, verifies its size/hash/provenance,
   and publishes the plugin release with third-party license information.
   It does not build or republish the helper executable.
6. Run **Verify release** manually in GitHub Actions with that exact plugin tag.
   This independently downloads the selected helper and verifies its size,
   hash, provenance and reported version. Record the run and outcome.

The independent verifier reads metadata from the requested plugin tag. The
normal launcher performs size/hash verification; it does not run GitHub
attestation verification on each user startup.

## Failed preparation or verification

Resolve failed checks before creating a release tag. Do not move an existing tag
or rewrite published `main` to hide a release problem. Diagnose failed workflows
against their exact source and artifact, and prepare a corrective release when
published contents need to change.

