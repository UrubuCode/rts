# Epic #226 — Paridade JS/TS

> **O QUE ESTE DOC É (nota 2026-07-28):** um **catálogo das SEMÂNTICAS JS/TS**
> que o motor precisa cobrir — as ~60 APIs de Array/Object/Math/String/URL/Date/
> Boolean/parseInt/destructuring, com os casos de borda de cada uma. É essa
> lista que continua útil.
>
> **O QUE ELE NÃO É:** um relatório de status. Todo número, contagem de teste e
> lista de PR aqui era do **MOTOR VELHO** (deletado no cutover P5) e foi
> REMOVIDO desta revisão — citá-los viola a regra "o número de paridade é real,
> sempre re-meça". Para o estado atual: `.github/cross_runtime_report.json` /
> `scripts/measure_new.sh`. Para o caminho de implementação: o modelo
> PolyValue/shapes de `rts-codegen-new-design.md`, não a ordem de PRs abaixo.

## Fechamento dos gaps maiores

As categorias abaixo foram fechadas no motor velho; no motor atual elas são a
ESPECIFICAÇÃO do que precisa existir, reimplementada sobre o modelo de valor
novo. Cada método não-primordial resolve pelo Registry/`MethodSpec` — nenhum
builtin dentro do motor.

### Module system completo (#213/#618/#619)

- **#213** AOT module graph: `compile_file` agora chama
  `ModuleGraph::load + flatten_for_jit` antes de emitir object. AOT
  e JIT compartilham flatten (uma feature so' precisa ser tratada uma
  vez). Antes AOT virava no-op de ~765 bytes pra qualquer programa
  com import relativo.
- **#619** Alias em `import { x as y }` e `export { x as y } from`:
  `ImportName { orig, local }` substitui `Vec<String>`. `local_alias_map`
  (local -> orig) consultado por `FnCtx::resolve_alias` antes de
  user_fns lookup.
- **#618** `export * as ns from "./mod"`: novo `Item::ExportNamespace`
  no AST. Flatten resolve key do source via ResolvedImport e enumera
  exports pra registrar `ns.<exp>` -> `<exp>` em `local_alias_map`
  com chave dotted.
- **#617** AOT classes: `runtime_support.a` (build.rs) agora inclui
  `globals/function/ops.rs` — sem isso `__RTS_FN_RT_INVOKE_AUTO` ficava
  undefined e qualquer classe user falhava no link AOT.

### Reflect API + Proxy (#218)

Ver `reflect-proxy.md` para spec completa.

- **Reflect**: 13 metodos estaticos. 9 reusam dispatch direto pra
  `collections.map_*`; 4 novos (apply/construct/getOwnPropertyDescriptor/
  defineProperty). 76 testes de cobertura.
- **Proxy fase 1**: get/set/has/deleteProperty traps + forward
  automatico. `Entry::Proxy { target, handler }` no GC.
- **Proxy fase 2**: ownKeys/apply/construct/getPrototypeOf. Hooks em
  MAP_KEYS/INVOKE_AUTO/FUNCTION_CALL/MAP_GET_PROTO.
- **Proxy fase 3**: setPrototypeOf/defineProperty/getOwnPropertyDescriptor.

Total: 119 testes dedicados a Reflect + Proxy (`tests/reflect_*.test.ts`,
`tests/proxy_*.test.ts`).

### Bugs de codegen corrigidos

- **#383** Ident desconhecido vira erro de compilacao (era warning +
  segfault). `compile_main` trata `undefined variable`/`unknown namespace
  member`/`undeclared user function` como hard-fail.
- **#584** Operador `/` segue JS spec (sempre f64). Antes `1/3 = 0`,
  `7/2 = 3`. Fix em duas camadas: `lower_div` (AST) promove para f64,
  `lower_bin` (MIR) emite `CvtFromSint` antes de `FDiv`.
- **#592** `users[i].field` em `Cls[]` propaga tipo. Tres sitios:
  `local_array_class_ty` em decls.rs, members.rs, user_fn.rs param.
- **#602** Optchain 3+ niveis em obj literal. Decls.rs recursa em
  sub-objs e popula `local_nested_obj_field_types[(root, sub_key)]`
  com tipos das folhas.
- **#573** `console.log(null)` -> "null", U64 ambiguo (JSON.parse
  scalar, WeakMap.get) via TPL_COERCE_AUTO. Codegen lower (basics.rs +
  builtins.rs) trata `ValTy::U64` igual `Handle` no path COERCE_AUTO.
- **#261** Computed key `obj["x"]` propaga field type igual `obj.x`.
  Members.rs faz lookup em `local_obj_field_types`/`global_obj_field_types`
  para Lit::Str. Para `obj[k]` variavel, aplica TPL_COERCE_AUTO no
  resultado pos-MAP_GET.

### Features novas

