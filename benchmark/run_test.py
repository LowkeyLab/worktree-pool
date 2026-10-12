import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

from run import Runner, summarize


class BenchmarkTests(unittest.TestCase):
    def test_summary_uses_complete_pairs_and_preserves_negative_savings(self):
        samples = [
            {
                "workload": "unchanged",
                "pair": 0,
                "arm": "pooled",
                "status": "completed",
                "total_seconds": 4,
            },
            {
                "workload": "unchanged",
                "pair": 0,
                "arm": "fresh",
                "status": "completed",
                "total_seconds": 2,
            },
            {
                "workload": "unchanged",
                "pair": 1,
                "arm": "fresh",
                "status": "incomplete",
                "total_seconds": 99,
            },
        ]
        result = summarize(samples)["unchanged"]
        self.assertEqual(result["complete_pairs"], 1)
        self.assertEqual(result["median_paired_savings_seconds"], -2)

    def fixture(self, root):
        source = root / "source"
        source.mkdir()
        (source / "Cargo.toml").write_text(
            '[package]\nname="benchmark-fixture"\nversion="0.1.0"\nedition="2024"\n[[bin]]\nname="fixture"\npath="main.rs"\n'
        )
        (source / "Cargo.lock").write_text(
            'version = 4\n[[package]]\nname = "benchmark-fixture"\nversion = "0.1.0"\n'
        )
        (source / "main.rs").write_text("fn main() {}\n")
        (source / ".gitignore").write_text("/target/\n/alternate-target/\n")
        (source / ".cargo").mkdir()
        (source / ".cargo/config.toml").write_text(
            '[build]\ntarget-dir = "alternate-target"\n'
        )
        subprocess.run(
            ["git", "init", "-b", "main", source], check=True, capture_output=True
        )
        subprocess.run(["git", "add", "."], cwd=source, check=True)
        subprocess.run(
            [
                "git",
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@localhost",
                "commit",
                "-m",
                "Fixture",
            ],
            cwd=source,
            check=True,
            capture_output=True,
        )
        return source

    def binary(self):
        return Path(
            os.environ.get(
                "POOL_BENCHMARK_BINARY",
                str(Path(__file__).resolve().parents[1] / "target/debug/worktree-pool"),
            )
        )

    def test_real_paired_runner_uses_private_catalog_and_equal_source(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = self.fixture(root)
            before = (source / "main.rs").read_bytes()
            result = Runner(
                source, root / "results", self.binary(), pairs=2, edit_file="main.rs"
            ).run()
            self.assertEqual(result["status"], "completed")
            self.assertTrue(
                all(
                    sample["retained_target_logical_bytes"] > 0
                    for sample in result["samples"]
                    if sample["arm"] == "pooled"
                )
            )
            self.assertEqual((source / "main.rs").read_bytes(), before)
            for workload in ("unchanged", "small_edit"):
                self.assertEqual(result["summary"][workload]["complete_pairs"], 2)
                samples = [s for s in result["samples"] if s["workload"] == workload]
                self.assertEqual(
                    [s["arm"] for s in samples], ["pooled", "fresh", "fresh", "pooled"]
                )
                for index in (0, 2):
                    self.assertEqual(
                        samples[index]["source_sha256"],
                        samples[index + 1]["source_sha256"],
                    )
            self.assertTrue((root / "results/data/worktree-pool/catalog.redb").exists())

    def test_failed_build_keeps_assignment_and_incomplete_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = self.fixture(root)
            failing_cargo = root / "failing-cargo"
            failing_cargo.write_text(
                '#!/bin/sh\nif [ "$1" = --version ]; then echo "failing Cargo fixture"; else exit 1; fi\n'
            )
            failing_cargo.chmod(0o700)
            runner = Runner(
                source,
                root / "results",
                self.binary(),
                pairs=1,
                edit_file="main.rs",
                cargo=str(failing_cargo),
            )
            with self.assertRaises(RuntimeError):
                runner.run()
            report = json.loads((root / "results/report.json").read_text())
            self.assertEqual(report["status"], "incomplete")
            handle = report["samples"][0]["assignment_handle"]
            self.assertEqual(
                runner.pool("assignment", "inspect", handle)["data"]["assignment"][
                    "state"
                ],
                "active",
            )
            self.assertTrue(report["samples"][0]["error"])


if __name__ == "__main__":
    unittest.main()
