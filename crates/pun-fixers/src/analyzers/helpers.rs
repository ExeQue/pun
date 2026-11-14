use pun_tokens::Kind::{self, *};
use pun_tokens::Tokens;

pub(crate) const COMPOUND_ASSIGNMENTS: &[Kind] = &[
    T_COALESCE_EQUAL,
    T_PLUS_EQUAL,
    T_MINUS_EQUAL,
    T_MUL_EQUAL,
    T_DIV_EQUAL,
    T_CONCAT_EQUAL,
    T_MOD_EQUAL,
    T_POW_EQUAL,
    T_AND_EQUAL,
    T_OR_EQUAL,
    T_XOR_EQUAL,
    T_SL_EQUAL,
    T_SR_EQUAL,
];

pub(crate) fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    pun_tokens::php_compat::strpos(haystack, needle, 0).is_some()
}

pub(crate) fn split_list(tokens: &Tokens, open: usize, close: usize) -> Vec<(usize, usize)> {
    let mut items = vec![];
    let mut k = tokens.next_meaningful(open);
    while k < close {
        let start = k;
        let mut last = k;
        while k < close && !tokens[k].equals(",", true) {
            if let Some((t, true)) = Tokens::detect_block_type(&tokens[k]) {
                k = tokens.find_block_end(t, k);
            }
            last = k;
            k = tokens.next_meaningful(k);
        }
        items.push((start, last));
        if k < close {
            k = tokens.next_meaningful(k);
        }
    }
    items
}
