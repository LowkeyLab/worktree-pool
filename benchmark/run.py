#!/usr/bin/env python3
"""Manually compare retained and fresh worktrees using the same Cargo source."""

import argparse
import hashlib
import json
import os
import shutil
import statistics
import subprocess
import time
from pathlib import Path


def path_from_receipt(value):
    return Path(os.fsdecode(bytes(value["bytes"])))


def summarize(samples):
    result = {}
    for workload in ("unchanged", "small_edit"):
        pairs = {}
        for sample in samples:
            if sample["workload"] == workload and sample["status"] == "completed":
                pairs.setdefault(sample["pair"], {})[sample["arm"]] = sample[
                    "total_seconds"
                ]
        complete = [pair for pair in pairs.values() if len(pair) == 2]
        result[workload] = {
            "complete_pairs": len(complete),
            "pooled_median_seconds": statistics.median(p["pooled"] for p in complete)
            if complete
            else None,
            "fresh_median_seconds": statistics.median(p["fresh"] for p in complete)
            if complete
            else None,
            "median_paired_savings_seconds": statistics.median(
                p["fresh"] - p["pooled"] for p in complete
            )
            if complete
            else None,
        }
    return result


class Runner:
    def __init__(
        self, source, output, binary, pairs=10, edit_file="lib.rs", cargo="cargo"
    ):
        if pairs < 1:
            raise ValueError("pairs must be positive")
        self.source = Path(source).resolve()
        self.output = Path(output).resolve()
        self.output.mkdir(mode=0o700)  # Refuse reuse of caller data.
        self.binary = Path(binary).resolve()
        self.pairs = pairs
        self.edit_file = Path(edit_file)
        if self.edit_file.is_absolute() or ".." in self.edit_file.parts:
            raise ValueError("edit file must stay inside the checkout")
        self.cargo = cargo
        self.env = {
            k: v
            for k, v in os.environ.items()
            if not k.startswith(("WORKTREE_POOL_", "GIT_")) and k != "CARGO_TARGET_DIR"
        }
        for key, suffix in (
            ("HOME", "home"),
            ("XDG_DATA_HOME", "data"),
            ("XDG_STATE_HOME", "state"),
            ("XDG_CONFIG_HOME", "config"),
        ):
            self.env[key] = str(self.output / suffix)
        # Both arms share the same dependency cache, but have separate target dirs.
        self.env["CARGO_HOME"] = os.environ.get(
            "CARGO_HOME", str(Path.home() / ".cargo")
        )
        self.env["RUSTUP_HOME"] = os.environ.get(
            "RUSTUP_HOME", str(Path.home() / ".rustup")
        )
        self.env["GIT_CONFIG_NOSYSTEM"] = "1"
        self.env["GIT_CONFIG_GLOBAL"] = "/dev/null"
        self.repo = self.output / "repository"
        self.samples = []
        self.report = {
            "schema_version": 1,
            "status": "incomplete",
            "samples": self.samples,
        }
        self.command_index = 0

    def command(self, argv, cwd=None):
        self.command_index += 1
        receipt = self.output / "commands" / str(self.command_index)
        receipt.mkdir(parents=True, mode=0o700)
        (receipt / "argv.json").write_text(json.dumps([str(a) for a in argv]) + "\n")
        with (
            (receipt / "stdout").open("wb") as out,
            (receipt / "stderr").open("wb") as err,
        ):
            result = subprocess.run(argv, cwd=cwd, env=self.env, stdout=out, stderr=err)
        (receipt / "exit-status").write_text(str(result.returncode) + "\n")
        if result.returncode:
            raise RuntimeError(
                f"command failed ({result.returncode}); retained receipt: {receipt}"
            )
        return (receipt / "stdout").read_bytes()

    def pool(self, *args):
        return json.loads(self.command([self.binary, "--json", *args]))

    def save(self):
        self.report["summary"] = summarize(self.samples)
        (self.output / "report.json").write_text(
            json.dumps(self.report, indent=2) + "\n"
        )

    def arm(self, workload, pair, arm):
        sample = {
            "workload": workload,
            "pair": pair,
            "arm": arm,
            "status": "incomplete",
        }
        self.samples.append(sample)
        start = time.perf_counter()
        try:
            if arm == "pooled":
                assignment = self.pool("acquire", "--repo", self.repository_id)["data"][
                    "assignment"
                ]
                checkout = path_from_receipt(assignment["path"])
                sample["assignment_handle"] = assignment["assignment_handle"]
            else:
                self.command(
                    [
                        "git",
                        "-C",
                        self.repo,
                        "-c",
                        "core.hooksPath=/dev/null",
                        "-c",
                        "core.fsync=objects,reference",
                        "-c",
                        "core.fsyncMethod=fsync",
                        "fetch",
                        "--no-prune",
                        "--no-prune-tags",
                        "--no-tags",
                        "--no-recurse-submodules",
                        "--no-auto-maintenance",
                        "--no-write-fetch-head",
                        "origin",
                        "+refs/heads/main:refs/remotes/origin/main",
                    ]
                )
                checkout = self.output / f"fresh-{workload}-{pair}"
                self.command(
                    [
                        "git",
                        "-C",
                        self.repo,
                        "worktree",
                        "add",
                        "--detach",
                        checkout,
                        self.commit,
                    ]
                )
            sample["acquire_seconds"] = time.perf_counter() - start
            head = self.command(["git", "rev-parse", "HEAD"], checkout).decode().strip()
            if head != self.commit:
                raise ValueError("arms must start from the same commit")
            edit = checkout / self.edit_file
            if not edit.is_file() or edit.is_symlink():
                raise ValueError("edit file must be a regular source file")
            if workload == "small_edit":
                with edit.open("a") as stream:
                    stream.write(f"\n// worktree-pool benchmark pair {pair}\n")
            source_hash = hashlib.sha256(edit.read_bytes()).hexdigest()
            other = next(
                (
                    s
                    for s in self.samples[:-1]
                    if s["workload"] == workload
                    and s["pair"] == pair
                    and s["status"] == "completed"
                ),
                None,
            )
            if other and other["source_sha256"] != source_hash:
                raise ValueError("paired source bytes differ")
            sample["source_sha256"] = source_hash
            sample["prepare_seconds"] = (
                time.perf_counter() - start - sample["acquire_seconds"]
            )
            build_start = time.perf_counter()
            self.command(
                [self.cargo, "build", "--locked", "--target-dir", checkout / "target"],
                checkout,
            )
            sample["build_seconds"] = time.perf_counter() - build_start
            sample["total_seconds"] = time.perf_counter() - start
            # Commit private edits before release; never discard protected work.
            cleanup = time.perf_counter()
            if workload == "small_edit":
                self.command(["git", "add", "--", self.edit_file], checkout)
                self.command(
                    [
                        "git",
                        "-c",
                        "user.name=Benchmark",
                        "-c",
                        "user.email=benchmark@localhost",
                        "commit",
                        "-m",
                        f"Benchmark pair {pair}",
                    ],
                    checkout,
                )
            if arm == "pooled":
                target = checkout / "target"
                sample["retained_target_logical_bytes"] = sum(
                    p.stat().st_size
                    for p in target.rglob("*")
                    if p.is_file() and not p.is_symlink()
                )
                self.pool("release", assignment["assignment_handle"])
            else:
                # Remove only this successful, private fixture and its build output.
                shutil.rmtree(checkout / "target")
                self.command(["git", "-C", self.repo, "worktree", "remove", checkout])
            sample["release_cleanup_seconds"] = time.perf_counter() - cleanup
            sample["status"] = "completed"
        except Exception as error:
            sample["error"] = str(error)
            raise
        finally:
            self.save()

    def run(self):
        try:
            self.commit = (
                self.command(["git", "-C", self.source, "rev-parse", "HEAD"])
                .decode()
                .strip()
            )
            self.command(
                [
                    "git",
                    "clone",
                    "--no-hardlinks",
                    "--no-checkout",
                    self.source,
                    self.repo,
                ]
            )
            self.command(
                ["git", "-C", self.repo, "remote", "set-url", "origin", self.source]
            )
            # Use a private main baseline for each source checkout.
            origin = self.output / "origin.git"
            self.command(["git", "init", "--bare", "--initial-branch=main", origin])
            self.command(
                [
                    "git",
                    "-C",
                    self.repo,
                    "push",
                    origin,
                    f"{self.commit}:refs/heads/main",
                ]
            )
            self.command(
                ["git", "-C", self.repo, "remote", "set-url", "origin", origin]
            )
            self.pool("catalog", "init")
            self.repository_id = self.pool("repo", "register", self.repo)["data"][
                "repository"
            ]["repository_id"]
            self.pool(
                "pool",
                "configure",
                "--repo",
                self.repository_id,
                "--max-worktrees",
                "1",
            )
            self.report.update(
                commit=self.commit,
                pool_binary_sha256=hashlib.sha256(self.binary.read_bytes()).hexdigest(),
                dependency_cache="shared",
                git=self.command(["git", "--version"]).decode().strip(),
                cargo=self.command([self.cargo, "--version"]).decode().strip(),
                rustc=self.command(["rustc", "--version"]).decode().strip(),
            )
            # One recorded warm-up establishes the retained build cache.
            self.arm("warmup", -1, "pooled")
            for workload in ("unchanged", "small_edit"):
                for pair in range(self.pairs):
                    arms = ("pooled", "fresh") if pair % 2 == 0 else ("fresh", "pooled")
                    for arm in arms:
                        self.arm(workload, pair, arm)
            self.report["status"] = "completed"
            return self.report
        finally:
            self.save()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--source", type=Path, default=Path(__file__).resolve().parents[1]
    )
    parser.add_argument("--pool-binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--pairs", type=int, default=10)
    args = parser.parse_args()
    Runner(args.source, args.output, args.pool_binary, args.pairs).run()
    print(args.output.resolve() / "report.json")


if __name__ == "__main__":
    main()
