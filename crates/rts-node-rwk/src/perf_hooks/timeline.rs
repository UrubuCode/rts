//! The User Timing timeline — `mark`, `measure`, `clearX`, `getEntries*`, and
//! the entry classes a program reaches with `instanceof`.
//!
//! # Where the entries live, and why
//!
//! A `thread_local!` `Vec` of plain Rust records, plus — for `detail` alone —
//! the raw `u64` the program passed. See the crate-module doc for why
//! thread-local rather than the process-global `Mutex` this file used to have:
//! a `detail` is a value belonging to the region of the thread that made it,
//! and the previous global would have handed one thread's cell to another.
//!
//! # Why the borrow is taken once per call
//!
//! Every argument here is read with a *predicate* rather than a coercion where
//! the question is "what is this" (`string_in`, `is_object`), and every one of
//! those takes a `&Context`. So each native takes one
//! [`rts_core_rwk::entry::with_runtime`] and does all of its reading,
//! resolving and allocating inside it. Nothing ambient is called from inside —
//! that is the abort this crate's entry surface documents, and the shape below
//! is what makes it checkable by reading one function at a time.

use rts_core_rwk::entry::{self, Context, Provided};
use std::cell::RefCell;

/// One buffered timeline entry.
pub(super) struct Record {
    pub(super) name: String,
    /// `"mark"`, `"measure"` or `"function"` — a `&'static str` because those
    /// are the only three this engine can produce, and a `String` would invite
    /// a fourth to be minted at a call site instead of declared here.
    pub(super) kind: &'static str,
    pub(super) start: f64,
    pub(super) duration: f64,
    /// `None` is `null` on the way out, which is what Node answers for an
    /// entry created without one.
    pub(super) detail: Option<u64>,
}

thread_local! {
    static ENTRIES: RefCell<Vec<Record>> = const { RefCell::new(Vec::new()) };
}

/// Reads the timeline. Never call anything that re-enters this from the body —
/// the `RefCell` is this module's own and a nested borrow panics here just as
/// the context's does.
pub(super) fn with_entries<T>(body: impl FnOnce(&mut Vec<Record>) -> T) -> T {
    ENTRIES.with(|held| body(&mut held.borrow_mut()))
}

/// How many entries the timeline holds — what an observer's cursor counts
/// against.
pub(super) fn count() -> usize {
    with_entries(|entries| entries.len())
}

/// The entry classes, so `e instanceof PerformanceMark` answers what a program
/// expects.
///
/// The constructors do nothing and answer `undefined` where Node throws an
/// illegal-constructor error (named in the crate-module doc); they exist only
/// to carry the `prototype` that makes `instanceof` work, which is the
/// `async_hooks::attach` shape.
const CLASSES: &[&str] = &[
    "PerformanceEntry",
    "PerformanceMark",
    "PerformanceMeasure",
    "PerformanceNodeEntry",
];

pub(super) fn install_classes(context: &mut Context, namespace: u64) {
    for name in CLASSES {
        let prototype = entry::make_prototype(context, name, &[]);
        let constructor = entry::make_callable(context, illegal);
        entry::put_member(context, constructor, "prototype", prototype);
        entry::put_member(context, namespace, name, constructor);
    }
}

extern "C" fn illegal(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::undefined_value()
}

/// The prototype an entry of this kind inherits from.
fn prototype_for(context: &mut Context, kind: &str) -> u64 {
    let name = match kind {
        "mark" => "PerformanceMark",
        "measure" => "PerformanceMeasure",
        // `'function'` is Node's own `PerformanceNodeEntry`, which is where its
        // `detail` (the timed call's arguments) is documented to live.
        _ => "PerformanceNodeEntry",
    };
    entry::make_prototype(context, name, &[])
}

