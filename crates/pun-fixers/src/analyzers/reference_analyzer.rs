use pun_tokens::{Kind::*, Proto, Tokens};

pub(crate) struct ReferenceAnalyzer;

impl ReferenceAnalyzer {
    pub(crate) fn is_reference(&self, tokens: &Tokens, index: usize) -> bool {
        if !tokens[index].equals("&", true) {
            return false;
        }
        let mut index = tokens.prev_meaningful(index);
        if tokens[index].equals_any(
            &[
                Proto::Char(b"="),
                Proto::Kind(T_AS),
                Proto::Kind(T_CALLABLE),
                Proto::Kind(T_DOUBLE_ARROW),
                Proto::Kind(CT_ARRAY_TYPEHINT),
            ],
            true,
        ) {
            return true;
        }
        if tokens[index].is_kind(T_STRING) {
            index = tokens.prev_meaningful(index);
        }
        tokens[index].equals_any(
            &[
                Proto::Char(b"("),
                Proto::Char(b","),
                Proto::Kind(T_NS_SEPARATOR),
                Proto::Kind(CT_NULLABLE_TYPE),
            ],
            true,
        )
    }
}
