# Plan: foundations, harness, docs and release

Status: housekeeping parts that every command depends on (old components C0, C10-C14). Last checked against main at
8e11612 (2026-11-14).

## 1. CI, gates and how threads work (C0)

**Today.** Cargo workspace (12 crates, `punphp-<x>` packages, all `publish = false`, every one on the shared
`[workspace.lints]`; Mago pinned at `=1.57.0`). CI runs the rule-mapping and docs checks, rustfmt, clippy with
`-D warnings`, the tests under cargo-nextest (`punphp-analyse` in its own job), parity and fuzz; a Coverage
workflow, run on request, runs `scripts/coverage.sh` (100% per crate); cargo-dist releases on tags. CI is enabled on GitHub and green
on main. The oracle is pinned in `.oracle/` (reference/oracle.md); 7 corpus projects (`corpus/manifest.toml`).

**Build.**
1. Get coverage green: the Coverage workflow (run on request) fails, with five crates under 100% (reports/COVERAGE.md).
2. Update AGENTS.md when the new design lands: rule names, no delegate mode, and "output must match upstream" narrowed
   to exit codes and machine formats.

## 2. Parity harness, corpus and fuzz (C11)

**Today.** `pun-harness format` on 7 projects writes reports/scoreboard.md; `scripts/fuzz-parity.sh`;
`scripts/cli-parity.sh`; exporters for upstream fixtures. Analyse uses `scripts/analyse-corpus.py`.

**Build.**
1. After config.md Build 1, the harness uses the new names (it runs `pun migrate` + `pun check --fix`).
2. Lint mode: `format(pun lint --fix)` against `format(rector process)` per project and per rule.
3. Analyse mode: errors (file, line, identifier) against PHPStan + Larastan per project and preset.
4. `scripts/cli-parity.sh` covers only exit codes and machine formats once Pint's screen goes.
5. Benchmark rig in CI. (pun-harness is dev-only, out of dist, and its fixtures live in pun-fixers/tests/fixtures.)

## 3. Docs site and brand (C12)

**Today.** Blume site (`docs/`), Sticker Pop design system (`docs/design/`). **Build:** update the pages as each command
lands (`.claude/skills/blume-update-docs`); the generated "Coming from Pint / PHPStan / Rector" page from mapping.toml;
one page per rule from mapping.toml; output samples in the docs use the brand colours.

## 4. Release (C13)

**Today.** Pun ships as the Composer package `punphp/pun`, the way Mago ships, and through the standalone installer below. `composer.json` sits at the repo
root, `composer/bin/pun` is the shim, and `.gitattributes` export-ignores everything else, so a tag's zipball holds
only `composer.json`, the shim, the README and LICENSE (`licenses/` is export-ignored). On first run the shim downloads `punphp-<target>.tar.gz` from the tag's GitHub release, checks its
`.sha256` and caches the binary in Composer's bin directory (`vendor/bin/pun-<version>-<target>`); it supports Linux (musl) and macOS on x86_64 and arm64.
cargo-dist builds those four archives on a tag push (`dist-workspace.toml`: no dist installers, no source tarball).
The standalone installer (Pushpak, 2026-11-14) is `docs/public/install`, which the docs site serves as
`https://pun.laravel.cloud/install`, as Bun serves `bun.sh/install`: `curl -fsSL https://pun.laravel.cloud/install | sh`
installs the latest stable release (the newest pre-release while there is none), and `sh -s -- v0.1.0` a named one. It
checks the archive's `.sha256`, and, with a signed-in `gh`, the GitHub artifact attestation the host job makes for every
archive. No crate, npm package or Homebrew tap is published.
The 0.1.0 betas (`v0.1.0-beta.1` to `beta.3`) were pre-releases; v0.1.0 is the first stable release (Pushpak,
2026-11-14), tagged and released on GitHub on 2026-11-14, so `composer require --dev punphp/pun` needs no `@beta`.
Packagist has the package (submitted 2026-11-11) but only the betas so far. The shim's `PUN_VERSION` follows the
workspace version.
**Build:** v0.1.0 on Packagist with the GitHub hook, then check its Composer and installer installs, then
publishing 1.0 after ../PLAN.md §3 Step 4.

## 5. Later (C10, C14)

- **LSP:** the last step, after 1.0's commands. Nothing is in the repo for it.
- **`pun run`, `pun test`:** the archived plan's phases 6-7. In 1.0 or after is open (../PLAN.md §4). `pun install`
  (phase 5) is built; the package manager's plan is packages.md.
- **Native Blade formatter** and a Pun reprinting preset: after 1.0 (format.md).
- **An extension system** (Mockery, Livewire, Filament, a project's own rules): after 1.0 (analyse.md §9).
