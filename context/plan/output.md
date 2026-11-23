# Plan: terminal output for every command

Status: **walked through with Pushpak**; answers in "Decisions". Build 1-6 are built (`crates/pun-cli/src/render/`).
Every command (format, lint, analyse, check, migrate, init) shares one human output, extended from format.md §9.
Decisions with dates: ../PLAN.md §2. Build list: §4.

**The contract.** Machine formats (`--format=...`) and exit codes match the upstream tool: Pint's
`--format=json|txt|checkstyle|junit|xml|gitlab|github` for format, lint and check, PHPStan's `--error-format` formats
for analyse. The one exception: a run error (Pun couldn't run) exits 2 where Pint exits 1. The human output and the
agent mode output are Pun's own. `scripts/cli-parity.sh` compares the machine output and exit codes with Pint.

## 1. The shared parts

Every command's human output is built from the same four pieces, so learning one command teaches all of them.

**1. File lines** (format, and `--fix` runs): what happened to each file, rule IDs in a fixed column.
```
 ✔ app/Models/User.php        format/imports/ordered, lint/dead-code/no-unused-private-method
```

**2. Problem blocks** (lint and analyse): one block per problem, oxlint and rustc style, with the code it's about.
```
 ✖ lint/laravel/no-debug-calls  dd() left in the code                        fixable
   ╭─ app/Http/Controllers/PostController.php:42:9
 41│     $post = Post::find($id);
 42│     dd($post);
   │     ━━━━━━━━━
   ╰─ help: remove the dd() call, or use Log::debug()

 ⚠ lint/style/prefer-early-return  return early instead of nesting the if
   ╭─ app/Services/Billing.php:17:5
   …

 ✖ analyse/undefined/method  Call to undefined method App\Models\User::fullName()
   ╭─ app/Http/Controllers/ProfileController.php:23:16
 23│     return $user->fullName();
   │                   ━━━━━━━━
   ╰─ help: did you mean `full_name` (a column on users)?
```

**3. The summary line**: always last, always the same order, only non-zero parts.
```
✖ 3 errors · 1 warning · 2 fixable (run pun check --fix) · 2 files need formatting · 412 files in 0.31s
✔ No problems · 412 files in 0.08s
```

**4. Run errors** (bad config, an unknown or unported rule in pun.toml, an unreadable file): one red line, then a
hint. They exit 2.
```
✖ pun.toml:7  unknown rule `lint/style/no-short-ternry`
  did you mean `lint/style/no-short-ternary`?
```

**The look:**
- Red ✖ means error. Yellow ⚠ means warning. Green ✔ means fixed or clean. Rule IDs are cyan, and the rule ID links to
  its docs page in terminals that support links.
- Paths are clickable (`path:line:col`).
- Unchanged files and timings are dim.
- ASCII fallbacks (`x`, `!`, `ok`) apply when the terminal can't show Unicode.
- Colour is on for a terminal and off when piped. `NO_COLOR` and `--color=always|never` override it.
- A spinner with a file counter shows only on a terminal, and only when a run passes one second.

## 2. Per command

| Command | Shows | Exit code |
| --- | --- | --- |
| `pun format` | file lines (✔ formatted), summary | 0 (Pint parity) |
| `pun format --test` | file lines (✖ needs formatting), summary | 1 if any file needs formatting (Pint parity) |
| `pun lint` | problem blocks, summary | 1 if any error |
| `pun lint --fix` | file lines for what was fixed, then problem blocks for what remains, summary | 1 if errors remain |
| `pun analyse` | problem blocks, summary | 1 if any error |
| `pun check` | one status line per step (format, lint, analyse) with its problems under it, one summary | 1 if any error or any file needs formatting |
| `pun migrate` | one line per config moved, then each unsupported item as a ⚠ block, summary | 0 when the rest migrated (unsupported items are named and skipped) |
| `pun init` | init.md §2 | 0 |

Exit code 2 means Pun itself couldn't run: a bad config, an unreadable file, or a crash. That is the same for every
command.

## 3. Agent mode

Under a coding agent every command prints compiler-style lines, not JSON. Detection: the laravel/agent-detector v2
environment list and `/opt/.devin`, then the agent-detector crate; Cursor's `CURSOR_TRACE_ID` alone doesn't count.
`PUN_AGENT=1|0`, `--agent` / `--no-agent` and `PAO_FORCE` / `PAO_DISABLE` override it.

```
pun analyse: fail, 3 errors in 2 files (412 files checked, 0.21s)
app/Models/User.php:42:9 analyse/undefined/method Call to undefined method App\Models\User::foo().
app/Http/Kernel.php:12:1 lint/style/prefer-early-return [fix:safe] Use an early return.
+2 more; run with -v for all
```

Summary line first (a passing run prints only that line); `path:line:col rule message` with an optional
`[fix:safe|unsafe]`; bare diff hunks; no ANSI, progress or boxes; lists capped with `+N more`; errors are one line
(`pun format: error: <message>`); `--format=json` stays available; exit codes don't change. Implemented once in
`crates/pun-cli/src/agent.rs`, pinned per command in `crates/pun-cli/tests/agent.rs`.

## 4. Build

1. **The renderer**: file lines, problem blocks, the summary line, run errors with hints, `--concise`. Done.
2. **The look**: brand colours, `NO_COLOR`, `--color`, ASCII fallbacks, links, the spinner. Done.
3. **`pun format` on the renderer**, and Pint's human screen removed (format.md Build 2). Done.
4. **GitHub annotations** when `GITHUB_ACTIONS` is set; `--format=github` forces them. Done.
5. **`pun check`'s grouped view.** Done.
6. **Exit code 2** for run errors in every command. Done.

## Decisions

Pushpak:

- format.md §9 is the approved starting point. Only lint `error` fails (oxlint); analyse findings are always errors.
- **Problem blocks with code by default**; `--concise` prints one line per problem.
- **`pun check` groups by command**, as Vite+'s `vp check` does. Each step prints one status line, its problems
  follow under it, and the summary line comes last. A passing step is one line. Pushpak compared grouping by file and
  by command and kept by command. The decided shape:
  ```
  ✔ format   All 412 files formatted                         (0.08s)
  ✖ lint     2 errors, 1 warning in 2 files · 2 fixable      (0.12s)
     …problem blocks…
  ✖ analyse  1 error in 1 file                               (0.21s)
     …problem blocks…
  ✖ 3 errors · 1 warning · 2 fixable (run pun check --fix) · 412 files in 0.31s
  ```
  The built view follows Pushpak's brand mockup instead where they differ (see "`pun check`'s grouped view" below).
- **GitHub annotations are automatic** when `GITHUB_ACTIONS` is set, on top of the normal output; `--format=github`
  forces them.
- **Clean files print nothing**; `-v` adds a dim line per clean file (assistant default, approved for format).
- **Pint's human screen is dropped.** Exit codes and machine formats stay Pint- and PHPStan-compatible.
- **Pun's brand colours** from docs/design/tokens.json (Sticker Pop) instead of the terminal's standard colours:
  truecolor when `COLORTERM` is `truecolor` or `24bit`, the nearest 256-colour value otherwise, the standard 16
  colours on basic terminals. The light or dark token variant follows the terminal's background (dark when it can't
  tell).

