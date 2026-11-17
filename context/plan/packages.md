# Plan: the package manager (`pun install`, `pun update`, `pun require`, `pun remove`)

Status: **`pun install` from a lock is built** (2026-11-14): composer.json and composer.lock (§3), the
autoload dump, `vendor/bin`, scripts and the pest-plugin port. Since 2026-11-17 it runs Composer's install-time
platform check ("Verifying lock file contents can be installed on current platform.", `verify.rs`) with Composer's
problem texts. On pinkary every
file in `vendor/` matches Composer 2.11.2's, modes included. **The resolver and `pun update`, `pun require` and
`pun remove` are built** (steps 8 and 9, 2026-11-14): a port of Composer's pool builder and SAT solver that passes
158 of Composer's installer fixtures (102 of them compared on the whole composer.lock), and on pinkary writes
Composer's exact lock in 1.8 s against Composer's 9.4 s (`context/reports/scoreboard.md`). Left of steps 8 and 9: the other corpus projects, the 20
recorded partial updates, `vcs` and `artifact` repositories, and the root `filter.summary-url`. Not built from steps 1-7: the
`.oracle/composer` phar and `scripts/install-parity.sh` (the tests' expectations come from Composer 2.11.2 runs),
and `pun composer` and its commands (`validate`, `dump-autoload`, `run-script`). **The plugin host is built for the
script events** (step 7, 2026-11-15): every allowed plugin other than the native `pestphp/pest-plugin` and
`php-http/discovery` runs on Composer 2.11.2's classes, activated by its PluginManager, with its listeners around the
scripts in Composer's priority order; a fixture plugin logs the same events as under Composer on install, `--no-scripts`,
`--no-plugins` and an upgrade mid-command. Left of step 7: the package, installer, pool and download events, custom
installer paths, plugin commands, the allow-plugins prompt, and the native ports of extension-installer and
phpcodesniffer-composer-installer. `pun x`,
`pun vet` and workspaces are built (§7). Decisions with dates: §6.
Beyond Composer: §7. Open questions: §8.

## 1. What it is

Pun installs a project's dependencies the way Composer does, much faster. It reads composer.json, reads and writes
composer.lock, and fills `vendor/` with the same files `composer install` writes, so `composer` and `pun` work on the
same project side by side (§6). The proof is a parity
harness: on every corpus project, `pun install` and `composer install` give byte-identical `vendor/` trees from the
same lock, and `pun update` writes the lock `composer update` writes.

The everyday commands are top-level, with Composer's names and flags:

| Command | Composer | What it does |
| --- | --- | --- |
| `pun install` | `composer install` | Installs composer.lock (resolves and writes it when there is none) |
| `pun update [packages]` | `composer update` | Resolves again, writes composer.lock, installs |
| `pun require <pkg[:constraint]>` | `composer require` | Adds to composer.json, then a partial update |
| `pun remove <pkg>` | `composer remove` | Removes from composer.json, then a partial update |
| `pun composer <command>` | `composer <command>` | Every other Composer command: `dump-autoload`, `show`, `outdated`, `why`, `why-not`, `audit`, `validate`, `licenses`, `run-script`, `check-platform-reqs`, `config`, ... (not built yet) |

`pun composer install` and `pun install` are the same command. A `pun composer` command Pun has not built yet is a
run error (exit 2) that names it; Pun does not shell out to the Composer CLI.

## 2. What we learnt from phast and Composer

- **phast** (github.com/phast-sh/phast, Rust, MIT or Apache-2.0, 9 crates, about 45k lines) is the speed reference:
  parallel downloads and extraction, a PubGrub resolver, a TOML manifest and lock of its own (`phast migrate` converts
  `composer.json` once and never reads it again), its own autoloader under `vendor/phast/` with a
  `\Composer\InstalledVersions` shim, a scripts runtime with a `\Composer\Script\Event` shim, and no Composer plugins.
  Pun keeps Composer's files instead, so phast's formats, resolver and autoloader are not what Pun ships. phast is a
  benchmark and a design reference only: no phast code is copied into Pun (§6).
- **Composer** (2.x, MIT, about 79k lines of PHP) is the oracle. The parts Pun ports: `DependencyResolver` (8k lines:
  Pool, PoolBuilder, PoolOptimizer, RuleSetGenerator, Solver, DefaultPolicy, Transaction), `Autoload` (2k),
  `Installer` and `Downloader` (7k), the parts of `Repository` and `Package` that read Packagist metadata and write the
  lock, and `composer/semver`. Its 231 installer fixtures (`tests/Composer/Test/Fixtures/installer/*.test`: a
  composer.json, an optional lock and installed repo, the expected operations and lock) and its autoload fixtures are
  the test source, exported to JSON as the fixer fixtures are.
- **Real projects need plugins and scripts.** Every Laravel corpus project allows `pestphp/pest-plugin` and
  `php-http/discovery`; dev-null.sh adds `laravel/vet`; larastan allows `dealerdirect/phpcodesniffer-composer-installer`.
  `post-autoload-dump` runs `Illuminate\Foundation\ComposerScripts::postAutoloadDump` (a PHP callback that takes a
  Composer `Event`) and `@php artisan package:discover`, which reads `vendor/composer/installed.json`. Without them a
  Laravel app is broken after install.

## 3. The decided behaviour

- **composer.json and composer.lock hold the dependencies**, as package.json and bun.lock do for Bun (Pushpak,
  2026-11-15, reversing the pun.toml/pun.lock move of 2026-11-14, §6). The package commands read composer.json and
  write composer.lock byte for byte as Composer 2.11.2 does; `require` and `remove` edit composer.json in place with a
  port of Composer's `JsonManipulator`. pun.toml holds only Pun's own settings: `[workspace]` and the tooling
  sections, as bunfig.toml holds only Bun's. A composer.json section in pun.toml (`[require]`, `[autoload]`, ...) is a
  config error (exit 2) that says it belongs in composer.json. `pun migrate` does not touch composer.json or
  composer.lock.
- **`vendor/` is Composer's layout:** package directories, `vendor/composer/` (`installed.json`, `installed.php`,
  `InstalledVersions.php`, `ClassLoader.php`, `autoload_*.php`, `platform_check.php`, `LICENSE`), `vendor/autoload.php`
  and the `vendor/bin` proxies, all byte-identical to Composer 2's. `ClassLoader.php` and `InstalledVersions.php` are
  Composer's own files, embedded with its MIT notice.
