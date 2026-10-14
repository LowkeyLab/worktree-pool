# Crates.io publishing plan

This document records the publishing interview on 2026-10-14. All interview
choices are accepted. The complete design was confirmed and implementation
authorized on 2026-10-14. The first registry upload, tag push, and source
publication still require separate approval.

## Accepted decisions

- Prepare and validate the publishing setup. The first crates.io upload requires
  separate approval after reviewing the release.
- Distribute the supported CLI through `cargo install worktree-pool --locked`.
  The Rust library is an unsupported implementation detail; see
  [the CLI distribution decision](adr/0002-cli-distribution-contract.md).
- Keep package and executable name `worktree-pool`, version `0.1.0`, and license
  `AGPL-3.0-only`. Runtime support remains Linux x86-64 with Git 2.36+; Rust 1.96+
  is required to compile. Stop for a naming decision if the crate name is taken.
- Document a manual first publication with a temporary token, followed by
  GitHub Actions Trusted Publishing for subsequent releases.
- Future publication is triggered by deliberately pushed version tags. The tag
  must match `Cargo.toml` and point to a commit merged into `main`.
- Prepare version bumps and reviewed changelog entries through ordinary PRs.
  Publishing consumes the approved tag rather than deciding the version.
- Require all repository checks and installed-package/minimum-Git acceptance
  for the exact tagged commit before publication. Verify installation of the
  exact published version from crates.io afterward.
- If upload succeeds but installation verification fails, report a failed
  release. Do not automatically yank it.
- During 0.x, use patch versions for compatible CLI fixes and minor versions
  for breaking CLI changes. Preserve independent JSON and catalog contracts.

- The initial owner is `tacascer`; adding organization-team ownership is deferred.
- Bind Trusted Publishing to `LowkeyLab/worktree-pool`, the publishing workflow,
  and the `crates-io` GitHub environment. Deliberate tag push is release approval;
  no second manual approval is required after tag push. Validation remains mandatory.
- On a rerun with an already-published version, verify the registry archive
  matches the intended package, skip uploading, and repeat installation acceptance.
  An archive mismatch fails the release; preserve the existing package and tag.
- Initially accept only `vMAJOR.MINOR.PATCH` tags. Prerelease support is deferred.

## Verified existing behavior

The repository already has Cargo-native minimum/stable CI and package acceptance.
`install/verify.py` checks archive contents, metadata, dependency resolution,
locked installation, native executable properties, and the installed public CLI
against Git 2.36. Its copied-source package check does not establish release-tag
provenance. No publishing workflow or changelog currently exists.

The crates.io API returned 404 for `worktree-pool` on 2026-10-14 at 21:55:23 UTC.
This is an observation, not a reservation; recheck immediately before upload.
GitHub login is `tacascer`. The public repository is `LowkeyLab/worktree-pool`,
with protected `main`, three required CI checks, and no rulesets. The `crates-io`
environment was created and verified during implementation: no reviewer gate,
only tags matching `v*` allowed.
Crates.io account state and trusted-publisher configuration are not verified.

## Proposed implementation

These additions and extensions implement the approved design.

1. Add `.github/workflows/publish.yml` for deliberate stable version tags. Validate
   strict tag syntax, package version, a clean checkout, membership in `origin/main`,
   and a versioned changelog entry. Run the existing minimum/stable repository
   checks and installed-package/Git 2.36 acceptance for that exact commit.
2. Add a small release-control helper under `install/` to keep tag/version checks,
   archive identity, registry outcomes, and rerun handling independently testable.
   Extend existing `install/verify.py` to accept the release archive and to verify
   an exact registry installation; reuse the native/version/minimum-Git/public CLI
   harness rather than create a second acceptance suite.
3. Produce a clean tagged-checkout archive with locked Cargo verification and
   preserve its SHA-256, tag, commit, archive, and acceptance reports as release
   evidence. The copied-source archive currently produced by the verifier cannot
   substitute for the tagged archive because Git provenance differs.
4. Publish from the same clean tagged checkout after validation, using the
   `crates-io` environment and workflow-bound OIDC credentials for later releases.
   Pin Actions to reviewed commits and avoid storing a long-lived publishing token.
   Serialize publication; do not cancel a running upload.
5. Cargo publishes from a manifest and rebuilds the archive; it does not accept a
   supplied `.crate` path. Verify reproducible packaging before upload from the
   same source, lockfile, toolchain, and options. Compare the published checksum
   against the verified archive afterward. This final check detects divergence
   after upload rather than guaranteeing supplied-archive upload.
6. On an existing version, require an exact registry checksum match before
   skipping upload and repeating registry installation acceptance. Network or
   authentication failures must not be interpreted as version absence. Uncertain
   upload results require registry inspection before another upload attempt.
7. Add `CHANGELOG.md` and a release guide covering the temporary-token first
   upload, account readiness, owner, environment/trusted-publisher configuration,
   validation evidence, tag preparation, and failed-release investigation. Update
   README/CONTRIBUTING with installation and CLI/library compatibility policy.

## Validation proposal

Run all checks required by `AGENTS.md`, including the existing packaging and
minimum-Git verifier with a new private output directory. Add meaningful release
control tests for invalid/prerelease tags, version mismatch, off-main/dirty source,
registry absence versus service failure, existing-version archive mismatch,
reruns, and post-upload acceptance failure. Verify supplied-archive and
registry-install modes with the real installed CLI harness and private fixtures.
Exercise the workflow without upload; actual OIDC exchange and registry-installed
acceptance of `0.1.0` remain live checks after separately approved publication.

## Approval boundary

All interview decisions are settled and the complete implementation proposal
was approved. Documentation in this
planning checkout records accepted decisions; it does not perform publication.
The first registry upload remains a separate approval even after implementation.

## Sources

- [Cargo publishing](https://doc.rust-lang.org/cargo/reference/publishing.html)
- [Trusted Publishing bootstrap](https://blog.rust-lang.org/2025/07/11/crates-io-development-update-2025-07/)
- [Trusted Publishing enhancements](https://blog.rust-lang.org/2026/01/21/crates-io-development-update/)
