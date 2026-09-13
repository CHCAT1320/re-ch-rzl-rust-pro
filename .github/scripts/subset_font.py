#!/usr/bin/env python3
"""Subset the embedded font down to the characters the program can draw.

Every string the UI renders is a hard-coded literal in `src/*.rs` (chart text
such as the song name is never drawn), so collecting the characters inside Rust
string literals is enough to cover all rendered text. Printable ASCII and the
characters produced by `format!` for numbers are added unconditionally.

The result is written to assets/fonts/rizline-subset.ttf, which is what
src/main.rs embeds. Re-run this after changing any user-visible string.

Usage:
    python .github/scripts/subset_font.py
    python .github/scripts/subset_font.py --font assets/fonts/rizline.ttf \
        --out assets/fonts/rizline-subset.ttf --src src
"""

from __future__ import annotations

import argparse
import pathlib
import sys
import tempfile

try:
    from fontTools.subset import main as subset_main
except ImportError:  # pragma: no cover - depends on the local environment
    sys.exit("fonttools is required: python -m pip install fonttools")


def string_literal_chars(text: str) -> set[str]:
    """Collect the characters inside Rust string literals, skipping comments."""
    chars: set[str] = set()
    i = 0
    n = len(text)
    while i < n:
        c = text[i]

        if c == "/" and text.startswith("//", i):
            i = text.find("\n", i)
            if i == -1:
                break
            continue

        if c == "/" and text.startswith("/*", i):
            depth = 1
            i += 2
            while i < n and depth:
                if text.startswith("/*", i):
                    depth += 1
                    i += 2
                elif text.startswith("*/", i):
                    depth -= 1
                    i += 2
                else:
                    i += 1
            continue

        # Raw strings: r"...", r#"..."#, br#"..."#. The prefix is skipped so the
        # scan lands on the opening quote.
        raw = c == "r" or (c == "b" and text.startswith("br", i))
        if raw:
            j = i + 1 if c == "r" else i + 2
            hashes = 0
            while j < n and text[j] == "#":
                hashes += 1
                j += 1
            if j < n and text[j] == '"':
                close = '"' + "#" * hashes
                j += 1
                end = text.find(close, j)
                if end == -1:
                    chars.update(text[j:])
                    break
                chars.update(text[j:end])
                i = end + len(close)
                continue

        if c == '"':
            i += 1
            literal: list[str] = []
            while i < n:
                d = text[i]
                if d == "\\" and i + 1 < n:
                    literal.append(text[i + 1])
                    i += 2
                    continue
                if d == '"':
                    i += 1
                    break
                literal.append(d)
                i += 1
            chars.update("".join(literal))
            continue

        i += 1

    return chars


def collect_chars(src_dir: pathlib.Path) -> set[str]:
    chars = {chr(code) for code in range(0x20, 0x7F)}
    files = sorted(src_dir.rglob("*.rs"))
    if not files:
        sys.exit(f"no .rs files under {src_dir}")
    for path in files:
        chars |= string_literal_chars(path.read_text(encoding="utf-8"))
    return chars


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--font", type=pathlib.Path, default=pathlib.Path("assets/fonts/rizline.ttf"))
    parser.add_argument("--out", type=pathlib.Path, default=pathlib.Path("assets/fonts/rizline-subset.ttf"))
    parser.add_argument("--src", type=pathlib.Path, default=pathlib.Path("src"))
    args = parser.parse_args()

    if not args.font.is_file():
        sys.exit(f"source font not found: {args.font}")

    chars = collect_chars(args.src)
    text = "".join(sorted(chars))

    args.out.parent.mkdir(parents=True, exist_ok=True)

    with tempfile.NamedTemporaryFile("w", encoding="utf-8", suffix=".txt", delete=False) as handle:
        handle.write(text)
        charset_file = pathlib.Path(handle.name)

    try:
        subset_main(
            [
                str(args.font),
                f"--text-file={charset_file}",
                f"--output-file={args.out}",
                "--layout-features=*",
                "--drop-tables+=EBDT,EBLC,CBDT,CBLC,gasp",
            ]
        )
    finally:
        charset_file.unlink(missing_ok=True)

    before = args.font.stat().st_size
    after = args.out.stat().st_size
    print(f"glyph set : {len(chars)} chars")
    print(f"font      : {before / 1048576:.2f} MB -> {after / 1024:.1f} KB ({args.out})")


if __name__ == "__main__":
    main()
