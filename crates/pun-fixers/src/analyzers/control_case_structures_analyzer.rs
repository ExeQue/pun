use pun_tokens::{Kind, Kind::*, Tokens};

const SUPPORTED_TYPES_WITH_CASE_OR_DEFAULT: &[Kind] = &[T_SWITCH, T_MATCH, T_ENUM];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CaseAnalysis {
    index: usize,
    colon_index: usize,
}

impl CaseAnalysis {
    pub(crate) fn get_colon_index(&self) -> usize {
        self.colon_index
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DefaultAnalysis {
    index: usize,
    colon_index: usize,
}

impl DefaultAnalysis {
    pub(crate) fn get_index(&self) -> usize {
        self.index
    }
    pub(crate) fn get_colon_index(&self) -> usize {
        self.colon_index
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ControlCaseStructuresAnalysis {
    open_index: usize,
    cases: Vec<CaseAnalysis>,
    default: Option<DefaultAnalysis>,
}

impl ControlCaseStructuresAnalysis {
    pub(crate) fn get_open_index(&self) -> usize {
        self.open_index
    }
    pub(crate) fn get_cases(&self) -> &[CaseAnalysis] {
        &self.cases
    }
    pub(crate) fn get_default_analysis(&self) -> Option<DefaultAnalysis> {
        self.default
    }
}

struct Frame {
    kind: Kind,
    brace_count: i64,
    cases: Vec<(usize, usize)>,
    default: Option<(usize, usize)>,
    alternative_syntax: bool,
    open: usize,
}

pub(crate) struct ControlCaseStructuresAnalyzer;

impl ControlCaseStructuresAnalyzer {
    pub(crate) fn find_switch_structures(tokens: &Tokens) -> Vec<ControlCaseStructuresAnalysis> {
        let mut out = Vec::new();
        if !tokens.is_token_kind_found(T_SWITCH) {
            return out;
        }

        let mut stack: Vec<Frame> = Vec::new();
        let mut is_type_of_interest = false;

        let mut index = 0;
        while index < tokens.count() {
            let token = &tokens[index];
            if token.is_given_kind(SUPPORTED_TYPES_WITH_CASE_OR_DEFAULT) {
                let kind = token.get_id().unwrap();
                let mut frame = Frame {
                    kind,
                    brace_count: 0,
                    cases: Vec::new(),
                    default: None,
                    alternative_syntax: false,
                    open: 0,
                };
                is_type_of_interest = kind == T_SWITCH;
                if kind == T_SWITCH {
                    let i = tokens.next_meaningful(index);
                    let i = tokens.find_block_end(Tokens::BLOCK_TYPE_PARENTHESIS, i);
                    frame.open = tokens.next_meaningful(i);
                    frame.alternative_syntax = tokens[frame.open].equals(":", true);
                }
                stack.push(frame);
                index += 1;
                continue;
            }

            let Some(top) = stack.last_mut() else {
                index += 1;
                continue;
            };

            if token.equals("{", true) {
                top.brace_count += 1;
                index += 1;
                continue;
            }

            if token.equals("}", true) {
                top.brace_count -= 1;
                if top.brace_count == 0 && !top.alternative_syntax {
                    let frame = stack.pop().unwrap();
                    if is_type_of_interest {
                        out.push(Self::build_switch_analysis(frame));
                    }
                    is_type_of_interest = stack.last().is_some_and(|f| f.kind == T_SWITCH);
                }
                index += 1;
                continue;
            }

            let mut current = index;
            if token.is_kind(T_ENDSWITCH) {
                assert!(
                    top.alternative_syntax,
                    "Analysis syntax failure, unexpected \"T_ENDSWITCH\"."
                );
                assert!(
                    top.kind == T_SWITCH,
                    "Analysis type failure, unexpected \"T_ENDSWITCH\"."
                );
                assert!(
                    top.brace_count == 0,
                    "Analysis count failure, unexpected \"T_ENDSWITCH\"."
                );
                current = tokens
                    .get_next_token_of_kind(index, &[";".into(), T_CLOSE_TAG.into()], true)
                    .unwrap();
                let frame = stack.pop().unwrap();
                out.push(Self::build_switch_analysis(frame));
                is_type_of_interest = stack.last().is_some_and(|f| f.kind == T_SWITCH);
            }

            if !is_type_of_interest {
                index += 1;
                continue;
            }

            let top = stack
                .last_mut()
                .expect("case outside of a control structure");
            if token.is_kind(T_CASE) {
                let open = Self::find_case_open(tokens, current);
                top.cases.push((current, open));
            } else if token.is_kind(T_DEFAULT) {
                assert!(
                    top.default.is_none(),
                    "Analysis multiple \"default\" found."
                );
                let open = tokens
                    .get_next_token_of_kind(current, &[":".into(), ";".into()], true)
                    .unwrap();
                top.default = Some((current, open));
            }
            index += 1;
        }
        out
    }

    fn build_switch_analysis(frame: Frame) -> ControlCaseStructuresAnalysis {
        let default = frame
            .default
            .map(|(index, colon_index)| DefaultAnalysis { index, colon_index });
        let mut cases: Vec<CaseAnalysis> = frame
            .cases
            .iter()
            .map(|&(index, colon_index)| CaseAnalysis { index, colon_index })
            .collect();
        cases.sort();
        ControlCaseStructuresAnalysis {
            open_index: frame.open,
            cases,
            default,
        }
    }

    fn find_case_open(tokens: &Tokens, mut index: usize) -> usize {
        let mut ternaries_count = 0;
        loop {
            if tokens[index].equals_any(&["(".into(), "{".into()], true) {
                let (t, _) = Tokens::detect_block_type(&tokens[index]).unwrap();
                index = tokens.find_block_end(t, index) + 1;
                continue;
            }
            if tokens[index].equals("?", true) {
                ternaries_count += 1;
                index += 1;
                continue;
            }
            if tokens[index].equals_any(&[":".into(), ";".into()], true) {
                if ternaries_count == 0 {
                    return index;
                }
                ternaries_count -= 1;
            }
            index += 1;
        }
    }
}
