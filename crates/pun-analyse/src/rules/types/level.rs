use mago_syntax::cst::*;
use pun_phpstan::symbols;
use pun_phpstan::types::{
    StrictMixed, Type, accepts_reasons, accepts_with, ancestor_args, intersect, to_benevolent,
    union,
};

use super::super::Checker;

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Levels: u8 {
        const CHECK_NULLABLES = 1 << 0;
        const CHECK_UNION_TYPES = 1 << 1;
        const CHECK_EXPLICIT_MIXED = 1 << 2;
        const CHECK_IMPLICIT_MIXED = 1 << 3;
    }
}

impl Levels {
    pub(crate) fn of(level: u8) -> Levels {
        let mut out = Levels::empty();
        out.set(Levels::CHECK_UNION_TYPES, level >= 7);
        out.set(Levels::CHECK_NULLABLES, level >= 8);
        out.set(Levels::CHECK_EXPLICIT_MIXED, level >= 9);
        out.set(Levels::CHECK_IMPLICIT_MIXED, level >= 10);
        out
    }

    pub(crate) fn strict_mixed(self) -> StrictMixed {
        StrictMixed {
            explicit: self.contains(Levels::CHECK_EXPLICIT_MIXED),
            implicit: self.contains(Levels::CHECK_IMPLICIT_MIXED),
        }
    }

    fn transform_accepted(self, accepting: &Type, accepted: &Type) -> Type {
        fn go(t: &Type, accepting: &Type) -> Type {
            match t {
                Type::Callable(_) | Type::Closure(_) => t.clone(),
                Type::Object(name, args, exact) if !args.is_empty() => {
                    generic(name, args, *exact, accepting)
                }
                Type::Null | Type::Benevolent(_) => t.traverse(&|i| go(i, accepting)),
                _ if !matches!(accepting, Type::Null) => {
                    t.clone().remove_null().traverse(&|i| go(i, accepting))
                }
                _ => t.traverse(&|i| go(i, accepting)),
            }
        }
        fn generic(name: &str, args: &[Type], exact: bool, accepting: &Type) -> Type {
            let contains_null = |t: &Type| {
                matches!(t, Type::Null) || matches!(t, Type::Union(ts) if ts.contains(&Type::Null))
            };
            let theirs = match accepting {
                Type::Object(a, theirs, _) if a.eq_ignore_ascii_case(name) => Some(theirs.clone()),
                Type::Object(a, theirs, _) => {
                    symbols::lookup(a).and_then(|c| ancestor_args(&c, name, theirs))
                }
                _ => None,
            };
            match theirs.filter(|theirs| theirs.len() == args.len()) {
                Some(theirs) => Type::Object(
                    name.to_string(),
                    args.iter()
                        .zip(&theirs)
                        .map(|(t, o)| match t {
                            Type::Union(_) if contains_null(t) && !contains_null(o) => {
                                go(&t.clone().remove_null(), accepting)
                            }
                            t => t.clone(),
                        })
                        .collect(),
                    exact,
                ),
                None => Type::Object(name.to_string(), args.to_vec(), exact),
            }
        }
        if self.contains(Levels::CHECK_NULLABLES) {
            accepted.clone()
        } else {
            go(accepted, accepting)
        }
    }

    pub(crate) fn accepts(self, accepting: &Type, accepted: &Type, strict_types: bool) -> bool {
        let accepted = self.transform_accepted(accepting, accepted);
        let accepting = self.strict_mixed().transform(accepting);
        let r = accepts_with(&accepting, &accepted, strict_types, self.strict_mixed());
        if self.contains(Levels::CHECK_UNION_TYPES) {
            r.yes()
        } else {
            !r.no()
        }
    }

