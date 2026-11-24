#!/usr/bin/env python3
"""Exports Composer's installer fixtures that run `update` into crates/pun-packages/tests/fixtures/resolver.json,
with the lock real Composer writes for each.

Usage: scripts/export-resolver-fixtures.py <composer source checkout>

Each case keeps the fixture's composer.json and lock and adds what `composer update --no-install` (the pinned
Composer on the PATH, run on the system PHP) wrote: the locked names and versions of `packages` and
`packages-dev` and the whole composer.lock text, or the exit code when it failed. The platform it ran on is recorded once, from
`composer show --platform`. Fixtures that use repositories other than `package`, plugins, or conditions are left
out.
"""

import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "crates/pun-packages/tests/fixtures/resolver.json"
RESOLVER_FLAGS = (
    r"--(no-dev|no-install|prefer-lowest|prefer-stable|minimal-changes|with-dependencies|with-all-dependencies"
    r"|ignore-platform-reqs|ignore-platform-req=[A-Za-z0-9_.*+-]+)"
)


def sections(text):
    parts = re.split(r"^--([A-Z-]+)--\n", text, flags=re.M)
    return {parts[i]: parts[i + 1] for i in range(1, len(parts) - 1, 2)}


def composer(args, cwd, home):
    env = dict(os.environ, COMPOSER_HOME=home, COMPOSER_CACHE_DIR=f"{home}/cache", COMPOSER_NO_INTERACTION="1")
    env.pop("COMPOSER_PREFER_DEV_OVER_PRERELEASE", None)
    return subprocess.run(["composer", *args], cwd=cwd, env=env, capture_output=True, text=True)


def main():
    source = Path(sys.argv[1]) / "tests/Composer/Test/Fixtures/installer"
    cases = []
    with tempfile.TemporaryDirectory() as home:
        for path in sorted(source.glob("*.test")):
            s = sections(path.read_text())
            run = s.get("RUN", "").strip()
            if not run.startswith("update") or "CONDITION" in s or "EXPECT-EXCEPTION" in s:
                continue
            args = run.split()[1:]
            if any(a in ("mirrors", "lock", "nothing", "--lock", "--dry-run") for a in args):
                continue
            manifest = json.loads(s["COMPOSER"])
            repos = manifest.get("repositories", [])
            if isinstance(repos, dict) or any(r.get("type") != "package" for r in repos if isinstance(r, dict)):
                continue
            if "plugin" in json.dumps(manifest).lower():
                continue
            manifest["repositories"] = repos + [{"packagist.org": False}]
            with tempfile.TemporaryDirectory() as project:
                Path(project, "composer.json").write_text(json.dumps(manifest, indent=4))
                lock = json.loads(s["LOCK"]) if "LOCK" in s else None
                if lock is not None:
                    Path(project, "composer.lock").write_text(json.dumps(lock, indent=4))
                flags = [a for a in args if a.startswith("-") and a != "-v"]
                # Only resolver options: one like --working-dir would point Composer at another project.
                if any(not re.fullmatch(RESOLVER_FLAGS, a) for a in flags):
                    sys.exit(f"{path.name}: not a resolver option: {' '.join(flags)}")
                names = [a for a in args if not a.startswith("-")]
                result = composer(
                    ["update", *names, *flags, "--no-install", "--no-scripts", "--no-plugins", "--no-audit"],
                    project,
                    home,
                )
                case = {"name": path.stem, "composer": json.loads(s["COMPOSER"]), "args": names + flags}
                if lock is not None:
                    case["lock"] = lock
                if result.returncode != 0:
                    case["expect"] = {"exit": result.returncode}
                elif not Path(project, "composer.lock").exists():
                    continue
                else:
                    text = Path(project, "composer.lock").read_text()
                    written = json.loads(text)
                    case["expect"] = {
                        key: [[p["name"], p["version"]] for p in written.get(key) or []]
                        for key in ("packages", "packages-dev")
                    }
                    case["expect"]["aliases"] = written.get("aliases", [])
                    case["expect"]["text"] = text
                cases.append(case)
        with tempfile.TemporaryDirectory() as project:
            Path(project, "composer.json").write_text("{}")
            shown = json.loads(composer(["show", "--platform", "--format=json"], project, home).stdout)
    platform = [{"name": p["name"], "version": p["version"]} for p in shown["platform"]]
    OUT.write_text(json.dumps({"platform": platform, "cases": cases}, indent=1) + "\n")
    print(f"{len(cases)} cases")


main()
