//! Rector 2.8.4's configured rules in no set that rewrite one name, string, modifier list or return:
//! `RenameStringRector`, `StaticCallToNewRector`, `RemoveInterfacesRector`, `RenameAttributeRector`,
//! `ChangeMethodVisibilityRector` and `WrapReturnRector`.

use crate::problem::{Edit, Safety, Span};
use mago_syntax::cst::{
    ClassLikeMember, ClassLikeMemberSelector, Expression, Identifier, MethodBody, Modifier, Node,
    NodeKind, Statement,
};

use super::{assert_class, assert_method, is_class, items, object, string, string_map};
use crate::rules::support::class_members::qualified;
use crate::rules::support::expressions::text;
use crate::rules::support::names::resolve_class_name;
use crate::rules::support::rewrites::list_removal;
use crate::rules::types::class_scope;
use crate::syntax::node_span;
use crate::{Context, OptionsError, Report, Rule};

/// php-parser's `pSingleQuotedString()`: a `'` and a `\` before `\`, `'` or the end get a backslash.
fn single_quoted(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = String::from("'");
    for (i, c) in value.char_indices() {
        let escape =
            c == '\'' || (c == '\\' && matches!(bytes.get(i + 1), None | Some(b'\\' | b'\'')));
        if escape {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('\'');
    out
}

/// `RenameStringRector`.
#[derive(Default)]
pub struct RenameString {
    pub renames: Vec<(String, String)>,
}

impl Rule for RenameString {
    fn id(&self) -> &'static str {
        "lint/refactor/rename-string"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        self.renames = string_map(options)?;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::LiteralString])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::LiteralString(literal) = node else {
            return;
        };
        let Some(value) = literal.value.and_then(|v| std::str::from_utf8(v).ok()) else {
            return;
        };
        let Some((_, new)) = self.renames.iter().find(|(old, _)| old == value) else {
            return;
        };
        out.push(Report::fixed(
            node_span(literal),
            format!("Rename the string to `{new}`."),
            Safety::Unsafe,
            vec![Edit::new(node_span(literal), single_quoted(new))],
        ));
    }
}

/// The node an expression is part of, past its `Expression` and parentheses wrappers.
fn parent<'n, 'ast, 'arena>(
    span: Span,
    ancestors: &'n [Node<'ast, 'arena>],
) -> Option<&'n Node<'ast, 'arena>> {
    ancestors.iter().rev().find(|a| {
        node_span(*a) != span && !matches!(a, Node::Expression(_) | Node::Parenthesized(_))
    })
}

/// `StaticCallToNewRector`.
#[derive(Default)]
pub struct StaticCallToNew {
    /// (class, method)
    pub calls: Vec<(String, String)>,
}

impl Rule for StaticCallToNew {
    fn id(&self) -> &'static str {
        "lint/refactor/replace-static-with-new"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        let mut calls = Vec::new();
        for item in items(options)? {
            let map = object(item, &["class", "method"], &["class", "method"])?;
            let (class, method) = (string(map, "class")?, string(map, "method")?);
            assert_class(&class)?;
            assert_method(&method)?;
            calls.push((class, method));
        }
        self.calls = calls;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::StaticMethodCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::StaticMethodCall(call) = node else {
            return;
        };
        let (Expression::Identifier(class), ClassLikeMemberSelector::Identifier(method)) =
            (call.class, &call.method)
        else {
            return;
        };
        let resolved = resolve_class_name(ancestors, class);
        let resolved = resolved.trim_start_matches('\\');
        if !self.calls.iter().any(|(c, m)| {
            m.as_bytes().eq_ignore_ascii_case(method.value) && c.eq_ignore_ascii_case(resolved)
        }) {
            return;
        }
        let span = node_span(call);
        let mut code = format!("new \\{resolved}{}", text(ctx, &call.argument_list));
        // php-parser's printer wraps a `new` that is the object of an access or call
        if parent(span, ancestors).is_some_and(|p| {
            matches!(
                p,
                Node::MethodCall(_)
                    | Node::NullSafeMethodCall(_)
                    | Node::PropertyAccess(_)
                    | Node::NullSafePropertyAccess(_)
                    | Node::ArrayAccess(_)
                    | Node::FunctionCall(_)
            ) && node_span(p).start == span.start
        }) {
            code = format!("({code})");
        }
        out.push(Report::fixed(
            span,
            "Create the object with `new` instead of the static factory.",
            Safety::Unsafe,
            vec![Edit::new(span, code)],
        ));
    }
}

/// `RemoveInterfacesRector`.
#[derive(Default)]
pub struct RemoveInterfaces {
    pub interfaces: Vec<String>,
}

