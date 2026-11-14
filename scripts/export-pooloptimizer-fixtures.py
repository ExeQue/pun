#!/usr/bin/env python3
"""Exports Composer's pool optimizer fixtures into crates/pun-packages/tests/fixtures/pooloptimizer.json.

Usage: scripts/export-pooloptimizer-fixtures.py <composer source checkout>

Each case keeps the fixture's REQUEST (require, locked, fixed, preferStable, preferLowest), POOL-BEFORE (the pool
`PoolOptimizerTest` optimizes) and POOL-AFTER (the packages Composer's optimizer keeps).
"""

import json
import re
import sys
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "crates/pun-packages/tests/fixtures/pooloptimizer.json"


def sections(text):
    parts = re.split(r"^--([A-Z-]+)--\n", text, flags=re.M)
    return {parts[i]: parts[i + 1] for i in range(1, len(parts) - 1, 2)}


def main():
    source = Path(sys.argv[1]) / "tests/Composer/Test/DependencyResolver/Fixtures/pooloptimizer"
    cases = []
    for path in sorted(source.glob("*.test")):
        s = sections(path.read_text())
        cases.append({
            "name": path.stem,
            "request": json.loads(s["REQUEST"]),
            "before": json.loads(s["POOL-BEFORE"]),
            "after": json.loads(s["POOL-AFTER"]),
        })
    OUT.write_text(json.dumps({"cases": cases}, indent=1) + "\n")


if __name__ == "__main__":
    main()
