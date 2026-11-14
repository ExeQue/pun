use std::cell::Cell;

/// Names a borrowed type for every lifetime, so [`Scoped`] can hold a borrow of it beyond the lifetime's reach.
pub(crate) trait Family: 'static {
    type Of<'a>: ?Sized + 'a;
}

/// A thread-local slot that holds a borrow for as long as [`Scoped::set`]'s closure runs.
pub(crate) struct Scoped<F: Family>(Cell<Option<&'static F::Of<'static>>>);

impl<F: Family> Scoped<F> {
    pub(crate) fn new() -> Self {
        Self(Cell::new(None))
    }

    pub(crate) fn set<R>(&self, value: &F::Of<'_>, f: impl FnOnce() -> R) -> R {
        struct Restore<'s, T: ?Sized + 'static>(&'s Cell<Option<&'static T>>, Option<&'static T>);
        impl<T: ?Sized> Drop for Restore<'_, T> {
            fn drop(&mut self) {
                self.0.set(self.1);
            }
        }
        // SAFETY: only the lifetime is erased. `Restore` puts the previous value back before this function returns,
        // also on unwinding, so the slot never holds `value` past its borrow. `get` hands it out to a closure that
        // must accept every lifetime, so nothing borrowed from it can outlive that call.
        let value = unsafe { std::mem::transmute::<&F::Of<'_>, &'static F::Of<'static>>(value) };
        let _restore = Restore(&self.0, self.0.replace(Some(value)));
        f()
    }

    pub(crate) fn get<R>(&self, f: impl for<'a> FnOnce(&'a F::Of<'a>) -> R) -> Option<R> {
        self.0.get().map(f)
    }
}
