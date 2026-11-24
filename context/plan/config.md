# Plan: config, rule names and presets (pun.toml)

Status: **built** (Build 1-9). This file holds the one pun.toml schema that format.md, lint.md, analyse.md, init.md
and commands.md use. Decisions with dates: ../PLAN.md §2. Rule placement, naming style and ignore comments: rules.md;
the data: crates/pun-config/rules/mapping.toml.

## 1. What it is

pun.toml is the only config Pun reads. Every command stops without it: "No pun.toml found. Run `pun migrate`, or
`pun init` for a new project." `pun migrate` writes it from old configs (plan/migrate.md) and `pun init` writes it for
a new project (plan/init.md). `pun config` prints the fully resolved config, every rule included.

## 2. Today

- `pun_config::load` (crates/pun-config/src/lib.rs, with the sections in schema.rs) reads the §3 schema into
  `Config.files`, `.lint`, `.analyse` and `.extensions`, with the errors in Decisions pinned in
  crates/pun-config/src/tests.rs.
- Every rule in mapping.toml has one `<command>/<group>/<name>` ID, a group from the fixed list, and its lint `sets`
  or analyse `preset` tags. `scripts/check-rule-mapping.py` rejects unknown groups and duplicate names.
- pun-cli's finder applies `[files]` and each command's `exclude`; `pun list-files`, `pun config` and
  `pun explain` are built.
- The problem and fix type that lint, analyse and the renderer share is `pun_lint::problem`.

## 3. The decided schema

```toml
[php]
version = "8.4"                       # override only; the default comes from composer.json's require.php

[files]                               # shared by every command, like Mago's [source]
paths = ["app", "tests"]              # what to check when no paths are given
exclude = ["build", "storage/**"]

[extensions]                          # laravel | pest | phpunit; each is on only when listed "on" (extensions.md)
laravel = "on"

[format]
preset = "laravel"                    # laravel | psr12 | empty
exclude = ["app/Generated/**"]        # added to [files] exclude, for format only
not-path = ["app/Legacy.php"]         # Pint's notPath (no notName: PLAN.md §2, 2026-11-23)

[format.rules]                        # only what differs from the preset
"imports/ordered" = { sort = "alphabetical" }
"arrays/syntax" = false

[lint]
preset = "recommended"               # Pun's ladder; see §5

[lint.groups]                         # a level for a whole group
style = "warn"
laravel-13 = "error"                  # turn an upgrade on

[lint.rules]                          # a rule's own line wins over its group
"laravel/no-debug-calls" = "error"
"style/prefer-early-return" = { level = "warn", fix = "off" }
"types/no-loose-comparison" = "off"

[[lint.overrides]]
files = ["tests/**"]
rules = { "laravel/no-debug-calls" = "off" }

[analyse]
preset = "recommended"                # basic | essential | recommended | strict | max
exclude = ["tests/**"]                # added to [files] exclude; not analysed and not scanned for symbols
baseline = "pun-baseline.toml"

[analyse.rules]
"types/missing-iterable-value-type" = "off"
"undefined/method" = { magic = true } # options in a table mean "on, with these options"
```

`pun init` writes only `[format] preset`, `[lint] preset` and `[analyse] preset`. Everything else is optional.

**Files.** `[files] paths` and `exclude` apply to every command; each command's own `exclude` is added on top, so
format, lint and analyse can see different file sets. `vendor/` is never checked, but analyse reads it for symbols.
`pun list-files` shows the result (§7).

## 4. Rule names and groups

- **Shape:** `<command>/<group>/<name>` (`format/imports/ordered`, `lint/dead-code/no-unused-private-method`,
  `analyse/undefined/method`). Inside a `[<command>.rules]` table the command part is implied.
- **One group list for all three commands**, fixed in mapping.toml with a one-line description each. Topic groups
  (`imports`, `arrays`, `phpdoc`, `types`, ...) and ecosystem groups (`laravel`, `pest`, `phpunit`). Upgrade groups
  are one per target (`laravel-13`, `php-8.5`). The format table is format.md §8; lint and analyse follow the same
  rules.
- **No aliases.** Old Pun names, PHP-CS-Fixer names and PHPStan identifiers in a `rules` table are an error that names
  the Pun ID to use. Only `pun migrate` reads upstream names.
