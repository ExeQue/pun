# Plan: foundations, harness, docs and release

Status: the housekeeping parts every command depends on (C0, C10-C14).

## 1. CI, gates and how threads work (C0)

**Today.** A Cargo workspace of 11 crates (`punphp-<x>` packages, all `publish = false`). Five use the shared
`[workspace.lints]`; the six that port upstream PHP carry their own `[lints]` table. The Mago crates are pinned at
`=1.61.0`. CI runs the rule-mapping and docs checks, rustfmt, clippy with `-D warnings`, the tests under
cargo-nextest (`punphp-analyse` in its own job), parity and fuzz. The Coverage workflow runs `scripts/coverage.sh`
(100% per crate) on request. cargo-dist releases on tags. The oracle is pinned in `.oracle/` (reference/oracle.md);
the corpus projects are in `corpus/manifest.toml`.

**Build.**
1. Get coverage green: the Coverage workflow fails, with five crates under 100% (reports/COVERAGE.md).
2. Update AGENTS.md for the new design: rule names, no delegate mode, and "output must match upstream" narrowed to
   exit codes and machine formats.

## 2. Parity harness, corpus and fuzz (C11)

**Today.** `pun-harness format` runs on the corpus and writes reports/scoreboard.md. `scripts/fuzz-parity.sh`,
`scripts/cli-parity.sh` and the upstream fixture exporters exist. Analyse uses `scripts/analyse-corpus.py`.

**Build.**
1. After config.md Build 1, the harness uses the new names (it runs `pun migrate` + `pun check --fix`).
2. Lint mode: `format(pun lint --fix)` against `format(rector process)` per project and per rule.
3. Analyse mode: errors (file, line, identifier) against PHPStan + Larastan per project and preset.
4. `scripts/cli-parity.sh` covers only exit codes and machine formats once Pint's screen goes.
5. A benchmark rig in CI. pun-harness is dev-only and out of dist; its fixtures live in pun-fixers/tests/fixtures.

## 3. Docs site and brand (C12)

**Today.** The Blume site (`docs/`) and the Sticker Pop design system (`docs/design/`).

**Build.** Update the pages as each command lands (`.claude/skills/blume-update-docs`). Output samples in the docs
use the brand colours. The rule pages and "Coming from" pages are plan/docs.md.

## 4. Release (C13)

**Today.** Pun ships two ways. No crate, npm package or Homebrew tap is published.

- **Composer package `punphp/pun`.** `composer.json` sits at the repo root and `composer/bin/pun` is the shim.
  `.gitattributes` export-ignores everything else, so a tag's zipball holds only `composer.json`, the shim, the README
  and LICENSE. On first run the shim downloads `punphp-<target>.tar.gz` from the tag's GitHub release, checks its
  `.sha256` and caches the binary in Composer's bin directory (`vendor/bin/pun-<version>-<target>`). The shim's
  `PUN_VERSION` follows the workspace version.
- **Standalone installer** `docs/public/install`, served as `https://pun.laravel.cloud/install` (Pushpak).
  `curl -fsSL https://pun.laravel.cloud/install | sh` installs the latest stable release, and `sh -s -- v0.1.0` a
  named one. It checks the archive's `.sha256` and, with a signed-in `gh`, the GitHub artifact attestation the host
  job makes for every archive.

cargo-dist builds the four archives on a tag push: Linux (musl) and macOS, x86_64 and arm64 (`dist-workspace.toml`:
no dist installers, no source tarball). Tags run to v0.2.7; Packagist lists releases up to v0.2.0.

**Build.** Packagist follows every tag (the GitHub hook), then check its Composer and installer installs. Publish 1.0
after ../PLAN.md §3 Step 4.

## 5. Later (C10, C14)

- **LSP:** the last step, after 1.0's commands. Nothing is in the repo for it.
- **`pun run`, `pun test`:** whether in 1.0 or after is open (../PLAN.md §4). `pun install` is built; the package
  manager's plan is packages.md.
- **Native Blade formatter** and a Pun reprinting preset: after 1.0 (format.md).
- **More extensions** (Mockery, Livewire, Filament, a project's own rules): after 1.0 (analyse.md §9). The extension
  system itself is plan/extensions.md.
