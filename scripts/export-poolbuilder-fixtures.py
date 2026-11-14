#!/usr/bin/env python3
"""Exports Composer's pool builder fixtures into crates/pun-packages/tests/fixtures/poolbuilder.json.

Usage: scripts/export-poolbuilder-fixtures.py <composer source checkout>

Each case keeps the fixture's ROOT, REQUEST, FIXED, PACKAGE-REPOS and EXPECT (the packages Composer's
`PoolBuilderTest` finds in the unoptimized pool). The composer.json files its path repositories read are kept under
`files`, by path relative to the fixtures directory.
"""

import json
import re
import sys
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "crates/pun-packages/tests/fixtures/poolbuilder.json"


def sections(text):
    parts = re.split(r"^--([A-Z-]+)--\n", text, flags=re.M)
    return {parts[i]: parts[i + 1] for i in range(1, len(parts) - 1, 2)}


def main():
    source = Path(sys.argv[1]) / "tests/Composer/Test/DependencyResolver/Fixtures/poolbuilder"
    cases = []
    for path in sorted(source.glob("*.test")):
        s = sections(path.read_text())
        parse = lambda key, default: json.loads(s[key]) if s.get(key, "").strip() else default
        cases.append({
            "name": path.stem,
            "root": parse("ROOT", {}),
            "request": parse("REQUEST", {}),
            "fixed": parse("FIXED", []),
            "repos": parse("PACKAGE-REPOS", []),
            "expect": parse("EXPECT", []),
        })
    files = {
        str(f.relative_to(source)): f.read_text()
        for f in sorted(source.glob("*/composer.json"))
    }
    OUT.write_text(json.dumps({"cases": cases, "files": files}, indent=1) + "\n")


if __name__ == "__main__":
    main()
