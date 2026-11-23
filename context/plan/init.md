# Plan: `pun init`

Status: Build 1-3 and 5 are built (crates/pun-cli/src/init.rs, tests/init.rs). Build 4 is not built; analyse.md
Build 1-2, which it waited for, has landed. Decisions with dates: ../PLAN.md §2.

## 1. What it is

`pun init` writes pun.toml for a project that has no Pint, PHP-CS-Fixer, Rector or PHPStan config yet. A project that
has one runs `pun migrate` instead. Every other command stops without pun.toml and says "run `pun migrate`, or
`pun init` for a new project".

## 2. The flow

```
$ pun init

  Pun · new project setup
  Found Laravel 13, Pest 4, PHP ^8.4 (composer.json)

? Code style                    › laravel        laravel | psr12
? Lint sets                     › [x] dead-code  [x] modernize  [ ] code-quality  [ ] types
                                  [ ] early-return  [ ] privatization  [ ] naming  [ ] coding-style
? Analysis preset               › recommended    basic | essential | recommended | strict | max

  Extensions on from composer.lock: Laravel, Pest and PHPUnit. See [extensions] in pun.toml.

  ✔ Wrote pun.toml
  Checking the project with these settings…
  ✖ 14 files need formatting · 9 lint problems (9 fixable) · 3 analyse errors · 212 files in 0.3s

? Fix what can be fixed now (pun check --fix)?          › yes
? Write a baseline for the 3 remaining analyse errors?  › no

  Done. Run `pun check` any time; `pun check --fix` before you commit.
  Add `pun check` to CI and your composer scripts.
```

1. **Detect** from composer.json and composer.lock: the PHP version, Laravel, Pest, PHPUnit and their versions.
2. **Ask three questions** with the detected defaults.
3. **Write pun.toml** with only the three choices.
4. **Show what the settings find** with one `pun check` run, so the choice isn't blind.
5. **Offer the next steps**: fix now and baseline the rest (interactive only). Composer scripts, CI and hooks are
   printed as commands, not written.

Non-interactive (agents, CI, no terminal, `--yes`): the defaults, or the flags `--style laravel --lint-sets
dead-code,types --analyse strict`. Nothing is asked; next steps are printed, not run.

## 3. Build

1. Done.
2. Done.
3. Done.
4. **One `pun check`** with the new settings after writing pun.toml, then offer `--fix` and a baseline (interactive
   only).
5. Done. *Done when* the flow in §2 is pinned by a test on a scratch Laravel project, interactive and not.

## Decisions

From Pushpak:

- **Defaults:** `laravel` style for a Laravel app, else `psr12`; dead-code + modernize; `recommended`.
- **Old configs present → offer migrate** ("Found phpstan.neon. Migrate it instead? › yes"); yes runs `pun migrate`.
- **pun.toml holds only the three choices.** Presets carry the rules; `pun config` prints the resolved list. init
  refuses to overwrite an existing pun.toml.
- **No workflow wiring in init** (too much code to maintain for now). init prints the commands. Later, `pun-init` and
  `pun-migrate` agent skills do the composer, CI and hook wiring.

Assistant's calls (change them freely):

- **`empty` is not offered.** It stays a valid `[format] preset`.
- **Old config without a terminal exits 2** and writes nothing: "Found pint.json: this project fits `pun migrate`,
  which turns it into pun.toml. Nothing written." It is the same refusal as an existing pun.toml, so scripts and agents
  notice it. On a terminal, "no" to the migrate offer goes on with the questions.
- **The header names major versions** ("Laravel 13, Pest 4"), a branch by its locked name (`dev-main`), PHPUnit only
  without Pest (Pest runs on it), the PHP constraint as written, and the composer files actually read.
- **The answers print as dialoguer's default theme reports them** ("Code style: laravel"), with the same lines without
  a terminal.
- **`CI`** (set, and not empty, `0` or `false`) counts as non-interactive.
- **Flags also set the terminal's defaults**: `pun init --style psr12` on a terminal starts the question on psr12.
- **Flag errors keep schema.rs's wording** with the flag in place of the pun.toml location: "--lint-sets: `x` is not a
  lint set; the sets are …", "--analyse x: expected `basic`, …". `--lint-sets ""` means no sets.
- **pun.toml starts with ``# Written by `pun init`.``**, as migrate's does.
- **Agent mode** prints one `pun init: ok, pun.toml written, style …, lint sets …, analyse …` line, with the next steps
  as its item.
