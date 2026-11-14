use mago_codex::metadata::class_like::ClassLikeMetadata;
use mago_codex::symbol::SymbolKind;
use mago_span::{HasSpan, Span};
use mago_syntax::cst::*;
use pun_phpstan::types::{Level, NeverKind, Type, intersect, is_super_type_of, union};

use super::super::missing_types::declaring;
use super::super::{Checker, declared};
use super::flow::list;

struct Declared {
    description: String,
    ret: Type,
    native: bool,
}

pub(in super::super) fn function(c: &mut Checker, f: &Function) {
    let name = c.resolved_span(f.name.span());
    let native = f.return_type_hint.is_some();
    let ret = declared::with_classes(c, || late(c, &declared::function(c, f).ret, None, !native));
    let d = Declared {
        description: format!("Function {name}()"),
        ret,
        native: f.return_type_hint.is_some(),
    };
    check(c, &f.body, f.span(), d);
}

pub(in super::super) fn method(c: &mut Checker, m: &Method) {
    let MethodBody::Concrete(b) = &m.body else {
        return;
    };
    let Some(context) = c.class_scope() else {
        return;
    };
    let Some(declaring) = declaring(c, m.span()) else {
        return;
    };
    let name = method_name(c, m);
    let native = m.return_type_hint.is_some();
    let ret = declared::with_classes(c, || {
        late(
            c,
            &declared::method(c, m, declaring, context).ret,
            Some(context),
            !native,
        )
    });
    let d = Declared {
        description: format!("Method {}::{name}()", c.display(context)),
        ret,
        native: m.return_type_hint.is_some(),
    };
    check(c, b, m.span(), d);
}

pub(in super::super) fn closure(c: &mut Checker, f: &Closure) {
    let Some(hint) = &f.return_type_hint else {
        return;
    };
    let class = c.class_scope();
    let ret = declared::with_classes(c, || {
        late(c, &declared::native(c, &hint.hint, class), class, false)
    });
    let d = Declared {
        description: "Anonymous function".to_string(),
        ret,
        native: true,
    };
    check(c, &f.body, f.span(), d);
}

pub(in super::super) fn get_hooks(
    c: &mut Checker,
    name: &[u8],
    hint: Option<&Hint>,
    at: Span,
    hooks: &PropertyHookList,
) {
    for h in &hooks.hooks {
        if !h.name.value.eq_ignore_ascii_case(b"get") {
            continue;
        }
        let PropertyHookBody::Concrete(PropertyHookConcreteBody::Block(b)) = &h.body else {
            continue;
        };
        let Some(context) = c.class_scope() else {
            return;
        };
        let ret = declared::with_classes(c, || {
            late(
                c,
                &declared::property(c, hint, at, &name[1..], context),
                Some(context),
                hint.is_none(),
            )
        });
        let d = Declared {
            description: format!(
                "Get hook for property {}::{}",
                c.display(context),
                String::from_utf8_lossy(name)
            ),
            ret,
            native: hint.is_some(),
        };
        check(c, b, h.span(), d);
    }
}

pub(super) fn method_name(c: &Checker, m: &Method) -> String {
    c.renamed
        .as_ref()
        .filter(|(span, _)| *span == m.span())
        .map_or_else(
            || String::from_utf8_lossy(m.name.value).into_owned(),
            |(_, n)| n.clone(),
        )
}