- **#450** `arguments.length` em fn user nao-arrow. Pass detecta uso
  textual no body e injeta Vec com params no prologue.
- **#287** `node:fs.readFileSync` funcional. Nova fn `__RTS_FN_NS_FS_READ_TEXT`
  que retorna string handle (READ_ALL antigo era byte-buffer com sig
  incompativel).
- **#224** UI event loop primitives: `app_check`, `app_add_timeout`,
  `app_repeat_timeout`, `app_add_idle`. Wrappers diretos sobre
  `fltk::app::*`.

### Issues fechadas por verificacao (ja funcionavam)

#260 (function expressions/IIFE), #255 (destructuring), #302 (circular
imports), #482 (prototype chain), #478 (Map/Set/WeakMap), #479 (Object
builtins), #476 (Array native methods), #433 (intersection types), #288
(node:path/os/process/util).

### Bonus de sessao

- Bug pre-existente em arrow VarDecl (`const arr = (a,b) => a+b`
  retornando 0) descoberto durante #450 e corrigido junto. Heuristica
  `body_has_return_value` aplicada em `try_lower_fn_expr_decl`.
- `tests/net_udp_echo.test.ts` flake corrigido (porta 51238 ocupada
  -> 9123 universal).

## Lote anterior — PRs #483-#547

Aproximadamente 60 APIs JS adicionadas/corrigidas:

### Array
indexOf/lastIndexOf/includes com fromIndex; reverse; shift/unshift;
slice; concat; fill; flat; flatMap; splice; findLast/findLastIndex;
reduceRight; copyWithin; sort com deteccao de strings; values/keys/
entries; toSorted/toReversed/toSpliced/with; Array.from(length) e
Array.from(arr).

### Object
entries; assign; freeze; fromEntries; seal; isFrozen; isSealed;
getPrototypeOf; defineProperty. keys/values/entries excluem
`__proto__`.

### Math
sign; hypot; expm1; log1p; fround; sinh/cosh/tanh/asinh/acosh/
atanh; imul; clz32. Constantes SQRT2/SQRT1_2/LN2/LN10/LOG2E/
LOG10E. Aliases abs/min/max/log/random.

### String
split com limit; startsWith/endsWith com offset; match/search/
matchAll via backend regex.

### Symbol
Novo namespace `globals/symbol/`. Symbol(desc), Symbol.for(key),
keyFor, description, toString. Well-known: iterator,
asyncIterator, hasInstance, toPrimitive, toStringTag.

### URL
URLSearchParams completo (get/set/has/delete/append/getAll/keys/
values/entries/toString) + URL.searchParams getter.

### Date
setFullYear/Month/Date/Hours/Minutes/Seconds + variantes UTC;
getTimezoneOffset; toUTCString/toDateString/toJSON/toLocaleString/
toTimeString.

### Outros
TextEncoder/TextDecoder (fix critico de OOM 2.4TB em PR #485 —
GlobalClassSpec apontava para fns sem self param);
encodeURIComponent/decodeURIComponent; WeakMap/WeakSet (semantica
strong por enquanto, #217 rastreia weak real); Boolean class
(toString/valueOf); parseInt(s, radix) com semantica JS-spec.

### Destructuring (#210)
Array/objeto, defaults, rest, aninhado, em params de fn/arrow,
em for-of, em catch, alias `{a: b}`.

## Issues abertas pesadas (fora de escopo de PR pequena)

| # | Tema | Bloqueio |
|---|---|---|
| #195 | Mutable closures | Env-record refactor; depende de #90 |
| #207 | Event loop async/await real | Promise refactor |
| #216 | Symbol como chave computada | Side-channel HashMap por objeto |
| #217 | WeakMap/WeakSet weak real + FinalizationRegistry | GC refactor |
| #222 | Map/Set Symbol.iterator real | Hoje so' stub |
| #223 | Dynamic import | Module resolver async |
| #284 | UI callbacks com use-after-free pos hot reload | Sentinel + reset coordenado |
| #301 | var hoisting em fn user | Conflito hoist + var assign — refator declare_local |
| #304 | toString/valueOf em coercao implicita | Obj literal precisa reify fn como Function handle |
| #305 | Integer overflow JS spec (i64 wraparound vs f64) | Refator todos os ops aritmeticos |
| #411 | Async/await thread-per-fn modelo Node | Promise + event loop |
| #419 | GC roots completos | Refator scanner |
| #477 | Generator infinito | State machine real |
| #211/#219/#225 | Generators / BigInt / Intl | Candidate-discard |

## Cobertura atual

**Não está aqui, de propósito.** Qualquer número escrito neste arquivo estaria
desatualizado no dia seguinte e seria do motor errado. Meça:

```bash
bash scripts/measure_new.sh          # histograma pass/bail por arquivo
cat .github/cross_runtime_report.json # paridade cross-runtime (badge automático)
```
