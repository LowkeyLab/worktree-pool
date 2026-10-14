#!/usr/bin/env python3
"""Guard exact-tag crates.io releases; never upload, yank, or change Git refs."""

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import time
import urllib.error
import urllib.request
from pathlib import Path

import tomllib

VERSION = r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
REGISTRY = "https://crates.io"


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def git(source, *args):
    return subprocess.check_output(["git", *args], cwd=source, text=True).strip()


def validate_source(source, tag):
    if re.fullmatch("v" + VERSION, tag) is None:
        raise ValueError("release tag must be vMAJOR.MINOR.PATCH without a prerelease")
    package = tomllib.loads((source / "Cargo.toml").read_text())["package"]
    if package["name"] != "worktree-pool" or tag != "v" + package["version"]:
        raise ValueError("tag does not match worktree-pool package version")
    if git(source, "status", "--porcelain", "--untracked-files=all"):
        raise ValueError("release checkout must be clean, including untracked files")
    commit = git(source, "rev-parse", "HEAD")
    if git(source, "rev-parse", f"refs/tags/{tag}^{{commit}}") != commit:
        raise ValueError("release tag does not point to checked-out commit")
    subprocess.run(
        ["git", "merge-base", "--is-ancestor", commit, "refs/remotes/origin/main"],
        cwd=source,
        check=True,
    )
    changelog = (source / "CHANGELOG.md").read_text()
    if (
        re.search(
            r"^## \[" + re.escape(package["version"]) + r"\](?:\s|$)",
            changelog,
            re.MULTILINE,
        )
        is None
    ):
        raise ValueError("release requires a versioned changelog entry")
    return {"tag": tag, "commit": commit, "version": package["version"]}


def package_archive(source, output):
    env = dict(os.environ, CARGO_TARGET_DIR=str(output / "cargo-target"))
    # Packaging/builds don't need a publishing credential.
    env.pop("CARGO_REGISTRY_TOKEN", None)
    env.pop("CARGO_REGISTRIES_CRATES_IO_TOKEN", None)
    with (output / "package.log").open("wb") as log:
        subprocess.run(
            ["cargo", "package", "--locked", "--registry", "crates-io"],
            cwd=source,
            env=env,
            stdout=log,
            stderr=log,
            check=True,
        )
    version = tomllib.loads((source / "Cargo.toml").read_text())["package"]["version"]
    archive = output / f"worktree-pool-{version}.crate"
    shutil.copyfile(output / "cargo-target/package" / archive.name, archive)
    return archive


def registry_checksum(version, *, registry=REGISTRY):
    if re.fullmatch(VERSION, version) is None:
        raise ValueError("invalid registry version")
    request = urllib.request.Request(
        f"{registry}/api/v1/crates/worktree-pool/{version}",
        headers={
            "User-Agent": "worktree-pool-release (github.com/LowkeyLab/worktree-pool)"
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            data = json.load(response)["version"]
    except urllib.error.HTTPError as error:
        error.close()
        if error.code == 404:
            return None
        raise
    checksum = data["checksum"]
    if data["num"] != version or re.fullmatch(r"[0-9a-f]{64}", checksum) is None:
        raise ValueError("registry returned invalid version/checksum")
    return checksum


def registry_decision(expected, actual):
    if actual is None:
        return "publish"
    if actual != expected:
        raise ValueError(
            "published archive differs from verified release; preserve version and tag"
        )
    return "verify-existing"


def read_evidence(directory):
    evidence = json.loads((directory / "release.json").read_text())
    if re.fullmatch(VERSION, evidence["version"]) is None:
        raise ValueError("invalid release evidence version")
    name = f"worktree-pool-{evidence['version']}.crate"
    if (
        evidence["archive"] != name
        or sha256(directory / name) != evidence["archive_sha256"]
    ):
        raise ValueError("release artifact checksum/name mismatch")
    return evidence


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("prepare", "check", "verify-registry"))
    parser.add_argument(
        "--source", type=Path, default=Path(__file__).resolve().parents[1]
    )
    parser.add_argument("--tag")
    parser.add_argument("--evidence", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.command != "verify-registry" and args.tag is None:
        parser.error("prepare/check require --tag")
    if args.command != "prepare" and args.evidence is None:
        parser.error("check/verify-registry require --evidence")
    os.umask(0o077)
    output = args.output.resolve()
    output.mkdir(mode=0o700)  # Never reuse or remove caller-owned evidence.
    if args.command == "prepare":
        evidence = validate_source(args.source, args.tag)
        archive = package_archive(args.source, output)
        evidence.update(
            archive=archive.name,
            archive_sha256=sha256(archive),
            cargo=subprocess.check_output(["cargo", "-V"], text=True).strip(),
            rustc=subprocess.check_output(["rustc", "-Vv"], text=True),
        )
        write_json(output / "release.json", evidence)
    else:
        evidence = read_evidence(args.evidence)
        if args.command == "check":
            current = validate_source(args.source, args.tag)
            if any(current[key] != evidence[key] for key in current):
                raise ValueError(
                    "release source differs from approved tag/commit/version"
                )
            if (
                subprocess.check_output(["cargo", "-V"], text=True).strip()
                != evidence["cargo"]
            ):
                raise ValueError("Cargo toolchain differs from verified package")
            if (
                subprocess.check_output(["rustc", "-Vv"], text=True)
                != evidence["rustc"]
            ):
                raise ValueError("Rust toolchain differs from verified package")
            archive = package_archive(args.source, output)
            if sha256(archive) != evidence["archive_sha256"]:
                raise ValueError("release packaging is not reproducible")
            decision = registry_decision(
                evidence["archive_sha256"], registry_checksum(evidence["version"])
            )
            write_json(output / "state.json", {**current, "decision": decision})
        else:
            # Only registry propagation (404) is retried; transport/auth/server failures stop.
            for attempt in range(12):
                actual = registry_checksum(evidence["version"])
                if actual is not None:
                    registry_decision(evidence["archive_sha256"], actual)
                    write_json(
                        output / "registry.json",
                        {"version": evidence["version"], "archive_sha256": actual},
                    )
                    break
                if attempt == 11:
                    raise ValueError(
                        "published version not visible; inspect registry before retrying upload"
                    )
                time.sleep(5)
    print(output)


if __name__ == "__main__":
    main()
