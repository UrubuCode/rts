//! `FormData` — an ordered multimap whose values may be objects.
//!
//! # Why the entries are a JS array and not a table
//!
//! Because a value may be a `Blob`, and a `u64` in a process-global Rust map is
//! invisible to the collector: the slot would be reused underneath it and
//! `fd.get("f")` would answer whatever was allocated there next. The entries
//! therefore live in an ordinary JS array hanging off the instance, which the
//! collector reaches by the same walk it reaches every other property. The
//! parent module states the split; [`super::headers`] is the other half of it,
//! and it can use a table precisely because a header value is text.
//!
//! # Why a `Blob` becomes a `File`, and how it is reached
//!
//! `fd.append(name, blob, filename)` is specified to store a **new `File`**
//! carrying that filename, and a program reads `fd.get(name).name` back. `File`
//! lives in `node:buffer`, in a crate this one cannot depend on — so it is
//! reached through the global object and CONSTRUCTED, rather than a second
//! `File` being written here. A second one would make `fd.get(x) instanceof File`
//! false for the object this file just built, which is the failure the whole
//! arrangement exists to avoid. When no `File` global is installed the value is
//! stored as it arrived, which loses the filename and is named below.

use rts_core::entry::{self, Context, Provided};

const METHODS: &[(&str, Provided)] = &[
    ("append", append),
    ("delete", delete),
    ("get", get),
    ("getAll", get_all),
    ("has", has),
    ("set", set),
    ("forEach", for_each),
    ("keys", keys),
    ("values", values),
    ("entries", entries),
];

/// The `FormData` constructor.
pub(super) fn class(context: &mut Context) -> u64 {
    let prototype = prototype(context);
    super::class_of(context, "FormData", prototype, construct)
}

/// The one `FormData.prototype`. Asked for HERE and nowhere else — see
/// [`super::class_of`] for what a second file asking cost.
fn prototype(context: &mut Context) -> u64 {
    entry::make_prototype(context, "FormData", METHODS)
}

/// `new FormData()` — the `form` argument browsers take needs a DOM, so it is
/// ignored here rather than half-read.
extern "C" fn construct(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let prototype = prototype(context);
        let instance = super::self_or_new(context, this, prototype);
        let rows = entry::make_array_in(context, Vec::new());
        entry::put_member(context, instance, "__entries", rows);
        instance
    })
}

/// The rows an instance carries — each a two-element `[name, value]` array.
fn rows_of(this: u64) -> Vec<u64> {
    super::elements(entry::get_indexed(this, super::string("__entries")))
}

/// Replaces the rows, in one write.
fn set_rows(this: u64, rows: Vec<u64>) {
    entry::with_runtime(|context| {
        let held = entry::make_array_in(context, rows);
        entry::put_member(context, this, "__entries", held);
    });
}

/// One row's name and value.
fn parts_of(row: u64) -> Option<(String, u64)> {
    let held = super::elements(row);
    Some((super::text(*held.first()?)?, *held.get(1)?))
}

/// A row.
fn row(name: &str, value: u64) -> u64 {
    entry::with_runtime(|context| {
        let name = entry::make_string(context, name);
        entry::make_array_in(context, vec![name, value])
    })
}

/// The value a `set`/`append` actually stores.
///
/// A string stays a string. A `Blob` becomes a `File` when a filename was
/// given, and the standard's `"blob"` default when one was not — built through
/// the global `File` class; see the module doc for why that route and not a
/// class of this folder's own.
fn stored_value(value: u64, filename: u64) -> u64 {
    let absent = entry::undefined_value();
    // A string argument is never a file, whatever the third argument says.
    if entry::with_runtime(|context| entry::string_in(context, value)).is_some() {
        return value;
    }
    if !entry::with_runtime(|context| entry::is_object(context, value)) {
        return super::string(&super::text(value).unwrap_or_default());
    }
    // Not a blob-shaped object — a plain object goes in as `String(value)`,
    // which is what the standard's `USVString` branch does.
    if entry::get_indexed(value, super::string("size")) == absent {
        return super::string(&super::text(value).unwrap_or_default());
    }
    let class = super::global("File");
    if !entry::with_runtime(|context| entry::is_callable_in(context, class)) {
        return value;
    }
    let name = match filename == absent {
        true => super::string("blob"),
        false => super::string(&super::text(filename).unwrap_or_default()),
    };
    let parts = entry::with_runtime(|context| entry::make_array_in(context, vec![value]));
    entry::construct(class, parts, name, absent, absent)
}

