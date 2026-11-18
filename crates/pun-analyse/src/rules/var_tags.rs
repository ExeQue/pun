use mago_span::{HasSpan, Position, Span};
use mago_syntax::cst::*;
use mago_syntax::walker::MutWalker;
use pun_phpstan::resolve::{resolve_late, resolve_str};
use pun_phpstan::types::{
    Template, Tri, Type, is_super_type_of, mixed, recommended_level, to_phpdoc_fully_qualified,
};

use super::{Checker, DISCOVERING_SYMBOLS_TIP, FunctionScopeKind, declared, generics};

pub(super) const IDENTIFIERS: &[&str] = &[
    "varTag.unresolvableType",
    "varTag.trait",
    "varTag.nativeType",
];

pub(super) fn scope(c: &Checker, at: Span) -> super::doc::NameScope {
    let k = c.class_scope();
    let mut scope = declared::scope_at(c, at, k);
    let (Some(function), Some(span)) = (c.functions.last(), c.var_tag_functions.last()) else {
        return scope;
    };
    let Some((_, entry)) = c.doc_before(c.file.id, span.start.offset) else {
        return scope;
    };
    let described = match (function.kind, k) {
        (FunctionScopeKind::Method { .. }, Some(k)) => {
            format!("method {}::{}()", declared::original_name(k), function.name)
        }
        _ => format!("function {}()", function.name),
    };
    for t in &entry.doc.templates {
        let template = Template {
            name: t.name.clone(),
            scope: described.clone(),
            bound: t
                .bound
                .as_ref()
                .map_or(Type::Mixed { explicit: false }, |b| {
                    pun_phpstan::resolve::resolve_in(b, &scope)
                }),
            default: t
                .default
                .as_ref()
                .map(|d| pun_phpstan::resolve::resolve_in(d, &scope)),
            argument: false,
        };
        scope
            .templates
            .insert(t.name.clone(), Type::Template(Box::new(template)));
    }
    scope
}

pub(super) fn node(c: &mut Checker, at: Span) {
    check(c, at, at);
}

pub(super) fn check(c: &mut Checker, at: Span, names: Span) {
    if c.level < 2 {
        return;
    }
    let file = c.file;
    let Some((raw, entry)) = c.doc_before(file.id, at.start.offset) else {
        return;
    };
    let start = raw.as_ptr() as usize - file.contents.as_ptr() as usize;
    let doc_line = Span {
        file_id: at.file_id,
        start: Position {
            offset: start as u32,
        },
        end: Position {
            offset: start as u32,
        },
    };
    let mut errors: Vec<(Span, &'static str, String, Option<&'static str>)> = Vec::new();
    let mut classes = Vec::new();
    declared::with_classes(c, || {
        let scope = scope(c, names);
        for (name, ty) in entry.doc.var_types(&scope) {
            let what = if name.is_empty() {
                "PHPDoc tag @var".to_string()
            } else {
                format!("PHPDoc tag @var for variable ${name}")
            };
            let mut unresolvable = false;
            super::type_aliases::each(&ty, &mut |t| {
                unresolvable |= matches!(
                    t,
                    Type::Error | Type::Never(pun_phpstan::types::NeverKind::Implicit)
                );
            });
            if unresolvable {
                errors.push((
                    doc_line,
                    "varTag.unresolvableType",
                    format!("{what} contains unresolvable type."),
                    None,
                ));
                continue;
            }
            let s = generics::escape(&what);
            let messages = generics::GenericMessages::at(&s);
            let mut found = generics::Found::default();
            generics::generic_object_check(&ty, &messages, &mut found);
            errors.extend(
                found
                    .errors
                    .into_iter()
                    .map(|(id, m, tip)| (at, id, m, tip)),
            );
            for class in generics::referenced_classes(&ty) {
                match pun_phpstan::symbols::lookup(&class) {
                    Some(k) if k.is_trait => errors.push((
                        at,
                        "varTag.trait",
                        format!("{what} has invalid type {class}."),
                        None,
                    )),
                    Some(_) => {}
                    None if c.in_class_exists(&class) => {}
                    None => errors.push((
                        at,
                        "class.notFound",
                        format!("{what} contains unknown class {class}."),
                        Some(DISCOVERING_SYMBOLS_TIP),
                    )),
                }
                classes.push(class);
            }
        }
    });
    let pairs: Vec<_> = classes
        .into_iter()
        .map(|name| super::class_names::Pair::node(name, at))
        .collect();
    let case = c.on(crate::rules::Switches::CHECK_CLASS_CASE_SENSITIVITY);
    super::class_names::check(c, &pairs, case);
    for (at, identifier, message, tip) in errors {
        c.report(at, identifier, message, tip);
    }
}

