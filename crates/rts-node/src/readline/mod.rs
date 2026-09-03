//! `node:readline` — line-splitting over a `Readable`, and cursor-control
//! utilities over a `Writable`. `node:readline/promises`'s `Interface` and
//! `createInterface` are [`promises`], not here — see that module's own doc
//! for what of `readline/promises` this covers and what it still does not.
//!
//! # Reuse-check
//!
//! `rts-cranelift` owns nothing terminal- or stream-shaped (checked per
//! `reuse-check`). `crates/rts-node/src/tty.rs` already refused raw
//! mode, cursor writes on a raw fd, and window-size queries for want of a
//! terminal dependency this crate does not accept — this module does not
//! contradict that. What is different here: `clearLine`/`cursorTo`/etc. in
//! THIS module write to a caller-supplied `Writable` **object** (an ANSI
//! escape sequence handed to that object's own `.write()` method), not to a
//! raw OS file descriptor — the write path `tty.rs` had no way to open. That
//! is why they are implemented here and refused there.
//!
//! `Interface` extends `EventEmitter` by chaining prototypes onto
//! `crate::events`'s already-registered one — the exact pairing
//! `make_prototype`/`set_prototype_in` documents, and the same shape
//! `tty.rs`'s `ReadStream`/`WriteStream` already use for one shared
//! prototype per class. No second emitter is written here. [`promises`]'s own
//! `Interface` prototype chains onto THIS module's the same way, for the same
//! reason: everything but `.question()` is identical between the two, and a
//! second `close`/`pause`/`resume`/… would be a second answer to what closing
//! one does.
//!
//! # The borrow this module has to get right
//!
//! A native function passed to `input.on('data', …)` is later invoked BY
//! `EventEmitter::emit`, which — per that module's own doc — calls it only
//! after collecting every listener and dropping its borrow. So the listener
//! here ([`on_data`]) starts with no borrow held. It must still never call
//! `entry::call` (emitting `'line'`, in turn) from inside its own
//! `with_runtime` — every helper below collects what it needs under one
//! borrow, drops it, and only then calls `entry::call`.
//!
//! # Why a listener can find "its" `Interface`
//!
//! A native callable given to `make_callable` has no closure slot — one
//! function pointer cannot carry which `Interface` subscribed. So the
//! `Interface` is stashed as a property on `input` itself, `__rts_readline__`,
//! at subscription time, and [`on_data`]/[`on_end`] read it back off `this`
//! (which `emit` binds to the emitter — `input` — not to the `Interface`).
//! State beside the object it is reachable from, the same shape `rts-std`'s
//! `expect()` uses for its received value. [`promises::question`] does carry a
//! closure slot (`rts_core::entry::closure_new`, minted with the promise as
//! its environment) — the two needs are different: this one has to be found
//! FROM an object it was not given at registration time, that one is given
//! directly to `once('line', …)` and never needs finding again.
//!
//! # Not implemented, by name
//!
//! - **`emitKeypressEvents`**, and every terminal-mode (`terminal: true`)
//!   line-editing keybinding (history navigation, kill/yank, cursor-by-word)
//!   — all need raw mode, which `tty.rs` already refuses.
//! - **History** (`options.history`/`historySize`/`removeHistoryDuplicates`,
//!   the `'history'` event) — only meaningful in terminal mode, above.
//! - **`completer`** (Tab completion) — only meaningful in terminal mode.
//! - **`crlfDelay` timing.** A `\r` and its paired `\n` split across two
//!   `'data'` chunks are always coalesced into one line break here,
//!   regardless of `options.crlfDelay` — the option is read and ignored.
//!   Getting the documented timing-window behavior right needs a timer this
//!   listener does not hold.
//! - **`rl.write(data, key)`** — the `key` form needs raw-mode key injection;
//!   only the plain string form is implemented.
//! - **`options.signal` / `QuestionOptions.signal`** — `AbortSignal`
//!   cancellation is not wired to `question()` or to `createInterface()`,
//!   in either the callback or the promise form.
//! - **`Symbol.dispose`**, `getCursorPos()` (real column/row math),
//!   `rl.pause()`/`rl.resume()` actually pausing byte delivery (the flag is
//!   recorded and the event fires; `input` keeps delivering `'data'`).
//! - **`ERR_INVALID_ARG_TYPE`/`ERR_INVALID_CURSOR_POS`** and every other
//!   Node error code — a malformed argument is read as absent/default here,
//!   never thrown; entry points in this crate have no protected region to
//!   throw into (the same limit `crate::assert` documents).