    pub(crate) fn accepts_tip(
        self,
        accepting: &Type,
        accepted: &Type,
        strict_types: bool,
    ) -> Option<String> {
        let accepted = self.transform_accepted(accepting, accepted);
        let accepting = self.strict_mixed().transform(accepting);
        match accepts_reasons(&accepting, &accepted, strict_types, self.strict_mixed()).as_slice() {
            [] => None,
            [one] => Some(one.clone()),
            all => Some(
                all.iter()
                    .map(|r| format!("• {r}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
        }
    }
}

pub(crate) struct Found {
    pub ty: Option<Type>,
    pub unknown: Vec<String>,
}

impl Checker<'_, '_> {
    pub(in super::super) fn levels(&self) -> Levels {
        let mut out = Levels::of(self.level);
        out.set(
            Levels::CHECK_EXPLICIT_MIXED,
            self.on(crate::rules::Switches::CHECK_EXPLICIT_MIXED),
        );
        out.set(
            Levels::CHECK_IMPLICIT_MIXED,
            self.on(crate::rules::Switches::CHECK_IMPLICIT_MIXED),
        );
        out
    }

    pub(in super::super) fn is_this_expression(e: &Expression) -> bool {
        matches!(super::flow::unparen(e), Expression::Variable(Variable::Direct(v)) if v.name == b"$this")
    }

    pub(in super::super) fn checks_expression(&self, e: &Expression) -> bool {
        !self.on(crate::rules::Switches::CHECK_THIS_ONLY) || Self::is_this_expression(e)
    }

    pub(in super::super) fn find_type_to_check(
        &self,
        e: &Expression,
        criteria: &dyn Fn(&Type) -> bool,
    ) -> Found {
        let l = self.levels();
        let mut unknown = Vec::new();
        let ty = self
            .checks_expression(e)
            .then(|| self.phpstan_type(e))
            .flatten()
            .and_then(|t| self.find_in(l, t, criteria, Some(&mut unknown)));
        Found { ty, unknown }
    }

    pub(in super::super) fn find_type_to_check_of(
        &self,
        ty: Option<Type>,
        criteria: &dyn Fn(&Type) -> bool,
    ) -> Option<Type> {
        ty.and_then(|t| self.find_in(self.levels(), t, criteria, Some(&mut Vec::new())))
    }

    pub(super) fn find_in(
        &self,
        l: Levels,
        mut t: Type,
        criteria: &dyn Fn(&Type) -> bool,
        top: Option<&mut Vec<String>>,
    ) -> Option<Type> {
        if !l.contains(Levels::CHECK_NULLABLES) && !t.is_null().yes() && !criteria(&Type::Null) {
            t = t.remove_null();
        }
        if let Some(explicit) = t.mixed_explicitness() {
            let strict = l.contains(if explicit {
                Levels::CHECK_EXPLICIT_MIXED
            } else {
                Levels::CHECK_IMPLICIT_MIXED
            });
            return strict.then(|| t.to_strict_mixed());
        }
        if matches!(t, Type::Never(_)) {
            return None;
        }
        if let Some(unknown) = top {
            let missing = t
                .object_class_names()
                .into_iter()
                .filter(|n| !n.contains("@anonymous"))
                .filter(|n| match self.codebase.get_class_like(n.as_bytes()) {
                    Some(k) => k.kind == mago_codex::symbol::SymbolKind::Trait,
                    None => pun_phpstan::symbols::class(n).is_none(),
                });
            let (guarded, missing): (Vec<String>, Vec<String>) =
                missing.partition(|n| self.in_class_exists(n));
            unknown.extend(missing);
            if !unknown.is_empty() || !guarded.is_empty() {
                return None;
            }
        }
        if !l.contains(Levels::CHECK_UNION_TYPES)
            && t.is_object().yes()
            && t.object_class_names().is_empty()
        {
            return None;
        }
        let template_compound = match &t {
            Type::Template(tt)
                if matches!(
                    tt.bound,
                    Type::Union(_) | Type::Benevolent(_) | Type::Intersection(_)
                ) =>
            {
                Some(tt.bound.clone())
            }
            _ => None,
        };
        if let Type::Union(types) | Type::Benevolent(types) =
            template_compound.as_ref().unwrap_or(&t)
        {
            let t = template_compound.as_ref().unwrap_or(&t);
            let filter = !l.contains(Levels::CHECK_UNION_TYPES) || matches!(t, Type::Benevolent(_));
            let kept: Vec<Type> = types
                .iter()
                .filter(|i| !filter || criteria(i))
                .map(|i| {
                    self.find_in(l, i.clone(), criteria, None)
                        .unwrap_or(Type::Error)
                })
                .collect();
            if !kept.is_empty() {
                let u = union(kept);
                return (!matches!(u, Type::Error)).then(|| {
                    if matches!(t, Type::Benevolent(_)) {
                        to_benevolent(u)
                    } else {
                        u
                    }
                });
            }
        }
        if let Type::Intersection(parts) = template_compound.as_ref().unwrap_or(&t)
            && parts
                .iter()
                .any(|t| matches!(t, Type::Template(_)) && t.mixed_explicitness().is_some())
        {
            return Some(intersect(
                parts
                    .iter()
                    .map(|t| {
                        if matches!(t, Type::Template(_)) {
                            t.to_strict_mixed()
                        } else {
                            t.clone()
                        }
                    })
                    .collect(),
            ));
        }
        Some(t)
    }
}
