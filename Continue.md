# Continue.md — handoff para a próxima sessão (`rts-codegen-new`)

> Estado em 2026-06-14. Branch **`feat/rts-codegen-new`** (tudo commitado + pushed).
> Motor velho (`rts-codegen-old`) **intocado e dirige o bin** até o cutover.
> Doc canônico: `docs/specs/rts-codegen-new-design.md`. Regras: `CLAUDE.md`.

---

## 1. Onde estamos

- **Cross-runtime (motor NOVO):** `73 batem / 7 divergem / 511 bail` de 609 (18 bun≠node).
  - Harness interno: `cargo test -p rts-codegen-new -- --ignored fixture_harness --nocapture`.
  - **Script agora dirige o motor NOVO** via `rts run-new` (commit `ac2b1635`):
    `bash scripts/cross_runtime_check.sh` → 73/7/511. (Baseline do motor VELHO,
    congelado, era 419/137/36 — agora só alcançável trocando `run-new`→`run` no script.)
  - `rts run-new <file>` = motor novo (1 arquivo, sem imports); `rts run` = velho.
- **Unit tests:** `495 passam` (`cargo test -p rts-codegen-new`).
- **Motor velho: ZERO regressão** em toda a construção — provado em cada mudança de
  crate compartilhado pelo gate: TS suite `1710/1710` + cross-runtime baseline
  (`419 pass / 137 diverge / 36 errors`).
- O motor novo roda **TS real** end-to-end via `front::run::run_source(src) -> Result<String>`
  (swc parse → rts-hir → lowering → JIT → executa, contra o runtime REAL).

## 2. A REGRA ARQUITETURAL (binding — não viole)

**A linha divisória é SINTAXE NATIVA.**

- **Sintaxe nativa ⇒ PRIMITIVO ⇒ codegen-direto (impl em `rts-primitives`):**
  `""`(String), `123`(Number), `true/false`(Boolean), `[]`(Array), `{}`(Object),
  `function`/arrow(Function), **`/re/`(RegExp — tem sintaxe → é primitivo!)**,
  template literals, `Error`+subclasses. O motor NOMEIA e lowera a sintaxe direto.
- **Sem sintaxe nativa ⇒ lib utilitária `rts-shared` ⇒ Registry, INDIRETO:**
  `Date`, `Map`, `Set`, `WeakMap`, `JSON`, `URL`, `Math`(métodos), `Promise`,
  `Proxy`, typed-arrays, backend. **NUNCA reimplementar como tabela codegen
  `__rtsadp_*`** — despachar pela Registry real.
- **Codegen-native legítimo (o motor é dono da rep):** o modelo de valor (PolyValue),
  literais, os **operadores polimórficos** (`genops.rs`/`genops_arith.rs`), e os
  **ops de ELEMENTO de array sobre palavras PolyValue** (`arrayops.rs`/`arraycb.rs` —
  o runtime espera elemento i64 cru, o motor novo guarda PolyValue word, então
  `arr.map/filter/reduce/join/indexOf` etc. são codegen). Resultados-array (regex
  match/split que constroem array de palavras) também ficam codegen.

### Como despachar uma classe rts-shared (o caminho certo, já existe)

`front/run/registry.rs` constrói a Registry real (`rts_engine::Engine::new()` + as
`register`/`register_class_spec` fns via facade, cacheada em OnceLock). Consulte:
`class_member(class, method, argc)` / `class_ctor` / `class_static` / `class_getter` /
`instanceof_predicate` → `ResolvedCall { symbol, recv_abi, arg_abis, ret }`.
`front/run/registry_call.rs::emit_registry_call` é o **marshal genérico a partir dos
`AbiType`** (Handle→`POLY_TO_HANDLE`, StrPtr→ptr+len, F64/I64/Bool→escalar,
receiver=args[0], rebox por `ret`). **`Date` é a migração-referência** (`front/run/
dateclass.rs`, sem nenhum codegen Date-específico — `value/dateops.rs` foi DELETADO).
`rts-engine` é dep direto do crate novo (o owner liberou; sem impacto no motor velho).

## 3. DIREÇÃO NOVA (2026-06-14) — stdlib em TS, NÃO migração-Registry

