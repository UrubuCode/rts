# RTS_ENGINE.md — Engine de despacho de métodos (resolução única + emissão única)

> Estado: **proposta / design canônico**. Complementa `docs/specs/rts-core-engine.md`
> (que cobre a metade-*registro* da engine) com a metade-*despacho* que falta:
> como `recv.method(args)` resolve para um símbolo nativo sem o string-match
> espalhado de hoje.
>
> Princípio inegociável: **100% nativo, zero interpretação.** TS bem-tipado
> compila para `call <symbol>` direto, idêntico a hoje. Só receiver genuinamente
> `any` paga 1 tag-check em runtime — custo que já existe.

---

## 0. TL;DR

Hoje existem **dois resolvedores de método paralelos** fazendo a mesma coisa:

1. **Tabela-dirigido** (`abi::global_class_lookup` → `GlobalClassSpec` →
   `lower_global_instance_call`) — limpo, genérico, dirige Date/URL/RegExp/etc.
2. **String-match** (`builtins.rs`, ~182 braços `match method { "indexOf" => … }`)
   — hand-rolled, duplica símbolos e assinaturas que a tabela já tem.

Para `String` os dois coexistem: **34 rows de spec E ~50 braços string-match
apontando para os mesmos símbolos `__RTS_FN_GL_STRING_*`.**

A engine = **unificar tudo no caminho tabela-dirigido**, adicionando as duas
peças que faltam: (A) um **tipo-de-receiver** que sobrevive até o call-site, e
(B) uma **porta-de-entrada única** de resolução.

**Mas** a fundação atual (`rts-macro` + registry + `lookup`) tem **5 falhas
estruturais** (§4) que, se a engine for construída em cima sem corrigi-las,
viram bug silencioso multiplicado. Ordem correta: **corrigir fundação → depois
engine** (§6).

---

## 1. Por que uma engine, e por que NÃO é interpretação

RTS compila TS → nativo via Cranelift. Não tem motor JS. O risco que o usuário
levantou é legítimo: hoje o codegen "menciona o código diretamente" — decide
qual método chamar por **comparação de string do nome** (`"toString"`,
`"indexOf"`, …) espalhada por ~12 arquivos. Isso não é interpretação em runtime
(o binário final é nativo), mas é uma **arquitetura de despacho ad-hoc**: a
decisão "qual método" está estruturalmente codificada como um match-de-string no
compilador, não como um contrato de dados.

Uma engine de verdade separa três responsabilidades hoje fundidas:

| Responsabilidade | Hoje | Engine |
|---|---|---|
| **Declarar** um método (nome, símbolo, args, retorno) | macro `#[rts_method]` (parcial) + literal em `builtins.rs` | macro `#[rts_method]` (única fonte) |
| **Resolver** `(receiver, método, aridade)` → alvo | waterfall string-match ordenado | `engine::resolve_method` (1 lookup chaveado) |
| **Emitir** o alvo em IR | inline em cada braço | `engine::emit_method_target` (1 emissor) |

O ganho de "ser engine" é exatamente essa separação: declarar vira dado, resolver
vira lookup, emitir vira uma função. Nenhuma das três envolve interpretar TS em
runtime. O hot-path numérico continua idêntico (invariante `rts ir`, §7).

---

## 2. Estado atual — o que já funciona (e não jogar fora)

O caminho tabela-dirigido **já prova que a tabela consegue dirigir o despacho**:

```
abi::global_class_lookup("Date")            → &GlobalClassSpec
  .instance_method("getFullYear")           → &NamespaceMember
signature::lower_member(member)             → assinatura Cranelift
lower_global_instance_call(member, recv, …) → emite call <member.symbol>
```

`lower_global_instance_call` (`ns_call.rs:481`) já faz **marshalling N-arg
genérico**: `StrPtr`→ptr+len, `F64`→fcvt, `I32`→coerce, resto→i64, receiver no
slot 0. Dirige construtores, métodos estáticos, getters e métodos de instância
de ~60 classes globais. A macro `#[rts_class]` gera as rows + os externs.

**A engine é uma evolução desse caminho, não um sistema novo.** Tudo que segue é
"fazer 100% das chamadas passarem por aqui".

