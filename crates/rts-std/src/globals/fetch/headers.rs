//! `Headers` — the Fetch Standard's header list, with its combining, its
//! ordering and its one exception.
//!
//! # The three rules that make this more than a map
//!
//! **Names are case-insensitive and stored lowercased**, so `X-A` and `x-a` are
//! one name. **A name may repeat**, and `get` answers the values joined with
//! `", "` — which is why the storage is an ordered `Vec` of pairs and not a
//! `HashMap`. And **`set-cookie` is exempt from the join when ITERATING**: the
//! standard yields each of its values as its own entry, because a cookie
//! containing a comma cannot be un-joined afterwards. `getSetCookie()` exists
//! for the same reason.
//!
//! Iteration is sorted by name, which is the standard's rule and Node's
//! behaviour; Bun preserves insertion order. The parent module records the
//! divergence and why this side of it was chosen.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use rts_core::entry::{self, Context, Provided};

/// One header list. Text only, which is what lets it live outside the heap —
/// see the parent module for the entry storage that could not.
static TABLE: Mutex<Option<HashMap<u64, Vec<(String, String)>>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn with_table<T>(body: impl FnOnce(&mut HashMap<u64, Vec<(String, String)>>) -> T) -> T {
    let mut guard = TABLE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let table = guard.get_or_insert_with(HashMap::new);
    body(table)
}

const METHODS: &[(&str, Provided)] = &[
    ("append", append),
    ("delete", delete),
    ("get", get),
    ("getSetCookie", get_set_cookie),
    ("has", has),
    ("set", set),
    ("forEach", for_each),
    ("keys", keys),
    ("values", values),
    ("entries", entries),
];

/// The `Headers` constructor.
pub(super) fn class(context: &mut Context) -> u64 {
    let prototype = prototype(context);
    super::class_of(context, "Headers", prototype, construct)
}

/// The one `Headers.prototype`. Asked for HERE and nowhere else — see
/// [`super::class_of`] for what a second file asking cost.
///
/// `@@iterator` is installed separately from [`METHODS`], as an ALIAS onto the
/// exact `entries` callable `install_host` already made — never a second
/// entry in the table, which is what
/// `Headers.prototype[Symbol.iterator] === Headers.prototype.entries` (a real
/// identity check the standard makes and a fixture pins) needs: `entry`'s
/// installer mints one callable object PER table row, so two rows naming the
/// same Rust function would still be two distinct JS function values.
fn prototype(context: &mut Context) -> u64 {
    let prototype = entry::make_prototype(context, "Headers", METHODS);
    let entries_fn = entry::get_member(context, prototype, "entries");
    entry::put_member(context, prototype, "@@iterator", entries_fn);
    // `Object.prototype.toString.call(new Headers())` — the string-keyed
    // `"@@toStringTag"` convention every host class in this workspace uses for
    // a symbol-keyed member (`node:crypto`'s `webcrypto`, `TextDecoder`, DOM's
    // `Event`).
    let tag = entry::make_string(context, "Headers");
    entry::put_member(context, prototype, "@@toStringTag", tag);
    prototype
}

/// `new Headers(init?)` — a `Headers`, an array of `[name, value]` pairs, or a
/// plain object.
extern "C" fn construct(_e: u64, this: u64, init: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    // `None` means `pairs_from` already raised — a malformed pair, which the
    // standard makes a `TypeError` at construction rather than a partially
    // built `Headers`.
    let Some(pairs) = pairs_from(init) else {
        return entry::undefined_value();
    };
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    with_table(|table| table.insert(id, Vec::new()));
    let instance = entry::with_runtime(|context| {
        let prototype = prototype(context);
        let instance = super::self_or_new(context, this, prototype);
        let held = entry::make_number(id as f64);
        entry::put_member(context, instance, "__headersId", held);
        instance
    });
    // Through `record` rather than straight into the table, so the constructor
    // validates exactly what `append` validates. An init that could not be
    // appended must not be admitted by a different door.
    for (name, value) in pairs {
        if !record(id, &name, &value) {
            return refuse(&name);
        }
    }
    instance
}

