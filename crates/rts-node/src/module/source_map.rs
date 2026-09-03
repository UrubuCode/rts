//! `module.SourceMap` — a source-map v3 payload, and the two lookups over it.
//!
//! # Reuse-check
//!
//! Nothing decodes base64-VLQ anywhere in the workspace. `node:buffer`'s codec
//! (reached through `entry::decode_base64`) turns base64 text into BYTES, which
//! is a different scheme over the same 64 characters: a `mappings` field is a
//! sequence of variable-length integers, each carrying a continuation bit and a
//! sign bit, whose values are running deltas rather than octets. Feeding it to
//! the byte decoder would decode successfully and mean nothing, so [`decode`]
//! is written here.
//!
//! # Why the payload lives on the instance rather than in a table beside it
//!
//! `url/mod.rs` and `fs/dir.rs` keep a `Mutex`-backed table keyed by a number
//! the instance carries, because a parsed `url::Url` is a Rust value no JS
//! object can hold. A source map has the opposite shape: Node exposes the
//! payload AS a property (`map.payload`), it is already an ordinary JS object,
//! and the decode is a pure function of it. A table would be a second copy of
//! something the program can mutate, and the two would disagree the moment it
//! did.
//!
//! The cost is named rather than hidden: `mappings` is decoded on every
//! `findEntry`/`findOrigin` call, which is linear in the size of the map. A
//! cache would have to be invalidated by a payload write nothing here observes.
//!
//! # Not implemented, by name
//!
//! - **The `lineLengths` option.** It only feeds Node's own line-length
//!   fallback lookup, which this decoder does not have; passing it changes
//!   nothing here, so it is not read.
//! - **`payload` validation.** Node throws for a payload that is not a v3 map.
//!   No function on this crate's host surface raises a catchable exception —
//!   `entry::throw` ends the program rather than unwinding to a handler — so a
//!   malformed payload yields an empty lookup result, the same trade
//!   `url/mod.rs` states for its own module.
//! - **`sourcesContent`.** Carried on the payload a program reads back, and
//!   never consulted: neither lookup answers source text.

use rts_core::entry::{self, Context, Provided};

/// The `SourceMap` constructor, with its prototype.
///
/// `url/mod.rs::class_ctor`'s recipe, not reached for: that helper is
/// `pub(super)` to the `url` module and widening another module's visibility
/// for one caller costs more than the two lines.
pub(super) fn install(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[("findEntry", find_entry), ("findOrigin", find_origin)];
    let prototype = entry::make_prototype(context, "SourceMap", members);
    let constructor = entry::make_callable(context, construct);
    entry::put_member(context, constructor, "prototype", prototype);
    // Back-link — see `crate::stream::class_ctor`'s doc.
    entry::declare_host_class(context, constructor, prototype, "SourceMap", 1);
    constructor
}

/// `new SourceMap(payload[, options])`.
///
/// The prototype is asked for by name rather than captured: `make_prototype` is
/// idempotent by name, so this answers the same object `install` built, and a
/// native has no environment to have captured it in.
extern "C" fn construct(_e: u64, this: u64, payload: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "SourceMap", &[]);
        // `this` is already an object when `new` over a subclass handed one in;
        // otherwise this call is the construction.
        let instance = match entry::is_object(context, this) {
            true => this,
            false => entry::make_instance(context, prototype),
        };
        entry::put_member(context, instance, "payload", payload);
        instance
    })
}