The assistant's calls where the plan is silent:

**Layout** (Build 1, 3).
- File lines pad the path to 27 characters (or the longest path in the batch plus two spaces), so the rules start in
  one column. Rule IDs are full IDs (`format/arrays/syntax`), in the order the rules ran.
- A problem's `fixable` label starts at character 78 of its header (two spaces after a longer message). An unsafe fix
  reads `fixable (unsafe)`.
- Code frames: one line of context before the span unless that line is blank; no context after. A tab shows as 4
  spaces, and indentation deeper than 4 columns is cut to 4; the cut never passes the span's start column. A blank
  line inside a multi-line span gets no marker. A span over more than 3 lines shows its first two lines, `…`, and its
  last. An empty span, or one at the line end, gets a one-column marker. No help closes the frame with a bare `╰─`.
  Columns count bytes, as `pun_lint::problem::LineIndex` does.
- `--concise`: ` ✖ path:line:col  rule  message  fixable`.
- Summary glyph: ✖ with any error or file needing formatting, ⚠ with only warnings, ✔ `No problems` otherwise.
- `pun format`: file lines (sorted by path, `--fix`'s remaining fixes as ✖ lines), a blank line, the error blocks,
  then the summary. `-v` adds a dim ✔ line per unchanged (or empty) file.
- `pun format`'s summary has no glyph: `Formatted N files, M unchanged · T files in Xs`; under `--test`,
  `N of T files need formatting · run \`pun format\` to fix · Xs` (`needs` for one), or `All T files formatted · Xs`.
  Failed files add a red `E errors` before the `run` hint or the time; a `--test` run with only errors reads
  `E errors · T files in Xs`. The time is dim.
- `-v` diff under each changed file: `   <old> <new> │ <sign><line>`, both numbers right-aligned to the widest one
  shown, blank on the side a line doesn't exist; removed lines red, added green, context dim; `…` between hunks; no
  `---`/`+++` header or `\ No newline` markers.
- Syntax errors use the problem block (rule `syntax-error`, as agent mode names it). The message is PHP's without
  `Parse error: ` and the final ` on line N.`; the span is empty at the first non-blank column of the line PHP names
  (the last line when PHP names the one after it).
- A fixer that produced invalid code, or threw, is a run error naming the file, with the hint "a rule failed on this
  file; please report it at https://github.com/shipfastlabs/pun/issues".
- An abort (an unreadable file) prints the abort box alone, with no progress before it.

**Colour, links and spinner** (Build 2).
- Detection: `--color=always|never` win, then non-empty `NO_COLOR` or `TERM=dumb` turn colour off, then a terminal
  gets colour and a pipe doesn't. 256 colours when `TERM` contains `256`. Light when `COLORFGBG`'s background is 7 or
  9-15. Unicode with a UTF-8 locale (`LC_ALL`, `LC_CTYPE`, `LANG`, first set) and not `TERM=linux`, or in Windows
  Terminal.
- ASCII fallbacks: `x` `!` `ok`, frame `,-` `|` `^` `` `- ``, `...`, and `-` for the summary's `·`.
- Token roles on dark terminals: ✖ `gum`, ✔ `mint`, rule IDs `ok` (light blue), dim `ink-3`, warning `zap`. Light
  terminals use `bad` for ✖, the terminal's green for ✔, `ok` for rule IDs, and SGR 33 for warnings, because the
  tokens have no warning colour that reads on a light ground. At 16 colours: red, yellow, green, cyan and SGR 2
  (dim). 256 colours pick the nearest of xterm 16-255 by RGB distance.
- Links (OSC 8): only on a terminal (or `--color=always`), never under `--color=never`, and only in terminals known to
  support them (iTerm2, WezTerm, VS Code, Ghostty, Hyper, Windows Terminal, Konsole, VTE ≥ 0.50, kitty, Alacritty,
  foot); `FORCE_HYPERLINK` overrides. Paths link to `file://<root>/<path>` (percent-encoded). Rule IDs link to
  `pun_config::RULE_DOCS_URL` (`https://pun.laravel.cloud/docs/rules/`) plus the ID.
- Spinner: on stderr, braille frames (`|/-\` in ASCII), `⠋ 120/412 files` every 80 ms after one second; dropping it
  clears the line. It runs during the file walk for human output only (no `--format`, no `-q`, no agent), and is
  dropped before the report prints.

**Flags** (Build 3).
- `--color` is on `pun format`, `pun lint`, `pun analyse` and `pun check`. `--ansi` / `--no-ansi` are hidden aliases of
  `--color=always|never` (`--ansi` wins when both are given), and they still decorate the machine reports as in Pint.
- `-q` keeps Pint's stderr lines for `pun format` and `pun check` outside agent mode and without `--format`, as Pint
  does. `pun lint` and `pun analyse` print nothing under `-q` (as PHP-CS-Fixer and PHPStan), in agent mode too.
- `pun format --watch` records every file's content after the first run, so late filesystem events for the run's own
  writes don't start a second run.

**GitHub annotations** (Build 4).
- Shape: `::error file=<path>,line=<n>,col=<c>,title=<rule IDs>::<message>`, `::warning …` for a lint `warning`, and
  `::error ::<message>` for a run error. This is PHPStan's `--error-format=github` with three differences: Pun writes
  the problem's column (PHPStan always writes `col=0`; Pun keeps `col=0` for a whole file), adds `title=` (the rule
  IDs, `, ` joined), and escapes per GitHub's workflow-command rules (message: newlines, `@mentions` in backticks,
  `%`, `\r`; property values: `%`, `\r`, `\n`, `:`, `,`), so a path with `,` or `:` and a message with `%` survive.
- What is annotated: under `--test` (and `pun lint` / `pun check` without `--fix`) each file that needs changes, at the
  line of its first differing byte, with `File needs formatting` when only format rules change it and
  `<lint rule IDs> can fix this file` otherwise; `--fix`'s remaining fixes the same way; syntax errors at the problem
  block's place (rule `syntax-error`); a fixer's failure at its file (rule `fixer-error`). Fixed files print nothing.
- Automatic mode: only `GITHUB_ACTIONS=true`, only for human output (no `--format`, `-q` or agent mode). The
  annotations follow the human output on stdout. A run error prints Pun's run error on stderr plus
  `::error ::<message>` on stdout. `--format=github` (also `--output-format=github`) prints only the annotations, as
  plain text.

**Exit code 2** (Build 6).
- Run errors are: config load errors, an unported rule, a bad `--format` / `--output-format`, Node or prettier
  missing for Blade, an `--output-to-file` that can't be written, an unreadable path or file, a malformed
  `@php-cs-fixer-ignore`, a fixer that threw or produced invalid code, and every error of `migrate`, `list-files`,
  `config` and `explain`. Findings exit 1: a file needing formatting, a lint error, a syntax error in the input.
- Run errors print on stderr through `Renderer::run_error`. A message of several lines (Pint's) shows its first line
  as the error and the rest, indented, as the hint. Prettier and Node errors get Pun's own wording naming
  `format/laravel/blade` and the install command for the project's package manager.
- `--format=<machine>` and `-q` runs keep Pint's box (and Pint's yellow prettier warning) on stdout, with exit 2.
  Config errors, which Pint has no box for, are Pun's run error there too. Agent mode keeps `agent::error`.
- `scripts/cli-parity.sh` expects Pint 1 and Pun 2 for the argument sets in its `ABORTS` allow-list, and still
  compares their machine stdout byte for byte.

**`pun check`'s grouped view** (Build 5, matched to Pushpak's brand mockup).
- Step lines start with the status glyph, then a coloured pill (format `zap`, lint `gum`, analyse `lilac`, bold
  `code-bg` ink; bright yellow/magenta/blue backgrounds at 16 colours; the plain padded label without colour). There
  is no per-step time, because the steps don't run separately.
- Problems under a step are two lines, `path:line:col  rule` then the message dim, not code frames. `--concise` puts
  them on one line. Lint fixes without `--fix` show at the first line they change, with the rule's summary.
- The summary has no `N files in Xs` and is bold `zap` (bold only on light terminals).
- Agent mode and every `--format` are the same as for the single commands.
