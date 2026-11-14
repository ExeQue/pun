---
name: pun-development
description: "Use when you format, lint or type-check PHP code, fix a Pun error, change pun.toml, or add, remove or update a Composer package in a project that uses Pun. Activate when the user mentions Pun, pun.toml, pun.lock, punx, Pint, PHP-CS-Fixer, Rector, PHPStan or Larastan. Do not use for JavaScript tooling."
license: MIT
metadata:
  author: punphp
---
# Pun

Pun is one binary that replaces Pint and PHP-CS-Fixer (`format`), Rector (`lint`), PHPStan and Larastan (`analyse`), and Composer's install commands. Run it as `vendor/bin/pun`, or as `pun` when it is on the `PATH`. Under a coding agent, Pun prints a short summary line and one line per finding.

## Check your changes

| Task | Command |
| --- | --- |
| Fix and check the changed files | `pun check --fix --dirty` |
| Check without editing (CI) | `pun check` |
| Format only | `pun format` (`--test` checks without editing) |
| Lint, with fixes | `pun lint --fix` |
| Type errors only | `pun analyse` |
| Files a command covers | `pun list-files` |

`check` exits 0 when nothing is left to fix, 1 when it finds a problem, and 2 when Pun cannot run (for example, an invalid `pun.toml`).

## Configuration

`pun.toml` holds the `[format]`, `[lint]` and `[analyse]` rules and the project's dependencies.

- `pun config` prints the resolved configuration.
- `pun explain <rule>` prints what a rule checks, whether its fix is safe, and its upstream name.
- `pun migrate` writes `pun.toml` from `pint.json`, `.php-cs-fixer.php`, `rector.php`, `phpstan.neon` and `composer.json`. It leaves those files in place.

Change a rule in `pun.toml`, not in `pint.json` or `phpstan.neon`: Pun reads only `pun.toml`.

## Dependencies

| Task | Command |
| --- | --- |
| Install the locked versions | `pun install` |
| Add a package | `pun require vendor/package` (`--dev` for require-dev) |
| Remove a package | `pun remove vendor/package` |
| Update packages | `pun update` (or `pun update vendor/package`) |
| Run a package's command without adding it | `pun x vendor/package` |
| Check the supply-chain reviews | `pun vet` |

These commands write `pun.toml` and `pun.lock`. Do not run `composer require` or `composer update` in a Pun project.
