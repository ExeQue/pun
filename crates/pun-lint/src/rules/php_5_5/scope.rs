//! The parts of PHPStan's scope that Rector 2.8.4's `GetCalledClassTo*ClassRector` rules read, from the syntax.

use mago_syntax::cst::{Modifier, Node};

/// The class-like the node is in, as `ScopeFetcher::fetch($node)->getClassReflection()` sees it.
pub(super) enum ClassLike {
    /// A `final` class, an enum or an anonymous class (`isFinalByKeyword()`).
    Final,
    /// A class that is not `final`, an interface or a trait.
    Open,
}

pub(super) fn class_like(ancestors: &[Node<'_, '_>]) -> Option<ClassLike> {
    ancestors.iter().rev().find_map(|a| match a {
        Node::Class(class) => Some(
            if class
                .modifiers
                .iter()
                .any(|m| matches!(m, Modifier::Final(_)))
            {
                ClassLike::Final
            } else {
                ClassLike::Open
            },
        ),
        Node::Enum(_) => Some(ClassLike::Final),
        Node::AnonymousClass(_) => Some(ClassLike::Final),
        Node::Interface(_) | Node::Trait(_) => Some(ClassLike::Open),
        _ => None,
    })
}

/// `$scope->isInAnonymousFunction()`: the innermost function-like is a closure or an arrow function.
pub(super) fn in_anonymous_function(ancestors: &[Node<'_, '_>]) -> bool {
    matches!(
        ancestors.iter().rev().find(|a| matches!(
            a,
            Node::Function(_) | Node::Method(_) | Node::Closure(_) | Node::ArrowFunction(_)
        )),
        Some(Node::Closure(_) | Node::ArrowFunction(_))
    )
}