- **The same resolution as Composer.** The solver is a function-by-function port of Composer's SAT solver and policy,
  so `pun update` picks the versions Composer picks, including on conflicts. PubGrub explains conflicts better; Pun's
  conflict message is its own human output over the same problem set (output.md).
- **Composer's environment:** `COMPOSER_HOME`, `COMPOSER_CACHE_DIR` (the same cache, so Pun and Composer share
  downloads and metadata), `auth.json` (project and home) and `COMPOSER_AUTH`, `config.platform`, `minimum-stability`,
  `prefer-stable`, `preferred-install`, `vendor-dir`, `bin-dir`, `optimize-autoloader`, `classmap-authoritative`,
  `apcu-autoloader`, `sort-packages`, `allow-plugins`, `process-timeout`, `audit` and `policy` settings.
- **Repositories:** Packagist (the v2 metadata API), any Composer repository (Private Packagist, Satis), `path`,
  `package`, `vcs`/`git`/`github`/`gitlab` (through the host's API and git), and `artifact`. Not `svn`, `hg` or `fossil`.
- **The platform is the system PHP's.** Pun asks the `php` on the PATH once for its version, extensions and their
  versions, as Composer sees them, unless `config.platform` says otherwise. Scripts run on that PHP too.
- **Scripts and events run as Composer runs them:** shell commands, `@php`, `@composer` (which runs Pun's own
  command), `@putenv`, script references, and PHP callbacks. A callback gets a `Composer\Script\Event` from a small
  bundled PHP shim (Event, Composer, Config, IOInterface, the package objects a callback reads), enough for Laravel's
  `ComposerScripts`, Pest's and the common callbacks; the shim's API is tested against the corpus.
- **Composer plugins work.** An allowed plugin runs as it runs under Composer, through a plugin host:
  - The host is a PHP process on the system PHP running Composer's own classes (a pinned `composer.phar`, MIT,
    downloaded once with a checksum into Composer's cache, not bundled into the binary). It builds a real `Composer`
    object from the files Pun has written (`composer.json`, `composer.lock`, `vendor/composer/installed.json`),
    activates the allowed plugins through Composer's PluginManager, and dispatches the events Pun sends it.
  - Pun and the host talk over a pipe in JSON. Pun fires every event Composer fires, in Composer's order: the script
    events, the package events (`pre-package-install` and the rest), the installer events (`pre-operations-exec`), and
    the plugin events (`pre-pool-create`, `pre-file-download`, `post-file-download`, `command`, `init`). Where a
    plugin changes Pun's work (a pool filter like laravel/vet's, a custom installer's install path like
    composer/installers', a gated operation, a rewritten download URL), the host sends the change back and Pun applies
    it.
  - A plugin's commands (`Capable` with a CommandProvider, like Pest's `pest:dump-plugins`) run through the host as
    `pun composer <command>`.
  - The host starts only when an allowed plugin subscribes to an event the command fires, and once per command.
  - The plugins every Laravel project has get native ports that write the same files without PHP, so the common
    install starts no host: `pestphp/pest-plugin`, `php-http/discovery`, `phpstan/extension-installer`,
    `dealerdirect/phpcodesniffer-composer-installer`. A port is used only when its parity test against the plugin
    passes for that plugin version; otherwise the host runs the plugin.
  - A plugin `allow-plugins` denies, or that it does not list, is handled as Composer handles it (skipped, or the
    interactive prompt, or the non-interactive error).
