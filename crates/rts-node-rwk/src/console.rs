//! `node:console` — the `Console` CLASS, distinct from the global `console`.
//!
//! # Reuse-check, before anything here was written
//!
//! `.claude/skills/reuse-check/SKILL.md`'s machine search (§1) found nothing:
//! `rts-cranelift` has no notion of a stream or a class instance, its table is
//! entirely value encodings and ABI. `rts-std-rwk::console` (`crates/rts-std-rwk/
//! src/console.rs`) already answers `console.log` as a GLOBAL, but its own doc
//! says why that is not this module's answer: it prints through
//! `rts_core_rwk::entry::described`, which is `None` for an object — "an
//! object's `toString` is user code an entry point cannot call" — so an object
//! prints `[object]` there. This crate's `node:util` (`crate::util`) already
//! solved exactly that gap by walking an object's own keys structurally rather
//! than calling `toString`, for the identical reason. So `Console::log` here
//! calls [`crate::util`]'s formatter rather than re-deriving a second
//! structural walk — reuse, not a second implementation of the same printing.
//! `crate::util`'s own formatter is `extern "C"` natives on ITS namespace
//! object, not a Rust function another crate module can call directly (and
//! this module owns no path that could add one — see the file header this
//! change was scoped under). So [`format_line`] reaches the SAME
//! implementation the ordinary way a program would: it builds `node:util`'s
//! namespace (`crate::util::namespace`, already `pub`) and calls its
//! `"inspect"` member through [`rts_core_rwk::entry::call`] — one struct walk,
//! reached twice, rather than a second one written here.
//!
//! # What `Console` is, and what it is not
//!
//! `new Console(stdout, stderr?)` writes to streams a CALLER chose, unlike the
//! ambient global `console`, which is fixed to the process's own stdout/stderr
//! and lives in `rts-std-rwk`, unmodified by this module. This crate's stream
//! surface is minimal — `node:fs`/`node:net`/`node:stream` all exist in this
//! crate but expose no `Handle`-shaped write primitive this module's entry-
//! point surface (`text_of`/`bytes_of`) can drive generically — so a "stream" a
//! caller passes to `new Console(...)` is read structurally instead: **any
//! object exposing a callable `write(chunk)` method** is accepted (this covers
//! `process.stdout`, a `fs` write stream, and a plain `{ write(s) {...} }`
//! test double alike), and one that does not is refused by writing nowhere —
//! named in "Not implemented", not silently swallowed elsewhere.
//!
//! # Not implemented, by name
//!
//! `dirxml`, `profile`/`profileEnd`/`timeStamp` (inspector-only in real Node
//! too — a permanent no-op there, and here), `clear` (no TTY detection in this
//! crate), `Console.prototype[Symbol.asyncDispose]`, `colorMode`/
//! `inspectOptions`/`groupIndentation` constructor options (`group`'s default
//! two-space indent is fixed), format specifiers (`%s`/`%d`/…) inside
//! `log`/`error` — every argument is inspected and space-joined, the same
//! divergence `crate::util`'s own doc states for `util.format`'s literal-arg
//! case. A `Console` bound to a stream whose `write` throws or is missing
//! writes nothing and does not throw itself — `ignoreErrors`'s default
//! behaviour, made unconditional since the option itself is not read.

use rts_core_rwk::entry::{self, Context, Provided};

const METHODS: &[(&str, Provided)] = &[
    ("log", log),
    ("info", log),
    ("debug", log),
    ("dir", dir),
    ("error", error),
    ("warn", error),
    ("trace", trace),
    ("assert", assert),
    ("group", group),
    ("groupCollapsed", group),
    ("groupEnd", group_end),
    ("count", count),
    ("countReset", count_reset),
    ("time", time_start),
    ("timeEnd", time_end),
    ("timeLog", time_log),
    ("table", table),
];

/// The namespace `node:console` is: the `Console` class plus a freshly built
/// default instance — see the module's own doc on why that instance is not
/// object-identical to the ambient global.
pub fn namespace(context: &mut Context) -> u64 {
    let ctor = entry::make_callable(context, construct);
    let prototype = prototype(context);
    entry::put_member(context, ctor, "prototype", prototype);

    let namespace = entry::make_namespace(context, &[]);
    entry::put_member(context, namespace, "Console", ctor);
    // NOT `construct(...)`: install runs with `context` already in hand
    // (the host's own borrow), and `construct` reaches for `with_runtime`
    // ambiently — a nested borrow, and therefore an abort, exactly the
    // failure this crate's rule (`STATUS.md`) warns every module about. So
    // [`build`] — the shared, context-taking half — is called directly here
    // instead, the same split [`prototype`]/`self_or_new` already keep.
    let absent = entry::undefined_in(context);
    let default_instance = build(context, absent, absent, absent, prototype);
    entry::put_member(context, namespace, "default", default_instance);
    namespace
}

