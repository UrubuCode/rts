# Handoff — levar o MOTOR NOVO a 100% do `rts:test` (cutover)

> Branch: `feat/rts-codegen-new`. Idioma: pt-BR. Leia ANTES: `CLAUDE.md` +
> `.claude/rules/00-meta.md`…`05-codegen-notes.md` + `prompt.md` (handoff da
> migração de primitivos → `.ts`) + `docs/specs/rts-codegen-new-design.md`
> (canônico). Este arquivo é o handoff da CAMPANHA DE CUTOVER (rodar a suíte
> `rts:test` inteira pelo motor novo p/ deletar o `rts-codegen-old`).

---

## 1. Objetivo e regra de ouro

**Meta:** o motor NOVO (`crates/rts-codegen-new`, rodado por `rts run-new <file>`)
passar TODOS os ~1710 testes / 630 arquivos `tests/*.test.ts` (a suíte `rts:test`
que hoje roda no motor VELHO via `rts test`). Quando o novo ≥ velho → deletar
`rts-codegen-old` (design doc P5, cutover).

**Honesty floor (NUNCA afrouxa):** nada que crasha/trava commitado como "pass";
sem deletar/hardcodar fixture p/ inflar número; build sempre compila. Um arquivo
"passa" só quando produz a saída correta pelo MESMO caminho que qualquer input
usaria. Regressão só explícita + justificada no commit.

---

## 2. O LOOP de trabalho (medir → atacar cluster → medir)

1. **Medir:** `bash measure_new.sh` (na raiz). Roda cada `tests/*.test.ts` pelo
   `rts run-new`, conta exit-0 (rodou sem bail) vs bailou, e imprime o
   **histograma de clusters de falha** (1ª falha por arquivo, normalizada). Saída
   detalhada em `/tmp/new_fail.txt` (arquivo→razão) e `/tmp/new_pass.txt`.
2. **Atacar o MAIOR cluster** (maior alavanca). Cada cluster é uma feature/gate.
3. **Re-medir** — o cluster resolvido revela o PRÓXIMO (os arquivos avançam pro
   obstáculo seguinte). 0/630 *completando* é esperado até o framework rts:test
   rodar (ver §4).

Precisa do binário release atualizado: `cargo build --release -p rts-runtime &&
cargo build --release` antes de medir.

---

## 3. Estado atual (o que já foi feito nesta campanha)

**Baseline de cobertura (medido via `measure_new.sh`):** **288/630 rodam sem bail
(exit 0)**, dos quais **269 100% VERDES** (asserções corretas) + ~19 com divergência
real exposta. Antes do framework: 0/630. ATENÇÃO: "exit 0" = rodou sem bail
(cobertura), NÃO = asserção passou — um `expect` que falha imprime ✗ e ainda sai 0.
Número honesto = **235 verdes**. Pass definitivo = `rts test` no cutover (P5).

**Nota honestidade (crashes no FAIL bucket):** ~15-20 arquivos do FAIL bucket
CRASHAM (exit 132/139) em vez de bailar limpo — NÃO são contados como verde (exit≠0
→ fora do pass-list), então NÃO inflam a métrica. Dois tipos: (a) bugs de codegen
pré-existentes em features (nullish_optional, object_method_this_binding,
for_in_values stack-overflow, arrow_lift_return); (b) chamadas de namespace
unsafe/async (ptr.copy/ffi/promise/net/sync) que LINKAM e rodam mas crasham por
semântica (ex.: ptr.copy com endereço errado) — o §7 (link-OK/runtime-SIGILL de
símbolo NÃO-linkado) está OK (símbolos linkados); são gaps de marshaling/feature
por-arquivo. `ptr` p.ex. tem 3 arquivos VERDES, então não dá pra remover o
namespace. Limpar cada crash = trabalho por-arquivo/feature.

### Triagem dos clusters restantes (histograma `measure_new.sh`)
MECÂNICOS JÁ DRENADOS nesta campanha: framework, top-let capturado→cell,
namespaces std, Float64→Int, símbolos gc. O QUE SOBRA é majoritariamente
FEATURE-GRANDE (cada uma um épico), não correção pontual:
- `unbound identifier` (48): globais não-fiados (JSON/Reflect/Symbol/Promise/
  console-as-value/performance/URL/BigInt) + idents órfãos de closures #195 que
  bailaram. JSON.stringify/parse precisa SERIALIZAR objetos shape/slot do motor
  novo (não trivial — o ns json opera no modelo velho).
