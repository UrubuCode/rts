//! `URL` — the WHATWG parser (`url` crate) behind an instance carrying a
//! numbered slot into [`TABLE`], the same shape `fs/dir.rs`'s `Cursor` table
//! uses.
//!
//! # Why the instance carries no snapshot properties
//!
//! Every one of `href`, `protocol`, `username`, `password`, `host`, `hostname`,
//! `port`, `pathname`, `search`, `hash` and `origin` is an accessor pair on
//! `URL.prototype` — see the module doc for what made that reachable. It has to
//! be an accessor rather than a data property refreshed after each write for a
//! reason that is not merely tidiness: `accessor::setter_for` stops its chain
//! walk at an own data property, so an instance carrying its own `pathname`
//! would make `url.pathname = "/x"` write that property and never reach the
//! setter — the exact silent no-op this module used to document as a
//! limitation.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use rts_core::entry::{self, Context, Provided};

static TABLE: Mutex<Option<HashMap<u64, url::Url>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn with_table<T>(body: impl FnOnce(&mut HashMap<u64, url::Url>) -> T) -> T {
    let mut guard = TABLE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let table = guard.get_or_insert_with(HashMap::new);
    body(table)
}

const METHODS: &[(&str, Provided)] = &[("toString", to_string), ("toJSON", to_string)];

/// The accessor pairs `URL.prototype` carries: name, getter, setter.
///
/// `origin` has no setter because the specification gives it none — it is
/// derived from the scheme and the host, and there is nothing to write.
const PROPERTIES: &[(&str, Provided, Option<Provided>)] = &[
    ("href", get_href, Some(set_href)),
    ("protocol", get_protocol, Some(set_protocol)),
    ("username", get_username, Some(set_username)),
    ("password", get_password, Some(set_password)),
    ("host", get_host, Some(set_host)),
    ("hostname", get_hostname, Some(set_hostname)),
    ("port", get_port, Some(set_port)),
    ("pathname", get_pathname, Some(set_pathname)),
    ("search", get_search, Some(set_search)),
    ("hash", get_hash, Some(set_hash)),
    ("origin", get_origin, None),
];

/// Builds the `URL` class and its statics, and returns the constructor.
pub(super) fn install(context: &mut Context) -> u64 {
    let prototype = entry::make_prototype(context, "URL", METHODS);
    let ctor = super::class_ctor(context, "URL", 1, construct, prototype);
    entry::put_member(context, prototype, "constructor", ctor);
    let can_parse_fn = entry::make_callable(context, can_parse);
    entry::put_member(context, ctor, "canParse", can_parse_fn);
    let parse_fn = entry::make_callable(context, static_parse);
    entry::put_member(context, ctor, "parse", parse_fn);
    ctor
}

/// The one `URL.prototype`, with [`PROPERTIES`] installed on it.
///
/// # Why the accessors arrive here and not in [`install`]
///
/// `entry::define_getter` is ambient: it takes its own borrow of the context.
/// [`install`] is handed a `&mut Context` by the host BEFORE any context is
/// installed on the thread, so calling it from there reaches for a thread-local
/// that is not set. Every path that can produce a `URL` instance is a native
/// instead, and a native runs with a context on the thread and no borrow held.
///
/// # Why it asks rather than remembers
///
/// A `static DONE: AtomicBool` would be process-global, and a context is
/// per-thread — a second thread running a program would find the flag already
/// set and a prototype with no accessors on it. Reading one back asks the only
/// authority there is. `origin` is the probe because it is read-only: a
/// receiver with no `__urlId` answers `""` through the getter and `undefined`
/// when there is no getter, and those two are never confusable.
fn prototype() -> u64 {
    let prototype = entry::with_runtime(|context| entry::make_prototype(context, "URL", METHODS));
    if entry::get_indexed(prototype, super::string("origin")) != entry::undefined_value() {
        return prototype;
    }
    for (name, getter, setter) in PROPERTIES {
        super::define_accessor(prototype, name, *getter, *setter);
    }
    prototype
}

