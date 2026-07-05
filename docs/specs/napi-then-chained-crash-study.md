# Study — chained `.then()` crash on an N-API Promise (#1548)

> Diagnostic document. The bug is **isolated and the root cause validated by
> experiment**. This file is the full context for the fix.

## Symptom

```ts
import a from "./pstr.node";
a.makeP().then((s: string) => console.log("got: " + s));  // CRASH ILLEGAL_INSTRUCTION
```

`a.makeP()` creates an N-API Promise (via `napi_create_promise` +
`napi_resolve_deferred`, synchronous — the Promise already returns **fulfilled**). The
`.then(cb)` **chained directly** on the result of `makeP()` crashes with
`ILLEGAL_INSTRUCTION (0xC000001D)` — a jump to an invalid fn ptr.

The test addon (`/tmp/napitest/add_addon/src/lib.rs`) is minimal:
```rust
unsafe extern "C" fn make_p(e:E,_info:I)->V {
    let mut def=ptr::null_mut(); let mut p=V(ptr::null_mut());
    napi_create_promise(e,&mut def,&mut p);
    let s=b"hello_string_result";
    let mut val=V(ptr::null_mut()); napi_create_string_utf8(e,s.as_ptr() as *const c_char,s.len(),&mut val);
    napi_resolve_deferred(e,def,val);   // resolves SYNCHRONOUSLY, inside makeP()
    p
}
```

## Decisive fact (experiment that isolates the cause)

| TS form | Result |
|---|---|
| `a.makeP().then(cb)` (chained) | **CRASH** (0-arg number, 1-arg string, 0-arg string — ALL) |
| `const p = a.makeP(); p.then(cb)` (Promise in a LOCAL) | **WORKS** (`LOCAL got: hello_string_result`, 0-arg number ok) |
| `Promise.resolve("x").then(cb)` (NATIVE) | **WORKS** |

The ONLY difference between the first two lines is the **receiver of the `.then`**:
- chained → receiver is an `Expr::Call` (`a.makeP()`)
- local → receiver is an `Expr::Ident` (`p`)

Therefore the napi Promise's representation is **correct** (`Entry::PromiseAsync`,
identical to the native one — both subsystems `__RTS_FN_NS_PROMISE_*` and
`__RTS_FN_GL_PROMISE_*` use the same `Entry::PromiseAsync(promise_slot)`). The bug
is purely a **codegen routing** issue of the `.then` when the receiver is not a
local ident.

## Why the LOCAL path works

`const p = a.makeP(); p.then(cb)`:
- `p` is a local Handle/I64 and there is a native addon in the program →
  `calls/mod.rs:808-822` calls
  `indirect::lower_napi_instance_method_call(ctx, "p", "then", call)`.
- That function (indirect.rs:813) has the **already-fixed** fast-path (lines
  839-863): for `then/catch/finally`, lower the callback with
  `lower_expr + coerce_to_i64` (identical to the native `.then`, letting the
  `__hoisted_arrow_` codegen choose handle-typed-for-number vs raw-func_addr-for-
  string/0-arg) and call `__RTS_FN_GL_PROMISE_THEN`/`THEN2`/`CATCH`/`FINALLY`.
- `GL_PROMISE_THEN2` (`globals/fetch/instance.rs:167`) does `classify` →
  `PromiseKind::Async(arc)` → settled fast-path → `enqueue_microtask_settled`.
  Drains correctly.

## Why the CHAINED path crashes

`a.makeP().then(cb)`:
- `a.makeP()` is lowered by `lower_native_addon_method_call`
  (indirect.rs:732) which returns **`TypedVal::new(result, ValTy::I64)`**
  (indirect.rs:803) — a Promise handle typed as **raw I64**.
- The `.then` has receiver `Expr::Call`, NOT `Expr::Ident`, so
  `qualified_member_name(callee)` is `None` and the napi-instance branch
  (calls/mod.rs:799-822) **never fires** (it requires `qualified.split_once('.')`
  with an ident obj).
