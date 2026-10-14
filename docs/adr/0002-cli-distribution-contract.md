---
status: accepted
---

# Publish worktree pool as a CLI product

The crates.io package distributes the supported `worktree-pool` executable.
Its Rust library remains an implementation detail used by the executable and
test helper, with no supported Rust API compatibility guarantee. Keeping those
exports avoids restructuring production and test wiring solely for distribution;
publishing them does not establish a library product.

During the 0.x series, compatible CLI fixes use patch releases and breaking CLI
changes use minor releases. JSON receipts, immutable events, and catalog formats
retain their independent version contracts. A crate version bump alone never
authorizes rewriting immutable history or bypassing existing unsupported-version
refusals and catalog preservation requirements.