---

## 3. Modo futuro (ideal) — exemplo concreto ponta a ponta

### 3.1 Runtime: declarar uma família primitiva como classe (uma vez)

`Array`/`Map`/`Set` hoje **não têm spec** — são alcançados só via `builtins.rs`
chamando `collections.vec_*`. No modo futuro viram classes declaradas, com as
rows apontando para os símbolos `__RTS_FN_NS_COLLECTIONS_VEC_*` que **já existem**
(zero runtime novo — rows `external`):

```rust
// crates/rts-core/src/namespaces/globals/array/mod.rs   (FUTURO)
/// Array.prototype — métodos sobre um handle Vec<i64>.
#[rts_class(Array, prefix = "COLLECTIONS_VEC", spec = "ARRAY_CLASS_SPEC")]
impl Array {
    // Caso comum: símbolo único, aridade fixa → vira SymbolCall puro.
    #[rts_method(external, ts = "join(sep: string): string")]
    fn join(_recv: Handle, _sep: Handle) -> Handle { unreachable!() }

    // Overload por aridade: DUAS rows, mesma `name`, aridades distintas.
    // O resolver escolhe pela contagem de args (hoje impossível — ver §4.3).
    #[rts_method(external, ts = "indexOf(x: any): number")]
    fn index_of(_recv: Handle, _x: I64) -> I64 { unreachable!() }
    #[rts_method(external, name = "indexOf", symbol = "__RTS_FN_NS_COLLECTIONS_VEC_INDEX_OF_FROM",
                 ts = "indexOf(x: any, from: number): number")]
    fn index_of_from(_recv: Handle, _x: I64, _from: I64) -> I64 { unreachable!() }

    // Variádico: NÃO é uma row fixed-arity — marcado `variadic`, emitido por um
    // handler residual que faz o loop de VEC_PUSH (ver §5.3).
    #[rts_method(external, variadic, ts = "push(...items: any[]): number")]
    fn push(_recv: Handle, _items: I64) -> I64 { unreachable!() }

    // Default-arg: `end` opcional; o EMISSOR sintetiza i64::MIN quando ausente.
    #[rts_method(external, default_args = "[_, i64::MIN]", ts = "slice(start?: number, end?: number): any[]")]
    fn slice(_recv: Handle, _start: I64, _end: I64) -> Handle { unreachable!() }
}
```

`String`/`Number`/`Boolean` já são classes (`STRING_CLASS_SPEC` tem 34 rows) —
só ganham `aliases`/`variadic`/`default_args` nas rows e perdem os braços
`builtins.rs` equivalentes.

### 3.2 Codegen: o call-site inteiro vira três linhas

```rust
// FUTURO — substitui o waterfall lower_string_builtin/lower_array_builtin/…
let recv = engine::recv_kind(ctx, &member.obj)?;          // RecvKind (1 ponto)
let target = engine::resolve_method(recv, &method, n_args) // lookup chaveado
                 .ok_or_else(|| unknown_method(recv, &method))?;
return engine::emit_method_target(ctx, target, recv_val, call); // 1 emissor
```

Sem ordem, sem "tenta string depois array depois map", sem `*_AUTO` espalhado.

### 3.3 O que cada chamada compila para

```ts
const s: string = getName();
s.indexOf("x", 3);     // RecvKind::Class("String"), 2 args
```
→ resolve para a row `indexOf/2` → `SymbolCall(__RTS_FN_GL_STRING_INDEX_OF_FROM)`
→ **`call` direto**, idêntico ao braço de hoje. Zero probe.

```ts
const a: number[] = [1,2,3];
a.indexOf(2, 1);       // RecvKind::Class("Array"), 2 args
```
→ row `indexOf/2` da `ARRAY_CLASS_SPEC` → `call __RTS_FN_NS_COLLECTIONS_VEC_INDEX_OF_FROM`.
**Mesma chave `(família, nome, aridade)` desambigua String vs Array** — o que a
ordem-de-tentativa de hoje fakeia.