mod promises;

use rts_core::entry::{self, Context, Provided};

/// The instance methods every `Interface` has, beyond what it inherits from
/// `EventEmitter` through the prototype chain [`namespace`] sets up.
pub(super) const INTERFACE_METHODS: &[(&str, Provided)] = &[
    ("close", close),
    ("pause", pause),
    ("resume", resume),
    ("setPrompt", set_prompt),
    ("getPrompt", get_prompt),
    ("prompt", prompt),
    ("write", write_data),
    ("question", question),
];

/// The namespace `node:readline` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("createInterface", create_interface),
        ("clearLine", clear_line),
        ("clearScreenDown", clear_screen_down),
        ("cursorTo", cursor_to),
        ("moveCursor", move_cursor),
    ];
    let namespace = entry::make_namespace(context, members);
    // The memoized half of `node:events`, not the namespace: this wants only the
    // prototype, and `events::namespace` builds a fresh namespace object and
    // three natives every call. See `events::emitter_prototype`.
    let emitter_prototype = crate::events::emitter_prototype(context);
    let interface_prototype = entry::make_prototype(context, "Interface", INTERFACE_METHODS);
    entry::set_prototype_in(context, interface_prototype, emitter_prototype);
    // `readline.promises` — Node has carried this as a member of the
    // callback-form namespace since v17, alongside the separate
    // `node:readline/promises` specifier `lib.rs` registers by reading it back
    // out (the same shape `node:dns`'s `.promises` member is read out under —
    // see that specifier's own comment in `lib.rs`). Built here rather than by
    // a second call to `promises::namespace`, so `require('readline').promises
    // === require('readline/promises')` holds instead of naming two objects.
    let interactive = promises::namespace(context);
    entry::put_member(context, namespace, "promises", interactive);
    namespace
}

/// `readline.createInterface(options)`.
extern "C" fn create_interface(_e: u64, _this: u64, options: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let prototype = entry::with_runtime(|context| entry::make_prototype(context, "Interface", INTERFACE_METHODS));
    build_interface(options, prototype)
}

/// Builds an `Interface` instance over `prototype` and wires `input`'s
/// `'data'`/`'end'` listeners — the whole of what `createInterface` does,
/// shared with [`promises::create_interface`] because everything about
/// reading lines off `input` is identical between the two; only which
/// prototype `question` comes from differs, and the caller supplies that.
pub(super) fn build_interface(options: u64, prototype: u64) -> u64 {
    let (rl, input, on_method) = entry::with_runtime(|context| {
        let absent = entry::undefined_in(context);
        let input = entry::get_member(context, options, "input");
        let output = entry::get_member(context, options, "output");
        let prompt_option = entry::get_member(context, options, "prompt");
        let rl = entry::make_instance(context, prototype);
        entry::put_member(context, rl, "input", input);
        entry::put_member(context, rl, "output", output);
        let prompt_value = if prompt_option == absent {
            entry::make_string(context, "> ")
        } else {
            prompt_option
        };
        entry::put_member(context, rl, "__prompt__", prompt_value);
        let empty = entry::make_string(context, "");
        entry::put_member(context, rl, "line", empty);
        entry::put_member(context, rl, "cursor", entry::make_number(0.0));
        entry::put_member(context, rl, "__buf__", empty);
        // Both properties: `events.rs`'s `eventNames()`/no-argument
        // `removeAllListeners()` read `__eventNames__` specifically, and a
        // missing one reads as an always-empty list forever, silently.
        let events = entry::make_object(context);
        entry::put_member(context, rl, "__events__", events);
        let event_names = entry::make_array_in(context, Vec::new());
        entry::put_member(context, rl, "__eventNames__", event_names);
        entry::put_member(context, rl, "__closed__", entry::boolean_value(false));
        let on_method = if input == absent {
            absent
        } else {
            entry::put_member(context, input, "__rts_readline__", rl);
            entry::get_member(context, input, "on")
        };
        (rl, input, on_method)
    });
    let absent = entry::undefined_value();
    if on_method != absent {
        let (data_name, end_name, data_listener, end_listener) = entry::with_runtime(|context| {
            (
                entry::make_string(context, "data"),
                entry::make_string(context, "end"),
                entry::make_callable(context, on_data),
                entry::make_callable(context, on_end),
            )
        });
        entry::call(on_method, input, data_name, data_listener, absent, absent);
        entry::call(on_method, input, end_name, end_listener, absent, absent);
    }
    rl
}

