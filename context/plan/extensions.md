# Extensions

Laravel, Larastan, Pest, PHPUnit and Doctrine support moves out of Pun's core into extensions. Each extension uses the
same API that a Symfony extension, or any other third-party extension, would use. The model is Mago's built-in Rust
plugins (`mago-analyzer`'s `Plugin` trait and `ALL_PLUGINS`), widened to every command. Extensions are Rust only for
now (Pushpak, 2026-11-14).

## What Mago does (1.59.0)

- **Built-in plugins.** `crates/analyzer/src/plugin/plugin.rs` defines
  `trait Plugin: Send + Sync { fn meta(&self) -> &'static PluginMeta; fn register(&self, registry: &mut PluginRegistry); }`,
  and `plugin/libraries/mod.rs` lists them in `static ALL_PLUGINS`. `PluginRegistry` has typed `register_*` methods for:
  - function and method return types;
  - assertions and throw types;
  - program, statement, expression, call and declaration hooks;
  - issue filters.
- **External extensions.** Worker processes over the `mago-extension` binary protocol, with a PHP SDK, configured
  under `[extension-hosts.NAME]`. They hook the linter and the analyzer only. Mago's formatter has no extension API.
- **Framework lint rules.** Mago has no plugin for these. They sit behind a fixed `enum Integration` (Laravel,
  Symfony, PHPUnit, Pest and others) that `mago init` fills from composer.json.

## Design

### The Extension trait

`crates/pun-extension` defines one trait with one hook per command. `pun-cli` lists the built-in extensions, as Mago
lists `ALL_PLUGINS`:

```rust
pub trait Extension: Send + Sync {
    fn meta(&self) -> &'static ExtensionMeta;
    fn format(&self, _: &mut pun_format::FixerRegistry) {}
    fn lint(&self, _: &mut pun_lint::RuleRegistry) {}
    fn analyse(&self, _: &mut pun_analyse::ExtensionRegistry) {}
    fn packages(&self, _: &mut pun_packages::PluginRegistry) {}
}

pub static ALL: &[&dyn Extension] = &[&Laravel, &Pest, &Phpunit, &Doctrine];
```

`ExtensionMeta` lives in `pun-config` and holds only data:

- `id`, `name`, `description` and `aliases`.
- `groups`: the rule groups the extension owns, such as `laravel` and `laravel-*`.
- `packages`: the locked packages that turn it on, such as `laravel/framework`.
- `rules`: its mapping.toml fragment, which uses the same schema as mapping.toml.

### Registration per command

- **Format.** `FixerRegistry::add`. Fixers are still sorted by priority, then by name, so the order extensions
  register in changes no output.
- **Lint.** `RuleRegistry::add` takes a native rule, and `add_fixer` takes a token fixer that backs a lint rule. The
  registry keeps insertion order, because the order of fix rounds changes the bytes.
- **Analyse.** Two layers:
  - **Mago's `PluginRegistry`, passed through,** for providers and expression hooks.
  - **PHPStan-shaped extension points in one `AnalyseExtension` trait.** Larastan plugs into PHPStan, not Mago, and
    parity is with Larastan. Its methods:
    - `from_parameters` for activation;
    - `stub_files`;
    - `prepare_codebase`;
    - `settings`;
    - `has_method` and `has_property`, the class reflection extensions;
    - `type_node`;
    - `visit_*`, `enter_function` and `leave_function`, which are rules;
    - `finish`, which collects;
    - `identifiers`.

  The checker holds `&[&dyn AnalyseExtension]` in place of `Option<&Larastan>`.
- **Packages.** Composer plugin ports, such as `pestphp/pest-plugin`, keyed by the plugin's package name.

### pun.toml

```toml
[extensions]
laravel = "auto"   # the default: on when pun.lock locks one of the extension's packages
pest = "off"
symfony = "on"
```

- The existing `[lint.groups]` and `[analyse.rules]` group switches keep working.
- `pun config` prints the resolved `[extensions]` table.

### Rule IDs

Rule IDs stay `<command>/<group>/<name>`, and each extension owns its groups, so every existing ID survives.
Registration fails in three cases:

- two extensions own the same group;
- an extension claims a core group;
- a rule uses a group that its extension does not own.

### Distribution

- **Now:** compiled in. First-party extensions are `crates/pun-ext-*`, and third-party extensions arrive by pull
  request.
- **Later:** a custom binary. `pun_cli::main(&[&dyn Extension])` would let a third party build its own `pun`. The
  crates have `publish = false`, so that needs tag-pinned git dependencies, and the patched-Mago `[patch]` section
  copied over.
- **Rejected:** dynamic libraries. Rust has no stable ABI, and Pun ships static musl binaries.
- **Future:** worker processes on Mago's protocol, or WASM.

## Migration

No step changes output. Every step must keep these green: the parity tests, `pun-harness`, `scripts/cli-parity.sh` and
`cargo nextest run -p punphp-analyse`.

1. **ExtensionMeta.** Add `ExtensionMeta` to `pun-config` for the four ecosystems. `Ecosystem` becomes a lookup over
   the metas, and `Gates::locked` matches on package names.
2. **The `[extensions]` table.** Add it with `auto`, `on` and `off`. An unknown name is a config error (exit 2).
3. **pun-extension and the lint registry.** `run.rs` and `lint_plan.rs` build their rules from `ALL`. The rules
   themselves stay inside pun-lint for now, because moving them means making about 400 helper references public.
   Done: `pun_extension::lint_rules` registers the core's rules, then each extension's. The core used to sit between
   Laravel's and Pest's rules, so a file's findings now list in a different order; on pinkary with every lint group on,
   `pun lint --fix` writes the same bytes. Four `lint/phpunit` rules that live in `types/` are now the PHPUnit
   extension's.
4. **The format registry.**
   - `build.rs` emits `core()`, `laravel()` and `pest()`.
   - The `php_unit_*` and Doctrine fixers stay core, because PHP-CS-Fixer ships them in its core.
5. **`AnalyseExtension`, one seam at a time.** Larastan implements it in place, in this order:
   1. stubs and codebase setup;
   2. providers and settings;
   3. reflection queries (`types/receivers.rs` first);
   4. node callbacks and `finish`;
   5. identifiers;
   6. `Project.larastan` becomes `extensions`.

   At the end, `pun analyse` JSON on pinkary, larastan, pest and laravel-framework must be byte-identical to the
   base commit.
6. **mapping.toml.** The ecosystem rows move into per-extension fragments.
   Folded into step 7 (2026-11-15): the Rust section loader and eight scripts read the one file, and
   `gen-phpstan-parameters.py` rewrites it in place, so the fragments move once each extension has a crate to hold
   them.
7. **Physical crates.** The extensions become `crates/pun-ext-{laravel,pest,phpunit}`, which makes the helpers they
   use public.
8. **Discovery.** Add `pun extension list` and a docs page. Done: docs/content/docs/commands/extension.mdx.

## Risks

- Lint registration order changes fixed bytes. Diff the old `all()` ID list against the registry's once, before
  deleting `all()`.
- Larastan branches sit inside core PHPStan rules (`calls.rs`, `types/receivers.rs`, `types/convert.rs` and
  `driver.rs`). Step 5.3 is the hard part, so spike it on `types/receivers.rs` first.