/// What an `init` argument says, in order — `None` when it was malformed and
/// has already raised the standard's `TypeError`.
fn pairs_from(init: u64) -> Option<Vec<(String, String)>> {
    let absent = entry::undefined_value();
    if init == absent || init == entry::null_value() {
        return Some(Vec::new());
    }
    // A `Headers` first: it has a list of its own, and reading it through
    // `own_keys` below would find `__headersId` instead.
    if let Some(id) = id_of(init) {
        return Some(with_table(|table| table.get(&id).cloned()).unwrap_or_default());
    }
    if entry::is_array(init) {
        let mut pairs = Vec::new();
        for pair in super::elements(init) {
            let parts = super::elements(pair);
            // Each element must be a NAME-VALUE pair, exactly two long — the
            // standard's `sequence<sequence<ByteString>>` overload, which
            // throws rather than silently reading `undefined` for a missing
            // second slot.
            let (Some(name), Some(value)) = (parts.first(), parts.get(1)) else {
                entry::throw_type_error(&format!(
                    "Failed to construct 'Headers': The provided value cannot be converted to a sequence \
                     because the sequence sequence element has length {}, which is not 2",
                    parts.len()
                ));
                return None;
            };
            let (Some(name), Some(value)) = (super::text(*name), super::text(*value)) else {
                continue;
            };
            pairs.push((name, value));
        }
        return Some(pairs);
    }
    let pairs = entry::with_runtime(|context| entry::member_names(context, init))
        .into_iter()
        .filter_map(|name| {
            let value = entry::get_indexed(init, super::string(&name));
            Some((name, super::text(value)?))
        })
        .collect();
    Some(pairs)
}

fn id_of(this: u64) -> Option<u64> {
    entry::number_of(entry::get_indexed(this, super::string("__headersId"))).map(|id| id as u64)
}

/// A header name, lowercased — `None` when it is not a valid HTTP token.
///
/// Validated rather than accepted, because the standard makes an invalid name a
/// `TypeError` and because a name carrying a `:` or a newline is how a header
/// list becomes a request-splitting bug one layer down. A native can raise now,
/// so the refusal is the specified one rather than a silent drop.
fn normalized_name(name: &str) -> Option<String> {
    let valid = !name.is_empty()
        && name.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || b"!#$%&'*+-.^_`|~".contains(&byte)
        });
    valid.then(|| name.to_ascii_lowercase())
}

/// A header value with leading and trailing HTTP whitespace removed — `None`
/// when it holds a byte no header value may carry.
fn normalized_value(value: &str) -> Option<String> {
    let trimmed = value.trim_matches(|character| matches!(character, ' ' | '\t' | '\r' | '\n'));
    let valid = !trimmed.bytes().any(|byte| matches!(byte, 0 | b'\r' | b'\n'));
    valid.then(|| trimmed.to_owned())
}

/// Raises the `TypeError` the standard owes for a name or value it refuses.
fn refuse(name: &str) -> u64 {
    entry::throw_type_error(&format!("Invalid header name or value: {name}"));
    entry::undefined_value()
}

/// Appends one pair, answering whether it was valid.
fn record(id: u64, name: &str, value: &str) -> bool {
    let (Some(name), Some(value)) = (normalized_name(name), normalized_value(value)) else {
        return false;
    };
    with_table(|table| {
        if let Some(list) = table.get_mut(&id) {
            list.push((name, value));
        }
    });
    true
}

/// `headers.append(name, value)`.
extern "C" fn append(_e: u64, this: u64, name: u64, value: u64, _c: u64, _d: u64) -> u64 {
    let (Some(id), Some(name), Some(value)) = (id_of(this), super::text(name), super::text(value))
    else {
        return entry::undefined_value();
    };
    match record(id, &name, &value) {
        true => entry::undefined_value(),
        false => refuse(&name),
    }
}

/// `headers.set(name, value)` — one entry replaces every existing one, at the
/// position of the first, which is the standard's "in place" wording.
extern "C" fn set(_e: u64, this: u64, name: u64, value: u64, _c: u64, _d: u64) -> u64 {
    let (Some(id), Some(name), Some(value)) = (id_of(this), super::text(name), super::text(value))
    else {
        return entry::undefined_value();
    };
    match set_by_id(id, &name, &value) {
        true => entry::undefined_value(),
        false => refuse(&name),
    }
}

/// The core of `set()`, over an already-resolved id — what
/// [`replace`] needs too.
fn set_by_id(id: u64, name: &str, value: &str) -> bool {
    let (Some(name), Some(value)) = (normalized_name(name), normalized_value(value)) else {
        return false;
    };
    with_table(|table| {
        let Some(list) = table.get_mut(&id) else {
            return;
        };
        let mut replaced = false;
        list.retain_mut(|(held, current)| {
            if *held != name {
                return true;
            }
            if replaced {
                return false;
            }
            *current = value.clone();
            replaced = true;
            true
        });
        if !replaced {
            list.push((name, value));
        }
    });
    true
}

/// Replaces one header on a list a sibling class owns — `set()`'s semantics
/// (overwrite, not append) rather than [`put`]'s: `Response.json()` needs this
/// to force `Content-Type: application/json` over the `text/plain` a plain
/// string body already implied.
pub(super) fn replace(headers: u64, name: &str, value: &str) {
    if let Some(id) = id_of(headers) {
        set_by_id(id, name, value);
    }
}