**O plano antigo "migrar Map/Set p/ caminho Registry" foi SUPERADO pelo owner.**
Ver memória `project_ts_stdlib_direction`. Resumo da decisão:

- Só PRIMITIVOS com sintaxe nativa ficam codegen-direto (String/Number/Boolean/
  `[]`/`{}`/Function/`/re/`/Error/template) — seus `__rtsadp_*` são nativos-
  legítimos, FICAM. Tudo SEM sintaxe nativa (`Map`/`Set`/`Date`/wrappers) deve ser
  **escrito em `.ts` com class** (campo privado `#value` guarda o nativo) e
  compilado a Cranelift pela MESMA máquina de classes do código de usuário.
- **NÃO** criar arquivo de lowering por-classe (mapsetclass/dateclass), **NÃO**
  inventar marshal PolyValue-verbatim/flag/`AbiType::Poly`, **NÃO** mover PolyValue
  pro runtime. Comentários em **inglês**.
- Infra-alvo: `engine.include(include_bytes!("map.ts"))` (fontes TS embarcadas →
  classes ambientes) + `engine.ns("engine", true)` (ns privada p/ imports nativos).

### Roadmap faseado (derisk: classe Map-like pura-TS bailava na FUNDAÇÃO, não em Map)
Tudo `rts-codegen-new` (fora do bin) → sem gate, EXCETO onde marcado. Testes em
`front/run/tests/heap_field.rs`. 510 unit verdes ao fim do que está ✅.

1. **F1 ✅** (`5b2137aa`) — acesso encadeado sobre campo-array (`this.field[i]`
   r/w, `.length`, `.push/.pop`). `ClassDesc.field_arrays` + `resolve_heap_receiver`
   (Ident + `Member(this, campo-array)`) em `obj.rs`; `is_array_receiver` em
   `method_array.rs`.
2. **F2 ✅** (`83d5a715`) — private fields `#name` como slot normal (decl + acesso
   já carregam `#`; só remover o bail em `class/mod.rs`). Private MÉTODOS ainda bailam.
3. **F3a ✅** (`85da50b6`) — param array-tipado (`xs: number[]`) → `HeapShape::Array`
   (indexar/`.length`/`.push` em param). `let`-from-array-method já vinha de graça.
4. **F3b ✅** (`e257dcef`) — rest params `...items` (function/method/ctor) via helper
   `marshal_call_args` (em `call.rs`) + `FnSig.rest_param`. Spread-into-CALL ainda baila.
5. **F3d ✅** (`fdfdb712`) — `??` (short-circuit, testa nullish não-falsy).

**A (sem gate) ✅** — `7b155118` A3 (bug repr `let a=null`→Tagged) + `571b48f9` A2
(spread no rest param). A1-optional dobrado em C.

**C (params completos) ✅** — `8d0a4ffe` C1 (plumbing AST/HIR `optional`+`default_expr`,
GATE verde 1710 / 419·137·36) + `b0ae3a54` C2 (consumo: `FnSig.fillable`, repr Tagged
p/ fillable, callee-side default-fill `if undefined → default`, optional→undefined).
Params agora: fixed/rest/optional/default.

**B1 (prelude) ✅** — `91336f53`: `render_source_with_prelude(prelude, user)` +
`merge_programs` em `front/run/mod.rs`. Classe TS no prelude SOMBREIA a nativa
(shadowing automático). Teste `tests/prelude.rs`: `class Map` em TS puro substitui
o Map nativo (`9 2 true false 2`), instanceof/typeof. 524 unit verdes.

**B2 ✅** — `e6acc62c`: builder `ModuleBuilder.private()/.order(n)` + `Engine.include(src)`
+ campos `Module.private/load_order` + `Registry.includes`. `run_source`/`render_source`
mergeiam `registry::includes_prelude()` como prelude. Gate verde (1710 / 419·137·36).
Enforcement de ns-privada/ordem ainda plumbed-mas-deferido.

