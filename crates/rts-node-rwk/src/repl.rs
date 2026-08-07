//! `node:repl` — a line-at-a-time REPL over `node:readline`'s `Interface`.
//!
//! # THE limit, stated up front rather than left to be discovered
//!
//! Every line is evaluated with `rts_core_rwk::entry::evaluate`, which
//! compiles and runs it as its **own fresh program** — no variables in, no
//! declarations out (see `vm.rs`'s module doc, and `entry::modules`'s doc on
//! `evaluate` itself, for the mechanism). That means **`let x = 1` on one
//! line does NOT make `x` visible on the next line** — the single behavior
//! real Node's REPL exists to provide (a persistent session) is exactly what
//! this module cannot give. Each line here is a `vm.runInNewContext`, not an
//! incremental extension of one running program. A caller relying on
//! cross-line state sees an unbound-name failure, which this engine answers
//! as `undefined` (the same handling `entry::evaluate`'s own doc names),
//! rather than the value a prior line assigned.
//!
//! A returned `undefined` after a line is evaluated is therefore ambiguous
//! in the same way `vm.rs` documents: it may be the line's real answer, an
//! object that could not cross, or a line that failed to compile. This
//! module does not attempt to tell those apart — see [`evaluate_line`].
//!
//! # Reuse-check
//!
//! Per `.claude/skills/reuse-check`: nothing machine-shaped is written
//! here. `crate::readline`'s `Interface` (its `namespace`, its
//! `createInterface`, its `'line'` event) is reused rather than
//! reimplemented — a `REPLServer` is built by calling `createInterface`
//! for the base object and chaining `REPLServer`'s own prototype onto
//! `Interface`'s, the same pairing `readline.rs` already uses for
//! `Interface` onto `EventEmitter`. `entry::evaluate` is reused from
//! `vm.rs`'s doc rather than a second compile-and-run seam invented here.
//!
//! # Not implemented, by name
//!
//! - **Persistent cross-line context** (`let x` surviving to the next line)
//!   — the module's whole limit, above; there is no engine capability to
//!   extend an already-compiled program with a later, independent snippet.
//! - **`_`/`_error` magic variables** — meaningless without persistence:
//!   they would need to survive from one evaluated line to the next, which
//!   nothing here does.
//! - **`useGlobal: true`** — "bind `context` to the real global object"
//!   presumes a real global object this module's evaluation can reach;
//!   `entry::evaluate` never runs against one. `replServer.context` is
//!   always a plain, inert object here, documented as such.
//! - **Recoverable multi-line input** (`repl.Recoverable`, continuation
//!   prompts for an unterminated `{`) — telling "incomplete" from "wrong"
//!   apart needs the parser's own diagnostic, which does not cross
//!   `entry::evaluate`'s boundary: only `Some(value)`/`None` does. Every
//!   parse failure reads as `undefined` here, same as a real runtime error
//!   — never buffered for a continuation line.
//! - **`.editor` mode, `.save`, `.load`** — file-backed multi-line editing;
//!   no more meaningful here than multi-line recovery, above.
//! - **`setupHistory`, `NODE_REPL_HISTORY*` env vars** — no persistent
//!   history file I/O is implemented; `setupHistory` is present and calls
//!   its callback with an error rather than pretending to load one.
//! - **`replMode` (`REPL_MODE_STRICT` vs `SLOPPY`)** — accepted as an option
//!   and not acted on: every line evaluates under whatever mode
//!   `entry::evaluate`'s own compile path already uses.
//! - **`breakEvalOnSigint`, `timeout`** — `entry::evaluate` has no interrupt
//!   hook (same limit `vm.rs` names for its own run options).
//! - **Custom `eval`, custom `completer`, `preview`, `useColors`,
//!   auto-`require` of core modules, `domain`-based uncaught-exception
//!   routing** — each needs machinery (a promise/callback bridge, a
//!   raw-mode keystroke hook, ANSI-capable color negotiation, dynamic
//!   module lookup from inside evaluated code, or `node:domain` itself)
//!   this crate does not have; none is approximated.
//! - **`Recoverable`, `REPL_MODE_SLOPPY`/`REPL_MODE_STRICT`,
//!   `repl.builtinModules`** — plain values with no native backing needed,
//!   simply not built here since nothing above reads them.

use rts_core_rwk::entry::{self, Context, Provided};

