#!/usr/bin/env python3
from __future__ import annotations

import re
from pathlib import Path

MAX_WORD_LEN = 24
WORD_RE = re.compile(r"^[a-z]+$")


def normalize_words(src: Path) -> list[str]:
    """Parse the source dict and return sorted, deduped words."""
    seen: set[str] = set()
    with src.open("r", encoding="utf-8") as infile:
        for line in infile:
            word = line.split(";", 1)[0].strip().lower()
            if not word:
                continue
            if len(word) > MAX_WORD_LEN:
                continue
            if not WORD_RE.match(word):
                continue
            seen.add(word)
    return sorted(seen)


def main() -> None:
    base_dir = Path(__file__).resolve().parent.parent
    src = base_dir / "spreadthewordlist.dict"
    dest = base_dir / "words.txt"

    words = normalize_words(src)
    dest.write_text("\n".join(words) + "\n", encoding="utf-8")
    print(f"wrote {len(words)} words to {dest.relative_to(base_dir)}")


if __name__ == "__main__":
    main()
