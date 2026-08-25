//! `exec` and `execFile` — spawn, accumulate the output, call back once.
//!
//! # Why these are a layer and not a third implementation
//!
//! Because that is what they are in Node: `exec` is `execFile` through a shell,
//! and `execFile` is `spawn` with the output collected. Everything they need
//! now exists — [`super::spawn_async::spawn`] pipes by default, and
//! [`super::stdio`] delivers the chunks — so this file is a listener and a
//! buffer, not a second way to start a process.
//!
//! The refusal that stood here said `exec`/`execFile` *"need the piped-stdio
//! accumulation above"*. They did, and it arrived.
//!
//! # What the callback is handed, and when
//!
//! `(error, stdout, stderr)`, once, after the child has exited AND both pipes
//! have ended — which is Node's order and matters: calling back at `'exit'`
//! would hand over a `stdout` still missing its last chunk, and a test
//! comparing the whole output would fail on a race rather than on a defect.
//!
//! `error` is `null` for a zero exit and an `Error` otherwise, carrying `code`
//! (the exit status) and `killed`, as Node's does. What it does NOT carry is
//! `signal` as a real signal name on Windows — `super::capture` documents the
//! same limit, and it is the OS's rather than this file's.
//!
//! # What is absent, by name
//!
//! `maxBuffer`. Node kills a child whose output passes it; nothing here counts,
//! so a runaway child grows this process's memory instead. Named rather than
//! half-enforced, because a limit that is checked late is a limit a program
//! cannot rely on — and because the honest fix is a counter in the reader
//! thread, which is [`super::stdio`]'s to add.

use rts_core::entry;

use super::shared::{option_value, string, text};

/// `exec(command[, options][, callback])`.
///
/// Through a shell, which is the whole difference from [`exec_file`]: the
/// command is one string and the shell splits it. `spec_of` already knows how —
/// the synchronous `execSync` goes the same way — so this asks for it by
/// setting the `shell` option rather than building a second shell invocation.
pub(super) extern "C" fn exec(_e: u64, _this: u64, command: u64, second: u64, third: u64, _d: u64) -> u64 {
    let (options, callback) = split(second, third);
    let options = with_shell(options);
    let absent = entry::undefined_value();
    let child = super::spawn_async::spawn(absent, absent, command, absent, options, absent);
    collect(child, callback)
}

/// `execFile(file[, args][, options][, callback])`.
pub(super) extern "C" fn exec_file(_e: u64, _this: u64, file: u64, args: u64, third: u64, fourth: u64) -> u64 {
    // `args` may be the options object or the callback when it is omitted,
    // which is Node's own overload rule: an array there is the argument list
    // and anything else has shifted along.
    let absent = entry::undefined_value();
    let arguments = match entry::with_runtime(|context| entry::is_array_in(context, args)) {
        true => args,
        false => absent,
    };
    let (second, third) = match arguments == absent {
        true => (args, third),
        false => (third, fourth),
    };
    let (options, callback) = split(second, third);
    let child = super::spawn_async::spawn(absent, absent, file, arguments, options, absent);
    collect(child, callback)
}

/// Which of two arguments is the options object and which is the callback.
///
/// Node lets either be omitted. Deciding by CALLABILITY rather than by position
/// is what makes `exec(cmd, cb)` and `exec(cmd, opts, cb)` both work — and it is
/// the same test `fs`'s `options_and_listener` makes for the same reason.
fn split(second: u64, third: u64) -> (u64, u64) {
    let absent = entry::undefined_value();
    let callable = |value: u64| entry::with_runtime(|context| entry::is_callable_in(context, value));
    if callable(second) {
        return (absent, second);
    }
    (second, if callable(third) { third } else { absent })
}

/// The caller's options with `shell` turned on.
///
/// A fresh object rather than a write into the caller's: `exec(cmd, opts)` must
/// not leave `opts.shell` set for the caller's next use of the same object,
/// which is a mutation a program can observe.
fn with_shell(options: u64) -> u64 {
    entry::with_runtime(|context| {
        let held = entry::make_object(context);
        if entry::is_object(context, options) {
            for name in entry::member_names(context, options) {
                let value = entry::get_member(context, options, &name);
                entry::put_member(context, held, &name, value);
            }
        }
        let yes = entry::boolean_value(true);
        entry::put_member(context, held, "shell", yes);
        held
    })
}

