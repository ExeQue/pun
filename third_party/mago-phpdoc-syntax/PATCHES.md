# Mago PHPDoc syntax 1.61.0: maintained patch

## Source

The library source and normalized manifest come from the published `mago-phpdoc-syntax` 1.61.0
package (checksum `283a30eef27d2c6ccda22898f36eee66fc76f6b81d9806815d1d529ac7a33e44`). Its
`.cargo_vcs_info.json` identifies upstream revision
[`08934a6e`](https://github.com/carthage-software/mago/tree/08934a6e3925cc1c836041efc931505330911f6e/crates/phpdoc-syntax).
Both license files come from that revision. Registry bookkeeping, the package lockfile, and
external tests are omitted, along with their manifest targets. The `indoc` development dependency
remains because the upstream lexer unit tests use it. Sibling Mago dependencies stay pinned to
`=1.61.0`. The root manifest patches this crate and excludes it from Pun's workspace members.

## Quoted shape-key source tokens

`ShapeKey::String::raw` retains the complete quoted token, including its delimiters and escapes.
The quoted constructor stores `Some(token.value)`. The three bare constructors (float, signed
float, and identifier) store `None`. The existing `value` slice, spans, display, and parsing rules
are unchanged. This follows the existing `LiteralStringType` raw-token representation.

Codex's array-key builder uses this metadata to decode quoted keys with PHPStan's PHPDoc escape
grammar before interning their semantic bytes and applying its existing numeric-key conversion.
Object-shape properties still use the existing `value` field. The parser does not decode strings
or depend on Pun's semantic decoder.

Public scanner, inline-annotation, and duplicate-key controls live in
`crates/pun-analyse/tests/duplicate_array_keys.rs`. The fixtures and reproducible PHPStan 2.3.1
capture under `tests/fixtures/duplicate-array-keys/` retain hexadecimal key and value bytes separately
from UTF-8 diagnostic presentation. Bare float spellings are parser controls, not a claim that
PHPStan accepts float keys in PHPDoc shapes.
