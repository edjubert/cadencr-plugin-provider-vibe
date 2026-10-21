#!/usr/bin/env python3
"""Build the native connector at CadencR's stable settings-import entrypoint."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[1]


def main():
    subprocess.run(
        [
            "cargo", "build", "--release", "--locked",
            "--manifest-path", str(ROOT / "Cargo.toml"),
            "--target-dir", str(ROOT / "target"),
        ],
        cwd=ROOT,
        check=True,
    )
    suffix = ".exe" if os.name == "nt" else ""
    source = ROOT / "target" / "release" / f"cadencr-plugin-provider-vibe{suffix}"
    destination = ROOT / "bin" / f"provider{suffix}"
    destination.parent.mkdir(exist_ok=True)
    with tempfile.NamedTemporaryFile(
        prefix=".provider-", dir=destination.parent, delete=False
    ) as temporary:
        staged = Path(temporary.name)
    try:
        shutil.copy2(source, staged)
        if os.name != "nt":
            staged.chmod(0o755)
        staged.replace(destination)
    finally:
        staged.unlink(missing_ok=True)
    print(f"Ready to import into CadencR: {destination}")


if __name__ == "__main__":
    main()
