//! `URLSearchParams` — an ordered `Vec<(String, String)>` behind a numbered
//! slot, the same [`TABLE`] shape [`super::class`] uses for `URL`. `size` is
//! a plain data property resynced by hand after every mutation — the same
//! trade `stream/common.rs` names for its own tracked properties, since this
//! crate has no accessor to reach from here (see the module doc).

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use rts_core_rwk::entry::{self, Context, Provided};

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
    ("getAll", get_all),
    ("has", has),
    ("set", set),
    ("sort", sort),
    ("toString", to_string),
    ("forEach", for_each),
    ("keys", keys),
    ("values", values),
    ("entries", entries),
];

/// Builds the `URLSearchParams` class and returns the constructor.
pub(super) fn install(context: &mut Context) -> u64 {
    let prototype = entry::make_prototype(context, "URLSearchParams", METHODS);
    super::class_ctor(context, construct, prototype)
}

/// `new URLSearchParams(init?)` — a string (leading `?` stripped), an array
/// of `[name, value]` pairs, or a plain object of `string`/`string[]`
/// values. Anything else (an iterable that is not an array — this crate has
/// no generic iterator reader) answers empty rather than throwing
/// `ERR_INVALID_TUPLE`.
extern "C" fn construct(_e: u64, this: u64, init: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let pairs = pairs_from(init);
    entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "URLSearchParams", METHODS);
        let instance = super::self_or_new(context, this, prototype);
        install_instance(context, instance, pairs);
        instance
    })
}

fn pairs_from(init: u64) -> Vec<(String, String)> {
    let absent = entry::undefined_value();
    if init == absent {
        return Vec::new();
    }
    if let Some(text) = super::text(init) {
        return pairs_from_query(text.strip_prefix('?').unwrap_or(&text));
    }
    if entry::is_array(init) {
        return collect_array(init)
            .into_iter()
            .filter_map(|pair| {
                let name = entry::text_of(entry::get_indexed(pair, entry::make_number(0.0)))?;
                let value = entry::text_of(entry::get_indexed(pair, entry::make_number(1.0)))?;
                Some((name, value))
            })
            .collect();
    }
    // A plain `Record<string, string | string[]>` — walked the same way
    // `querystring.rs::stringify` walks an options object.
    collect_array(entry::own_keys(init))
        .into_iter()
        .filter_map(|key| entry::text_of(key).map(|name| (name, key)))
        .flat_map(|(name, key)| {
            let value = entry::get_indexed(init, key);
            let values = match entry::is_array(value) {
                true => collect_array(value).into_iter().filter_map(entry::text_of).collect(),
                false => entry::described(value).into_iter().collect::<Vec<_>>(),
            };
            values.into_iter().map(move |value| (name.clone(), value))
        })
        .collect()
}

fn pairs_from_query(text: &str) -> Vec<(String, String)> {
    form_urlencoded::parse(text.as_bytes())
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect()
}

fn install_instance(context: &mut Context, instance: u64, pairs: Vec<(String, String)>) {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let count = pairs.len();
    with_table(|table| {
        table.insert(id, pairs);
    });
    let id_value = entry::make_number(id as f64);
    entry::put_member(context, instance, "__spId", id_value);
    sync_size(context, instance, count);
}

fn sync_size(context: &mut Context, instance: u64, count: usize) {
    let size = entry::make_number(count as f64);
    entry::put_member(context, instance, "size", size);
}

fn id_of(this: u64) -> Option<u64> {
    entry::number_of(entry::get_indexed(this, super::string("__spId"))).map(|value| value as u64)
}

/// A `URLSearchParams` instance built from a `URL`'s own query string — the
/// snapshot [`class::refresh`](super::class) hands off as `url.searchParams`.
/// Not a live view; see the module doc.
pub(super) fn from_query(context: &mut Context, query: &str) -> u64 {
    let prototype = entry::make_prototype(context, "URLSearchParams", METHODS);
    let instance = entry::make_instance(context, prototype);
    install_instance(context, instance, pairs_from_query(query));
    instance
}

extern "C" fn append(_e: u64, this: u64, name: u64, value: u64, _c: u64, _d: u64) -> u64 {
    mutate(this, |pairs| {
        pairs.push((super::text(name).unwrap_or_default(), super::text(value).unwrap_or_default()));
    })
}

/// `params.delete(name, value?)` — every entry named `name` (all of them,
/// when `value` is absent), or only the ones also matching `value`.
extern "C" fn delete(_e: u64, this: u64, name: u64, value: u64, _c: u64, _d: u64) -> u64 {
    let Some(name) = super::text(name) else {
        return entry::undefined_value();
    };
    let value = super::text(value);
    mutate(this, |pairs| {
        pairs.retain(|(key, held)| !(*key == name && value.as_deref().is_none_or(|value| value == held)));
    })
}

extern "C" fn get(_e: u64, this: u64, name: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(name) = super::text(name) else {
        return entry::null_value();
    };
    match read(this, |pairs| pairs.iter().find(|(key, _)| *key == name).map(|(_, value)| value.clone())) {
        Some(Some(value)) => super::string(&value),
        _ => entry::null_value(),
    }
}

