# Mago extras: the `Pun/*` rules

The `Pun/*` rules are opt-in lint rules beyond Pint, adopted from [Mago's linter](https://mago.carthage.software/1.61.0/en/tools/linter/rules/).
None of them is a format rule, so `pun format` stays byte-identical to Pint.

Each is one file in `crates/pun-fixers/src/fixers/pun_*.rs` that uses the same `Fixer` trait and registration as the
ports, and runs at priority 0. A pint.json names them `Pun/<name>`; `pun migrate` turns
`"Pun/prefer_fake_helper": true` into `"laravel/prefer-fake-helper" = "error"`. Turn one on by its lint ID under
`[lint.rules]` in `pun.toml`; `pun lint --fix` and `pun check --fix` apply the fix. Mago's test cases are the unit
tests (`cargo test -p punphp-fixers --lib pun_`); where Pun deliberately differs, the test asserts Pun's output and
says why.

Status: all are ported. Risky = `is_risky()` in the fixer (`tests/pun_rules.rs` checks the list). Fix and set come
from `crates/pun-config/rules/mapping.toml`; "no set" means the rule is only on when named.

| `Pun/*` name | lint ID | What it does | Status |
|---|---|---|---|
| `Pun/prefer_dedicated_status_assertion` | `laravel/prefer-dedicated-status-assertion` | `->assertStatus(404)` → `->assertNotFound()` (25 codes); skips calls with comments inside | safe, code-quality |
| `Pun/prefer_fake_helper` | `laravel/prefer-fake-helper` | `$this->faker` → `fake()` in classes named `*Factory`; skips write contexts, where `fake()` would not compile | unsafe, code-quality |
| `Pun/prefer_array_validation_rules` | `laravel/prefer-array-validation-rules` | `'required\|string'` → `['required', 'string']` in `rules()`, for string-literal keys whose value is the whole literal | safe, code-quality |
| `Pun/no_implicit_model_query` | `laravel/no-implicit-model-query` | `User::where(...)` → `User::query()->where(...)`; a name heuristic, resolves `use ... as` aliases | risky, unsafe, code-quality |
| `Pun/final_controller` | `laravel/require-final-controller` | makes non-abstract classes named `*Controller` `final`; skips a class named exactly `Controller` and controllers extended in the same file. Risky: subclasses and mocks in other files break | risky, unsafe, no set |
| `Pun/use_specific_expectations` | `pest/prefer-specific-expectations` | `expect($x)->toBe(true\|false\|null)` → `->toBeTrue()` / `->toBeFalse()` / `->toBeNull()`; only rewrites that keep the test's meaning | safe, coding-style |
| `Pun/use_dedicated_expectation` | `pest/prefer-dedicated-expectation` | `expect(is_array($x))->toBeTrue()` → `expect($x)->toBeArray()`, likewise `str_starts_with`, `str_ends_with`, `property_exists` and other exact-matcher `is_*` checks; keeps `->not`. Risky: some matchers also require a string or object | risky, unsafe, coding-style |
| `Pun/use_simpler_expectation` | `pest/prefer-simpler-expectation` | `expect($a > $b)->toBeTrue()` → `expect($a)->toBeGreaterThan($b)`, likewise `>=` `<` `<=` `===` `!==` `instanceof`, when the comparison is the whole argument; not negations | safe, coding-style |
| `Pun/no_only` | `pest/no-only` | removes `->only()` (and the whitespace before it) from `test`/`it`/`describe`/`todo`/hook chains | unsafe, coding-style |
| `Pun/prefer_array_spread` | `arrays/prefer-array-spread` | `array_merge($a, [1], $b)` → `[...$a, 1, ...$b]`. Risky: needs PHP ≥ 8.1, and a spread accepts `Traversable`s where `array_merge()` throws | risky, unsafe, modernize |
| `Pun/prefer_first_class_callable` | `php-8.1/prefer-first-class-callable` | `fn ($x) => $this->m($x)` → `$this->m(...)`, methods only. Risky: the callable then receives every argument the caller passes | risky, unsafe, no set |
| `Pun/prefer_explode_over_preg_split` | `performance/prefer-explode-over-preg-split` | `preg_split('/, /', $s)` → `explode(', ', $s)`; drops a `-1`/`0`/`null` limit (`explode()` reads `-1` as "all but the last piece"), skips empty patterns and non-literal limits | safe, code-quality |
| `Pun/no_redundant_isset` | `complexity/no-redundant-isset` | `isset($a, $a['b'])` → `isset($a['b'])` | safe, code-quality |
| `Pun/no_redundant_literal_return` | `complexity/no-redundant-literal-return` | `if ($x === null) { return null; } return $x;` → `return $x;`; floats are not literals here (`-0.0 === 0.0`) | safe, code-quality |
| `Pun/no_self_assignment` | `dead-code/no-self-assignment` | deletes `$a = $a;`, variables only: property self-assignments can have effects (`__set`, hooks, mutators) | safe, dead-code |
| `Pun/sensitive_parameter` | `security/add-sensitive-parameter` | adds `#[\SensitiveParameter]` to non-boolean params named like `*password`/`*token`/`*secret`/`*apikey`/`*api_key` | safe, code-quality |
| `Pun/no_redundant_nullsafe` | `complexity/no-redundant-nullsafe` | `$user?->name ?? 'Guest'` / `isset($user?->profile)` → `->`, only when the fetch is the whole left side of `??` | safe, code-quality |

Mago's lint-only Laravel rules are native `pun-lint` rules (`crates/pun-lint/src/rules/laravel/`):
`laravel/no-request-all`, `laravel/prefer-casts-method`, `laravel/prefer-anonymous-migration`,
`laravel/prefer-view-array` and `laravel/no-controller-middleware`. Mago's other lint rules map to Pun lint rules in
mapping.toml (`[upstream.mago-linter]`).
