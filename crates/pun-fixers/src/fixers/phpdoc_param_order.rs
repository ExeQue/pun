use crate::analyzers::docblock::type_expression::REGEX_TYPES;
use crate::analyzers::docblock::{Annotation, DocBlock};
use crate::analyzers::preg::Preg;
use pun_format::Fixer;
use pun_tokens::Kind::*;
use pun_tokens::php_compat::preg_quote;
use pun_tokens::{BlockType, Token, Tokens};
use std::path::Path;

struct PhpdocParamOrderFixer;

pub(crate) fn fixer() -> Box<dyn Fixer> {
    Box::new(PhpdocParamOrderFixer)
}

impl Fixer for PhpdocParamOrderFixer {
    fn name(&self) -> &'static str {
        "phpdoc_param_order"
    }
    fn priority(&self) -> i32 {
        0
    }
    fn is_candidate(&self, tokens: &Tokens) -> bool {
        tokens.is_token_kind_found(T_DOC_COMMENT)
    }
    fn fix(&self, _file: &Path, tokens: &mut Tokens) {
        for index in 0..tokens.count() {
            if !tokens[index].is_kind(T_DOC_COMMENT) {
                continue;
            }
            if tokens
                .get_next_token_of_kind(index, &[T_FUNCTION.into(), T_FN.into()], true)
                .is_none()
            {
                return;
            }
            let param_block_start = tokens
                .get_next_token_of_kind(index, &["(".into()], true)
                .unwrap();
            let doc = DocBlock::new(tokens[index].get_content());
            let param_annotations = doc.get_annotations_of_type(&["param"]);
            if param_annotations.is_empty() {
                continue;
            }
            let param_names = get_function_param_names(tokens, param_block_start);
            rewrite_doc_block(&doc, &param_names, &param_annotations);
            tokens.set(index, Token::new(T_DOC_COMMENT, doc.get_content()));
        }
    }
}

fn get_function_param_names(tokens: &Tokens, param_block_start: usize) -> Vec<Vec<u8>> {
    let param_block_end = tokens.find_block_end(BlockType::Parenthesis, param_block_start);
    let mut names = vec![];
    let mut i = tokens.get_next_token_of_kind(param_block_start, &[T_VARIABLE.into()], true);
    while let Some(index) = i.filter(|&i| i < param_block_end) {
        names.push(tokens[index].get_content().to_vec());
        i = tokens.get_next_token_of_kind(index, &[T_VARIABLE.into()], true);
    }
    names
}

fn rewrite_doc_block(doc: &DocBlock, param_names: &[Vec<u8>], params: &[&Annotation]) {
    let mut ordered = sort_param_annotations(param_names, params);
    let params_start = params[0].get_start();
    let params_end = params[params.len() - 1].get_end();
    let between = || {
        doc.get_annotations()
            .iter()
            .filter(move |a| a.get_start() >= params_start && a.get_end() <= params_end)
    };
    ordered.extend(
        between()
            .filter(|a| a.get_tag().get_name() != b"param")
            .map(Annotation::get_content),
    );
    for (annotation, content) in between().zip(ordered) {
        annotation.remove();
        doc.get_line(annotation.get_start())
            .unwrap()
            .borrow_mut()
            .set_content(content);
    }
}

fn sort_param_annotations(names: &[Vec<u8>], params: &[&Annotation]) -> Vec<Vec<u8>> {
    let mut valid: Vec<(usize, Vec<u8>)> = vec![];
    for name in names {
        for index in find_param_annotation_by_identifier(params, name) {
            let content = params[index].get_content();
            match valid.iter_mut().find(|(i, _)| *i == index) {
                Some(e) => e.1 = content,
                None => valid.push((index, content)),
            }
        }
    }
    let invalid: Vec<Vec<u8>> = (0..params.len())
        .filter(|i| !valid.iter().any(|(v, _)| v == i))
        .map(|i| params[i].get_content())
        .collect();
    let mut ordered: Vec<Vec<u8>> = valid.into_iter().map(|(_, c)| c).collect();
    ordered.extend(invalid);
    ordered
}

fn find_param_annotation_by_identifier(params: &[&Annotation], identifier: &[u8]) -> Vec<usize> {
    let mut block_level: isize = 0;
    let mut block_match = false;
    let mut block_indices = vec![];
    let param_regex = [
        &br"/\*\h*@param\h*(?:|"[..],
        REGEX_TYPES.as_bytes(),
        br"\h*)&?(?=\$\b)",
        &preg_quote(identifier, None),
        br"\b/",
    ]
    .concat();
    for (i, param) in params.iter().enumerate() {
        let content = param.get_content();
        let block_start = Preg::r#match(r"/\s*{\s*/", &content);
        let block_end_matches = Preg::match_all(r"/}[\*\s\n]*/", &content).len() as isize;
        if block_level == 0 && Preg::match_groups_uncached(&param_regex, &content).is_some() {
            if !block_start {
                return vec![i];
            }
            block_match = true;
        }
        if block_start {
            block_level += 1;
        }
        block_level -= block_end_matches;
        if block_match {
            block_indices.push(i);
            if block_level == 0 {
                return block_indices;
            }
        }
    }
    vec![]
}