extern "C" fn get_all(_e: u64, this: u64, name: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let name = super::text(name).unwrap_or_default();
    let values = read(this, |pairs| {
        pairs.iter().filter(|(key, _)| *key == name).map(|(_, value)| value.clone()).collect::<Vec<_>>()
    })
    .unwrap_or_default();
    entry::with_runtime(|context| {
        let held = values.iter().map(|value| super::string_in(context, value)).collect();
        entry::make_array_in(context, held)
    })
}

extern "C" fn has(_e: u64, this: u64, name: u64, value: u64, _c: u64, _d: u64) -> u64 {
    let Some(name) = super::text(name) else {
        return entry::boolean_value(false);
    };
    let value = super::text(value);
    let found = read(this, |pairs| {
        pairs.iter().any(|(key, held)| *key == name && value.as_deref().is_none_or(|value| value == held))
    })
    .unwrap_or(false);
    entry::boolean_value(found)
}

/// `params.set(name, value)` — replaces every existing entry for `name`
/// with one, at the position of the FIRST one found (or the end, if there
/// were none) — matching the WHATWG algorithm's "in place" wording.
extern "C" fn set(_e: u64, this: u64, name: u64, value: u64, _c: u64, _d: u64) -> u64 {
    let name = super::text(name).unwrap_or_default();
    let value = super::text(value).unwrap_or_default();
    mutate(this, |pairs| {
        let mut replaced = false;
        pairs.retain_mut(|(key, held)| {
            if *key != name {
                return true;
            }
            if replaced {
                return false;
            }
            *held = value.clone();
            replaced = true;
            true
        });
        if !replaced {
            pairs.push((name.clone(), value.clone()));
        }
    })
}

/// `params.sort()` — stable, by name, UTF-16 code-unit order approximated
/// with Rust's own `str` (`char`, i.e. Unicode scalar value) ordering: they
/// agree everywhere but the surrogate range, which cannot occur in a valid
/// Rust `String` to begin with.
extern "C" fn sort(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    mutate(this, |pairs| pairs.sort_by(|(a, _), (b, _)| a.cmp(b)))
}

extern "C" fn to_string(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let pairs = read(this, <[(String, String)]>::to_vec).unwrap_or_default();
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    for (name, value) in &pairs {
        serializer.append_pair(name, value);
    }
    super::string(&serializer.finish())
}

/// `params.forEach(fn, thisArg?)`.
extern "C" fn for_each(_e: u64, this: u64, callback: u64, this_arg: u64, _c: u64, _d: u64) -> u64 {
    let pairs = read(this, <[(String, String)]>::to_vec).unwrap_or_default();
    let receiver = match this_arg == entry::undefined_value() {
        true => this,
        false => this_arg,
    };
    for (name, value) in pairs {
        let (name_value, value_value) = entry::with_runtime(|context| {
            (super::string_in(context, &name), super::string_in(context, &value))
        });
        entry::call(callback, receiver, value_value, name_value, this, entry::undefined_value());
    }
    entry::undefined_value()
}

/// `params.keys()` — a plain array, not an `IterableIterator`; see the
/// module doc.
extern "C" fn keys(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let names = read(this, |pairs| pairs.iter().map(|(name, _)| name.clone()).collect::<Vec<_>>()).unwrap_or_default();
    entry::with_runtime(|context| {
        let held = names.iter().map(|name| super::string_in(context, name)).collect();
        entry::make_array_in(context, held)
    })
}

extern "C" fn values(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let values = read(this, |pairs| pairs.iter().map(|(_, value)| value.clone()).collect::<Vec<_>>()).unwrap_or_default();
    entry::with_runtime(|context| {
        let held = values.iter().map(|value| super::string_in(context, value)).collect();
        entry::make_array_in(context, held)
    })
}

/// `params.entries()` — an array of two-element arrays, the same
/// approximation [`keys`]/[`values`] make.
extern "C" fn entries(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let pairs = read(this, <[(String, String)]>::to_vec).unwrap_or_default();
    entry::with_runtime(|context| {
        let rows = pairs
            .iter()
            .map(|(name, value)| {
                let name_value = super::string_in(context, name);
                let held_value = super::string_in(context, value);
                entry::make_array_in(context, vec![name_value, held_value])
            })
            .collect();
        entry::make_array_in(context, rows)
    })
}

fn read<T>(this: u64, body: impl FnOnce(&[(String, String)]) -> T) -> Option<T> {
    let id = id_of(this)?;
    with_table(|table| table.get(&id).map(|pairs| body(pairs)))
}

fn mutate(this: u64, body: impl FnOnce(&mut Vec<(String, String)>)) -> u64 {
    let Some(id) = id_of(this) else {
        return entry::undefined_value();
    };
    let count = with_table(|table| {
        let Some(pairs) = table.get_mut(&id) else {
            return None;
        };
        body(pairs);
        Some(pairs.len())
    });
    if let Some(count) = count {
        entry::with_runtime(|context| sync_size(context, this, count));
    }
    entry::undefined_value()
}

/// A JS array's elements — the same recipe `querystring.rs::collect_array`
/// and `stream/common.rs::collect_array` both already use.
fn collect_array(array: u64) -> Vec<u64> {
    let absent = entry::undefined_value();
    if array == absent {
        return Vec::new();
    }
    let length = entry::number_of(entry::get_indexed(array, super::string("length"))).unwrap_or(0.0) as usize;
    (0..length).map(|index| entry::get_indexed(array, entry::make_number(index as f64))).collect()
}
