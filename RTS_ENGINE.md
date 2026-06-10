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

---

## 9. API de autoria rica em Rust (módulos / classes / fns / consts / variáveis)

Como declarar TODA a superfície RTS de forma rica e extensível a partir do Rust,
de modo que adicionar uma família nova (tipo `node:`) seja barato. Esta é a
camada que materializa as correções da §4.

### 9.1 Veredito: macro como superfície, linkme como montagem

**Proc-macro é a superfície de autoria** (`#[rts_module]`/`#[rts_class]`/
`#[rts_fn]`/`#[rts_var]`), **não** um builder runtime. Razão: só a macro funde,
numa declaração, (a) o `#[no_mangle] extern "C"`, (b) a metadata derivada da
assinatura (args/returns/ts), (c) o prelúdio StrPtr→ptr+len+UTF-8. Um builder
runtime ainda exigiria escrever cada extern à mão, moveria erro pra startup, e
adicionaria heap-registry — resolve módulos-dinâmicos (que RTS não quer) ao custo
da declaração-única (que RTS quer).

**`linkme::distributed_slice` é a montagem** (não a superfície). Cada
`#[rts_module]`/`#[rts_class]` se auto-registra num slice; os arrays à mão
(`SPECS`, `GLOBAL_CLASS_SPECS`, `NODE_SPECS`) + os 1104 `add_fn!` morrem.
Invariante preservado: per-member fica `'static const`; **só o agregado** vira um
`OnceLock<Registry>` montado no startup do `rts.exe` — e como o codegen lê essas
tabelas *dentro* do rts.exe em execução (no momento do lowering), o heap é do
**compilador**, não do binário emitido. Pureza nativa intacta.

> **Correção crítica (review):** o raciocínio de AOT-safety NÃO é sobre o binário
> AOT do usuário — `SPECS`/`GLOBAL_CLASS_SPECS` vivem no `rts.exe` (rts-codegen).
> O risco real é **transitividade rlib→bin**: o `rts.exe` retém as entradas
> linkme registradas no rlib `rts-runtime`? Esse é o caminho frágil do linkme,
> agravado no **MSVC/COFF** (alvo primário). → **F0 spike obrigatório** (um
> `#[distributed_slice]` trivial, build AOT MSVC, assertar que a entrada
> sobrevive) ANTES de F3/F4. Se falhar, Track A (abaixo) sobrevive sem linkme.

### 9.2 Modelo de membro — modifiers como DADO (sem struct novo)

`NamespaceMember` ganha **um** campo bitflag + **três** MemberKinds. Unifica o
mundo builtin com o de classe-usuário (que já modela readonly/private/static/
setter no `ClassMeta` do AST — `validate_visibility` members.rs:2216,
`field_is_readonly_in_hierarchy` :2269).

```rust
pub struct MemberFlags(u8);   // const-construtível, Copy
//   READONLY  — escrita = erro de codegen
//   STATIC    — static field (vs Constant read-once)
//   MUTABLE   — backing storage é var atômica (rts_var let/var)
// (PRIVATE/PROTECTED: ver 9.5 — NÃO viram flag enforçada)

pub enum MemberKind {
    Function, Constant, Constructor, InstanceMethod, StaticMethod, InstanceGetter,
    InstanceSetter,  // NOVO: fn(handle, value) -> void ; backs `inst.prop = v`
    VarGetter,       // NOVO: fn() -> T ; backs leitura `ns.v` de var mutável
    VarSetter,       // NOVO: fn(value) -> void ; backs `ns.v = e` ; ausente => read-only
}
```

Property settável de builtin = `InstanceGetter` + `InstanceSetter` mesmo `name`
(substitui os branches string-keyed `pathname`/`lastIndex` em mod.rs:591-632).
Var mutável de módulo = `VarGetter` + (opcional) `VarSetter`.

> **Correção (review):** adicionar variantes de `MemberKind` **quebra todo `match`
> exaustivo** sobre kind (signature.rs etc.) — é a *forcing function* que revela
> cada site de consumo. Tratar como passo mecânico (F2) antes de dar significado.

### 9.3 `#[rts_module("scheme:name")]` + ModuleScheme