impl Rule for RemoveInterfaces {
    fn id(&self) -> &'static str {
        "lint/refactor/remove-interfaces"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        self.interfaces = items(options)?
            .into_iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or("each of `items` must be a string")
            })
            .collect::<Result<_, _>>()?;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[
            NodeKind::Class,
            NodeKind::AnonymousClass,
            NodeKind::Interface,
        ])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let (keyword, types) = match node {
            Node::Class(c) => match &c.implements {
                Some(i) => (&i.implements, &i.types),
                None => return,
            },
            Node::AnonymousClass(c) => match &c.implements {
                Some(i) => (&i.implements, &i.types),
                None => return,
            },
            Node::Interface(i) => match &i.extends {
                Some(e) => (&e.extends, &e.types),
                None => return,
            },
            _ => return,
        };
        let names: Vec<&Identifier<'_>> = types.iter().collect();
        let removed: Vec<bool> = names
            .iter()
            .map(|n| {
                let name = resolve_class_name(ancestors, n);
                let name = name.trim_start_matches('\\');
                self.interfaces.iter().any(|i| i.eq_ignore_ascii_case(name))
            })
            .collect();
        if !removed.contains(&true) {
            return;
        }
        let spans: Vec<Span> = names.iter().map(|n| node_span(*n)).collect();
        let edits = list_removal(&spans, &removed).unwrap_or_else(|| {
            // every name goes: drop the clause and the whitespace before it
            let source = ctx.source.as_bytes();
            let mut start = node_span(keyword).start;
            while start > 0 && source[start - 1].is_ascii_whitespace() {
                start -= 1;
            }
            vec![Edit::new(Span::new(start, spans[spans.len() - 1].end), "")]
        });
        out.push(Report::fixed(
            node_span(keyword),
            "Remove the configured interface.",
            Safety::Unsafe,
            edits,
        ));
    }
}

/// `RenameAttributeRector`.
#[derive(Default)]
pub struct RenameAttribute {
    /// (old attribute, new attribute)
    pub renames: Vec<(String, String)>,
}

impl Rule for RenameAttribute {
    fn id(&self) -> &'static str {
        "lint/refactor/rename-attribute"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        let mut renames = Vec::new();
        for item in items(options)? {
            let keys = ["old-attribute", "new-attribute"];
            let map = object(item, &keys, &keys)?;
            let (old, new) = (string(map, keys[0])?, string(map, keys[1])?);
            assert_class(&old)?;
            assert_class(&new)?;
            renames.push((old, new));
        }
        self.renames = renames;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Attribute])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Attribute(attribute) = node else {
            return;
        };
        // Rector reads the attributes of a class, method, property or parameter only
        let owner = ancestors.iter().rev().nth(1);
        if !owner.is_some_and(|o| {
            matches!(
                o,
                Node::Class(_)
                    | Node::AnonymousClass(_)
                    | Node::Method(_)
                    | Node::PlainProperty(_)
                    | Node::HookedProperty(_)
                    | Node::FunctionLikeParameter(_)
            )
        }) {
            return;
        }
        let name = resolve_class_name(ancestors, &attribute.name);
        let name = name.trim_start_matches('\\');
        let Some((_, new)) = self
            .renames
            .iter()
            .find(|(old, _)| old.eq_ignore_ascii_case(name))
        else {
            return;
        };
        let span = node_span(&attribute.name);
        out.push(Report::fixed(
            span,
            format!("Rename the attribute to `{new}`."),
            Safety::Unsafe,
            vec![Edit::new(
                span,
                format!("\\{}", new.trim_start_matches('\\')),
            )],
        ));
    }
}

// php-parser's `Modifiers` flags
const PUBLIC: i64 = 1;
const PROTECTED: i64 = 2;
const PRIVATE: i64 = 4;
const STATIC: i64 = 8;
const ABSTRACT: i64 = 16;
const FINAL: i64 = 32;
const READONLY: i64 = 64;

fn flag(modifier: &Modifier<'_>) -> i64 {
    match modifier {
        Modifier::Public(_) => PUBLIC,
        Modifier::Protected(_) => PROTECTED,
        Modifier::Private(_) => PRIVATE,
        Modifier::Static(_) => STATIC,
        Modifier::Abstract(_) => ABSTRACT,
        Modifier::Final(_) => FINAL,
        Modifier::Readonly(_) => READONLY,
        Modifier::PublicSet(_) | Modifier::ProtectedSet(_) | Modifier::PrivateSet(_) => 0,
    }
}

