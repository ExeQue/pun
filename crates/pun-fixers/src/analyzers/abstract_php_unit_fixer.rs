use crate::analyzers::analysis::NamespaceUseType;
use crate::analyzers::attribute_analyzer::AttributeAnalyzer;
use crate::analyzers::fully_qualified_name_analyzer::FullyQualifiedNameAnalyzer;
use crate::analyzers::php_unit_test_case_analyzer::PhpUnitTestCaseAnalyzer;
use pun_tokens::Kind::*;
use pun_tokens::Tokens;

pub(crate) fn is_candidate(tokens: &Tokens) -> bool {
    tokens.is_all_token_kinds_found(&[T_CLASS, T_STRING])
}

pub(crate) fn apply_fix(
    tokens: &mut Tokens,
    mut apply_php_unit_class_fix: impl FnMut(&mut Tokens, usize, usize),
) {
    let analyzer = PhpUnitTestCaseAnalyzer;
    let mut before = tokens.count();
    while let Some((class_index, (start, end))) = analyzer.next_php_unit_class(tokens, before) {
        apply_php_unit_class_fix(tokens, start, end);
        before = class_index;
    }
}

pub(crate) fn is_test_attribute_present(tokens: &Tokens, index: usize) -> bool {
    let attribute_index = tokens
        .get_prev_token_of_kind(index, &["{".into(), T_ATTRIBUTE.into()], true)
        .unwrap();
    if !tokens[attribute_index].is_kind(T_ATTRIBUTE) {
        return false;
    }
    let fqn = FullyQualifiedNameAnalyzer::new(tokens);
    for attribute_analysis in AttributeAnalyzer::collect(tokens, attribute_index) {
        for attribute in &attribute_analysis.attributes {
            let name = fqn
                .get_fully_qualified_name(&attribute.name, attribute.start, NamespaceUseType::Class)
                .to_ascii_lowercase();
            if name == b"phpunit\\framework\\attributes\\test" {
                return true;
            }
        }
    }
    false
}

pub(crate) fn get_doc_block_index(tokens: &Tokens, mut index: usize) -> usize {
    loop {
        index = tokens.get_prev_non_whitespace(index, None).unwrap();
        if tokens[index].is_kind(CT_ATTRIBUTE_CLOSE) {
            index = tokens
                .get_prev_token_of_kind(index, &[T_ATTRIBUTE.into()], true)
                .unwrap();
        }
        if !tokens[index].is_given_kind(&[
            T_PUBLIC,
            T_PROTECTED,
            T_PRIVATE,
            T_FINAL,
            T_ABSTRACT,
            T_COMMENT,
            T_ATTRIBUTE,
            T_READONLY,
        ]) {
            return index;
        }
    }
}

pub(crate) struct AssertCall {
    pub index: usize,
    pub lowered_name: Vec<u8>,
    pub open_brace_index: usize,
}

pub(crate) struct PreviousAssertCalls {
    index: usize,
    start_index: usize,
    functions: crate::analyzers::functions_analyzer::FunctionsAnalyzer,
}

impl PreviousAssertCalls {
    pub(crate) fn new(start_index: usize, end_index: usize) -> Self {
        Self {
            index: end_index,
            start_index,
            functions: crate::analyzers::functions_analyzer::FunctionsAnalyzer::new(),
        }
    }

    pub(crate) fn next(&mut self, tokens: &Tokens) -> Option<AssertCall> {
        while self.index > self.start_index {
            let index = tokens.get_prev_token_of_kind(self.index, &[T_STRING.into()], true)?;
            self.index = index.saturating_sub(1);
            let lowered = pun_tokens::php_compat::strtolower(tokens[index].get_content());
            if !lowered.starts_with(b"assert") {
                continue;
            }
            let open_brace_index = tokens.get_next_meaningful_token(index).unwrap();
            if !tokens[open_brace_index].equals("(", true) {
                continue;
            }
            if !self.functions.is_the_same_class_call(tokens, index) {
                continue;
            }
            return Some(AssertCall {
                index,
                lowered_name: lowered,
                open_brace_index,
            });
        }
        None
    }
}

