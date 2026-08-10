//! `URL` — the WHATWG parser (`url` crate) behind an instance carrying a
//! numbered slot into [`TABLE`], the same shape `fs/dir.rs`'s `Cursor` table
//! uses. See the module doc for why there are no property setters.

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

/// Builds the `URL` class and its statics, and returns the constructor.
pub(super) fn install(context: &mut Context) -> u64 {
    let prototype = entry::make_prototype(context, "URL", METHODS);
    let ctor = super::class_ctor(context, construct, prototype);
    let can_parse_fn = entry::make_callable(context, can_parse);
    entry::put_member(context, ctor, "canParse", can_parse_fn);
    let parse_fn = entry::make_callable(context, static_parse);
    entry::put_member(context, ctor, "parse", parse_fn);
    ctor
}

/// `new URL(input, base?)`. Malformed input answers an INERT instance — one
/// carrying no `__urlId` — rather than throwing; see the module doc.
extern "C" fn construct(_e: u64, this: u64, input: u64, base: u64, _c: u64, _d: u64) -> u64 {
    let Some(text) = super::text(input) else {
        return entry::with_runtime(|context| {
            let prototype = entry::make_prototype(context, "URL", METHODS);
            super::self_or_new(context, this, prototype)
        });
    };
    let base_text = base_text_of(base);
    let parsed = parse_with_base(&text, base_text.as_deref());
    entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "URL", METHODS);
        let instance = super::self_or_new(context, this, prototype);
        if let Some(parsed) = parsed {
            install_instance(context, instance, parsed);
        }
        instance
    })
}

/// `base`, as text — either a plain string argument or a `URL` instance,
/// read off its own `href` snapshot property rather than a second parse.
fn base_text_of(base: u64) -> Option<String> {
    let absent = entry::undefined_value();
    if base == absent {
        return None;
    }
    if let Some(href) = super::get_text(base, "href") {
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

/// Records the parsed `url::Url` in [`TABLE`] and writes the snapshot data
/// properties every getter Node specifies for `URL` reads from.
fn install_instance(context: &mut Context, instance: u64, parsed: url::Url) {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    refresh(context, instance, &parsed);
    with_table(|table| {
        table.insert(id, parsed);
    });
    let id_value = entry::make_number(id as f64);
    entry::put_member(context, instance, "__urlId", id_value);
}

/// Writes every snapshot property off a freshly parsed (or re-parsed)
/// `url::Url` — the one place all of them are derived, so `href`/`origin`/…
/// never disagree with what was actually parsed.
fn refresh(context: &mut Context, instance: u64, parsed: &url::Url) {
    let pairs: &[(&str, String)] = &[
        ("href", parsed.as_str().to_owned()),
        ("protocol", format!("{}:", parsed.scheme())),
        ("username", parsed.username().to_owned()),
        ("password", parsed.password().unwrap_or("").to_owned()),
        ("host", parsed.host_str().map(|host| host_with_port(parsed, host)).unwrap_or_default()),
        ("hostname", parsed.host_str().unwrap_or("").to_owned()),
        ("port", parsed.port().map(|port| port.to_string()).unwrap_or_default()),
        ("pathname", parsed.path().to_owned()),
        ("search", parsed.query().map(|query| format!("?{query}")).unwrap_or_default()),
        ("hash", parsed.fragment().map(|fragment| format!("#{fragment}")).unwrap_or_default()),
        ("origin", origin_of(parsed)),
    ];
    for (name, value) in pairs {
        let held = super::string_in(context, value);
        entry::put_member(context, instance, name, held);
    }
    let search_params = super::search_params::from_query(context, parsed.query().unwrap_or(""));
    entry::put_member(context, instance, "searchParams", search_params);
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
    let href = super::get_text(this, "href").unwrap_or_default();
    super::string(&href)
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
    entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "URL", METHODS);
        let instance = entry::make_instance(context, prototype);
        install_instance(context, instance, parsed);
        instance
    })
}

/// A fresh `URL` instance over an already-parsed `url::Url` — what
/// [`fileurl::path_to_file_url`](super::fileurl::path_to_file_url) needs,
/// which has a `url::Url` already and no text to reparse.
pub(super) fn from_parsed(context: &mut Context, parsed: url::Url) -> u64 {
    let prototype = entry::make_prototype(context, "URL", METHODS);
    let instance = entry::make_instance(context, prototype);
    install_instance(context, instance, parsed);
    instance
}

/// The stored `url::Url` for an instance's `__urlId`, if it has one.
pub(super) fn stored(instance: u64) -> Option<url::Url> {
    let id_value = entry::get_indexed(instance, super::string("__urlId"));
    let id = entry::number_of(id_value)? as u64;
    with_table(|table| table.get(&id).cloned())
}
