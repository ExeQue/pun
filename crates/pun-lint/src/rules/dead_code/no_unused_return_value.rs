//! Port of Mago 1.61.0's `no-unused-return-value`: a private method with a return type (other than `void` and `never`)
//! whose every call is a bare statement. Like Mago, the rule reads one class body: a call in a nested class counts
//! for the outer class too, it is silent when the class itself uses a trait or calls any method by a dynamic name, and
//! a string literal naming the method (any case) counts as a use.

use mago_span::{HasSpan, Span};
use mago_syntax::cst::*;
use mago_syntax::walker::MutWalker;

use crate::syntax::{span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoUnusedReturnValue;

impl Rule for NoUnusedReturnValue {
    fn id(&self) -> &'static str {
        "lint/dead-code/no-unused-return-value"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Class, NodeKind::Enum, NodeKind::AnonymousClass])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let members = match node {
            Node::Class(class) => &class.members,
            Node::Enum(r#enum) => &r#enum.members,
            Node::AnonymousClass(class) => &class.members,
            _ => return,
        };
        let mut candidates = Vec::new();
        for member in members {
            match member {
                ClassLikeMember::TraitUse(_) => return,
                ClassLikeMember::Method(method)
                    if method.modifiers.contains_private()
                        && matches!(method.body, MethodBody::Concrete(_))
                        && !method.name.value.starts_with(b"__")
                        && method.return_type_hint.as_ref().is_some_and(|hint| {
                            !matches!(hint.hint, Hint::Void(_) | Hint::Never(_))
                        }) =>
                {
                    candidates.push(Candidate {
                        method,
                        discarded: 0,
                        consumed: false,
                    });
                }
                _ => {}
            }
        }
        if candidates.is_empty() {
            return;
        }
        let mut walker = CallSites {
            candidates,
            discarded_call: None,
            dynamic: false,
        };
        for member in members {
            walker.walk_class_like_member(member, &mut ());
        }
        if walker.dynamic {
            return;
        }
        for candidate in walker.candidates {
            let Some(hint) = &candidate.method.return_type_hint else {
                continue;
            };
            if candidate.discarded == 0 || candidate.consumed {
                continue;
            }
            out.push(Report {
                span: span(hint.hint.span()),
                message: format!(
                    "The return value of `{}()` is never used.",
                    String::from_utf8_lossy(candidate.method.name.value)
                ),
                help: Some(
                    "Declare the method `void` and stop returning a value, or use the result where the method is \
                     called."
                        .to_string(),
                ),
                fix: None,
            });
        }
    }
}

struct Candidate<'ast, 'arena> {
    method: &'ast Method<'arena>,
    discarded: usize,
    consumed: bool,
}

struct CallSites<'ast, 'arena> {
    candidates: Vec<Candidate<'ast, 'arena>>,
    /// The call a bare statement just counted as discarded, so its own visit does not count it as a use.
    discarded_call: Option<Span>,
    dynamic: bool,
}

impl<'ast, 'arena> CallSites<'ast, 'arena> {
    fn find(&mut self, name: &[u8]) -> Option<&mut Candidate<'ast, 'arena>> {
        self.candidates
            .iter_mut()
            .find(|candidate| candidate.method.name.value.eq_ignore_ascii_case(name))
    }

    fn reference(&mut self, selector: &ClassLikeMemberSelector<'arena>, span: Span) {
        let ClassLikeMemberSelector::Identifier(name) = selector else {
            self.dynamic = true;
            return;
        };
        if self.discarded_call == Some(span) {
            self.discarded_call = None;
        } else if let Some(candidate) = self.find(name.value) {
            candidate.consumed = true;
        }
    }
}

impl<'ast, 'arena> MutWalker<'ast, 'arena, ()> for CallSites<'ast, 'arena> {
    fn walk_in_statement_expression(
        &mut self,
        statement: &'ast ExpressionStatement<'arena>,
        (): &mut (),
    ) {
        let Expression::Call(call) = unparenthesize(statement.expression) else {
            return;
        };
        let (Call::Method(MethodCall {
            method: ClassLikeMemberSelector::Identifier(name),
            ..
        })
        | Call::NullSafeMethod(NullSafeMethodCall {
            method: ClassLikeMemberSelector::Identifier(name),
            ..
        })
        | Call::StaticMethod(StaticMethodCall {
            method: ClassLikeMemberSelector::Identifier(name),
            ..
        })) = call
        else {
            return;
        };
        if let Some(candidate) = self.find(name.value) {
            candidate.discarded += 1;
            self.discarded_call = Some(call.span());
        }
    }

    fn walk_in_method_call(&mut self, call: &'ast MethodCall<'arena>, (): &mut ()) {
        self.reference(&call.method, call.span());
    }

    fn walk_in_null_safe_method_call(
        &mut self,
        call: &'ast NullSafeMethodCall<'arena>,
        (): &mut (),
    ) {
        self.reference(&call.method, call.span());
    }

    fn walk_in_static_method_call(&mut self, call: &'ast StaticMethodCall<'arena>, (): &mut ()) {
        self.reference(&call.method, call.span());
    }

    fn walk_in_method_partial_application(
        &mut self,
        partial: &'ast MethodPartialApplication<'arena>,
        (): &mut (),
    ) {
        self.reference(&partial.method, partial.span());
    }

    fn walk_in_static_method_partial_application(
        &mut self,
        partial: &'ast StaticMethodPartialApplication<'arena>,
        (): &mut (),
    ) {
        self.reference(&partial.method, partial.span());
    }

    fn walk_in_literal_string(&mut self, string: &'ast LiteralString<'arena>, (): &mut ()) {
        if let Some(candidate) = string.value.and_then(|value| self.find(value)) {
            candidate.consumed = true;
        }
    }
}
