# Plano — GC novo + API/ABI gc modernizada (motor novo, zero legacy)

> **Status:** PLANO DE EXECUÇÃO. A ser executado em sessão dedicada. Objetivo
> duplo e claro: (1) entregar o MELHOR GC para o ambiente nativo do RTS;
> (2) entregar a MELHOR API/ABI de gc para o motor novo (`rts-codegen-new`), SEM
> nenhuma bagagem do motor antigo. Há poder para refazer a ABI inteira — não se
> limitar à forma atual de comunicação (extern-C-por-função, handles-como-i64),
> adaptar livremente para a lógica mais atualizável do motor novo.
>
> Pré-requisito de leitura: [`gc-generational-design.md`](gc-generational-design.md)
> (o desenho do coletor — fase weak + geracional copying nursery) e
> [`rts-codegen-new-design.md`](rts-codegen-new-design.md) §5 (PolyValue / GC).

## 0. Por que agora (o que está atrapalhando)

A API `gc.*` atual é majoritariamente do MOTOR ANTIGO (handles `i64` onde um
handle de string DOBRAVA como a própria string). No motor novo strings são
`PolyValue` `TAG_STR` nativas — a API de **string pool manual** virou lixo que
ativamente quebra:

- `gc.string_from_i64/f64/concat/new/from_static` retornam um Handle com
  `ts_signature: number` → o motor novo reboxa o resultado como NÚMERO cru
  (`ret_is_string_handle = ts_returns_string(ts_sig)`), então
  `print(gc.string_from_i64(v))` imprime o número do handle, não `"v"`.
- **~110 fixtures legacy** usam o padrão manual
  `const h = gc.string_from_i64(v); print(h); gc.string_free(h);` — pura dança do
  pool do motor antigo. O motor novo faz isso NATIVO: `print(String(v))` /
  `` print(`${v}`) `` / `print("" + v)` (todos verificados funcionando).
- O harness `rts:test` (BUNDLE_TS) NÃO usa `gc.*` — só as fixtures. Logo migrar é
  seguro e isolado.

Conclusão: NÃO "consertar" `gc.string_*` (não é forma do motor novo). REMOVER a
API de pool manual + MIGRAR as fixtures para string nativa + REDESENHAR a
superfície `gc.*` que sobra para PolyValue-nativa.

## 1. Auditoria da superfície atual

### 1.1 O coletor (motor)
- `crates/rts-engine/src/heap/handles.rs` — **2078 linhas**, `enum Entry` com
  ~40 variantes (mistura: `String`/`Buffer`/`Vec(Box<Vec<i64>>)`/`Map(Box<IndexMap>)`
  + backend `Tcp*`/`Tls*`/`Udp*`/`Sync*`/`Atomic*`/`JoinHandle` + motor-novo
  `Instance`/`Function`/`Symbol`/`WeakMap`/`Proxy`/`FinalizationRegistry`).
- `crates/rts-std/src/collector/` — `collector.rs` (165, mark+sweep + gcells),
  `string_pool.rs` (1284), `stack.rs`, `generator.rs`, `error.rs`, `mod.rs`.
- GC atual: mark+sweep preciso (UserStackMap + scanner conservador), `GCELL_*`,
  scanner reconhece palavras NaN-boxed PolyValue (design §5.4).

### 1.2 A API `gc.*` (membros — classificar)
`string_from_i64 string_from_f64 string_concat string_eq string_cmp
string_from_static string_new string_len string_ptr string_free handle_len
env_alloc env_get env_set env_free closure_alloc closure_fn_ptr closure_env
instance_new instance_class instance_free instance_load_i64 instance_store_i64
instance_load_i32 instance_store_i32 instance_load_f64 instance_store_f64
collect collect_vec live_count` + internos `POLY_TO_HANDLE POLY_FROM_HANDLE
GCELL_GET GCELL_SET COLLECT COLLECT_DEBT`.

Classificação:
- **LEGACY / REMOVER** (pool manual de string do motor antigo — substituído por
  PolyValue TAG_STR + String()/template):
  `string_from_i64 string_from_f64 string_concat string_eq string_cmp
  string_from_static string_new string_len string_ptr string_free`.
  (O POOL em si — `Entry::String` + intern — FICA; é onde TAG_STR vive. O que sai
  é a SUPERFÍCIE `gc.string_*` exposta ao usuário/fixtures.)
- **LEGACY / REMOVER** se o motor novo não usa: `instance_load_i32/store_i32`,
  `instance_load_f64/store_f64`, `handle_len`, `collect_vec` — confirmar 0 uso no
  `rts-codegen-new` + migrar as poucas fixtures.