pub(super) fn late(c: &Checker, t: &Type, class: Option<&ClassLikeMetadata>, phpdoc: bool) -> Type {
    let go = |t: &Type| late(c, t, class, phpdoc);
    let own = |scope: &str| {
        !scope.starts_with("class ")
            || class.is_some_and(|k| scope == format!("class {}", declared::original_name(k)))
    };
    match t {
        Type::LateArrayShape(shape) => go(&shape.resolve()),
        Type::Conditional(cd) => union(vec![go(&cd.then), go(&cd.otherwise)]),
        Type::Template(tt) if !own(&tt.scope) => go(&tt.bound),
        Type::Intersection(ts) => intersect(ts.iter().map(go).collect()),
        Type::OffsetAccess(..) => Type::Error,
        Type::Object(name, args, _)
            if (name == "key-of" || name == "value-of") && args.len() == 1 =>
        {
            pun_phpstan::resolve::key_or_value_of(&go(&args[0]), name == "value-of")
        }
        Type::Object(name, args, exact) => Type::Object(
            c.reflected_name(name.as_bytes()),
            args.iter().map(go).collect(),
            *exact,
        ),
        Type::Static(_, args) if phpdoc && let Some(k) = class => {
            let args = args.iter().map(go).collect();
            let name = c.reflected_name(k.name.as_bytes());
            if k.flags.is_final() || k.kind == SymbolKind::Enum {
                Type::Object(name, args, false)
            } else {
                Type::Static(name, args)
            }
        }
        Type::Static(name, args) => Type::Static(
            c.reflected_name(name.as_bytes()),
            args.iter().map(go).collect(),
        ),
        t => t.traverse(&go),
    }
}

fn is_mixed(t: &Type) -> bool {
    match t {
        Type::Mixed { .. } => true,
        Type::Template(t) => matches!(t.bound, Type::Mixed { .. }),
        _ => false,
    }
}

fn contains_null(t: &Type) -> bool {
    match t {
        Type::Null => true,
        Type::Union(ts) | Type::Benevolent(ts) => ts.iter().any(contains_null),
        _ => false,
    }
}

pub(super) fn generator_return(t: &Type) -> Option<Type> {
    match t {
        Type::Object(name, args, _) if name.eq_ignore_ascii_case("Generator") => Some(
            args.get(3)
                .cloned()
                .unwrap_or_else(pun_phpstan::types::mixed),
        ),
        Type::Never(_) => Some(Type::Never(NeverKind::Implicit)),
        _ => None,
    }
}

fn check(c: &mut Checker, b: &Block, parent: Span, d: Declared) {
    let ret = d.ret;
    let results = super::flow::Results {
        types: &c.types,
        call_terminations: Some(&c.call_terminations),
        contents: &c.file.contents,
        codebase: Some(c.codebase),
    };
    let (_, ends) =
        results.execution_ends(&list(b.statements.as_slice()), parent, Some(b.right_brace));
    for end in ends {
        if end.flow.always {
            continue;
        }
        let mut ret = ret.clone();
        if is_super_type_of(&ret, &Type::Void).yes() && !is_mixed(&ret) {
            continue;
        }
        if end.flow.has_yield {
            if c.level >= 2
                && let Some(r) = generator_return(&ret)
            {
                ret = r;
                if ret.is_void().yes() {
                    continue;
                }
                if !is_mixed(&ret) {
                    c.report(
                        end.at,
                        "return.missing",
                        format!(
                            "{} should return {} but return statement is missing.",
                            d.description,
                            ret.describe(Level::TypeOnly)
                        ),
                        None,
                    );
                }
            }
            continue;
        }
        if !d.native && c.level < 2 && contains_null(&ret) {
            continue;
        }
        if matches!(
            ret,
            Type::Never(NeverKind::Explicit | NeverKind::NonAccepting)
        ) {
            let message = format!(
                "{} should always throw an exception or terminate script execution but doesn't do that.",
                d.description
            );
            if d.native {
                c.report_non_ignorable(end.at, "return.never", message, None);
            } else {
                c.report(end.at, "return.never", message, None);
            }
            continue;
        }
        if matches!(ret, Type::Mixed { .. }) && !d.native {
            continue;
        }
        let message = format!(
            "{} should return {} but return statement is missing.",
            d.description,
            ret.describe(Level::TypeOnly)
        );
        if d.native {
            c.report_non_ignorable(end.at, "return.missing", message, None);
        } else {
            c.report(end.at, "return.missing", message, None);
        }
    }
}