/// The instance methods every `REPLServer` has beyond what it inherits
/// through the prototype chain [`namespace`] sets up onto
/// `readline.Interface`.
const REPL_METHODS: &[(&str, Provided)] = &[
    ("defineCommand", define_command),
    ("displayPrompt", display_prompt),
    ("setupHistory", setup_history),
];

/// The namespace `node:repl` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[("start", start)];
    let namespace = entry::make_namespace(context, members);
    let readline_namespace = crate::readline::namespace(context);
    let interface_ctor = entry::get_member(context, readline_namespace, "Interface");
    let interface_prototype = entry::get_member(context, interface_ctor, "prototype");
    let repl_prototype = entry::make_prototype(context, "REPLServer", REPL_METHODS);
    entry::set_prototype_in(context, repl_prototype, interface_prototype);
    let repl_ctor = entry::make_callable(context, repl_server_ctor);
    entry::put_member(context, repl_ctor, "prototype", repl_prototype);
    entry::put_member(context, namespace, "REPLServer", repl_ctor);
    namespace
}

/// `repl.start(options)` — the same construction `new REPLServer(options)`
/// does; `start` is the documented entry point, the constructor exists for
/// an embedder that wants to bypass its defaulting.
extern "C" fn start(_e: u64, _this: u64, options: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    repl_server_ctor(0, entry::undefined_value(), options, a1, a2, a3)
}

/// `new repl.REPLServer(options)`. Builds the base `Interface` through
/// `readline.createInterface` (reused, not reimplemented — see the module
/// doc), links a `REPLServer` instance to it in both directions, subscribes
/// the evaluate/print `'line'` handler, and writes the first prompt.
extern "C" fn repl_server_ctor(_e: u64, this: u64, options: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let create_interface = entry::with_runtime(|context| {
        let readline_namespace = crate::readline::namespace(context);
        entry::get_member(context, readline_namespace, "createInterface")
    });
    // Called as an ordinary function value, outside any borrow — the same
    // convention `readline.rs` calls every listener/method value through.
    let base = entry::call(create_interface, absent, options, absent, absent, absent);

    let (server, output, prompt) = entry::with_runtime(|context| {
        // Idempotent by name: the same prototype `namespace` already
        // registered, chained onto `Interface`'s.
        let prototype = entry::make_prototype(context, "REPLServer", REPL_METHODS);
        let server = match entry::is_object(context, this) {
            true => this,
            false => entry::make_instance(context, prototype),
        };
        let input = entry::get_member(context, base, "input");
        let output = entry::get_member(context, base, "output");
        entry::put_member(context, server, "input", input);
        entry::put_member(context, server, "output", output);
        let empty_prompt = entry::undefined_in(context);
        let prompt_option = entry::get_member(context, options, "prompt");
        let prompt = if prompt_option == empty_prompt {
            entry::make_string(context, "> ")
        } else {
            prompt_option
        };
        entry::put_member(context, server, "__prompt__", prompt);
        let context_obj = entry::make_object(context);
        entry::put_member(context, server, "context", context_obj);
        // Each side reaches the other: `on_line`'s `this` is `base` (see
        // `readline.rs`'s own doc on why `emit` binds a listener's `this`
        // to the emitter, not to a wrapper object), so it needs a way back
        // to `server`; `server` needs `base` for the methods it inherits
        // that read `this.input`/`this.output`, which those inherited
        // methods already do directly since `server` itself carries them.
        entry::put_member(context, base, "__repl__", server);
        (server, output, prompt)
    });

    let (line_name, line_method, line_listener) = entry::with_runtime(|context| {
        (
            entry::make_string(context, "line"),
            entry::get_member(context, base, "on"),
            entry::make_callable(context, on_line),
        )
    });
    entry::call(line_method, base, line_name, line_listener, absent, absent);
    write_to_value(output, prompt);
    server
}

