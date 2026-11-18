use mago_span::{Position, Span};
use mago_syntax::cst::{Expression, Statement};
use pun_phpstan::phpdoc::TagValue;

use super::Checker;

pub(super) const IDENTIFIERS: &[&str] = &["phpDoc.parseError", "phpDoc.phpstanTag"];

const POSSIBLE_PHPSTAN_TAGS: &[&str] = &[
    "@phpstan-param",
    "@phpstan-param-out",
    "@phpstan-var",
    "@phpstan-extends",
    "@phpstan-implements",
    "@phpstan-use",
    "@phpstan-template",
    "@phpstan-template-contravariant",
    "@phpstan-template-covariant",
    "@phpstan-return",
    "@phpstan-throws",
    "@phpstan-ignore",
    "@phpstan-ignore-next-line",
    "@phpstan-ignore-line",
    "@phpstan-method",
    "@phpstan-pure",
    "@phpstan-impure",
    "@phpstan-immutable",
    "@phpstan-type",
    "@phpstan-import-type",
    "@phpstan-property",
    "@phpstan-property-read",
    "@phpstan-property-write",
    "@phpstan-consistent-constructor",
    "@phpstan-assert",
    "@phpstan-assert-if-true",
    "@phpstan-assert-if-false",
    "@phpstan-self-out",
    "@phpstan-this-out",
    "@phpstan-allow-private-mutation",
    "@phpstan-readonly",
    "@phpstan-readonly-allow-private-mutation",
    "@phpstan-require-extends",
    "@phpstan-require-implements",
    "@phpstan-sealed",
    "@phpstan-param-immediately-invoked-callable",
    "@phpstan-param-later-invoked-callable",
    "@phpstan-param-closure-this",
    "@phpstan-pure-unless-callable-is-impure",
    "@phpstan-pure-unless-parameter-passed",
    "@phpstan-all-methods-pure",
    "@phpstan-all-methods-impure",
];

pub(super) fn statement(c: &mut Checker, s: &Statement) {
    if !matches!(
        s,
        Statement::Class(_)
            | Statement::Interface(_)
            | Statement::Trait(_)
            | Statement::Enum(_)
            | Statement::Function(_)
            | Statement::Constant(_)
            | Statement::OpeningTag(_)
            | Statement::ClosingTag(_)
            | Statement::Inline(_)
    ) {
        super::var_tags::node(c, mago_span::HasSpan::span(s));
        super::var_names::statement(c, s);
        super::var_tags::this_tag(c, s);
    }
    match s {
        Statement::Expression(e) if !matches!(e.expression, Expression::Assignment(_)) => {}
        Statement::OpeningTag(_) | Statement::ClosingTag(_) | Statement::Inline(_) => {}
        _ => node(c, mago_span::HasSpan::span(s)),
    }
}

pub(super) fn nop(c: &mut Checker, end: Span) {
    let contents = &c.file.contents;
    let Some((doc, _)) = c.doc_before(c.file.id, end.start.offset) else {
        return;
    };
    let start = doc.as_ptr() as usize - contents.as_ptr() as usize;
    let offset = Position::new((start + doc.len()) as u32);
    let at = Span::new(end.file_id, offset, offset);
    let code = contents[..start]
        .iter()
        .rposition(|b| !b.is_ascii_whitespace())
        .unwrap_or(0);
    let names = Position::new(code as u32);
    node(c, at);
    super::invalid_throws::node(c, at);
    super::var_tags::check(c, at, Span::new(end.file_id, names, names));
    super::var_names::clause(c, at);
}

pub(super) fn clause(c: &mut Checker, at: Span) {
    node(c, at);
    super::invalid_throws::node(c, at);
    super::var_tags::node(c, at);
    super::var_names::clause(c, at);
}

pub(super) fn node(c: &mut Checker, at: Span) {
    if c.level < 2 {
        return;
    }
    let file = c.file;
    let Some((doc, entry)) = c.doc_before(file.id, at.start.offset) else {
        return;
    };
    let start = doc.as_ptr() as usize - file.contents.as_ptr() as usize;
    for tag in &entry.phpdoc {
        let offset = start
            + doc
                .split_inclusive(|b| *b == b'\n')
                .take(tag.line - 1)
                .map(<[u8]>::len)
                .sum::<usize>();
        let line = Span {
            file_id: at.file_id,
            start: Position {
                offset: offset as u32,
            },
            end: Position {
                offset: offset as u32,
            },
        };
        let name = &tag.name;
        if !name.starts_with("@phan-") && !name.starts_with("@psalm-") {
            let message = match &tag.value {
                TagValue::TypeAlias(alias, Err(e)) => Some(format!(
                    "PHPDoc tag {name} {alias} has invalid value: {}",
                    e.message()
                )),
                TagValue::Invalid(value, e) => Some(format!(
                    "PHPDoc tag {name} has invalid value ({value}): {}",
                    e.message()
                )),
                _ => None,
            };
            if let Some(message) = message {
                c.report(line, "phpDoc.parseError", message, None);
            }
        }
        if name.starts_with("@phpstan-") && !POSSIBLE_PHPSTAN_TAGS.contains(&name.as_str()) {
            c.report(
                line,
                "phpDoc.phpstanTag",
                format!("Unknown PHPDoc tag: {name}"),
                None,
            );
        }
    }
}