Generaliza `#[rts_namespace]` pra aceitar o specifier completo (com `:`, scheme).
`NamespaceSpec` ganha `scheme: &'static str`. Um `ModuleScheme` (slice linkme)
torna "adicionar `node:`/`bun:`" dado, não branch:

```rust
pub struct ModuleScheme {
    pub prefix: &'static str,                                 // "rts" | "node" | "bun"
    pub exports_default: bool,
    pub resolve: fn(&str) -> Option<&'static NamespaceSpec>,
}
```
`builtin_module(spec)` (runtime.rs:21) vira `split_once(':')` → acha o scheme no
slice → resolve. O guarda-chuva bare-`rts` (RTS_EXPORTS) passa a ser **derivado**
dos specs scheme=="rts" (mata o drift hand-list). Node deixa de ter
`NodespaceSpec` paralelo — vira `NamespaceSpec{scheme:"node", alias_of:Some("fs")}`.

### 9.4 `#[rts_var(const|let|var, Type, default)]` + write-path nativo

Escolha do usuário: **global atômico do processo** + leitura `x.v` e **escrita
nativa `x.v = 5`**. Macro expande:

```rust
#[rts_var(var, I64, default = 7)] static SEED;
// gera:
static __RTS_VAR_<NS>_SEED: AtomicI64 = AtomicI64::new(7);
#[no_mangle] extern "C" fn __RTS_FN_NS_<NS>_SEED_GET() -> i64 { …load(SeqCst) }
#[no_mangle] extern "C" fn __RTS_FN_NS_<NS>_SEED_SET(v: i64)  { …store(v,SeqCst) }
// + member VarGetter (+ VarSetter se let/var ; const => só GET + flag READONLY)
```
Mapa de tipo: I64/Handle→AtomicI64, U64→AtomicU64, F64→AtomicU64(bits), Bool→
AtomicBool. Str não pode ser var (vira Handle).

- **Read** (`ns.v`): adicionar `VarGetter` ao `matches!(kind, Constant)`
  (members.rs:1006) — senão reifica como fn-handle. (NÃO é grátis, corrige o
  design.)
- **Write** (`ns.v = e`): UM braço novo no topo de `lower_assign_expr`
  (expressions/mod.rs:446, ANTES do fallback `MAP_SET` em :1193 que hoje corrompe
  silenciosamente): resolve VarSetter via registry → `call <setter>(coerce(rhs))`;
  READONLY → erro duro. O mesmo braço cobre `InstanceSetter` de builtin.
- **GC root** (var Handle): emitir thunk só pra `Type==Handle` num slice
  `GC_VAR_ROOTS`, drenado em main() **antes do 1º GC tick** em JIT **E** no
  prólogo `__RTS_MAIN` do AOT (emit.rs hoje tem **zero** global-root — path novo).

### 9.5 Enforcement vs cosmético — readonly sim, private redesenhado

- **readonly**: enforçado no write-site (braço acima erra antes de emitir call).
  Macro carimba `READONLY` em **todo getter-sem-setter** — senão readonly-by-
  omission cai no MAP_SET corrompido (correção do review).
- **`#private`/protected em builtin**: `validate_visibility` chaveia em
  `ctx.current_class`, que **nunca** é o nome da builtin durante uma chamada de
  usuário → enforçar é mecanicamente impossível. **Não** vira flag morta que
  codegen finge ler. Modelado como **"membro não exposto a TS"** (a macro
  simplesmente não emite o membro público). É a representação honesta: código TS
  do usuário nunca está "dentro" do corpo Rust da builtin.
- **d.ts**: o gerador hoje não emite membro de classe (emit_types.rs:94/123).
  Surfacing de modifiers é follow-up desacoplado do enforcement (lint byte-a-byte
  do `rts.d.ts` muda em PR separado).

### 9.6 Trilhas (separáveis — o risco não bloqueia o valor)