/// The right-hand sides of the assignments that sit under a doc comment: `assignment()` prices their native type.
/// And the `$this` of each method call after a `@var` tag retypes `$this` in the same function-like
/// (`StatementsHandler::processStmtVarAnnotation()` assigns the tag's type to it), so the call is checked on that
/// type, not on the class.
pub(super) fn typed_expressions(
    program: &Program,
    contents: &[u8],
    comments: &super::Comments,
    level: u8,
) -> Vec<Span> {
    let mut w = Assigned {
        contents,
        comments,
        level,
        retyped: vec![None],
        spans: Vec::new(),
    };
    w.walk_program(program, &mut ());
    w.spans
}

struct Assigned<'c> {
    contents: &'c [u8],
    comments: &'c super::Comments,
    level: u8,
    /// Per function-like: whether a `@var` tag has retyped `$this` above an earlier statement, None where there is
    /// no `$this`.
    retyped: Vec<Option<bool>>,
    spans: Vec<Span>,
}

impl Assigned<'_> {
    /// A closure takes `$this`, retyped or not, from the enclosing scope; a static one has none.
    fn closure(&mut self, is_static: bool) {
        let parent = self.retyped.last().copied().flatten();
        self.retyped.push(parent.filter(|_| !is_static));
    }

    fn this_object(&mut self, object: &Expression) {
        if self.retyped.last() == Some(&Some(true)) && super::types::flow::is_this(object) {
            self.spans.push(object.span());
        }
    }
}

