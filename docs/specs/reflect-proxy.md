# Reflect + Proxy

Issue: #218. Spec do design e limitacoes da implementacao em RTS.

## Reflect API (13 metodos)

Codegen detecta `Reflect.<method>(...)` em `lower_call` e despacha
estaticamente. Nao existe `Reflect` como object handle runtime — eh
apenas um namespace de dispatch direto.

| Metodo | Backing |
|---|---|
| `Reflect.get(obj, key)` | `__RTS_FN_NS_COLLECTIONS_MAP_GET` (proxy-aware) |
| `Reflect.set(obj, key, val)` | `__RTS_FN_NS_COLLECTIONS_MAP_SET` (proxy-aware) — sempre retorna `true` |
| `Reflect.has(obj, key)` | `__RTS_FN_NS_COLLECTIONS_MAP_HAS` (proxy-aware) |
| `Reflect.deleteProperty(obj, key)` | `__RTS_FN_NS_COLLECTIONS_MAP_DELETE` — JS spec: sempre `true` em RTS v0 |
| `Reflect.ownKeys(obj)` | `__RTS_FN_NS_COLLECTIONS_MAP_KEYS` (proxy-aware) |
| `Reflect.getPrototypeOf(obj)` | `__RTS_FN_NS_COLLECTIONS_MAP_GET_PROTO` (proxy-aware) |
| `Reflect.setPrototypeOf(obj, proto)` | `__RTS_FN_GL_REFLECT_SET_PROTOTYPE_OF` (proxy-aware) |
| `Reflect.isExtensible(obj)` | stub — sempre `true` (RTS v0 sem freeze tracking) |
| `Reflect.preventExtensions(obj)` | stub — no-op, retorna `true` |
| `Reflect.apply(fn, this, args)` | `__RTS_FN_GL_FUNCTION_APPLY` |
| `Reflect.construct(T, args)` | `__RTS_FN_GL_REFLECT_CONSTRUCT` (proxy-aware) |
| `Reflect.getOwnPropertyDescriptor(obj, key)` | `__RTS_FN_GL_REFLECT_GET_OWN_PROPERTY_DESCRIPTOR_PROXY` |
| `Reflect.defineProperty(obj, key, desc)` | `__RTS_FN_GL_REFLECT_DEFINE_PROPERTY_PROXY` |

Descriptors sao Map sintetizados: `{ value, writable: true, enumerable:
true, configurable: true }`. RTS v0 nao tem metadata por slot (todo slot
eh writable/enumerable/configurable). Accessor descriptors com `get`/
`set` aceitos como Map mas executados como `value` no slot.

## Proxy (13 traps)

Construtor `new Proxy(target, handler)` aloca `Entry::Proxy { target,
handler }` em `crates/rts-runtime/src/namespaces/gc/handles.rs`. Ponto
unico de despache: cada operacao MAP_* checa `resolve_proxy(handle)` no
inicio e, se for Proxy, delega pra `globals/proxy/ops::dispatch_*`.

### Traps implementadas

| Trap | Onde dispara | Forward (sem trap) |
|---|---|---|
| `get(target, prop)` | `obj.x`, `obj["x"]`, `Reflect.get` | `MAP_GET_CHAIN(target, prop)` |
| `set(target, prop, val)` | `obj.x = v`, `Reflect.set` | `MAP_SET(target, prop, val)` |
| `has(target, prop)` | `Reflect.has` (e futuramente `in`) | `MAP_HAS(target, prop)` |
| `deleteProperty(target, prop)` | `Reflect.deleteProperty` | `MAP_DELETE(target, prop)` |
| `ownKeys(target)` | `Object.keys`, `Reflect.ownKeys`, `for...in` | `MAP_KEYS(target)` |
| `apply(target, this, args)` | `proxy(args)`, `Reflect.apply` | `FUNCTION_APPLY(target, this, args)` |
| `construct(target, args)` | `Reflect.construct` | aloca Map vazio + `FUNCTION_APPLY(target, inst, args)` |
| `getPrototypeOf(target)` | `Reflect.getPrototypeOf` | `MAP_GET_PROTO(target)` |
| `setPrototypeOf(target, proto)` | `Reflect.setPrototypeOf` | `MAP_SET(target, "__proto__", proto)` |
| `defineProperty(target, key, desc)` | `Reflect.defineProperty` | extrai `desc.value` + `MAP_SET` |
| `getOwnPropertyDescriptor(target, key)` | `Reflect.getOwnPropertyDescriptor` | sintetiza `{value, writable: true, ...}` |
| `isExtensible(target)` | (nao chamada — codegen direto retorna true) | stub |
| `preventExtensions(target)` | (nao chamada — codegen direto retorna true) | stub |

