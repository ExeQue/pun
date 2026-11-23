# Analyse behaviour notes

How `pun analyse` follows PHPStan and Larastan, in more detail than the public page
(`docs/content/docs/commands/analyse.mdx`). Every behaviour here gives the same types and errors as PHPStan 2.3.5
with Larastan. The gaps are listed at the end.

## Configuration

- PHPStan parameters Pun reads: `checkThisOnly` (property receiver eligibility, at any level),
  `checkAdvancedIsset` (independent of the level, without changing other rules' levels),
  `treatPhpDocTypesAsCertain` (default true; when false, `isset` and `??` checks use native operand types without
  falling back to PHPDoc), `checkInternalClassCaseSensitivity` and `featureToggles.checkImportedClassNameCase` (both
  default false; bleeding edge turns on imported-name checking unless disabled).
- Scope pollution parameters, all default true:
  - `polluteScopeWithLoopInitialAssignments` off: `for` exits include the pre-loop scope, and `while` does when zero
    iterations are possible. It does not change `checkMaybeUndefinedVariables`, `foreach` or `do-while`.
  - `polluteScopeWithAlwaysIterableForeach` off: a nonempty `foreach` keeps its final types but does not make new
    variables definitely defined after the loop. Assignments in the iterable expression still apply.
  - `polluteScopeWithBlock` off: a standalone braced block joins its incoming and final variable types and certainty.
    Termination, loop exits, throws and expression effects are unchanged; control-statement braces don't count.
- Method-call, override and consistent-constructor rules are enabled independently. Turning off renamed-parameter
  reports keeps the other method diagnostics and unmatched-ignore errors. A joined renamed-parameter diagnostic consumes
  only the exact ignored identifier, not others with the same prefix.
- `compact()` findings share the `analyse/undefined/variable` selection.

## Larastan

- `Config::collection()` resolves documented config shapes in the caller's namespace, class and template scope,
  including implicit integer keys and class-constant keys. Like Larastan, it caches the first resolved type for each
  complete config key.
- Source `class_alias()` calls register no aliases during analysis or scanning. Laravel's statically resolved alias
  registry is used.

## Calls and callables

- Static calls check PHPDoc `@method` signatures, including inherited tags: required and named arguments, and static
  calls to non-static methods.
- Failed named instance and static method lookups keep the error type when no reflected or provider result exists.
  Ordinary method calls keep unknown-class diagnostics and their tips. Dynamic selectors and `method_exists()`
  assertions keep their own fallback.
- Unresolved named function calls give the error type after argument evaluation, also inside `function_exists()`
  guards. Calls through `mixed` or bare `object` give `mixed`. Definitely invalid callees, including `mixed` narrowed
  by a negative `is_callable()`, give the error type.
- Inaccessible array and string callables keep their access diagnostic but return `mixed`, also through
  `call_user_func()` and `call_user_func_array()`. Accessible calls and existing closures keep declared signatures.
- First-class built-in function callables keep every positional overload, with its parameter names, reference flags
  and return type. Creating the callable selects no overload.
- First-class instance method callables with dynamic names, and those of a missing named method (after its
  diagnostic), have type bare `Closure`. The receiver and name evaluate once, in that order. A bare `Closure` satisfies
  ordinary callable signatures without invented types, but not `pure-callable` without known purity.
- Dynamic callable arguments keep their reference policies below level 2, telling a read of an existing variable from
  the creation of an output variable. Callable-array refinements keep callable constraints and surviving key order
  without restoring removed offsets.
- Stored callable signatures keep the declared return type rather than specializing it from the call's arguments.
- Inline non-static closures and arrow functions use the class named by `@param-closure-this`, resolved on named
  functions and on methods called through a class name or one known object class, including nullable receivers.
- Inherited PHPStan API signatures resolve PHPDoc names with the embedded declaration's namespace and imports.
- Trait aliases use the reflected lookup name for PHPDoc purity, return types and constructor classification.
  Trait-alias override checks use the alias name, adapted visibility and method policy. Case-insensitive alias lookups
  keep the written spelling in reflected names and diagnostics.
- Override prototypes exclude interfaces a parent class or another interface already supplies; the rest keep
  declaration order. Method-purity override checks include built-in parents and inherited interface methods, using
  their reflected side effects.

### `void` and nullsafe

- Named function, method and static calls returning `void` have value type `null`. Void-use diagnostics keep the raw
  return type. Calls through callable values keep `void` as the value type.
- `match` combines arm types without dropping `void`. `??` uses the value type: a named void call selects the fallback,
  a callable-value void call keeps `void`.
- Nullsafe calls keep declared `null` returns apart from null from a nullable receiver, including nullable template
  bounds. Nullsafe chains drop only short-circuit null, so a nullable method result stays nullable for a chained call.

### Arguments

- Argument-count checks tell empty arrays from non-empty open array shapes passed with `...`.
- Unpacked operands and ordinary non-closure arguments evaluate once, in written order. Binding uses each unpack's
  stored type.
- Overload selection expands constant-array operands before comparing types, and every unpack bypasses arity
  filtering. Unknown unpacks replace collided selector keys; constant-array alternatives combine value types.
  Selection reads argument types even when argument-type and by-reference diagnostics are off.
- When argument-type or by-reference checks are on, calls respect `@no-named-arguments` on functions, methods and
  constructors, including inherited policies. Named arguments and string-keyed unpacking report `argument.named` after
  binding. Unknown arguments keep their binding diagnostics.
- From level 5, shared argument checks validate allowed built-in constants, report unresolvable generic parameter
  types (with the comparison's or array-shape key's explanation), and reject static closures for parameters that need
  a bound `$this`. They also apply to attribute constructors.
- Named-argument checks apply finality to the method's actual declaring class. Compound argument binding composes the
  original signatures before selecting a variant. Union and intersection reflections keep their own visibility and
  named-argument policies, and generic proxy unions keep every member's policy, also through nullsafe calls.
- Binding uses the selected declaring class's built-in status, including through recursive `@mixin` and supplied
  template arguments. Interface `@phpstan-require-extends` methods use the required type's reflection, including
  mixins and generic arguments; inherited requirements keep lookup order and cycle protection.

## Properties

- Static property checks resolve each name in a finite inferred name set. Single-class receivers resolve singleton
  dynamic names and keep supplied or inherited generic arguments.
- Receiver eligibility includes parenthesized and nullsafe receivers. Failed reads keep the error type, not `null`.
- `++`/`--` validate the stored new value apart from a postfix expression's old result.
- Writes use the reflected writable type, including compatible PHPDoc over native properties and a set hook's
  parameter type. Writable types and declaring-class descriptions resolve generic arguments through the declaring
  ancestor, including defaults and bounds.
- Native `unset` checks keep a property's modifiers and hooks under a PHPDoc annotation; annotation-only properties get
  no native modifiers. Interface `@phpstan-require-extends` properties take part, as non-native when annotation-only.
- Literal `$this->property` checks read virtual initialization markers from the incoming scope, also before `??=`.
  Constructor summaries join normal and explicit-return exits (explicit returns with their scope before `finally`);
  throwing exits don't count. Same-class instance methods get the summaries unless serialization can bypass the
  constructor. Constructor helper calls add non-transitive summaries from same-file methods of at most 50 lines;
  ambiguous compound helper declarations stay conservative. `unset` does not clear the markers.

## Narrowing and invalidation

- Instance calls invalidate narrowed mutable receiver properties for constructors, known side effects and unresolved
  methods. Possibly impure calls keep receiver narrowings. Readonly properties, and private properties declared outside
  the callee's class, keep their facts.
- Assertions narrow repeated zero-argument method calls even when reflection marks them possibly impure. Known side
  effects invalidate call observations that depend on the affected receiver or object arguments. Built-in method
  effects come from PHPStan's reflection metadata, not source PHPDoc purity.
- Boolean and loop joins keep only method specifications that hold on both paths. Strict equality assertions refine
  both operands, including impossible comparisons.
- Branch joins keep conditional type correlations even when every branch defines the variable. Narrowing a
  discriminating guard narrows its correlated receiver, value or unpacked array. Writes and unsets invalidate affected
  correlations, including symbolic array indexes.
- Function and method entry keep top-level conditional parameter relationships, including forward references and
  negated conditions. Closures keep them when both variables are captured; arrow parameters invalidate those they
  shadow. Read-only by-reference captures keep their types at creation.
- Constant-array `foreach` correlates each literal key with its value when both targets are direct variables and the
  loop has no doccomment; writes invalidate it. Generic key and value unions imply no such link.
- Class-constant checks keep expression guards across branches, ternaries and short-circuits. Writes invalidate guards
  for the affected receiver or selector; named calls keep argument identities; impure methods and by-reference
  arguments invalidate. Dynamic method selectors keep their written identities, telling instance from static literal
  selectors.
- Negative type guards keep excluded values in `mixed`. Object intersections combine excluded classes and drop
  duplicate constraints without losing generic arguments or `static`. Unions restore only alternatives proven to belong
  to the excluded type.
- Short-circuit boolean expressions keep evaluated operand types and excluded object alternatives. Output arguments of
  a skipped right operand stay possibly undefined.
- Only declared or inherited assertion contracts narrow a caller's arguments, not a predicate inferred from a callee's
  body. Expression statements apply unconditional assertions after operand evaluation and joins; built-in `assert()`
  keeps its truthy-argument behavior.
- At file scope and after `extract()`, `unset` removes a variable's holder without making it definitely absent: a later
  read is `mixed` and possibly undefined. Elsewhere it is definitely undefined. False `isset` refinements use the same
  fallback.

## Control flow

- `finally` entry joins branch ends, throw points and abrupt exits. A nonterminating `finally` updates pending return,
  break and continue scopes before outer scopes join.
- For and while conditions use evaluated operand types without evaluating again. Do-while keeps its unconditional
  first body. Scope joins drop values from explicitly absent variable holders but keep genuine nullable values.
- An empty-iteration path keeps template iterator identity, including empty-list intersections for list bounds.
- `foreach` over sealed constant lists of up to 16 required entries keeps sequential array mutation state, including
  local `continue` edges. Nested replay applies PHPStan's product limit of eight once an outer replay has more than one
  entry. Other loops (references, `break`, `return`, optional entries, shape unions, associative iterators, non-direct
  targets) use general loop inference.
- Implicit `never` from an empty value domain does not make later statements unreachable. Call termination follows the
  resolved declaration, not a dynamic return type. An expression statement of explicit `never` terminates. `exit`,
  `die` and `throw` give a nonaccepting `never`, also through inferred arrow-function returns.
- At every level the rule walk skips statements after termination but keeps labels and function and class
  declarations. Unreachable-statement diagnostics start at level 4.

## Assignments, destructuring and `unset`

- Assignment targets keep their preparation-time reads before the right-hand side and the write. Array roots, indexes,
  property receivers and selectors don't evaluate again. `??=` targets use write context without suppressing reads of
  indexes or the right-hand side, and keep right-hand effects and throws when the target is already nonnull.
- Compound assignments keep the completed input value apart from the written holder. Optional array slots keep absence
  apart from a nullable value. Cached property values don't skip evaluating their receiver or selector.
- References are allowed on individual destructuring targets and nested `array(...)` targets inside short-array
  destructuring. Arrays directly inside an explicit `list()` are literal expressions, not targets; their keys and values
  are read after the right-hand side and preceding targets.
- Keyed destructuring keeps empty-offset assignment contexts across later items; branch merges and nested functions
  isolate them. Parser checks permit omitted destructuring targets, including nested ones.
- Reference-valued array literals keep failed reads as `*ERROR*`, not `mixed`. Taking a reference keeps the root value
  and invalidates dependent offset observations. Virtual offset writes update reference-linked locals, not copies.
- Offset unsets transform each member of a template's union bound and keep the template when none changes.

## `isset`, `compact()` and offsets

- `compact()` keeps resolved function-import names and reads variable certainty before evaluating arguments. Benevolent
  unions and callable-array intersections keep their literal variable names.
- Nested variable names are checked as reads even where assignment, `isset`, `empty`, `??` or `unset` allow an
  undefined outer target. Variable-name type errors point to the name expression's line.
- From level 1, `isset` checks each operand's incoming variable certainty; level 4 adds offset and property checks.
  Multiline diagnostics use the `isset` line. Property reflection and unnecessary-coalescing checks keep their own
  full-type rules.
- Integer-to-string casts keep decimal-integer-string precision, including bounded ranges. Array offset checks coerce
  definite decimal-integer strings to integer keys; string offset checks keep the raw key. For `isset` and `??`,
  broad booleans become `0|1` and broad floats integers; float keys keep their invalid-key diagnostics.
- Constant-array checks try each union-key alternative and integer ranges of at most 129 values. Only selected fields
  count for nullability; optional fields stay possibly absent.
- Variable names read from literal array offsets keep branch-dependent definedness after direct offset assignments;
  writes to the array or another offset invalidate them. Literal string offsets use PHP bytes and allow negative
  indexes.

## Native types and the PHPDoc-certainty tip

- For supported native expressions, constant `if` checks tell documented from native types when adding the
  PHPDoc-certainty tip; function type-predicate checks use each argument's native type. The remaining-cases tip wins.
- Native array literals, offsets, `isset` operands and `??=` keep separate native values without replacing documented
  results or evaluating again. Native constant arrays keep insertion order through literals, copies and supported
  nested reads, without PHPDoc-only callable constraints.
- Nullable single-class parameters support native nullsafe reads of ordinary scalar properties, from the native
  declaration. Dynamic, magic, hooked and array-valued properties are unsupported.
- For named-object parameters, the PHPDoc type stays within the native declaration: an incompatible documented `null`
  doesn't make a nonnullable parameter nullable, while compatible subclasses and template refinements apply.
- Native local values survive bounded function calls with positional scalar or null arguments and no reference,
  callable, assertion or global effects. Unpacked arguments are unsupported.
- Native `unset` transfers ordinary locals and single-level offsets with literal, finite literal-union or integer
  keys. Other targets, and transfers with missing native inputs, are unavailable and imply no PHPDoc certainty.

## Constants

- Global constant reflection keeps initializer unions apart from file-local flow values; deferred references resolve
  before ternary and `??` filtering. Class and interface constants keep canonical initializer unions apart from
  declared types, and `self`, `parent` and named dependencies resolve in the declaring class's scope.
- Duplicate class-like constants use the last declaration's reflection; duplicate and conflict diagnostics use each
  declaration's location.
- Trait constants use the implementing class's scope; inherited non-private ones keep that identity when a subclass
  reuses the trait. Conflicts compare full initializer unions and array shapes, ignoring union member order.
- Unsupported initializer expressions are `mixed` in reflection; ordinary calls keep flow types. Initializer `??`
  removes explicit `null` without subtracting it from `mixed`.
- Named `new` initializers keep bare class identities without constructor-inferred generics; member lookups use
  template defaults and bounds. `new (Box::class)(1)` is `object`.
- Finite class-string and object holders resolve constants across their possible classes, including forward and
  scanned dependencies. Exact class strings keep raw initializers; object holders respect final classes and
  declarations.
- `parent::class` without a parent is `class-string` with an invalid-parent diagnostic; dynamic `NonexistentParent`
  holders are `class-string&literal-string`; other missing constants are the error type.
- Canonical joins keep reflection order, including the first `mixed` or error value. Reflected magic constants use
  PHPStan's reflection context; trait declaration and attribute checks use the consuming class's scope.
- Named class-constant reads keep keyed initializer array order; conditional initializers keep both alternatives with
  their key orders. Sealed array addition keeps right-operand-first key order and left-operand value precedence.
- Dynamic fetches like `Example::{$name}` check each name of a finite string selector, keep branch narrowing and report
  missing or inaccessible constants. Their value stays `mixed`. Unresolved named fetches give an error value, not
  `never`.
- `__COMPILER_HALT_OFFSET__` needs a parsed top-level `__halt_compiler()`; occurrences in comments, strings or inline
  HTML don't count.

## Classes and attributes

- Trait constructors keep adapted visibility, named-argument policies and parameter types in the consuming class's
  scope. Constructor template checks use the reflected return type (implicit `void` included), not the raw `@return`.
- Shared attribute checks keep explicit import aliases and evaluate parenthesized flag expressions.
- Dynamic `new $class` reports unknown class names even when another possible name exists. Invalid named
  instantiations keep their object types and constructor argument effects. An invalid target gives an error value and
  does not make later statements unreachable.
- An unresolved `new parent()` has PHPStan's `parent` type: no class name or members, does not satisfy `object`, and
  survives copies, arrays and unions. Its bool cast is `true`; other casts give error values.
- `get_class()` returns `class-string`; `::class` returns `class-string&literal-string` with no known value.
- `clone` keeps only object alternatives; cloning `$this` gives `static`. Invalid trait `instanceof` diagnostics
  describe the operand's inferred type.

## Types

- Constant-bound templates keep their identities next to literal scalars. Union descriptions use PHPStan's order,
  including distinct null-bound templates. Union method reflection keeps member order for the declaring class; printed
  descriptions stay sorted. Generic comparisons treat reordered union and intersection members as equal.
- Type compatibility treats `NAN` as equal to `NAN` and `0.0` as equal to `-0.0`, also in array shapes.
- At level 9, receiver checks validate explicit `mixed`; at level 10, implicit `mixed` too. Diagnostics keep template
  bounds and defaults. Static calls and first-class callables through `class-string<stdClass>` report non-object
  method errors; `class-string<mixed>` is unchecked.
- Full inference keeps explicit `mixed` from declarations, built-in returns, assignments and captures. Mixed offset
  reads keep explicitness but drop truthiness and exclusions. Unknown properties and methods are implicit `mixed`.
- Argument checks keep declared `mixed` inside generics and callable signatures. Generic callable arguments specialize
  their own templates first; generic class arguments respect variance. First-class generic callbacks keep conditional
  returns until their input types are known. Return checks resolve template-key array shapes first; own value
  templates keep their identity.
- Exact instantiated-class alternatives don't mark the classes final. Per-value reflection overrides survive
  conversion to PHPStan object types; template inference removes them.
- Array literals keep bounded integer and string key alternatives as optional constant slots. Repeated finite keys
  replace optional-slot values and widen required ones. Automatic indexes and optional constant unpacks follow
  PHPStan's builder limits.
- Array writes tell definite literal keys from literal alternatives within a general key. A false `isset` leaves the
  array unchanged when its nullable entry might be absent; a required nullable entry narrows to `null`. Failed offset
  reads don't fill the value cache, and later absence probes refresh presence. `??` scopes tell missing offsets from
  stored null.
- Continuing branch joins keep closed-array alternatives and drop offset facts missing from a continuing branch. Unions
  keep same-key shape alternatives when two required fields differ, but equal multi-key shapes merge across one
  differing field, and equal-key closed shapes can merge nullable nested values. Singleton shapes with disjoint values
  stay separate. Sequential optional-key joins merge equal values but keep tagged and non-plain empty-array
  alternatives. Offset reads keep incoming union order.
- Literal-string reads through an integer range of at most 128 values give the selected byte union, plus the empty
  string for partly valid ranges.

### PHPDoc

- Union lowering keeps literal alternatives and nullable boundaries without recursive stack growth over flat unions.
- Offset resolution keeps whether `mixed` was explicit but drops its exclusions; an offset of an error type is implicit
  `mixed`.
- Quoted array-shape keys are decoded to bytes (also through template substitution) before numeric-key conversion.
  Raw quoted scalars and keys keep non-UTF-8 bytes, and offsets count original bytes; diagnostic text may still
  substitute invalid UTF-8. Numeric string keys normalize before overwrite lookup and unsealed-extra exclusion.
- Optional non-list keys don't make a shape definitely non-list. In an unsealed shape, `...<bool>` gives the value type
  of extra integer or string keys, and `...<string, bool>` gives key and value types.
- Class, identifier and object-property names keep their text representation.

### Built-in functions

- `strlen()` uses PHP scalar casts and byte lengths: exact lengths or a contiguous range for literals, no enumeration
  of general ranges.
- `str_repeat()` gives exact literals when the output is under 100 bytes, else the applicable string refinements,
  including lowercase and uppercase at once. Repeating an unknown nonempty string twice gives only
  `non-empty-string`.
- String unions keep incomparable refinements (`lowercase-string|uppercase-string`) instead of widening to `string`.
  Numeric strings keep a separate `non-empty-string` only when inferred. Concatenating two nonempty strings gives
  `non-falsy-string`. Constant strings keep binary bytes.
- `array_rand()` keeps possible keys and returns a non-empty list for a count of at least two. `rand()`, `mt_rand()`
  and `random_int()` keep integer bounds; only `rand()` swaps reversed bounds.
- `filter_input()` and `filter_var()` with integer and IP filters keep failure values, array flags, defaults and range
  options. Other filters and `FILTER_THROW_ON_FAILURE` use the declaration.
- Literal `in_array()` constant-condition diagnostics use the constraints every comparison alternative shares, not the
  runtime membership result.
- `PHPStan\dumpType()` and `PHPStan\dumpPhpDocType()` report one non-ignorable diagnostic per argument on the call's
  line, in PHPStan's precise notation and as PHPDoc.

## Purity

- From level 2, `analyse/functions/invalid-pure` reports `void.pure` for supported functions and private methods whose
  bodies provably have no side effects.
- Named constructor calls use their reflected purity and throw declarations, including inherited constructors.
- Local `unset` keeps operand effects and evaluates an offset's index before its receiver. Plain-array targets keep
  index throw points apart from index impurity and termination.
- Purity checks include array template bounds for direct locals and single offsets. Offset reads and unsets add no
  `ArrayAccess` call when the receiver is definitely an array or definitely not `ArrayAccess`. Offset writes still need
  proven array receivers.

## Parser and literals

- Severe parser errors are non-ignorable. An invalid file adds no scanned symbols, and the output then holds only the
  parser errors.
- Pun reports a parser error for: invalid reserved or qualified special class names, duplicate imports, invalid class
  or member modifiers, variadic defaults, direct `void` parameters, `try` without `catch` or `finally`, unknown property
  hooks, getter parameter lists, empty hook lists, namespaces nested in a braced namespace, mixed namespace styles,
  misplaced namespace declarations, code outside braced namespaces, one attribute on several global constants, an
  unmatched `}` at compilation-unit entry (later declarations are rejected), an unparenthesized arrow function on the
  right of `|>` for PHP 8.5 targets, `$value =& new Thing()` (at the assignment start), trailing commas in `for`
  clauses and echo lists including short echo tags (at the comma), empty array elements (after namespace checks), and
  heredoc or nowdoc closing markers that mix tabs and spaces (at any target version).
- Opening tags inside leading shebang text stay visible to analysis and scanning. Named and direct anonymous classes
  respect pre-8.1 `readonly` tokenization.
- Heredoc and nowdoc bodies validate indentation before escape decoding. Fatal Unicode escape errors stop later
  reductions; malformed escapes are nonfatal. Locations follow PHP's string-escape newline counting.
- Malformed legacy octals such as `08` report `Invalid numeric literal` over the whole literal. Analysis keeps PHP's
  float reading of overflowing octal prefixes; lint and format keep strict syntax checks.
- Integer literals above the signed 64-bit maximum become floats from their original magnitude, with PHP's separate
  rounding for decimal and radix literals, also above `u128`. Source bytes and spans are unchanged.

## Not done yet

- Larastan: generic `ArrayAccess::offsetGet()` return types in PHPDoc offset resolution.
- Method calls with arguments and property-correlated receiver selection, in call observations.
- Dynamic callable result inference.
- Per-name filtered observations for dynamic method calls.
- Bindings of `@param-closure-this` from several possible receiver classes.
- Expression guards through dynamic function and nullsafe calls, indirect variables, dynamic property selectors and
  general type-dependent comparisons.
- Constants: general mixed and spread array ordering, expanded initializer cycles, dynamic initializer selectors,
  local-variable copies and array addition over conditional operands.
- Attribute rule support.
- General PHP syntax-error translation.
- Independent `compact()` rule selection, and whole-rule parity for `isset`.
- Global PHPDoc-certainty options.
- Compound argument-type and by-reference checking.
- General and union array unpacks keep their fallback; general array-union normalization.
- Whole-shape inference for open lists and optional nonempty prefixes.
- Native type dumps, scope dumps and the `PHPStan\Testing` assertion functions.
- Purity: dynamic constructor targets, built-in constructor throw extensions, conditional constructor policies,
  offset writes through references and nested or property roots, unknown effect producers, and full pure-function and
  pure-method validation.
- Lint return-type parity with the pinned Rector's PHPStan (see [oracle.md](oracle.md)).
