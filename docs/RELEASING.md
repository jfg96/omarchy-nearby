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

## Branches

Omarchy's plugin manager installs and updates from the repository's default
branch. `omarchy plugin update` fetches that branch tip and offers any new commit
as an update; it does not read versions or tags. The default branch is therefore
`stable`, which only ever points at a published `vX.Y.Z` tag, and development
happens on `main`.

| Branch | Contents | Moves |
| --- | --- | --- |
| `main` | Development: merged pull requests and release preparation | On every merge |
| `stable` | What users install and update to | Fast-forward to each new `vX.Y.Z` tag, nothing else |

Pull requests target `main`. Never commit to `stable`, open pull requests against
it, or force-push it. [Stable guard](../.github/workflows/stable-guard.yml)
rejects pull requests into `stable` and a push that leaves it on an untagged
commit.

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
   x86_64 and aarch64 executables natively in GitHub Actions, generates
   checksum/license assets, attests each executable, and publishes one GitHub
   prerelease containing both.
5. Download and independently verify the published bytes of each executable
   before selecting them: check byte size, SHA256, provenance and `--version`
   (run the aarch64 executable on an aarch64 machine). Never substitute a
   locally built executable.

Helper releases remain marked prerelease in GitHub so they do not replace the
latest stable plugin release. This is independent of their stable-format tags.

## Select the verified helper

Update all fields of [helper-release.env](../helper-release.env) with the
verified release values. The three `AARCH64` fields are optional as a group;
without them the launcher refuses aarch64 machines, which then need a local
build. Older helper tags have no aarch64 asset.

| Field | Required value |
| --- | --- |
| `NEARBY_HELPER_TAG` | Exact `helper-vX.Y.Z` tag |
| `NEARBY_HELPER_ASSET` | `omarchy-nearby-helper-vX.Y.Z-linux-x86_64` |
| `NEARBY_HELPER_SIZE` | Exact byte size, at most 32 MiB |
| `NEARBY_HELPER_SHA256` | Lowercase SHA256 of the published bytes |
| `NEARBY_HELPER_AARCH64_ASSET` | `omarchy-nearby-helper-vX.Y.Z-linux-aarch64` |
| `NEARBY_HELPER_AARCH64_SIZE` | Exact byte size of the aarch64 helper, at most 32 MiB |
| `NEARBY_HELPER_AARCH64_SHA256` | Lowercase SHA256 of the published aarch64 bytes |
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
   runs checks, downloads each pinned helper, verifies its size/hash/provenance,
   and publishes the plugin release with third-party license information.
   It does not build or republish the helper executable.
6. Run **Verify release** manually in GitHub Actions with that exact plugin tag.
   On an x86_64 and an aarch64 runner, it independently downloads the helper
   selected for that machine and verifies its size, hash, provenance and
   reported version. Record the run and outcome.
7. Only after both workflows succeed, fast-forward `stable` to the tag. This
   is the step that offers the release to installed copies:

   ```sh
   git fetch origin
   git push origin 'vX.Y.Z^{commit}:refs/heads/stable'
   ```

   Without `--force` the push is refused unless it is a fast-forward. A refused
   push means `stable` is not an ancestor of the tag; stop and diagnose rather
   than forcing it.

The independent verifier reads metadata from the requested plugin tag. The
normal launcher performs size/hash verification; it does not run GitHub
attestation verification on each user startup.

## Failed preparation or verification

Resolve failed checks before creating a release tag. Do not move an existing tag,
rewind `stable` or rewrite published `main` to hide a release problem. Diagnose failed workflows
against their exact source and artifact, and prepare a corrective release when
published contents need to change.
