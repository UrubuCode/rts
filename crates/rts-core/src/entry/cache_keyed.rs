//! Filling the cache of a site whose key the program computed.
//!
//! # What this answers that [`super::cache`] does not
//!
//! Nothing about layouts. It resolves an operand to a key number, hands the
//! question to [`super::cache::cache_resolve`] — the same resolver, so the two
//! can never disagree about where a property is — and then writes one extra
//! word the keyed site compares against.
//!
//! Its own module rather than a function in `cache.rs` because that file is 756
//! lines against this crate's 500-line ceiling, and rule 6 says new code lands
//! in a small focused module rather than being appended to one that is over.
//!
//! # Why the key written back is the RAW operand
//!
//! `rts_cranelift::ir::inst::Terminator::CachedGetKeyed` states the design and
//! this half of it is the obligation that crosses the boundary: the machine
//! recognises the next key by comparing the operand's own 64 bits against this
//! word. Writing a normalised key here — the resolved number, or a canonical
//! string — would answer this read correctly and be refused by every read after
//! it. That failure has no symptom: the site never hits, never lies, and looks
//! exactly like a site whose key keeps changing.

use rts_cranelift::symbols::CACHE_KEY_OFFSET;

use super::with_current;
use crate::value::Value;

/// Where a property is, for a site that was handed a key rather than told one.
///
/// Reports a byte offset, or a negative number for anything this path may not
/// serve — and the caller's miss path is the ordinary computed read, which
/// serves all of it correctly and slowly.
///
/// # What it refuses, and why each refusal is a refusal rather than a fix
///
/// **A key that is not a string.** A number reaching `o[k]` may be an array
/// index, and an element is not a property of any shape — there is no offset in
/// the receiver's cell that could answer it. A symbol has a key but reaches it
/// through its own memo, which is a second path this would have to keep in step
/// with; it is left to the miss.
///
/// **A string whose key the registry has not issued.** Nothing to look up.
///
/// Both send the site to `__rts_get_indexed`, which is where every one of those
/// cases is already answered.
#[rtse::entry("rts_cache_resolve_keyed")]
pub fn cache_resolve_keyed(object: u64, key: u64, cache: i64) -> i64 {
    // Resolved in its own borrow, before the resolver takes one: `cache_resolve`
    // opens the context itself, and holding it across that call is the re-entry
    // panic `functions.rs` records as "a deadlock this repository has already
    // paid for once".
    let Some((number, cell)) = with_current(|context| {
        let cell = Value(key).as_slot()?;
        // Text only. `key_of_text_cell` answers `None` for a cell that is not a
        // string, which is what keeps a symbol — also a cell — out of here
        // rather than a second test that could disagree with the first.
        match context.key_of_text_cell(cell)? {
            crate::object::Key::Name(named) => Some((named.index() as u32, cell)),
            // A string that spells a canonical index still resolves to a NAME
            // here, so this arm is not the `o["0"]` case — it is the shape of
            // `Key` being wider than what a text cell can produce. Refused
            // rather than unwrapped, so that widening it later is a decision
            // someone makes instead of a panic someone meets.
            crate::object::Key::Index(_) => None,
        }
    }) else {
        return -1;
    };

    let offset = super::cache::cache_resolve(object, i64::from(number), cache);
    if offset < 0 {
        return offset;
    }

    // Rooted BEFORE the cell is written, and the order is the invariant: the
    // moment the word is there, a collection that ran without this cell in its
    // root set could free the string a site is now comparing against. See
    // `Context::remembered_keys` for what that costs.
    //
    // Marked by CELL rather than by value bits, and the difference is measured:
    // a `HashSet<u64>` here cost **11.3 ns of a 43 ns miss** (2026-08-23), which
    // was more than the key resolution above it. A miss writes this every time,
    // so what it must be is a store, not a hash.
    //
    // Already checked before storing: the mark is set once and read every miss,
    // so a load and a predicted branch beat dirtying a cache line that already
    // holds what it should.
    with_current(|context| {
        let at = cell as usize;
        if context.remembered_keys.len() <= at {
            context.remembered_keys.resize(at + 1, false);
        }
        if !context.remembered_keys[at] {
            context.remembered_keys[at] = true;
        }
    });

    // Only after the layout resolved, and that order is the invariant: the
    // machine hits when BOTH words match, so a key written beside a layout that
    // was refused would be a site claiming to recognise a receiver the resolver
    // just declined.
    //
    // SAFETY: `cache` is the address of the cell this site declared, as
    // everywhere else in this pair of modules, and the word is within the sixty-
    // four bytes every site is given. `CACHE_KEY_OFFSET` is the machine's own
    // constant rather than a number repeated here, which is what stops the two
    // sides drifting.
    unsafe {
        let word = (cache as *mut u8).offset(CACHE_KEY_OFFSET as isize) as *mut u64;
        word.write(key);
    }
    offset
}
