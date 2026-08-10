# rts-napi-rwk — the phases

Each phase ends with something that RUNS, not with a file that compiles. The
measure at the end is a real `.node` addon calling in; until one does, the
measure is a test in this crate that drives the same entry points a `.node`
would.

## P1 — the handle model, and values (DONE)

`napi_status`, `napi_value`, `napi_env`, handle scopes, and the value surface
that needs nothing beyond `rts-core`'s existing entry points: numbers, booleans,
`undefined`, `null`, UTF-8 strings both ways, and `napi_typeof`.

Why first: everything else takes or answers a `napi_value`, so the handle model
is the decision the rest inherits. It is also the one that had to be made
differently from `rts-napi` (rule 2 of the README).

The engine piece it needed — a root the collector can see that is not on a frame
— landed with it: `rts_core::entry::external`, and
`collect_cycle::a_value_held_from_outside_the_heap_survives_and_stops_when_released`
is what says it works.

## P2 — objects and properties (DONE)

`napi_create_object`, `get`/`set`/`has`/`delete_property` by key and by name,
`napi_get_property_names`, the array surface (`create_array`,
`create_array_with_length`, `is_array`, `get_array_length`, `get`/`set_element`)
and `napi_get_value_string_utf8`, which is here rather than with the other value
reads because it is the one that writes into memory the ADDON owns.

