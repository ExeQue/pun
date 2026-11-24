# Plan: rule docs and migration mapping pages

Status: built (§8). What is left is content (§6).

All pages are generated from [`crates/pun-config/rules/mapping.toml`](../../crates/pun-config/rules/mapping.toml):
1,214 Pun rules (164 format, 680 lint, 370 analyse; 1,191 ported) and 3,215 upstream entries, each pointing at a Pun
rule or carrying a drop or unsupported reason. Prose and examples are fields in mapping.toml too (§7.3).

## 1. Pages and URLs

| Page | URL |
| --- | --- |
| Rules directory | `/docs/rules` |
| Group page | `/docs/rules/<command>/<group>` |
| Rule page | `/docs/rules/<command>/<group>/<name>` (the rule ID is the path) |
| Preset / set page | `/docs/rules/presets/<preset>`, `/docs/rules/sets/<set>` |
| Migration mapping | `/docs/migrate/from/<tool>` (pint, php-cs-fixer, rector, phpstan, larastan, mago) |

Preset URLs include the command (`format-laravel`, `analyse-basic`). Generated folders use Blume's collapsible groups,
so the sidebar does not list every rule at once.

## 2. Rules directory (`/docs/rules`)

- The totals on top, then a grid of the groups with a count and a one-line purpose. Ecosystem groups (laravel, pest,
  phpunit) and upgrade groups (php-8.4, laravel-12) get their own rows.
- One searchable table: `ID | summary | enabled by | fix`. Search matches the Pun ID and every upstream name, so
  pasting `no_alias_functions` or `method.notFound` finds the Pun rule.
- Filter chips, kept in the URL so a view is linkable: command, group, preset/set, fix (safe / unsafe / mixed / none),
  ecosystem, upstream tool, and a "not in any preset" bucket.

## 3. Rule page

In this order:

1. **Title**: the full ID, with a copy button.
2. **Badges as sentences**: "Enabled by the `coding-style` set." / "Fix: unsafe (can change behaviour)." / "Runs when
   Laravel is installed." / "Planned: not in Pun yet." (`ported`, `gated_by`, `intent`).
3. **Replaces**: every upstream name, linked to its upstream docs. Merged rules list every source.
4. **Turn it on**: the `pun.toml` snippet and the ignore comment (`// @pun-ignore ...`).
5. **What it does / why**: the summary, then a short paragraph.
6. **Examples**: Avoid / Prefer, or a diff for fixing rules. Analyse rules show the code and Pun's exact message.
7. **Options** (format rules): Pun name, type, allowed values, default, upstream option name.
8. **Linked rules**: `fixed_by` both ways and `[meta] conflicts`.
9. **Footer**: other rules in the group, source file, "Edit this page".

## 4. Group and preset/set pages

- Group page: the description, counts per command, then the rules as compact cards. One group page per command,
  since `style` means different things in format and lint.
- Preset/set page: what it is for, every rule it turns on with the config it applies, then the rules it leaves off.
  Analyse presets also say which PHPStan levels they cover and what each level adds.

## 5. Migration mapping (`/docs/migrate/from/<tool>`)

One page per upstream tool, linked from `pun migrate`'s page. Each page has:

1. **Config translation**: where the user's existing keys go (`pint.json` `preset` -> `[format] preset`, PHPStan
   `level: 5` -> `recommended` plus on/off lines, `ignoreErrors` -> `[[analyse.ignore]]`, Rector prepared sets ->
   lint sets, Larastan neon booleans).
2. **Rule table**: `upstream name (linked) | Pun rule (linked) | notes`. Notes say `same`, `merged with X`,
   `options carried over / dropped`, or the drop/unsupported reason. Searchable, with an "unsupported only" filter.
3. **Unsupported**, grouped by reason, each ending with "send a PR" (Pun has no PHP fallback).
4. **Pun-only rules** (`pun_only`).

PHPStan is mapped at identifier level (what users write in `ignoreErrors` and `@phpstan-ignore`), with pattern
identifiers (`%s.alwaysFalse`) expanded.

## 6. Content gaps (what mapping.toml doesn't hold yet)

- 361 of the 370 analyse rules have no `summary`. Format and lint rules all have one.
- No rule has examples, and only `format/casts/no-short-bool` has a "why" paragraph. Sources: format from the
  fixtures in `crates/pun-fixers/tests`, lint from oracle runs rewritten in Pun's words, analyse from oracle runs.
- Options carry types but no per-option prose.

These are separate rows in PLAN.md §3. Generation does not invent placeholder prose.

## 7. Decisions

1. Every rule gets a page; unported ones are marked Planned.
2. Pages are generated MDX: a script writes one `.mdx` per rule from mapping.toml into `docs/content/docs/rules/`, and
   a check fails when they are out of sync.
3. Examples and prose live in mapping.toml, as fields on each rule (why, examples).
4. One mapping page per tool under `/docs/migrate/from/`, plus a short overview page.
5. Filters ship at launch: one table of every rule, with search across upstream names and URL-kept filter chips.

## 8. Build

| Step | Status | Verification |
| --- | --- | --- |
| 1. Generator and folder metadata | built | `python3 scripts/gen-rule-docs.py --check`, run beside the mapping check in CI |
| 2. Rule page and content schema | built | `python3 scripts/test-rule-docs.py` |
| 3. Groups, presets and sets | built | Every command group, preset and set has a page |
| 4. Migration mapping | built | Six tool pages and the overview |
| 5. Rules directory | built | Generated JSON with every upstream spelling; URL-kept filters checked in a browser |
| 6. CLI links | built | `pun_config::RULE_DOCS_URL` (`https://punphp.com/docs/rules/`) plus the rule ID, in `pun explain`'s `docs` row and the terminal hyperlink on each rule ID; `deployment.site` in `docs/blume.config.ts` is the same origin |
