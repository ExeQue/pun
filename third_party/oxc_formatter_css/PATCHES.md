# oxc_formatter_css 0.80.0: maintained patch

## Source

`src/` and `LICENSE` come from oxc revision
[`846e62ab`](https://github.com/oxc-project/oxc/tree/846e62abbbf7c646844a4b8729210dbe2317f5c7/crates/oxc_formatter_css)
(tag `oxfmt_v0.80.0`). The manifest names the workspace dependencies it inherited there; the
oxc crates stay on that revision. Tests, fixtures, the build script and lints are omitted. The
root manifest depends on this copy and excludes its directory from Pun's workspace members.

Pun's native Blade formatter matches Prettier 3.10.1 byte for byte, so each patch below reverts
one of oxc's deliberate differences from Prettier (`DIVERGENCES.md` upstream).

## Signed items break the list

Upstream `signed-value-resplit`. postcss-values splits a signed item past the first position of
a top-level comma list into an operator and a word (`Arial, -apple-system`, `50px, -3em`), so
Prettier's `shouldBreakList` puts every item on its own line when the list breaks. oxc keeps the
fill. `print/value.rs` `comma_group_is_signed` restores Prettier's reading. A custom property's
value (`--x: a, -b`) and `--`-prefixed items keep the fill, as in Prettier.
