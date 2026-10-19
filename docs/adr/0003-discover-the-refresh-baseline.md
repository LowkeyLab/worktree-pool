---
status: accepted
---

# Discover the refresh baseline from origin

Acquisition currently assumes `origin/main`. This prevents default acquisition when the remote uses another default branch. Issue #15 requires a fresh fetch and separates the refresh baseline from the task reference.

Each refresh discovers the default branch from the remote `origin`. This change adds no configuration override. Discovery must report one valid default branch. Discovery failure stops acquisition without a fallback to `main`, `master`, or cached `origin/HEAD`.

The operation records the discovered branch before fetching. That operation continues with its recorded branch if the remote changes its default. The next refresh discovers the default again.

Every acquisition requires a successful fresh fetch of the refresh baseline, including acquisition with an explicit task reference. Acquisition resolves the task reference after the baseline fetch succeeds. Without a task reference, acquisition uses the commit from the successful baseline fetch. A failed fetch blocks acquisition even when the task reference exists locally.

New versioned events record discovery, the selected baseline, and diagnostics. Existing refresh events retain their original `origin/main` meaning. Immutable historical events remain unchanged. Recovery uses the recorded baseline and does not fetch. A local reference observation does not prove a successful fresh fetch.

## Failure and recovery

Discovery and fetch failures retain a recorded operation that requires explicit recovery before another acquisition attempt. This rule also applies when discovery fails before a fetch starts. Interrupted operations retain the existing requirement for explicit recovery.

Failed acquisition responses include the repository and refresh operation identities. They identify the failed stage and include available source and destination references. Discovery can fail before it identifies a branch. Such a failure must not invent a failed branch reference.

Durable diagnostics contain structured failure categories, the failure stage, available branch references, Git exit status when available, and a corrective instruction. They exclude credentials, remote URLs, and raw Git stderr. Inspection retains these diagnostics after the command exits. Controlled diagnostic text limits detail but avoids storing arbitrary subprocess output.

Recovery does not repeat discovery or fetch. Historical refresh operations retain their original `origin/main` baseline. A new operation uses its recorded baseline. If discovery never selected a baseline, recovery reports that absence. Reconciliation cannot establish a successful fresh fetch from local state. A later acquisition requires a new successful refresh.

## Acceptance criteria

Use private catalogs and real local bare Git remotes for behavior checks.

- Check default acquisition from a remote whose only branch and remote HEAD are `master`.
- Check explicit task references in that repository, including `origin/master` and a locally known task branch or commit.
- Check that explicit task references resolve after the mandatory baseline fetch.
- Check a default branch with a nested name, such as `release/stable`.
- Change the remote default between operations. Check that the next operation discovers the new default.
- Change the remote default after discovery. Check that the current operation retains its recorded baseline.
- Check rejection of missing, ambiguous, or invalid default discovery without a cached-reference fallback.
- Check that transport and reference-update failures block acquisition despite usable local tracking references or task references.
- Check that failed acquisition responses identify the refresh operation and retain structured diagnostics through later inspection.
- Check that diagnostics exclude remote URLs and raw stderr, including credential-bearing fixture output.
- Check explicit recovery after discovery failures, fetch failures, and interrupted refreshes. Recovery must not perform discovery or fetch.
- Replay historical refresh events without rewriting them. Check their original `origin/main` meaning and supported new event versions.
- Retain assignment safety, exact resolved commits, and ordinary detached checkout behavior.

Update README.md, CONTRIBUTING.md, and WIRE_SCHEMA.md with the implemented behavior. Explain the mandatory fetch for explicit task references and the new diagnostic fields. Agent guidance that requires `origin/main` also needs an update when implementation is available.

## Trade-offs

Live discovery follows remote default changes without stored configuration. Discovery also requires remote access before each fetch. Repositories with no valid remote default require correction at the remote before acquisition can proceed.

Explicit task references currently use local Git state without a fetch. The accepted policy changes that behavior and requires updated command documentation. ADR 0002 requires a minor release for breaking CLI changes during the 0.x series. Event and catalog formats retain their independent version contracts.

## Reference

[Issue #15](https://github.com/LowkeyLab/worktree-pool/issues/15)