**B3 ✅ — Map/Set AGORA EM TYPESCRIPT** — `af57ce28` (paridade provada) + `58d3dd16`
(migração): `crates/rts-shared/src/stdlib/map_set.ts` (generic `Map<K,V>`/`Set<T>`,
campos-array privados, `===`, delete via shift+pop, `get size()`), embarcado via
`e.include(rts_runtime::stdlib::MAP_SET_TS)` em `build_registry`, compilado a Cranelift,
SOMBREIA+SUBSTITUI o nativo. DELETADO: `value/mapset.rs`, entries `__rtsadp_map_*/set_*`
+ `is_map/is_set` em runtime_link/wrappers, rows Map/Set em `class_meta`/globalclass,
chamada de `register_mapset_class_spec` (a fn FICA em rts-shared p/ motor velho).
Object-keys passaram a funcionar (melhoria). Harness **73/7** (matches +2). 537 unit.

### O TEMPLATE ESTÁ PRONTO (replicar p/ outras classes não-primitivas)
Para migrar mais (Date, futuros): escrever `.ts` em rts-shared/stdlib/, `e.include(...)`
em build_registry, deletar o nativo new-engine. Para classes que precisam de PRIMITIVOS
que TS não expressa (Date: epoch/tz), usar a ns PRIVADA (`engine.ns("engine", true)` —
flag já existe, falta o ENFORCEMENT de visibilidade no import resolver).

**Gaps p/ stdlib mais rica (sem gate, incrementais):**
- `Array.splice` (+ métodos que wrappers ergonômicos usem) — hoje os wrappers Map/Set
  evitam splice (delete via shift+pop).
- index OOB de array → `undefined` (hoje `0`).
- iterable-init (`new Map([[k,v]])`), `forEach`/iteração, Symbol.iterator — o nativo
  também bailava; precisam de suporte real (iteradores).
- enforcement ns-privada (filtrar símbolos no run final) + aplicação de load-order.
- default-param que referencia param anterior (callee-scope já cobre o caso comum).

## 4. Cauda longa de paridade (independente da direção stdlib-TS)

Por ROI (use o `bail_histogram` pra confirmar a cada passo — ver §6):
1. **static-member access** (`Class.staticField` em formas que ainda bailam — ~41 no
   histograma).
2. **dynamic class dispatch** — `inst.method()` quando a classe do receiver não é
   estática (hoje baila; precisa de tag de classe em runtime / shape-IC).
3. **generators** `function*`/`yield` (state machine) — categoria.
4. **Symbol + iterators** — `Symbol.iterator`, for-of sobre iterável custom.
5. **BigInt** `123n` — categoria.
6. **JSON** via Registry (`JSON.stringify` precisa de glue codegen p/ andar no shape;
   `parse` retorna rep — cuidado com impedância, como Date).
7. **Math** restante via Registry (intrinsics sqrt/abs/min/max ficam inline — design).
8. Resto da surface de builtins; depois **P5 cutover** (motor novo dirige o bin,
   `rts-codegen-old` deletado) quando paridade ≥ 70.7% real.

## 5. DÍVIDAS / BUGS ABERTOS (anotados, não bloqueantes)

- **`&&=`/`||=` com call-RHS** → "cannot coerce Float64 to Int64" (1 divergência).
- **`.at`/`charCodeAt` de surrogate solto** — pool UTF-8 não round-trip surrogate
  (o `charAt` do próprio runtime tem o mesmo limite).
- **Formatação numérica exótica** (divergências pré-existentes): `toFixed(1e21)`→`1e+21`,
  `Number.MIN_VALUE`→`5e-324` denormal, `console.log(-0)`→`-0`. Pode consertar em
  `rts-primitives` (owner liberou) — mas é 1 fixture cada, baixo ROI.
- **`e.stack`** do error-slot codegen não captura frame da fn que lançou (1 divergência).
- **Teste unit pré-existente quebrado (NÃO da minha sessão):**
  `rts-codegen-old::mir_codegen::tests::inline_routing_caller_callee_e2e` panica em
  cranelift-frontend FunctionBuilder (construção de IR). Não toquei mir_codegen; a
  suíte TS (mesmo JIT) passa 1710/1710. Flaky/stale no motor velho frozen.
- **`{name: string}` annotation de param** → rts-hir mapeia pra `Unknown` (não Object).
  `: object` funciona; o literal-object-type não. Fix seria em rts-parser/rts-hir.

## 6. COMO TRABALHAR (processo provado)