/// Splits `text` on `\n` (a paired `\r` stripped, so `\r\n` and `\n` both
/// count as one break) into complete lines, plus whatever trailed the last
/// break — the remainder a caller still accumulating a stream keeps for next
/// time.
///
/// `pub(crate)` rather than local to [`on_data`]: `fs/handle.rs`'s
/// `readLines()` wants the SAME split (Node's own `readline` module is what
/// defines what "a line" means here), over a whole file it already has in one
/// string rather than one `'data'` chunk at a time — extracted here instead
/// of writing a second line-splitter, per this module's own reuse-check.
pub(crate) fn split_lines(text: &str) -> (Vec<String>, String) {
    let mut lines = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find('\n') {
        let mut one = &rest[..at];
        if let Some(stripped) = one.strip_suffix('\r') {
            one = stripped;
        }
        lines.push(one.to_owned());
        rest = &rest[at + 1..];
    }
    (lines, rest.to_owned())
}

/// The `'data'` listener subscribed onto `input` by [`build_interface`].
/// `this` is `input` (see the module doc) — the `Interface` is read back off
/// its `__rts_readline__` property.
extern "C" fn on_data(_e: u64, this: u64, chunk: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let (rl, buffered) = entry::with_runtime(|context| {
        let absent = entry::undefined_in(context);
        let rl = entry::get_member(context, this, "__rts_readline__");
        if rl == absent {
            return (absent, String::new());
        }
        let held = entry::get_member(context, rl, "__buf__");
        (rl, entry::text_in(context, held).unwrap_or_default())
    });
    if rl == absent {
        return absent;
    }
    let Some(chunk_text) = entry::text_of(chunk) else {
        return absent;
    };
    let mut combined = buffered;
    combined.push_str(&chunk_text);
    let (lines, remainder) = split_lines(&combined);
    let (line_name, emit_method) = entry::with_runtime(|context| {
        let remainder_value = entry::make_string(context, &remainder);
        entry::put_member(context, rl, "__buf__", remainder_value);
        (entry::make_string(context, "line"), entry::get_member(context, rl, "emit"))
    });
    for text in lines {
        let value = entry::with_runtime(|context| entry::make_string(context, &text));
        entry::call(emit_method, rl, line_name, value, absent, absent);
    }
    absent
}

/// The `'end'` listener subscribed onto `input` — closes the `Interface`.
extern "C" fn on_end(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let rl = entry::with_runtime(|context| entry::get_member(context, this, "__rts_readline__"));
    if rl != absent {
        close(0, rl, absent, absent, absent, absent);
    }
    absent
}

/// `rl.close()` — idempotent: emits `'close'` only the first time.
extern "C" fn close(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let (already, emit_method, name) = entry::with_runtime(|context| {
        let flag = entry::get_member(context, this, "__closed__");
        // Inside the borrow, so the context-taking form: the ambient one aborted
        // every `rl.close()`.
        let already = entry::to_boolean_in(context, flag);
        entry::put_member(context, this, "__closed__", entry::boolean_value(true));
        (already, entry::get_member(context, this, "emit"), entry::make_string(context, "close"))
    });
    if !already {
        let absent = entry::undefined_value();
        entry::call(emit_method, this, name, absent, absent, absent);
    }
    entry::undefined_value()
}

/// `rl.pause()` — records the flag and emits `'pause'`; does not stop
/// `input` from continuing to deliver `'data'` (see the module doc).
extern "C" fn pause(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    emit_named(this, "pause")
}

/// `rl.resume()`.
extern "C" fn resume(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    emit_named(this, "resume")
}

/// `rl.setPrompt(prompt)`.
extern "C" fn set_prompt(_e: u64, this: u64, value: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| entry::put_member(context, this, "__prompt__", value));
    entry::undefined_value()
}

/// `rl.getPrompt()`.
extern "C" fn get_prompt(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| entry::get_member(context, this, "__prompt__"))
}

/// `rl.prompt(preserveCursor?)` — writes the configured prompt to `output`.
extern "C" fn prompt(_e: u64, this: u64, _preserve: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let (output, prompt_value) = entry::with_runtime(|context| {
        (entry::get_member(context, this, "output"), entry::get_member(context, this, "__prompt__"))
    });
    write_to(output, prompt_value);
    entry::undefined_value()
}