/// `headers.get(name)` — every value for the name, joined with `", "`, or
/// `null`. `set-cookie` joins here too; only ITERATION treats it apart.
extern "C" fn get(_e: u64, this: u64, name: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(name) = super::text(name).and_then(|name| normalized_name(&name)) else {
        return entry::null_value();
    };
    match read(this, |list| combined(list, &name)) {
        Some(Some(joined)) => super::string(&joined),
        _ => entry::null_value(),
    }
}

/// `headers.getSetCookie()` — every `set-cookie` value, in order, unjoined.
extern "C" fn get_set_cookie(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let values = read(this, |list| {
        list.iter()
            .filter(|(name, _)| name == "set-cookie")
            .map(|(_, value)| value.clone())
            .collect::<Vec<_>>()
    })
    .unwrap_or_default();
    super::string_array(&values)
}

extern "C" fn has(_e: u64, this: u64, name: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(name) = super::text(name).and_then(|name| normalized_name(&name)) else {
        return entry::boolean_value(false);
    };
    let found = read(this, |list| list.iter().any(|(held, _)| *held == name)).unwrap_or(false);
    entry::boolean_value(found)
}

extern "C" fn delete(_e: u64, this: u64, name: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let (Some(id), Some(name)) = (id_of(this), super::text(name).and_then(|name| normalized_name(&name)))
    else {
        return entry::undefined_value();
    };
    with_table(|table| {
        if let Some(list) = table.get_mut(&id) {
            list.retain(|(held, _)| *held != name);
        }
    });
    entry::undefined_value()
}

/// `headers.forEach(fn, thisArg?)`, over the same rows iteration yields.
extern "C" fn for_each(_e: u64, this: u64, callback: u64, this_arg: u64, _c: u64, _d: u64) -> u64 {
    let rows = rows_of(this);
    let receiver = match this_arg == entry::undefined_value() {
        true => this,
        false => this_arg,
    };
    for (name, value) in rows {
        let (name, value) = (super::string(&name), super::string(&value));
        entry::call(callback, receiver, value, name, this, entry::undefined_value());
    }
    entry::undefined_value()
}

/// The rows a `keys`/`values`/`entries` iterator was built over, hidden on the
/// iterator object itself — `@@`-prefixed, so `for`-`in`/`Object.keys` never
/// see it, the same convention `rts-node`'s own ad hoc iterators use (see
/// `crates/rts-node/src/events/on_iterator.rs`).
const ITER_ROWS: &str = "@@#headers_rows";
/// How far a `keys`/`values`/`entries` iterator has walked its rows.
const ITER_AT: &str = "@@#headers_at";

/// `headers.keys()` — a real iterator, not the materialised array this used to
/// answer. `entry::list_iterator` (what `Array`/`Map`/`Set` share) is internal
/// to `rts-core` and not reachable from this crate, so this is a small iterator
/// of its own — the same shape `rts-node`'s `events.on` already builds by hand
/// (a plain object carrying its own `next`), over a list that IS built eagerly:
/// see [`entries`] for why that is fine for a header list and wrong for a live
/// collection.
extern "C" fn keys(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let names: Vec<String> = rows_of(this).into_iter().map(|(name, _)| name).collect();
    let rows = entry::with_runtime(|context| {
        let held: Vec<u64> = names.iter().map(|name| entry::make_string(context, name)).collect();
        entry::make_array_in(context, held)
    });
    made_iterator(rows)
}

extern "C" fn values(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let held: Vec<String> = rows_of(this).into_iter().map(|(_, value)| value).collect();
    let rows = entry::with_runtime(|context| {
        let held: Vec<u64> = held.iter().map(|value| entry::make_string(context, value)).collect();
        entry::make_array_in(context, held)
    });
    made_iterator(rows)
}

/// `headers.entries()`, and `headers[Symbol.iterator]` — the SAME callable, see
/// [`prototype`]. Each call answers a fresh iterator over the rows as they are
/// NOW: a header list is small enough, and read often enough only after being
/// fully assembled, that building the list eagerly costs nothing observable —
/// unlike `Map`/`Set`, nothing in the Fetch Standard promises a `Headers`
/// iterator sees an `append()` made after it was created.
extern "C" fn entries(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let rows = rows_of(this);
    let listed = entry::with_runtime(|context| {
        let built = rows
            .iter()
            .map(|(name, value)| {
                let pair = vec![
                    entry::make_string(context, name),
                    entry::make_string(context, value),
                ];
                entry::make_array_in(context, pair)
            })
            .collect();
        entry::make_array_in(context, built)
    });
    made_iterator(listed)
}

