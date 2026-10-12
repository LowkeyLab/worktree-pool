# Standalone dependency contract

Cargo.toml pins every direct version. Cargo.lock pins the complete standalone
resolution. The manifest and lockfile are the dependency authority for development and distribution.
Cargo installation must use `--locked`. Cargo enables default features unless
this table says otherwise. Listed features are explicitly requested additions.

| Scope   | Crate                | Exact version | Default features | Requested features |
| ------- | -------------------- | ------------- | ---------------- | ------------------ |
| Runtime | `anyhow`             | `1.0.104`     | Enabled          | None               |
| Runtime | `chrono`             | `0.4.45`      | Enabled          | `serde`            |
| Runtime | `clap`               | `4.6.7`       | Enabled          | `derive`           |
| Runtime | `cloudevents-sdk`    | `0.9.0`       | Disabled         | None               |
| Runtime | `config`             | `0.15.27`     | Disabled         | `toml`             |
| Runtime | `redb`               | `4.3.0`       | Enabled          | None               |
| Runtime | `serde`              | `1.0.229`     | Enabled          | `derive`           |
| Runtime | `sha2`               | `0.11.0`      | Disabled         | None               |
| Runtime | `serde_json`         | `1.0.154`     | Enabled          | None               |
| Runtime | `thiserror`          | `2.0.21`      | Enabled          | None               |
| Runtime | `tracing`            | `0.1.44`      | Enabled          | None               |
| Runtime | `tracing-subscriber` | `0.3.23`      | Enabled          | `json`             |
| Runtime | `uuid`               | `1.28.0`      | Enabled          | `v4`, `serde`      |
| Test    | `googletest`         | `0.14.3`      | Enabled          | None               |
| Test    | `insta`              | `1.48.0`      | Enabled          | `yaml`             |
| Test    | `tempfile`           | `3.27.0`      | Enabled          | None               |
| Test    | `proptest`           | `1.12.0`      | Disabled         | `std`              |

The Cargo installation acceptance retains pure `metadata.json`,
`resolved-dependencies.json` and `production-features.log` from the extracted
archive. Metadata includes the test graph. The production feature tree excludes
development edges and fixes the target to `x86_64-unknown-linux-gnu`. These
outputs record resolved transitive versions and feature activation rather than
assuming that requested features describe the complete transitive graph.

App execution needs Git and ordinary native C runtime libraries. Rust,
Cargo, Bazel, Python and Nix are build/verification tools, not app
runtime dependencies. The minimum-Git acceptance build disables optional
HTTP, transport layer security, Perl, Python, Tcl/Tk, `gettext` and `iconv` support, and links pinned `zlib`
1.3.1 statically. It proves ordinary local Git transport. It doesn't certify
HTTPS/SSH or that custom Git build as a general replacement for system Git.