- **Output:** machine formats (`--format=json` of `show`, `outdated`, `audit`, `licenses`), exit codes and the files
  Pun writes match Composer. A run error exits 2 where Composer exits 1 (output.md). The human output is Pun's own,
  with phases and timings (`Resolved 211 packages in 0.14s`), and agent mode is compact as for the other commands.
- **Speed is the point:** parallel HTTP/2 metadata and dist downloads, parallel extraction, Composer's shared cache,
  a no-op `pun install` that only checks the lock against `installed.json`, and classmap scanning with Pun's own lexer.
  `scripts/bench.py` gains `BENCH=install` (cold cache, warm cache, no-op) against Composer and phast.

## 4. Crates

Built as one crate, not the three planned (`pun-semver`, `pun-composer`, `pun-fetch`):

- `pun-packages`: composer/semver's version parser and constraints (`version.rs`, `constraint.rs`), the lock
  (`lock.rs`), the resolver (`resolver/`: Composer repositories and Packagist's v2 metadata, `package` and `path`
  repositories, PoolBuilder, PoolOptimizer, Solver, DefaultPolicy, the problem reports), downloads through
  Composer's shared cache and auth over ureq (`fetch.rs`), the installed repository, the autoload dump and class
  map, bin proxies, scripts and events, the native plugin ports (`plugins.rs`), and the manifest edits of `require`
  and `remove` (`manifest_edit.rs`). Its resolver and installer tests run on recorded metadata (`tests/fixtures`).
- `pun-config`: reading composer.json (`manifest.rs`) and the detection the config loader runs on it and
  composer.lock (`composer.rs`).
- `pun-cli`: the commands (`install.rs`, `update.rs` with `require` and `remove`, `x.rs`, `vet.rs`, `workspace.rs`,
  `self_update.rs`) and their output.
- `pun-packages/src/plugin_host.rs` and `assets/plugin-host.php`: the plugin host. The PHP bootstrap loads the pinned
  composer.phar (downloaded once into Composer's cache and checked against its SHA-256), builds Composer from the
  project's files with scripts off, and answers JSON requests on a pipe: `dispatch` (one priority phase of a script
  event) and `sync` (activate the plugins installed or upgraded since). The `Dispatcher` sends each event's
  priority ≥ 0 phase, runs the scripts, then sends the negative phase. It starts the host only when installed.json
  has an allowed plugin with no native port.
- Not built: the package, installer, pool and download events through the host, custom installer paths, plugin
  commands (`pun composer <command>`), and `vcs`, `git` and `artifact` repositories.

## 5. Build

Each step lands with its tests green and its parity check, in this order. Install from a lock comes first: it is
what CI and most teammates run, and it needs no resolver.

1. **Oracle and corpus.** `scripts/oracle.sh` installs the pinned Composer 2.11.2 phar into `.oracle/composer`.
   `scripts/install-parity.sh <project>` runs `composer install` and `pun install` on scratch copies with an empty
   vendor and compares the trees (paths, contents, modes, symlinks). Composer's installer and autoload fixtures and
   composer/semver's tests are exported to JSON.
2. **`pun-semver`.** Done when composer/semver's tests pass.
3. **composer.json and composer.lock.** Models, `content-hash`, the lock writer, and `pun composer validate`. Done
   when every corpus lock round-trips byte for byte and `validate` matches Composer's messages and exit codes.
4. **`pun install` from a lock.** Download, cache, extract, `installed.json` and `installed.php`, bin proxies, path
   repositories (symlink or copy), source installs for `preferred-install: source` and dev branches. Done when
   install-parity gives identical vendor trees on pinkary, laravel-framework, larastan, laravel-pint and pest, except
   `vendor/composer/autoload_*` until step 5.
