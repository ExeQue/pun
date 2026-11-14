use mago_syntax::cst::{Expression, MethodBody, Node, NodeKind, Statement};

use super::property_attribute::{ClassView, MODEL, report, rewrite};
use crate::rules::rector_stmts::block_list;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

const COLLECTED_BY: &str = "Illuminate\\Database\\Eloquent\\Attributes\\CollectedBy";

pub struct PreferCollectedByAttribute;

impl Rule for PreferCollectedByAttribute {
    fn id(&self) -> &'static str {
        "lint/laravel/prefer-collected-by-attribute"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Class, NodeKind::AnonymousClass])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Some(class) = ClassView::of(node) else {
            return;
        };
        if !class.is_object_type(ctx, ancestors, MODEL)
            || class.has_attribute(ancestors, COLLECTED_BY)
        {
            return;
        }
        let Some((index, method)) = class.method("newCollection") else {
            return;
        };
        let Some(value) = collected_by_value(ctx.source, method) else {
            return;
        };
        let group = format!("#[\\{COLLECTED_BY}({value})]");
        let Some(edits) = rewrite(ctx.source, &class, &[group], &[], &[index]) else {
            return;
        };
        out.push(report(
            &class,
            "Changes model newCollection method to use the CollectedBy attribute.",
            edits,
        ));
    }
}

fn collected_by_value(source: &str, method: &mago_syntax::cst::Method<'_>) -> Option<String> {
    let MethodBody::Concrete(block) = &method.body else {
        return None;
    };
    let list = block_list(source, block);
    if list.len() != 1 {
        return None;
    }
    let Statement::Return(r) = list.stmts[0] else {
        return None;
    };
    let Some(Expression::Instantiation(new)) = r.value.map(unparenthesize) else {
        return None;
    };
    let name = match new.class {
        Expression::Identifier(id) => node_span(id),
        Expression::Self_(k) | Expression::Parent(k) => node_span(k),
        _ => return None,
    };
    Some(format!("{}::class", &source[name.start..name.end]))
}