/// `fd.append(name, value, filename?)`.
extern "C" fn append(_e: u64, this: u64, name: u64, value: u64, filename: u64, _d: u64) -> u64 {
    let Some(name) = super::text(name) else {
        return entry::undefined_value();
    };
    let value = stored_value(value, filename);
    let mut rows = rows_of(this);
    rows.push(row(&name, value));
    set_rows(this, rows);
    entry::undefined_value()
}

/// `fd.set(name, value, filename?)` — one entry replaces every existing one, at
/// the position of the first.
extern "C" fn set(_e: u64, this: u64, name: u64, value: u64, filename: u64, _d: u64) -> u64 {
    let Some(name) = super::text(name) else {
        return entry::undefined_value();
    };
    let value = stored_value(value, filename);
    let mut replaced = false;
    let mut kept = Vec::new();
    for held in rows_of(this) {
        let Some((held_name, _)) = parts_of(held) else {
            continue;
        };
        if held_name != name {
            kept.push(held);
            continue;
        }
        if !replaced {
            kept.push(row(&name, value));
            replaced = true;
        }
    }
    if !replaced {
        kept.push(row(&name, value));
    }
    set_rows(this, kept);
    entry::undefined_value()
}

extern "C" fn get(_e: u64, this: u64, name: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(name) = super::text(name) else {
        return entry::null_value();
    };
    rows_of(this)
        .into_iter()
        .filter_map(parts_of)
        .find(|(held, _)| *held == name)
        .map_or_else(entry::null_value, |(_, value)| value)
}

extern "C" fn get_all(_e: u64, this: u64, name: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let name = super::text(name).unwrap_or_default();
    let held: Vec<u64> = rows_of(this)
        .into_iter()
        .filter_map(parts_of)
        .filter(|(held, _)| *held == name)
        .map(|(_, value)| value)
        .collect();
    entry::with_runtime(|context| entry::make_array_in(context, held))
}

extern "C" fn has(_e: u64, this: u64, name: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let name = super::text(name).unwrap_or_default();
    let found = rows_of(this)
        .into_iter()
        .filter_map(parts_of)
        .any(|(held, _)| held == name);
    entry::boolean_value(found)
}

extern "C" fn delete(_e: u64, this: u64, name: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let name = super::text(name).unwrap_or_default();
    let kept: Vec<u64> = rows_of(this)
        .into_iter()
        .filter(|row| parts_of(*row).is_some_and(|(held, _)| held != name))
        .collect();
    set_rows(this, kept);
    entry::undefined_value()
}

/// `fd.forEach(fn, thisArg?)`.
extern "C" fn for_each(_e: u64, this: u64, callback: u64, this_arg: u64, _c: u64, _d: u64) -> u64 {
    let receiver = match this_arg == entry::undefined_value() {
        true => this,
        false => this_arg,
    };
    for held in rows_of(this) {
        let Some((name, value)) = parts_of(held) else {
            continue;
        };
        let name = super::string(&name);
        entry::call(callback, receiver, value, name, this, entry::undefined_value());
    }
    entry::undefined_value()
}

extern "C" fn keys(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let names: Vec<String> = rows_of(this).into_iter().filter_map(parts_of).map(|(name, _)| name).collect();
    super::string_array(&names)
}

extern "C" fn values(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let held: Vec<u64> = rows_of(this).into_iter().filter_map(parts_of).map(|(_, value)| value).collect();
    entry::with_runtime(|context| entry::make_array_in(context, held))
}

/// `fd.entries()` — the rows themselves, which are already `[name, value]`
/// arrays. A fresh outer array, so a program mutating the result does not
/// rewrite the form.
extern "C" fn entries(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let rows = rows_of(this);
    entry::with_runtime(|context| entry::make_array_in(context, rows))
}