/// A fresh iterator object over an already-built array: `next()` answers
/// `{ value, done }`, walking forward one slot per call, and `@@iterator`
/// answers itself — the two members `for`-`of`, spread and `Array.from` all
/// go through.
fn made_iterator(rows: u64) -> u64 {
    entry::with_runtime(|context| {
        let members: &[(&str, Provided)] = &[("next", iterator_next)];
        let iterator = entry::make_namespace(context, members);
        entry::put_member(context, iterator, ITER_ROWS, rows);
        let zero = entry::make_number(0.0);
        entry::put_member(context, iterator, ITER_AT, zero);
        let itself = entry::make_callable(context, iterator_self);
        entry::put_member(context, iterator, "@@iterator", itself);
        iterator
    })
}

/// `iterator[Symbol.iterator]()` — the iterator itself.
extern "C" fn iterator_self(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    this
}

/// `iterator.next()` — the row the cursor is on, or `{ undefined, true }` past
/// the end, which is left as-is rather than wrapping: an exhausted iterator
/// here stays exhausted, the same rule `list_iterator::next` states.
///
/// Two borrows, never nested: `super::elements` opens its OWN `with_runtime`
/// to read `rows`' indices, so it must run after the first borrow (which only
/// reads the cursor) has already ended — the borrow discipline
/// `authoring-natives.md` states for every native that calls back into the
/// runtime, here between two calls into the SAME crate rather than into user
/// code.
extern "C" fn iterator_next(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let (rows, at) = entry::with_runtime(|context| {
        let rows = entry::get_member(context, this, ITER_ROWS);
        let at = entry::number_of(entry::get_member(context, this, ITER_AT)).unwrap_or(0.0) as usize;
        (rows, at)
    });
    let items = super::elements(rows);
    entry::with_runtime(|context| match items.get(at) {
        Some(value) => {
            let advanced = entry::make_number((at + 1) as f64);
            entry::put_member(context, this, ITER_AT, advanced);
            result(context, *value, false)
        }
        None => {
            let absent = entry::undefined_in(context);
            result(context, absent, true)
        }
    })
}

/// One `{ value, done }` iterator result.
fn result(context: &mut Context, value: u64, done: bool) -> u64 {
    let object = entry::make_object(context);
    entry::put_member(context, object, "value", value);
    entry::put_member(context, object, "done", entry::boolean_value(done));
    object
}

/// Every value for a name, joined — `None` when the name is not present at all,
/// which is the difference between `""` and `null`.
fn combined(list: &[(String, String)], name: &str) -> Option<String> {
    let held: Vec<&str> = list
        .iter()
        .filter(|(held, _)| held == name)
        .map(|(_, value)| value.as_str())
        .collect();
    match held.is_empty() {
        true => None,
        false => Some(held.join(", ")),
    }
}

/// The rows iteration yields: sorted by name, one per name with the values
/// joined — except `set-cookie`, which yields one row per value.
fn rows_of(this: u64) -> Vec<(String, String)> {
    let list = read(this, <[(String, String)]>::to_vec).unwrap_or_default();
    let mut names: Vec<String> = list.iter().map(|(name, _)| name.clone()).collect();
    names.sort();
    names.dedup();
    names
        .into_iter()
        .flat_map(|name| match name == "set-cookie" {
            true => list
                .iter()
                .filter(|(held, _)| *held == name)
                .map(|(held, value)| (held.clone(), value.clone()))
                .collect::<Vec<_>>(),
            false => combined(&list, &name).map(|joined| (name, joined)).into_iter().collect(),
        })
        .collect()
}

fn read<T>(this: u64, body: impl FnOnce(&[(String, String)]) -> T) -> Option<T> {
    let id = id_of(this)?;
    with_table(|table| table.get(&id).map(|list| body(list)))
}

/// One header off a list, for a sibling class reading its own `headers`.
pub(super) fn value_of(headers: u64, name: &str) -> Option<String> {
    read(headers, |list| combined(list, name))?
}

/// Appends one pair to a list a sibling class owns — what
/// [`super::message`] needs to write the `Content-Type` a body implies.
pub(super) fn put(headers: u64, name: &str, value: &str) {
    if let Some(id) = id_of(headers) {
        record(id, name, value);
    }
}

/// A fresh `Headers` over an `init` value, for a sibling class.
pub(super) fn made(init: u64) -> u64 {
    let class = entry::with_runtime(|context| class(context));
    let absent = entry::undefined_value();
    entry::construct(class, init, absent, absent, absent)
}