5. **`pun composer dump-autoload`.** PSR-4, PSR-0, classmap (with `exclude-from-classmap`), files, `-o`,
   `--classmap-authoritative`, `--apcu`, `platform_check.php`. Done when Composer's autoload fixtures pass and the
   corpus `vendor/composer/` is identical in all modes.
6. **Scripts and events.** The Event shim, every event `install` and `dump-autoload` fire, `pun composer run-script`.
   Done when pinkary's `post-autoload-dump` runs and `php artisan package:discover` writes the same
   `bootstrap/cache/packages.php` after `pun install` as after `composer install`.
7. **Plugins.** First the host with the script, package and installer events and plugin commands; then the native
   ports, each tested against the real plugin. Done when every corpus project and a plugin fixture set
   (pest-plugin, php-http/discovery, extension-installer, composer/installers, phpcodesniffer-composer-installer,
   laravel/vet, symfony/flex, a custom installer, a plugin command) give the same files as Composer.
8. **Metadata and the solver.** Packagist v2 and Composer repositories with the shared cache and conditional
   requests; Pool, PoolBuilder, PoolOptimizer, RuleSetGenerator, Solver, DefaultPolicy, the problem reports. Done when
   Composer's installer fixtures pass. The `pre-pool-create`, `pre-file-download` and `post-file-download` events
   reach the plugin host here.
9. **`pun update`, `pun require`, `pun remove`.** Full and partial updates (`--with-dependencies`,
   `--with-all-dependencies`, `--prefer-lowest`, `--minimal-changes`), `require` and `remove` with JsonManipulator.
   Done when `pun update` writes the same composer.lock as Composer on every corpus project from its composer.json,
   and on 20 recorded partial updates.
10. **The long tail of `pun composer`:** `show`, `outdated`, `why`, `why-not`, `audit`, `licenses`, `check-platform-reqs`,
    `config`, `init`, `create-project`, `global`, `status`, `reinstall`, `bump`, in that order, each with its machine
    format matched.
11. **Docs and bench.** A docs section for the package manager (commands, compatibility, plugins, what is not
    supported), `BENCH=install` results in `context/reports/scoreboard.md`.

## 6. Decisions

- Top-level verbs `install`, `update`, `require`, `remove`; `pun composer <command>` for the rest (Pushpak,
  2026-11-13).
- `composer.json` and `composer.lock` are the source of truth, and every Composer feature they carry is supported
  (Pushpak, 2026-11-15). This reverses the move of the dependencies into pun.toml and pun.lock "like phast"
  (Pushpak, 2026-11-14): pun.lock is gone, and pun.toml keeps only `[workspace]` and Pun's own settings, after Bun's
  package.json / bunfig.toml split. Pun-only install settings that Composer has no key for go in pun.toml, not
  `extra.pun`, when they are built (§7).