- Intent (correctness, suspicious, style) and fix safety stay as data in mapping.toml, shown in output and docs.

## 5. Presets, sets and levels

- **Format presets:** `laravel`, `psr12`, `empty`. There is no `per` or `symfony` preset; their rules stay available
  one by one, and migrate expands a pint.json that used them into explicit rules.
- **Lint presets and Rector sets** (2026-11-24, lint.md §8): `[lint] preset` is `essential`, `recommended` or
  `strict`, each including the one below; `[lint] sets` takes Rector's sets exactly (`rector/dead-code`,
  `rector/php-sets`, `rector-laravel/laravel-code-quality`, ...). An extension that is on lets its rules in the preset
  or sets run. Rules in neither are opt-in by name or group.
- **Lint levels, as oxlint:** `"off"`, `"warn"`, `"error"` per rule or group. A rule enabled by a set is `error` unless
  set to `warn`. Only `error` fails. `--deny-warnings` makes warnings fail, `--max-warnings <n>` fails above n.
  `-A` / `-W` / `-D <rule or group>` set a level for one run. `[[lint.overrides]]` changes levels per path.
- **Lint fixes:** `--fix` applies every fix of every enabled rule. `fix = "off"` keeps a rule report-only.
- **Analyse presets** by PHPStan level band, each including the one below: `basic` (0-1), `essential` (2-3),
  `recommended` (4-5, default), `strict` (6-8), `max` (9-10). The four type switches (`union-types`, `nullables`,
  `explicit-mixed`, `implicit-mixed`) come with the presets. Analyse rules are on or off; there is no `warn`.
- **PHPStan level presets:** `phpstan/level_0` to `phpstan/level_10` and `phpstan/level_max` run exactly PHPStan's
  level (analyse.md §3), including the lint rules that are PHPStan checks.
- **Ecosystems:** Laravel, Pest and PHPUnit turn on only through `[extensions]`; their versions in composer.lock gate
  the version groups. A project switches one's rules off with its group (`[lint.groups] laravel = "off"`,
  `[analyse.rules]` the same way).

## 6. Build

Each step ends with the existing gates green (`cargo test -p punphp-config`, the corpus harness at 0 differing files).

1. **New names in mapping.toml.** Done.
2. **Move the 40 code-deleting format rules to lint** (commands.md §4). Done.
3. **The new pun.toml schema in pun-config.** Done.
4. **TOML-native option names** for every format rule. Done.
5. **composer.json and composer.lock detection.** Done.
6. **Remove delegate mode.** Done.
7. **`[files]` and per-command `exclude`** in the finder, and **`pun list-files`** (§7). Done.
8. **`pun config`** and **`pun explain <rule>`**. Done.
9. **The shared problem type.** Done.

## 7. `pun list-files` (Pushpak, after Mago's `list-files`)

Prints the files Pun would check with the current pun.toml, one per line, so you can see that `paths` and `exclude`
do what you think.

```
pun list-files [--command format|lint|analyse] [-0] [paths...]
```

- Without `--command`: the `[files]` set (paths minus the shared excludes). With `--command analyse`: that command's
  set, with its own `exclude` applied. `--command format` also applies Pint's finder rules (not-path,
  `.blade.php` only with the Blade rule on).
- `-0` / `--zero-terminate` ends each name with a NUL byte for `xargs -0`.
- Paths are relative to the project root and sorted. Files analyse only reads for symbols (`vendor/`) are not listed.
- Agent mode prints `pun list-files: N files[, <command> set] (Ts)`, then the full, uncapped list. `-0` and
  `--format=json` (an array; `json` is the only format) print no summary. `--config` works as on every command.
- Example: with `[files] paths = ["app", "tests"]`, `exclude = ["storage/**"]` and `[analyse] exclude = ["tests/**"]`,
  `pun list-files` lists app and tests, and `pun list-files --command analyse` lists only app.

## Decisions

Pushpak (../PLAN.md §2): pun.toml only, and stop without it; `group/name` with no aliases; three format presets;
lint sets; oxlint levels; `--fix` applies all, `fix = "off"`; five analyse presets with `max` on top; analyse on or
off; ecosystems on only through `[extensions]`; PHP version from composer.json; no PHP fallback; `pun list-files`;
the shared problem type. The `[files]` section with per-command `exclude` and the `[analyse] preset` key (instead of
a top-level `preset`) are the assistant's defaults.

