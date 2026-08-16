#!/usr/bin/env python3
"""Verify that relative links in Markdown files point at files that exist.

The documentation is a large part of this project and cross-links heavily, so
a rename that silently breaks a dozen links is a realistic failure. Runs in CI
and needs no dependencies.

External links (http, https, mailto) and pure anchors are not checked.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

LINK = re.compile(r"\[[^\]]*\]\(([^)]+)\)")
SKIP_PREFIXES = ("http://", "https://", "mailto:", "#")
SKIP_DIRS = {".git", "node_modules", "target", "dist"}


def markdown_files(root: Path) -> list[Path]:
    return [
        path
        for path in sorted(root.rglob("*.md"))
        if not SKIP_DIRS & set(path.relative_to(root).parts)
    ]


def main() -> int:
    root = Path(__file__).resolve().parent.parent
    broken: list[str] = []

    for path in markdown_files(root):
        for match in LINK.finditer(path.read_text(encoding="utf-8")):
            link = match.group(1).strip()
            if link.startswith(SKIP_PREFIXES):
                continue
            target = (path.parent / link.split("#")[0]).resolve()
            if not target.exists():
                broken.append(f"{path.relative_to(root)}: {link}")

    if broken:
        print("Tote interne Links gefunden:\n", file=sys.stderr)
        for entry in broken:
            print(f"  {entry}", file=sys.stderr)
        return 1

    print(f"{len(markdown_files(root))} Markdown-Dateien geprüft, keine toten Links.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
