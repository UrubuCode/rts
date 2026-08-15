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
fn prototype(context: &mut Context) -> u64 {
    entry::make_prototype(context, "Headers", METHODS)
}

/// `new Headers(init?)` — a `Headers`, an array of `[name, value]` pairs, or a
/// plain object.
extern "C" fn construct(_e: u64, this: u64, init: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let pairs = pairs_from(init);
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

/// What an `init` argument says, in order.
fn pairs_from(init: u64) -> Vec<(String, String)> {
    let absent = entry::undefined_value();
    if init == absent || init == entry::null_value() {
        return Vec::new();
    }
    // A `Headers` first: it has a list of its own, and reading it through
    // `own_keys` below would find `__headersId` instead.
    if let Some(id) = id_of(init) {
        return with_table(|table| table.get(&id).cloned()).unwrap_or_default();
    }
    if entry::is_array(init) {
        return super::elements(init)
            .into_iter()
            .filter_map(|pair| {
                let parts = super::elements(pair);
                Some((super::text(*parts.first()?)?, super::text(*parts.get(1)?)?))
            })
            .collect();
    }
    entry::with_runtime(|context| entry::member_names(context, init))
        .into_iter()
        .filter_map(|name| {
            let value = entry::get_indexed(init, super::string(&name));
            Some((name, super::text(value)?))
        })
        .collect()
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
    let (Some(name), Some(value)) = (normalized_name(&name), normalized_value(&value)) else {
        return refuse(&name);
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
    entry::undefined_value()
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

extern "C" fn keys(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let names: Vec<String> = rows_of(this).into_iter().map(|(name, _)| name).collect();
    super::string_array(&names)
}

extern "C" fn values(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let held: Vec<String> = rows_of(this).into_iter().map(|(_, value)| value).collect();
    super::string_array(&held)
}

extern "C" fn entries(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let rows = rows_of(this);
    entry::with_runtime(|context| {
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
    })
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
