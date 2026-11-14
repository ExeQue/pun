use std::cell::RefCell;
use std::collections::HashMap;

use mago_analyzer::plugin::{
    ExpressionHookResult, HookContext, HookResult, InvocationInfo, MethodCallHook,
    MethodReturnTypeProvider, MethodTarget, NullSafeMethodCallHook, PluginRegistry, Provider,
    ProviderContext, ProviderMeta,
};
use mago_codex::identifier::function_like::FunctionLikeIdentifier;
use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::object::TObject;
use mago_codex::ttype::get_never;
use mago_codex::ttype::union::TUnion;
use mago_span::{HasSpan, Span};
use mago_syntax::cst::{Expression, MethodCall, NullSafeMethodCall};

thread_local! {
    static RECEIVERS: RefCell<HashMap<Span, Span>> = RefCell::new(HashMap::new());
}

static META: ProviderMeta = ProviderMeta::new(
    "pun::union-method",
    "Union method call",
    "Types a method call on a union from the classes that declare the method, as PHPStan does",
);

static TARGETS: [MethodTarget; 1] = [MethodTarget::any_class(b"*")];

pub(crate) fn register(registry: &mut PluginRegistry) {
    registry.register_method_call_hook(UnionMethod);
    registry.register_nullsafe_method_call_hook(UnionMethod);
    registry.register_method_provider(UnionMethod);
}

struct UnionMethod;

impl Provider for UnionMethod {
    fn meta() -> &'static ProviderMeta {
        &META
    }
}

fn enter(call: Span, object: &Expression<'_>) -> HookResult<ExpressionHookResult> {
    RECEIVERS.with(|r| r.borrow_mut().insert(call, object.span()));
    Ok(ExpressionHookResult::Continue)
}

fn leave(call: Span) -> HookResult<()> {
    RECEIVERS.with(|r| r.borrow_mut().remove(&call));
    Ok(())
}

impl MethodCallHook for UnionMethod {
    fn before_method_call(
        &self,
        call: &MethodCall<'_>,
        _context: &mut HookContext<'_, '_>,
    ) -> HookResult<ExpressionHookResult> {
        enter(call.span(), call.object)
    }

    fn after_method_call(
        &self,
        call: &MethodCall<'_>,
        _context: &mut HookContext<'_, '_>,
    ) -> HookResult<()> {
        leave(call.span())
    }
}

impl NullSafeMethodCallHook for UnionMethod {
    fn before_nullsafe_method_call(
        &self,
        call: &NullSafeMethodCall<'_>,
        _context: &mut HookContext<'_, '_>,
    ) -> HookResult<ExpressionHookResult> {
        enter(call.span(), call.object)
    }

    fn after_nullsafe_method_call(
        &self,
        call: &NullSafeMethodCall<'_>,
        _context: &mut HookContext<'_, '_>,
    ) -> HookResult<()> {
        leave(call.span())
    }
}

impl MethodReturnTypeProvider for UnionMethod {
    fn targets() -> &'static [MethodTarget] {
        &TARGETS
    }

    fn get_return_type(
        &self,
        context: &ProviderContext<'_, '_, '_>,
        class: &[u8],
        method: &[u8],
        invocation: &InvocationInfo<'_, '_, '_>,
    ) -> Option<TUnion> {
        let codebase = context.codebase();
        let magic = matches!(invocation.inner().target.get_function_like_identifier(),
            Some(FunctionLikeIdentifier::Method(_, m)) if m.as_bytes().eq_ignore_ascii_case(b"__call"));
        if !magic || codebase.method_exists(class, method) {
            return None;
        }
        let receiver = RECEIVERS.with(|r| r.borrow().get(&invocation.span()).copied())?;
        let declared = (context.get_expression_type(&receiver)?.types.iter()).any(|t| {
            matches!(t, TAtomic::Object(TObject::Named(o)) if codebase.method_exists(o.name.as_bytes(), method))
        });
        declared.then(get_never)
    }
}