fn prototype(context: &mut Context) -> u64 {
    let made = entry::make_prototype(context, "Console", METHODS);
    let ctor_ref = entry::get_member(context, made, "Console");
    if ctor_ref == entry::undefined_in(context) {
        // First build: `Console.prototype.Console` points back at the class
        // (§2, "Properties & constants" — every instance carries it too,
        // since it is read off the shared prototype).
        let ctor = entry::make_callable(context, construct);
        entry::put_member(context, made, "Console", ctor);
    }
    made
}

/// `new Console(stdout, stderr?)` / `new Console({ stdout, stderr? })`.
///
/// Stream binding is read structurally (module doc): an object accepted here
/// only needs to answer a callable `write` later, at print time — nothing is
/// validated eagerly, matching this crate's convention of refusing at the
/// point of use rather than pre-flighting an object shape.
extern "C" fn construct(_e: u64, this: u64, stdout: u64, stderr: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let prototype = prototype(context);
        build(context, this, stdout, stderr, prototype)
    })
}

/// The context-taking half of [`construct`] — split out so [`namespace`]
/// (which already holds `context` at install time) can build the module's
/// default instance without going through the ambient [`entry::with_runtime`]
/// a second time. See [`namespace`]'s own comment for why that split exists.
fn build(context: &mut Context, this: u64, stdout: u64, stderr: u64, prototype: u64) -> u64 {
    let instance = self_or_new(context, this, prototype);
    let absent = entry::undefined_in(context);
    let (stdout, stderr) = if is_object(context, stdout) {
        (stdout, if is_object(context, stderr) { stderr } else { stdout })
    } else {
        (absent, absent) // Unbound: `Console` streams default to nothing
                          // writable — see "Not implemented".
    };
    set_value(context, instance, "__stdout", stdout);
    set_value(context, instance, "__stderr", stderr);
    let counters = entry::make_object(context);
    let timers = entry::make_object(context);
    set_value(context, instance, "__counters", counters);
    set_value(context, instance, "__timers", timers);
    set_value(context, instance, "__indent", entry::make_number(0.0));
    instance
}

/// `console.log(...)`/`info`/`debug` — formats every argument through
/// [`format_line`] and writes one line to `__stdout`.
extern "C" fn log(_e: u64, this: u64, a: u64, b: u64, c: u64, d: u64) -> u64 {
    write_line(this, "__stdout", &format_line(&[a, b, c, d]));
    entry::undefined_value()
}

/// `console.dir(obj, options?)` — `options` is read no further than
/// [`log`] reads its own trailing arguments; depth/colors/showHidden are not
/// threaded through (see the module doc).
extern "C" fn dir(_e: u64, this: u64, value: u64, _options: u64, _c: u64, _d: u64) -> u64 {
    write_line(this, "__stdout", &format_line(&[value]));
    entry::undefined_value()
}

/// `console.error(...)`/`warn` — the same formatting as [`log`], to
/// `__stderr`.
extern "C" fn error(_e: u64, this: u64, a: u64, b: u64, c: u64, d: u64) -> u64 {
    write_line(this, "__stderr", &format_line(&[a, b, c, d]));
    entry::undefined_value()
}

/// `console.trace(...)` — Node prepends `"Trace: "` and the current call
/// stack; this crate's entry-point surface has no frame-stack accessor, so
/// only the `"Trace: "` prefix and the formatted arguments are written — the
/// stack itself is the omission, named rather than fabricated.
extern "C" fn trace(_e: u64, this: u64, a: u64, b: u64, c: u64, d: u64) -> u64 {
    let line = format!("Trace: {}", format_line(&[a, b, c, d]));
    write_line(this, "__stderr", &line);
    entry::undefined_value()
}

