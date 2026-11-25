//! `[analyse] php-rules` (prototype): runs the worker on one checked file, answering its `getType()` questions
//! with the types the checker inferred.

use std::path::Path;

use mago_span::{Position, Span};
use pun_phpstan::ignore::Error;
use pun_phpstan::types::to_phpdoc_fully_qualified;

use super::Checker;

pub(super) fn check(c: &Checker, path: &Path, rules: &crate::PhpRules) -> Vec<Error> {
    let context = (c.trait_context.as_ref()).map(|t| (t.class.as_str(), t.trait_name.as_str()));
    let found = rules.check(path, context, |start, end| {
        // ponytail: only spans Pun recorded a type for answer (driver.rs asks for every expression's); a span the
        // two parsers cut differently answers mixed
        let span = Span::new(c.file.id, Position::new(start), Position::new(end));
        c.phpstan_type_at(span)
            .map(|t| to_phpdoc_fully_qualified(&t))
    });
    (found.into_iter())
        .map(|f| {
            let mut e = Error::new(f.message, c.label.clone(), Some(f.line));
            e.identifier = f.identifier;
            e.tip = f.tip;
            e.can_be_ignored = !f.non_ignorable;
            if let Some(t) = &c.trait_context {
                e.file_path.clone_from(&t.class_file);
                e.trait_file_path = Some(c.file_name());
            }
            e
        })
        .collect()
}

/// Every expression's span, for the worker's `getType()`.
pub(super) fn expressions(program: &mago_syntax::cst::Program) -> Vec<Span> {
    use mago_span::HasSpan;
    use mago_syntax::cst::Expression;
    use mago_syntax::walker::MutWalker;
    struct All(Vec<Span>);
    impl<'ast, 'arena> MutWalker<'ast, 'arena, ()> for All {
        fn walk_in_expression(&mut self, e: &'ast Expression<'arena>, (): &mut ()) {
            self.0.push(e.span());
        }
    }
    let mut all = All(Vec::new());
    all.walk_program(program, &mut ());
    all.0
}