impl<'ast, 'arena> MutWalker<'ast, 'arena, ()> for Assigned<'_> {
    fn walk_in_statement(&mut self, s: &'ast Statement<'arena>, (): &mut ()) {
        if !matches!(
            s,
            Statement::Foreach(_) | Statement::Static(_) | Statement::Global(_)
        ) && let Some((_, entry)) = self
            .comments
            .doc_before(self.contents, s.span().start.offset as usize)
            && entry.doc.vars.iter().any(|(_, name, _)| name == "this")
            && let Some(Some(last)) = self.retyped.last_mut()
        {
            *last = true;
        }
    }

    fn walk_in_statement_expression(&mut self, s: &'ast ExpressionStatement<'arena>, (): &mut ()) {
        if self.level >= 2
            && let Expression::Assignment(a) = s.expression
            && self
                .comments
                .doc_before(self.contents, s.span().start.offset as usize)
                .is_some()
        {
            self.spans.push(assigned_expression(a.rhs).span());
        }
    }

    fn walk_in_closure(&mut self, f: &'ast Closure<'arena>, (): &mut ()) {
        self.closure(f.r#static.is_some());
    }

    fn walk_out_closure(&mut self, _: &'ast Closure<'arena>, (): &mut ()) {
        self.retyped.pop();
    }

    fn walk_in_arrow_function(&mut self, f: &'ast ArrowFunction<'arena>, (): &mut ()) {
        self.closure(f.r#static.is_some());
    }

    fn walk_out_arrow_function(&mut self, _: &'ast ArrowFunction<'arena>, (): &mut ()) {
        self.retyped.pop();
    }

    fn walk_in_method(&mut self, m: &'ast Method<'arena>, (): &mut ()) {
        self.retyped
            .push((!m.modifiers.contains_static()).then_some(false));
    }

    fn walk_out_method(&mut self, _: &'ast Method<'arena>, (): &mut ()) {
        self.retyped.pop();
    }

    fn walk_in_function(&mut self, _: &'ast Function<'arena>, (): &mut ()) {
        self.retyped.push(None);
    }

    fn walk_out_function(&mut self, _: &'ast Function<'arena>, (): &mut ()) {
        self.retyped.pop();
    }

    fn walk_in_method_call(&mut self, call: &'ast MethodCall<'arena>, (): &mut ()) {
        self.this_object(call.object);
    }

    fn walk_in_null_safe_method_call(
        &mut self,
        call: &'ast NullSafeMethodCall<'arena>,
        (): &mut (),
    ) {
        self.this_object(call.object);
    }
}

/// php-parser's `Assign::$expr` and `AssignRef::$expr`: without parentheses and the `&`.
fn assigned_expression<'e, 'arena>(e: &'e Expression<'arena>) -> &'e Expression<'arena> {
    match e {
        Expression::Parenthesized(p) => assigned_expression(p.expression),
        Expression::UnaryPrefix(u) if u.operator.is_reference() => assigned_expression(u.operand),
        e => e,
    }
}

/// The `@var` types above the statement at `at`, keyed as `ResolvedPhpDoc::getVarTags()` merges every doc
/// comment's tags: by variable name, and by position for the tags without one. A later doc comment wins.
fn var_tag_types(c: &Checker, at: Span) -> (Vec<(String, Type)>, Vec<Type>) {
    let contents = &c.file.contents;
    let mut docs = Vec::new();
    let mut offset = at.start.offset;
    while let Some((raw, entry)) = c.doc_before(c.file.id, offset) {
        offset = (raw.as_ptr() as usize - contents.as_ptr() as usize) as u32;
        docs.push(entry);
    }
    let (mut named, mut unnamed) = (Vec::<(String, Type)>::new(), Vec::<Type>::new());
    if docs.is_empty() {
        return (named, unnamed);
    }
    let scope = scope(c, at);
    for doc in docs.into_iter().rev() {
        let vars = declared::with_classes(c, || doc.doc.var_types(&scope));
        let mut position = 0;
        for (name, ty) in vars {
            if name.is_empty() {
                if position < unnamed.len() {
                    unnamed[position] = ty;
                } else {
                    unnamed.push(ty);
                }
                position += 1;
            } else if let Some(entry) = named.iter_mut().find(|(n, _)| *n == name) {
                entry.1 = ty;
            } else {
                named.push((name, ty));
            }
        }
    }
    (named, unnamed)
}

/// VarTagTypeRuleHelper::checkVarType() for an assignment to a variable (`WrongVariableNameInVarTagRule`'s
/// `processAssign()` once its variable-name checks pass): the `@var` type has to be a subtype of the
/// assigned expression's native type.
pub(super) fn assignment(c: &mut Checker, at: Span, lhs: &Expression, rhs: &Expression) {
    let Expression::Variable(Variable::Direct(v)) = lhs else {
        return;
    };
    let name = String::from_utf8_lossy(&v.name[1..]);
    let (named, unnamed) = var_tag_types(c, at);
    let var_tag = match named.into_iter().find(|(n, _)| *n == name) {
        Some((_, ty)) => ty,
        None => match unnamed.into_iter().next() {
            Some(ty) => ty,
            None => return,
        },
    };
    let expr = assigned_expression(rhs);
    let Some(native) = c.native_types.get(&expr.span()) else {
        return;
    };
    let native = c.phpstan_type_from_in(expr, native, &c.native_types);
    let class = c.class_scope().map(declared::original_name);
    let message = declared::with_classes(c, || {
        if valid_super_type_of_expr(expr, &native, &resolve_late(&var_tag), class.as_deref()).yes()
        {
            return None;
        }
        let level = recommended_level(&native, Some(&var_tag));
        Some(format!(
            "PHPDoc tag @var with type {} is not subtype of native type {}.",
            var_tag.describe(level),
            native.describe(level)
        ))
    });
    if let Some(message) = message {
        c.report(at, "varTag.nativeType", message, None);
    }
}

/// `StatementsHandler::processStmtVarAnnotation()` for a `@var` tag that names `$this` above a statement other than
/// a foreach, static or global: the tag retypes `$this`, and when that changes its type, VarTagChangedExpressionTypeRule
/// checks the tag against `$this`'s native type, `$this(Class)`.
pub(super) fn this_tag(c: &mut Checker, s: &Statement) {
    if matches!(
        s,
        Statement::Foreach(_) | Statement::Static(_) | Statement::Global(_)
    ) || !c.this_defined()
    {
        return;
    }
    let (named, _) = var_tag_types(c, s.span());
    let Some((_, tag)) = named.into_iter().find(|(name, _)| name == "this") else {
        return;
    };
    let previous = c.retype_this(tag.clone());
    // a closure bound elsewhere (`Closure::bind()`, `@param-closure-this`, a macro) has that class's `$this`
    let bound = c.classes.last().is_some_and(|k| k.bound);
    let Some(k) = c.class_scope().filter(|_| c.level >= 2 && !bound) else {
        return;
    };
    let class = declared::original_name(k);
    let native = Type::This(class.clone());
    if previous.as_ref().unwrap_or(&native) == &tag {
        return;
    }
    // an anonymous class shows as `class@anonymous/file.php:line`
    let shown = Type::This(c.display(k));
    let message = declared::with_classes(c, || {
        if valid_super_type(&native, &resolve_late(&tag), Some(&class), 0).yes() {
            return None;
        }
        let level = recommended_level(&native, Some(&tag));
        Some(format!(
            "PHPDoc tag @var with type {} is not subtype of native type {}.",
            tag.describe(level),
            shown.describe(level)
        ))
    });
    if let Some(message) = message {
        c.report(s.span(), "varTag.nativeType", message, None);
    }
}

fn valid_super_type_of_expr(
    expr: &Expression,
    ty: &Type,
    var_tag: &Type,
    class: Option<&str>,
) -> Tri {
    match expr {
        Expression::Array(Array { elements, .. })
        | Expression::LegacyArray(LegacyArray { elements, .. })
            if elements.is_empty() =>
        {
            at_least_maybe_super_type(
                &Type::Array(Box::new(mixed()), Box::new(mixed())),
                var_tag,
                class,
            )
        }
        Expression::Array(_)
        | Expression::LegacyArray(_)
        | Expression::ConstantAccess(_)
        | Expression::Literal(_)
        | Expression::CompositeString(_)
        | Expression::MagicConstant(_) => at_least_maybe_super_type(ty, var_tag, class),
        Expression::Instantiation(_) => match ty {
            Type::Object(name, args, generic) if !args.is_empty() => valid_super_type(
                &Type::Object(name.clone(), Vec::new(), *generic),
                var_tag,
                class,
                0,
            ),
            ty => valid_super_type(ty, var_tag, class, 0),
        },
        _ => valid_super_type(ty, var_tag, class, 0),
    }
}

/// `reportAnyTypeWideningInVarTag` off: a generic object's invariant type arguments are compared covariantly.
fn covariant(t: &Type) -> Type {
    match t {
        Type::Object(name, args, generic) if !args.is_empty() => Type::Object(
            name.clone(),
            args.iter()
                .map(|a| match a {
                    Type::Projection(..) | Type::Wildcard => covariant(a),
                    a => Type::Projection(true, Box::new(covariant(a))),
                })
                .collect(),
            *generic,
        ),
        t => t.traverse(&covariant),
    }
}

fn valid_super_type(ty: &Type, var_tag: &Type, class: Option<&str>, depth: usize) -> Tri {
    let ty = covariant(ty);
    if ty.is_constant_array().yes() && ty.is_iterable_at_least_once().no() {
        return at_least_maybe_super_type(
            &Type::Array(Box::new(mixed()), Box::new(mixed())),
            var_tag,
            class,
        );
    }
    if ty.is_iterable().yes() && var_tag.is_iterable().yes() {
        let is = at_least_maybe_super_type(&ty, var_tag, class);
        if is.no() {
            return is;
        }
        let (inner, inner_var_tag) = (ty.iterable_value_type(), var_tag.iterable_value_type());
        if ty == inner || *var_tag == inner_var_tag {
            return super_type_of_var_type(&inner, &inner_var_tag, class);
        }
        return valid_super_type(&inner, &inner_var_tag, class, depth + 1);
    }
    if depth == 0 && ty.is_constant_value().yes() {
        return at_least_maybe_super_type(&ty, var_tag, class);
    }
    super_type_of_var_type(&ty, var_tag, class)
}

/// The type as its PHPDoc, resolved again (`$type->toPhpDocNode()` through the TypeNodeResolver).
fn through_phpdoc(ty: &Type, class: Option<&str>) -> Type {
    resolve_str(&to_phpdoc_fully_qualified(ty), class)
}

fn super_type_of_var_type(ty: &Type, var_tag: &Type, class: Option<&str>) -> Tri {
    if is_super_type_of(ty, var_tag).yes() {
        return Tri::Yes;
    }
    is_super_type_of(&through_phpdoc(ty, class), var_tag)
}

fn at_least_maybe_super_type(ty: &Type, var_tag: &Type, class: Option<&str>) -> Tri {
    if !is_super_type_of(ty, var_tag).no() {
        return Tri::Yes;
    }
    if is_super_type_of(&through_phpdoc(ty, class), var_tag).no() {
        Tri::No
    } else {
        Tri::Yes
    }
}
