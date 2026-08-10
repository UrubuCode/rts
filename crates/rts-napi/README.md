# rts-napi — not built, and waiting for a port

**This crate does not compile and is not a workspace member.** It is kept, in
full, because it is the only thing in the repository that implements N-API and
the intent is to port it — not to rewrite it from memory later.

## Why it stopped building

It names `rts-engine` and `rts-shared` directly — handles, `alloc_rtse`,
`global_roots`, `rts_shared::globals::date` — and both were deleted on
2026-08-10 with the rest of the old engine. Sixteen call sites in one file reach
`rts_engine::heap::handles::*`; that is the whole port, and it is a real one:
the old runtime's handle table and this engine's `Region` are different
answers to the same question, not two spellings of one.

## What the port has to answer

- **A handle that outlives a call.** N-API hands a `napi_ref` to C code that may
  keep it across turns. `rts-core-rwk` has no equivalent yet — a value is
  reachable from the region or it is not — so this needs whatever the collector
  decides an external root is.
- **Where the finalizer queue drains.** The old engine had
  `drain_pending_napi_finalizers` on its own loop. The new one has
  `rts_cranelift::sched` and the host's drain.
- **The 157 `napi_*` symbols stay exactly as they are.** They are a foreign C
  ABI whose names ARE the interface — a compiled `.node` links against those
  strings — so `CLAUDE.md`'s "never hand-write a symbol name" does not apply to
  them, and converting them to an attribute would break every existing addon.

## To bring it back

Re-add `"crates/rts-napi"` to the workspace `members` and port the file the
compiler names first. `docs/guides/napi.md` is the plan as it stood, and its
banner says which engine it was written against.