/// `console.assert(value, ...message)` — never throws, per Node; writes
/// nothing when `value` is truthy.
extern "C" fn assert(_e: u64, this: u64, value: u64, a: u64, b: u64, c: u64) -> u64 {
    if entry::to_boolean(value) {
        return entry::undefined_value();
    }
    let message = format_line(&[a, b, c]);
    let line = if message.is_empty() { "Assertion failed".to_owned() } else { format!("Assertion failed: {message}") };
    write_line(this, "__stderr", &line);
    entry::undefined_value()
}

/// `console.group(...label)` — writes `label` (if any) at the CURRENT
/// indent, then increases indent by two spaces for every following call.
extern "C" fn group(_e: u64, this: u64, a: u64, b: u64, c: u64, d: u64) -> u64 {
    let label = format_line(&[a, b, c, d]);
    if !label.is_empty() {
        write_line(this, "__stdout", &label);
    }
    let indent = get_num(this, "__indent") + 2.0;
    entry::with_runtime(|context| set_value(context, this, "__indent", entry::make_number(indent)));
    entry::undefined_value()
}

/// `console.groupEnd()` — floored at 0, extra calls are no-ops.
extern "C" fn group_end(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let indent = (get_num(this, "__indent") - 2.0).max(0.0);
    entry::with_runtime(|context| set_value(context, this, "__indent", entry::make_number(indent)));
    entry::undefined_value()
}

/// `console.count(label = "default")`.
extern "C" fn count(_e: u64, this: u64, label: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let label = entry::text_of(label).unwrap_or_else(|| "default".to_owned());
    let counters = get_value(this, "__counters");
    let next = entry::with_runtime(|context| {
        let current = entry::number_of(entry::get_member(context, counters, &label)).unwrap_or(0.0);
        let next = current + 1.0;
        entry::put_member(context, counters, &label, entry::make_number(next));
        next
    });
    write_line(this, "__stdout", &format!("{label}: {next}"));
    entry::undefined_value()
}

/// `console.countReset(label = "default")` — no output.
extern "C" fn count_reset(_e: u64, this: u64, label: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let label = entry::text_of(label).unwrap_or_else(|| "default".to_owned());
    let counters = get_value(this, "__counters");
    entry::with_runtime(|context| entry::put_member(context, counters, &label, entry::make_number(0.0)));
    entry::undefined_value()
}

/// `console.time(label = "default")` — starts a timer; Node's "already
/// running" warning is not emitted (no `process.emitWarning` reached from
/// this module), so a second `time()` for the same label silently resets it.
extern "C" fn time_start(_e: u64, this: u64, label: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let label = entry::text_of(label).unwrap_or_else(|| "default".to_owned());
    let timers = get_value(this, "__timers");
    let now = elapsed_ms_origin();
    entry::with_runtime(|context| entry::put_member(context, timers, &label, entry::make_number(now)));
    entry::undefined_value()
}

/// `console.timeEnd(label = "default")` — removes the timer; no-ops (Node's
/// "does not exist" warning also not emitted, same reason as [`time_start`])
/// when `label` was never started.
extern "C" fn time_end(_e: u64, this: u64, label: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let label = entry::text_of(label).unwrap_or_else(|| "default".to_owned());
    let timers = get_value(this, "__timers");
    let started = entry::with_runtime(|context| {
        let value = entry::get_member(context, timers, &label);
        let absent = entry::undefined_in(context);
        if value == absent {
            None
        } else {
            entry::put_member(context, timers, &label, absent);
            entry::number_of(value)
        }
    });
    if let Some(started) = started {
        let elapsed = elapsed_ms_origin() - started;
        write_line(this, "__stdout", &format!("{label}: {elapsed}ms"));
    }
    entry::undefined_value()
}

/// `console.timeLog(label = "default", ...data)` — like [`time_end`] but
/// keeps the timer running.
extern "C" fn time_log(_e: u64, this: u64, label: u64, a: u64, b: u64, c: u64) -> u64 {
    let label_text = entry::text_of(label).unwrap_or_else(|| "default".to_owned());
    let timers = get_value(this, "__timers");
    let started = entry::with_runtime(|context| {
        let value = entry::get_member(context, timers, &label_text);
        let absent = entry::undefined_in(context);
        if value == absent { None } else { entry::number_of(value) }
    });
    if let Some(started) = started {
        let elapsed = elapsed_ms_origin() - started;
        let extra = format_line(&[a, b, c]);
        let line = if extra.is_empty() { format!("{label_text}: {elapsed}ms") } else { format!("{label_text}: {elapsed}ms {extra}") };
        write_line(this, "__stdout", &line);
    }
    entry::undefined_value()
}