- **AVALIAR / MIGRAR para PolyValue-nativo**: `env_*` (closures #195 usam env de
  PolyValue agora — ver se a API `env_alloc(i32)/env_get/set` ainda casa, ou se é
  substituída pelo mecanismo de cell/Vec atual), `closure_*`, `instance_*`
  (instâncias de classe são `Entry::Vec` keyed agora — `instance_new/class/load/store`
  pode estar duplicando o caminho `__rtsadp_obj_*`).
- **MANTER (núcleo do motor novo)**: `POLY_TO_HANDLE POLY_FROM_HANDLE GCELL_GET
  GCELL_SET COLLECT live_count` (+ os `__rtsadp_obj_*` que JÁ são a API de objeto
  PolyValue-nativa).

### 1.3 `enum Entry` — o que o motor novo realmente precisa
DO MOTOR NOVO: `String` (TAG_STR), `Vec(Box<Vec<i64>>)` = objeto/array keyed
(slot0 shape + valores PolyValue), `Function`, `Instance` (avaliar se ainda
distinto de `Vec`), `Symbol`, `WeakMap/WeakSet/WeakRef/FinalizationRegistry`,
`Proxy`, `Closure`/`Env`, `ErrorObj`, `DateMs`, `Regex`, `Json`, `BigFixed`,
`Promise*`, `Buffer`, e os backend (`Tcp*/Tls*/Udp*/Sync*/Atomic*/Http*/Events*/
JoinHandle/ProcessChild/Hasher/CString/OsString`) — estes backend ficam (são
recursos reais de namespaces ativos), mas devem ser AUDITADOS: qualquer um sem
caminho vivo no motor novo é lixo.

## 2. Parte A — O GC (melhor coletor pro ambiente RTS)

Sem reescrita de arquitetura agora — seguir o caminho faseado de
[`gc-generational-design.md`](gc-generational-design.md):

