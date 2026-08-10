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

## P6 — finalizers

Nothing runs when a cell is freed today, and that is now the ONLY missing
trigger: P5 wired the other two. A collector change first and a crate change
second — the sweep knows a cell is dying and tells nobody, and the same hook is
what `FinalizationRegistry` waits on. `entry::weak` is half of it already: it
learns of a death at exactly the right moment, and what it does with that today
is clear a word rather than call anything.

## P7 — async work and threadsafe functions

Blocked on the engine, not on effort. `Context` is reached through a
thread-local and nothing spawns a thread that can run JavaScript; `CLAUDE.md`
records that as an architecture rather than a gap, and this phase is what would
change that decision. It stays last so nothing before it is written against an
assumption it makes.

## P8 — loading a real `.node`

`napi_module_register`, the export table, and the first third-party addon that
runs. This is the phase that replaces every claim above with a measurement, and
until it happens, this crate's status is "the tests pass".
