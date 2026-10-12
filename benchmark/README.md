# Manual Cargo benchmark

Compare a retained worktree and its build outputs with a fresh Git worktree for
unchanged source and a small edit. The default workload builds this repository
with `cargo build --locked` at its recorded HEAD. Commit source changes first.

```sh
cargo build --locked
python3 benchmark/run.py \
  --pool-binary "$PWD/target/debug/worktree-pool" \
  --output /absolute/new/private-benchmark-directory \
  --pairs 10
```

Use a quiet Linux x86-64 machine. One recorded warm-up prepares the retained
cache. Both arms share the Cargo dependency cache; target directories remain
separate. Pair order alternates. Each pair starts at the same commit and checks
matching edited source bytes. Private edits are committed before release.

`report.json` retains raw acquisition, preparation, build, total, and separate
release/cleanup timings, failures, tool versions, executable digest, and logical
retained target bytes. Summaries report medians and paired savings, including
negative savings. There is no adoption threshold or CI performance gate.

The runner owns an isolated catalog and local origin. It preserves incomplete
runs and assignments on failure; inspect the recorded handles before recovery.
It refuses an existing output directory and leaves the caller repository alone.
Successful disposable fixtures lose their build output; retained fixtures keep it.

CI exercises runner correctness against a tiny real Cargo/Git fixture. Those
fixture tests establish execution and comparison behavior, not a performance
claim about the full application. Historical Bazel/Nicknamer measurements in
CONTRIBUTING.md use a different workload and protocol.
