# Plan: format (`pun format`)

Status: **walked through with Pushpak** (2026-10-17); the answers are in "Decisions". This file covers how the
formatter works, the decided design and the build list (§7). Decisions with dates: ../PLAN.md §2. Shared schema:
config.md. Output: output.md.

## 1. What it is

`pun format` rewrites the layout of PHP files exactly as Pint does. It never changes what the code does. On any
project, `pun check --fix` writes the same bytes as `pint`. `pun format` alone runs only the safe subset (§4).

**How parity is checked.** `pun-harness format` compares Pint with `pun migrate` followed by `pun check --fix` on every
corpus project, with the laravel and psr12 presets. `scripts/cli-parity.sh` compares exit codes, machine formats and
resulting files byte for byte. `scripts/fuzz-parity.sh` mutates corpus files (CRLF and mixed line endings included)
and compares the two tools. Today all 8 corpus projects and the fuzzer show 0 differing files
([scoreboard](../reports/scoreboard.md)).

## 2. How it works

Pint wraps PHP-CS-Fixer, a token fixer: it edits only the tokens a rule objects to and doesn't reprint the file. A
pretty-printer can never match that byte for byte, so Pun reimplements PHP-CS-Fixer's fixers.

For each file:

1. **Lex** into tokens identical to PHP's `token_get_all` (pun-tokens' lexer, checked against PHP).
2. **Transform:** retag tokens into PHP-CS-Fixer's custom kinds (array brackets, nullable `?`, union `|`,
   named-argument colons, attribute brackets and so on).
3. **Lint:** check that the file parses with PHP's own grammar. Files with syntax errors are reported and skipped, as
   Pint does.
4. **Run the fixers** in PHP-CS-Fixer's order (priority, then name), each exactly once, on the shared token list.
   There is no repeat-until-stable loop, because upstream has none.
5. **Print** the tokens back. Unchanged tokens keep their original bytes.

File discovery follows Pint's finder rules (pun-cli's `finder.rs`), and files run in parallel. A result cache reuses
a file's result from an earlier run with the same key (`crates/pun-cli/src/cache.rs`, PLAN.md §1).

Pun runs no upstream PHP (config.md Build 6). An unported rule in pun.toml is a config error. `pun migrate` names it,
skips it, and asks for a PR to shipfastlabs/pun.

**Blade.** With the `laravel/blade` rule on, `.blade.php` files go through Prettier and prettier-plugin-blade in Node
(Bun in a project with a Bun lock file), as Pint 1.32 does. A few threads format the templates, largest first, each
with one long-lived worker (one per 32 templates, at most `--max-processes`). The other threads format the PHP files.
It needs Node or Bun, and Prettier is most of Pinkary's run time. The dependency probe's worker becomes the first
formatting worker, and Pun stops an unused probe worker on early exits, non-Blade stdin and PHP-only selections.
Workers stay parallel with Node's default memory settings: serializing them or fixing the semi-space size was slower.

**Native Blade (opt-in, 2026-11-23).** `[format] blade = "native"` swaps the Prettier call for crates/pun-blade, a
Rust port of prettier-plugin-blade 3.3 and the Prettier 3.10 parts it uses (doc printer, HTML printer, embeds). Pint's
pre- and post-passes around it are unchanged. The default stays `prettier`. Embedded JS, CSS, JSON, YAML and Markdown
go through oxc's formatters (oxfmt targets Prettier parity), converted from oxc's IR into Prettier docs. oxc's CSS
formatter is vendored in third_party/oxc_formatter_css with Prettier-parity patches (PATCHES.md). Tailwind class
sorting is not ported: native mode leaves class order as written. Parity, byte for byte against Prettier with the Blade
plugin: every golden file outside tests/blade/tailwind (except six that sort classes), and 2,752 real templates from
16 Laravel apps and packages. Known risk: oxc's own Prettier gaps; fix them in vendored copies as they show up.

## 3. The config

