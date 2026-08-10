//! RFC 7541 §2.3.2/§4 — the dynamic table: a FIFO of `(name, value)` pairs,
//! newest at index 1 (indices continue past the static table's 61 entries on
//! the wire), evicted oldest-first to stay under a byte budget.
//!
//! One instance per direction per session — `http2.md` §4's "HPACK dynamic
//! table" note is why: `maxDeflateDynamicTableSize` bounds this side's
//! outbound table, the peer's advertised `SETTINGS_HEADER_TABLE_SIZE` bounds
//! the inbound one, and they are never the same object.

#[derive(Clone)]
struct Entry {
    name: String,
    value: String,
}

/// RFC 7541 §4.1 — each entry costs its name/value lengths plus 32 bytes of
/// bookkeeping overhead, by definition, not measurement.
const ENTRY_OVERHEAD: usize = 32;

/// A FIFO of `(name, value)` pairs with byte-budgeted eviction — RFC 7541
/// §2.3.2/§4. One per session direction; see the module doc.
pub struct DynamicTable {
    entries: std::collections::VecDeque<Entry>,
    size: usize,
    max_size: usize,
}

impl DynamicTable {
    /// An empty table budgeted at `max_size` bytes.
    pub fn new(max_size: usize) -> Self {
        Self { entries: std::collections::VecDeque::new(), size: 0, max_size }
    }

    fn entry_size(name: &str, value: &str) -> usize {
        name.len() + value.len() + ENTRY_OVERHEAD
    }

    /// Inserts a new entry at the front, evicting from the back until the
    /// table fits — RFC 7541 §4.4. An entry larger than the whole table is
    /// simply not stored (the table becomes empty, matching the spec's "an
    /// attempt to add an entry larger than the maximum size... results in
    /// the table being emptied").
    pub fn insert(&mut self, name: String, value: String) {
        let needed = Self::entry_size(&name, &value);
        while self.size + needed > self.max_size {
            match self.entries.pop_back() {
                Some(evicted) => self.size -= Self::entry_size(&evicted.name, &evicted.value),
                None => break,
            }
        }
        if needed > self.max_size {
            return;
        }
        self.size += needed;
        self.entries.push_front(Entry { name, value });
    }

    /// `SETTINGS_HEADER_TABLE_SIZE`/`Dynamic Table Size Update` (RFC 7541
    /// §6.3) changing the budget — shrinking evicts immediately.
    pub fn set_max_size(&mut self, max_size: usize) {
        self.max_size = max_size;
        while self.size > self.max_size {
            match self.entries.pop_back() {
                Some(evicted) => self.size -= Self::entry_size(&evicted.name, &evicted.value),
                None => break,
            }
        }
    }

    /// The `(name, value)` at a dynamic-table index, where 1 is the most
    /// recently inserted entry (the caller has already subtracted the
    /// static table's 61).
    pub fn at(&self, index: usize) -> Option<(&str, &str)> {
        index
            .checked_sub(1)
            .and_then(|i| self.entries.get(i))
            .map(|entry| (entry.name.as_str(), entry.value.as_str()))
    }

    /// How many entries the table currently holds.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the table currently holds no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_read_back() {
        let mut table = DynamicTable::new(4096);
        table.insert("custom-header".into(), "value".into());
        assert_eq!(table.at(1), Some(("custom-header", "value")));
    }

    #[test]
    fn newest_is_index_one() {
        let mut table = DynamicTable::new(4096);
        table.insert("first".into(), "a".into());
        table.insert("second".into(), "b".into());
        assert_eq!(table.at(1), Some(("second", "b")));
        assert_eq!(table.at(2), Some(("first", "a")));
    }

    #[test]
    fn eviction_drops_oldest_first() {
        let mut table = DynamicTable::new(Entry_cost("a", "1") + Entry_cost("b", "2"));
        table.insert("a".into(), "1".into());
        table.insert("b".into(), "2".into());
        assert_eq!(table.len(), 2);
        table.insert("c".into(), "3".into());
        assert_eq!(table.len(), 2);
        assert_eq!(table.at(1), Some(("c", "3")));
        assert_eq!(table.at(2), Some(("b", "2")));
    }

    #[test]
    fn shrinking_budget_evicts() {
        let mut table = DynamicTable::new(4096);
        table.insert("a".into(), "1".into());
        table.insert("b".into(), "2".into());
        table.set_max_size(Entry_cost("b", "2"));
        assert_eq!(table.len(), 1);
        assert_eq!(table.at(1), Some(("b", "2")));
    }

    #[allow(non_snake_case)]
    fn Entry_cost(name: &str, value: &str) -> usize {
        DynamicTable::entry_size(name, value)
    }
}