```
Track A (baixo risco, SEM linkme — faço primeiro)
  A1  member.rs: MemberFlags + InstanceSetter/VarGetter/VarSetter + NamespaceSpec.scheme
      + GlobalClassSpec::instance_setter. Tratar fallout de match exaustivo (mecânico).
  A2  macro: #[rts_module("scheme:name")] (thin) ; #[rts_setter]/#[rts_prop] ;
      #[rts_var(kind,Type,default)] (atomic + GET/SET + member(s)).
  A3  codegen: VarGetter read (1 linha) ; braço write-path VarSetter/InstanceSetter
      (+ readonly hard-error) ; substituir pathname/lastIndex hardcoded por InstanceSetter.
  A4  GC_VAR_ROOTS drain (JIT + AOT __RTS_MAIN), só Handle. Fixture: var read+write+readonly-reject.

Track B (gated por F0 spike MSVC/COFF)
  F0  spike: 1 #[distributed_slice] trivial, build AOT MSVC, assertar sobrevivência rlib→bin.
  B1..B3  linkme: MODULE_SPECS/CLASS_SPECS/JIT_SYMBOLS + ModuleScheme ; matar os 3 arrays +
          add_fn! ; promover drift-check pra assert release name-set. Cada array deletado = 1 commit.

Redesign  PRIVATE/PROTECTED como não-exportado (não flag enforçada).
```

Invariantes da §7 valem aqui: nada vira metadata morta (todo modifier tem
consumidor de codegen ou não existe), 100% nativo, suíte verde por passo.

---

## 10. O NOVO MODO — motor genérico + módulos externos nativos

A extensibilidade **não é um subsistema "rts-plugin" separado**. É o que o motor
**é** quando fica genérico: codegen resolve tudo por **um registry** e emite
`call` nativo, sem saber se o módulo foi compilado-dentro (builtin) ou carregado
de um `.dll`/`.so` (externo). "Plugin" = só "módulo externo registrado no mesmo
motor". Esse é o **novo modo**: nada físico exposto dentro do codegen.

### 10.1 O motor já é genérico (a aposta, verificada)

`lower_ns_call_body` (`ns_call.rs:174`) **já** é um motor: dado **qualquer**
`&NamespaceMember`, deriva a assinatura Cranelift de `member.args`/`member.returns`
(via `lower_member`/`scalar_to_cl`) e emite
`module.declare_function(member.symbol, Import) + ins().call(fref)`. **Zero**
conhecimento de qual módulo/método está hardcoded ali. A fronteira já é `extern
"C"` típado + handle `u64` opaco (= modelo N-API **sem** a camada de boxing).
A macro já funde `#[no_mangle] extern "C"` + a row + o SPEC. Logo: um membro de
módulo externo desce **byte-idêntico** a `io.print`.

### 10.2 Registry unificado (builtin + externo no mesmo lugar)

Os três arrays-à-mão (`SPECS`, `GLOBAL_CLASS_SPECS`, os 1104 `add_fn!`) viram
**um** `OnceLock<RwLock<Registry>>` em rts-codegen — heap do **compilador**
(rts.exe), lido no lowering, **não** no binário emitido (pureza nativa intacta).

```rust
struct Registry {
    modules: HashMap<&'static str, ModuleEntry>,   // "<scheme>:<name>"
    classes: HashMap<&'static str, ClassEntry>,
    jit_symbols: HashMap<&'static str, *const u8>, // symbol -> fn ptr (builtin E externo)
    _libs: Vec<Arc<PluginLib>>,                    // mantém cada .dll/.so mapeada
}
enum SpecOrigin { Builtin, External { lib: LibId } }
```
- **Builtin** popula no startup (Track A drena os const arrays; Track B linkme,
  gated pelo spike F0). `lookup`/`global_class_lookup` passam a ler o registry —
  **mesmos call-sites**.
- **Externo** popula no `LoadLibrary`/`dlopen`: o host **copia/interna** os
  descritores (string → arena `'static`), guarda só o `fn_ptr` + símbolo
  internado, e `Arc<Library>` em `_libs` mantém a imagem viva.
- Codegen **não distingue** origem. `SpecOrigin` só decide o caminho AOT
  (builtin = reloc estático; externo = slot de import) e diagnóstico — **nunca**
  ramifica o marshalling.

### 10.3 ABI externa CONGELADA (repr(C), versionada) — `rts-abi::c_plugin`