/// A `{name, entryType, startTime, duration, detail}` object over one record.
pub(super) fn build_entry(context: &mut Context, record: &Record) -> u64 {
    let prototype = prototype_for(context, record.kind);
    let object = entry::make_instance(context, prototype);
    let name = entry::make_string(context, &record.name);
    entry::put_member(context, object, "name", name);
    let kind = entry::make_string(context, record.kind);
    entry::put_member(context, object, "entryType", kind);
    let start = entry::make_number(record.start);
    entry::put_member(context, object, "startTime", start);
    let duration = entry::make_number(record.duration);
    entry::put_member(context, object, "duration", duration);
    let detail = record.detail.unwrap_or_else(|| entry::null_in(context));
    entry::put_member(context, object, "detail", detail);
    object
}

/// An argument that really is a string, as text.
///
/// `string_in` and not `text_in`: the callers below ask "was a mark name given"
/// and "is this bound a name or a timestamp", which are questions about what a
/// value IS. `text_in` answers `"42"` for the number `42` and would turn a
/// numeric `measure` bound into a lookup for a mark named `"42"`.
fn named(context: &Context, value: u64) -> Option<String> {
    entry::string_in(context, value)
}

/// The name argument every entry-producing call takes.
///
/// Coerced with `text_in` on purpose, and only here: Node's `mark`/`measure`
/// run `String(name)` over whatever they are given, so `mark(1)` really does
/// produce a mark named `"1"`. `undefined` is the one value that is not a
/// name — Node throws for it, this answers `None` and the caller answers
/// `undefined`.
fn entry_name(context: &Context, value: u64) -> Option<String> {
    match value == entry::undefined_in(context) {
        true => None,
        false => entry::text_in(context, value),
    }
}

/// A `measure` bound: a number is a timestamp, a string is the name of an
/// earlier entry whose `startTime` is used, anything else is absent.
///
/// A name that matches nothing answers `None` where Node throws — the stated
/// divergence in the crate-module doc, and it degrades to "measure from the
/// origin", which is visible, rather than to a wrong duration.
fn resolve_time(context: &Context, value: u64) -> Option<f64> {
    if value == entry::undefined_in(context) {
        return None;
    }
    if let Some(name) = named(context, value) {
        return with_entries(|entries| {
            entries
                .iter()
                .rev()
                .find(|record| record.name == name)
                .map(|record| record.start)
        });
    }
    entry::number_of(value)
}

/// `performance.mark(name[, options])` — `options.startTime` and
/// `options.detail` are both honoured.
pub(super) extern "C" fn mark(
    _e: u64,
    _this: u64,
    name: u64,
    options: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    let default_start = super::now_ms();
    entry::with_runtime(|context| {
        let Some(name) = entry_name(context, name) else {
            return entry::undefined_in(context);
        };
        let absent = entry::undefined_in(context);
        let start = entry::number_of(entry::get_member(context, options, "startTime"))
            .unwrap_or(default_start);
        let detail = entry::get_member(context, options, "detail");
        let record = Record {
            name,
            kind: "mark",
            start,
            duration: 0.0,
            detail: (detail != absent).then_some(detail),
        };
        let built = build_entry(context, &record);
        with_entries(|entries| entries.push(record));
        built
    })
}

/// `performance.measure(name[, startMarkOrOptions[, endMark]])` — both
/// documented forms.
pub(super) extern "C" fn measure(
    _e: u64,
    _this: u64,
    name: u64,
    start_or_options: u64,
    end_mark: u64,
    _a3: u64,
) -> u64 {
    let default_end = super::now_ms();
    entry::with_runtime(|context| {
        let Some(name) = entry_name(context, name) else {
            return entry::undefined_in(context);
        };
        let absent = entry::undefined_in(context);
        // The options form is recognised LAST, by elimination. A string is a
        // cell in the region and therefore satisfies `is_object`, so asking
        // "is it an object" first would read `measure('m', 'a', 'b')`'s start
        // mark as an options bag with no `start` in it.
        let options = named(context, start_or_options).is_none()
            && entry::number_of(start_or_options).is_none()
            && start_or_options != absent
            && entry::is_object(context, start_or_options);
        let (start, end, detail) = match options {
            true => {
                let bag = start_or_options;
                let given_start = entry::get_member(context, bag, "start");
                let start = resolve_time(context, given_start);
                let given_end = entry::get_member(context, bag, "end");
                let end = resolve_time(context, given_end);
                let span = entry::number_of(entry::get_member(context, bag, "duration"));
                let detail = entry::get_member(context, bag, "detail");
                // Node derives the missing side from `duration` when exactly
                // one bound is given; with neither the span is measured from
                // the origin to now, which is what "measure with nothing
                // given" means rather than a value invented for the gap.
                let (start, end) = match (start, end, span) {
                    (Some(s), None, Some(d)) => (s, s + d),
                    (None, Some(e), Some(d)) => (e - d, e),
                    (s, e, _) => (s.unwrap_or(0.0), e.unwrap_or(default_end)),
                };
                (start, end, (detail != absent).then_some(detail))
            }
            false => (
                resolve_time(context, start_or_options).unwrap_or(0.0),
                resolve_time(context, end_mark).unwrap_or(default_end),
                None,
            ),
        };
        let record = Record {
            name,
            kind: "measure",
            start,
            duration: (end - start).max(0.0),
            detail,
        };
        let built = build_entry(context, &record);
        with_entries(|entries| entries.push(record));
        built
    })
}