- It falls into the generic member-method block (calls/mod.rs:1197+,
  `!matches!(m.obj, Expr::Ident)`):
  - line 1246: `recv_tv.ty == ValTy::I64` →
    `try_global_class_instance_method(ctx, "Number", "then", ...)`.
    `Number.prototype.then` does not exist in the Registry → `None`.
  - line 1258: `ValTy::Handle` does NOT match (it's I64) → skips the Handle block.
  - falls into the **generic fallback** (string_builtin / `MAP_GET("then")` + invoke),
    which reifies the callback as a handle and invokes it with the wrong convention →
    **ILLEGAL_INSTRUCTION**.

Confirmed in the IR (`rts ir --allow-native-addons /tmp/napitest/ptnum.ts`): in the
native case the `.then` receives a raw `func_addr`; in the chained napi case there is
`call fnX(handle_slot, func_addr)` (reify) + `.then(promise, reified_handle)`
— wrongly reified callback.

## The fix (what the agent must implement)

Route `<expr>.then/catch/finally(cb)` to `__RTS_FN_GL_PROMISE_THEN*`
**also when the receiver is a napi Promise resulting from a native-addon
CallExpr** (not only when it is a local).

Options (pick the cleanest, most surgical, regression-free one):

### Option A (preferred) — detect in the generic block
In the `!matches!(m.obj, Expr::Ident)` block of `calls/mod.rs` (~1197), BEFORE the
Number/Handle branches (~1239-1257), add:

```rust
// (N-API) <expr>.then/catch/finally where <expr> may be a napi Promise
// (result of a native addon, or any Handle/I64 with an addon in the program).
// Route to GL_PROMISE_THEN* — which detects PromiseAsync at runtime via
// classify() and enqueues the microtask. Without this, the chained .then falls
// into the MAP_GET fallback and crashes (callback reified with the wrong convention).
// The receiver is coerced to i64 (handle). GL_PROMISE_THEN2 with NotPromise is
// a safe passthrough (returns the handle itself), so it is benign if it is not a
// Promise — but restricting to `any_native_addon()` avoids affecting other cases.
if matches!(method_name.as_str(), "then" | "catch" | "finally")
    && !call.args.is_empty()
    && crate::codegen::lower::passes::native_addon::any_native_addon()
    && matches!(recv_tv.ty, ValTy::I64 | ValTy::Handle)
{
    let recv = ctx.coerce_to_i64(recv_tv).val;
    // callback EXACTLY like the native .then (lower_expr + coerce_to_i64) —
    // NOT lower_callable_target_h (which reifies handle is_arrow=0/rk=void and
    // crashes for 0-arg and string). Identical to lower_napi_instance_method_call.
    let lower_cb = |ctx: &mut FnCtx, e: &Expr| -> Result<cranelift_codegen::ir::Value> {
        let tv = lower_expr(ctx, e)?;
        Ok(ctx.coerce_to_i64(tv).val)
    };
    let cb_h = lower_cb(ctx, &call.args[0].expr)?;
    let (sym, arity3) = match method_name.as_str() {
        "then" if call.args.len() >= 2 => ("__RTS_FN_GL_PROMISE_THEN2", true),
        "then" => ("__RTS_FN_GL_PROMISE_THEN", false),
        "catch" => ("__RTS_FN_GL_PROMISE_CATCH", false),
        _ => ("__RTS_FN_GL_PROMISE_FINALLY", false),
    };
    let result = if arity3 {
        let cb2 = lower_cb(ctx, &call.args[1].expr)?;
        let f = ctx.get_extern(sym, &[cl::I64, cl::I64, cl::I64], Some(cl::I64))?;
        let inst = ctx.builder.ins().call(f, &[recv, cb_h, cb2]);
        ctx.builder.inst_results(inst)[0]
    } else {
        let f = ctx.get_extern(sym, &[cl::I64, cl::I64], Some(cl::I64))?;
        let inst = ctx.builder.ins().call(f, &[recv, cb_h]);
        ctx.builder.inst_results(inst)[0]
    };
    return Ok(TypedVal::new(result, ValTy::Handle));
}
```

**Caution:** `recv_tv` has already been consumed by `lower_expr(ctx, &m.obj)` at line
1198. Reuse `recv_tv` (do not re-lower m.obj — otherwise makeP() runs twice and the
deferred is registered twice). Verify the exact position: insert AFTER
`let mut recv_tv = lower_expr(ctx, &m.obj)?;` (1198) and the mapcall re-typing block
(1223-1235), but BEFORE the Number branch (1246). Coercing `recv_tv` to i64
consumes it; since the following branches also consume it, ensure the `return` in the
match (do not fall through).

**`any_native_addon()` restriction**: prevents this branch from intercepting `.then` of
expressions in programs WITHOUT an addon (where the native path already works via the
Promise type-tag). With an addon present, it is safe because `GL_PROMISE_THEN2`
treats `NotPromise` as a passthrough (returns the handle without invoking the callback) — but
restricting it still minimizes the surface.

### Option B — re-type makeP's return as Handle
Less surgical; `lower_native_addon_method_call` doesn't know whether the return is a
Promise (only known at runtime). Discarded — it would require type-flow that doesn't exist.

## Mandatory post-fix verification

Tests that MUST pass (all in `/tmp/napitest/`):
```
target/release/rts.exe run --allow-native-addons /tmp/napitest/ptnum.ts
  → "num then ran, no value"
target/release/rts.exe run --allow-native-addons /tmp/napitest/pstr.ts
  → "got: hello_string_result"
target/release/rts.exe run --allow-native-addons /tmp/napitest/pstr2.ts
  → "then ran (no value used)"
```
Non-regression (must keep working):
```
/tmp/napitest/ptnum_local.ts, pstr_local.ts  (local var — already ok)
/tmp/napitest/nat0num.ts, nat0str.ts, natstr.ts  (native — already ok)
```
Suite:
```
cargo build --release   (TWO steps: -p rts-runtime first if touching AOT)
cargo test --release --lib
target/release/rts.exe test    (expected 1710/1710 — native .then must not regress)
```

## Known build gotcha

`cargo build --release --bin rts` sometimes does NOT recompile the chain
(rts-codegen → bin), reporting "Finished" without a rebuild. Force it by touching a file
in the chain (e.g.: append `// rl <epoch>` to `crates/rts-runtime/src/lib.rs`,
build, remove). Always check the mtime of `target/release/rts.exe` after the
build and re-run the napi tests.

## Pending cleanup (do not forget in the PR)

- Remove `RTS_NAPI_DEBUG` logging from `crates/rts-napi/src/async_work.rs`
  (lines ~97-112).
- The fixed napi `.then` is already in `indirect.rs:839-863`
  (`lower_napi_instance_method_call`) — keep it; the new fix is Option A in the
  generic block of `calls/mod.rs`.

## Branch state

Branch `feat/napi-finish`, modified (uncommitted):
- `crates/rts-codegen/.../calls/indirect.rs` (local .then fix — already applied)
- `crates/rts-engine/src/heap/handles.rs` (trace_children PromiseAsync — keeps the
  resolved value alive in the GC)
- `crates/rts-engine/src/lib.rs` (rebuild touch — remove)
- `crates/rts-napi/src/async_work.rs` (RTS_NAPI_DEBUG — remove)

Issue #1548 does **NOT** close (the dev will put in a new engine later). PR is
`Refs #1548`, merge `--squash --delete-branch`.