```ts
arr.map(x => x.slice(1));   // x é param de arrow → tipo apagado
```
→ `recv_kind(x)` = `RecvKind::Unknown` → `resolve_method` retorna
`RuntimeAuto` → emite **um** `__RTS_FN_RT_DISPATCH_AUTO(handle, method_id, argv)`
que lê o tag do `Entry` em runtime (`Entry::String`/`Vec`/`Map`) e ramifica.
**Isto é 1 tag-check + call nativo, não interpretação.** É exatamente o que os 6
`*_AUTO` de hoje já fazem, generalizado para um.

### 3.4 Invariante observável

`target/release/rts.exe ir bench.ts` no hot-path numérico **não pode** mostrar
box/guard/probe novo. Receiver tipado → `call`. Só `any` → 1 `DISPATCH_AUTO`.
CI compara o IR dos benches canônicos (§7).

---

## 4. Falhas fatais da fundação atual (corrigir ANTES da engine)

O usuário perguntou explicitamente: *"caso o sistema do rts-macro possua uma
falha ou algo similar, mencione."* Achei **5 falhas estruturais + 3 papercuts.**
A lógica da engine (`resolve_method`/`emit_method_target`) é limpa, mas ela se
apoia em três primitivas — a **macro**, o **registry** e o **`lookup`** — e as
três têm buracos que, se ignorados, viram gap silencioso multiplicado por
centenas de métodos.

### 4.1 CRÍTICA — agregação ainda é array escrito à mão (stage 2 não feito)

`GLOBAL_CLASS_SPECS` (63 entradas) e `SPECS` (~48 entradas) em
`crates/rts-codegen/src/abi/mod.rs` são **arrays hand-mantidos**. A macro deriva
o `*_CLASS_SPEC` por classe, mas registrar a classe no array é **manual**.

> **Por que é fatal para a engine:** a engine descobre métodos iterando o
> registry. Esquecer uma linha `&…::ARRAY_CLASS_SPEC` = a família inteira some
> do despacho **sem erro de compilação** — `resolve_method` retorna `None`, o
> método cai no fallback ou explode. "Declarei a classe e não funciona" sem
> pista. A quádrupla-escrita está meio-morta: declarar-método funciona DENTRO da
> classe; registrar a CLASSE ainda é cópia manual.

**Correção (stage 2 do core-engine):** `linkme::distributed_slice`. Cada
`#[rts_class]`/`#[rts_namespace]` faz `#[distributed_slice(CLASS_REGISTRY)]` —
o array se monta sozinho no link. `global_class_lookup` itera o slice. Esquecer
vira impossível: declarou = registrado.

### 4.2 CRÍTICA — `jit.rs` ainda tem 1104 `add_fn!` à mão (stage 3 não feito)

Cada extern `__RTS_FN_*` precisa ser registrado **de novo** no mapa de símbolos
do JIT (`add_fn!("__RTS_FN_GL_STRING_X", path)`). Adicionar um método = `#[rts_method]`
(auto) **mais** uma linha em `jit.rs` (manual).

> **Por que é fatal:** esquecer a linha do JIT = método funciona no AOT mas dá
> **símbolo-ausente / ACCESS_VIOLATION em `rts run`**. A engine multiplica o
> número de métodos table-driven; cada um é uma chance de esquecer o `add_fn!`.
> Pior: falha só no caminho JIT, então passa em metade dos testes.

**Correção (stage 3):** `JITBuilder::symbol_lookup_fn` → `GetProcAddress(GetModuleHandle(NULL), name)`
(Win) / `dlsym(RTLD_DEFAULT, name)` (Unix). Todo `__RTS_*` é `#[no_mangle]`
linkado estático no `rts.exe`, já está na tabela de símbolos do processo. Mata as
1104 linhas. Caveat AOT: garantir `#[used]`/export-list para o linker não podar
externs não-referenciados (testar MSVC).

### 4.3 CRÍTICA — `instance_method(name)` é first-by-name, SEM aridade

`global_class.rs:42`:
```rust
self.members.iter().find(|m| m.kind == InstanceMethod && m.name == name)
```

É o **primeiro por nome**. Se duas rows compartilham `name` (overload
`indexOf/1` vs `indexOf/2`), **só a primeira é alcançável pela tabela.** A
seleção-por-aridade que existe está fakeada **em um único call-site**
(`mod.rs:1117`, `args.len()-1 == n_call_args`), não na primitiva.

