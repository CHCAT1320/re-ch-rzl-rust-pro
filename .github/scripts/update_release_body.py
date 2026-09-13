#!/usr/bin/env python3
"""Refresh the auto-generated sections of a release body in place.

The body keeps anything written by hand; only the regions between the markers
are replaced, so the asset list and the generated changelog stay current
without wiping manual edits. Missing markers are added on first use.

Usage:
    python .github/scripts/update_release_body.py BODY_FILE ASSETS_FILE CHANGES_FILE
"""

from __future__ import annotations

import pathlib
import sys

ASSETS_START = "<!-- rzl:assets:start -->"
ASSETS_END = "<!-- rzl:assets:end -->"
CHANGES_START = "<!-- rzl:changes:start -->"
CHANGES_END = "<!-- rzl:changes:end -->"


def replace_region(body: str, start: str, end: str, content: str) -> str | None:
    if start not in body or end not in body:
        return None
    before, rest = body.split(start, 1)
    _, after = rest.split(end, 1)
    return f"{before}{start}\n{content.strip()}\n{end}{after}"


def ensure_region(body: str, start: str, end: str, content: str, prepend: bool) -> str:
    replaced = replace_region(body, start, end, content)
    if replaced is not None:
        return replaced

    block = f"{start}\n{content.strip()}\n{end}"
    if not body.strip():
        return block
    return f"{block}\n\n{body}" if prepend else f"{body}\n\n{block}"


def main() -> None:
    body_path = pathlib.Path(sys.argv[1])
    assets = pathlib.Path(sys.argv[2]).read_text(encoding="utf-8")
    changes = pathlib.Path(sys.argv[3]).read_text(encoding="utf-8")

    body = body_path.read_text(encoding="utf-8") if body_path.exists() else ""
    body = ensure_region(body, ASSETS_START, ASSETS_END, assets, prepend=True)
    body = ensure_region(body, CHANGES_START, CHANGES_END, changes, prepend=False)

    body_path.write_text(body.rstrip() + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
