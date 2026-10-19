# Changelog

## [0.2.0]

- Discover the refresh baseline from the default branch on `origin` for every acquisition.
- Require a successful baseline fetch before resolving an explicit task reference.
- Retain selected baselines and controlled discovery diagnostics in versioned events.
- Reconcile interrupted refreshes without remote discovery or fetch.

Explicit-reference acquisition now requires access to `origin`.
Historical events retain their original meaning. Older binaries reject the new event types.

## [0.1.0]

First release of the standalone `worktree-pool` CLI:

- Explicit assignments for reusable local Git worktrees.
- Durable catalog history, inspection, recovery, relocation, and rebuild.
- JSON receipts and preserved ignored files/build artifacts.
- Linux x86-64 support, Git 2.36+ runtime, and Rust 1.96+ source builds.

The Rust library is an implementation detail without a supported API contract.