> **Por que é fatal:** a tabela **estruturalmente não consegue guardar
> overloads** hoje. String/Array vivem de overloads (`indexOf`, `slice`,
> `startsWith`, `splice`…). Migrar `builtins.rs` → rows é impossível enquanto o
> lookup só vê a primeira row de cada nome.

**Correção:** assinatura nova `resolve_member(name, n_args) -> Option<&Member>`
com seleção por aridade dentro da primitiva (a lógica de `mod.rs:1117` vira a
referência), com fallback para a row de maior aridade + default-args. Os 4 sites
duplicados colapsam nessa única primitiva.

### 4.4 CRÍTICA — `NamespaceMember` é fino demais para semântica de protótipo

A struct hoje: `name, kind, symbol, args:&[AbiType], returns, doc, ts_signature,
intrinsic, pure`. **Falta:** `aliases`, `variadic`, `default_args`,
`receiver_family` (ou owner). Sem esses campos a macro **não consegue expressar
como dado**:

- aliases (`toLowerCase`|`toLocaleLowerCase`, `trimStart`|`trimLeft`, `includes`|`contains`) — hoje OR-pattern hardcoded no match.
- variádico (`push`/`concat`/`unshift`) — N calls de 1 source; o emissor genérico faz zip 1:1 e **erra em mismatch** (`ns_call.rs:500`).
- default-args (`slice` end, `padStart` pad) — o emissor erra em too-few-args; injeção de default é **lógica nova do emissor**, não algo que o runtime aplica.

> **Por que é fatal:** sem estender a struct + a macro, "migrar builtins → rows"
> só cobre o subconjunto fixed-arity-símbolo-único. O resto continua código
> hardcoded — a engine fica pela metade e os dois resolvedores continuam vivos.

**Correção:** estender `NamespaceMember` com `aliases: &[&str]`, `variadic: bool`,
`default_args: &[DefaultArg]`; estender o parser da macro (`parse_class_member`
já lê `name`/`ts`/`symbol`/`intrinsic` — adicionar 3 chaves). **Feito no Step 0,
não diferido.**

### 4.5 CRÍTICA — símbolo é cola stringly-typed re-digitada (bypassa o registry)

A macro deriva `__RTS_FN_GL_<PREFIX>_<FN_IDENT.to_uppercase()>`. `builtins.rs`
**re-digita o mesmo literal** em cada braço. Não há link em compile-time entre os
dois. Renomeie a fn Rust → o símbolo derivado muda → o literal em `builtins.rs`
**ainda compila** → quebra silenciosa. Pior: há caminhos de emissão
(`builtins.rs`) que **não passam pelo registry**, violando a tese de segurança do
core-engine ("o registry é o único portão de despacho").

> **Por que é fatal para estabilidade:** duas fontes de verdade para todo símbolo
> (a row da spec E o literal). O `rts.d.ts` lint checa a spec contra o gerador,
> mas **não** checa `builtins.rs`. Drift invisível.

**Correção:** **a engine só emite via `member.symbol`** — nunca um literal. Ao
deletar cada braço de `builtins.rs`, o símbolo passa a vir da row. Regra de
lint: nenhum `__RTS_FN_*` como string-literal fora de uma row de spec ou de
`symbol = "…"`. (Sub-papercut 4.5b: derivação por `to_uppercase()` é colisão-não-guardada
— `to_json`→`TO_JSON` e um hipotético `toJson`→`TOJSON` divergem; dois idents
snake distintos podem colidir no upper. Baixa probabilidade, mas adicionar
`validate_symbol` anti-colisão no agregador.)

### 4.6 Papercuts (não-fatais, mas corrigir junto)

- **Default de Str-nula = `return 0`.** O prelúdio de reconstrução de string da
  macro faz `return <zero-do-tipo>` em null/UTF-8-inválido. Para retorno
  `Handle`, `0` é um handle potencialmente-válido/lixo, não "undefined". Método
  de string com arg nula retorna handle 0 → lixo/crash downstream. `on_null`
  existe mas é opt-in. **Correção:** default deveria ser um sentinela de-erro
  explícito por tipo, não 0 cru.
