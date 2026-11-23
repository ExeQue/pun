# Plan: `pun migrate`

Status: **built**. Build 1-6 and the composer.json step (plan/packages.md) are done. Decisions with dates:
../PLAN.md §2.

## 1. What it is

`pun migrate` moves a project's pint.json, .php-cs-fixer(.dist).php, rector.php and phpstan.neon onto one pun.toml.
It moves everything Pun supports and names everything it doesn't. The proof is the harness: `pun migrate` followed by
`pun check --fix` gives the same files as the old tools.

## 2. Today (`pun-cli` migrate.rs, `pun-config` migrate/)

- An interactive picker detects pint.json, .php-cs-fixer.php, phpstan.neon, rector.php and mago.toml and shows a
  checklist on a terminal. `--from pint,phpstan` and `--yes` skip it, as do agent mode and no terminal.
- mago.toml is listed as "not supported yet". With no config at all, migrate offers a starter pun.toml.
- The run is idempotent.

## 3. The decided behaviour

**Unsupported items are named, skipped, and Pun asks for a PR.** There is no fallback to PHP:
```
 ✔ pint.json        → [format] preset laravel, 6 rules changed, 3 risky rules → [lint]
 ✔ rector.php       → [lint] sets dead-code, code-quality; 4 rules
 ✔ phpstan.neon     → [analyse] 212 rules (level 6), 14 ignoreErrors
 ⚠ phpdoc_tag_casing                   not supported by Pun yet; skipped
 ⚠ phpstan: Mockery extension          not supported by Pun yet; skipped
   Want them? Send a PR to https://github.com/shipfastlabs/pun
 ⚠ phpstan-baseline.neon was not copied. Run `pun analyse --generate-baseline` for a Pun baseline.
```
- **pint.json and .php-cs-fixer.php:** the preset, then a delta of rules with TOML option names. `per` expands into
  explicit rules; a `symfony` preset stops migrate with an error. Risky and code-deleting rules go to `[lint.rules]`
  at `error`. Custom fixer classes are named as unsupported. The Finder (paths, exclude, notPath) moves to
  `[format]`; `notName` is named as unsupported.
- **rector.php:** paths and skip, prepared sets to `[lint] sets` one to one, single rules to `[lint.rules]`, PHP and
  Laravel level sets to the upgrade groups, all at `error`. Symfony, Doctrine and downgrade sets are named as
  unsupported.
- **phpstan.neon:** the level expands into the exact rules it ran, parameters into rule options, `ignoreErrors` into
  `[[analyse.ignore]]` entries (`reportUnmatched: false` and `reportUnmatchedIgnoredErrors: false` become
  `report-unmatched = false` on each entry), and every extension that isn't built in is named. Larastan, Pest's
  plugin, phpstan-phpunit, strict-rules and deprecation-rules are built in. `stubFiles` carry over.
- **Extensions:** migrate writes `[extensions]` for the Laravel, Pest and PHPUnit packages composer.lock locks.
- **Merged rules:** upstream names that do the same thing (Pint's `array_push` and Rector's
  `ChangeArrayPushToArrayAssignRector`) map to one Pun rule, written once even when both tools enabled it. An old Pun
  ID in a pun.toml stops with an error naming the new rule. docs/content/docs/migrate/merged-rules.mdx lists every
  merge.
- **Baselines are never copied.** Migrate tells the user to run `pun analyse --generate-baseline`.
- **Ignore comments:** `@php-cs-fixer-ignore` becomes `@pun-ignore-file` with Pun IDs. When phpstan.neon is migrated,
  `@phpstan-ignore <identifiers>` becomes `@pun-ignore <rule IDs>`, and `@phpstan-ignore-line` / `-next-line` become
  `@pun-ignore` / `@pun-ignore-next-line` naming the rules analyse reports on that line. A comment Pun can't map is
  left as is and named.
- **Closest preset:** opt-in. With `--offer-preset` on a terminal, after writing exact rules, migrate offers the closest
  preset with a count of the problems the switch adds and drops ("You're on PHPStan level 8; the closest Pun preset is
  `strict`. Switch?"). Without the flag it never asks: the exact rules are what the project ran (Pushpak, 2026-11-23).
- **No workflow edits.** Migrate doesn't touch composer scripts, CI, hooks or editor settings. Future `pun-init` and
  `pun-migrate` agent skills will do that.

## 4. Build

1. Done. *Done when* `pun migrate` + `pun check --fix` equals Pint on all 8 corpus projects.
2. Done.
3. Done.
4. Done. *Done when* migrated Pinkary gives the same `pun lint --fix` result as `rector process`, after format.
5. Done. *Done when* migrated corpus projects report the same errors as PHPStan.
6. Done.

## Decisions

- **Complete and explicit** (Pushpak): unsupported items are named and skipped with a request to send a PR to
  shipfastlabs/pun, never sent to PHP. Baselines are never copied. Everything migrates at `error`. The closest preset
  is offered interactively. No workflow wiring.
- **Open:** `--remove-tools` (../PLAN.md §4).
- **Rule sets expand from the pinned PHP-CS-Fixer's own set definitions.** crates/pun-config/presets/sets.json holds
  every set (exported by scripts/export-rule-sets.php), so `per` and any set in a pint.json expand into explicit
  rules. The `@auto` sets read the project's composer.json, so they stay unexpanded and are named.
  `pun format --preset per` without a pun.toml names per's unported rules under their Pun IDs.
- **"Can run" means a native fixer** (`pun_fixers::all`), the same test that config.md Build 6 applies at run
  time. A rule without one (no Pun ID, deprecated, not ported, or an option TOML can't hold) is named and left out. A
  preset rule Pun can't run is written as `false`, so the migrated pun.toml always runs. Migrate then exits 0.
- **The `.php-cs-fixer.php` Finder** is read in the same `php -r` run. `in()` folders become `[files] paths`
  (relative to the project, the root itself left out). `exclude()` (without the default `vendor`) and
  `notPath()` become `[format]` keys, as for pint.json; `notName()` is named as unsupported. Any other Finder call, a `name()` other than `*.php`, or a
  Finder that is not a Symfony Finder is named as `Finder <call>: no pun.toml key for it`. An empty Finder list is left
  out of pun.toml.