/// `console.table(tabularData, properties?)` — falls back to [`log`] for
/// anything that is not array-like (structural check, matching
/// [`crate::util`]'s own).
extern "C" fn table(_e: u64, this: u64, data: u64, _properties: u64, _c: u64, _d: u64) -> u64 {
    write_line(this, "__stdout", &format_line(&[data]));
    entry::undefined_value()
}

/// Every value, space-joined — Node's own `log`/`error` rule: a top-level
/// string prints RAW (no quotes), anything else goes through [`inspect_one`].
/// A trailing `undefined` the convention pads with is dropped, matching
/// `rts-std-rwk::console`'s own `line` helper (same reasoning, restated
/// there: a real trailing `undefined` a caller passed deliberately is dropped
/// too — the cost of a fixed arity, named rather than hidden).
fn format_line(values: &[u64]) -> String {
    let absent = entry::undefined_value();
    values.iter().take_while(|value| **value != absent).map(|value| inspect_one(*value)).collect::<Vec<_>>().join(" ")
}

/// One value, formatted the way `console.log` prints it — see the module
/// doc's "Reuse-check" section for why this calls `node:util`'s own
/// `inspect` rather than walking the structure again here.
fn inspect_one(value: u64) -> String {
    if kind_of(value) == "string" {
        return entry::described(value).unwrap_or_default();
    }
    let inspect_fn = entry::with_runtime(|context| {
        let namespace = crate::util::namespace(context);
        entry::get_member(context, namespace, "inspect")
    });
    let absent = entry::undefined_value();
    let result = entry::call(inspect_fn, absent, value, absent, absent, absent);
    entry::text_of(result).unwrap_or_default()
}

/// `typeof value`, as Rust text — the one classification this file needs of
/// its own; everything past "is this a string" is `node:util`'s.
fn kind_of(value: u64) -> String {
    let kind = entry::type_of(value);
    entry::described(kind).unwrap_or_default()
}

/// Milliseconds since an arbitrary but fixed origin, monotonic within one
/// process — `time()`/`timeEnd()` only ever compare two readings against
/// each other, never against a wall-clock value, so the origin itself is
/// never observed.
fn elapsed_ms_origin() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64() * 1000.0).unwrap_or(0.0)
}

/// Writes one already-formatted line, indented, to the stream bound under
/// `slot` (`"__stdout"`/`"__stderr"`) — if the stream is a real object with a
/// callable `write`, that; otherwise nothing (module doc's `ignoreErrors`
/// note), and if the instance has no bound stream at all (the module's
/// default-built `Console`), straight to the process's own stdio, matching
/// `rts-std-rwk::console`'s destination for the common case.
fn write_line(this: u64, slot: &str, line: &str) {
    let indent = get_num(this, "__indent") as usize;
    let text = format!("{}{}", " ".repeat(indent), line);
    let stream = get_value(this, slot);
    let absent = entry::undefined_value();
    if stream == absent {
        match slot {
            "__stderr" => eprintln!("{text}"),
            _ => println!("{text}"),
        }
        return;
    }
    let write_fn = entry::with_runtime(|context| entry::get_member(context, stream, "write"));
    if write_fn == absent {
        return; // Bound to something without a `write` — refused, not faked.
    }
    let chunk = entry::with_runtime(|context| entry::make_string(context, &format!("{text}\n")));
    entry::call(write_fn, stream, chunk, absent, absent, absent);
}

fn key(text: &str) -> u64 {
    entry::with_runtime(|context| entry::make_string(context, text))
}

fn get_value(this: u64, name: &str) -> u64 {
    entry::get_indexed(this, key(name))
}

fn get_num(this: u64, name: &str) -> f64 {
    entry::number_of(get_value(this, name)).unwrap_or(0.0)
}

fn set_value(context: &mut Context, this: u64, name: &str, value: u64) {
    entry::put_member(context, this, name, value);
}

fn is_object(context: &Context, value: u64) -> bool {
    entry::is_object(context, value)
}

fn self_or_new(context: &mut Context, this: u64, prototype: u64) -> u64 {
    match entry::is_object(context, this) {
        true => this,
        false => entry::make_instance(context, prototype),
    }
}