It needed nothing new underneath, which was the prediction and is worth
recording as kept: every one of them forwards to `rts-core`'s `computed` or
`modules` surface. Two decisions are pinned by tests rather than by comment — a
missing property is `napi_ok` with `undefined` (the language's answer and the
ABI's are the same one), and a truncated string stops at a CHARACTER boundary,
never mid-sequence.

## P3 — functions, both directions (DONE)

`napi_create_function`, `napi_call_function`, `napi_get_cb_info`,
`napi_is_callable`, and `env::destroy`, which is the teardown the registry made
necessary.

The decision this phase existed for: **a callback's identity travels in the
environment**. `closure_new(code, environment)` stores a value beside the code
and hands it back as the call's first argument — the mechanism a compiled
closure uses to find what it captured — so one shared trampoline reads a slot
number from there and stands in for the right addon function. Nothing is keyed
by the callable's value, which is what "not a global map" meant.

The registry is thread-local because a `Context` is: a callback can only run
against the context it was registered under, so a global would be a lock around
something with one legal user.

What is deliberately approximate and stated where it happens: `argc` counts to
the last non-`undefined` word, because the calling convention pads with
`undefined` and `f(1, undefined)` is indistinguishable from `f(1)` at this
layer.

## P4 — references and lifetimes (DONE)

`napi_create_reference`, `napi_reference_ref`/`unref`, `napi_get_reference_value`,
`napi_delete_reference`.

The phase asked the collector for a second capability rather than reusing the
first one wrongly, and got it: `rts_core::entry::weak` WATCHES where `external`
HOLDS, and the sweep clears a watch as it frees the cell. Two mechanisms, not
one with a flag — holding and watching are opposite instructions to the
collector, and a bit deciding which is how a value comes to be kept by a
reference that promised not to.

A refcount above zero holds; zero watches; `ref`/`unref` move between them. Once
the collector has taken the value, `napi_get_reference_value` answers a NULL
handle rather than `undefined`, because an addon must be able to tell a
reference TO `undefined` from a reference whose value is gone.

Second client, not built here: the language's own `WeakRef`, which holds its
target strongly and says so. Wiring it is language-visible — `deref()` would
start answering `undefined` — so it belongs in a change whose suite run is about
that.

## P5 — wrapping native state (DONE)

`napi_wrap`, `napi_unwrap`, `napi_remove_wrap`, `napi_create_external`,
`napi_get_value_external`.

The pointer lives in `rts_core::entry::foreign` — one word in an `Aside`, which
is how that crate already says "state beside a cell" eighteen times over. The
engine being replaced used a heap-entry kind (`Entry::NapiExternal`); this heap
has cells and shapes and no variant to add, which is why the mechanism differs
rather than being ported. A property was the other alternative and is wrong
twice: visible to the program, and it would have to hold a value.

The finalizer runs on `napi_remove_wrap` and on `env::destroy`, and **not when
the object is collected** — that is P6, and it is a collector change rather than
anything this phase could have done. What IS guaranteed is narrower and stated
where an addon author reads it: the pointer never outlives the object, so it can
never be read against a cell that has become something else.

`napi_typeof` answers `napi_external` by asking this module, because the ABI
distinguishes an external from an object and the language does not. The record
is keyed by a WATCH rather than a cell number, for the reason P4 built watches:
a cell is reused, and a stale entry would report somebody else's object as an
external.

## P6 — finalizers (DONE)

The collector tells someone now. `rts_core::entry::finalize` takes a
registration — a C function pointer and two words, so the runtime never learns
what an environment is — and the sweep moves it to a queue as it frees the cell.

**The sweep does not call it, and that is the design rather than a limitation.**
It runs with the runtime's borrow held; a finalizer calls out by definition. So
the queue drains where microtasks do, which is the one point every host in this
repository already pumps — neither `rts-host` nor `rts-runtime` had to be taught
anything.

A wrap now has three triggers and exactly one fires: `napi_remove_wrap` and
`env::destroy` withdraw the registration, the collector IS it.

Still true and stated where an addon author reads it: a finalizer is not a
destructor. It runs at the next drain after the collection, never during it, and
a program that ends first runs none.

Second client, not wired here: `FinalizationRegistry`, which waits on this same
hook and on `WeakRef` being made weak first.

## P7 — async work and threadsafe functions

Blocked on the engine, not on effort. `Context` is reached through a
thread-local and nothing spawns a thread that can run JavaScript; `CLAUDE.md`
records that as an architecture rather than a gap, and this phase is what would
change that decision. It stays last so nothing before it is written against an
assumption it makes.

## P7c — errors (DONE)

`napi_throw`, `throw_error`/`type_error`/`range_error`, `create_error` and its
two siblings, `is_exception_pending`, `get_and_clear_last_exception`,
`is_error`.

A throw goes into the runtime's ONE slot — the same one a compiled `throw`
writes and a compiled call site checks — so an exception an addon raises is
caught by a `try` in the program, and one the program raised is visible to the
addon. Nothing is duplicated.

The engine grew two public functions for it: `throw_value`, which takes the
value and supplies the tag itself (which number a `catch` matches stays the
runtime's secret), and `make_named_error`, which builds one of the language's
own error classes. `named_error` — the runtime's own raise — now goes through
the second rather than beside it, because a second copy of "how an error is
built" is how one of them comes to register the class on demand and the other
not. That was a real bug once.

`napi_is_error` asks `value instanceof Error`, through the same global lookup
and the same operator a program uses. The first version looked for a `message`
property and a test now pins why that is wrong: a plain object with a `message`
is not an Error.

## P7d — classes and descriptors (DONE)

`napi_define_class`, `napi_define_properties`, `napi_new_instance`,
`napi_instanceof`, and the `napi_property_descriptor` an addon builds them from.

A class is assembled the way the LANGUAGE assembles one — a callable marked as
a constructor, a plain object as its prototype, methods put on the prototype,
the two linked. Not with `rts-core`'s `make_prototype`, which is how `Math` and
`Error` are made and is wrong here for two reasons it states itself: it takes a
`&'static str` (an addon's name is read at run time) and it PANICS when two
callers define one name from different files. Inside the engine that can only be
a programming error; from an addon it would be a crash a script could cause.

`napi_static` is honoured because it changes WHERE a member lives. `writable`,
`enumerable` and `configurable` are not, and the module says so rather than
accepting them quietly — a flag taken and ignored reads as supported.

The phase also found a bug in P2 and fixed it: `napi_get_named_property` used
`get_member`, which reads a data property and cannot run user code, so a getter
answered `undefined` by name and its real value by key. Both doors go through
`get_indexed` now, which is what P2's own module doc had claimed all along, and
a test pins it.

## P7e — buffers (DONE)

`napi_create_buffer`, `create_buffer_copy`, `get_buffer_info`, `is_buffer`,
`is_typedarray`, `get_typedarray_info`, `create_arraybuffer`.

The phase is about one thing: the pointer has to be REAL. `rts-core`'s
`bytes_of` copies deliberately, and a compression addon handed a copy fills a
temporary the program never sees — not a slower answer, a wrong one. So
`bytes_pointer` was added there, and its contract is Node's own: valid while
the buffer is alive, which means an addon keeping it across a turn must keep a
`napi_ref` too.

That the address survives other allocations is measured, not assumed — each
buffer's bytes are their own `Vec`, so growing the table moves headers and not
bytes, and a test writes, allocates sixty-four more buffers, and reads back.

Two honest refusals rather than plausible answers: `get_typedarray_info` will
not say which element type a view has (nothing exports it, and guessing
`uint8_array` makes an addon read a `Float64Array` eight times too far), and
`create_arraybuffer` answers a `Uint8Array` because this engine's `ArrayBuffer`
cell has no window of its own — observable as `x instanceof ArrayBuffer` being
false, and named in the module rather than hidden.

## P8a — registration (DONE)

`napi_module_register`, the `napi_module` record, and running a registrar to
produce its exports. Both shapes an addon uses are handled and both are tested:
hanging properties on the object it was given, and answering something else
entirely (a function, a class), which using the given object regardless would
silently discard.

The older path records and does not run. A static constructor fires before a
`Context` exists, so evaluating there would reach a thread-local runtime the
host has not installed — an abort, not an error.

## P8b — the export table (DONE)

Done: the crate is linked into `rts`, every entry point is in one list
(`src/exported.rs`), and the symbols are in the binary. Each of those three took
a measurement rather than an assumption — with the crate merely listed as a
dependency the linker never opened the rlib, and naming it from `rts`'s own lib
was not enough either; the BIN has to take the reference. `napi_create_double`
was absent from the executable until it did.

A test walks `src/` and fails if an entry point is missing from the list, which
is what keeps the export arguments complete once they exist: a symbol absent
there is one an addon fails to resolve, with a name and no explanation.

Left, and it is a change to the BUILD rather than to this crate:

An addon resolves `napi_create_double` and its hundred siblings **out of the
host process**, by name, at load time. That works when the process exports
them: `-rdynamic` on ELF, an export table entry on COFF, `-exported_symbols_list`
on Mach-O. This binary exports none of them, and `rts-napi-rwk` is not even
linked into it.

The export arguments are passed now, by the root `build.rs`, which parses the
same list rather than restating it: `/EXPORT:` per name on COFF,
`--export-dynamic` on ELF, a generated `-exported_symbols_list` on Mach-O.
MEASURED on Windows — the linker produces `rts.exp` and `rts.lib`, 22 KB and
36 KB, and both contain the names. Neither existed before the arguments.

The other two platforms are written and not verified, and the asymmetry is the
same one the AOT position-independence fix carries: what is verified is that the
argument reaches the linker, because the same `build.rs` emits all three.

## P8c — opening a `.node` (DONE)

`loader::open` maps the file and finds how to ask it for its exports:
`LoadLibraryW`/`GetProcAddress` on Windows, `dlopen`/`dlsym` elsewhere.

Both entry points, in the order the platform forces. A modern addon exports
`napi_register_module_v1` and is found by name; an older one calls
`napi_module_register` from a static constructor, which runs while the library
is being MAPPED — before this code sees anything — so the loader watches the
registration list grow across the map instead of asking the library a question
it cannot answer.

`RTLD_NOW`, deliberately: every undefined symbol resolved before anything runs,
so an addon missing one fails where the path is still in hand to name. Lazily,
the same failure arrives inside somebody's callback with no context.

Nothing is ever unloaded — no `dlclose`, no `FreeLibrary`. The addon's code
stays reachable from every value it produced: a callable's code address, a
finalizer, a threadsafe function's callback. Unmapping turns each into a jump
into nothing. Node does not unload addons either, for the same reason.

Measured against a real library, because this repository has no `.node` to
build: the tests map `kernel32.dll` (or `libc`), find a symbol it exports, fail
to find one it does not, and then check that `open` refuses it by name — "it is
a shared library, but not an addon", which is the message a user pointing
`require` at the wrong file needs.

## P7f — the rest of the value surface (DONE)

`napi_create_uint32`/`int64`, `napi_get_value_int32`/`uint32`/`int64`, the four
coercions, `napi_strict_equals`, `napi_get_global`.

Every one is the language's own operator, called: `ToInt32` is `x | 0`,
`Number(x)` is `x - 0`, `String(x)` is `"" + x`. None is reimplemented, and the
tests are the reason — `Number("0x10")` is 16, `ToInt32` of 2^31 is negative,
`ToUint32` of -1 is `u32::MAX`, and a hand-rolled version gets at least one of
those wrong on the first try.

`napi_get_value_int64` is written out rather than left to `as i64`, because the
ABI's three cases and Rust's cast disagree: NaN and the infinities are zero, out
of range clamps, the rest truncates toward zero.

Asking a string for an `int32` is REFUSED rather than coerced. The ABI has
`napi_coerce_to_number` for that, and answering 0 would hide an addon's type
error.

## P8d — a third-party addon (MEASURED, does not run yet)

Done, and the result is a list rather than a guess.

`@napi-rs/uuid-win32-x64-msvc` — a real prebuilt addon off npm, 300 KB — was
loaded with `rts napi <file>`. It MAPPED: the library opens, its constructors
run, and it reaches the process looking for its bindings. Then it panicked with
`Must load N-API bindings`, which is `napi-sys` saying a symbol it requires is
not in the export table.

Which is measurable, and was measured. The addon names every symbol it looks up
as a string in its own binary — that is how `GetProcAddress` binding works — so
the gap is a diff:

**It wants 119. We export 80. Forty-five are missing.**

```
napi_add_env_cleanup_hook          napi_get_dataview_info
napi_adjust_external_memory        napi_get_last_error_info
napi_async_destroy                 napi_get_new_target
napi_async_init                    napi_get_node_version
napi_cancel_async_work             napi_get_prototype
napi_close_callback_scope          napi_get_uv_event_loop
napi_close_escapable_handle_scope  napi_get_value_string_latin1
napi_close_handle_scope            napi_get_value_string_utf16
napi_coerce_to_object              napi_get_version
napi_create_dataview               napi_has_element
napi_create_external_arraybuffer   napi_has_named_property
napi_create_external_buffer        napi_has_own_property
napi_create_promise                napi_is_arraybuffer
napi_create_string_latin1          napi_is_dataview
napi_create_string_utf16           napi_is_promise
napi_create_symbol                 napi_make_callback
napi_create_typedarray             napi_open_callback_scope
napi_delete_element                napi_open_escapable_handle_scope
napi_escape_handle                 napi_open_handle_scope
napi_fatal_error                   napi_reject_deferred
napi_fatal_exception               napi_remove_env_cleanup_hook
napi_get_arraybuffer_info          napi_resolve_deferred
                                   napi_run_script
```

Most are shallow. The handle scopes are `Env::open`/`Env::close`, which already
exist; `has_named_property` and `has_element` are the doors P2 built; the
promise four are `rts-core`'s `promise_new`/`promise_settle`. Three are not:
`napi_get_uv_event_loop` hands over a libuv loop this engine does not have,
`napi_make_callback` is Node's async-context machinery, and
`napi_run_script` needs a compiler the AOT binary deliberately does not carry.

**`napi_get_last_error_info` is the one to do first** regardless of size: it is
what `napi-sys` probes before anything else, so its absence is what turns every
other gap into one unhelpful panic.

Until an addon actually runs, the crate's status stays "the tests pass". What
changed is that the distance to that is now written down.

The old `rts-napi` lists 157 names against this crate's 70, and the difference
is the surface still missing rather than a disagreement — another reason to keep
that tree readable until this one catches up.

Until a third-party addon runs, this crate's honest status stays "the tests
pass".
