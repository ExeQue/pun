# Extensions

Laravel, Pest and PHPUnit support lives in extensions: Rust types compiled into `pun` that implement one `Extension`
trait with a hook per command, the same API any other extension would use (Rust only for now, Pushpak).

## Design

### The Extension trait

`crates/pun-extension` defines the trait, and `pun-cli` lists the built-in extensions
(`crates/pun-cli/src/extensions.rs`):

```rust
pub trait Extension: Send + Sync {
    fn meta(&self) -> &'static ExtensionMeta;
    fn format(&self, _: &mut FixerRegistry) {}
    fn lint(&self, _: &mut RuleRegistry) {}
}

pub(crate) static ALL: [&dyn Extension; 3] = [&Laravel, &Pest, &Phpunit];
```

Not built: an `analyse` hook (Larastan is wired through `pun_analyse::AnalyseExtension` directly, step 5) and a
`packages` hook for Composer plugin ports (today the `NATIVE` list in `crates/pun-packages/src/plugins.rs`).

`ExtensionMeta` lives in `pun-config` (`crates/pun-config/src/extensions.rs`) and holds only data:

- `id`, `name` and `description`.
- `groups`: the rule groups the extension owns, such as `laravel`; each also owns its version groups (`laravel-12`).
- `packages`: the locked packages that show a project uses it, such as `laravel/framework`; the first one gates the
  version groups.

Not built: `aliases`, and `rules` (its mapping.toml fragment, step 6).

### Registration per command

- **Format.** `FixerRegistry::add`. Fixers are sorted by priority, then by name, so registration order changes no
  output.
- **Lint.** `RuleRegistry::add` takes native lint rules. The registry keeps insertion order, because the order of fix
  rounds changes the bytes. The token fixers that back lint rules (the `Pun/*` ports included) stay in pun-fixers'
  core list; the Laravel extension's `format` hook adds only the Blade fixer.
- **Analyse.** Two layers:
  - Mago's `PluginRegistry`, passed through, for providers and expression hooks.
  - `pun_analyse::AnalyseExtension`, shaped like PHPStan's extension points, because Larastan plugs into PHPStan and
    parity is with Larastan. It has `stub_files`, `bundled_stub`, `class_aliases`, `prepare_codebase`, `load`,
    `register` (the `PluginRegistry`, per file), `settings`, `typed_expressions` and `type_node`. Still to add:
    `from_parameters` (activation), the rule callbacks `visit_*`, `enter_function` and `leave_function`, `finish`
    (collects) and `identifiers`.
  - A crate-private `ClassReflectionExtension` (`rules/class_reflection.rs`) answers method and property lookups:
    `method` (returns `MethodAnswer::{Forward(Type, Reflected), Exists(Option<String>)}`), `has_property`,
    `calls_inaccessible_method` (a model's `increment`/`decrement`) and `static_forward` (a static call on a model to
    a builder method). It takes the crate-private `Checker`, so it stays crate-private until step 7 gives it a public
    reflection facade.

  The checker will hold `&[&dyn AnalyseExtension]` in place of `Option<&Larastan>`.
- **Packages.** Composer plugin ports, such as `pestphp/pest-plugin`, keyed by the plugin's package name. Not built
  as a hook (see above).

### pun.toml

```toml
[extensions]
laravel = "on"
pest = "on"
```

- An extension runs only when `[extensions]` says `"on"`, so pun.toml alone shows what runs (Pushpak). A locked
  package never turns one on. `pun migrate` and `pun init` write `"on"` for each extension whose package composer.lock locks.
  `"auto"` is a config error that says so.
- An extension that is on brings every rule of its groups that sits in some lint set, whatever `[lint] sets` lists.
  Its rules in no set (its upgrade groups, opinionated codemods) stay opt-in by name or group. Analyse rules still
  follow the analyse preset.
- Format rules don't depend on extensions: `format/laravel/blade` stays a `[format.rules]` entry, which migrate writes
  only when pint.json turns it on.
- The `[lint.groups]` and `[analyse.rules]` group switches keep working.
- `pun config` prints the resolved `[extensions]` table.

### Rule IDs

Rule IDs stay `<command>/<group>/<name>`, and each extension owns its groups, so every existing ID survives. Tests in
`crates/pun-cli/src/extensions.rs` fail when a core rule sits in an extension's group, or an extension's lint rule or
fixer sits in a group it does not own. Nothing checks this at run time, and nothing checks that two extensions own
different groups.

### Distribution

- **Now:** compiled in. First-party extensions become `crates/pun-ext-*`; third-party extensions arrive by pull
  request.
- **Later:** a custom binary. `pun_cli::main(&[&dyn Extension])` would let a third party build its own `pun`. The
  crates have `publish = false`, so that needs tag-pinned git dependencies and a copy of the patched-Mago `[patch]`
  section.
- **Rejected:** dynamic libraries. Rust has no stable ABI, and Pun ships static musl binaries.
- **Future:** worker processes or WASM.

## Migration

No step changes output. Every step keeps these green: the parity tests, `pun-harness`, `scripts/cli-parity.sh` and
`cargo nextest run -p punphp-analyse`.

1. Done.
2. Done.
3. Done. `pun_extension::lint_rules` registers the core's rules, then each extension's. The rules still live inside
   pun-lint, because moving them means making about 400 helpers public.
4. Done. The `php_unit_*` fixers stay core, because PHP-CS-Fixer ships them in its core.
5. **`AnalyseExtension`, one seam at a time.** Larastan implements it in place:
   1. Done.
   2. Done.
   3. Done.
   4. Node callbacks and `finish` (next). Also moves the static-forward gate in `calls.rs` off `c.larastan`.
   5. Identifiers.
   6. `Project.larastan` becomes `extensions`. `Project::extensions()` still yields `project.larastan`, and `load`
      takes `&mut self`, so this needs owned extensions.

   *Done when* `pun analyse` JSON on pinkary, larastan, pest and laravel-framework is byte-identical to the base
   commit.
6. **mapping.toml.** The ecosystem rows move into per-extension fragments. Done with step 7, because the Rust section
   loader and eight scripts read the one file and `gen-phpstan-parameters.py` rewrites it in place.
7. **Physical crates.** The extensions become `crates/pun-ext-{laravel,pest,phpunit}`, which makes the helpers they use
   public.
8. Done.

## Decisions

- **Built-in extensions** are Laravel, Pest and PHPUnit. Doctrine and Symfony support is removed.
- **Method lookup order** (Pushpak): `MethodPhase::BeforeMixins` runs after the class's own methods, and a forward that
  finds nothing lets the lookup go on; `MethodPhase::Last` runs after `@mixin` and `@phpstan-require-extends`. PHPStan
  asks once, before `@mixin`; moving to that order needs its own corpus proof.

## Risks

- Lint registration order changes fixed bytes. A test in `crates/pun-cli/src/extensions.rs` compares the registry's
  ID list with `pun_lint::rules::all()`; deleting `all()` loses that check.
- Larastan branches still sit inside core rules (`calls.rs`, `declared.rs`, `property_access.rs`, `types/access.rs`,
  `types/convert.rs` and `driver.rs`).