The assistant's calls where the plan is silent:

**Values and precedence** (Build 3).
- A lint rule is `"error"`, `"warn"` or `"off"`, or a table with `level` (default `error`), `fix = "off"` and the
  rule's options. `true` / `false` are an error naming `"error"` / `"off"`. A lint group takes a level only.
- An analyse rule is `"on"`, `"off"` or a table of options (on). Every group takes `"on"` / `"off"` in
  `[analyse.rules]`.
- Options of a rule a fixer runs reach it with snake_case top-level keys; other rules keep them as written. Format
  option names, `null` spellings, the one-option shorthand and their errors are in format.md Decisions ("Option
  names"). Lint rules that are fixers keep PHP-CS-Fixer's option names, kebab-cased, until lint's own option pass.
- Set (at `error`) < group < the rule's own line.
- Only what a set or preset turns on is gated. An ecosystem group's rules need the ecosystem on (`"on"` in
  `[extensions]` and its group not switched off). A `php-X.Y` rule needs `[php] version` >= X.Y. A `laravel-N` rule
  needs Laravel on and a locked version >= N; a locked branch has no version, so it passes no gate. A group's level or
  a rule's own line turns rules on whatever the gates. `doctrine` is never detected.
- `[php] version` must be `major.minor`. Without it, Pun takes the lowest version composer.json's `require.php`
  admits, else 8.3 (the floor). The lock is read on every load.

**Errors** (Build 3, 6). Config errors exit 2 (output.md).
- Removed keys are errors that name the replacement: the top-level `preset`, `[format] preset = "per" | "symfony"`,
  `fix = "apply" | "safe-only"`, `"info"`, `severity`, `[format] paths` (names `[files] paths`), and `[analyse] level`
  or a number as `[analyse] preset` (names `phpstan/level_<n>` and Pun's preset that holds that level).
- Rules Pun can't run: a rule a set or group turns on is skipped. A rule named in pun.toml or set by the preset stops
  every `format`, `lint` and `check` run (so a lint rule stops `pun format` too) with
  `<pun.toml path>: rule(s) not supported by Pun yet: `<Pun ID>`, ...`. The upstream name stands in when the rule has
  no Pun ID (such as `@PER-CS`), and `preset <name>` replaces the path when no pun.toml was read.
- The check runs in pun-cli against the real fixer registry (`pun_fixers::all`), not mapping.toml's `ported` flag.

**composer.json and composer.lock** (Build 5).
- `pun_config::composer::detect(root)` reads only composer.json and composer.lock. It returns the `require.php`
  constraint with its lowest admitted version (`php_version()` gives `major.minor`), and the locked Laravel
  (`laravel/framework`), Pest (`pestphp/pest`) and PHPUnit (`phpunit/phpunit`) versions, each marked `dev` when it
  comes from `packages-dev`.
- Only the lock shows that a project uses an ecosystem (what `pun migrate`, `pun init` and `pun extension list` read);
  there is no fallback to `require`. `laravel/laravel` and `illuminate/*` are not the framework.
- A locked branch (`dev-main`, `3.x-dev`) keeps its raw version with no numbers. A pre-release (`5.0.0-RC1`) counts
  as its release. A constraint that admits nothing (`>=8.4 <8.3`) gives no PHP version. `>8.4` gives 8.4.1.

**Migrate's writer** (kept minimal for migrate Build 1).
- Risky and code-deleting fixers are written at `"error"` (a table of options without `level` when they have
  options). The moved laravel-preset rules run in the same priority-ordered token pass, so Pint parity holds.
- Pint's `in` becomes `[files] paths`. A `per` or `symfony` preset becomes `empty` plus its rules, with their sets
  listed as unsupported. The starter has no top-level `preset`.
- `pun migrate` reads a `.php-cs-fixer.php` through the project's own PHP-CS-Fixer (one `php -r` run,
  crates/pun-cli/src/php_cs_fixer_rules.php), because the config is PHP code. No rule ever runs in PHP. A static read
  is a follow-up for migrate Build 1.
- The lint names of the 40 moved rules, the added groups, and how `sets` and `preset` were derived are in rules.md
  Decisions.