> **Verificado:** `NamespaceMember`/`NamespaceSpec`/`GlobalClassSpec`/`AbiType`/
> `MemberKind`/`MemberFlags` **não são repr(C)** (só `js_error.rs` tem
> `#[repr(u8)]`). São layout-Rust com `&'static str`/`&[T]` (fat-pointers) — **UB
> silencioso** se cruzarem `.dll` compilada por outra versão de rustc. Por isso a
> fronteira externa é uma camada **separada** repr(C); os tipos internos ficam.

```rust
// todo .dll/.so externo exporta exatamente isto:
#[no_mangle] pub extern "C" fn rts_plugin_register(host: *const RtsHost,
                                                    reg: *const RtsRegistrar) -> i32;
pub const RTS_PLUGIN_ABI_VERSION: u32 = 1;
// AbiType/MemberKind/MemberFlags como códigos u8/u32 FIXOS — nunca o discriminante Rust.
#[repr(C)] struct RtsMemberDesc {
    name: *const u8, name_len: u64, kind: u8, flags: u32,
    args: *const u8, args_len: u64, returns: u8, variadic: u8,
    fn_ptr: *const c_void,                 // o ponteiro extern "C" REAL (load-bearing)
    symbol: *const u8, symbol_len: u64,    // símbolo canônico (JIT name + AOT)
    ts_sig: *const u8, ts_sig_len: u64,
}
// + RtsHost (callbacks gc::alloc_string/buffer/vec/map, free_handle, gc_root_add/remove,
//   register_thread), RtsRegistrar (add_module/add_class), RtsModuleDesc, RtsClassDesc.
```
- `abi_version` é checado **antes** de ler qualquer descritor → mismatch falha
  limpo no load (log + skip), **nunca** crash.
- Diferencial vs N-API: o externo entrega **ponteiros típados nativos** que
  codegen `call`a direto — zero marshalling além do StrPtr/Handle que builtins já
  têm. Sem interpretador, sem boxing.

### 10.4 Autoria — a MESMA macro

O autor escreve o **mesmo** `#[rts_module]`/`#[rts_class]` de um builtin, num
crate `cdylib`, mais **uma** linha `rts_plugin_entry!{ modules=[...], classes=[...] }`
(gerada por macro). Arm `cfg(rts_plugin)` na macro emite o inventário por-crate +
os descritores. A macro **deriva o extern E o descritor da mesma assinatura Rust**
— é o que fecha o buraco do `fn_ptr` (§10.7).

### 10.5 JIT-first (real) vs AOT (greenfield pesado) — escopo honesto

- **JIT** (`rts run`/`test`): **real, dias de trabalho, zero mudança de codegen.**
  Carrega o `.dll` em rts.exe antes de compilar, injeta os `fn_ptr` em
  `JITBuilder::symbol`/`symbol_lookup_fn`; `finalize_definitions()` resolve o
  símbolo externo igual a `io.print`. Precisa: wrapper `libloading` (~50 LOC,
  **não existe nada de dynamic-loading hoje**) + o registrar + 3 linhas em
  `build_jit_module`.
- **AOT** (`rts compile`): **cliff greenfield, a maior parte da engenharia.**
  `Linkage::Import` estático **falha no link** num símbolo que só existe em
  runtime. Precisa inventar do zero: slot de import por-callee +
  **`call_indirect`** (codegen tem **ZERO** `call_indirect` hoje; replicar
  expansão StrPtr 2-slot + `declare_value_needs_stack_map`) + inicializador
  `dlopen` antes do `__RTS_MAIN` + namespace `dylib` estático no archive +
  **trap-stub em slot não-preenchido** (senão ACCESS_VIOLATION — chão de
  honestidade). MIR não faz indirect (`TailCallIndirect` unimplemented). **NO-GO
  shippar AOT** até isso existir + provado. JIT-first valida a ABI barato.

### 10.6 GC, threads, segurança — honesto, sem fingir

- **GC root-set NÃO existe hoje** (`gc_root_add`/`remove` são infra **nova** — só
  há `Entry::Function::keep_alive`). Handle que o externo segura through
  await/callback/thread sem pin é varrido (GC_TICK_INTERVAL=256). Precisa
  construir o root-set persistente antes de expor o contrato.
- **Scan cross-thread é Windows-only** (`thread_registry` — SuspendThread; Linux/
  macOS = no-op, só main thread). `register_thread` **não** torna handle de thread
  de externo seguro fora do Windows. Construir o scan portável OU escopar
  externo-com-thread a Windows + documentar.
