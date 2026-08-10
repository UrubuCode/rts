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

## P3 — functions, both directions

`napi_create_function` (a JS callable whose body is the addon's), `napi_call_function`,
`napi_callback_info` (argc/argv/this/data). The engine has `make_callable` and
`call_with_args`; what this phase decides is where a callback's `data` pointer
lives, and the answer is not a global map.

## P4 — references and lifetimes

`napi_create_reference`, `napi_reference_ref`/`unref`/`delete`, and the WEAK
case, which `entry::external` deliberately does not answer: it holds, and a weak
reference must not. That is the phase that asks the collector for a second
capability rather than reusing the first one wrongly.

## P5 — wrapping native state

`napi_wrap`, `napi_unwrap`, `napi_create_external`. Needs a raw pointer beside a
cell; `rts-core`'s `Aside<T>` is the established shape and the reason this is
not P2.

## P6 — finalizers

Nothing runs when a cell is freed today. This is a collector change first and a
crate change second, and it is where P5's pointers stop leaking.

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