The schema is config.md §3: `[format] preset` (`laravel`, `psr12`, `empty`), `not-name` / `not-path`, and
`[format.rules]` as a delta to the preset, with `group/name` keys (§8) and TOML-native options (Decisions, "Option
names"). `pun migrate` translates PHP-CS-Fixer's options. Format rules have no levels: a diff is a diff. They can only
be ignored per file, with `// @pun-ignore-file format/<group>/<name>` before the first statement (rules.md §4).
`pun migrate` rewrites `// @php-cs-fixer-ignore` to this form.

## 4. Risky rules are not format rules

PHP-CS-Fixer marks some fixers "risky" because they can change behaviour. Pint runs them like any other rule. Pun
moves all 76 to `pun lint` (rules.md §1), so `pun format` is always safe on save. Consequences:

- Pint's laravel preset turns on 4 risky rules, and Pinkary's pint.json 10 more. `pun migrate` puts them in
  `[lint.rules]` at `error`. `--fix` applies every enabled fix, so `pun check --fix` still does everything `pint` did.
- So `pun format` alone is **not** Pint-identical on a project that uses risky rules; `pun check --fix` is, and the
  harness proves it.

## 5. Rules: what exists

There are 164 format rules, all native: PHP-CS-Fixer's safe fixers and Pint's Blade rule. The code-deleting fixers
are lint rules (commands.md §4). The 4 Doctrine annotation fixers are unsupported (rules.md O9). The laravel
preset turns on 115 of them and psr12 50. `pun migrate` names and skips a pint.json rule with no Pun rule, such as a
deprecated fixer or a custom fixer class.

## 6. The CLI

`pun format` takes Pint's flags that change what it does (`--test`, `--bail`, `--repair`, `--dirty`,
`--diff=<branch>`, `--config`, `--no-config`, `--preset`, `--format`, `--output-to-file`, `--output-format`,
`--stdin-filename`, `--max-processes`, `--cache-file`, `-v`, `-q`, and `--ansi`/`--no-ansi` as hidden spellings of
`--color`). It adds `--staged`, `--since=<ref>`, `--watch`, `--stdin`, `--explain`, `--color` and `--no-cache`. Pint's
no-op flags (`-p`/`--parallel`, `-n`/`--no-interaction`) are gone, and so is `--blade`: Blade formatting is the
`laravel/blade` rule in pun.toml (Pushpak, 2026-11-23). The human output is Pun's own (§9). Exit codes and
the machine formats (`--format=json|txt|checkstyle|junit|xml|gitlab|github`) stay Pint-compatible. Under a coding
agent it prints compact lines (output.md §3).

## 7. Build

Speed and memory come first (decided).

1. **Presets cut to three.** Done. `--preset per` and `[format] preset = "per"` are a config error (exit 2) naming
   the replacement; `per` stays in `PRESETS` for `pun migrate` only. `symfony` is a config error too, and
   `pun migrate` stops on a pint.json that uses it.
2. **The new output.** Done.
3. **Fewer allocations per token.** Done.
4. **Memory target** and 5. **8-core benchmark in CI:** dropped. No benchmark figures are published (PLAN.md §2,
   2026-11-23).
6. **Pinkary's CI swap:** `vendor/bin/pint` replaced by Pun. *Done when* Pinkary's CI is green on Pun. Status: todo
   (PLAN.md §3).
7. **Port every format rule.** Done: all 164 are native. Follow-ups are rows in PLAN.md §3.

After 1.0: native Blade as the default (it is opt-in now) and a Pun reprinting preset.

## 8. Grouped rule names (approved)

**Shape.** Every rule is `<group>/<name>`, and its full ID is `<command>/<group>/<name>`: `format/imports/ordered`,
`format/phpdoc/align`, `format/laravel/blade`. Biome uses the same idea (`lint/style/useConst`).

**Rules that keep it maintainable:**

1. **Groups are a fixed list in mapping.toml** (`[meta.groups]`), each with a one-line description. Adding a group is
   a deliberate change; `scripts/check-rule-mapping.py` rejects a rule whose group isn't in the list.
