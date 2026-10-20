#!/usr/bin/env python3
"""Export the connector's SVG logo and transparent PNG size ladder."""

from pathlib import Path
import shutil
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]
SIZES = (16, 24, 32, 48, 64, 128, 256, 512, 1024)


def main():
    renderer = shutil.which("rsvg-convert")
    if renderer is None:
        sys.exit("rsvg-convert is required. On macOS: brew install librsvg")

    source = ROOT / "icon.svg"
    destination = ROOT / "logos"
    destination.mkdir(exist_ok=True)
    svg = source.read_text(encoding="utf-8")
    (destination / "logo.svg").write_text(
        svg.replace('width="1em" height="1em"', 'width="512" height="512"'),
        encoding="utf-8",
    )
    for size in SIZES:
        subprocess.run(
            [
                renderer,
                "--width", str(size),
                "--height", str(size),
                "--output", str(destination / f"logo-{size}.png"),
                str(source),
            ],
            check=True,
        )
    print(f"Exported logo.svg and {len(SIZES)} PNG sizes to {destination}")


if __name__ == "__main__":
    main()