/// `new URL(input, base?)`. Malformed input answers an INERT instance — one
/// carrying no `__urlId` — rather than throwing; see the module doc.
extern "C" fn construct(_e: u64, this: u64, input: u64, base: u64, _c: u64, _d: u64) -> u64 {
    let prototype = prototype();
    let Some(text) = super::text(input) else {
        return entry::with_runtime(|context| super::self_or_new(context, this, prototype));
    };
    let base_text = base_text_of(base);
    let parsed = parse_with_base(&text, base_text.as_deref());
    let instance = entry::with_runtime(|context| super::self_or_new(context, this, prototype));
    if let Some(parsed) = parsed {
        install_instance(instance, parsed);
    }
    instance
}

/// `base`, as text — either a plain string argument or a `URL` instance, whose
/// `href` getter answers the serialization rather than a second parse.
fn base_text_of(base: u64) -> Option<String> {
    let absent = entry::undefined_value();
    if base == absent {
        return None;
    }
    if let Some(href) = super::get_text(base, "href").filter(|href| !href.is_empty()) {
        return Some(href);
    }
    super::text(base)
}

fn parse_with_base(text: &str, base: Option<&str>) -> Option<url::Url> {
    match base {
        Some(base) => {
            let base = url::Url::parse(base).ok()?;
            url::Url::options().base_url(Some(&base)).parse(text).ok()
        }
        None => url::Url::parse(text).ok(),
    }
}

/// Records the parsed `url::Url` in [`TABLE`] and hangs the two own properties
/// an instance has on it.
///
/// Ambient, and every property below is written in ONE borrow after the table
/// insert: `search_params::bound_to` needs the id, and the id has to exist
/// before anything can read through it.
fn install_instance(instance: u64, parsed: url::Url) {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    with_table(|table| {
        table.insert(id, parsed);
    });
    let id_value = entry::make_number(id as f64);
    entry::with_runtime(|context| entry::put_member(context, instance, "__urlId", id_value));
    // Built ONCE and kept, which is what makes `u.searchParams === u.searchParams`
    // hold and what `codex_urlsearchparams_live_sort` reads: the object a
    // program holds on to has to be the one a later `u.search = "?z=9"` shows
    // through. It stays live without being rebuilt because it reads this same
    // id out of this same table.
    let params = super::search_params::bound_to(id);
    entry::with_runtime(|context| entry::put_member(context, instance, "searchParams", params));
}

pub(super) fn host_with_port(parsed: &url::Url, host: &str) -> String {
    match parsed.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_owned(),
    }
}

/// `url.origin` — `scheme://host[:port]` for a URL with a host, `"null"`
/// for an opaque-origin scheme (`file:`, `data:`, …), matching the WHATWG
/// origin-serialization rule for the cases this crate can tell apart.
fn origin_of(parsed: &url::Url) -> String {
    match parsed.host_str() {
        Some(host) if parsed.scheme() != "file" => {
            format!("{}://{}", parsed.scheme(), host_with_port(parsed, host))
        }
        _ => "null".to_owned(),
    }
}

extern "C" fn to_string(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    super::string(&read(this, |parsed| parsed.as_str().to_owned()).unwrap_or_default())
}

/// `URL.canParse(input, base?)`.
extern "C" fn can_parse(_e: u64, _this: u64, input: u64, base: u64, _c: u64, _d: u64) -> u64 {
    let Some(text) = super::text(input) else {
        return entry::boolean_value(false);
    };
    let base_text = base_text_of(base);
    entry::boolean_value(parse_with_base(&text, base_text.as_deref()).is_some())
}

/// `URL.parse(input, base?)` — the constructor's algorithm, `null` instead
/// of a thrown error on failure.
extern "C" fn static_parse(_e: u64, _this: u64, input: u64, base: u64, _c: u64, _d: u64) -> u64 {
    let Some(text) = super::text(input) else {
        return entry::null_value();
    };
    let base_text = base_text_of(base);
    let Some(parsed) = parse_with_base(&text, base_text.as_deref()) else {
        return entry::null_value();
    };
    made(parsed)
}

