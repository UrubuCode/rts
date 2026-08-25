//! `node:*` — the Node compatibility modules, for the new engine.
//!
//! # Why this is a crate and not part of `rts-std`
//!
//! Because the two answer different questions. `rts` is what THIS runtime
//! offers; `node:fs` is what a program written for another runtime expects to
//! find. They diverge — Node's `fs.readFileSync` answers a `Buffer` unless told
//! otherwise, and nothing about this engine wants that — and a crate holding
//! both would keep having to say which of the two a given decision belongs to.
//!
//! # Why it exists NOW and did not before
//!
//! Measured, twice. `node:*` is imported by 102 of the 818 files in the repo's
//! suite, and until modules compiled none of those files could be read at all —
//! so a module here would have been a structure with no producer, which is what
//! the crate READMEs call a gap rather than a feature.
//!
//! `rts-host`'s `suite_coverage` is what changed the answer: with modules
//! landed, 586 of the 818 compile, and the unbound names left in the ranking are
//! Node's. The plug is the same pair `rts-std` uses — `make_namespace` to
//! build one, `declare_module` to name it — so nothing new was invented here.
//!
//! # What every module in here owes the reader
//!
//! A named list of what it does NOT implement. A Node module is large and this
//! is a subset; a subset that pretends otherwise is how a program finds out at
//! run time, and this repository's rule is that a gap is refused by name rather
//! than approximated. `undefined` for a member that is absent is the honest
//! answer, and each module says which members those are.

#![deny(missing_docs)]
#![deny(dead_code)]

mod errors;
pub mod assert;
pub mod async_hooks;
pub mod buffer;
pub mod child_process;
pub mod diagnostics_channel;
pub mod dns;
pub mod cluster;
pub mod console;
pub mod constants;
pub mod crypto;
pub mod dgram;
pub mod domain;
pub mod events;
pub mod fs;
pub mod http;
pub mod http2;
pub mod inspector;
pub mod https;
pub mod module;
pub mod os;
pub mod net;
pub mod path;
pub mod perf_hooks;
pub mod punycode;
pub mod process;
pub mod querystring;
pub mod readline;
pub mod repl;
pub mod string_decoder;
pub mod sqlite;
pub mod stream;
pub mod test_runner;
pub mod timers;
pub mod trace_events;
pub mod tls;
pub mod tty;

pub mod url;
pub mod util;
pub mod v8;
pub mod wasi;
pub mod vm;
pub mod worker_threads;
mod ws;
pub mod zlib;

use rts_core::entry::Context;

/// Registers every `node:` module this crate provides.
///
/// Under both spellings: `node:fs` and `fs`. The prefixed one is what a modern
/// program writes and the bare one is what the ecosystem is full of, and a
/// resolver that answered only one of them would refuse half the corpus for a
/// reason that has nothing to do with what it can do.
/// The modules built the first time a program names one, and not before.
///
/// # Why deferral is worth a table
///
/// Measured, 2026-08-11, release: [`install`] cost 5.7 ms of the 6.4 ms a
/// trivial program spent between having compiled code and running it, and
/// building these namespaces was 4 ms of it — paid by every program, including
/// the ones that import nothing. The cost is spread (`constants` 0.9 ms,
/// `http2` 0.8 ms, then a long tail), so there was no single module to fix.
///
/// # Why these and not everything
///
/// The list above [`install`]'s use of this one is eager, and each entry there
/// says which of three reasons put it there. Nothing on THIS list is named by
/// another specifier or by a global, so the only way to reach one is an import
/// — which is exactly the event that builds it.
const LAZY: &[(&str, fn(&mut Context) -> u64)] = &[
    ("assert", assert::namespace),
    ("async_hooks", async_hooks::namespace),
    ("buffer", buffer::namespace),
    ("child_process", child_process::namespace),
    ("cluster", cluster::namespace),
    ("console", console::namespace),
    // A flattened VIEW of `os.constants`, `fs.constants` and the RSA half of
    // `crypto.constants` — the tables themselves stay where they are, so no
    // number is defined twice. Node deprecated this module and the ecosystem
    // still imports it.
    ("constants", constants::namespace),
    ("crypto", crypto::namespace),
    ("dgram", dgram::namespace),
    ("diagnostics_channel", diagnostics_channel::namespace),
    ("dns", dns::namespace),
    ("domain", domain::namespace),
    ("http", http::namespace),
    ("http2", http2::namespace),
    ("https", https::namespace),
    ("net", net::namespace),
    ("os", os::namespace),
    ("path", path::namespace),
    ("punycode", punycode::namespace),
    ("querystring", querystring::namespace),
    ("readline", readline::namespace),
    ("repl", repl::namespace),
    ("sqlite", sqlite::namespace),
    ("string_decoder", string_decoder::namespace),
    ("test", test_runner::namespace),
    ("tls", tls::namespace),
    ("trace_events", trace_events::namespace),
    ("tty", tty::namespace),
    ("v8", v8::namespace),
    ("vm", vm::namespace),
    ("wasi", wasi::namespace),
    ("worker_threads", worker_threads::namespace),
    ("zlib", zlib::namespace),
];

