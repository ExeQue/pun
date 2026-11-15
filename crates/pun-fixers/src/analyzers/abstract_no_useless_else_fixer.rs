use pun_tokens::{Kind::*, Tokens};

pub(crate) const PRIORITY: i32 = 39;

pub(crate) fn is_superfluous_else(tokens: &Tokens, index: usize) -> bool {
    let mut previous_block_start = index;
    loop {
        let (start, previous_block_end) = get_previous_block(tokens, previous_block_start);
        previous_block_start = start;

        let mut previous = previous_block_end;
        if tokens[previous].equals("}", true) {
            previous = tokens.prev_meaningful(previous);
        }
        if !tokens[previous].equals(";", true)
            || tokens[tokens.prev_meaningful(previous)].equals("{", true)
        {
            return false;
        }

        let candidate_index = tokens.get_prev_token_of_kind(
            previous,
            &[
                ";".into(),
                T_BREAK.into(),
                T_CLOSE_TAG.into(),
                T_CONTINUE.into(),
                T_EXIT.into(),
                T_GOTO.into(),
                T_IF.into(),
                T_RETURN.into(),
                T_THROW.into(),
            ],
            true,
        );
        let Some(candidate_index) = candidate_index.filter(|&c| {
            !tokens[c].equals_any(&[";".into(), T_CLOSE_TAG.into(), T_IF.into()], true)
        }) else {
            return false;
        };

        if tokens[candidate_index].is_kind(T_THROW) {
            let previous_index = tokens.prev_meaningful(candidate_index);
            if !tokens[previous_index].equals_any(&[";".into(), "{".into()], true) {
                return false;
            }
        }

        if is_in_conditional(tokens, candidate_index, previous_block_start)
            || is_in_condition_without_braces(tokens, candidate_index, previous_block_start)
        {
            return false;
        }

        if tokens[previous_block_start].is_kind(T_IF) {
            return true;
        }
    }
}

fn get_previous_block(tokens: &Tokens, index: usize) -> (usize, usize) {
    let close = tokens.prev_meaningful(index);
    let mut previous = close;
    if tokens[close].equals("}", true) {
        previous = tokens.find_block_start(Tokens::BLOCK_TYPE_BRACE, close);
    }
    let mut open = tokens
        .get_prev_token_of_kind(
            previous,
            &[T_IF.into(), T_ELSE.into(), T_ELSEIF.into()],
            true,
        )
        .unwrap();
    if tokens[open].is_kind(T_IF) {
        let else_candidate = tokens.prev_meaningful(open);
        if tokens[else_candidate].is_kind(T_ELSE) {
            open = else_candidate;
        }
    }
    (open, close)
}

fn is_in_conditional(tokens: &Tokens, index: usize, lower_limit_index: usize) -> bool {
    let candidate_index = tokens
        .get_prev_token_of_kind(index, &[")".into(), ";".into(), ":".into()], true)
        .unwrap();
    if tokens[candidate_index].equals(":", true) {
        return true;
    }
    if !tokens[candidate_index].equals(")", true) {
        return false;
    }
    let open = tokens.find_block_start(Tokens::BLOCK_TYPE_PARENTHESIS, candidate_index);
    tokens
        .get_prev_meaningful_token(open)
        .is_some_and(|p| p > lower_limit_index)
}

fn is_in_condition_without_braces(
    tokens: &Tokens,
    mut index: usize,
    lower_limit_index: usize,
) -> bool {
    loop {
        if tokens[index].is_comment() || tokens[index].is_whitespace(None) {
            index = tokens.prev_meaningful(index);
        }
        let token = &tokens[index];
        if token.is_given_kind(&[T_IF, T_ELSEIF, T_ELSE]) {
            return true;
        }
        if token.equals(";", true) {
            return false;
        }
        if token.equals("{", true) {
            return false;
        }
        if token.equals(")", true) {
            let (t, _) = Tokens::detect_block_type(token).unwrap();
            index = tokens.find_block_start(t, index);
            index = tokens.prev_meaningful(index);
        } else {
            index -= 1;
        }
        if index <= lower_limit_index {
            return false;
        }
    }
}
