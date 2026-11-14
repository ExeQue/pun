use mago_span::{HasSpan, Span};
use mago_syntax::cst::*;
use pun_phpstan::types::{Cast, Level, StrictMixed, Type, cast_is_error};

use super::super::{Checker, declared};
use super::flow::unparen;

pub(crate) const IDENTIFIERS: &[&str] = &[
    "method.nameNotString",
    "staticMethod.nameNotString",
    "classConstant.nameNotString",
    "property.nameNotString",
    "staticProperty.nameNotString",
];

pub(in super::super) fn member(selector: &ClassLikeMemberSelector) -> Option<Span> {
    match selector {
        ClassLikeMemberSelector::Variable(v) => Some(v.span()),
        ClassLikeMemberSelector::Expression(e) => Some(e.expression.span()),
        _ => None,
    }
}

pub(in super::super) fn constant(selector: &ClassLikeConstantSelector) -> Option<Span> {
    match selector {
        ClassLikeConstantSelector::Expression(e) => Some(e.expression.span()),
        _ => None,
    }
}

pub(in super::super) fn static_property(v: &Variable) -> Option<Span> {
    match v {
        Variable::Indirect(i) => Some(unparen(i.expression).span()),
        Variable::Nested(n) => Some(n.variable.span()),
        Variable::Direct(_) => None,
    }
}

pub(super) fn typed(e: &Expression, out: &mut Vec<Span>) {
    let (holder, name) = match e {
        Expression::Call(Call::Method(m)) => (Some(m.object.span()), member(&m.method)),
        Expression::Call(Call::NullSafeMethod(m)) => (Some(m.object.span()), member(&m.method)),
        Expression::Call(Call::StaticMethod(m)) => (Some(m.class.span()), member(&m.method)),
        Expression::Access(Access::Property(p)) => (Some(p.object.span()), member(&p.property)),
        Expression::Access(Access::NullSafeProperty(p)) => {
            (Some(p.object.span()), member(&p.property))
        }
        Expression::Access(Access::StaticProperty(p)) => {
            (Some(p.class.span()), static_property(&p.property))
        }
        Expression::Access(Access::ClassConstant(a)) => {
            (Some(a.class.span()), constant(&a.constant))
        }
        _ => (None, None),
    };
    if let Some(name) = name {
        out.extend(holder);
        out.push(name);
    }
}

pub(in super::super) fn not_string(
    c: &Checker,
    name: Span,
    castable: bool,
) -> Option<(Type, Type)> {
    if c.level < 2 && c.written(name) != "$this" {
        return None;
    }
    let ty = c.phpstan_type_at(name)?;
    let sm = StrictMixed::default();
    let fine = |t: &Type| {
        if castable {
            !cast_is_error(t, Cast::String, sm)
        } else {
            t.is_string().yes()
        }
    };
    let found = c.find_type_to_check_of(Some(ty.clone()), &fine)?;
    (!fine(&found)).then_some((ty, found))
}

pub(in super::super) fn check(
    c: &mut Checker,
    name: Span,
    castable: bool,
    what: &str,
    holder: &Expression,
    nullsafe: bool,
    identifier: &str,
) {
    if !c.on(crate::rules::Switches::NON_STRINGABLE_NAMES) {
        return;
    }
    let Some((ty, found)) = not_string(c, name, castable) else {
        return;
    };
    let class = c.class_scope();
    let keyword = |name: &str| match name.to_ascii_lowercase().as_str() {
        "self" | "static" => class.map(|k| k.original_name.to_string()),
        "parent" => class.and_then(|k| k.direct_parent_class.as_ref()).map(|p| {
            c.codebase
                .get_class_like(p.as_bytes())
                .map_or_else(|| p.to_string(), |k| k.original_name.to_string())
        }),
        _ => Some(name.to_string()),
    };
    let holder = match holder {
        Expression::Identifier(id) => keyword(&c.resolved(id)),
        Expression::Self_(_) => keyword("self"),
        Expression::Static(_) => keyword("static"),
        Expression::Parent(_) => keyword("parent"),
        e => c
            .phpstan_type_at(e.span())
            .map(|t| if nullsafe { t.remove_null() } else { t })
            .map(|t| declared::with_classes(c, || t.describe(Level::TypeOnly))),
    };
    let Some(holder) = holder else { return };
    let given = if castable { ty } else { found };
    let given = declared::with_classes(c, || given.describe(Level::Precise));
    let line = c.line_of(name);
    c.report_at_line(
        line,
        identifier,
        format!("{what} name for {holder} must be a string, but {given} was given."),
        None,
    );
}
