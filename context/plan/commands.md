# Plan: who does what (format, lint, analyse) and `pun check`

Status: **walked through with Pushpak**; answers in "Decisions". Build 1-2 are done, 3-5 are open (§5). Each command
does exactly the job its name promises. This file gives the split, how a rule is placed, and how `pun check` runs.

## 1. Today

Pint and PHP-CS-Fixer, Rector and PHPStan each promise one job and also do parts of the others: CS-Fixer's risky
fixers change behaviour, Rector also does coding style, PHPStan also bans syntax that needs no types. So one idea
often lives in several tools at once; rules.md §2 has the overlaps, with strict comparison as the worked example.
Pun keeps one rule per idea, in one command.

`pun check` runs format and lint per file, then analyse as a separate pass over the same paths
(crates/pun-cli/src/run.rs), and prints the grouped view (output.md Decisions). The one shared parse and the `--fix`
loop of §3 are not built (§5).

## 2. Pun's split: three questions

Each command answers one question and makes one promise.

| Command | Question | Promise | Needs | Edits |
| --- | --- | --- | --- | --- |
| `pun format` | How does the code look? | Never changes what the code does | the file's tokens | always, safe on save |
| `pun lint` | Is there a better way to write this? | Every change is labelled safe or unsafe; `--fix` applies all of them (`fix = "off"` keeps a rule report-only) | one file's syntax tree (types optional, to make more fixes safe) | with `--fix` |
| `pun analyse` | Is this wrong? | Never touches your files | types and symbols from the whole project | never |

`pun check` runs all three in one pass and is what CI and agents run. `pun migrate` only moves config onto pun.toml.

**How a rule is placed** (rules.md §1, the first match wins):
1. It changes only layout and keeps behaviour → **format**.
2. It edits code in any other way, or it reports something one file can decide → **lint**.
3. It needs types or other files to decide → **analyse**.

**What that means per tool:**
- **CS-Fixer's risky fixers are lint.** `pun format` never changes behaviour, so format-on-save is always safe.
  `pun check --fix` is what equals `pint`.
- **Rector's style rules map onto the matching format rule.** The rest of Rector is lint.
- **PHPStan's syntax-only bans are lint**, where no types are needed. PHPStan core's own identifiers stay in analyse
  until 1.0, so `pun analyse` matches `phpstan analyse` from the start.
- **When lint or format can fix what analyse finds, analyse drops it** (Pushpak): PHPStan's `method.unused` is
  `lint/…/no-unused-private-method`, and `pun analyse` does not report it. Analyse keeps only reports nothing fixes.

## 3. How `pun check` works

`pun check` is the one command for CI, pre-commit and agents: lint, format and analyse in one process, reading and
parsing each file once.

**`pun check` (report only):**
1. **Find files** from `[files]` plus each command's own `exclude` (config.md §3), and read the versions from
   composer.json and composer.lock.
2. **Index pass (parallel).** Parse every file once into Mago's syntax tree and collect its classes, functions and
   constants into one project index. Analyse needs this before it can check any file. Not cached: every run parses
   every file.
3. **Check pass (parallel, per file).** On the same parse: run the lint rules, run the format rules on the tokens (no
   writes), and run analyse against the index.
4. **Report, grouped by step** (output.md Decisions): one status line each for format, lint and analyse, with the
   step's problems under it, each tagged with its full rule ID and `[fix:safe]` / `[fix:unsafe]` when lint can fix it.
   When analyse and lint report the same thing, only the lint problem prints.
5. **Summary and exit code.** The exit code is 1 if anything is at `error`, or anything needs formatting.

**`pun check --fix`:**
1. **Lint fixes**: every fix of every enabled rule, except rules with `fix = "off"` (no `--unsafe`). They run in a
   loop until no rule has more to fix, because one fix can expose the next.
2. **Format** the result, so whatever lint inserted gets the project's layout.
3. **Write** each changed file once.
4. **Analyse** the final code, re-indexing only the files that changed, and report what is left. Analyse never edits.
5. **Report** what was fixed per file, then what remains, as in the report-only run.

The order is lint, then format, then analyse, because each step works on the previous step's output. Lint changes code,
format tidies it, and analyse checks what you will actually commit.

**Pint parity.** On a project migrated from pint.json, `pun check --fix` writes the same bytes as `pint`. CS-Fixer's
risky fixers stay token fixers and run inside step 2 in PHP-CS-Fixer's priority order.

**The single commands** stay for editors and for running one step: `pun format` on save, `pun lint --fix`, and
`pun analyse` for PHPStan-style runs.

## 4. Which format rules move to lint

**The test.** Format may only:
- change whitespace, line breaks and indentation;
- change casing;
- change how the same construct is spelled (`array()` to `[]`, quotes, cast spelling, `else if` to `elseif`,
  `\Foo` to an import);
- sort lists whose order means nothing (imports, union types, `implements` lists, PHPDoc tags).

Anything else is lint with a safe fix:
- removing code;
- adding or removing PHPDoc;
- rewriting one construct into another;
- changing a declaration or the order of class members;
- adding test metadata.

**Result:** 40 of the 208 format rules moved to lint (11 of them in the laravel preset, 2 in psr12). Format keeps 168
rules: laravel 115, psr12 50. Their lint names are in rules.md Decisions.

What this means for users:
- `pun format` and format-on-save never delete or restructure code.
- `pun check --fix` still writes Pint's bytes, because these stay token fixers and run in the same priority-ordered
  pass as format.
- `pun migrate` writes the ones a pint.json enables under `[lint.rules]` at `error`, and `--fix` applies them.
- `lint/control-flow/prefer-bare-return` and `lint/complexity/require-consistent-return` contradict each other; both
  are lint, and mapping.toml lists them as a conflict pair.

## 5. Build (`pun check`)

Each step keeps `pun check --fix` equal to Pint on pint.json projects.

1. **The 40 rules move** (config.md Build 2). Done.
2. **Grouped output** (output.md Build 5). Done.
3. **One parse per file:** Mago's tree shared by lint and analyse, tokens for format, an index pass before checking.
4. **Analyse joins** when `pun analyse` exists (analyse.md Build 1): report-only after lint fixes and format.
5. **`--fix` loop:** lint fixes until stable, format, write each file once, re-index changed files, analyse.
   *Done when* `pun check --fix` equals running `pun lint --fix`, `pun format` and `pun analyse` in order on every
   corpus project.

## Decisions

Pushpak:

- **One job per command.** format = how it looks, lint = a better way to write it, analyse = is it wrong. The three
  questions in §2 decide where every rule goes.
- **`pun check` runs lint (with fixes), format and analyse together** (§3).
- **No `pun upgrade` command.** Version upgrades (PHP, Laravel, PHPUnit, Pest) and codemods are lint rules in their
  own groups, named the same way as format rules (`group/name`).
- **`pun migrate` must carry an old config over completely**: pint.json, .php-cs-fixer.php, rector.php and
  phpstan.neon. Anything Pun doesn't support yet is named explicitly to the user, never dropped silently.
- **No levels.** PHPStan's levels don't come over; rules are turned on by name and by presets.
- **Every PHPStan core rule is supported natively.**
- **Fixers that delete or restructure code are lint**, so format only changes layout, casing and spelling (§4).
- **Laravel support is built in.** Nothing to install or import, unlike Larastan or rector-laravel.
- **Modernisation rules** (Rector's PHP sets as `prefer-*` rules) are lint rules, gated by the PHP version.

Assistant's default: **report-only rules stay in lint** (`no-eval`, `no-debug-calls`). Placement goes by what a rule
needs (one file or the whole project), not by whether it fixes; this follows from §2.
