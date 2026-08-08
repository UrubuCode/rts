# The UI surface on the new engine

**What this is.** How `rts:egui` and `rts:input` reach the new engine, what the
port changed in the surface a program writes, and what is deliberately still on
the old one.

The design of the UI itself is `egui-crate.md`, `input-system.md` and
`render-input-interfaces.md`. This document is only about the boundary, and it
does not restate them.

---

## The shape

```
rts-egui / rts-dom / rts-render / rts-input     the logic. Plain Rust.
        │                                        &str, f64, u64. No engine.
        ├──────────────┐
        ▼              ▼
   src/abi.rs      rts-ui-rwk          two shells over one logic
   (feature          (crate)
    old-engine)         │
        │               ▼
        ▼        rts-core-rwk          natives: a function pointer beside a cell
   rts-engine  ← rts-abi               natives: a linker symbol
```

**Why two shells and not a migration.** A native is a different thing in each
engine. The old one bakes `__RTS_FN_NS_EGUI_DRAW_RECT` into a symbol table and
the JIT resolves it by name; the new one holds an `extern "C"` function pointer
beside a cell, with no name a linker ever sees — `docs/engine/authoring-natives.md`
§1. There is no spelling of a function that is both.

So the logic became engine-free and each engine got a shell. The old shell is
behind the `old-engine` feature, **on by default**, so the old engine compiles
unchanged and `rts-runtime` — which *is* the old engine — asks for it
explicitly. The new engine takes the same crates with `default-features = false`.

The feature is the right mechanism because the question is one of
**availability** — "does this build have the old ABI?" — which is the only
reason `docs/engine/architecture.md` accepts for a crate boundary. It is not a
permission: nothing is being forbidden, an interface is being removed.

`rts-ui-rwk` is a crate of its own for the same rule one level up. A window
needs a window system: it does not exist on wasm, it does not exist in a
headless build, and a target without one should not pull wgpu and winit into its
graph to find that out. The host installs it behind the `ui` feature.

---

## What changed in the surface, and why each one had to

Three things. Every one of them follows from the convention a native has here,
which is the convention a compiled JavaScript function has:
`(environment, this, a0, a1, a2, a3) -> value`.

### 1. Past four arguments, an options object

`drawMesh` had thirteen parameters. The convention carries four; what overflows
goes into a vector the runtime holds, and that vector is not exposed to a host
native — so a fifth argument is not merely awkward, it is invisible.

```ts
// before
egui.drawMesh(win, mesh, 0, 0, 0, 0, 0, 1, 1, 1, 0xFF00FFFF, 0, 0);
// now
egui.drawMesh(win, { mesh, x: 0, y: 0, z: 0, color: 0xFF00FFFF });
```

**The rule: up to four, positional; more than that, one options object in the
second position.** A surface mixing both conventions at random would be worse
than either. A missing field takes a documented default, because this engine
does not throw yet — `entry/throw.rs` says why — so the alternative to a default
is silence.

Rejected: widening the convention, or exposing the rest vector to hosts. Both
change the engine to accommodate one surface, and the thirteen-positional call
they preserve does not say which zero is the rotation.

### 2. Geometry arrives as a typed view, not an address

```ts
// before
egui.meshUpload(win, verts.ptr(), vertexCount, indices.ptr(), indexCount);
// now
egui.meshUpload(win, new Float32Array(verts), new Uint32Array(indices));
```

A raw address is an arbitrary memory read from a number the program computed,
and here it is worse than that: the collector moves cells, so the address the
program read can stop naming the buffer before Rust uses it. `bytes_of` answers
a **copy** of the window a view describes, and the copy is what makes the
boundary safe. It is paid per upload, not per frame.

It also removes a whole class of caller error: the lengths come from the views,
so a program cannot declare a count that disagrees with its data.

### 3. Booleans are booleans

`isOpen()` and `key()` answer `true`/`false`. The old ABI had no boolean type
and answered `0`/`1`, so every call site compared against a number.

`pump()` answers **"keep going"**. It used to answer `0 = continue`, which is
the convention of a process exit code and the inverse of what a `while` reads.

Everything else keeps its name on purpose: a program that already draws reads
the same, and the difference stays where it is real.