/// Appends an entry produced by something other than a program's own
/// `mark`/`measure` — today only [`super::timerify`]'s `'function'` entries.
pub(super) fn append(record: Record) {
    with_entries(|entries| entries.push(record));
}

pub(super) extern "C" fn clear_marks(
    _e: u64,
    _this: u64,
    name: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    clear("mark", name)
}

pub(super) extern "C" fn clear_measures(
    _e: u64,
    _this: u64,
    name: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    clear("measure", name)
}

/// `performance.clearResourceTimings([name])`.
///
/// Complete rather than a stub: the timeline holds no `'resource'` entry
/// because nothing in this crate produces one, and "remove every `'resource'`
/// entry" over a timeline with none is the correct and total answer. What is
/// missing is the producer, named in the crate-module doc.
pub(super) extern "C" fn clear_resource_timings(
    _e: u64,
    _this: u64,
    name: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    clear("resource", name)
}

fn clear(kind: &str, name: u64) -> u64 {
    let target = entry::with_runtime(|context| entry_name(context, name));
    with_entries(|entries| {
        entries.retain(|record| {
            record.kind != kind || target.as_deref().is_some_and(|want| record.name != want)
        })
    });
    entry::undefined_value()
}

pub(super) extern "C" fn get_entries(
    _e: u64,
    _this: u64,
    _a0: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    entry::with_runtime(|context| entries_array(context, |_| true))
}

pub(super) extern "C" fn get_entries_by_name(
    _e: u64,
    _this: u64,
    name: u64,
    kind: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    entry::with_runtime(|context| {
        let Some(name) = entry_name(context, name) else {
            return entry::make_array_in(context, Vec::new());
        };
        let kind = entry_name(context, kind);
        entries_array(context, |record| {
            record.name == name && kind.as_deref().is_none_or(|want| record.kind == want)
        })
    })
}

pub(super) extern "C" fn get_entries_by_type(
    _e: u64,
    _this: u64,
    kind: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    entry::with_runtime(|context| {
        let Some(kind) = entry_name(context, kind) else {
            return entry::make_array_in(context, Vec::new());
        };
        entries_array(context, |record| record.kind == kind)
    })
}

/// Every buffered entry matching `keep`, oldest first — the order `append`
/// already maintains, which is the chronological order the reference doc
/// requires without a sort.
///
/// # The borrow shape, which is the abort this file is written against
///
/// Takes `&mut Context` rather than reaching for one, so it can only be called
/// from inside a borrow that already exists. The alternative — an ambient
/// helper each caller remembers to call outside its own `with_runtime` — is
/// exactly the rule that gets forgotten once.
fn entries_array(context: &mut Context, keep: impl Fn(&Record) -> bool) -> u64 {
    // Copied out before building, because `build_entry` allocates and a
    // `RefCell` borrow held across an allocation is a borrow held longer than
    // it needs to be for no reason.
    let selected: Vec<usize> = with_entries(|entries| {
        entries
            .iter()
            .enumerate()
            .filter(|(_, record)| keep(record))
            .map(|(index, _)| index)
            .collect()
    });
    build_indices(context, &selected)
}