/// `map.findEntry(lineOffset, columnOffset)` — the mapping covering a generated
/// position, or `{}`.
///
/// Both arguments are 0-based, and every number on the answer is 0-based too:
/// that is what Node exposes, and shifting them here would make `findOrigin`
/// (which is 1-based on the way in and out) shift them back.
extern "C" fn find_entry(_e: u64, this: u64, line: u64, column: u64, _c: u64, _d: u64) -> u64 {
    let (Some(line), Some(column)) = (entry::number_of(line), entry::number_of(column)) else {
        return empty();
    };
    let map = Payload::read(this);
    let Some(found) = map.lookup(line, column) else {
        return empty();
    };
    let source = found.source.and_then(|index| map.sources.get(index).cloned());
    let name = found.name.and_then(|index| map.names.get(index).cloned());
    entry::with_runtime(|context| {
        let entry_object = entry::make_object(context);
        put_number(context, entry_object, "generatedLine", found.line);
        put_number(context, entry_object, "generatedColumn", found.column);
        // A one-field segment maps a generated position to nothing — there is
        // no original source for it — so the original fields are absent rather
        // than zero, which would name the first line of the first source.
        if let Some(source) = source {
            let text = entry::make_string(context, &source);
            entry::put_member(context, entry_object, "originalSource", text);
            put_number(context, entry_object, "originalLine", found.original_line);
            put_number(context, entry_object, "originalColumn", found.original_column);
        }
        if let Some(name) = name {
            let text = entry::make_string(context, &name);
            entry::put_member(context, entry_object, "name", text);
        }
        entry_object
    })
}

/// `map.findOrigin(lineNumber, columnNumber)` — the original position, or `{}`.
///
/// `lineNumber` is 1-based on the way in and on the way out, which is the one
/// place this API differs from [`find_entry`]; the column is 0-based in both.
extern "C" fn find_origin(_e: u64, this: u64, line: u64, column: u64, _c: u64, _d: u64) -> u64 {
    let (Some(line), Some(column)) = (entry::number_of(line), entry::number_of(column)) else {
        return empty();
    };
    let map = Payload::read(this);
    let Some(found) = map.lookup(line - 1.0, column) else {
        return empty();
    };
    let Some(source) = found.source.and_then(|index| map.sources.get(index).cloned()) else {
        return empty();
    };
    let name = found.name.and_then(|index| map.names.get(index).cloned());
    entry::with_runtime(|context| {
        let origin = entry::make_object(context);
        let text = entry::make_string(context, &source);
        entry::put_member(context, origin, "fileName", text);
        put_number(context, origin, "lineNumber", found.original_line + 1);
        put_number(context, origin, "columnNumber", found.original_column);
        // `name` is `undefined` on Node's origin when the segment carries no
        // name index, and present-with-undefined is what a program destructures.
        let value = match name {
            Some(name) => entry::make_string(context, &name),
            None => entry::undefined_in(context),
        };
        entry::put_member(context, origin, "name", value);
        origin
    })
}

/// One decoded segment.
struct Segment {
    line: i64,
    column: i64,
    source: Option<usize>,
    original_line: i64,
    original_column: i64,
    name: Option<usize>,
}

/// What a lookup needs off the payload, read out of the JS object once.
struct Payload {
    segments: Vec<Segment>,
    sources: Vec<String>,
    names: Vec<String>,
}

impl Payload {
    /// Reads `this.payload`.
    ///
    /// Every read here is AMBIENT (`get_indexed`, `is_array`) and therefore
    /// outside any borrow — a property read can reach a getter, which is user
    /// code, which is why this crate's surface refuses to do it under a held
    /// context. The result is plain Rust data, so the object built afterwards
    /// needs only one borrow.
    fn read(this: u64) -> Self {
        let payload = member(this, "payload");
        let mappings = string_of(member(payload, "mappings")).unwrap_or_default();
        let root = string_of(member(payload, "sourceRoot")).unwrap_or_default();
        let sources = strings(member(payload, "sources"))
            .into_iter()
            .map(|source| join_root(&root, &source))
            .collect();
        Self {
            segments: decode(&mappings),
            sources,
            names: strings(member(payload, "names")),
        }
    }

    /// The last segment at or before a generated position.
    ///
    /// A linear scan over an already-ordered list rather than a binary search:
    /// the list is rebuilt on every call (see the module doc), so the decode
    /// already costs what the scan does and a binary search would only make the
    /// cheaper half of the work cheaper.
    fn lookup(&self, line: f64, column: f64) -> Option<&Segment> {
        let (line, column) = (line as i64, column as i64);
        self.segments
            .iter()
            .take_while(|segment| {
                segment.line < line || (segment.line == line && segment.column <= column)
            })
            .last()
    }
}

