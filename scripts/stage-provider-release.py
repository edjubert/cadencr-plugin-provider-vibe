#!/usr/bin/env python3
"""Stage a native Apple Silicon build with its exact source and file hashes."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess


ROOT = Path(__file__).resolve().parents[1]


def output(*command):
    return subprocess.check_output(command, cwd=ROOT, text=True).strip()


def digest(path):
    data = path.read_bytes()
    return {"sha256": hashlib.sha256(data).hexdigest(), "size": len(data)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if platform.system() != "Darwin" or platform.machine() != "arm64":
        raise RuntimeError("Release staging requires a native Apple Silicon macOS host")
    subprocess.run(["git", "diff", "--quiet", "HEAD", "--"], cwd=ROOT, check=True)
    commit = output("git", "rev-parse", "HEAD")
    if os.environ.get("GITHUB_SHA", commit) != commit:
        raise RuntimeError("Checked-out source does not match the workflow revision")
    metadata = json.loads(output("cargo", "metadata", "--locked", "--no-deps",
                                 "--format-version", "1"))
    package = next(p for p in metadata["packages"]
                   if p["name"] == "cadencr-plugin-provider-vibe")
    binary = ROOT / "bin" / "provider"
    version = output(str(binary), "version")
    if version != package["version"]:
        raise RuntimeError("Built executable does not match the source version")
    args.output.mkdir()
    distribution = args.output / "distribution"
    (distribution / "bin").mkdir(parents=True)
    for relative in ("bin/provider", "icon.svg", "README.md", "LICENSE"):
        shutil.copy2(ROOT / relative, distribution / relative)
    (distribution / "bin" / "provider").chmod(0o755)
    repository = os.environ.get(
        "GITHUB_REPOSITORY", "edjubert/cadencr-plugin-provider-vibe"
    )
    run_id = os.environ.get("GITHUB_RUN_ID")
    receipt = {
        "schema_version": 1,
        "repository": package["repository"],
        "source_commit": commit,
        "version": version,
        "target": "darwin-aarch64",
        "build_run": None if run_id is None else {
            "id": run_id,
            "attempt": os.environ.get("GITHUB_RUN_ATTEMPT"),
            "url": f"https://github.com/{repository}/actions/runs/{run_id}",
        },
        "toolchain": {
            "rustc": output("rustc", "--version", "--verbose"),
            "cargo": output("cargo", "--version"),
            "macos": output("sw_vers", "-productVersion"),
            "deployment_target": os.environ.get("MACOSX_DEPLOYMENT_TARGET"),
        },
        "cargo_lock": digest(ROOT / "Cargo.lock"),
        "files": {
            path.relative_to(distribution).as_posix(): digest(path)
            for path in sorted(distribution.rglob("*")) if path.is_file()
        },
    }
    (args.output / "provenance.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps(receipt, indent=2))


if __name__ == "__main__":
    main()