### Dispatch end-to-end

```
user TS:                obj.x
codegen:                map_get_static(obj_handle, "x")
runtime MAP_GET_CHAIN:  resolve_proxy(handle)?
                          Some(target, handler) -> dispatch_get(target, handler, "x")
                          None                    -> walk Map chain
proxy::dispatch_get:    lookup_trap(handler, "get")?
                          Some(trap_fn) -> INVOKE_AUTO(trap_fn, 0, [target, key_h])
                          None          -> MAP_GET_CHAIN(target, "x")  // forward direto
```

Re-entrancia: `dispatch_get` chama trap via `INVOKE_AUTO`, que pode chamar
de volta `MAP_GET` etc. Para evitar loop infinito quando a trap acessa
target, o forward usa `target` (nao o proxy) — `target` nao eh Proxy
entao o caminho default executa.

### Sentinel collision (`__get_<key>` / `__set_<key>`)

Codegen historico tem getters/setters dinamicos via slots
`__get_<key>` / `__set_<key>` armazenados como handle Function. O
member access checa esse sentinel **antes** do MAP_GET normal. Em
Proxy, isso causaria recursao + access violation (trap retorna handle
de string, codegen interpreta como fn ptr).

Fix: criada `__RTS_FN_NS_COLLECTIONS_MAP_GET_DIRECT(handle, key)` que
NAO faz dispatch de Proxy. Codegen chama essa em vez de MAP_GET ao
checar sentinels. Resultado: getters/setters dinamicos continuam
funcionando em Map normal sem disparar trap em Proxy.

## Limitacoes

- **Mutable closure em trap**: `(t,k) => { count++; return ... }` nao
  persiste a mutacao quando invocada via re-entrant `INVOKE_AUTO`
  (issue #195). Workaround: trap retorna valor calculado puro.
- **Trap recebendo `k: any`**: passar `k` (handle) pra `Reflect.get(t, k)`
  dentro do trap nao funciona — Reflect.* esperam StrPtr literal.
  Workaround: usar valor fixo no trap.
- **Proxy callable como param fn**: passar Proxy callable pra fn user
  com `p: any` e fazer `p()` la dentro crasha (codegen nao tem info
  de tipo pra dispatch). Path direto `p()` no escopo onde foi criado
  funciona.
- **Invariants do spec ECMA**: `defineProperty` rejeitando
  `configurable: false` override, `setPrototypeOf` falhando em
  non-extensible, etc. — nao implementadas.
- **Accessor descriptors reais**: `get`/`set` no descriptor sao
  aceitos como Map mas nao instalados como getters/setters do slot.

## Cobertura de testes

- `tests/reflect_api.test.ts` — 15 happy-path
- `tests/reflect_edge_cases.test.ts` — 32 atipicos (loop, falsy, try/catch,
  ref mutation, ternario, churn, ownKeys preservation)
- `tests/reflect_with_classes.test.ts` — 29 (heranca, observer pattern,
  defineProperty serial, merge via Reflect)
- `tests/proxy_phase1.test.ts` — 20 (4 traps basicas + forward + isolacao)
- `tests/proxy_phase2.test.ts` — 12 (ownKeys/apply/construct/getProto)
- `tests/proxy_phase2_extreme.test.ts` — 24 (multi-trap, ownKeys 50 keys,
  loop apply, var-args ctor, proto chain)
- `tests/proxy_wild.test.ts` — 24 (proxy de proxy, em prop, shared, tap)
- `tests/proxy_phase3.test.ts` — 20 (setProto/defineProperty/getOwnDesc
  trap + forward + reject + regressao em Map normal)
- `tests/proxy_phase3_extreme.test.ts` — 19 (proto chain, multi-trap,
  round-trip, value=0 falsy preserved)

Total: **195 testes** dedicados a Reflect + Proxy.
