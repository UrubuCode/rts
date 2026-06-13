# Estudo — crash de `.then()` encadeado em Promise N-API (#1548)

> Documento de diagnóstico. O bug está **isolado e a causa raiz validada por
> experimento**. Este arquivo é o contexto completo para a correção.

## Sintoma

```ts
import a from "./pstr.node";
a.makeP().then((s: string) => console.log("got: " + s));  // CRASH ILLEGAL_INSTRUCTION
```

`a.makeP()` cria uma Promise N-API (via `napi_create_promise` +
`napi_resolve_deferred`, síncrono — a Promise já volta **fulfilled**). O
`.then(cb)` **encadeado direto** sobre o resultado de `makeP()` crasha com
`ILLEGAL_INSTRUCTION (0xC000001D)` — pulo para fn ptr inválido.

O addon de teste (`/tmp/napitest/add_addon/src/lib.rs`) é mínimo:
```rust
unsafe extern "C" fn make_p(e:E,_info:I)->V {
    let mut def=ptr::null_mut(); let mut p=V(ptr::null_mut());
    napi_create_promise(e,&mut def,&mut p);
    let s=b"hello_string_result";
    let mut val=V(ptr::null_mut()); napi_create_string_utf8(e,s.as_ptr() as *const c_char,s.len(),&mut val);
    napi_resolve_deferred(e,def,val);   // resolve SÍNCRONO, dentro de makeP()
    p
}
```

## Fato decisivo (experimento que isola a causa)

| Forma TS | Resultado |
|---|---|
| `a.makeP().then(cb)` (encadeado) | **CRASH** (número 0-arg, string 1-arg, string 0-arg — TODOS) |
| `const p = a.makeP(); p.then(cb)` (Promise em LOCAL) | **FUNCIONA** (`LOCAL got: hello_string_result`, número 0-arg ok) |
| `Promise.resolve("x").then(cb)` (NATIVO) | **FUNCIONA** |

A ÚNICA diferença entre as duas primeiras linhas é o **receiver do `.then`**:
- encadeado → receiver é uma `Expr::Call` (`a.makeP()`)
- local → receiver é um `Expr::Ident` (`p`)

Logo a representação da Promise napi está **correta** (`Entry::PromiseAsync`,
idêntica à nativa — ambos subsistemas `__RTS_FN_NS_PROMISE_*` e
`__RTS_FN_GL_PROMISE_*` usam o mesmo `Entry::PromiseAsync(promise_slot)`). O bug
é puramente de **roteamento de codegen** do `.then` quando o receiver não é um
ident local.

## Por que o caminho LOCAL funciona

`const p = a.makeP(); p.then(cb)`:
- `p` é local Handle/I64 e há addon nativo no programa →
  `calls/mod.rs:808-822` chama
  `indirect::lower_napi_instance_method_call(ctx, "p", "then", call)`.
- Essa função (indirect.rs:813) tem o fast-path **já corrigido** (linhas
  839-863): para `then/catch/finally`, lower o callback com
  `lower_expr + coerce_to_i64` (idêntico ao `.then` nativo, deixa o codegen de
  `__hoisted_arrow_` escolher handle-typed-para-number vs func_addr-cru-para-
  string/0-arg) e chama `__RTS_FN_GL_PROMISE_THEN`/`THEN2`/`CATCH`/`FINALLY`.
- `GL_PROMISE_THEN2` (`globals/fetch/instance.rs:167`) faz `classify` →
  `PromiseKind::Async(arc)` → fast-path settled → `enqueue_microtask_settled`.
  Drena correto.

## Por que o caminho ENCADEADO crasha

`a.makeP().then(cb)`:
- `a.makeP()` é lowered por `lower_native_addon_method_call`
  (indirect.rs:732) que retorna **`TypedVal::new(result, ValTy::I64)`**
  (indirect.rs:803) — handle de Promise tipado como **I64 cru**.
- O `.then` tem receiver `Expr::Call`, NÃO `Expr::Ident`, então
  `qualified_member_name(callee)` é `None` e o ramo napi-instance
  (calls/mod.rs:799-822) **nunca dispara** (exige `qualified.split_once('.')`
  com obj ident).
- Cai no bloco genérico de member-method (calls/mod.rs:1197+,
  `!matches!(m.obj, Expr::Ident)`):
  - linha 1246: `recv_tv.ty == ValTy::I64` →
    `try_global_class_instance_method(ctx, "Number", "then", ...)`.
    `Number.prototype.then` não existe no Registry → `None`.
  - linha 1258: `ValTy::Handle` NÃO casa (é I64) → pula o bloco Handle.
  - cai no **fallback genérico** (string_builtin / `MAP_GET("then")` + invoke),
    que reifica o callback como handle e o invoca com convenção errada →
    **ILLEGAL_INSTRUCTION**.

Confirmado no IR (`rts ir --allow-native-addons /tmp/napitest/ptnum.ts`): no
nativo o `.then` recebe `func_addr` cru; no napi encadeado aparece
`call fnX(handle_slot, func_addr)` (reify) + `.then(promise, handle_reificado)`
— callback reificado errado.

## A correção (o que o agente deve implementar)

Rotear `<expr>.then/catch/finally(cb)` para `__RTS_FN_GL_PROMISE_THEN*`
**também quando o receiver é uma Promise napi resultante de uma CallExpr de
addon nativo** (não só quando é local).

