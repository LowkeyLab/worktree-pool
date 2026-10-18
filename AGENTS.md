# Worktree pool

Use worktree-pool assignments for edits. Preserve exact handles and receipts.
Runtime support is Linux x86-64 with Git 2.36+; Rust 1.96 is the minimum compiler.

Use Cargo for development. Before finishing a change, run:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked --release
python3 -m unittest discover -s benchmark -p '*_test.py'
```

Run `python3 install/verify.py --output /absolute/new/private-directory` for
packaging, installed CLI, and minimum-Git acceptance. The output must not exist.
The crash helper is an example target, not an installed production command.

Use googletest for Rust assertions and retain real Git/filesystem integration.
Never mutate user catalogs in tests or benchmarks. Benchmarks are manual; CI
checks runner correctness without performance thresholds.

Read CONTRIBUTING.md and WIRE_SCHEMA.md before changing public contracts.

## Agent skills

### Issue tracker

Use GitHub Issues in `LowkeyLab/worktree-pool`. Before tracker operations,
read `docs/agents/issue-tracker.md`.

### Triage labels

Use the five default triage labels. Before triaging,
read `docs/agents/triage-labels.md`.

### Domain docs

Use a single-context layout. Before exploring domain concepts or decisions,
read `docs/agents/domain.md`.
