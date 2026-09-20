//! The `toJSON` hook, and the key it is called with.

use super::super::super::{Context, with_current};
use crate::value::Value;

/// The value a `toJSON` hook answers, or the value itself.
///
/// # Why the walk can afford this
///
/// The module header used to say this was "a feature with a design" waiting for
/// a caller, and named its cost: every descent probes for the method, releases,
/// calls, and restarts classification on whatever came back. That cost is real
/// and it is paid here — but the caller arrived, and it is correctness rather
/// than a feature. `JSON.stringify(new Date())` and every object with a `toJSON`
/// serialised as `{}`, which is well-formed JSON that lost the value.
///
/// The infinite-walk worry it also named does not happen, and NOT for the reason
/// that first looks right. The hook runs BEFORE the cell is pushed onto the
/// cycle path, so a hook answering the object it hangs off is not seen as a
/// cycle — the walk simply continues into that object once, finds the hook is a
/// function and skips it, and writes `{}`. It terminates. It also does not match
/// the language, which recurses until the stack runs out. Measured by running
/// it, not reasoned about: the first version of this comment claimed the cycle
/// stack caught it, and it does not.
///
/// A primitive is answered before anything is read, so the common member — a
/// number, a string — costs one borrow and no lookup.
///
/// `key` is the property key `toJSON` is called with — see [`Writer::write`]
/// for where each of the three callers gets theirs. It used to be `undefined`
/// unconditionally, because `write` is reached from three places and only one
/// had a key in hand; now all three do, so the hook sees what the
/// specification says it sees rather than a value that happened to be at hand
/// at the one call site that had one.
pub(super) fn to_json_of(value: u64, key: HookKey) -> u64 {
    // The common shape, decided inside ONE borrow: an ordinary object, asked
    // for `toJSON` by KEY. The general route below converts a string cell to a
    // key and then walks the chain through `get_indexed`, which is a second
    // resolution of a name this crate already knows the number of — paid per
    // object value, and answering "absent" for almost all of them.
    //
    // A proxy and a getter are the two cases it hands back, because both call
    // user code and neither may happen while the context is borrowed.
    enum Ask {
        /// An ordinary object, and this is what `toJSON` read as.
        Read(u64),
        /// Not an object at all: nothing to ask.
        Skip,
        /// Ask the long way — a proxy, or an accessor spelling of `toJSON`.
        Slowly,
    }
    let asked = with_current(|context| {
        // A BIGINT is asked too, in as many words: `SerializeJSONProperty`
        // reads `toJSON` when the value is an Object **or a BigInt**. It is the
        // one primitive with that exemption, and it has to be — a bigint has no
        // JSON form, so a hook is the only way a program can give it one, and
        // not looking means the `TypeError` in `write` fires for a value that
        // had an answer. The SLOW route because a bigint is
        // `Value::from_client` and not a cell: there is no cell for
        // `accessor::resolve` to start a chain walk from, and `get_indexed`
        // already knows how one reaches `BigInt.prototype`.
        if super::super::super::bigints::digits_of(context, value).is_some() {
            return Ask::Slowly;
        }
        if !super::super::super::primitive::is_object_in(context, value) {
            return Ask::Skip;
        }
        let Some(cell) = Value(value).as_slot() else {
            return Ask::Slowly;
        };
        if context.proxy_at(cell).is_some() {
            return Ask::Slowly;
        }
        let key = context.well_known("toJSON");
        match super::super::super::accessor::resolve(context, cell, key) {
            super::super::super::accessor::Found::Value(found) => Ask::Read(found),
            super::super::super::accessor::Found::Absent => Ask::Skip,
            super::super::super::accessor::Found::Getter(_) => Ask::Slowly,
        }
    });
    let hook = match asked {
        Ask::Skip => return value,
        Ask::Read(hook) => hook,
        // Through the ordinary read, so an inherited `toJSON` is found — which
        // is how `Date` provides one — and so an accessor spelling of it runs.
        Ask::Slowly => {
            let name = with_current(|context| context.well_known_text("toJSON"));
            super::super::super::computed::get_indexed(value, name)
        }
    };
    if !with_current(|context| super::super::super::modules::is_callable_in(context, hook)) {
        return value;
    }
    // Only HERE does the key become a value, which is the point of `HookKey`:
    // by this line the value is an object AND it has a callable `toJSON`, which
    // almost nothing does. Built eagerly it was a `number_to_string` and a cell
    // per element of every array ever serialised.
    let (key, absent) = with_current(|context| {
        (key.value(context), super::super::super::objects::undefined_of(context))
    });
    super::super::super::functions::call(hook, value, key, absent, absent, absent)
}

/// What `toJSON` will be called with, before anything decides it will be
/// called at all.
///
/// # Why the index is not resolved at the call site
///
/// Because resolving it ALLOCATES — an array member's key is its index
/// ToString'd, which is a `number_to_string` and a string cell — and the site
/// that has the index cannot know whether the member is even an object, let
/// alone whether it has a hook. Every element of every array serialised paid
/// for a value that was then discarded.
#[derive(Clone, Copy)]
pub(in crate::entry::json) enum HookKey {
    /// A key the caller already holds as a value: a property name, or the
    /// empty string the root is serialised under.
    Given(u64),
    /// An array member's position, ToString'd only if a hook is reached.
    Index(usize),
    /// A property the shape walk named, resolved to its one cell only if a hook
    /// is actually reached.
    ///
    /// The plain-object path never materialises a key otherwise — not building
    /// them is the whole of what it saves — so this variant is what keeps a
    /// `toJSON` seeing exactly the value the general path would have shown it.
    Named(rts_cranelift::shape::Key),
}

impl HookKey {
    /// The key as a value, which is what a call's argument is.
    ///
    /// Reached from two places now — `toJSON` and the replacer — and one of
    /// them would otherwise convert an index a second time and disagree with
    /// the first about what `"0"` is.
    pub(super) fn value(self, context: &mut Context) -> u64 {
        match self {
            HookKey::Given(value) => value,
            HookKey::Index(at) => context
                .intern_value(crate::coerce::number_to_string(at as f64))
                .bits(),
            HookKey::Named(key) => context.key_value(key),
        }
    }
}