/// A fresh `URL` instance over an already-parsed `url::Url` — what
/// [`fileurl::path_to_file_url`](super::fileurl::path_to_file_url) needs,
/// which has a `url::Url` already and no text to reparse.
///
/// Ambient rather than context-taking, because [`prototype`] is: the accessors
/// have to be in place before an instance whose every property is one is handed
/// to a program.
pub(super) fn made(parsed: url::Url) -> u64 {
    let prototype = prototype();
    let instance = entry::with_runtime(|context| entry::make_instance(context, prototype));
    install_instance(instance, parsed);
    instance
}

/// The stored `url::Url` for an instance's `__urlId`, if it has one.
pub(super) fn stored(instance: u64) -> Option<url::Url> {
    read(instance, Clone::clone)
}

/// The [`TABLE`] key an instance carries, if it is one of ours.
fn id_of(instance: u64) -> Option<u64> {
    entry::number_of(entry::get_indexed(instance, super::string("__urlId"))).map(|id| id as u64)
}

/// Something read off the parsed URL a receiver names.
fn read<T>(this: u64, body: impl FnOnce(&url::Url) -> T) -> Option<T> {
    let id = id_of(this)?;
    with_table(|table| table.get(&id).map(body))
}

/// The parsed URL for an id — what [`super::search_params`] reads through.
pub(super) fn read_at<T>(id: u64, body: impl FnOnce(&url::Url) -> T) -> Option<T> {
    with_table(|table| table.get(&id).map(body))
}

/// A change to the parsed URL an id names.
pub(super) fn write_at(id: u64, body: impl FnOnce(&mut url::Url)) {
    with_table(|table| {
        if let Some(parsed) = table.get_mut(&id) {
            body(parsed);
        }
    });
}

/// One getter: the receiver's parsed URL, serialized one way.
///
/// `""` and not `undefined` for a receiver that is not one of ours — the
/// prototype itself, or an instance whose input did not parse. Every one of
/// these properties is a `USVString` in the specification, so a string is the
/// answer of the right TYPE; and [`prototype`] tells an installed getter from a
/// missing one by exactly that difference.
fn getter(this: u64, body: impl FnOnce(&url::Url) -> String) -> u64 {
    super::string(&read(this, body).unwrap_or_default())
}

/// One setter: the assigned text, applied to the receiver's parsed URL.
///
/// A refusal by the parser leaves the URL as it was and answers `undefined`,
/// which is what WHATWG specifies for nine of the ten setters ("if it fails,
/// return") — see the module doc for the tenth.
fn setter(this: u64, value: u64, body: impl FnOnce(&mut url::Url, &str)) -> u64 {
    let Some(id) = id_of(this) else {
        return entry::undefined_value();
    };
    let text = entry::text_of(value).unwrap_or_default();
    write_at(id, |parsed| body(parsed, &text));
    entry::undefined_value()
}

extern "C" fn get_href(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    getter(this, |parsed| parsed.as_str().to_owned())
}

/// `url.href = x` — a whole reparse, since every other component comes from it.
extern "C" fn set_href(_e: u64, this: u64, value: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    setter(this, value, |parsed, text| {
        if let Ok(reparsed) = url::Url::parse(text) {
            *parsed = reparsed;
        }
    })
}

extern "C" fn get_protocol(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    getter(this, |parsed| format!("{}:", parsed.scheme()))
}

/// `url.protocol = x` — the trailing `:` is optional, as the specification's
/// "basic URL parser in scheme-start state" makes it.
extern "C" fn set_protocol(_e: u64, this: u64, value: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    setter(this, value, |parsed, text| {
        let _ = parsed.set_scheme(text.trim_end_matches(':'));
    })
}

extern "C" fn get_username(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    getter(this, |parsed| parsed.username().to_owned())
}

extern "C" fn set_username(_e: u64, this: u64, value: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    setter(this, value, |parsed, text| {
        let _ = parsed.set_username(text);
    })
}

extern "C" fn get_password(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    getter(this, |parsed| parsed.password().unwrap_or("").to_owned())
}