- Port Composer's solver, not PubGrub, so the lock matches Composer's (assistant's call, AGENTS.md "Match the upstream
  tool").
- The solver runs on the pool Composer's `PoolOptimizer` leaves, without `Intervals::compactConstraint` (a group can
  only be smaller, so a version or two more stays). When that pool has no solution, the solver runs again on the
  whole pool, so the problems name every version that was loaded (assistant's call, 2026-11-14).
- Metadata files are revalidated on every resolve, as Composer does, although Packagist sends `max-age=900`: a
  release tagged a minute ago must be resolvable (assistant's call, 2026-11-14).
- Composer plugins must work (Pushpak, 2026-11-13): every allowed plugin runs through the plugin host on Composer's
  own classes; native ports only speed up the common ones.
- No phast code is copied (Pushpak, 2026-11-13). Pun's code is written from Composer's behaviour and its own design;
  phast is a benchmark and a reference for ideas only. Composer's own files that `vendor/` must contain
  (`ClassLoader.php`, `InstalledVersions.php`) and the phar the host runs keep Composer's MIT notice.
- `@composer` in a script runs the `composer` on the PATH until `pun composer` has the command (assistant's call,
  revised 2026-11-14: there is no `pun composer` yet).
- `pun self-update` replaces a standalone install with the latest release, checked as the installer checks it; a copy
  Composer installed is updated by Composer (Pushpak, 2026-11-14).
- Composer 2.11.2 is the pinned oracle, and the plugin host runs that same phar; Pun follows Composer's minor releases
  as it follows PHPStan's (Pushpak, 2026-11-14).
- A fresh clone needs no Composer: the standalone installer (`curl -fsSL https://pun.laravel.cloud/install | sh`,
  foundations.md §4) gives a global `pun` that runs `pun install` (Pushpak, 2026-11-14). `pun install` in a project that requires `punphp/pun`
  rewrites the `vendor/punphp/pun` shim but not the binary it runs, which the shim keeps beside it; a test covers it.

## 7. Beyond Composer: what pnpm and Cargo add

Every feature here keeps `vendor/` what Composer writes, and an unused feature changes nothing Pun writes. Settings
that Composer has no key for live in pun.toml (§6), as Bun keeps them in bunfig.toml's `[install]`: the
`extra.pun.*` names below become pun.toml keys when they are built. Each one is built after the step it depends on.

1. **Workspaces (monorepos), as Cargo and pnpm have them.** Built (2026-11-14) as below, except where noted.
   - The root `pun.toml` opts in with `[workspace] members`, a list of globs (`["packages/*", "apps/*"]`) (Pushpak,
     2026-11-14). Each member is a directory with its own composer.json, which needs a `name`. Pun adds a `path`
     repository per glob and requires each member at `*@dev` in memory; nothing is written to the root
     composer.json. Not built: `pun workspace init`.
   - One composer.lock and one `vendor/` at the root, as Cargo keeps one `Cargo.lock` and one `target/`. Members
     are installed as path packages (symlinked) and every member's `require` joins one resolution, so the whole
     repository gets one version of each package.
   - Every command finds the workspace root by walking up from the current directory, so `pun install` works in any
     member.
   - `-p <member>` (Cargo's flag; pnpm calls it `--filter`) picks members by name or glob: `pun require -p acme/billing
     guzzlehttp/guzzle` edits that member's manifest and updates the root lock; in a member, the member is the default.
     Not built: without a constraint, `require` should use the constraint other members already have for that
     package (it writes the newest `^major.minor` today).
   - `pun composer run-script --workspace <script>` runs a script in every member that defines it, in dependency
     order, with independent members in parallel. `pun check`, `format`, `lint` and `analyse` take `--workspace` too
     and use each member's own `pun.toml`.
   - `pun composer validate --workspace` reports what Cargo's `workspace.dependencies` would have prevented: two
     members requiring different constraints for one package, and a member requiring another member at a version it
     does not have.
2. **A shared file store, as pnpm has.** After step 4. Every extracted file lives once in a content-addressed store
   in Pun's cache, and `vendor/` gets copy-on-write clones of it (APFS, btrfs, XFS), so the second project that needs
   `laravel/framework` 13.2.0 installs it without copying. The bytes and modes are Composer's, so install parity
   holds. Where the filesystem cannot clone, files are copied; hardlinks (pnpm's default) are opt-in through
   `extra.pun.link = "hardlink"`, because editing a hardlinked file in `vendor/` edits every project's copy.
3. **`--locked` and `--offline`, as Cargo has them.** After step 4, not built. `pun install --locked` fails when
   there is no composer.lock or its `content-hash` does not match composer.json (Composer only warns). It is the default when
   `CI` is set, as pnpm's `--frozen-lockfile` is. `--offline` resolves and installs from the cache only and names the
   first package it cannot find there.
4. **`pun x`, as `pnpm dlx` and `cargo install` have it.** Built (2026-11-14), also as `punx`.
   `pun x laravel/installer new app` resolves the package into a cached scratch project (its own composer.json
   and composer.lock) and runs its binary without touching the current project. `pun x pkg:^1.2` (or `pkg@1.2`) pins a
   version, short aliases name common tools (`laravel`, `pint`, `phpstan`, `pest`, ...), and `punx list` and
   `punx clean [package...]` show and remove the cached installs.
5. **A minimum release age, as pnpm has it.** After step 8. `extra.pun.minimum-release-age` (minutes) makes the
   resolver ignore versions published more recently, against compromised releases that are pulled within hours.
   Off by default, because with it `pun update` can pick older versions than `composer update`.
6. **Patches, as `pnpm patch` and Cargo's `[patch]` have them.** After step 7. `pun patch <package>` copies the
   package to a scratch directory to edit, and `pun patch --commit <dir>` writes a unified diff to `patches/` and
   registers it under `extra.patches` in the format `cweagans/composer-patches` reads. The same patch then applies
   under Composer with that plugin, and Pun applies it natively.

## 8. Open questions (needs Pushpak)

1. **Which §7 features land before 1.0.** Proposed: `--locked` and `--offline` with step 4, and the rest after 1.0
   (workspaces and `pun x` are built).