2. **Two kinds of group.** *Topic* groups say what part of the code a rule touches. *Ecosystem* groups (`laravel`,
   `phpunit`, `pest`) hold rules that only make sense for one framework or tool. A rule goes in an ecosystem group
   when it's useless without that ecosystem, otherwise in a topic group.
3. **The same group names in all three commands**, so `imports` means the same thing in format, lint and analyse.
4. **The name doesn't repeat the group** (`imports/ordered`, not `imports/ordered-imports`), is kebab-case, and is
   unique within its group. `whitespace/no-blank-line-whitespace` is the one decided exception, where the group word
   is part of a term. Format names describe the construct (`arrays/syntax`); `no-` only when the rule removes something
   (`whitespace/no-trailing`).
5. **No aliases.** Only one name exists per rule; `pun migrate` maps the Pint and PHP-CS-Fixer names onto it. Renames
   are breaking changes, which is fine until 1.0.

**Format groups:** the topic groups `arrays`, `attributes`, `casing`, `casts`, `classes`, `comments`, `control-flow`,
`functions`, `imports`, `operators`, `phpdoc`, `statements`, `strings`, `tags`, `types`, `whitespace`, and the
ecosystem groups `laravel` and `phpunit`.

**Examples:** `imports/ordered`, `phpdoc/align`, `arrays/syntax`, `whitespace/no-trailing`, `casing/keywords`,
`strings/single-quote`, `control-flow/yoda`, `casts/spacing`, `laravel/blade`. Every rule, with its presets, is in
`[rules.format]` in crates/pun-config/rules/mapping.toml and on the docs site under
`docs/content/docs/rules/format/`.

**In pun.toml**, TOML's dotted keys make the group read naturally, and a whole group can be switched off:

```toml
[format.rules]
imports.ordered = { sort = "alphabetical" }
phpdoc.align = false
arrays.syntax = "short"          # rules with one option take the value directly

[format.groups]
phpunit = false                  # every rule in the group
```

**In ignore comments and output**, the full ID: `// @pun-ignore-file format/strings/single-quote`, and
`app/User.php  imports/ordered, imports/no-unused` in a report.

## 9. Pun's own terminal output (approved)