/// A source under the payload's `sourceRoot`, which is a prefix rather than a
/// path join — a root may end in a separator or not, and may not be a path at
/// all (a URL prefix is legal).
fn join_root(root: &str, source: &str) -> String {
    match root.is_empty() || root.ends_with('/') {
        true => format!("{root}{source}"),
        false => format!("{root}/{source}"),
    }
}

/// The `mappings` field, as segments in generated order.
///
/// The four running values (source index, original line, original column, name
/// index) carry ACROSS lines and the generated column resets at each `;` —
/// that asymmetry is the format, and getting it backwards yields a map that
/// decodes without error and points at the wrong lines.
fn decode(mappings: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let (mut source, mut original_line, mut original_column, mut name) = (0i64, 0i64, 0i64, 0i64);
    for (line, group) in mappings.split(';').enumerate() {
        let mut column = 0i64;
        for field in group.split(',').filter(|field| !field.is_empty()) {
            let values = vlq(field);
            // 1, 4 and 5 are the legal widths; anything else is a malformed
            // segment, and skipping it keeps the running values consistent for
            // the rest of the map rather than shifting everything after it.
            if values.len() != 1 && values.len() != 4 && values.len() != 5 {
                continue;
            }
            column += values[0];
            let mapped = values.len() >= 4;
            if mapped {
                source += values[1];
                original_line += values[2];
                original_column += values[3];
            }
            if values.len() == 5 {
                name += values[4];
            }
            segments.push(Segment {
                line: line as i64,
                column,
                source: mapped.then(|| source.max(0) as usize),
                original_line,
                original_column,
                name: (values.len() == 5).then(|| name.max(0) as usize),
            });
        }
    }
    segments
}

/// One segment's base64-VLQ integers.
fn vlq(field: &str) -> Vec<i64> {
    let mut values = Vec::new();
    let (mut accumulated, mut shift) = (0i64, 0u32);
    for character in field.chars() {
        let Some(digit) = sextet(character) else {
            return Vec::new();
        };
        accumulated += i64::from(digit & 31) << shift;
        // The high bit says another sextet follows; the low bit of the FIRST
        // one is the sign, which is why it is read only when the value closes.
        if digit & 32 != 0 {
            shift += 5;
            continue;
        }
        let negative = accumulated & 1 != 0;
        let magnitude = accumulated >> 1;
        values.push(match negative {
            true => -magnitude,
            false => magnitude,
        });
        accumulated = 0;
        shift = 0;
    }
    values
}

/// A base64 character's six bits.
fn sextet(character: char) -> Option<u8> {
    let value = match character {
        'A'..='Z' => character as u8 - b'A',
        'a'..='z' => character as u8 - b'a' + 26,
        '0'..='9' => character as u8 - b'0' + 52,
        '+' => 62,
        '/' => 63,
        _ => return None,
    };
    Some(value)
}

/// A property of an object, by name.
fn member(object: u64, name: &str) -> u64 {
    let key = entry::with_runtime(|context| entry::make_string(context, name));
    entry::get_indexed(object, key)
}

/// A value's text, only when it really is a string.
fn string_of(value: u64) -> Option<String> {
    entry::with_runtime(|context| entry::string_in(context, value))
}

/// An array of strings, empty for anything that is not one.
///
/// A non-string element becomes the empty string rather than being dropped,
/// because a source map's indices are POSITIONS in this array — dropping one
/// would silently rename every source after it.
fn strings(value: u64) -> Vec<String> {
    if !entry::is_array(value) {
        return Vec::new();
    }
    let length = entry::number_of(member(value, "length")).unwrap_or(0.0);
    let length = length.max(0.0) as usize;
    (0..length)
        .map(|index| {
            let held = entry::get_indexed(value, entry::make_number(index as f64));
            string_of(held).unwrap_or_default()
        })
        .collect()
}

/// A number property, from a context already in hand.
fn put_number(context: &mut Context, object: u64, name: &str, value: i64) {
    let number = entry::make_number(value as f64);
    entry::put_member(context, object, name, number);
}

/// `{}` — what both lookups answer when nothing maps.
fn empty() -> u64 {
    entry::with_runtime(entry::make_object)
}