/// The `'line'` listener a `REPLServer` subscribes onto its base
/// `Interface`. `this` is the base object (`emit` binds a listener's `this`
/// to the emitter — see `readline.rs`'s own doc) — the `REPLServer` is read
/// back off its `__repl__` property. Evaluates the line (see the module doc
/// on what that does and does not preserve), writes the answer, and
/// re-prompts.
extern "C" fn on_line(_e: u64, this: u64, line: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let server = entry::with_runtime(|context| entry::get_member(context, this, "__repl__"));
    if server == absent {
        return absent;
    }
    let Some(source) = entry::text_of(line) else {
        return absent;
    };
    let result = evaluate_line(&source);
    let rendered = entry::with_runtime(|context| render(context, result));
    let (output, prompt) = entry::with_runtime(|context| {
        (entry::get_member(context, server, "output"), entry::get_member(context, server, "__prompt__"))
    });
    write_to_string(output, &rendered);
    write_to_string(output, "\n");
    write_to_value(output, prompt);
    absent
}

/// Evaluates one REPL line. `None` covers every case `entry::evaluate`
/// itself cannot distinguish (compile failure, or a value that could not
/// cross) — see the module doc; [`on_line`] answers the same `"undefined"`
/// text for all of them.
fn evaluate_line(source: &str) -> Option<u64> {
    entry::evaluate(source)
}

/// The line's printed form. `entry::evaluate`'s answer is a bare value and
/// this crate has no `util.inspect`-style formatter (see the module doc's
/// "Not implemented" list), so this renders only what
/// `entry::number_of`/a boolean/singleton comparison/`entry::text_in`
/// already know how to say.
fn render(context: &Context, value: Option<u64>) -> String {
    let Some(value) = value else {
        return "undefined".to_owned();
    };
    if let Some(number) = entry::number_of(value) {
        return number.to_string();
    }
    if value == entry::boolean_value(true) {
        return "true".to_owned();
    }
    if value == entry::boolean_value(false) {
        return "false".to_owned();
    }
    if value == entry::undefined_in(context) {
        return "undefined".to_owned();
    }
    if value == entry::null_in(context) {
        return "null".to_owned();
    }
    entry::text_in(context, value).unwrap_or_else(|| "undefined".to_owned())
}

/// Calls `output.write(value)` if `output` is present — the same shape
/// `readline.rs`'s own `write_to` uses.
fn write_to_value(output: u64, value: u64) {
    let absent = entry::undefined_value();
    if output == absent {
        return;
    }
    let write_method = entry::with_runtime(|context| entry::get_member(context, output, "write"));
    if write_method != absent {
        entry::call(write_method, output, value, absent, absent, absent);
    }
}

/// [`write_to_value`] over freshly interned text.
fn write_to_string(output: u64, text: &str) {
    let value = entry::with_runtime(|context| entry::make_string(context, text));
    write_to_value(output, value);
}

/// `replServer.defineCommand(keyword, cmd)` — recorded on the instance under
/// `__commands__` (so a caller reading it back at least finds what was
/// registered); not wired to dot-command dispatch, since [`on_line`] never
/// parses a leading `.` — every line is evaluated as source, always. A
/// malformed argument is read as absent rather than thrown, the same limit
/// `readline.rs` documents (no protected region to throw into).
extern "C" fn define_command(_e: u64, this: u64, keyword: u64, cmd: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let absent = entry::undefined_in(context);
        let commands = match entry::get_member(context, this, "__commands__") {
            value if value == absent => {
                let object = entry::make_object(context);
                entry::put_member(context, this, "__commands__", object);
                object
            }
            existing => existing,
        };
        if let Some(name) = entry::text_in(context, keyword) {
            entry::put_member(context, commands, &name, cmd);
        }
    });
    entry::undefined_value()
}

/// `replServer.displayPrompt(preserveCursor?)`.
extern "C" fn display_prompt(_e: u64, this: u64, _preserve: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let (output, prompt) = entry::with_runtime(|context| {
        (entry::get_member(context, this, "output"), entry::get_member(context, this, "__prompt__"))
    });
    write_to_value(output, prompt);
    entry::undefined_value()
}

/// `replServer.setupHistory(pathOrConfig, callback)` — see the module doc:
/// no history file I/O is implemented. Calls `callback(error, this)` with
/// an `Error`-shaped string rather than silently succeeding, so a caller
/// awaiting the callback learns history is unavailable instead of assuming
/// it loaded.
extern "C" fn setup_history(_e: u64, this: u64, _config: u64, callback: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    if callback != absent {
        let error = entry::with_runtime(|context| {
            entry::make_string(context, "setupHistory is not implemented: no history file I/O in this crate")
        });
        entry::call(callback, absent, error, this, absent, absent);
    }
    entry::undefined_value()
}