- **A1. Fase weak (pequeno, #217):** WeakMap/WeakSet/WeakRef/FinalizationRegistry
  REAIS via uma fase entre mark e sweep no mark+sweep atual. Não reescreve a GC.
  Hoje são `.ts`/Entry strong-ref interino.
- **A2. Geracional copying (nursery) (grande, DEFERIDO até ~90% cross-runtime):**
  young bump-alloc + minor GC copia sobreviventes + write barrier + remembered
  set + TLAB por-thread. A handle indirection (PolyValue = índice de slot) torna
  MOVER ≈ grátis (atualiza só slot→endereço, sem pointer-patching). Old gen
  mark-compact, roda raro.

**Esta atualização de API é o que PREPARA o terreno para A1/A2:** uma `Entry`
enxuta + uma ABI PolyValue-nativa + o tracing de filhos correto (palavras
NaN-boxed) são pré-requisitos de um coletor móvel limpo.

## 3. Parte B — HandleTable / `Entry` redesign

- **B1. Auditar cada variante de `Entry`** por uso vivo no motor novo + namespaces
  ativos. Remover toda variante morta (lixo). Documentar o tracing de filhos de
  cada variante sobrevivente (o que o mark/copy precisa visitar) — pré-requisito
  do geracional.
- **B2. Quebrar `handles.rs` (2078 linhas)** em submódulos < 500 (regra de layout)
  — `entry/` por categoria (primordial / collection / backend / weak), tracing
  centralizado.
- **B3. Confirmar o contrato PolyValue↔handle** num único lugar: `POLY_FROM_HANDLE`
  (handle 64-bit → payload 48-bit slot) e `POLY_TO_HANDLE` (payload → handle,
  generation lida do slab). Hoje a regra `& PAYLOAD_MASK` foi pegadinha (ver
  Proxy #218) — centralizar e testar.
- **B4. Objeto keyed = `Entry::Vec`** de palavras PolyValue (slot0 = shape-id).
  Avaliar fundir `Instance` com `Vec` (instância de classe já é objeto keyed). Um
  só tipo de objeto simplifica o tracing do geracional.

## 4. Parte C — API/ABI `gc.*` redesign (zero legacy)

- **C1. REMOVER a superfície de pool manual de string** (`gc.string_from_i64/
  f64/concat/eq/cmp/new/from_static/len/ptr/free`) da spec (`collector/mod.rs`),
  do `runtime_link.rs`/`abi_sig.rs` do motor novo, e dos símbolos JIT. Strings são
  TAG_STR; conversão é `String()`/template/`+` nativos; comparação é `===`/`<`
  nativos.
- **C2. REMOVER `instance_load/store_i32/f64`, `handle_len`, `collect_vec`** e
  qualquer outro membro sem caminho vivo no motor novo (confirmar por grep no
  `rts-codegen-new`).
- **C3. MODERNIZAR a forma de comunicação** (poder total para refazer a ABI):
  - A superfície `gc.*` que sobra é INTERNA (motor↔runtime), não user-facing TS.
    Não precisa de um `extern "C"` por operação trivial nem de `ts_signature`
    mentirosa. Definir uma ABI PolyValue-nativa mínima: `collect()`, `live_count()`,
    `poly_to_handle`/`poly_from_handle`, `gcell_get/set`, e os `__rtsadp_obj_*`
    (get/set/has/delete/keys/values) como A API de objeto canônica.
  - Onde fizer sentido, trocar múltiplos externs por um caminho data-driven
    (alinhado ao §10 do design — ABI derivada de SPECS). NÃO carregar o padrão
    antigo de "um símbolo manual por operação".
- **C4. `env_*`/`closure_*`/`instance_*`**: reconciliar com os mecanismos atuais
  do motor novo (cell por-invocação #195, `__rtsadp_obj_*`, env de closure por
  Vec de PolyValue). Remover o que duplica; manter um caminho só.

## 5. Parte D — Migração das ~110 fixtures legacy

- **D1. Reescrever o padrão** `const h = gc.string_from_i64(v); print(h);
  gc.string_free(h);` → `` print(`${v}`) `` (ou `print(String(v))`). Idem
  `string_from_f64`/`string_from_static`/`string_concat`. Script de migração
  (regex) + revisão manual dos casos compostos.
- **D2. Fixtures que testam a API gc EM SI** (`alloc_*`, `gc_instance_*`,
  `env_*` low-level): decidir caso a caso — se a API foi removida, a fixture vai
  junto (testava motor antigo) OU é reescrita para o mecanismo novo. O piso de
  honestidade: não deletar fixture para inflar número; deletar só o que testava
  uma API removida por design (regressão explícita e justificada).
- **D3. Re-medir** correção real (assertion-level, não só run-exit-0 — ver
  [`project_measure_metric`] na memória: `measure_new.sh` conta cobertura de
  execução, não correção). Esperado: grande salto de CORREÇÃO quando as ~110
  fixtures pararem de imprimir números de handle.

## 6. Fases de execução (ordem)

1. **C1 + D1 primeiro** (desbloqueio imediato, baixo risco): remover `gc.string_*`
   + migrar fixtures para string nativa. Isto sozinho destrava as ~110 fixtures
   e limpa o maior "atrapalhar". Medir correção antes/depois.
2. **C2/C3/C4 + B1**: drenar o resto da API legacy + auditar `Entry`. Cada
   remoção: grep zero-uso no motor novo, suíte verde, gate sem hard violation.
3. **B2/B3/B4**: refatorar `handles.rs` em submódulos < 500 + centralizar o
   contrato PolyValue↔handle + fundir Instance/Vec.
4. **A1 (fase weak / #217)**: com a Entry enxuta + tracing documentado, a fase
   weak entra limpa.
5. **A2 (geracional)**: projeto dedicado, DEFERIDO até ~90% cross-runtime
   (não trocar a GC enquanto o motor ainda preenche semântica — só adiciona
   variável instável no caminho crítico).

## 7. Invariantes / piso (nunca cede)

- **Build compila; suíte conhecida a cada passo; regressão só explícita e
  justificada** (regra REGRESS-WHEN-NECESSARY).
- **Doutrina PRIMORDIAL-vs-Registry**: o motor nomeia só primordiais; `gc` é
  namespace interno (não classe não-primordial), mas a forma de chamá-lo segue a
  ABI do motor novo, sem hardcode de classe não-primordial no front.
- **Layout**: nenhum arquivo do motor > 500 linhas (quebrar `handles.rs`).
- **Honestidade da métrica**: usar medida de CORREÇÃO (parsear `✗` do `run-new`),
  não só run-exit-0. Não deletar fixture para inflar; remoção de fixture só quando
  ela testava uma API removida por design.
- **GC**: nada que crashe/trave commitado como "pass". O scanner reconhece
  palavras NaN-boxed PolyValue (design §5.4); manter essa invariante em qualquer
  mudança de `Entry`.

## 8. Critério de pronto

- `gc.string_*` e todo membro legacy REMOVIDOS; `grep` por eles no repo = 0 (fora
  de histórico).
- Fixtures migradas para string nativa; correção (assertion-level) medida e
  subindo.
- `handles.rs` quebrado em submódulos < 500; `Entry` auditada (zero variante
  morta); contrato PolyValue↔handle centralizado + testado.
- A ABI `gc.*` restante é PolyValue-nativa, mínima, sem `ts_signature` mentirosa,
  alinhada à direção data-driven do design.
- (Fase A1) WeakMap/WeakSet/WeakRef/FinalizationRegistry reais via fase weak.
- (Fase A2, deferida) geracional copying nursery — projeto dedicado pós-90%.