/// An array of entry objects for these timeline positions.
///
/// Shared with [`super::observer`], whose notification list is the same
/// operation over a different selection — two builders would be two answers to
/// what a `PerformanceEntry` looks like.
pub(super) fn build_indices(context: &mut Context, indices: &[usize]) -> u64 {
    let mut built = Vec::with_capacity(indices.len());
    for index in indices {
        let record = with_entries(|entries| {
            entries.get(*index).map(|record| Record {
                name: record.name.clone(),
                kind: record.kind,
                start: record.start,
                duration: record.duration,
                detail: record.detail,
            })
        });
        if let Some(record) = record {
            built.push(build_entry(context, &record));
        }
    }
    entry::make_array_in(context, built)
}

/// The three readers an observer's entry list carries, which are the same
/// three the `performance` object carries and must answer identically.
pub(super) const LIST_METHODS: &[(&str, Provided)] = &[
    ("getEntries", list_entries),
    ("getEntriesByName", list_entries_by_name),
    ("getEntriesByType", list_entries_by_type),
];

/// The property an entry list carries its already-built objects under, and the
/// one carrying how many there are.
///
/// The count is stored rather than read back off the array because **nothing
/// on this host surface reads an array element or a `length` from inside a
/// borrow** — `get_indexed` is ambient, and `entry::modules` exports no
/// context-taking indexed read. Reported as a missing capability rather than
/// worked around silently a second time.
pub(super) const LIST_HELD: &str = "__entries__";
pub(super) const LIST_COUNT: &str = "__count__";

extern "C" fn list_entries(
    _e: u64,
    this: u64,
    _a0: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    entry::with_runtime(|context| entry::get_member(context, this, LIST_HELD))
}

/// The filtered forms read the already-built objects rather than the records,
/// because a notification list is a snapshot: an entry cleared from the
/// timeline after delivery must still be in the list the callback was handed.
///
/// # Why the comparison is on bits and not on text
///
/// `make_string` interns, and its own documentation states the consequence:
/// two occurrences of one string ARE the same cell. So re-interning the wanted
/// name and comparing the raw values is an exact string-identity test with no
/// coercion anywhere in it — which is the defect class this crate names, and
/// the reason `text_of` appears nowhere below.
fn filtered(this: u64, name: Option<u64>, kind: Option<u64>) -> u64 {
    // One borrow: read the list, its count, and intern the four names. Every
    // read of an ELEMENT below is ambient and therefore outside it.
    let (list, count, name_key, kind_key, want_name, want_kind) =
        entry::with_runtime(|context| {
            let list = entry::get_member(context, this, LIST_HELD);
            let count = entry::number_of(entry::get_member(context, this, LIST_COUNT))
                .unwrap_or(0.0)
                .max(0.0) as usize;
            let name_key = entry::make_string(context, "name");
            let kind_key = entry::make_string(context, "entryType");
            let intern = |context: &mut Context, given: Option<u64>| {
                given
                    .and_then(|value| entry::string_in(context, value))
                    .map(|text| entry::make_string(context, &text))
            };
            let want_name = intern(context, name);
            let want_kind = intern(context, kind);
            (list, count, name_key, kind_key, want_name, want_kind)
        });
    let mut kept = Vec::new();
    for index in 0..count {
        let item = entry::get_indexed(list, entry::make_number(index as f64));
        let matches = want_name.is_none_or(|want| entry::get_indexed(item, name_key) == want)
            && want_kind.is_none_or(|want| entry::get_indexed(item, kind_key) == want);
        if matches {
            kept.push(item);
        }
    }
    entry::make_array(kept)
}

extern "C" fn list_entries_by_name(
    _e: u64,
    this: u64,
    name: u64,
    kind: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    filtered(this, Some(name), Some(kind))
}

extern "C" fn list_entries_by_type(
    _e: u64,
    this: u64,
    kind: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    filtered(this, None, Some(kind))
}