/// php-parser's `pModifiers()`.
fn print_modifiers(flags: i64) -> String {
    [
        (FINAL, "final "),
        (ABSTRACT, "abstract "),
        (PUBLIC, "public "),
        (PROTECTED, "protected "),
        (PRIVATE, "private "),
        (STATIC, "static "),
        (READONLY, "readonly "),
    ]
    .iter()
    .filter(|(f, _)| flags & f != 0)
    .map(|(_, s)| *s)
    .collect()
}

/// `ChangeMethodVisibilityRector`.
#[derive(Default)]
pub struct ChangeMethodVisibility {
    /// (parent class, method, php-parser visibility flag)
    pub changes: Vec<(String, String, i64)>,
}

impl Rule for ChangeMethodVisibility {
    fn id(&self) -> &'static str {
        "lint/refactor/change-method-visibility"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        let mut changes = Vec::new();
        for item in items(options)? {
            let keys = ["class", "method", "visibility"];
            let map = object(item, &keys, &keys)?;
            let (class, method) = (string(map, "class")?, string(map, "method")?);
            assert_class(&class)?;
            assert_method(&method)?;
            let visibility = super::int(map, "visibility")?;
            if ![PUBLIC, PROTECTED, PRIVATE, STATIC, ABSTRACT, FINAL].contains(&visibility) {
                return Err(
                    "`visibility` must be one of 1, 2, 4, 8, 16 and 32 (Rector's Visibility constants)"
                        .into(),
                );
            }
            changes.push((class, method, visibility));
        }
        self.changes = changes;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Class])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Class(class) = node else { return };
        let mut chain = ancestors.to_vec();
        chain.push(node);
        let Some(parent) = class_scope(&chain).and_then(|s| s.parent_class) else {
            return;
        };
        let parent = parent.trim_start_matches('\\');
        for member in class.members.iter() {
            let ClassLikeMember::Method(method) = member else {
                continue;
            };
            let old = method.modifiers.iter().map(flag).fold(0, |a, f| a | f);
            let mut flags = old;
            for (_, _, visibility) in self.changes.iter().filter(|(c, m, _)| {
                c.eq_ignore_ascii_case(parent)
                    && m.as_bytes().eq_ignore_ascii_case(method.name.value)
            }) {
                // VisibilityManipulator::replaceVisibilityFlag()
                if ![STATIC, ABSTRACT, FINAL].contains(visibility) {
                    flags &= !(PUBLIC | PROTECTED | PRIVATE);
                }
                flags |= visibility;
            }
            if flags == old {
                continue;
            }
            let at = node_span(&method.function).start;
            let start = method
                .modifiers
                .iter()
                .next()
                .map_or(at, |m| node_span(m).start);
            out.push(Report::fixed(
                node_span(&method.name),
                "Change the method's visibility as configured.",
                Safety::Unsafe,
                vec![Edit::new(Span::new(start, at), print_modifiers(flags))],
            ));
        }
    }
}

/// `WrapReturnRector`.
#[derive(Default)]
pub struct WrapReturn {
    /// (type, method, wrap in an array)
    pub wraps: Vec<(String, String, bool)>,
}

impl Rule for WrapReturn {
    fn id(&self) -> &'static str {
        "lint/refactor/wrap-return"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        let mut wraps = Vec::new();
        for item in items(options)? {
            let keys = ["type", "method", "is-array-wrap"];
            let map = object(item, &keys, &keys)?;
            let (class, method) = (string(map, "type")?, string(map, "method")?);
            assert_class(&class)?;
            assert_method(&method)?;
            let wrap = map["is-array-wrap"]
                .as_bool()
                .ok_or("`is-array-wrap` must be a boolean")?;
            wraps.push((class, method, wrap));
        }
        self.wraps = wraps;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Class])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Class(class) = node else { return };
        let name = qualified(ancestors, class.name.value);
        for member in class.members.iter() {
            let ClassLikeMember::Method(method) = member else {
                continue;
            };
            let wrapped = self.wraps.iter().any(|(t, m, wrap)| {
                *wrap
                    && m.as_bytes().eq_ignore_ascii_case(method.name.value)
                    && is_class(ctx, &name, t.trim_start_matches('\\'))
            });
            let MethodBody::Concrete(body) = &method.body else {
                continue;
            };
            if !wrapped {
                continue;
            }
            // the method's own statements only
            for statement in body.statements.iter() {
                let Statement::Return(r#return) = statement else {
                    continue;
                };
                let Some(value) = r#return.value else {
                    continue;
                };
                if matches!(value, Expression::Array(_) | Expression::LegacyArray(_)) {
                    continue;
                }
                out.push(Report::fixed(
                    node_span(value),
                    "Wrap the returned value in an array.",
                    Safety::Unsafe,
                    vec![Edit::new(
                        node_span(value),
                        format!("[{}]", text(ctx, value)),
                    )],
                ));
            }
        }
    }
}