Built in output.md Build 3 (its assistant's calls record where the build differs from the samples below). The style
follows current Rust tools: Biome and oxfmt (a summary line with a time, coloured diffs, code frames for errors) and
Ruff (one line per file, an exact count at the end). No tool's output is copied byte for byte.

**`pun format`** (fixes):
```
 ✔ app/Models/User.php      imports/ordered, imports/no-unused
 ✔ app/Http/Kernel.php      whitespace/no-trailing
Formatted 2 files, 482 unchanged · 484 files in 0.21s
```

**`pun format --test`** (check only, exits 1):
```
 ✖ app/Models/User.php      imports/ordered
 ✖ app/Http/Kernel.php      whitespace/no-trailing
2 of 484 files need formatting · run `pun format` to fix · 0.18s
```

**`-v`** adds a coloured diff under each file, with line numbers and a few lines of context (green added, red removed,
dim context). **Syntax errors** get a code frame like rustc and Biome:
```
 ✖ app/Broken.php:12:1  syntax error, unexpected token "}"
    11 │     return $x
  > 12 │ }
       │ ^
```

**Rules for the look:** green ✔ for fixed, red ✖ for needs-fixing or errors, yellow for warnings, dim grey for
unchanged files and timings; rule IDs in a fixed column; one summary line that always has the count and the time;
nothing for unchanged files unless `-v`. Colour is automatic on a terminal and off when piped, with `NO_COLOR` and
`--color=always|never`. Under a coding agent the compact agent format is unchanged.

**What stays Pint-compatible:** exit codes, and the machine formats `--format=json|checkstyle|junit|gitlab|xml|txt`, so
CI integrations keep working.

**Progress:** a single spinner line with a file counter, only on a terminal and only when the run takes over a
second (output.md §1).

## Decisions

Answered 2026-10-17 (Pushpak):

- **Risky rules are lint rules.** `pun format` is behaviour-safe; `pun check --fix` is what equals `pint`.
- **Grouped names:** every rule is `group/name`, full ID `format/group/name`, designed for maintenance and growth
  (§8). **No aliases**: breaking renames are fine until the release.
- **Options are redesigned for TOML**, not copied from PHP-CS-Fixer. `pun migrate` translates upstream options.
- **Three presets only: `laravel`, `psr12`, `empty`.** `per` is not a preset; its rules stay available one by one,
  and migrate expands a pint.json that uses it. Pun has no `symfony` preset, and migrate stops on a pint.json that
  uses it.
- **No pun.toml: stop**, with "Run `pun migrate`, or `pun init` for a new project".
- **Blade defaults to Node and Prettier.** Native Blade is opt-in with `[format] blade = "native"` (2026-11-23):
  byte parity with Prettier except Tailwind class sorting, which is skipped. oxc's formatters for embedded code.
- **No PHP fallback for unported rules.** Pun never runs PHP-CS-Fixer or Pint. `pun migrate` names each unsupported
  rule, skips it, and asks for a PR:
  ```
  ⚠ phpdoc_tag_casing  not supported by Pun yet; skipped
    Want it? Send a PR to https://github.com/shipfastlabs/pun
  ```
  Custom fixer classes in `.php-cs-fixer.php` are unsupported the same way.
- **A Pun style** (`preset = "pun"`, a whole-file reprint at a line width) is wanted, after 1.0.
- **Pun's own terminal output** (§9), modelled on oxfmt, Biome and Ruff. Exit codes and machine formats stay
  Pint-compatible; scripts/cli-parity.sh and tests/output.rs then cover only those.
- **Speed and memory are a priority now.**
- **Big runs stay parallel** (one worker per core).
- **A result cache** reuses unchanged files' results (Pushpak, 2026-11-15); `--cache-file` and `--no-cache` control it.

**Option names** (assistant's calls where "redesigned for TOML" is silent). The data is
`[options."format/<group>/<name>"]` in crates/pun-config/rules/mapping.toml (141 options of 79 format rules, checked
against the pinned PHP-CS-Fixer's definitions in crates/pun-config/tests/fixer_options.json, exported by
scripts/export-fixer-options.php). The reader and writer are crates/pun-config/src/options.rs.

- **Kebab-case names and values**, like every other pun.toml key: `_` in upstream option names and enum values is
  written as `-`. PHP keywords (`include_once`) and free values (tag names, type names, operators, text) keep their
  spelling.
- **Few renames, one vocabulary:** sort options become `sort`, with `alpha` as `alphabetical`; `imports_order`
  becomes `order`; `null_adjustment` becomes `null-position` with `first` / `last`; `ascend` / `descend` become
  `ascending` / `descending`; `PHPDoc` becomes `phpdoc`. Every other option keeps PHP-CS-Fixer's word, so its docs
  still explain it.
- **`null`, which TOML lacks:** where upstream's `null` means "leave it alone", Pun writes `"ignore"`; where it means
  "follow another option", Pun writes `"same-as-<option>"`. An option whose default is `null` is written by leaving it
  out. No other option takes `null`.
- **Shorthand** (`arrays.syntax = "short"`): a rule with exactly one option takes that option's value directly,
  unless the option is a bool (ambiguous with on/off) or a table (ambiguous with the options table). `pun migrate`
  and `pun config` write the shorthand whenever it applies.
- **Errors:** pun.toml errors are Pun's, and name the Pun option when an upstream name is used. `pun migrate` stops,
  with PHP-CS-Fixer's message (`[array_syntax] Invalid configuration: ...`), on options PHP-CS-Fixer itself rejects,
  because Pint would stop on them too. A value of the wrong type is written as it is, and pun.toml's reader names it.
  PHP's `[]` is read as an empty table for table-valued options.
- Lint rules that are fixers keep PHP-CS-Fixer's option names, kebab-cased; giving them Pun names is a follow-up in
  lint's plan.