- **Sem cross-check spec↔extern.** Nada garante que toda row tem um extern com
  aquele símbolo (rows `external` apontam para símbolos de outra namespace; um
  typo no `symbol="…"` só aparece em runtime). **Correção:** teste de
  build-time que resolve todo `member.symbol` contra a tabela de símbolos.
- **Sem gate "todo braço de builtins tem row".** Durante a migração, deletar um
  braço sem a row equivalente = método some. **Correção:** estender o lint
  `rts.d.ts` para exigir row antes de deletar braço (oráculo de divergência, §6).

---

## 5. Arquitetura estável e escalável

Camadas, de baixo (fundação) para cima (engine). Cada camada tem **um** dono e
um contrato; nada pula camada.

```
┌─ rts-macro ────────────────────────────────────────────────┐
│  #[rts_namespace] / #[rts_class] + #[rts_method] etc.        │
│  → deriva: extern "C" + NamespaceMember (com aliases/        │
│    variadic/default_args) + distributed_slice registry entry │  FUNDAÇÃO
└──────────────────────────────────────────────────────────────┘
┌─ rts-abi ──────────────────────────────────────────────────┐
│  NamespaceMember (estendido) · GlobalClassSpec ·             │
│  resolve_member(name, n_args) (com aridade) · Intrinsic      │
│  CLASS_REGISTRY / MEMBER_REGISTRY (linkme, auto-montado)     │
└──────────────────────────────────────────────────────────────┘
┌─ rts-codegen::engine (NOVO) ───────────────────────────────┐
│  recv_kind(ctx, expr) -> RecvKind         (1 ponto de tipo) │
│  resolve_method(recv, name, n) -> MethodTarget  (1 lookup)  │  ENGINE
│  emit_method_target(ctx, target, recv, call)    (1 emissor) │
└──────────────────────────────────────────────────────────────┘
┌─ call-sites (calls/mod.rs, indirect.rs, members.rs) ───────┐
│  todos chamam a engine; nenhum string-match próprio         │  CONSUMIDORES
└──────────────────────────────────────────────────────────────┘
```

### 5.1 `RecvKind` — o tipo-de-receiver que faltava

```rust
enum RecvKind {
    Class(&'static str),  // "String","Number","Array","Map","Set","RegExp" + classes-usuário
    UserClass(ClassId),   // classe definida pelo usuário (dispatch virtual)
    ProtoInstance,        // __proto__ map runtime — preserva MAP_GET_CHAIN
    ObjectLiteral,        // preserva guarda #480 (obj.add do usuário ≠ Set.add)
    Unknown,              // genuíno `any` — arrow param, map-get, capture
}
```

`recv_kind` **consolida num ponto** o que hoje está espalhado em ~12
side-channels do `FnCtx` (`local_array_vars`, `local_string_vars`,
`local_class_ty`, …) + `lhs_static_class`. Mesma informação, coletada uma vez.

**Honestidade sobre tipos (não negociar):** a info estática é **suficiente** para
Tier-1 (classe estaticamente conhecida) e Tier-2 (família via `ValTy::Bool`→Boolean,
numérico→Number, `: T[]`→Array) e **insuficiente** para Tier-3 (`Handle` opaco —
arrow param, resultado de map-get/JSON.parse). Tier-3 é **comum** em código
callback-heavy, não edge raro. A resposta correta para Tier-3 **nunca é chutar
por ordem** (fonte dos bugs de hoje) — é `RecvKind::Unknown` → `RuntimeAuto`.

### 5.2 `MethodTarget` — precisa cobrir TODOS os casos, não só builtins

```rust
enum MethodTarget {
    SymbolCall(&'static NamespaceMember),                 // ~maioria: call symbol, marshal args
    InlineIr(MethodIntrinsic, &'static NamespaceMember),  // Bool.toString, array.at, charCodeAt-F64
    UserClassMethod { owner: ClassId, method, arity, virtual: bool }, // classe-usuário + operator overload
    VirtualDispatch { candidates: &[ClassId], method },   // override em hierarquia
    Residual(ResidualKind),                               // variádico / regex-polimórfico / coerção
    RuntimeAuto(AutoKind),                                // receiver Unknown
}
```

