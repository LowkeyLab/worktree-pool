"""Release decisions against real private Git repositories and HTTP responses."""

import json
import os
import subprocess
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.error import HTTPError

import release


class SourceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.source = Path(self.temp.name)
        self.env = {
            key: value
            for key, value in os.environ.items()
            if not key.startswith("GIT_")
        }
        self.env.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull)
        self.git("init", "-b", "main")
        self.git("config", "user.name", "Release fixture")
        self.git("config", "user.email", "release@example.invalid")
        (self.source / "Cargo.toml").write_text(
            '[package]\nname="worktree-pool"\nversion="0.1.0"\n'
        )
        (self.source / "CHANGELOG.md").write_text(
            "# Changes\n\n## [0.1.0]\n\nInitial CLI.\n"
        )
        self.commit()
        self.git("tag", "v0.1.0")
        self.git("update-ref", "refs/remotes/origin/main", "HEAD")

    def git(self, *args):
        return subprocess.check_output(
            ["git", *args], cwd=self.source, stderr=subprocess.DEVNULL, text=True
        ).strip()

    def commit(self):
        self.git("add", ".")
        self.git("commit", "-m", "fixture")

    def test_merged_release_is_accepted(self):
        result = release.validate_source(self.source, "v0.1.0")
        self.assertEqual(
            result,
            {
                "tag": "v0.1.0",
                "version": "0.1.0",
                "commit": self.git("rev-parse", "HEAD"),
            },
        )

    def test_prerelease_and_invalid_tags_are_rejected(self):
        for tag in ("v0.1.0-alpha", "v01.1.0", "0.1.0", "v0.1.0/evil"):
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                release.validate_source(self.source, tag)

    def test_version_mismatch_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "package version"):
            release.validate_source(self.source, "v0.1.1")

    def test_dirty_and_untracked_sources_are_rejected(self):
        for name in ("Cargo.toml", "untracked"):
            with self.subTest(name=name):
                path = self.source / name
                previous = path.read_text() if path.exists() else None
                path.write_text((previous or "") + "\n")
                with self.assertRaisesRegex(ValueError, "clean"):
                    release.validate_source(self.source, "v0.1.0")
                if previous is None:
                    path.unlink()
                else:
                    path.write_text(previous)

    def test_tag_at_another_commit_is_rejected(self):
        (self.source / "new").write_text("new")
        self.commit()
        with self.assertRaisesRegex(ValueError, "checked-out commit"):
            release.validate_source(self.source, "v0.1.0")

    def test_unmerged_release_is_rejected(self):
        self.git("switch", "-c", "feature")
        (self.source / "new").write_text("new")
        self.commit()
        self.git("tag", "-f", "v0.1.0")
        with self.assertRaises(subprocess.CalledProcessError):
            release.validate_source(self.source, "v0.1.0")

    def test_missing_changelog_entry_is_rejected(self):
        (self.source / "CHANGELOG.md").write_text("# Changes\n")
        self.commit()
        self.git("tag", "-f", "v0.1.0")
        self.git("update-ref", "refs/remotes/origin/main", "HEAD")
        with self.assertRaisesRegex(ValueError, "changelog"):
            release.validate_source(self.source, "v0.1.0")


class RegistryTests(unittest.TestCase):
    def setUp(self):
        self.status = 200
        self.body = {"version": {"num": "0.1.0", "checksum": "a" * 64}}
        test = self

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):
                test.assertEqual(self.path, "/api/v1/crates/worktree-pool/0.1.0")
                self.send_response(test.status)
                self.end_headers()
                self.wfile.write(json.dumps(test.body).encode())

            def log_message(self, *args):
                pass

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        thread.start()
        self.addCleanup(self.server.server_close)
        self.addCleanup(thread.join)
        self.addCleanup(self.server.shutdown)
        self.registry = f"http://127.0.0.1:{self.server.server_port}"

    def test_existing_matching_version_can_resume_acceptance(self):
        checksum = release.registry_checksum("0.1.0", registry=self.registry)
        self.assertEqual(
            release.registry_decision("a" * 64, checksum), "verify-existing"
        )

    def test_absent_version_can_publish(self):
        self.status = 404
        self.assertEqual(
            release.registry_decision(
                "a" * 64, release.registry_checksum("0.1.0", registry=self.registry)
            ),
            "publish",
        )

    def test_http_failures_are_not_absence(self):
        for status in (401, 403, 429, 500, 503):
            with self.subTest(status=status):
                self.status = status
                with self.assertRaises(HTTPError):
                    release.registry_checksum("0.1.0", registry=self.registry)

    def test_archive_mismatch_fails_without_replacing_release(self):
        with self.assertRaisesRegex(ValueError, "preserve version and tag"):
            release.registry_decision(
                "b" * 64, release.registry_checksum("0.1.0", registry=self.registry)
            )

    def test_malformed_registry_identity_is_rejected(self):
        for body in (
            {"version": {"num": "0.2.0", "checksum": "a" * 64}},
            {"version": {"num": "0.1.0", "checksum": "invalid"}},
        ):
            with self.subTest(body=body):
                self.body = body
                with self.assertRaises(ValueError):
                    release.registry_checksum("0.1.0", registry=self.registry)


class EvidenceTests(unittest.TestCase):
    def test_changed_archive_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)
            archive = path / "worktree-pool-0.1.0.crate"
            archive.write_bytes(b"verified")
            release.write_json(
                path / "release.json",
                {
                    "version": "0.1.0",
                    "archive": archive.name,
                    "archive_sha256": release.sha256(archive),
                },
            )
            release.read_evidence(path)
            archive.write_bytes(b"replaced")
            with self.assertRaisesRegex(ValueError, "checksum"):
                release.read_evidence(path)


if __name__ == "__main__":
    unittest.main()
