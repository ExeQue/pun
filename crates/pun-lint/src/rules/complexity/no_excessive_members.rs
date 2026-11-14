use mago_syntax::cst::{ClassLikeMember, Node, NodeKind, Property};

use crate::rules::support::declarations::{class_like_header_span, is_method_setter_or_getter};
use crate::rules::support::options::{boolean, configure, uint};
use crate::{Context, OptionsError, Report, Rule};

fn report(node: Node<'_, '_>, message: String, help: &str) -> Report {
    Report {
        span: class_like_header_span(node),
        message,
        help: Some(help.into()),
        fix: None,
    }
}

fn members<'ast, 'arena>(
    node: Node<'ast, 'arena>,
) -> Option<(&'static str, &'ast [ClassLikeMember<'arena>])> {
    Some(match node {
        Node::Class(n) => ("Class", n.members.as_slice()),
        Node::Trait(n) => ("Trait", n.members.as_slice()),
        Node::Enum(n) => ("Enum", n.members.as_slice()),
        Node::Interface(n) => ("Interface", n.members.as_slice()),
        Node::AnonymousClass(n) => ("Anonymous class", n.members.as_slice()),
        _ => return None,
    })
}

pub struct NoExcessiveEnumCases {
    pub threshold: usize,
}

impl Default for NoExcessiveEnumCases {
    fn default() -> Self {
        NoExcessiveEnumCases { threshold: 20 }
    }
}

impl Rule for NoExcessiveEnumCases {
    fn id(&self) -> &'static str {
        "lint/complexity/no-excessive-enum-cases"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Enum])
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        configure(options, |key, value| {
            match key {
                "threshold" => self.threshold = uint(value, u16::MAX.into())?,
                _ => return None,
            }
            Some(())
        })
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Enum(r#enum) = node else { return };
        let cases = r#enum
            .members
            .iter()
            .filter(|m| matches!(m, ClassLikeMember::EnumCase(_)))
            .count();
        if cases > self.threshold {
            out.push(report(
                node,
                "Enum has too many cases.".into(),
                "Try splitting the enum into smaller logical groups or refactoring to reduce the total number of cases.",
            ));
        }
    }
}

pub struct NoExcessiveMethods {
    pub threshold: usize,
    pub count_hooks: bool,
    pub count_setters_and_getters: bool,
}

impl Default for NoExcessiveMethods {
    fn default() -> Self {
        NoExcessiveMethods {
            threshold: 10,
            count_hooks: false,
            count_setters_and_getters: false,
        }
    }
}

impl Rule for NoExcessiveMethods {
    fn id(&self) -> &'static str {
        "lint/complexity/no-excessive-methods"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[
            NodeKind::Class,
            NodeKind::Trait,
            NodeKind::Enum,
            NodeKind::Interface,
            NodeKind::AnonymousClass,
        ])
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        configure(options, |key, value| {
            match key {
                "threshold" => self.threshold = uint(value, u16::MAX.into())?,
                "count-hooks" => self.count_hooks = boolean(value)?,
                "count-setters-and-getters" => self.count_setters_and_getters = boolean(value)?,
                _ => return None,
            }
            Some(())
        })
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Some((kind, members)) = members(node) else {
            return;
        };
        let mut count = 0;
        for member in members {
            match member {
                ClassLikeMember::Method(method) => {
                    if self.count_setters_and_getters || !is_method_setter_or_getter(method) {
                        count += 1;
                    }
                }
                ClassLikeMember::Property(Property::Hooked(hooked)) if self.count_hooks => {
                    count += hooked.hook_list.hooks.len();
                }
                _ => {}
            }
        }
        if count > self.threshold {
            out.push(report(
                node,
                format!("{kind} has too many methods."),
                "Try reducing the number of methods, or consider splitting the structure into smaller, more focused structures.",
            ));
        }
    }
}

pub struct NoExcessiveProperties {
    pub threshold: usize,
}

impl Default for NoExcessiveProperties {
    fn default() -> Self {
        NoExcessiveProperties { threshold: 10 }
    }
}

impl Rule for NoExcessiveProperties {
    fn id(&self) -> &'static str {
        "lint/complexity/no-excessive-properties"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[
            NodeKind::Class,
            NodeKind::Trait,
            NodeKind::Interface,
            NodeKind::AnonymousClass,
        ])
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        configure(options, |key, value| {
            match key {
                "threshold" => self.threshold = uint(value, u16::MAX.into())?,
                _ => return None,
            }
            Some(())
        })
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Some((kind, members)) = members(node) else {
            return;
        };
        let properties: usize = members
            .iter()
            .map(|m| match m {
                ClassLikeMember::Property(Property::Plain(plain)) => plain.items.len(),
                ClassLikeMember::Property(Property::Hooked(_)) => 1,
                _ => 0,
            })
            .sum();
        if properties > self.threshold {
            out.push(report(
                node,
                format!("{kind} has too many properties."),
                "Try reducing the number of properties, or consider grouping related properties into a single object.",
            ));
        }
    }
}
