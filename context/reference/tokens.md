# pun-tokens

Package `punphp-tokens`, imported as `pun_tokens`. It holds a lexer that matches `token_get_all` and the token layer
the fixers run on, with the behaviour of PHP-CS-Fixer v3.95.25 (the version bundled in the oracle Pint).

## Public API

Method names follow PHP-CS-Fixer's token API, snake_cased, so a fixer reads like its upstream counterpart. Indices are
`usize`; an optional index is `Option<usize>`. Ranges are inclusive.

- **Kinds.** `Kind::T_STRING`, `Kind::CT_ARRAY_BRACKET_OPEN`; `kind.name()` gives `"T_STRING"` or
  `"CT::T_ARRAY_BRACKET_OPEN"`; `Kind::from_name`. Alias: `Kind::T_PAAMAYIM_NEKUDOTAYIM`.
- **Token.** `Token { id: Option<Kind>, content: Content }`, `Token::new(kind, content)`, `Token::char("(")`.
  Queries: `equals(proto, case_sensitive)`, `equals_any(&[..], cs)`, `equals_token(&Token, cs)`, `is_kind`,
  `is_given_kind`, `is_array`, `is_comment`, `is_whitespace(Option<&[u8]>)`, `is_cast`, `is_classy`,
  `is_object_operator`, `is_keyword`, `is_magic_constant`, `is_native_constant`, `get_id`, `get_content`, `get_name`.
- **Proto** (token prototype): `"(".into()`, `T_STRING.into()`, `(T_STRING, "foo").into()`.
- **Tokens: build and print.** `Tokens::from_code(&[u8])`, `from_array(Vec<Token>)`, `set_code`,
  `generate_code() -> Vec<u8>`, `generate_partial_code(start, end)`.
- **Tokens: edit.** `tokens[i]` reads; `tokens.set(i, token)` writes and keeps the found-kinds and block caches
  current. `len`/`count`, `iter`, `offset_exists`, `is_empty_at`, `clear_at`, `clear_range`,
  `override_range(start, end, Vec<Token>)`, `insert_at(i, Vec<Token>)`, `insert_slices(BTreeMap<usize, Vec<Token>>)`,
  `clear_empty_tokens`, `clear_token_and_merge_surrounding_whitespace`, `ensure_whitespace_at_index(i, offset, ws) ->
  bool`, `remove_leading_whitespace(i, ws)`, `remove_trailing_whitespace(i, ws)`.
- **Tokens: navigate.** `get_next_meaningful_token`, `get_prev_meaningful_token`, `get_meaningful_token_sibling(i,
  dir)`, `get_next/prev_non_whitespace(i, ws)`, `get_non_whitespace_sibling`, `get_next/prev_token_of_kind(i,
  &[Proto], cs)`, `get_token_of_kind_sibling`, `get_token_not_of_kind_sibling(i, dir, &[Proto])`,
  `get_token_not_of_kinds_sibling(i, dir, &[Kind])`, `get_non_empty_sibling`.
- **Tokens: blocks.** `find_block_end(Tokens::BLOCK_TYPE_PARENTHESIS, i)`, `find_block_start`,
  `Tokens::detect_block_type(&token) -> Option<(BlockType, is_start)>`, `Tokens::block_edges(t)`. They take `&self`;
  the cache is interior-mutable.
- **Tokens: search.** `find_given_kind(k, start, end) -> Vec<usize>`,
  `find_given_kinds(&[k], ..) -> BTreeMap<Kind, Vec<usize>>`,
  `find_sequence(&[Proto], start, end, |key| cs) -> Option<Vec<usize>>` (indices, not tokens),
  `is_token_kind_found(Kind | "content")`, `is_any_token_kinds_found`, `is_all_token_kinds_found`, `count_token_kind`,
  `is_changed`, `clear_changed`, `is_monolithic_php`, `is_partial_code_multiline`, `has_alternative_syntax`.
- **TokensAnalyzer.** `TokensAnalyzer::new(&tokens)`: `get_classy_elements() -> BTreeMap<index, ClassyElement>`,
  `get_classy_modifiers`, `get_import_use_indexes(per_ns)`, `is_array`, `is_array_multi_line`, `is_block_multiline`,
  `get_method_attributes`, `is_anonymous_class`, `is_lambda`, `get_last_token_index_of_arrow_function`,
  `is_unary_successor_operator`, `is_unary_predecessor_operator`, `is_binary_operator`, `is_while_part_of_do_while`,
  `is_enum_case`, `is_super_global`.
- **Transformers.** `transformers::tokenize_name` splits a name into its tokens.
- **php_compat.** `strlen`, `strtolower`, `strtoupper`, `strcmp`, `strcasecmp`, `strnatcmp`, `strnatcasecmp`, `trim`,
  `rtrim`, `rtrim_chars`, `ltrim_chars`: PHP's string functions, byte-oriented, with ASCII-only case mapping.
