# Continue.md — handoff para a próxima sessão (`rts-codegen-new`)

> Estado em 2026-06-14. Branch **`feat/rts-codegen-new`** (tudo commitado + pushed).
> Motor velho (`rts-codegen-old`) **intocado e dirige o bin** até o cutover.
> Doc canônico: `docs/specs/rts-codegen-new-design.md`. Regras: `CLAUDE.md`.

---

## 1. Onde estamos

- **Harness cross-runtime (motor NOVO):** `77 rodam / 71 batem / 6 divergem` de 609.
  Medir: `cargo test -p rts-codegen-new -- --ignored fixture_harness --nocapture`.
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

**FALTA (próximos):**
- **F3c-opt** (sem gate) — omitir param OPCIONAL `x?` no call → passar `undefined`
  (em `marshal_call_args`; achar se HirParam marca `optional` separado de
  `has_default`). `Array.ts` usa `deleteCount?`.
- **F3c-default** (⚠️ GATE — toca rts-hir) — default params `= expr`: a EXPRESSÃO é
  descartada no `rts-hir/lower.rs:608` (só `has_default` sobrevive). Threading do
  expr no HIR + `FnSig.defaults` + preencher em `marshal_call_args`. rts-hir é
  compartilhado c/ motor velho → gate.
- **spread-into-call** — `f(x, ...arr)` / `this.#v.push(...items)` (packing/append
  no rest; `__rtsadp_arr_spread_append` existe).
- **BUG repr null-local** — `let a = null` guarda Float64 → ler de volta vira NaN/0
  (`console.log(a)` imprime `0`, devia `null`). Correção de solidez.
- **(menor)** index OOB de array devolve `0`, não `undefined`.

6. **infra `include()` + ns privada/load-order** (arquitetura, API do owner):
   - `engine.include(include_bytes!("map.ts"))` → classes ambientes.
   - `engine.ns(name, true)` → ns privada (símbolos só no ambiente interno do TS
     embarcado; não expostos no run final).
   - `engine.ns(name, number>1)` → preferência de ordem de carga (usar Maps/Objects
     dentro do ambiente engine). ⚠️ muda assinatura `.ns()` em rts-engine (builder
     compartilhado) → GATE. Adicionar SÓ junto do consumidor (senão dead-code).
7. escrever `rts-primitives/*.ts` + `rts-shared/*.ts` (Array/Map/Set wrappers, etc.)
   + Date via ns privada; **deletar** o Rust correspondente (`value/mapset.rs`,
   `class_meta` Map/Set, `__rtsadp_map_*/set_*`, `register_mapset_class_spec`). GATE.

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
