//! The generated symbol table's shared vocabulary: one entry type, one ordering
//! invariant, one lookup.
//!
//! `rts-symbol-baker` emits a `.rs` file whose `symbols()` returns a
//! `Vec<SymbolEntry>` built with these types. The engine installs that vector
//! into the `JITBuilder`; the AOT path declares [`AotSymbols`] names into the
//! object module. Both sides read the SAME table, so a symbol cannot exist for
//! one path and be missing from the other.
//!
//! # The ordering invariant (binding)
//!
//! **Entries are in strictly ascending byte order of `name`, with no
//! duplicates.** The baker sorts before emitting and rejects a duplicate name
//! outright, so a re-bake of unchanged sources is byte-identical.
//!
//! Two properties fall out of that single rule, and they are the reason the
//! owner asked for an ORDERED table rather than a `HashMap` built at startup:
//!
//! - **Lookup is a binary search, not a scan or a hash build.** [`lookup`] is
//!   `O(log n)` over a slice already in memory — no allocation, no hashing, no
//!   startup cost proportional to the table.
//! - **Every scope is a contiguous RANGE.** The prefixes sort as
//!   `__RTS…` < `__rtsa_` < `__rtsadp_` < `__rtsm_` < `__rtsn_`, and within
//!   `__rtsm_` a module's members sort together (`__rtsm_node_fs_*` is one
//!   unbroken run). So "all members of `node:fs`" is [`range_with_prefix`] — two
//!   binary searches returning a subslice — instead of a filter over the whole
//!   table. A future resolver can select the candidate range from the prefix
//!   alone and never touch, let alone match against, an unrelated scope.

/// One installable symbol: an `extern "C"` linker name and its address.
///
/// The pointer is to a `#[no_mangle] extern "C"` function with static lifetime.
#[derive(Clone, Copy)]
pub struct SymbolEntry {
    /// The exact linker name, e.g. `__rtsm_io_print`.
    pub name: &'static str,
    /// The function's address, never dereferenced as data.
    pub ptr: *const u8,
}

// SAFETY: every `ptr` is the address of static `extern "C"` code in the linked
// image, never read or written as data — sound to share across threads.
unsafe impl Send for SymbolEntry {}
unsafe impl Sync for SymbolEntry {}

/// The AOT side of the table: the linker names to declare into the object
/// module, in the same order as [`SymbolEntry`] rows.
pub type AotSymbols = &'static [&'static str];

/// Address of `name`, or `None`. Requires the ordering invariant above.
pub fn lookup(table: &[SymbolEntry], name: &str) -> Option<*const u8> {
    table
        .binary_search_by(|e| e.name.cmp(name))
        .ok()
        .map(|i| table[i].ptr)
}

/// The contiguous subslice of entries whose name starts with `prefix`.
///
/// Sorted order makes this two binary searches; the result is the whole scope
/// (`__rtsm_node_fs_`) or the whole prefix class (`__rtsa_`) with no scan.
pub fn range_with_prefix<'a>(table: &'a [SymbolEntry], prefix: &str) -> &'a [SymbolEntry] {
    let start = table.partition_point(|e| e.name < prefix);
    let end = start
        + table[start..].partition_point(|e| e.name.starts_with(prefix));
    &table[start..end]
}

/// Debug-assert the invariant on a freshly built table.
///
/// The baker guarantees it statically; this is the cheap runtime witness for a
/// table that was hand-edited or merged from two sources.
pub fn is_sorted_unique(table: &[SymbolEntry]) -> bool {
    table.windows(2).all(|w| w[0].name < w[1].name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(names: &'static [&'static str]) -> Vec<SymbolEntry> {
        names
            .iter()
            .map(|n| SymbolEntry {
                name: n,
                ptr: n.as_ptr(),
            })
            .collect()
    }

    #[test]
    fn lookup_finds_and_misses() {
        let table = t(&["__rtsa_obj_get", "__rtsm_io_print", "__rtsn_fmod"]);
        assert!(lookup(&table, "__rtsm_io_print").is_some());
        assert!(lookup(&table, "__rtsm_io_nope").is_none());
    }

    #[test]
    fn a_scope_is_one_contiguous_range() {
        let table = t(&[
            "__rtsa_obj_get",
            "__rtsm_io_print",
            "__rtsm_node_fs_readFileSync",
            "__rtsm_node_fs_writeFileSync",
            "__rtsn_fmod",
        ]);
        assert!(is_sorted_unique(&table));
        let fs = range_with_prefix(&table, "__rtsm_node_fs_");
        assert_eq!(fs.len(), 2);
        assert_eq!(range_with_prefix(&table, "__rtsn_").len(), 1);
        assert_eq!(range_with_prefix(&table, "__rtsz_").len(), 0);
    }
}