**Files and globs** (Build 7).
- An `exclude` entry with `*`, `?`, `[` or `{` is a glob matched against the root-relative path (`*` crosses `/`), as
  in Mago. Any other entry is a root-relative path prefix.
- For format, plain entries in `[files] exclude` and `[format] exclude` keep Pint's meaning (a folder name at any
  depth, or path segments under each `in` dir), so migrated pint.json excludes and the Pint corpus are unchanged. Only
  glob entries take the glob meaning there (also for `--stdin-filename`). A pint.json exclude with a glob character
  means a glob, not a literal folder name.
- The `[files]` set (no `--command`, `lint`, `analyse`): `*.php` files (`.blade.php` included), no `vendor` folder at
  any depth, no dot files or VCS folders, symlinked folders not followed. Pint's own skips (`storage`, `build`,
  `node_modules`, `bootstrap/cache`, IDE helpers, Blade without the rule, not-path) apply to format only.
- Path arguments replace `[files] paths` (and `[analyse] paths`). Outside format a pun.toml exclude also drops an
  explicitly named file. Format keeps Pint's rules: an explicit file bypasses every filter, and with `in` set the
  arguments replace the whole finder.
- A missing `[files] paths` entry fails with Pint's `The "x" directory does not exist.`, a missing argument with
  `The path "x" is not readable.`. A `[files] paths` entry may name a file.
- `[lint] exclude` applies to `pun lint` and the lint half of `pun check`. `[analyse] paths` replaces `[files] paths`
  for analyse.

**`pun config`** (Build 8).
- It prints pun.toml's own sections and keys: `[php] version`, `[files]`, `[extensions]` (every built-in extension,
  `"on"` or `"off"`), `[format]` (preset, exclude, not-path, rules), `[lint]` (sets, exclude, rules,
  `[[lint.overrides]]`) and `[analyse]` (preset, paths, exclude, baseline, rules).
- Rules are keyed `<group>/<name>` under their command's table, as `pun migrate` writes them; a rule with no Pun ID
  keeps its upstream name. Groups and sets are expanded into the rules they turn on, so `[lint.groups]` is not printed.
  Each command's `exclude` is the full list it applies.
- Two resolved-only keys: `ecosystems` (lint and analyse) and `[analyse] switches`. The TOML starts with
  `# Resolved from <files>.` (no comment when nothing was read). Agent mode keeps its summary line.

**`pun explain <rule>`** (Build 8).
- It takes a full ID or `<group>/<name>` (no two commands share one today), then an upstream key as written in any
  `[upstream.<tool>]`, then the loose match pun.toml's errors use (other spellings, old `format.` / `lint.` names).
  Anything but an ID starts with "`<name>` is `<id>` in Pun." and exits 0.
- An upstream key with no rule is an error with its `drop` / `unsupported` reason, or naming its non-rule target
  (PHPStan parameters such as `checkUnionTypes`). An unknown name is an error. Errors exit 2.
- The text: the ID, the summary line when mapping.toml has one, then rows `group` (with the `[meta.groups]` line),
  `presets` / `sets` / `preset` by command (`none (opt-in by name or group)` when empty), `intent` when set, `fix`
  (`none` without one), `fixed by` for analyse rules that have it, `upstream` (every `[upstream.*]` row mapping to the
  ID, as `name (tool)` in file order; `none (Pun's own rule)`), and `docs`.
- `docs` is the rule's page, `pun_config::RULE_DOCS_URL` followed by the ID
  (`https://pun.laravel.cloud/docs/rules/format/arrays/syntax`).

**The problem type** (Build 9).
- Columns count bytes, 1-based, as PHP's `strlen` does: `é` advances the column by 2. Lines are 1-based and end at
  `\n`, `\r\n` or a lone `\r`, as in PHP's scanner. An offset past the end is clamped to the end.
- Fixes apply in the order given, so the caller's order is the priority. A fix applies whole or not at all. It is
  deferred to the next round when one of its edits overlaps an edit already applied, or is an insertion at or inside
  one (the order of the inserted text would be ambiguous). Adjacent replacements both apply. A fix whose edits are out
  of range, split a UTF-8 character or overlap each other is invalid and never applies.
- Levels serialize as `error` / `warning`, safety as `safe` / `unsafe`. The rule ID is the full
  `<command>/<group>/<name>` string and the path is relative to the project root.
