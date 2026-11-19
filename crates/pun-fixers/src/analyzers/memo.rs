use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{LazyLock, PoisonError, RwLock};

/// The results of a pure function, shared by every thread: doc block lines and types recur across a project's files,
/// so one thread's work saves the others theirs.
pub(crate) struct Memo<K, V>(LazyLock<RwLock<HashMap<K, V, foldhash::fast::FixedState>>>);

impl<K: Hash + Eq, V: Clone> Memo<K, V> {
    pub(crate) const fn new() -> Self {
        Memo(LazyLock::new(Default::default))
    }

    /// The value for `key`, from `compute(key)` the first time.
    pub(crate) fn get_or<Q>(&self, key: &Q, compute: impl FnOnce(&Q) -> V) -> V
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ToOwned<Owned = K> + ?Sized,
    {
        let found = (self.0.read().unwrap_or_else(PoisonError::into_inner))
            .get(key)
            .cloned();
        found.unwrap_or_else(|| {
            let value = compute(key);
            (self.0.write().unwrap_or_else(PoisonError::into_inner))
                .insert(key.to_owned(), value.clone());
            value
        })
    }
}

impl<K: Hash + Eq, V: Clone> Default for Memo<K, V> {
    fn default() -> Self {
        Memo::new()
    }
}