---

## What is not ported, and why each one is a decision

| absent | why |
|---|---|
| `rts:gpu` (compute) | the shell is not written yet. **Not** a blocker — see the correction below |
| `egui.drawWater` | its only consumer is `crate::compute`, which is behind the same feature |
| `rts:dom`, `render.*` | the tree, parser and layout engine are 18 000 engine-free lines in `rts-dom`; the port is the same shell this crate is, for that surface |
| `egui.render(win, dom)` | without the DOM namespace there is no handle to pass, so it would be a function that always draws nothing |

### A correction, kept rather than quietly fixed

This table first said that `rts:gpu` was blocked because "a compute buffer **is**
an `Entry::Buffer` in the old engine's `HandleTable`", so porting it meant
deciding where those bytes live in the new engine.

**That was false**, and it came from reasoning about the module instead of
reading it. The `wgpu::Buffer` lives in a `HashMap<u64, wgpu::Buffer>` inside
`compute` itself. What crosses from the old engine is only the program-side byte
buffer — and `rts-core-rwk` already answers that in all three directions:
`bytes_of`, `write_bytes` and `make_bytes` over a typed view, which is exactly
what `meshUpload` in this very port already uses.

So `rts:gpu` is the same shell as everything else here. What it actually needs:
the five-argument calls (`writeAt`) take an options object like the rest,
`Entry::Buffer` reads become `bytes_of`, the read-back becomes `write_bytes`,
and `adapterName` becomes `make_string`. Work not done, not a wall.

Each absence fails at the `import` line, which is where it should hurt.
`rts-std-rwk` once refused to register a façade `rts:egui` for exactly this
reason: a UI that compiles and does not paint is the failure mode that costs the
most time before it is understood.

---

## How it is verified

`crates/rts-host-rwk/tests/ui_surface.rs` — three tests, each one running the
program: the specifiers resolve, the members are callable, and calling with no
window crosses the boundary instead of aborting. That last one is not pedantry:
a reentrant borrow of the context inside an `extern "C"` frame cannot unwind, so
it **aborts the process** — which is a test disappearing, not a test failing.

None of them opens a window. That is `crates/rts-host-rwk/examples/janela.rs`:

```bash
cargo run -p rts-host-rwk --example janela
```

It is an example rather than an ignored test for a platform reason. winit panics
when the event loop is created off the **main** thread, and a cargo `#[test]`
runs on a secondary one. There is an escape hatch
(`EventLoopBuilderExtWindows::any_thread`) and taking it would mean defeating a
compatibility warning in order to call this a test.

### The number that was here, and why it is gone

This said "600 frames in 11.8 s — about 51 fps against a 60 Hz vsync, in a debug
build", labelled as evidence rather than a performance claim. The label did not
save it. `perf-claim` is unambiguous — *"never benchmark a debug build; a debug
number is not a number"* — and quoting a rate invites the reading the disclaimer
denies.

It is worse than that: **no frame rate measured through vsync measures this
engine at all.** With `PresentMode::Fifo` the monitor sets the pace, so the
number would be about the display in a release build too. There is no profile
that makes it a performance claim; there is only a profile that makes it look
like one.

What the run actually establishes is liveness, and that needs no rate: the
program opened a window, the loop ran to its own end, 600 frames were presented,
and the process exited 0. Run it and look — that is what the example is for.

A real comparison against the old engine is a different experiment and has not
been run: same loop, same k draws per frame, **release**, vsync off, time per
frame on both engines. Until someone runs it, nothing here says the new engine
is faster or slower at drawing.

---

## The one thing the host does that the program does not have to know

`rts_ui_rwk::shutdown()`, called by the host when a program ends.

On Windows a `thread_local` destructor runs from the TLS callback during
`LdrShutdownProcess` — while the driver's DLLs are being unloaded. Destroying a
`wgpu::Device` there makes the AMD D3D12 driver fast-fail with `0xC0000409`
after a clean, fully-flushed run, with no message at all. `rts-egui` already
knew this and had `shutdown_shared_gpu` for the old CLI to call.

The host calls it, not the program: a program that forgot the line would die on
exit, and it has no way to know the line exists.
