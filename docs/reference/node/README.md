# `node:` reference — read this first

These documents describe the `node:` surface **against an engine that no longer
exists**. `crates/rts-codegen-new/` was deleted on 2026-08-10, once `rts ir`,
`rts eval` and `rts emit-types` — the last things entering through it — had been
rebuilt on `rts-codegen` + `rts-cranelift` + `rts-core-rwk`.

## What is still true, and what is not

**Still true:** what each `node:` module must DO. The API surfaces, the
semantics, the argument shapes, the behaviours a test pins — none of that was
ever a property of the compiler that ran them, and the pages are the reference
they were written to be.

**No longer true:** every sentence about how a module is *reached*. The
`NodespaceSpec` / `NODE_SPECS` / `node_lookup` mechanism, the
`ns_prefix_for("node:x")` mapping, and — most often repeated — the doctrine that
"the engine must never hardcode `"crypto"` anywhere, not even in an allow-list"
were rules about `rts-codegen-new`'s Registry. That crate is gone and so is its
doctrine; `CLAUDE.md` says so at the point where it used to say the opposite.

A path like `crates/rts-codegen-new/src/front/run/registry_build.rs` in one of
these pages is a pointer into a deleted tree. It is left as written rather than
mass-edited: a sed over 35 files would produce sentences naming a crate that
does not exist while reading as though they had been checked, and this note is
the honest version of the same information. `git log -- crates/rts-codegen-new`
recovers the real thing when a decision needs its reasoning.

## Where the answers are now

- `crates/rts-node-rwk/` — the `node:` modules this engine provides.
- `crates/rts-core-rwk/README.md` — the rules binding on a native.
- `docs/engine/authoring-natives.md` — how a module is authored here.
- `CLAUDE.md` — what the `rts:`/`node:` surface currently carries, and what
  left by decision.
