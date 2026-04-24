//! Slab-based handle table for runtime-managed values.
//!
//! Handles are opaque `u64` values returned from allocator functions
//! (`__RTS_FN_NS_GC_STRING_NEW`, future object/array/buffer equivalents).
//! The layout encodes a 16-bit generation and a 48-bit slot index so stale
//! handles can be detected cheaply — a handle becomes invalid once its slot
//! is reused with a bumped generation.
//!
//! Threading model: a single global table behind a `Mutex`. Performance is
//! acceptable for the current stage; a per-thread pool is a later concern.

use std::sync::Mutex;
use std::sync::OnceLock;

const GEN_SHIFT: u32 = 48;
const SLOT_MASK: u64 = (1u64 << GEN_SHIFT) - 1;
const SENTINEL_INVALID: u64 = 0;

/// Value kinds stored behind a handle. Extensible as namespaces grow.
#[derive(Debug)]
pub enum Entry {
    /// UTF-8 string owned on the heap.
    String(Vec<u8>),
    /// Fixed-point decimal number, see `bigfloat::fixed::FixedDecimal`.
    ///
    /// Path uses `super::super` (gc's parent) to stay valid in both the
    /// main crate (`namespaces::bigfloat`) and the standalone runtime
    /// staticlib (`crate::bigfloat`).
    BigFixed(Box<super::super::bigfloat::fixed::FixedDecimal>),
    /// Tombstone left by `free`. Reused on next `alloc` with a bumped
    /// generation so dangling handles fail validation.
    Free,
}

#[derive(Debug)]
struct Slot {
    generation: u16,
    entry: Entry,
}

#[derive(Debug, Default)]
pub struct HandleTable {
    slots: Vec<Slot>,
    /// Indices of `Free` slots available for reuse.
    free_list: Vec<u32>,
}

impl HandleTable {
    pub fn alloc(&mut self, entry: Entry) -> u64 {
        if let Some(idx) = self.free_list.pop() {
            let slot = &mut self.slots[idx as usize];
            slot.generation = slot.generation.wrapping_add(1);
            slot.entry = entry;
            return encode(slot.generation, idx);
        }
        let idx = self.slots.len() as u32;
        self.slots.push(Slot {
            generation: 1,
            entry,
        });
        encode(1, idx)
    }

    pub fn free(&mut self, handle: u64) -> bool {
        let Some((expected_gen, idx)) = decode(handle) else {
            return false;
        };
        let Some(slot) = self.slots.get_mut(idx as usize) else {
            return false;
        };
        if slot.generation != expected_gen {
            return false;
        }
        slot.entry = Entry::Free;
        self.free_list.push(idx);
        true
    }

    pub fn get(&self, handle: u64) -> Option<&Entry> {
        let (expected_gen, idx) = decode(handle)?;
        let slot = self.slots.get(idx as usize)?;
        if slot.generation != expected_gen || matches!(slot.entry, Entry::Free) {
            return None;
        }
        Some(&slot.entry)
    }
}

fn encode(generation: u16, slot: u32) -> u64 {
    ((generation as u64) << GEN_SHIFT) | (slot as u64 & SLOT_MASK)
}

fn decode(handle: u64) -> Option<(u16, u32)> {
    if handle == SENTINEL_INVALID {
        return None;
    }
    let generation = ((handle >> GEN_SHIFT) & 0xFFFF) as u16;
    let slot = (handle & SLOT_MASK) as u32;
    Some((generation, slot))
}

/// Global table instance. Exposed to the rest of the runtime so sibling
/// namespaces (bigfloat, etc) can allocate/query handles uniformly.
pub fn table() -> &'static Mutex<HandleTable> {
    static TABLE: OnceLock<Mutex<HandleTable>> = OnceLock::new();
    TABLE.get_or_init(|| Mutex::new(HandleTable::default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_string_entry() {
        let mut t = HandleTable::default();
        let h = t.alloc(Entry::String(b"hello".to_vec()));
        assert!(matches!(t.get(h), Some(Entry::String(b)) if b == b"hello"));
        assert!(t.free(h));
        assert!(t.get(h).is_none());
    }

    #[test]
    fn stale_handle_rejected_after_reuse() {
        let mut t = HandleTable::default();
        let h1 = t.alloc(Entry::String(b"first".to_vec()));
        t.free(h1);
        let h2 = t.alloc(Entry::String(b"second".to_vec()));
        assert!(t.get(h1).is_none(), "stale handle must not resolve");
        assert!(matches!(t.get(h2), Some(Entry::String(_))));
    }
}