pub(crate) fn target_fulfills(candidate: &str, target: &str) -> bool {
    if candidate == "newest" {
        return true;
    }
    let parts = |v: &str| -> Vec<u32> { v.split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    parts(candidate) >= parts(target)
}

pub(crate) fn ensure_is_doc_block_with_annotation(
    tokens: &mut Tokens,
    index: usize,
    annotation: &str,
    preventing_annotations: &[&str],
    preventing_attributes: &[&str],
    line_end: &[u8],
) {
    let doc_block_index = get_doc_block_index(tokens, index);
    if is_prevented_by_attribute(tokens, index, preventing_attributes) {
        return;
    }
    if tokens[doc_block_index].is_kind(T_DOC_COMMENT) {
        update_doc_block_if_needed(
            tokens,
            doc_block_index,
            annotation,
            preventing_annotations,
            line_end,
        );
    } else {
        create_doc_block(tokens, doc_block_index, annotation, line_end);
    }
}

pub(crate) fn create_doc_block(
    tokens: &mut Tokens,
    doc_block_index: usize,
    annotation: &str,
    line_end: &[u8],
) {
    use crate::analyzers::whitespaces_analyzer::WhitespacesAnalyzer;
    use pun_tokens::Token;
    let next = tokens
        .get_next_non_whitespace(doc_block_index, None)
        .unwrap();
    let original_indent = WhitespacesAnalyzer::detect_indent(tokens, next);
    let doc = [
        b"/**",
        line_end,
        &original_indent,
        b" * @",
        annotation.as_bytes(),
        line_end,
        &original_indent,
        b" */",
    ]
    .concat();
    let index = tokens.get_next_meaningful_token(doc_block_index).unwrap();
    tokens.insert_at(
        index,
        vec![
            Token::new(T_DOC_COMMENT, doc),
            Token::new(T_WHITESPACE, [line_end, &original_indent].concat()),
        ],
    );
    if !tokens[index - 1].is_kind(T_WHITESPACE) {
        let mut extra_new_lines = line_end.to_vec();
        if !tokens[index - 1].is_kind(T_OPEN_TAG) {
            extra_new_lines.extend_from_slice(line_end);
        }
        extra_new_lines.extend(WhitespacesAnalyzer::detect_indent(tokens, index));
        tokens.insert_at(index, vec![Token::new(T_WHITESPACE, extra_new_lines)]);
    }
}

fn update_doc_block_if_needed(
    tokens: &mut Tokens,
    doc_block_index: usize,
    annotation: &str,
    preventing_annotations: &[&str],
    line_end: &[u8],
) {
    use crate::analyzers::docblock::DocBlock;
    use crate::analyzers::whitespaces_analyzer::WhitespacesAnalyzer;
    use pun_tokens::Token;
    let mut doc = DocBlock::new(tokens[doc_block_index].get_content());
    for preventing in preventing_annotations {
        if !doc.get_annotations_of_type(&[*preventing]).is_empty() {
            return;
        }
    }
    if doc.get_lines().len() == 1 && doc.get_annotations_of_type(&[annotation]).is_empty() {
        let next = tokens
            .get_next_non_whitespace(doc_block_index, None)
            .unwrap();
        let indent = WhitespacesAnalyzer::detect_indent(tokens, next);
        doc.make_multi_line(&indent, line_end);
    }
    let original_indent = WhitespacesAnalyzer::detect_indent(tokens, doc_block_index);
    let mut lines: Vec<Vec<u8>> = doc
        .get_lines()
        .iter()
        .map(|l| l.borrow().get_content().to_vec())
        .collect();
    let at = lines.len() - 1;
    lines.insert(
        at,
        [
            &original_indent[..],
            b" * @",
            annotation.as_bytes(),
            line_end,
        ]
        .concat(),
    );
    tokens.set(doc_block_index, Token::new(T_DOC_COMMENT, lines.concat()));
}

fn is_prevented_by_attribute(
    tokens: &Tokens,
    mut index: usize,
    preventing_attributes: &[&str],
) -> bool {
    if preventing_attributes.is_empty() {
        return false;
    }
    loop {
        index = tokens.get_prev_meaningful_token(index).unwrap();
        if !tokens[index].is_given_kind(&[T_FINAL, T_READONLY]) {
            break;
        }
    }
    if !tokens[index].is_kind(CT_ATTRIBUTE_CLOSE) {
        return false;
    }
    let index = tokens.find_block_start(Tokens::BLOCK_TYPE_ATTRIBUTE, index);
    let fqn = FullyQualifiedNameAnalyzer::new(tokens);
    AttributeAnalyzer::collect(tokens, index)
        .iter()
        .any(|analysis| {
            analysis.attributes.iter().any(|attribute| {
                let name = fqn.get_fully_qualified_name(
                    &attribute.name,
                    attribute.start,
                    NamespaceUseType::Class,
                );
                preventing_attributes
                    .iter()
                    .any(|p| p.as_bytes() == pun_tokens::php_compat::strtolower(&name))
            })
        })
}