Opções (escolher a mais limpa e cirúrgica, sem regressão):

### Opção A (preferida) — detectar no bloco genérico
No bloco `!matches!(m.obj, Expr::Ident)` de `calls/mod.rs` (~1197), ANTES dos
ramos Number/Handle (~1239-1257), adicionar:

```rust
// (N-API) <expr>.then/catch/finally onde <expr> pode ser uma Promise napi
// (resultado de addon nativo ou qualquer Handle/I64 com addon no programa).
// Roteia para GL_PROMISE_THEN* — que detecta PromiseAsync em runtime via
// classify() e enfileira o microtask. Sem isto o .then encadeado cai no
// fallback MAP_GET e crasha (callback reificado com convenção errada).
// O receiver é coerced a i64 (handle). GL_PROMISE_THEN2 com NotPromise é
// passthrough seguro (retorna o próprio handle), então é benigno se não for
// Promise — mas restringir a `any_native_addon()` evita afetar outros casos.
if matches!(method_name.as_str(), "then" | "catch" | "finally")
    && !call.args.is_empty()
    && crate::codegen::lower::passes::native_addon::any_native_addon()
    && matches!(recv_tv.ty, ValTy::I64 | ValTy::Handle)
{
    let recv = ctx.coerce_to_i64(recv_tv).val;
    // callback EXATAMENTE como o .then nativo (lower_expr + coerce_to_i64) —
    // NÃO lower_callable_target_h (que reifica handle is_arrow=0/rk=void e
    // crasha p/ 0-arg e string). Idêntico a lower_napi_instance_method_call.
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

**Cuidado:** `recv_tv` já foi consumido por `lower_expr(ctx, &m.obj)` na linha
1198. Reusar `recv_tv` (não re-lower o m.obj — senão makeP() roda 2x e o
deferred é registrado duas vezes). Verificar a posição exata: inserir DEPOIS de
`let mut recv_tv = lower_expr(ctx, &m.obj)?;` (1198) e do bloco de re-tipagem
mapcall (1223-1235), mas ANTES do ramo Number (1246). Coercer `recv_tv` a i64
consome-o; como os ramos seguintes também consomem, garantir o `return` no
match (não cair adiante).

**Restrição `any_native_addon()`**: evita que esse ramo intercepte `.then` de
expressões em programas SEM addon (onde o caminho nativo já funciona via
type-tag de Promise). Com addon presente, é seguro porque `GL_PROMISE_THEN2`
trata `NotPromise` como passthrough (retorna o handle sem invocar callback) — mas
ainda assim restringir minimiza superfície.

### Opção B — re-tipar o retorno de makeP como Handle
Menos cirúrgico; `lower_native_addon_method_call` não sabe se o retorno é uma
Promise (só sabe em runtime). Descartada — exigiria type-flow que não existe.

## Verificação obrigatória pós-fix

Testes que DEVEM passar (todos em `/tmp/napitest/`):
```
target/release/rts.exe run --allow-native-addons /tmp/napitest/ptnum.ts
  → "num then ran, no value"
target/release/rts.exe run --allow-native-addons /tmp/napitest/pstr.ts
  → "got: hello_string_result"
target/release/rts.exe run --allow-native-addons /tmp/napitest/pstr2.ts
  → "then ran (no value used)"
```
Não-regressão (devem continuar funcionando):
```
/tmp/napitest/ptnum_local.ts, pstr_local.ts  (local var — já ok)
/tmp/napitest/nat0num.ts, nat0str.ts, natstr.ts  (nativo — já ok)
```
Suite:
```
cargo build --release   (DOIS passos: -p rts-runtime primeiro se for tocar AOT)
cargo test --release --lib
target/release/rts.exe test    (esperado 1710/1710 — .then nativo não pode regredir)
```

## Build gotcha conhecido

`cargo build --release --bin rts` às vezes NÃO recompila a cadeia
(rts-codegen → bin) reportando "Finished" sem rebuild. Forçar tocando um arquivo
da cadeia (ex.: append `// rl <epoch>` a `crates/rts-runtime/src/lib.rs`,
buildar, remover). Sempre conferir o mtime de `target/release/rts.exe` após o
build e re-rodar os testes napi.

## Limpeza pendente (não esquecer no PR)

- Remover logging `RTS_NAPI_DEBUG` de `crates/rts-napi/src/async_work.rs`
  (linhas ~97-112).
- O `.then` napi corrigido já está em `indirect.rs:839-863`
  (`lower_napi_instance_method_call`) — manter; a correção nova é a Opção A no
  bloco genérico de `calls/mod.rs`.

## Estado do branch

Branch `feat/napi-finish`, modificados (uncommitted):
- `crates/rts-codegen/.../calls/indirect.rs` (.then fix local — já aplicado)
- `crates/rts-engine/src/heap/handles.rs` (trace_children PromiseAsync — mantém
  valor resolvido vivo no GC)
- `crates/rts-engine/src/lib.rs` (toque de rebuild — remover)
- `crates/rts-napi/src/async_work.rs` (RTS_NAPI_DEBUG — remover)

Issue #1548 **NÃO fecha** (o dev vai colocar um motor novo depois). PR é
`Refs #1548`, merge `--squash --delete-branch`.