- **Modelo head+editor:** a sessão principal orquestra; despacha **agents** (Agent tool)
  pra implementar cada incremento (mantém o contexto principal leve). Cada incremento:
  1 feature, modular (**<500 linhas/arquivo — split em pasta se passar**), bail explícito
  (nunca valor errado — honesty floor), testes exact-stdout via `run_source`.
- **Medir ROI:** `cargo test -p rts-codegen-new -- --ignored bail_histogram --nocapture`
  (em `front/run/fixture_check/histogram.rs`) — histograma de motivos de bail nos 609
  fixtures. **Atacar os clusters maiores** (foi assim que 14→71 aconteceu).
- **GATE obrigatório ao tocar crate compartilhado** (rts-hir/mir/primitives/shared/engine
  — o motor velho usa todos): `cargo build --release` → `target/release/rts.exe test`
  (TS deve ficar 1710/1710) → `bash scripts/cross_runtime_check.sh` (deve bater o baseline
  419/137/36). Adicionar DEP ao rts-codegen-new NÃO precisa de gate (não entra no bin).
- **Permissão do owner:** PODE mexer em rts-hir/rts-mir/rts-primitives/rts-shared/rts-engine
  quando necessário (com o gate). Aditivo + não-quebrar o formato de registro
  (`Engine::ns/class/member/func/Sig/done` + `__RTS_FN_*`).
- **Commits:** conventional, branch-only, **NÃO mesclar na main nem abrir PR** até o
  cutover (política do owner). Terminar mensagem com `Co-Authored-By: Claude Opus 4.8
  (1M context) <noreply@anthropic.com>`.
- **Comunicação:** português. **NÃO usar codegen `__rtsadp_*` pra classe rts-shared** —
  é o erro que essa regra corrige; usar a Registry.

## 7. MAPA DE ARQUIVOS (`crates/rts-codegen-new/src/`)

- `value/` — PolyValue (NaN-box) + adapters: `value/{mod,emit,layout}.rs` (modelo),
  `abi_adapter.rs`/`emit_marshal.rs`/`abi_sig.rs` (marshal PolyValue↔ABI),
  `genops.rs`/`genops_arith.rs` (ops polimórficas), `arrayops.rs`/`arraycb.rs` (array
  elementos+callbacks), `objops.rs` (property dinâmica), `errslot.rs` (try/catch),
  `funcops.rs` (function values), `inspect.rs` (console.log), `mapset.rs`/`wrappers.rs`/
  `regexops.rs` (← migrar Map/Set; RegExp/Error ficam), `dyndispatch.rs`,
  `iterops.rs`, `dateops.rs` JÁ DELETADO.
- `front/run/` — o lowering do programa: `mod.rs`/`lower.rs`/`stmt.rs`/`expr.rs`/
  `binop.rs`/`call.rs`/`method.rs`/`obj.rs`/`loops.rs`/`newexpr.rs`/`assign.rs`/
  `trycatch.rs`/`toprimitive.rs`; `class/` (classes), `desugar/` (template/optchain/
  destructure/objmethod via AST re-read), `registry.rs`+`registry_call.rs`+`dateclass.rs`
  (Registry-driven), `globalclass.rs` (← tabela a esvaziar p/ Map/Set),
  `fixture_check/` (harness+histograma), `tests/` (todos os testes por tópico).
- `front/hir_lower/` — o subset numérico inicial (P1.5); `repr.rs` (lattice),
  `dispatch.rs` (tabela String/Number — pode migrar p/ Registry depois),
  `runtime_link.rs`+`registry_link.rs` (símbolos JIT).

## 8. RESUMO DO QUE JÁ FUNCIONA

numérico/strings/objetos/arrays(shapes) · métodos String/Number/Array (+dynamic dispatch)
· function values + callbacks + **closures** · classes + herança/getters/static · Map/Set/
Error + instanceof + extends Error · Math/Object/Number statics · globais (NaN/Infinity/
parseInt/...) · ops polimórficas + untyped · precisão de op (`===`/`!`/`**`/spread/
logical-assign) · template literals · optional chaining · destructuring · for-of/for/
for-in · try/catch/throw · RegExp · ToPrimitive (toString/valueOf) · object-literal methods
· **Date via Registry**. Achei+corrigi 3 bugs de solidez reais no caminho.