> **Erro a evitar (apontado pelo review):** se `MethodTarget` for só
> `SymbolCall | InlineIr | RuntimeAuto`, a engine é porta **só-pra-builtins** —
> métodos de classe-usuário e **operator overload** (`a+b → a.add(b)`,
> `operators.rs:347`) ficam num resolvedor paralelo, e a engine **não unifica
> nada**. `UserClassMethod` + `VirtualDispatch` são obrigatórios desde o Step 1.

### 5.3 `MethodIntrinsic` — separado de `abi::Intrinsic`, NÃO reusar

> **Erro a evitar (apontado pelo review):** `abi::Intrinsic` é consumido pelo
> `intrinsic_resolver_default` do MIR (match **exaustivo**, sem modelo de
> receiver). Adicionar `BoolToString`/`ArrayAtNegIndex` ali **quebra a
> compilação do MIR** ou polui um enum de namespace-call com variantes que o MIR
> nunca alcança. Usar um enum **`MethodIntrinsic` local à engine**, com um
> emissor que recebe o receiver. `lower_intrinsic` (namespace) fica intocado.

### 5.4 Residuais (ficam código, honestamente)

Nem tudo vira row. Ficam como handlers residuais explícitos, marcados na row:

- **Variádico** (`push`/`concat`/`unshift`): loop de N calls + spread (`VEC_EXTEND_FROM`).
- **Regex/callback-polimórfico** (`replace`/`split`/`match`/`matchAll`): ramifica em /regex/ vs string vs fn.
- **Coerção-construção** (`Array(…)`/`Number(…)`/`String(…)` bare): branch numérico-vs-handle.
- **Inline-IR** (`Bool.toString` select, `array.at` negativo, `charCodeAt` F64): retornam ValTy/sentinela diferente do declarado.

**Payoff honesto:** a engine deleta os braços **fixed-arity-símbolo-único** (a
maioria dos 182) + os 6 `*_AUTO` → 1 + os 4 try-order sites → 1. **Não** "metade
de `builtins.rs` evaporada" — os residuais acima continuam. Mas viram um conjunto
**pequeno e nomeado**, não 2258 linhas de match.

### 5.5 `DISPATCH_AUTO` — design-spike, não freebie

Os 6 `*_AUTO` têm assinaturas **heterogêneas** (`SLICE_AUTO(u64,i64,i64)→u64`,
`CONCAT_AUTO(u64,i64)→i64`, sentinela `i64::MIN`). Colapsar em um
`DISPATCH_AUTO(handle, method_id, argv)` exige uma convenção argv-boxing que o
codebase **não tem**. **Não deletar os 6 típados** até o substituto passar a
suíte ambíguo-handle sem SIGILL (chão de honestidade). Alternativa segura:
manter AUTO por-forma mas **gerá-los das rows** (uma fonte de verdade, sigs
preservadas).

---

## 6. Ordem de implementação — fundação antes da engine

Cada step entrega sozinho, **build verde + suíte verde**. A regra de ouro: não
construir `resolve_method` sobre uma fundação com os buracos da §4.

