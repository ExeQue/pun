# Lint behaviour notes

How the `types/*` lint rules match Rector. They are too detailed for the public command page
(`docs/content/docs/commands/lint.mdx`).

- Types are read as Rector 2.8.4 reads them with PHPStan 2.3.5, the version it requires. In a trait, only `$this` and
  the trait's own members have no type.
- `types/add-return-type-from-constant` reads documented constant types. Anonymous classes count as final for
  late-static constant lookup.
- Property-type rules keep relative `parent` hints from constructors and accessors instead of replacing them with the
  parent's class name. Their PHPDoc cleanup compares scalar-only unions as Rector compares scalar types.
- Constructor initialization detection recognizes same-class named static calls and property assignments, with
  imported and qualified class names. It follows documented peer receivers and inherited helper declarations,
  including external and generic receivers. Only direct, first-level helper assignments count; helper calls do not
  recurse through other helpers.
- Class-name case rules and ambiguous intersection receivers check calls and assignments separately, as Rector does.
- Object-type comparison keeps intersection constraints and drops one only when class reflection proves it redundant.
- Fix rounds refresh closure and arrow-function signatures after source edits. Moving a closure keeps its parameters,
  return type and captured variables.