- Externo **nunca** aloca handle próprio — só via `RtsHost.alloc_*` (HandleTable
  compartilhada). Handle é opaco (layout gen|slot|shard não é estável).
- **Segurança = honestidade, não sandbox.** Carregar `.dll` nativo = código
  arbitrário com privilégio total do processo (igual `.node` N-API). Defesas são
  **integridade**: SHA256-pin no lockfile, verificar antes de `LoadLibrary`,
  **só carregar do manifest** (`rts.json` `rtsPlugins`, nunca auto-discovery).
  `import "plugin:foo"` sem entrada no manifest = **erro de compilação duro**
  (fail closed). Desabilitar/last-priority o fallback dlsym do JIT para externos +
  namespace de símbolo reservado `__RTS_FN_PLUGIN_*` (anti-shadowing). Documentar:
  declarar um plugin = autorizar execução de código. Sem alegar contenção.

### 10.7 builtins.rs é PRÉ-REQUISITO da tese, não nota de rodapé

"Registry é o portão único" e "codegen sem hardcode" **só são verdade depois** de
E2-E4 (§4): hoje String/Array/Map/Set/Number/console/RegExp despacham por
string-match em `builtins.rs` (2258 linhas, ~182 braços), com símbolos e
assinaturas re-digitados, **fora do registry**. Array/Map/Set **nem têm spec**.
Logo um externo **não consegue** estender despacho-de-método-de-receiver do jeito
que `String.indexOf` faz — porque `String.indexOf` **também** não passa pelo
registry. Enquanto `builtins.rs` não for drenado pras rows, externos são
first-class só no caminho **namespace + global-class** (já genérico), não no de
método-de-tipo. **Must-fixes adicionais** antes de externos shipar: `fn_ptr`
by-honor → **v1 só macro-autorado** (macro deriva extern+descritor de 1
assinatura) + validar códigos AbiType no register; **pin Bool=i64** (signature.rs
lowra Bool→I64; doc de ty.rs mente "i8" → corrigir); F2 arity-keyed **antes** de
overload de externo; mover os 2 symbol-switches de `ns_call.rs` (:272 spawn-bitcast,
:314 ambiguous-ret) pra `MemberFlags` (RAW_BITS_ARG/AMBIGUOUS_RET) — senão emit
não é data-driven e externo não declara isso.

### 10.8 Ordem (compõe com F/E/A — não é fork)

```
JIT-first (real, dias):
  S1  member.rs frozen surface (A1a feito) + NamespaceSpec.scheme.
  S2  F2 resolve_member arity-keyed (pré-req de overload).
  S3  F3 Track A: Registry unificado; register_builtins() drena os const arrays. lookup lê o registry.
  S4  F4 jit symbol_lookup_fn (builtins via GetProcAddress/dlsym + registry.jit_symbols); encolhe add_fn!.
  S5  dynamic-loading: wrapper libloading (LoadLibrary/dlopen/Mach-O). ~50 LOC. Testado com .dll throwaway.
  S6  congelar rts-abi::c_plugin (repr(C) + RTS_PLUGIN_ABI_VERSION + códigos u8 fixos). Aditivo.
  S7  loader JIT: manifest + SHA256 + LoadLibrary + RtsHost + register + intern no Registry. Scheme "plugin:".
      → 1º módulo externo roda sob `rts run`. MILESTONE.
  S8  rts_plugin_entry!{} + arm cfg(rts_plugin) na macro. Plugin de referência (crc32) + teste.
AOT (greenfield, gated):
  S9  fork de call-site por SpecOrigin → import-slot + call_indirect (StrPtr 2-slot + stack map),
      inicializador dlopen antes do __RTS_MAIN, namespace dylib estático, trap-stub. NO-GO até provado.
Paralelo (pré-req da tese de genericidade):
  E2-E4  drenar builtins.rs pras rows + gc_root_set + scan cross-thread portável.
```

Cada step build+suíte verde. JIT-first reusa o caminho-nomeado já-genérico (sem
mudar codegen); AOG é subsistema novo inteiro (capacidade de dynamic-loading que
o projeto tem zero hoje) — não deixar o sucesso do JIT implicar que AOT está perto.