- `class não é user class` (42): classes Registry não-fiadas (Proxy/ArrayBuffer/
  WeakMap/TextEncoder/URL/DataView/typed arrays) — cada uma registro+spec+dispatch.
- `call to unknown function` (34): ~17 são `__RTS_GEN_*` (generators #211, deferido)
  + `Symbol` (#216) + globais soltos (structuredClone/btoa/setTimeout/encodeURI*).
- `expression arrow` (30) + stack-overflow em `tail_call`/`tco_*`: closures
  MUTÁVEIS (#195, env-record) + TCO ausente no motor novo (return_call/CallConv::Tail
  — otimização a implementar; recursão de cauda profunda estoura a pilha).
- `assignment target must be simple identifier` (14): member compound-assign/
  increment (`this.n++`, `this.n += x`, `obj.prop = v`) — read-modify-write de
  propriedade (shape slots). `builder_method_chain` também precisa marcar a var
  intermediária com a classe quando o método retorna `ret_class == classe receiver`.
- `expression await` (11): async real / event-loop (#207).
- `raw/unrecognized` (18): template/optchain em sítios não-pareados (construtor —
  pulei p/ não desalinhar o prólogo; corpos de arrow extraído).

Mecânicos drenados (todos feitos): member compound-assign/`++` (`this.n+=x`),
ret_class de método fluente (`c.inc().add()`). O QUE SOBRA agora é:
- **grind multi-método #226** (21): métodos de Array ainda não-impl
  (entries/values/keys/flat(depth)/flatMap/toSorted/toReversed/with/findLast/
  reduceRight/copyWithin) — cada um precisa trampolim + lowering.
- **features grandes**: generators #211, Proxy #218, typed-arrays/DataView/
  ArrayBuffer, async #207, closures mutáveis #195 (env-record), TCO (return_call).
- **globais soltos** (1-2 arq cada): encodeURI*/btoa/structuredClone (limpos);
  setTimeout/queueMicrotask (precisam event-loop).
- JSON.stringify/parse: serializar objetos shape/slot do motor novo.
Não há mais correção pontual barata — daqui pra frente é trabalho de feature
sustentado, melhor por épico em sessão dedicada.

Commits recentes (mais novo primeiro), todos com gate (unit 703/703):
- `728af1bc` `gen.next()/.return()/.throw()` corretos — engine lê o result-Map via
  acessores ITER_VALUE/DONE e reconstrói `{value,done}` do modelo novo (→269).
  [#211 fase 3]. CONHECIDO: `[...gen()]` spread + yield* de array = fase seguinte.
- `71dea0a0` Map/Set iteráveis via `*[Symbol.iterator]()` REAL + for-of de classe
  iterável (motor nomeia só a chave de protocolo; .ts JS puro) (→266).
- `b11c0003` generators LAZY state-machine (loops com yield) — GEN_SM_* + DRAIN
  (→265). [#211 fase 2]. CONHECIDO: yield* de array erra (GEN_DELEGATE_START
  runtime era-i64 não entende array NaN-boxed — fase 3); `.next()` direto fase 3.
- `a3171e79` generators EAGER (yields lineares) — __RTS_GEN_FINISH→SET_RET (→262).
  [#211 fase 1]
- `50b8ef90` idx_get cobre índice Tagged (`this.#a[this.#i]`) (→256).
- `29069d12` globais string→string (encode/decodeURI, btoa, atob) (→255).
- `42d348ff` for-of genérico sobre fonte Tagged (string param, array aninhado) (→251).
- `27ed11d3` Map/Set forEach/keys/values/entries/clear + for-of sobre call/método
  array (FnSig.ret_array) (→246).
- `3c3f26c5` new Map([[k,v]])/Set(iterable) + `__rtsadp_idx_get` (index genérico em
  receptor não-provado) (→242).
- `cd57728b` Array #226 callback (findLast/findLastIndex/reduceRight/flatMap) +
  toSpliced/copyWithin (→239).
- `9bde3293` métodos Array #226 (toSorted/toReversed/with/sort/flat(depth)/fromIndex)
  + fix `fcvt_to_sint_sat` em numeric_to_i64 (arr.flat(Infinity) não trapa) (→235).
- `23d65bf3` ret_class de método (return this/new C) → cadeia fluente (→228 verdes).
- `bb7f3177` member/index compound-assign + ++/-- (→224 verdes).
- `28a3a371` coerção Float64→Int (ToInteger) + símbolos gc closure (→221 verdes).
- `86f8bf2b` registra superfície ampla de namespaces std + gc internos (→213).
- `240eb013` top-level let capturado+mutado vira cell #195-parcial (→193).
- `2e647331` docs handoff.
- **`472ac269` FRAMEWORK `rts:test` RODA NO MOTOR NOVO (0 → 172 verdes)** — o
  GATE-MESTRE do §4. Bundle incluído como prelude; `import from "rts:test"` →
  funções ambientes; dispatch bare-ambient `test_core`/`string`/`fmt` (prelude-
  only); prelude com statements top-level prependados ao main; templates em método
  de classe + re-`this`-rewrite; arrow-callback de fn ambiente; gcell-função
  `f()`; captura de module-global vira cell-viva; param `fn:i64` chamado forçado
  Tagged; `expect(x).toBe(y)` via ret_class inferido; StrPtr coage via ToString;
  superfície completa `test_core`/`string`/`fmt` JIT-linkada (anti-SIGILL §7).
- `8e2c735c` imports de namespace-objeto (`import {io,gc} from "rts"` → `gc.x()`).
- `9b0b5f94` fix: gcell store thread-local (corrida cross-programa nos unit tests).
- **`27e3d4fd` GLOBAIS MUTÁVEIS MODULE-LEVEL (#195) + void fall-through** — maior
  alavanca: destravou o `print` do harness (escreve `let` top-level capturada) que
  bloqueava 369/630 arquivos. Ver §6.
- `3f4260ea`/`33fa9a8a`/`c2332c28`/`bb6d1986` Object/String/Boolean/Number → `.ts`.

**Baseline:** motor novo unit **703/703**; motor velho TS **1709/1710** (a 1 falha
`console_override_variadic` é PRÉ-EXISTENTE — confirmada no HEAD limpo via stash,
NÃO é regressão; sempre cite assim).

---

## 4. GATE-MESTRE — o framework `rts:test` — ✅ FEITO (`472ac269`)

> **RESOLVIDO.** Os 6 sub-gates abaixo foram todos implementados (ver commit
> `472ac269` e §3). O framework compila + roda; 172 arquivos verdes. O texto
> original abaixo fica como REGISTRO do que foi feito. O PRÓXIMO trabalho é a
> **cauda longa de clusters** do histograma do `measure_new.sh` (maior alavanca
> primeiro): hoje `expression arrow` (44, arrows que ainda bailam fora do padrão
> de callback), `unbound identifier` (39), `class X não é user class` (38),
> `call to unknown function` (32), `no Registry entry` p/ `.map/.filter` com
> callback (20), `expression raw/unrecognized` (18, templates/optchain em sítios
> ainda não pareados), `cannot coerce FloatN to IntN` (18). Re-rode
> `measure_new.sh` e ataque o maior.

### (registro) O gate do framework `rts:test`

**Por que é o gate-mestre:** TODO `.test.ts` termina com as asserções:
```ts
import { describe, test, expect } from "rts:test";
describe("fixture:x", () => { test("...", () => { expect(__rtsCapturedOutput).toBe("..."); }); });
```
O motor novo trata `rts:test` como NAMESPACE builtin (`rts:<ns>`) →
`namespace_member("test","expect")` falha. Mas `rts:test` é um **MÓDULO TS**:
`crates/rts-std/src/test/bundle.ts` (238 linhas). `describe`/`test`/`expect` são
FUNÇÕES TS que precisam ser COMPILADAS + linkadas + EXECUTADAS. **Enquanto o
framework não roda no motor novo, NENHUM arquivo completa** (fica 0/630
*completando*).

**O que o bundle.ts usa (features a garantir no motor novo):**
- `test_core.*` (suite_begin/case_begin/case_end/print_summary), `string.*`,
  `fmt.*` — namespaces a REGISTRAR no registry (`front/run/registry.rs`) +
  JIT-linkar (`runtime_link.rs`). Ver §5/§7.
- `let _before_all_fn: i64 = 0;` mutado em `beforeAll(fn){ _before_all_fn = fn; }`
  → MODULE GLOBAL (o gcell de #195 JÁ cobre isso).
- `fn: i64` chamado como `fn()` — **chamar um valor-função (i64 handle)**. O
  callback de `test`/`describe` é uma arrow `() => {...}` reificada num
  TAG_FUNCTION. Conferir que `funcops`/reify + call-indirect cobrem.
- `expect(x)` retorna um MATCHER (objeto com `.toBe`/`.toEqual`/…). Ver o resto do
  bundle (`sed -n '40,238p'`).

**Sub-gates CONCRETOS já levantados (ataque nesta ordem):**
1. **Registrar + JIT-LINKAR `test_core`, `string`, `fmt`.** `ns::test_core::register`
   existe (`crates/rts-std/src/test/mod.rs:287`), `fmt` idem. MAS **test_core tem 0
   símbolos em `runtime_link.rs`** → chamar `test_core.suite_begin` = SIGILL. É
   preciso adicionar TODOS os `__RTS_FN_NS_{TEST_CORE,STRING,FMT}_*` em
   `runtime_link::jit_symbols` + sigs (ou via registry_call). (~35 fns; agente
   dedicado.) Confirme cada símbolo existe no runtime antes.
2. **Dispatch BARE-AMBIENT de namespace:** o bundle chama `test_core.x()`/
   `string.x()`/`fmt.x()` SEM importar (idents ambientes, "import stripping"). Hoje
   `test_core` é unbound. Precisa: um Ident que nomeia um namespace REGISTRADO e não
   é local → `obj.member()` resolve via `namespace_member` (como `Math.x()` em
   `mathobj.rs`, ou estender o ramo ns-objeto de `method.rs` p/ namespaces ambientes
   conhecidos, não só os importados). Só p/ código origem-prelude (o bundle).
3. **Incluir `bundle.ts` como prelude** (`registry.rs::build_registry` →
   `e.include(rts_runtime::TEST_BUNDLE_TS)`; const em `rts-primitives`/`rts-runtime`
   facade OU lê de rts-std). describe/test/expect/Matcher viram ambientes.
4. **Wire `import {…} from "rts:test"`:** NÃO ligar como `Builtin{ns:"test"}` (vira
   chamada de namespace e falha). Tratar `rts:test` como MÓDULO: cada nome resolve
   à função/classe ambiente do prelude de mesmo nome (skip binding, ou binding
   Local-ambiente). `flatten.rs` + `module_entry::apply_bindings`.
5. **Classe `Matcher` + `expect()`** retornando `new Matcher(actual)` com getter
   `not` + métodos. Classes/getters já existem no motor — validar o bundle compila.
6. **`fn: i64` chamado como `fn()`:** o callback de describe/test é arrow `()=>{}`
   reificada (TAG_FUNCTION); `fn()` = call-indirect de valor-função. Validar
   `funcops`/reify cobrem o param-i64-chamado.

Itera com `run-new` num `.test.ts` mínimo após cada sub-gate; o bundle revela o
próximo bail.

**Abordagem provável (validar):** incluir `bundle.ts` como PRELUDE (igual
`error.ts`/`MAP_SET_TS` em `registry.rs::build_registry` via `e.include(...)`) p/
describe/test/expect virarem funções AMBIENTES; e fazer `import {…} from "rts:test"`
ligar a essas funções do prelude (caso especial no resolver de módulos /
`flatten.rs` + `namespace_member`/binding). Registrar `test_core`/`string`/`fmt`.
ALTERNATIVA: tratar `rts:test` como um módulo-arquivo embutido. Medir qual é menos
invasivo. **Cuidado SIGILL:** todo símbolo de namespace chamado tem que estar
JIT-linked (`runtime_link.rs`) — senão link-OK/runtime-SIGILL (viola o floor).

Depois do framework vem a **cauda longa de features** (abstract class, switch,
for-of genérico, async real/event-loop, callbacks de array com captura, getters/
setters, spread, Object.create/defineProperty, etc.) — cada cluster do histograma.

---

## 5. Mapa de arquivos do motor novo (onde mexer)

Pipeline: `TS → SWC → AST → HIR → front/run/ (lowering→Cranelift) → JIT/AOT`.
NÃO há MIR no motor novo. Egraph é o único otimizador.

| Papel | Caminho |
|---|---|
| Entrada single-file / string | `front/run/mod.rs` (`build_program`, `run_source`, `render_source`) |
| Entrada multi-arquivo (resolve `./imports`) | `front/run/module_entry.rs` (`run_path`/`build_path`/`apply_bindings`) |
| Resolver/grafo de módulos + bindings | `front/modules/` (`flatten.rs` = Binding{Builtin/Local}, `resolve.rs`) |
| Lowerer (contexto por função; campos locals/captures/gcells/classes/builtins) | `front/run/lower.rs` (struct + `lower_function`) |
| Statements (let/assign/if/while/return/…) | `front/run/stmt.rs` |
| Compound/logical assign | `front/run/assign.rs` |
| Expressões + ident resolution | `front/run/expr.rs` (`lower_ident`) |
| Chamadas (user fn, builtin import, global fn) | `front/run/call.rs` (`lower_builtin_call` pub(super)) |
| Dispatch de método (string/number/array/object/ns-obj/registry) | `front/run/method.rs` (`try_method_dispatch`) |
| Classes (.ts + user): ctor/método/getter/instanceof | `front/run/class/` + `globalclass.rs` |
| Estáticas de Object | `front/run/objstatic.rs` |
| Math/Number estáticas | `front/run/mathobj.rs` |
| Namespace privado `engine.*` (helpers irredutíveis) | `front/run/engineobj.rs` + `crates/rts-std/src/engine/` |
| Registry (registra ns + classes; `namespace_member`) | `front/run/registry.rs` |
| Marshal genérico de chamada de namespace/registry | `front/run/registry_call.rs` |
| Valores-função (reify/call/bind) | `front/run/funcval/` + `value/funcops.rs` |
| **Globais mutáveis (#195): get/set por id** | `front/run/gcell.rs` + análise em `front/run/funcval/mod.rs::module_globals` |
| Tabela de sigs ABI (símbolos→SymSig) | `value/abi_sig.rs` |
| Símbolos JIT (símbolo→fn_ptr) | `runtime_link.rs` (`jit_symbols`) |
| Compilação/JIT do programa | `front/run/module_jit.rs` (`compile_program`/`define_one`) |
| PolyValue (NaN-box) | `value/mod.rs` (`PolyValue`, encode, BOX_BASE, tags) |
| GC collector + roots (microtask + gcell) | `crates/rts-std/src/collector/collector.rs` (`finish_cycle`, `mark_gcell_roots`) |
| Testes unit do motor novo | `front/run/tests/*.rs` (registrar cada arquivo em `tests/mod.rs`) |

---

## 6. PADRÕES PROVADOS (reuse exato)

### A. Primordial → `.ts` (dupla-natureza)
Já feito p/ Boolean/Number/String/Object/Error. Padrão (ver `prompt.md` §2 e
`crates/rts-primitives/src/{number,boolean,string,object,error}.ts`):
classe `.ts` no prelude (método-lib); core irredutível em Rust re-exposto PRIVADO
via `rts:engine` (`engine.num_*`/`str_*`/`obj_has`); rota primitivo→classe via
`method::try_primitive_class_method`; `X(x)` factory + `new X(x)` via classe.

### B. `engine.*` helper privado (envolve impl Rust existente)
Lowering table-driven em `engineobj.rs` (`engine_member`/`engine_num_member`/arms
dedicados); impl em `crates/rts-std/src/engine/mod.rs` (#[no_mangle] que CHAMA o
`__RTS_FN_*` existente — nunca reimplementa); sig em `value/abi_sig.rs`; símbolo
JIT em `runtime_link.rs`. Gate de privacidade: só código origem-prelude
(`is_prelude`) pode nomear `engine`.

### C. Globais mutáveis module-level (#195) — JÁ FEITO, reuse o mecanismo
`funcval::module_globals(funcs, main)` acha `let` top-level escrita-de-função →
cell por id; `gcell.rs` faz read/write via `__RTS_FN_NS_GC_GCELL_GET/SET`; store
thread-local + `mark_gcell_roots` em `collector.rs`. Threading `gcells:
HashMap<String,u32>` espelha `captures` (LoweredProgram→module_jit→Lowerer).

### D. Imports de namespace-objeto — JÁ FEITO
`import {io,gc} from "rts"` → `Binding::Builtin{ns:<nome>, member:""}` (flatten.rs);
`gc.member()` → `lower_builtin_call` via `namespace_member` (method.rs); ns
registrado em registry.rs + símbolos JIT-linked.

### E. Void fall-through — JÁ FEITO
Função `:void`/ret Tagged que cai no fim emite `return undefined`
(`lower.rs::lower_function`).

---

## 7. Disciplina de SIGILL / JIT-linkage (CRÍTICO)

Registrar um namespace/membro no registry NÃO garante que o símbolo
`__RTS_FN_NS_*` esteja JIT-linked. Se o lowering emitir `call <símbolo>` e o
símbolo não estiver em `runtime_link::jit_symbols` → **link-OK em compile, SIGILL
em runtime** (viola o floor: "nada que crasha como pass"). Ao habilitar uma nova
chamada de namespace: confirme o símbolo em `runtime_link.rs` E a sig em
`value/abi_sig.rs` (ou que `registry_call` use a sig do Member do registry).
Teste e2e ANTES de contar como ganho.

---

## 8. Como TESTAR / GATE (antes de cada commit)

```bash
# unit do motor novo (rápido; baseline 703)
cargo test -p rts-codegen-new --lib 2>&1 | tail -5
# build incremental do bin (debug) p/ run-new
cargo build && target/debug/rts.exe run-new caminho/arquivo.ts
# GATE motor velho (SEMPRE que tocar crate compartilhado: rts-primitives/
# rts-shared/rts-std/rts-engine/rts-hir/rts-ast/rts-parser) — baseline 1709/1710
cargo build --release -p rts-runtime && cargo build --release
target/release/rts.exe test 2>&1 | tr '\r' '\n' | grep -iE "Files|Tests" | tail
```
A 1 falha do motor velho (`console_override_variadic`) é PRÉ-EXISTENTE. Qualquer
falha A MAIS = regressão sua → corrigir ou justificar explícito.

Testes unit: `front/run/tests/*.rs` usam `assert_stdout(src, expected)` /
`assert_bails(src)`. Ao implementar uma feature que era um `assert_bails`,
ATUALIZE o teste negativo p/ `assert_stdout` (regressão intencional/justificada).
Registre cada arquivo novo em `front/run/tests/mod.rs`.

---

## 9. Coordenação de AGENTES (modelo desta campanha)

- **A cabeça principal coordena**; dispara AGENTES Explore (read-only) p/ MAPEAR a
  maquinaria de um gate ANTES de implementar (evita thrash). Ex.: "mapeie como X
  é lowered, file:line, sem dumpar arquivos". Resultado volta como conclusão.
- **Paralelize gates INDEPENDENTES** em agentes (clusters que não tocam os mesmos
  arquivos). Gates que tocam `lower.rs`/`method.rs`/`stmt.rs` (núcleo) são
  sequenciais — conflitam.
- Cada agente: implementa UM cluster, roda o gate (unit + e2e do(s) arquivo(s) do
  cluster), e reporta diff + resultado. A cabeça integra + roda o gate full +
  commita.
- **Sempre meça com `measure_new.sh`** antes/depois p/ quantificar (e detectar
  regressão de cobertura). Arquivos <500 linhas (split por pasta).

---

## 10. Convenções de commit

Conventional commits (`feat:`/`fix:`/`refactor:`/`docs:`/`chore:`), corpo pt-BR
explicando o PORQUÊ + os gates (unit X/Y, motor velho 1709/1710 pré-existente),
footer:
```
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
```
Heredoc no Bash (`git commit -F - <<'EOF'`), NÃO here-string PowerShell.

---

**Resumo de uma linha:** rode `measure_new.sh`, ataque o maior cluster (próximo =
o FRAMEWORK `rts:test` / bundle.ts, §4), gate sempre (unit 703 + velho 1709/1710),
reuse os padrões §6, cuide do SIGILL §7, coordene via agentes §9, commits §10.