/// Registers every `node:` module this crate provides.
///
/// Under both spellings: `node:fs` and `fs`. Most are registered rather than
/// built — see [`LAZY`].
pub fn install(context: &mut Context) {
    // Built once and named twice. `fs::namespace` makes a NEW object every call,
    // so asking it a second time for `fs.promises` would register a specifier
    // pointing at an object no `fs` a program imported ever held.
    let files = fs::namespace(context);
    // `events` and `stream` FIRST, and this is load-bearing rather than tidy:
    // `make_prototype` is idempotent BY NAME, so whoever asks for "EventEmitter"
    // first decides what is on it. A module that chains onto it asks with an
    // empty member list, so if it runs first the real methods never arrive and
    // `emit` silently reaches no listener — which is what the alphabetical order
    // of this table caused the moment `console` and `dgram` joined it.
    let events_namespace = events::namespace(context);
    let stream_namespace = stream::namespace(context);
    // AFTER those two, and built once because `node:inspector/promises` names it
    // a second time below. Putting it before them cost a test run: it chains
    // onto `EventEmitter` with an empty member list, won the name, and every
    // emitter in the program silently lost its methods. The comment above is
    // load-bearing and this is the proof.
    let inspecting = inspector::namespace(context);
    // Built once and named twice, the same reason `files` is: `node:sys` is
    // Node's own deprecated alias of `node:util` (`require('sys') ===
    // require('util')`), not a second module, so a program importing either
    // reaches the identical object rather than a second `format`/
    // `isDeepStrictEqual` this crate would have to keep in step with the
    // first.
    let util_namespace = util::namespace(context);
    // The ones that must exist before the program starts, and each is on this
    // list for a reason rather than by habit — see [`LAZY`] for what the rest
    // do instead.
    //
    // - `events` and `stream`: the prototype-order rule above. A module that
    //   chains onto `EventEmitter` must never be built first, and a LAZY module
    //   is built whenever a program happens to import it, which is an order
    //   nothing here controls. Keeping these two eager is what makes every
    //   deferred build safe: they are already in place when one runs.
    // - `fs`, `inspector`, `util`: a second specifier below names an object
    //   read out of one of them (`fs/promises`, `inspector/promises`, `sys`).
    // - `process`, `timers`, `url`, `perf_hooks`: a member of each is a GLOBAL,
    //   so a program reaches it with no import for the deferral to key on.
    let modules = [
        ("events", events_namespace),
        ("fs", files),
        ("inspector", inspecting),
        ("perf_hooks", perf_hooks::namespace(context)),
        ("process", process::namespace(context)),
        ("stream", stream_namespace),
        ("timers", timers::namespace(context)),
        ("url", url::namespace(context)),
        ("util", util_namespace),
        ("sys", util_namespace),
    ];
    for (name, namespace) in modules {
        rts_core::entry::declare_module(context, &format!("node:{name}"), namespace);
        rts_core::entry::declare_module(context, name, namespace);
    }
    // Registering all of these costs 14 us, measured 2026-08-11 — which is the
    // answer to "why not prune the list by what the program imports instead":
    // a pruning pass could remove only this, and building what is left is 1.4 ms
    // of eager namespaces the globals need whether or not anything is imported.
    for (name, build) in LAZY {
        rts_core::entry::declare_module_lazy(context, &[&format!("node:{name}"), name], *build);
    }

    // `ws` fica FORA da lista acima, e é o único: aquele laço registra cada
    // módulo sob duas grafias porque `node:fs` e `fs` são dois nomes do mesmo
    // módulo do Node. `ws` não é um módulo do Node sob nenhuma das duas — é um
    // pacote do npm, e a plataforma só oferece o evento `'upgrade'` do
    // `http.Server` para quem quiser escrever um. Registrar `node:ws`
    // inventaria um nome que o Node não tem.
    ws::install(context);

    // What still has work to do after the program's last statement.
    //
    // Four of these were never pumped by anything: each has a background thread
    // and a `pump` that only ran when the program happened to call into that
    // same module again, so a watcher started and then waited on delivered
    // nothing. The host used to name two by hand, which is precisely how the
    // other four went unnoticed — a list of names is a thing to forget to add
    // to.
    //
    // `timers` and `worker_threads` register themselves where they build their
    // namespaces, because both are reached on a worker's thread too. These four
    // are registered here because they are reached through several namespaces
    // each (`fs` and `fs/promises`; `net`, `http` and `https`) and registering
    // per namespace would register them more than once.
    rts_core::entry::declare_loop_source(context, "node:fs.watch", fs::source);
    rts_core::entry::declare_loop_source(context, "node:net", net::source);
    rts_core::entry::declare_loop_source(context, "node:dgram", dgram::source);
    rts_core::entry::declare_loop_source(
        context,
        "node:child_process",
        child_process::source,
    );

    // What a program reaches with NO import line.
    //
    // Node makes these globals and this crate had exactly one — `console`, in
    // `rts-std`. So `process.argv`, `Buffer.from`, `setTimeout` and `new
    // URL(...)` were all unbound names in a program that never imported them,
    // which is how most programs write them.
    //
    // Every one of these is a member of a namespace built above, reached rather
    // than rebuilt: a second `URL` class beside `node:url`'s would make
    // `new URL(x) instanceof (await import("node:url")).URL` false, and two
    // `Buffer`s would not compare equal. The classes stay where they are and the
    // global is another name for the same cell.
    let by_name = |name: &str| {
        modules
            .iter()
            .find(|(held, _)| *held == name)
            .map(|(_, namespace)| *namespace)
    };
    // `process` and `Buffer` are the two a program reaches for without thinking.
    // `Buffer` comes from the runtime rather than from `node:buffer`, because
    // that is where the class lives — `layering.md` puts it in `rts-core`.
    if let Some(namespace) = by_name("process") {
        rts_core::entry::declare_global(context, "process", namespace);
    }
    let buffer_class = rts_core::entry::buffer_class(context);
    rts_core::entry::declare_global(context, "Buffer", buffer_class);
    // `global` — Node's older name for the global object, and the SAME object,
    // which is what a program tests when it writes `global === globalThis`. It
    // is here rather than in `rts-core` because it is Node's spelling and not
    // the language's: `globalThis` is what the specification names.
    //
    // Node's own test harness reads it on its first line, so a corpus written
    // against Node dies at load without it — measured against
    // `test/common/index.js`, which is what found this.
    let global = rts_core::entry::global_object(context);
    rts_core::entry::declare_global(context, "global", global);
    // `Blob` and `File` are globals in Node, and this crate already had both —
    // whole, with `slice` as a real view and the three async readers — sitting
    // inside `node:buffer` where only an import could reach them. Minted here
    // rather than by building that namespace, because `node:buffer` is deferred
    // and building it eagerly for two names would pay for the whole module on
    // every program; `blob::classes` is idempotent, so the import path finds the
    // same two cells and `globalThis.Blob === (await import("node:buffer")).Blob`
    // holds.
    let (blob, file) = buffer::blob::classes(context);
    rts_core::entry::declare_global(context, "Blob", blob);
    rts_core::entry::declare_global(context, "File", file);
    // `globalThis.crypto` is Node's `require('node:crypto').webcrypto`, and it
    // is the SAME object here — `webcrypto::object` is memoized per context, so
    // building it now does not commit us to a second one when something later
    // imports the deferred `node:crypto`. Two would make
    // `globalThis.crypto === crypto.webcrypto` false, which Node answers true.
    let web_crypto = crypto::webcrypto::object(context);
    rts_core::entry::declare_global(context, "crypto", web_crypto);
    // The timer family, `URL`/`URLSearchParams` and `performance`: each named
    // out of the namespace that owns it.
    let from_modules: &[(&str, &[&str])] = &[
        (
            "timers",
            &[
                "setTimeout",
                "clearTimeout",
                "setInterval",
                "clearInterval",
                "setImmediate",
                "clearImmediate",
            ],
        ),
        ("url", &["URL", "URLSearchParams"]),
        ("perf_hooks", &["performance"]),
    ];
    for (module, names) in from_modules {
        let Some(namespace) = by_name(module) else {
            continue;
        };
        for name in *names {
            let member = rts_core::entry::get_member(context, namespace, name);
            rts_core::entry::declare_global(context, name, member);
        }
    }

    // `node:module` answers WHICH modules exist, so it is built from the list
    // just registered rather than from a second copy of the names — two lists is
    // how `isBuiltin` comes to disagree with what an import actually finds. It
    // names itself too, which the loop above cannot do for it.
    let mut provided: Vec<&str> = modules.iter().map(|(name, _)| *name).collect();
    // The deferred ones too, and this is the reason `isBuiltin` could not be
    // built from what has been BUILT: a module nothing has imported yet is
    // still a module Node reports as built in, and asking the registry "which
    // namespaces exist" would answer with whichever ones a program happened to
    // reach. The registration is the fact; the object is not.
    provided.extend(LAZY.iter().map(|(name, _)| *name));
    provided.push("module");
    // `isBuiltin("timers/promises")` is true in Node, and this list is the only
    // thing that answers it. It is named here rather than at the point the
    // specifier is registered below because `node:module` is built before that
    // — and a second list, kept in step by hand, is exactly how `isBuiltin`
    // comes to disagree with what an import finds.
    provided.push("timers/promises");
    provided.push("assert/strict");
    provided.push("dns/promises");
    let modules_module = module::namespace(context, &provided);
    rts_core::entry::declare_module(context, "node:module", modules_module);
    rts_core::entry::declare_module(context, "module", modules_module);

    // `node:inspector/promises` is its own specifier, and here it resolves to
    // the SAME namespace as `node:inspector`. In Node the two differ in exactly
    // one way — `session.post` answers a promise instead of taking a callback —
    // and this crate cannot mint a promise from a native, so the difference
    // cannot be expressed. Registering it anyway is the lesser wrong: a program
    // importing it gets a working `Session` whose `post` takes a callback,
    // rather than an unresolved specifier. `inspector/mod.rs` says so in its
    // "not implemented" list, which is where a program's author would look.

    rts_core::entry::declare_module(context, "node:inspector/promises", inspecting);
    rts_core::entry::declare_module(context, "inspector/promises", inspecting);

    // `node:fs/promises` is its own specifier and resolves to the object `fs`
    // carries as `promises` — the SAME object, not a second one built beside it.
    // Two would be two answers to what `fs.promises.readFile === ` compares.
    let promises = rts_core::entry::get_member(context, files, "promises");
    rts_core::entry::declare_module(context, "node:fs/promises", promises);
    rts_core::entry::declare_module(context, "fs/promises", promises);

    // `node:util/types` the same way, and the same object `util` carries as
    // `types`: `require("util/types").isDate === require("util").types.isDate`
    // is a line a program can write, and two objects would make it false.
    //
    // Absent until now, and the corpus that found it is Node's own: its
    // `test/common/index.js` requires this specifier on the way in, so every
    // file of that suite died at load with "cannot find module" — a missing
    // NAME reported where a missing feature would be looked for.
    // `node:stream/web` sao as classes WHATWG que os globais ja instalaram —
    // a MESMA classe sob a segunda porta que o Node lhe da, nao um segundo
    // conjunto. Ver `stream/web.rs`.
    let web = stream::web_namespace(context);
    rts_core::entry::declare_module(context, "node:stream/web", web);
    rts_core::entry::declare_module(context, "stream/web", web);

    // `node:stream/promises` — as MESMAS duas operacoes de `node:stream`, a
    // responder promessas. Uma camada e nao uma segunda implementacao, senao
    // "este fluxo acabou?" passa a ter duas respostas que discordam na primeira
    // vez que uma delas souber de um erro que a outra nao soube.
    let stream_promises = stream::promises_namespace(context);
    rts_core::entry::declare_module(context, "node:stream/promises", stream_promises);
    rts_core::entry::declare_module(context, "stream/promises", stream_promises);

    let types = rts_core::entry::get_member(context, util_namespace, "types");
    rts_core::entry::declare_module(context, "node:util/types", types);
    rts_core::entry::declare_module(context, "util/types", types);

    // `node:timers/promises` is a module of its OWN, and this is the one place
    // in this function where that is true of a `x/y` specifier: it exports
    // different functions from `node:timers` rather than an object `timers`
    // already carries. So it is built here rather than read out of a namespace —
    // and it shares the parent's timer table, which is what makes
    // `setTimeout(cb, 5)` and `await setTimeout(5)` one order instead of two.
    let timing = timers::promises::namespace(context);
    rts_core::entry::declare_module(context, "node:timers/promises", timing);
    rts_core::entry::declare_module(context, "timers/promises", timing);

    // `node:assert/strict` and `node:dns/promises` were listed as ABSENT
    // specifiers while the objects they name had been built all along —
    // `assert.strict` and `dns.promises` are members of the namespaces just
    // registered. So the gap was three lines of registration each, not a module,
    // and a program importing either got the empty namespace an unregistered
    // specifier leaves behind while `assert.strict.equal` worked perfectly one
    // property read away.
    //
    // Read out rather than rebuilt, for the reason `fs/promises` states one
    // comment down: a second `assert.strict` would make
    // `(await import("node:assert/strict")).equal === assert.strict.equal`
    // false, and `dns.promises` carries the `ADDRCONFIG`/`V4MAPPED`/`ALL`
    // numbers that `dns::namespace` put on THAT object.
    let assertions = rts_core::entry::module_at_name(context, "node:assert");
    let strict = rts_core::entry::get_member(context, assertions, "strict");
    rts_core::entry::declare_module(context, "node:assert/strict", strict);
    rts_core::entry::declare_module(context, "assert/strict", strict);
    let names = rts_core::entry::module_at_name(context, "node:dns");
    let resolving = rts_core::entry::get_member(context, names, "promises");
    rts_core::entry::declare_module(context, "node:dns/promises", resolving);
    rts_core::entry::declare_module(context, "dns/promises", resolving);

    // `node:path/posix` and `node:path/win32` are their own specifiers too, and
    // they resolve to the objects `path` already carries under those names — the
    // SAME objects, for the reason one comment up: two would be two answers to
    // what `path.posix === (await import("node:path/posix")).default` compares.
    //
    // Two files in the suite import them this way, and every assertion in both
    // read `undefined`, which is what an unresolved specifier leaves behind.
    let paths = rts_core::entry::module_at_name(context, "node:path");
    {
        let posix = rts_core::entry::get_member(context, paths, "posix");
        rts_core::entry::declare_module(context, "node:path/posix", posix);
        rts_core::entry::declare_module(context, "path/posix", posix);
        let win32 = rts_core::entry::get_member(context, paths, "win32");
        rts_core::entry::declare_module(context, "node:path/win32", win32);
        rts_core::entry::declare_module(context, "path/win32", win32);
    }
}
