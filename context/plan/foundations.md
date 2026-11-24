# Plan: foundations, harness, docs and release

Status: the housekeeping parts every command depends on (C0, C10-C14).

## 1. CI, gates and how threads work (C0)

**Today.** A Cargo workspace of 11 crates (`punphp-<x>` packages, all `publish = false`). Five use the shared
`[workspace.lints]`; the six that port upstream PHP carry their own `[lints]` table. The Mago crates are pinned at
`=1.61.0`. CI runs the rule-mapping and docs checks, shellcheck on the installer, rustfmt, clippy with
`-D warnings`, the tests under cargo-nextest (`punphp-analyse` in its own job), parity, fuzz, and a macOS build on
Rust 1.100 (the release toolchain). The Coverage workflow runs `scripts/coverage.sh` (100% per crate) on request.
cargo-dist releases on tags. The oracle is pinned in `.oracle/` (reference/oracle.md); the corpus projects are in
`corpus/manifest.toml`.

**Build.**
1. Get coverage green: the Coverage workflow fails. reports/COVERAGE.md has four crates under 100% (pun-config,
   pun-lint, pun-phpstan, pun-tokens), and its table has no row for pun-extension.
2. Update AGENTS.md for the new design. Done.

## 2. Parity harness, corpus and fuzz (C11)

**Today.** `pun-harness format` runs on the corpus and writes reports/scoreboard.md; it is the only mode.
pun-harness is dev-only and out of dist; its fixtures live in pun-fixers/tests/fixtures. `scripts/fuzz-parity.sh`,
`scripts/cli-parity.sh` and the upstream fixture exporters exist. Analyse uses `scripts/analyse-corpus.py`.

**Build.**
1. The harness uses the new names (it runs `pun migrate` + `pun check --fix`). Done.
2. Lint mode: `format(pun lint --fix)` against `format(rector process)` per project and per rule.
3. Analyse mode: errors (file, line, identifier) against PHPStan + Larastan per project and preset.
4. `scripts/cli-parity.sh` compares exit codes and resulting files, and output only for machine formats. Done.

## 3. Docs site and brand (C12)

**Today.** The Blume site (`docs/`) and the Sticker Pop design system (`docs/design/`).

**Build.** Update the pages as each command lands (`.claude/skills/blume-update-docs`). Output samples in the docs
use the brand colours. The rule pages and "Coming from" pages are plan/docs.md.

## 4. Release (C13)

**Today.** Pun ships two ways. No crate, npm package or Homebrew tap is published.

- **Composer package `shipfastlabs/pun`.** `composer.json` sits at the repo root and `composer/bin/pun` is the shim.
  `.gitattributes` export-ignores everything else, so a tag's zipball holds only `composer.json`, the shim, the README,
  LICENSE and the Laravel Boost guideline and skill (`resources/boost/`). On first run the shim downloads
  `punphp-<target>.tar.gz` from the tag's GitHub release, checks its `.sha256` and caches the binary in Composer's
  bin directory (`vendor/bin/pun-<version>-<target>`). The shim's `PUN_VERSION` follows the workspace version.
- **Standalone installer** `docs/public/install`, served as `https://punphp.com/install` (Pushpak).
  `curl -fsSL https://punphp.com/install | sh` installs the latest stable release, and `sh -s -- v0.1.0` a
  named one. It installs `punx` beside `pun`, checks the archive's `.sha256` and, with a signed-in `gh`, the GitHub
  artifact attestation the host job makes for every archive. `pun self-update` replaces the binary and verifies it
  the same way.

cargo-dist builds the four archives on a tag push: Linux (musl) and macOS, x86_64 and arm64 (`dist-workspace.toml`:
no dist installers, no source tarball). Tags run to v0.2.7; every tag from v0.1.0 has a GitHub release except v0.2.2,
whose macOS build failed. Packagist lists releases up to v0.2.0. On Linux x86_64 the v0.2.7 installer verifies the
attestation and `composer require --dev shipfastlabs/pun` installs v0.2.0.

**Build.** Packagist follows every tag (the GitHub hook), then check the Composer install on Linux and macOS and the
installer on macOS (../PLAN.md §3 Step 5). Publish 1.0 after ../PLAN.md §3 Step 4.

## 5. Later (C10, C14)

- **LSP:** the last step, after 1.0's commands. Nothing is in the repo for it.
- **`pun run`, `pun test`:** whether in 1.0 or after is open (../PLAN.md §4). `pun install` is built; the package
  manager's plan is packages.md.
- **Native Blade formatter** and a Pun reprinting preset: after 1.0 (format.md).
- **More extensions** (Mockery, Livewire, Filament, a project's own rules): after 1.0 (analyse.md §9). The extension
  system itself is plan/extensions.md.