/// `url.password = ""` REMOVES the password rather than storing an empty one,
/// which is what makes `https://u:@h/` serialize back as `https://u@h/`.
extern "C" fn set_password(_e: u64, this: u64, value: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    setter(this, value, |parsed, text| {
        let _ = parsed.set_password(Some(text).filter(|text| !text.is_empty()));
    })
}

extern "C" fn get_host(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    getter(this, |parsed| {
        parsed.host_str().map(|host| host_with_port(parsed, host)).unwrap_or_default()
    })
}

/// `url.host = x` — host AND port, which is the whole difference from
/// `hostname`. Split here rather than handed to the parser whole because
/// `Url::set_host` treats its argument as a host alone, so `"h:81"` would be
/// refused as an invalid host name instead of setting the port.
extern "C" fn set_host(_e: u64, this: u64, value: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    setter(this, value, |parsed, text| {
        let (host, port) = split_port(text);
        if parsed.set_host(Some(host)).is_err() {
            return;
        }
        if let Some(port) = port {
            let _ = parsed.set_port(port);
        }
    })
}

/// A `host:port` authority, split only when what follows the last colon really
/// is a port. `"[::1]"` has colons and no port, which is why the split is
/// anchored after the closing bracket.
fn split_port(text: &str) -> (&str, Option<Option<u16>>) {
    let after_brackets = text.rfind(']').map_or(0, |at| at + 1);
    let Some(at) = text[after_brackets..].rfind(':').map(|at| at + after_brackets) else {
        return (text, None);
    };
    let (host, port) = (&text[..at], &text[at + 1..]);
    match port.is_empty() {
        true => (host, Some(None)),
        false => match port.parse::<u16>() {
            Ok(number) => (host, Some(Some(number))),
            // Not a port at all — the whole string stays the host, and the
            // parser is what refuses it.
            Err(_) => (text, None),
        },
    }
}

extern "C" fn get_hostname(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    getter(this, |parsed| parsed.host_str().unwrap_or("").to_owned())
}

extern "C" fn set_hostname(_e: u64, this: u64, value: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    setter(this, value, |parsed, text| {
        let _ = parsed.set_host(Some(text));
    })
}

extern "C" fn get_port(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    getter(this, |parsed| parsed.port().map(|port| port.to_string()).unwrap_or_default())
}

/// `url.port = ""` removes the port; anything that is not a number in range is
/// ignored, per the specification.
extern "C" fn set_port(_e: u64, this: u64, value: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    setter(this, value, |parsed, text| {
        if text.is_empty() {
            let _ = parsed.set_port(None);
            return;
        }
        if let Ok(number) = text.parse::<u16>() {
            let _ = parsed.set_port(Some(number));
        }
    })
}

extern "C" fn get_pathname(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    getter(this, |parsed| parsed.path().to_owned())
}

extern "C" fn set_pathname(_e: u64, this: u64, value: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    setter(this, value, |parsed, text| parsed.set_path(text))
}

extern "C" fn get_search(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    getter(this, |parsed| query_text(parsed))
}

/// `url.search` — `"?" + query`, and `""` rather than `"?"` for a URL with no
/// query or an empty one, which is what the serializer specifies.
pub(super) fn query_text(parsed: &url::Url) -> String {
    match parsed.query().filter(|query| !query.is_empty()) {
        Some(query) => format!("?{query}"),
        None => String::new(),
    }
}

extern "C" fn set_search(_e: u64, this: u64, value: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    setter(this, value, |parsed, text| {
        let query = text.strip_prefix('?').unwrap_or(text);
        parsed.set_query(Some(query).filter(|query| !query.is_empty()));
    })
}

extern "C" fn get_hash(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    getter(this, |parsed| match parsed.fragment().filter(|fragment| !fragment.is_empty()) {
        Some(fragment) => format!("#{fragment}"),
        None => String::new(),
    })
}

extern "C" fn set_hash(_e: u64, this: u64, value: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    setter(this, value, |parsed, text| {
        let fragment = text.strip_prefix('#').unwrap_or(text);
        parsed.set_fragment(Some(fragment).filter(|fragment| !fragment.is_empty()));
    })
}

extern "C" fn get_origin(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    getter(this, origin_of)
}