- **Lexer.** `tokenize(&[u8]) -> Vec<RawToken { kind, text, line }>` is `token_get_all`;
  `apply_parse_mode(&mut [RawToken])` applies its `TOKEN_PARSE` retagging.

Where upstream throws on code it cannot handle (a null index after a failed lookup, an unbalanced block in a
transformer), Pun panics at the same point. Callers that want to skip such a file wrap `from_code` in
`catch_unwind`.

## Representation choices

- **Bytes, not `String`.** `Content` is bytes, inline up to 22 bytes, else a boxed slice. PHP source may be invalid
  UTF-8 or hold binary after `__halt_compiler`, and PHP's string functions are byte-oriented anyway.
- **Kinds by name, not number.** `Kind` is a `u16` enum, because PHP's token numbers change between versions and the
  names don't. `id: None` is a single-character token whose identity is its content (`'('`, `'b"'`, the empty cleared
  token).
- **Caches kept.** The found-kinds registry and the block-edge cache follow upstream's invalidation rules, because a
  stale cache is observable. The static code-hash cache is left out; it is only an optimization.
- **TOKEN_PARSE.** Under `TOKEN_PARSE` the parser retags semi-reserved keywords used as identifiers (`Foo::class`,
  `function list()`, `case Default`, named args `default:`) to `T_STRING`. `parse_mode.rs` reproduces this with a
  heuristic over neighbouring tokens that covers every such grammar position. It does not detect parse errors.
- **Transformer order.** Priority descending; ties in the order the oracle machine reports. No transformer pair with
  equal priority interacts on the corpus.

## Lexer: Pun's own, not mago-syntax

`lexer.rs` follows PHP's own scanner as `token_get_all` drives it. Mago's lexer serves a parser and differs from
`token_get_all` in structure, not only in naming: newline handling after open and close tags, comments before `?>`,
property-name context, the ampersand tokens, `T_BAD_CHARACTER`, the `__halt_compiler` tail, integer overflow, casts,
unterminated constructs, keyword context and line numbers. A mapping layer would have rebuilt most of the scanner on
top of Mago's output. Mago stays the choice for parsing and analysis elsewhere in Pun.

Assumptions: `short_open_tag=Off` (the php.ini default) and the PHP 8.5 token set.

## Parity

`cargo run --release -p punphp-tokens --example parity -- --mode raw|parse|fixer|classy <path>...` runs
`tools/dump.php` under system PHP 8.5.11 and diffs every `.php` file (`vendor/` and `node_modules/` skipped):

- `raw`: (kind, text, line) against `token_get_all`;
- `parse`: against `token_get_all(.., TOKEN_PARSE)`;
- `fixer`: (kind incl. `CT::`, content) against PHP-CS-Fixer's tokens from `.oracle/php-cs-fixer`;
- `classy`: the classy elements.

`cargo test -p punphp-tokens` runs every mode on `tests/fixtures/`, which covers heredoc and nowdoc, inline HTML, `?>`
at EOF, `<?=`, `__halt_compiler` with a binary tail, CRLF, BOM, invalid UTF-8, attributes, property hooks, asymmetric
visibility, casts, number overflow, unterminated constructs, TOKEN_PARSE demotions and every CT kind.

Results (files identical / total; "skipped" means PHP itself rejects the file under TOKEN_PARSE):

| corpus | files | tokens | raw | parse | fixer (post-transform) |
|---|---|---|---|---|---|
| pinkary (the app directories and Blade views; the whole checkout gives the same result) | 483 | 163,731 | 483/483 | 484/484 | 484/484 |
| laravel/framework @86864f3 | 3,158 | 3,382,340 | 3,158/3,158 | 3,157/3,157 (1 skipped) | 3,157/3,157 (1 skipped) |
| pestphp/pest | 633 | 294,631 | 633/633 | 633/633 | 633/633 |
| larastan | 579 | 232,837 | 579/579 | 579/579 | 579/579 |
| laravel/pint | 336 | 70,850 | 336/336 | 335/335 (1 skipped) | 335/335 (1 skipped) |
| laravel/laravel | 27 | 6,176 | 27/27 | 27/27 | 27/27 |
| tests/fixtures | 23 | | 23/23 | all parseable identical | all parseable identical |

No token mismatches in any mode. `get_classy_elements` matches upstream on every file checked.

Timings (release, one thread, laravel/framework `src/`, 1.12M tokens): lexing 65 ms; `Tokens::from_code` (lex,
TOKEN_PARSE and 20 transformers) about 340 ms. PHP's `token_get_all` loop takes about 1.8 s and PHP-CS-Fixer's
tokenizer about 10 s on the same files.

## Known gaps

- Constant-invocation detection in `TokensAnalyzer` and the other token analyzers (namespaces, imports, functions,
  arguments, attributes) are not in pun-tokens yet.
- `short_open_tag=On` is not supported.
- `strcmp` returns only the sign (PHP returns libc `memcmp`'s raw value); `strcasecmp` returns PHP's exact byte
  difference.
- Transformer tie order inside Pint's PHAR is assumed equal to the source checkout's.
