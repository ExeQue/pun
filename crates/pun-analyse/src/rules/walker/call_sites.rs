//! The walker's entry into function, method and static method calls.

use super::super::*;

impl<'arena> Checker<'_, 'arena> {
    pub(super) fn visit_in_function_call(&mut self, call: &FunctionCall<'arena>) {
        variables::compact(self, call);
        self.bind_function_arguments(call);
        dead_code::function_check(self, call);
        debug::function_call(self, call);
        larastan::function_call(self, call);
        types::function_call(self, call);
        types::castable_call(self, call);
        types::define_value(self, call);
        function_checks::function_call(self, call);
        restricted_functions::function_call(self, call);
        deprecations::ini_option(self, call);
        calls::function_call(self, call);
        types::callable_call(self, call);
        types::call_user_func(self, call);
        dead_code::function_call(self, call);
        dead_code::sort_call(self, call);
        dead_code::haystack_call(self, call);
        assign::arguments(self, assign::Callee::Function(call));
        assign::function_virtual_assign(self, call);
        self.in_exists_call += usize::from(calls::is_class_exists_family(self, call));
        self.guards.extend(self.define_guard(call));
    }

    pub(super) fn visit_in_null_safe_method_call(&mut self, call: &NullSafeMethodCall<'arena>) {
        self.bind_method_arguments(call.object, &call.method, &call.argument_list, false);
        if self.method_rules.call_methods {
            named_argument::call(self, call.span(), call.object, &call.argument_list, true);
        }
        internal::method_call(self, call.span(), call.object, &call.method);
        strict::dynamic_call(self, call.span(), call.object, &call.method);
        if self.method_rules.call_methods {
            types::method_call(
                self,
                call.span(),
                call.object,
                &call.method,
                &call.argument_list,
                true,
            );
            if let Some(name) = types::dynamic_names::member(&call.method) {
                types::dynamic_names::check(
                    self,
                    name,
                    false,
                    "Method",
                    call.object,
                    true,
                    "method.nameNotString",
                );
            }
        }
        types::nullsafe_method(self, call);
    }

    pub(super) fn visit_in_method_call(&mut self, call: &MethodCall<'arena>) {
        self.bind_method_arguments(call.object, &call.method, &call.argument_list, false);
        if self.method_rules.call_methods {
            named_argument::call(self, call.span(), call.object, &call.argument_list, false);
        }
        dead_code::method_check(self, call);
        internal::method_call(self, call.span(), call.object, &call.method);
        strict::dynamic_call(self, call.span(), call.object, &call.method);
        larastan::method_call(self, call);
        if let ClassLikeMemberSelector::Identifier(m) = &call.method
            && m.value.eq_ignore_ascii_case(b"call")
        {
            let class = self.call_class(&call.argument_list);
            let defines_this = !call.argument_list.arguments.is_empty()
                && match types::flow::unparen(call.object) {
                    Expression::Closure(f) => f.r#static.is_none(),
                    Expression::ArrowFunction(f) => f.r#static.is_none(),
                    _ => false,
                };
            self.rebind(call.object, class, defines_this);
        }
        if self.method_rules.call_methods && self.this_defined() {
            calls::method_call(self, call);
        }
        if self.method_rules.call_methods {
            types::method_call(
                self,
                call.span(),
                call.object,
                &call.method,
                &call.argument_list,
                false,
            );
            if let Some(name) = types::dynamic_names::member(&call.method) {
                types::dynamic_names::check(
                    self,
                    name,
                    false,
                    "Method",
                    call.object,
                    false,
                    "method.nameNotString",
                );
            }
        }
        assign::arguments(self, assign::Callee::Method(call));
    }

    pub(super) fn visit_in_static_method_call(&mut self, call: &StaticMethodCall<'arena>) {
        self.bind_method_arguments(call.class, &call.method, &call.argument_list, true);
        dead_code::static_method_check(self, call);
        through_static::method(self, call);
        internal::static_method_call(self, call);
        larastan::static_method_call(self, call);
        if self.is_closure_bind(call) {
            self.closure_bind += 1;
            if let Some(Argument::Positional(a)) = call.argument_list.arguments.first() {
                let class = self.bind_class(&call.argument_list);
                self.rebind(a.value, class, false);
            }
        }
        calls::static_method_call(self, call);
        types::static_call(self, call);
        if let Some(name) = types::dynamic_names::member(&call.method) {
            types::dynamic_names::check(
                self,
                name,
                false,
                "Method",
                call.class,
                false,
                "staticMethod.nameNotString",
            );
        }
        assign::arguments(self, assign::Callee::StaticMethod(call));
    }
}