/// Attaches the listeners that accumulate and finally call back.
///
/// # Why this is written in JavaScript rather than in Rust
///
/// Because the pieces are already JavaScript-shaped: the accumulation is three
/// closures over two strings, and the delivery is `emit` calling them on the
/// program's own thread. Writing it in Rust would mean a second table keyed by
/// process id, a second set of queued records, and a second place that decides
/// when a child is finished — all of which [`super::spawn_async`] already has.
/// What is here instead is the smallest thing that cannot be expressed as data:
/// a native that installs listeners.
fn collect(child: u64, callback: u64) -> u64 {
    let absent = entry::undefined_value();
    if child == absent || callback == absent {
        return child;
    }
    let state = entry::with_runtime(|context| {
        let held = entry::make_object(context);
        let empty = entry::make_string(context, "");
        entry::put_member(context, held, "out", empty);
        entry::put_member(context, held, "err", empty);
        // Three things must happen before the callback: the child exits, and
        // each pipe ends. Counted rather than assumed, because their order is
        // the OS's and not ours.
        let zero = entry::make_number(0.0);
        entry::put_member(context, held, "done", zero);
        entry::put_member(context, held, "code", zero);
        entry::put_member(context, held, "callback", callback);
        entry::put_member(context, held, "child", child);
        held
    });
    let on = entry::with_runtime(|context| entry::get_member(context, child, "on"));
    let out = entry::with_runtime(|context| entry::get_member(context, child, "stdout"));
    let err = entry::with_runtime(|context| entry::get_member(context, child, "stderr"));
    for (stream, slot) in [(out, "out"), (err, "err")] {
        if stream == absent || stream == entry::null_value() {
            // No pipe means no end to wait for: count it as already finished,
            // or the callback waits for a stream that will never speak.
            bump(state);
            continue;
        }
        // Uma funcao por slot, e nao uma funcao marcada com o slot: dentro de
        // um listener o `this` e o STREAM que emitiu, nao a funcao — a marca
        // ficava num objeto que o listener nunca ve, e todo o `stdout` chegava
        // vazio ao callback.
        let native = match slot {
            "out" => gather_out as *const () as usize as i64,
            _ => gather_err as *const () as usize as i64,
        };
        let listener = entry::closure_new(native, state);
        let stream_on = entry::with_runtime(|context| entry::get_member(context, stream, "on"));
        entry::call(stream_on, stream, string("data"), listener, absent, absent);
        let ended = entry::closure_new(finished as *const () as usize as i64, state);
        entry::call(stream_on, stream, string("end"), ended, absent, absent);
    }
    let exited = entry::closure_new(on_exit as *const () as usize as i64, state);
    entry::call(on, child, string("exit"), exited, absent, absent);
    child
}

/// One `'data'` chunk of the child's stdout.
extern "C" fn gather_out(state: u64, _this: u64, chunk: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    gather(state, "out", chunk)
}

/// One `'data'` chunk of the child's stderr.
extern "C" fn gather_err(state: u64, _this: u64, chunk: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    gather(state, "err", chunk)
}

/// Appends one chunk to one slot.
///
/// A chunk arrives as a `Buffer` unless something called `setEncoding`, and
/// `exec` hands the callback TEXT — so the bytes are decoded here rather than
/// by asking the stream to decode, which would change what a program's own
/// `'data'` listener sees.
fn gather(state: u64, slot: &str, chunk: u64) -> u64 {
    let piece = text(chunk).unwrap_or_else(|| {
        let bytes = entry::with_runtime(|context| entry::bytes_of(context, chunk))
            .unwrap_or_default();
        String::from_utf8_lossy(&bytes).into_owned()
    });
    entry::with_runtime(|context| {
        let held = entry::get_member(context, state, slot);
        let mut joined = entry::string_in(context, held).unwrap_or_default();
        joined.push_str(&piece);
        let value = entry::make_string(context, &joined);
        entry::put_member(context, state, slot, value);
    });
    entry::undefined_value()
}

/// One pipe reached its end.
extern "C" fn finished(state: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    bump(state);
    deliver_if_ready(state);
    entry::undefined_value()
}

/// The child exited — record the status and count it.
extern "C" fn on_exit(state: u64, _this: u64, code: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let held = match entry::number_of(code) {
            Some(status) => entry::make_number(status),
            None => entry::make_number(0.0),
        };
        entry::put_member(context, state, "code", held);
    });
    bump(state);
    deliver_if_ready(state);
    entry::undefined_value()
}

/// Counts one of the three things that must happen.
fn bump(state: u64) {
    entry::with_runtime(|context| {
        let held = entry::get_member(context, state, "done");
        let count = entry::number_of(held).unwrap_or(0.0) + 1.0;
        let value = entry::make_number(count);
        entry::put_member(context, state, "done", value);
    });
}

/// Calls back, once all three have.
fn deliver_if_ready(state: u64) {
    let done = entry::with_runtime(|context| {
        entry::number_of(entry::get_member(context, state, "done")).unwrap_or(0.0)
    });
    if done < 3.0 {
        return;
    }
    // Guarded so a fourth event — a `'close'` following an `'end'`, say —
    // cannot call a program's callback twice, which is worse than not calling
    // it: a test counting invocations would report a failure nobody caused.
    let already = entry::with_runtime(|context| entry::get_member(context, state, "called"));
    if already == entry::boolean_value(true) {
        return;
    }
    let absent = entry::undefined_value();
    let (callback, out, err, error) = entry::with_runtime(|context| {
        let yes = entry::boolean_value(true);
        entry::put_member(context, state, "called", yes);
        let callback = entry::get_member(context, state, "callback");
        let out = entry::get_member(context, state, "out");
        let err = entry::get_member(context, state, "err");
        let code = entry::number_of(entry::get_member(context, state, "code")).unwrap_or(0.0);
        let error = match code == 0.0 {
            true => entry::null_in(context),
            false => {
                let object = entry::make_object(context);
                let message = entry::make_string(context, &format!("Command failed with exit code {code}"));
                entry::put_member(context, object, "message", message);
                let held = entry::make_number(code);
                entry::put_member(context, object, "code", held);
                let no = entry::boolean_value(false);
                entry::put_member(context, object, "killed", no);
                object
            }
        };
        (callback, out, err, error)
    });
    entry::call(callback, absent, error, out, err, absent);
}