/// `rl.write(data)` — string form only, see the module doc.
extern "C" fn write_data(_e: u64, this: u64, data: u64, _key: u64, _c: u64, _d: u64) -> u64 {
    let output = entry::with_runtime(|context| entry::get_member(context, this, "output"));
    write_to(output, data);
    entry::undefined_value()
}

/// `rl.question(query, callback)` — the callback form only; [`promises`]
/// carries the promise form. Implemented as a one-shot `'line'` listener,
/// exactly `rl.once('line', callback)` — no Rust-side state of its own.
extern "C" fn question(_e: u64, this: u64, query: u64, callback: u64, _c: u64, _d: u64) -> u64 {
    let (output, once_method, line_name) = entry::with_runtime(|context| {
        (
            entry::get_member(context, this, "output"),
            entry::get_member(context, this, "once"),
            entry::make_string(context, "line"),
        )
    });
    write_to(output, query);
    let absent = entry::undefined_value();
    entry::call(once_method, this, line_name, callback, absent, absent);
    absent
}

/// Calls `output.write(value)` if `output` is present.
pub(super) fn write_to(output: u64, value: u64) {
    let absent = entry::undefined_value();
    if output == absent {
        return;
    }
    let write_method = entry::with_runtime(|context| entry::get_member(context, output, "write"));
    if write_method != absent {
        entry::call(write_method, output, value, absent, absent, absent);
    }
}

/// Emits a no-argument event by name against `this`'s inherited `emit`.
fn emit_named(this: u64, name: &str) -> u64 {
    let (emit_method, name_value) =
        entry::with_runtime(|context| (entry::get_member(context, this, "emit"), entry::make_string(context, name)));
    let absent = entry::undefined_value();
    entry::call(emit_method, this, name_value, absent, absent, absent);
    absent
}

/// `readline.clearLine(stream, dir[, callback])`.
extern "C" fn clear_line(_e: u64, _this: u64, stream: u64, dir: u64, _cb: u64, _d: u64) -> u64 {
    let direction = entry::number_of(dir).unwrap_or(0.0);
    let sequence = if direction < 0.0 {
        "\x1b[1K"
    } else if direction > 0.0 {
        "\x1b[0K"
    } else {
        "\x1b[2K"
    };
    write_ansi(stream, sequence);
    entry::boolean_value(true)
}

/// `readline.clearScreenDown(stream[, callback])`.
extern "C" fn clear_screen_down(_e: u64, _this: u64, stream: u64, _cb: u64, _c: u64, _d: u64) -> u64 {
    write_ansi(stream, "\x1b[0J");
    entry::boolean_value(true)
}

/// `readline.cursorTo(stream, x[, y][, callback])` — the four-slot signature
/// this module's calling convention leaves means `y` and `callback` cannot
/// both be read positionally when both are given; `y` is read from the third
/// slot whenever it is a number, matching the common `cursorTo(s, x, y)`
/// call shape used in the test plan (§6.9 of the spec).
extern "C" fn cursor_to(_e: u64, _this: u64, stream: u64, x: u64, y: u64, _cb: u64) -> u64 {
    let Some(column) = entry::number_of(x) else {
        return entry::boolean_value(false);
    };
    let sequence = match entry::number_of(y) {
        Some(row) => format!("\x1b[{};{}H", row as i64 + 1, column as i64 + 1),
        None => format!("\x1b[{}G", column as i64 + 1),
    };
    write_ansi(stream, &sequence);
    entry::boolean_value(true)
}

/// `readline.moveCursor(stream, dx, dy[, callback])`.
extern "C" fn move_cursor(_e: u64, _this: u64, stream: u64, dx: u64, dy: u64, _cb: u64) -> u64 {
    let dx = entry::number_of(dx).unwrap_or(0.0) as i64;
    let dy = entry::number_of(dy).unwrap_or(0.0) as i64;
    let mut sequence = String::new();
    if dy < 0 {
        sequence.push_str(&format!("\x1b[{}A", -dy));
    } else if dy > 0 {
        sequence.push_str(&format!("\x1b[{}B", dy));
    }
    if dx < 0 {
        sequence.push_str(&format!("\x1b[{}D", -dx));
    } else if dx > 0 {
        sequence.push_str(&format!("\x1b[{}C", dx));
    }
    write_ansi(stream, &sequence);
    entry::boolean_value(true)
}

/// Writes a literal ANSI escape sequence to a stream's `.write()`.
fn write_ansi(stream: u64, sequence: &str) {
    let value = entry::with_runtime(|context| entry::make_string(context, sequence));
    write_to(stream, value);
}
