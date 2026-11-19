//! The hash rustc uses for its own maps (`FxHash`): a multiply and a rotate per word, where the standard `SipHash` costs a
//! few rounds per word. The resolver's maps are keyed by package names and constraints from repository metadata, and
//! hashing them was half of the pool optimizer's time.

use std::hash::{BuildHasherDefault, Hasher};

#[derive(Default, Clone, Copy)]
pub(super) struct Fx(u64);

impl Hasher for Fx {
    fn write(&mut self, bytes: &[u8]) {
        let (words, rest) = bytes.as_chunks::<8>();
        for w in words {
            self.add(u64::from_le_bytes(*w));
        }
        if !rest.is_empty() {
            let mut last = [0u8; 8];
            last[..rest.len()].copy_from_slice(rest);
            self.add(u64::from_le_bytes(last));
        }
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

impl Fx {
    fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

pub(super) type Build = BuildHasherDefault<Fx>;
pub(super) type HashMap<K, V> = std::collections::HashMap<K, V, Build>;
pub(super) type HashSet<T> = std::collections::HashSet<T, Build>;
pub(super) type IndexMap<K, V> = indexmap::IndexMap<K, V, Build>;
pub(super) type IndexSet<T> = indexmap::IndexSet<T, Build>;