```
FUNDAÇÃO (corrige §4 — pré-requisito da engine)
  F1  Estender NamespaceMember: aliases, variadic, default_args (§4.4).
      Estender parser da macro. Suíte intocada (campos default-vazios).
  F2  resolve_member(name, n_args) com aridade em rts-abi (§4.3).
      Os 4 sites duplicados passam a chamar essa primitiva. Verde.
  F3  linkme CLASS_REGISTRY/MEMBER_REGISTRY; SPECS/GLOBAL_CLASS_SPECS
      derivados do slice (§4.1). Migrar em lote, conferir contagem.
  F4  jit symbol_lookup_fn; deletar os 1104 add_fn! (§4.2). Testar AOT MSVC.
  F5  Lint: nenhum __RTS_FN_* literal fora de row; cross-check spec↔extern;
      default Str-nula = sentinela explícito, não 0 (§4.5, §4.6).

ENGINE (sobre a fundação sólida)
  E0  engine/ com RecvKind, MethodTarget (incl. UserClassMethod), MethodIntrinsic,
      resolve_method (wrapper sobre resolve_member), recv_kind (junta os
      side-channels num ponto). Testes unit vs comportamento atual. Ninguém chama
      ainda → suíte intocada.
  E1  Rotear classe-usuário + global-class pela engine (caminho que JÁ funciona).
      + oráculo de divergência (roda engine-path E path-antigo, trapa em
      divergência sobre a suíte). Regressão visível e limitada.
  E2  Number (~27 braços, zero colisão de nome). DELETA lower_number_builtin só
      com suíte verde. Template para os demais.
  E3  String (~50 braços, 34 rows já existem). Aliases/default_args via macro.
      Mantém só regex/matchAll/charCodeAt-F64 como residual.
  E4  Array/Map/Set: specs external sobre COLLECTIONS_*. Tier-3 → DISPATCH_AUTO
      (após spike §5.5). Somem os *_AUTO.
  E5  (POR ÚLTIMO, maior risco) ladder qualified-string + bare-globals. Extrair
      classificador-de-callee-shape como refactor estrutural puro PRIMEIRO
      (mesma ordem, só fatorado); dobrar só forwards mecânicos.
  E6  (opcional, perf) ValTy::Handle → Handle(HandleKind) espelhando HIR; deleta
      side-channels; class-id inteiro → vtable (mata gc.string_eq chain). Engine
      vira resolver compartilhado do MIR após HIR ganhar This + tipo-de-classe.
```

**Por que essa ordem é a estável:** F1-F5 tornam a tabela capaz de **guardar** a
semântica (overloads, aliases, variádico) e **se montar sozinha** (linkme,
symbol-lookup). Só então E0-E6 constroem a resolução em cima. Construir a engine
antes de F1-F3 = `resolve_method` sobre uma tabela que não tem overloads, não
auto-registra e re-digita símbolos = exatamente o bug-silencioso-multiplicado
que o usuário teme.

---

## 7. Invariantes (chão de honestidade + perf — nunca suspensos)

1. **100% nativo.** Receiver tipado (Tier-1/2) → `call <symbol>` direto, sem
   box/probe. Só `any` (Tier-3) paga 1 tag-check — custo que já existe hoje via
   `*_AUTO`. Não é interpretação.
2. **Hot-path numérico não regride.** `rts ir bench/monte_carlo_pi.ts` etc. não
   mostra IR novo no loop. CI compara.
3. **A row é a fonte única.** Todo símbolo emitido vem de `member.symbol`, nunca
   de um literal. Sem drift.
4. **Resolução é chaveada, não ordenada.** `(RecvKind, nome, aridade)` desambigua
   — zero "primeiro-que-retorna-Some-vence". Mata a classe inteira de bugs de
   ordem (#311/#480, string-before-map SIGILL).
5. **Migração com oráculo.** Nenhum braço de `builtins.rs` é deletado sem row
   equivalente provada (lint) e sem divergência-zero na suíte (oráculo E1).
6. **Tier-3 nunca chuta.** Receiver genuinamente desconhecido → `RuntimeAuto`,
   nunca uma família adivinhada. Falso-Unknown = call correto lento; falso-Class
   = crash. Conservador por construção.
7. **MIR fica roteado-pra-AST** em member/class até HIR ganhar This +
   tipo-de-classe (E6). A engine é AST-only até lá.

---

## 8. Relação com `docs/specs/rts-core-engine.md`

| | core-engine.md | RTS_ENGINE.md (este) |
|---|---|---|
| Foco | **registro** (mata quádrupla-escrita, object model, tier dinâmico) | **despacho** (mata o string-match de `builtins.rs`) |
| Stage 1 (macro) | feito (50 ns + 27 classes) | reusa |
| Stage 2/3 (linkme + jit) | planejado, **não feito** | **F3/F4 — pré-requisito desta engine** |
| Stage 5 (vtable) | planejado | **E6 — class-id integer** |

São o mesmo épico visto de dois ângulos. Este doc cobre a metade que o
core-engine.md deixou implícita e adiciona a auditoria de fundação (§4) que torna
a coisa toda mantível em vez de um castelo sobre arrays-à-mão.
