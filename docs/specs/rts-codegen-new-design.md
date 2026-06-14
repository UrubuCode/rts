# Redesenho do motor `rts-codegen` — documento de design canônico

> **Status:** especificação canônica do redesenho do motor de codegen (engine
> TypeScript/JavaScript → código nativo, backend Cranelift). É O documento de
> referência: o time e os agentes futuros seguem **isto**. O crate vivo do
> redesenho é `crates/rts-codegen-new/`; o motor antigo, congelado, é
> `crates/rts-codegen-old/`. Enquanto a migração strangler-fig não termina, o
> `bin`/`cli` continuam plugados no `rts-codegen-old`.
>
> **Idioma:** pt-BR. **Tese:** *provar-monomórfico-e-desembrulhar onde o sistema
> de tipos consegue (preservando o caminho numérico vencedor); cair para UMA
> representação tagueada honesta dentro do valor + shapes + inline-caches de dado
> seguras para AOT onde não consegue.*

---

## Índice

1. [Contexto histórico — a verdade sobre os 100%](#1-contexto-histórico--a-verdade-sobre-os-100)
2. [O que o motor antigo realmente é (os problemas a corrigir)](#2-o-que-o-motor-antigo-realmente-é-os-problemas-a-corrigir)
3. [O que é genuinamente bom (preservar intacto)](#3-o-que-é-genuinamente-bom-preservar-intacto)
4. [A nova tese e os mapeamentos para os módulos do crate](#4-a-nova-tese-e-os-mapeamentos-para-os-módulos-do-crate)
5. [Pilar 1 — PolyValue (`value.rs`): um valor NaN-boxed de 64 bits](#5-pilar-1--polyvalue-valuers-um-valor-nan-boxed-de-64-bits)
6. [Pilar 2 — Lattice de representação (`repr.rs`)](#6-pilar-2--lattice-de-representação-reprrs)
7. [Pilar 3 — Regra de solidez + confiança nos tipos TS](#7-pilar-3--regra-de-solidez--confiança-nos-tipos-ts)
8. [Pilar 4 — Shapes (`shape.rs`) + inline caches de dado (`ic.rs`)](#8-pilar-4--shapes-shapers--inline-caches-de-dado-icrs)
9. [Pilar 5 — Caminho único de lowering (`lower/`), sem MIR](#9-pilar-5--caminho-único-de-lowering-lower-sem-mir)
10. [Pilar 6 — Dispatch data-driven (`dispatch.rs`) + ABI gerada (`abi_gen.rs`)](#10-pilar-6--dispatch-data-driven-dispatchrs--abi-gerada-abi_genrs)
11. [Por que isto é mais simples que o V8 (e o custo honesto)](#11-por-que-isto-é-mais-simples-que-o-v8-e-o-custo-honesto)
12. [Plano de migração strangler-fig](#12-plano-de-migração-strangler-fig)
13. [O que é deletado do motor antigo (por nome)](#13-o-que-é-deletado-do-motor-antigo-por-nome)
14. [Apêndice — mapa de módulos do crate](#14-apêndice--mapa-de-módulos-do-crate)

---

## 1. Contexto histórico — a verdade sobre os 100%

Em 2026-06-06/07 o RTS atingiu **100% de paridade cross-runtime** (372/372, 0
divergências; suíte TS 1719/1719), tag `v0.0-202606072107`, commit `27e16378`.
Isto é factual e verificável no git. **Mas é importante dizer honestamente como
foi atingido:** sobre o **motor antigo**, moendo casos especiais hardcoded
dentro de arquivos gigantescos — a *mesma* arquitetura que ele tem hoje:

| Arquivo (`rts-codegen-old`)                                    | LOC    |
|----------------------------------------------------------------|--------|
| `src/codegen/lower/expressions/calls/mod.rs`                   | 4622   |
| `src/codegen/lower/passes/parallelism.rs`                      | 3189   |
| `src/codegen/lower/passes/this_arrow.rs`                       | 2810   |
| `src/codegen/lower/expressions/operators.rs`                   | 2616   |
| `src/codegen/lower/expressions/members.rs`                     | 2592   |
| `src/codegen/jit.rs`                                           | 2784   |

Os 100% foram o **máximo local da abordagem hardcoded**, não validação do
design. O próprio `MAINTENANCE.md` do motor antigo, no commit dos 100%, admitia
a parede com todas as letras:

> *"um bool perde a tag ao cruzar uma fronteira de função porque parâmetros/
> retornos são `i64` e `true == 1`. Taguear bools como sentinelas foi TENTADO e
> REVERTIDO — quebra 83 testes TS. Precisa de um refactor real de
> rastreamento-de-tipo-bool, não de um hack de sentinela."*

Isto é o **problema de representação de valor**, auto-identificado no pico.

Depois dos 100%, o conjunto de fixtures cresceu de 391 → 612 (casos mais
difíceis) e o badge caiu para 70.7%. Nessa janela, 126 commits de codegen
drenaram lógica hardcoded em direção ao Registry (**bom**) — mas também
adicionaram `Entry::FloatPrim` ("narrow-storage"), reintroduzindo *boxing*
(**direção errada**, vide §2.2). O redesenho existe para que o próximo platô não
seja outro máximo local de hacks.

---

## 2. O que o motor antigo realmente é (os problemas a corrigir)

### 2.1 Representação de valor: um slot `i64` sobrecarregado

Há **um** slot de ABI `i64` que significa, a depender do contexto:
`{ int puro, handle GC, float boxed, handle de string, sentinela
undefined/null/bool = i64::MIN + k }`. A "type tag" **não foi eliminada** — foi
*espalhada* em quatro side-tables de tempo de compilação dentro de um `FnCtx`
com **93 campos** (`crates/rts-codegen-old/src/codegen/lower/ctx.rs`):

```rust
// ctx.rs — as quatro side-tables (linhas 498/503/509/514)
pub fresh_handle_set:        HashSet<cranelift_codegen::ir::Value>,
pub optional_chain_values:   HashSet<cranelift_codegen::ir::Value>,
pub var_member_call_values:  HashSet<cranelift_codegen::ir::Value>,
pub var_vec_slot_values:     HashSet<cranelift_codegen::ir::Value>,
```

Mais heurísticas de forma-da-AST (`is_map_get_call`), mais helpers de re-tag em
runtime (`__RTS_FN_RT_FLOAT_BOX/UNBOX/EQ_AMBIG/NUM_ARITH`).

É **insólido por construção**: um novo acessor de container que *esqueça* de
registrar um `ir::Value` na side-table certa silenciosamente mis-coerce o valor
— **resultado numérico errado e silencioso**, o pior modo de falha possível
(nem crash, nem diagnóstico).

### 2.2 `Entry::FloatPrim` re-boxa floats

Para caber em `Map<String,i64>` / `Vec<i64>`, floats fracionários são re-boxados.
O próprio doc-comment em `crates/rts-engine/src/heap/handles.rs:485` confessa:

> *"`FloatPrim` é um número primitivo cujo bits f64 não cabem no i64 do container
> sem ambiguidade — então é boxed e o read-back (typeof/===/arith/INSPECT) o
> desembrulha como NÚMERO primitivo."*

"Sem boxing" é falso uma camada abaixo. E **não escala**: cada novo tipo
armazenável-em-container precisaria do seu próprio quarteto BOX/UNBOX/EQ/ARITH.
Hoje o `Entry` já carrega `String`, `FloatPrim`, `StringBox`, `NumberBox`, … —
cada um com seu pequeno zoológico de re-tag.

### 2.3 Objetos: dicionário V8 como *default*

O default de um objeto é `HashMap<String,i64>` + links `__proto__` + uma tag de
classe em string. Isto é o **modo dicionário/slow-mode do V8 transformado em
default**. O `members.rs` (2592 LOC) então faz à mão ~30 caminhos de tempo de
compilação para *desviar* do hashmap = otimização de hidden-class **sem uma
hidden class**. Um layout de struct real ("flat") existe, mas é **gated por
variável de ambiente** — ou seja, o caminho rápido é o exceção, não a regra.

### 2.4 Dispatch virtual: comparação linear de strings

Override de método é resolvido por **O(N) alocações de string-literal +
`gc.string_eq` por call site por override**. É um inline-cache megamórfico
implementado como *comparação linear de strings*.

### 2.5 Dois tiers de otimizador

`HIR → MIR (SSA de 84 instruções; passes fold/cse/dce/fma/narrow/inline) →
Cranelift`. Os passes de MIR **re-fazem** o que a egraph do Cranelift
(`use_egraphs=true`, setado em `emit.rs:91` e `jit.rs:97`) já faz. O próprio
`crates/rts-mir/src/passes/fold.rs:16` admite:

> *"Float folding is intentionally omitted — Cranelift's e-graph pass with
> `use_egraphs=true` already covers it intraprocedurally."*

Pior: o MIR só aceita um *whitelist* numérico; **~99% do JS real faz bail
silencioso** para um caminho **separado e completo** AST→Cranelift. São **dois
codegens completos** mantidos em paralelo.

### 2.6 `guards.rs` é código morto

`crates/rts-engine/src/abi/guards.rs::guard_for` — a suposta autoridade de
coerção de argumentos `any` — tem **zero call sites de produção**. As únicas
referências a `guard_for` são a própria definição (linha 45) e três chamadas
*nos testes do próprio arquivo* (linhas 64/72/80). A coerção real é ad-hoc:
`TPL_COERCE_AUTO` espalhado por 12 ocorrências em `operators.rs` e dezenas de
outros arquivos.

### 2.7 `jit.rs`: 1113 `add_fn!` manuais

`crates/rts-codegen-old/src/codegen/jit.rs` registra **exatamente 1113** símbolos
de runtime à mão (`add_fn!`). Um rename → *link OK* + **SIGILL por mismatch de
ABI em runtime**, sem nenhuma verificação em tempo de build. É uma classe inteira
de bugs latentes que o compilador não pega.

### 2.8 Duplicação no switchboard

Em `calls/mod.rs`: lógica de `JSON.stringify` aparece ~5× duplicada, `Math.max`
2×, listas hardcoded de `console.*`. O switchboard de 4622 LOC é o coração do
problema arquitetural: **builtins no motor**, em vez de metadados no Registry.

---

## 3. O que é genuinamente bom (preservar intacto)

### 3.1 O caminho numérico monomórfico

Primitivas `extern "C"` planas (`AbiType` = enum de 8 variantes; `StrPtr` é o
único caso de 2 slots), inline de intrínsecos (`sqrt`/`abs`/`min`/`max` como IR
Cranelift direta), e a **egraph do Cranelift como o otimizador real**. Métricas:
Monte Carlo ~5× Bun, AOT 16.9 ms. **NÃO TOCAR.** Este caminho é o produto.

### 3.2 O Registry / doutrina PRIMORDIAL

O motor nomeia *diretamente* APENAS as classes primordiais (`String`, `Object`,
`Array`, `Function`, `Promise`, `Boolean`, `Number`, `Error` + subclasses).
Todo o resto resolve via a **Registry real** (`registry.rs` constrói de
`Engine::new()` + as `register`/`register_class_spec` fns; `registry_call.rs` é o
marshal genérico a partir dos `AbiType` do `Member`) → **um único INVOKE
genérico**. Correto e escalável.

**A linha divisória é SINTAXE NATIVA (clarificação binding do owner):**
- **Sintaxe nativa ⇒ PRIMITIVO ⇒ codegen-direto (rts-primitives):** literais
  `""`/`123`/`true`/`[]`/`{}`/função/**`/re/` (RegExp tem sintaxe nativa → é
  primitivo, NÃO Registry)**/template, + `Error` (primordial). O motor nomeia +
  lowera a sintaxe direto; impl em `rts-primitives`.
- **Sem sintaxe nativa ⇒ lib utilitária rts-shared ⇒ Registry, indireto:**
  `Date`/`Map`/`Set`/`WeakMap`/`JSON`/`URL`/`Math`/`Promise`/`Proxy`/typed-arrays/
  backend — acessadas via `new X()`/estáticos, despachadas pela Registry, **nunca
  reimplementadas como tabelas codegen `__rtsadp_*`**. `Date` é a migração
  referência (feita); `Map`/`Set` são os próximos.

### 3.3 O instinto `ValTy`

Uma tag semântica de tempo de compilação separada do tipo de máquina. O
redesenho **generaliza** isto no lattice de representação (`repr.rs`).

### 3.4 Cache de objeto SHA256 + `compile_program` compartilhado

`crates/rts-codegen-old/src/cache.rs` faz cache por `file_sha256` +
`compiler_fingerprint` (com invalidação por dep transitiva). O `compile_program`
é compartilhado por JIT e AOT (`FnCtx.module = &mut dyn Module`). Ambos
preservados em `pipeline.rs`.

---

## 4. A nova tese e os mapeamentos para os módulos do crate

> **Identidade:** *provar-monomórfico-e-desembrulhar onde o sistema de tipos
> consegue (preservando o caminho numérico vencedor); cair para UMA representação
> tagueada honesta dentro do valor + shapes + inline-caches de dado seguras para
> AOT onde não consegue.*

| Pilar | Módulo do crate                              | Substitui no motor antigo |
|-------|----------------------------------------------|---------------------------|
| 1. PolyValue | `crates/rts-codegen-new/src/value.rs` | as 4 side-tables, `Entry::FloatPrim`, os helpers `FLOAT_*` |
| 2. Lattice de repr | `crates/rts-codegen-new/src/repr.rs` | `ValTy` + heurísticas de forma-da-AST |
| 3. Solidez + confiança TS | `repr.rs` + `lower/` + `guards` | `TPL_COERCE_AUTO` espalhado, `guards.rs` morto |
| 4. Shapes + ICs | `shape.rs` + `ic.rs` | `HashMap<String,i64>` default, dispatch por `gc.string_eq` |
| 5. Lowering único | `lower/mod.rs` + `pipeline.rs` | o tier MIR e o codegen AST duplicado |
| 6. Dispatch + ABI gerada | `dispatch.rs` + `abi_gen.rs` | switchboard `calls/mod.rs`, os 1113 `add_fn!` |

Cada pilar tem sua seção abaixo, concreta e construível.

---

## 5. Pilar 1 — PolyValue (`value.rs`): um valor NaN-boxed de 64 bits

> Módulo: `crates/rts-codegen-new/src/value.rs` (referenciado por `lib.rs` como
> `value::PolyValue`; é o **Incremento 1** do crate). Nota: no momento da escrita
> deste doc, `value.rs` ainda não existe no disco — `lib.rs` já o declara
> (`pub mod value;`) e esta seção é a especificação que o implementa.

### 5.1 O layout de bits exato

Um `PolyValue` é **um único `u64`**. A ideia (NaN-boxing) explora que o IEEE-754
double tem um grande espaço de bit-patterns NaN que nenhum double "real" produz
após canonicalização. Reservamos o quadrante qNaN-negativo para valores boxed;
tudo fora dele é um `f64` inline genuíno.

```text
PolyValue (u64)
═══════════════════════════════════════════════════════════════════════════════
  BOX_BASE = 0xFFF8_0000_0000_0000   ← qNaN negativo: o "espaço boxed"

  boxed   ⟺  (bits & BOX_BASE) == BOX_BASE
  inline  ⟺  caso contrário → é um f64 real (reinterpret_cast direto)

Quando boxed:
  bit  63        : 1  (sinal — parte do BOX_BASE)
  bits 62..51    : 1…1 (expoente todo-1 + bit alto da mantissa — BOX_BASE)
  bits 50..48    : TAG (3 bits)
  bits 47.. 0    : PAYLOAD (48 bits)

TAG (bits 50..48):
  0  reservado (símbolo — futuro)
  1  INT32        payload = i32 (zero-extended em 48 bits; sinal no bit 31)
  2  SINGLETON    payload = qual singleton (undefined/null/false/true/hole/empty)
  3  STR          payload = slot da HandleTable (string GC)
  4  OBJECT       payload = slot da HandleTable (objeto com shape)
  5  FUNCTION     payload = slot da HandleTable (Function)
  6  reservado (bigint — futuro)
  7  reservado
```

Definição em Rust (a forma canônica que `value.rs` exporta):

```rust
/// Um valor JS de 64 bits NaN-boxed. Inline f64 OU um boxed tagueado.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PolyValue(pub u64);

pub const BOX_BASE: u64 = 0xFFF8_0000_0000_0000;
const TAG_SHIFT: u32 = 48;
const TAG_MASK:  u64 = 0x7;                       // 3 bits
const PAYLOAD_MASK: u64 = 0x0000_FFFF_FFFF_FFFF;  // 48 bits

#[repr(u64)]
pub enum Tag { Symbol=0, Int32=1, Singleton=2, Str=3, Object=4, Function=5, BigInt=6 }

#[repr(u64)]
pub enum Singleton { Undefined=0, Null=1, False=2, True=3, Hole=4, Empty=5 }
```

### 5.2 Canonicalização de NaN — por que doubles reais nunca colidem

A única forma de um `f64` genuíno cair no espaço boxed seria ser, ele próprio, um
qNaN negativo. `from_f64` canonicaliza **todo** NaN para o qNaN *positivo* antes
de armazenar:

```rust
impl PolyValue {
    pub fn from_f64(x: f64) -> PolyValue {
        if x.is_nan() {
            // qNaN canônico POSITIVO — fora de BOX_BASE por construção.
            PolyValue(0x7FF8_0000_0000_0000)
        } else {
            PolyValue(x.to_bits())
        }
    }

    pub fn is_boxed(self) -> bool { (self.0 & BOX_BASE) == BOX_BASE }

    pub fn as_f64(self) -> f64 {
        debug_assert!(!self.is_boxed());
        f64::from_bits(self.0)
    }
}
```

Resultado: doubles reais (inclusive `±Infinity`, `-0.0`, e o NaN canônico
positivo) são **disjuntos** do espaço boxed. `NaN === NaN` continua `false` em JS
— isto é semântica de `===`, tratada no lowering, não no bit-pattern.

### 5.3 box / unbox como operações puras de Cranelift

Tudo isto vira IR pura que a egraph dobra:

```rust
// tag(v): extrai os 3 bits de tag (só faz sentido se boxed).
pub fn tag(self) -> u64 { (self.0 >> TAG_SHIFT) & TAG_MASK }

// box_int32(i): INT32 boxed.
pub fn box_int32(i: i32) -> PolyValue {
    PolyValue(BOX_BASE | ((Tag::Int32 as u64) << TAG_SHIFT) | (i as u32 as u64))
}

// unbox_int32(v): payload de volta em i32.
pub fn unbox_int32(self) -> i32 { (self.0 & PAYLOAD_MASK) as u32 as i32 }

// box_handle(tag, slot): STR/OBJECT/FUNCTION boxed apontando para um slot.
pub fn box_handle(tag: Tag, slot48: u64) -> PolyValue {
    PolyValue(BOX_BASE | ((tag as u64) << TAG_SHIFT) | (slot48 & PAYLOAD_MASK))
}
pub fn slot(self) -> u64 { self.0 & PAYLOAD_MASK }
```

No lowering, cada um destes é uma sequência curta de `bitcast` / `band` / `bor` /
`icmp` / `select` — **operações que a egraph do Cranelift dobra**. Um
`box(unbox(x))` redundante some no otimizador (é exatamente por isso que box/
unbox precisam ser IR pura, não chamadas extern — vide pilar 5).

`typeof v` vira **uma única inspeção de tag**:

```text
typeof:
  !is_boxed(v)            → "number"          (f64 inline)
  tag == INT32            → "number"
  tag == SINGLETON:
      Undefined           → "undefined"
      Null                → "object"          (o famoso bug-feature de JS)
      False/True          → "boolean"
  tag == STR              → "string"
  tag == OBJECT           → "object"  (ou "function" se for callable — checa shape)
  tag == FUNCTION         → "function"
```

### 5.4 GC-safety: o payload é um *slot index*, não um ponteiro

Esta é a propriedade que torna o NaN-boxing seguro para o GC preciso do RTS. O
`HandleTable` do `rts-engine` já codifica handles assim
(`crates/rts-engine/src/heap/handles.rs:3-9`):

```text
Handle u64 do HandleTable existente:
  [63..48] generation (16 bits)
  [47.. 5] per-shard table slot (43 bits)
  [ 4.. 0] shard index (5 bits)
```

Os **48 bits inferiores** (slot + shard) cabem **exatamente** no `PAYLOAD_MASK`
de 48 bits do PolyValue. As referências ao heap são **índices de slot**, nunca
ponteiros crus. Consequência: mesmo que o GC mova/realoque o backing store de um
slot, o PolyValue não precisa mudar — ele carrega o índice, não o endereço.

**Mudança requerida no GC (`rts-engine`):** o scanner conservativo de stack
(`gc/collector.rs`, hoje varre palavras procurando handles `u64`) precisa
aprender a **reconhecer uma palavra boxed-handle** e extrair o slot. A regra:
uma palavra `w` na stack é uma raiz potencial se `(w & BOX_BASE) == BOX_BASE` E
`tag(w) ∈ {STR, OBJECT, FUNCTION}`; nesse caso a raiz é `slot(w)`. Inteiros
inline, floats inline e singletons **não** são raízes (não referenciam heap).
Isto é mais preciso que hoje — palavras que *parecem* handles mas são floats
deixam de ser falsos-positivos.

### 5.5 A decisão dos bits de geração (honestamente)

O payload de 48 bits carrega **apenas o slot** (slot + shard); os 16 bits de
geração do handle existente ficam **acima do bit 48** e **não cabem** no
PolyValue boxed. A decisão de design:

- **A geração é validada no lado do slab**, não embutida no PolyValue. Quando o
  runtime resolve um PolyValue OBJECT/STR/FUNCTION para o `Entry`, ele acessa o
  slot e a geração corrente do slab.
- **Um PolyValue vivo mantém o slot alcançável.** O scanner de stack marca o
  slot (§5.4), logo o sweep não libera aquele slot enquanto o PolyValue está
  vivo. Portanto **uma leitura de geração-stale não pode ocorrer para valores
  vivos**: o slot não é reciclado debaixo de um PolyValue que ainda o referencia.
- **Caveat WeakRef/FinalizationRegistry:** referências fracas, por definição,
  *não* mantêm o slot vivo. Para elas a geração importa — uma WeakRef deve
  guardar `(slot, generation)` completos (64 bits, fora do PolyValue) e checar a
  geração na deref. Isto é correto e esperado: WeakRefs são o único lugar onde
  uma geração-stale é semanticamente observável, e o RTS já trata WeakRef como
  caso especial (issue #217). PolyValue cobre o caso forte (a esmagadora
  maioria); WeakRef carrega o handle de 64 bits completo.

### 5.6 O que isto deleta

- As **4 side-tables** (`fresh_handle_set`, `optional_chain_values`,
  `var_member_call_values`, `var_vec_slot_values`) — a tag agora vive *no valor*.
- `Entry::FloatPrim` — floats fracionários cabem inline no PolyValue (são f64
  inline) e em containers de `PolyValue`.
- O zoológico `__RTS_FN_RT_FLOAT_BOX/UNBOX/EQ_AMBIG/NUM_ARITH` — box/unbox são IR
  pura; igualdade e aritmética operam sobre tags, não sobre helpers de re-tag.

---

## 6. Pilar 2 — Lattice de representação (`repr.rs`)

> Módulo: `crates/rts-codegen-new/src/repr.rs` (já existe, esqueleto pronto).

### 6.1 O enum e a regra de join

```rust
pub enum Repr {
    Int32,            // i32 desembrulhado num registrador i64 (small-int fast path)
    Float64,          // f64 desembrulhado num registrador f64 (o caminho vencedor)
    Bool,             // 0/1 desembrulhado
    Ref(RefKind),     // handle GC de kind estaticamente conhecido (ainda slot, não ptr)
    Tagged,           // desconhecido / união / any → o PolyValue uniforme
}
pub enum RefKind { Str, Object, Array, Function, Registered }
```

A regra central, **total e decidível**:

```rust
pub fn join(self, other: Repr) -> Repr {
    if self == other { self } else { Repr::Tagged }
}
```

Toda a solidez deriva disto: **todo `ir::Value` tem exatamente UM `Repr`**.
Onde dois braços discordam, a representação **widena para `Tagged`** (o
PolyValue). box/unbox são **nós de IR explícitos** inseridos nas fronteiras
provadas — uma **função TOTAL da IR**, em contraste direto com a abordagem de
side-table do motor antigo, onde a tag era "rastreada em outro lugar" e podia
dessincronizar.

### 6.2 Onde os valores ficam desembrulhados

Um valor é mantido `Int32`/`Float64`/`Bool`/`Ref` (em registrador, sem box)
**somente** onde o front-end **PROVA** monomorfismo, a partir de:

- **literais** (`42` → `Int32`, `3.14` → `Float64`, `true` → `Bool`, `"x"` →
  `Ref(Str)`);
- **anotações TS validadas em fronteiras não-confiáveis** (vide pilar 3);
- **fluxo local** (resultado de aritmética provada numérica continua numérica).

### 6.3 Totalidade nos pontos difíceis (nenhum vaza para "rastreado em outro lugar")

A regra de join precisa ser honesta justamente onde o motor antigo trapaceava
com side-tables. Para cada ponto difícil, a representação é decidida *na IR*:

- **Phis de cabeçalho de loop:** o `Repr` do phi é o join de *todos* os
  predecessores (entrada + back-edge). Se o back-edge pode produzir `Tagged`, o
  phi é `Tagged` desde o início do loop — não há "promoção tardia". Isto exige
  um fixpoint barato sobre o CFG do loop antes de escolher a representação do
  cabeçalho (uma passada; o lattice tem altura 2, então converge imediato).
- **Exceções vinculadas no catch:** o binding do `catch (e)` é sempre `Tagged`
  (uma exceção pode ser qualquer valor). Sem exceção.
- **Bindings de destructuring:** cada binding recebe o `Repr` do elemento/
  propriedade-fonte; se a fonte é um container de `PolyValue`, o binding é
  `Tagged` (a menos que provado monomórfico por anotação validada).
- **Vars capturadas por closure:** o environment-record guarda `PolyValue`
  (`Tagged`) por padrão; só desembrulha se *toda* leitura E escrita da var
  capturada concordam num `Repr` monomórfico (análise de captura). O default
  conservador é `Tagged` — correto, nunca silenciosamente errado.
- **Estado de generator:** a state-machine do generator persiste `PolyValue` em
  seus slots (`Tagged`); valores são desembrulhados *após* o resume, no ponto de
  uso, se provados.

Em **nenhum** desses casos existe um "this value is secretly not-a-pure-int"
guardado num `HashSet`. A representação é uma propriedade da IR, ponto.

---

## 7. Pilar 3 — Regra de solidez + confiança nos tipos TS

### 7.1 O problema: tipos TS são insólidos

Um parâmetro `: number` **pode** receber uma string vinda de JS não-tipado ou de
um `any`. Confiar cegamente na anotação e desembrulhar como `Float64` produz
exatamente o bug silencioso do motor antigo, com outra roupa.

### 7.2 A regra única

> **Desembrulhe com base numa representação PROVADA no ponto; e INSIRA checks de
> runtime nas fronteiras não-confiáveis.**

Fronteiras não-confiáveis (onde um check de tag é inserido antes de desembrulhar):

- parâmetros de **funções exportadas** / pontos de entrada públicos;
- valores de tipo **`any`**;
- resultados de **`JSON.parse`**;
- resultados de **chamadas externas / resolvidas pelo Registry** (a fronteira
  retorna `PolyValue`);
- qualquer fronteira onde o compilador não consegue provar o produtor.

Dentro de uma região provada (resultado de aritmética numérica, literal, var
local com fluxo monomórfico), **nenhum check** — é o caminho rápido vencedor.

### 7.3 O `+` polimórfico — sem adivinhar pela forma da AST

`a + b`:

- **ambos provados número** (`Int32`/`Float64`, ou ambos desembrulháveis sem
  check) → `iadd`/`fadd` nativo. **Caminho rápido, custo zero.**
- **caso contrário** → **UM** `ADD_GENERIC(PolyValue, PolyValue) -> PolyValue`
  que roda o **algoritmo real do `+` de JS** (`ToPrimitive` em ambos; se algum
  vira string → concatenação; senão `ToNumber` + soma). **Nunca** adivinhação por
  forma-da-AST (`is_map_get_call` e amigos morrem aqui).
- **fast path inline para o caso secretamente-monomórfico:** antes de chamar
  `ADD_GENERIC`, o lowering emite um check de tag barato inline — se ambos os
  PolyValues são INT32/float inline, soma na hora; só cai no genérico se as tags
  discordam. Isto recupera performance quando o tipo estático falhou em provar
  mas o runtime é, de fato, numérico.

O mesmo padrão (fast-path inline + genérico honesto) vale para `===`, `<`,
`==`, etc.

### 7.4 `guards.rs` vira real (ou é substituído)

A autoridade **única** de coerção passa a existir de verdade. `guard_for`
(`crates/rts-engine/src/abi/guards.rs`) ou é promovido ao caminho real (todas as
inserções de coerção/check passam por ele) ou é substituído por um módulo
equivalente em `rts-codegen-new`. O que **não** pode continuar: `guard_for` como
código morto com `TPL_COERCE_AUTO` espalhado por 16 arquivos fazendo a coerção
de verdade ad-hoc. **Uma autoridade, um lugar.**

---

## 8. Pilar 4 — Shapes (`shape.rs`) + inline caches de dado (`ic.rs`)

> Módulos: `crates/rts-codegen-new/src/shape.rs` + `crates/rts-codegen-new/src/ic.rs`
> (esqueletos prontos).

### 8.1 Hidden classes (shapes)

Um objeto é `{ shape_id, slots: [PolyValue; N] }`. A `Shape` é a layout
compartilhada por todos os objetos construídos do mesmo jeito:

```rust
pub struct Shape {
    pub id: ShapeId,
    pub slots: HashMap<String, SlotIdx>,        // nome → índice de slot inline
    pub transitions: HashMap<String, ShapeId>,  // árvore de transição (add-property)
    pub proto: Option<ShapeId>,                 // shape do prototype (proto ICs)
}
```

- **Acesso a propriedade** = comparar `shape_id` + load em offset fixo
  (`slots[slot_of(shape, key)]`). Não é hash lookup.
- **Construção de objeto** caminha a **árvore de transição**: `{}` → add `"x"` →
  add `"y"` produz uma cadeia determinística de shapes; dois objetos com a mesma
  sequência de chaves **compartilham** o mesmo shape final.
- **Dispatch de método** é chaveado na *classe do shape*, não numa cadeia de
  `gc.string_eq`.

Isto substitui o **default** `HashMap<String,i64>` (§2.3). O layout flat passa a
ser o **default**, não gated por env-var.

### 8.2 Inline caches de dado — seguras para AOT, sem self-modifying code

ICs clássicas do V8 **patcham código de máquina** em runtime. Com object-files
AOT do Cranelift não dá para auto-modificar código portavelmente. Logo uma IC do
RTS é uma **célula de dado** que o código emitido carrega e checa:

```text
Site de acesso  obj.x  com uma PropIcCell adjacente (segmento de dado gravável):

    sid = load obj.shape_id
    if  sid == cell.shape           ; um icmp num u32 carregado
        v = load obj.slots[cell.slot]   ; fast path: offset fixo
    else
        v = slow_path(obj, "x", &cell)  ; resolve via shape, ATUALIZA a cell
```

```rust
#[repr(C)]
pub struct PropIcCell {
    pub shape: ShapeId,   // shape esperado
    pub slot:  SlotIdx,   // offset do slot
    pub state: u32,       // discriminante de IcState, mutado pelo slow path
}
```

A guarda é um `icmp` sobre um `u32` carregado; a célula vive num segmento de dado
gravável → **funciona idêntico para JIT e AOT**. É a simplificação que mantém o
motor enxuto enquanto transforma lookup megamórfico de string em
*pointer-compare*.

### 8.3 Máquina de estados da IC

```text
Uninit ──(1ª shape vista)──▶ Mono{shape,slot}
   │                              │
   │                              ├─(mesma shape)──▶ fast path
   │                              └─(shape nova)───▶ Poly (tabela inline pequena, K shapes)
                                                        │
                                                        └─(K excedido)──▶ Mega (sempre chama o resolver genérico)
```

`uninit → mono → poly → mega`. Substitui **ambos**: o property-bag por hashmap
(default) e o dispatch O(N) por comparação de string.

### 8.4 Modo dicionário só para patológicos

Cai para dicionário (`HashMap`) **apenas** em objetos patológicos: chaves
computadas em massa, mapas gigantes, `delete` frequente. O caminho comum nunca
toca um hashmap.

### 8.5 A linha de simplicidade (mantida deliberadamente)

Shapes + ICs de dado mono/poly/mega + árvore de transição + fallback dicionário.
**E SÓ.** Explicitamente **fora de escopo**:

- **NÃO** há deopt especulativo.
- **NÃO** há on-stack replacement (OSR).
- **NÃO** há grafo de invalidação de código-dependente.
- **NÃO** há deprecação de hidden-class.

Estas são as fontes de complexidade do V8 que o RTS escolhe *não* pagar (§11).

---

## 9. Pilar 5 — Caminho único de lowering (`lower/`), sem MIR

> Módulo: `crates/rts-codegen-new/src/lower/mod.rs` (esqueleto: `lower_function()`).

### 9.1 Um caminho, não dois

`HIR → Cranelift IR`, direto. A egraph do Cranelift (`use_egraphs=true`) é o
**ÚNICO** otimizador. O motor antigo tinha **dois codegens completos** (o "AST
authoritative" e o `HIR→MIR→Cranelift` que re-fazia a egraph e caía no AST para
~99% do JS real). Aqui há **um**.

### 9.2 O trabalho exato do front-end na IR

A única responsabilidade do front-end é o que o Cranelift **genuinamente não sabe
fazer** (semântica de JS), e nada além disso:

- **coerções JS-semânticas:** `ToNumber` / `ToString` / `ToBoolean`;
- **resolução do `+` polimórfico** (pilar 3);
- **inserção de box/unbox** (pilar 1) — como IR pura, para a egraph dobrar;
- **emissão dos sites de shape/IC** (pilar 4);
- **semântica de wrap de int estreito** (i8/u8/i16/u16 — o que `narrow.rs` fazia);
- **arestas de exceção** (edges de try/catch).

**Todo o resto é delegado à egraph do Cranelift:** const-fold, CSE, DCE, FMA,
strength reduction, inlining intraprocedural. Isto deleta o tier MIR redundante e
o codegen AST duplicado (~3000 LOC).

### 9.3 Por que box/unbox precisam ser IR pura (não extern call)

Se `box`/`unbox` fossem chamadas extern, a egraph não conseguiria ver através
delas e um `box(unbox(x))` redundante sobreviveria. Como são `bitcast`/`band`/
`bor`/`select` puros (pilar 1, §5.3), a egraph **dobra o par redundante** — o
custo do PolyValue some exatamente nos lugares onde a representação já era
monomórfica de fato. Esta é a razão técnica de o pilar 1 e o pilar 5 estarem
acoplados.

---

## 10. Pilar 6 — Dispatch data-driven (`dispatch.rs`) + ABI gerada (`abi_gen.rs`)

> Módulos: `crates/rts-codegen-new/src/dispatch.rs` + `crates/rts-codegen-new/src/abi_gen.rs`.

### 10.1 Todo método não-primordial é um `MethodSpec`

O motor nomeia diretamente APENAS primordiais. Todo o resto (`Map`/`Set`/`Date`/
`RegExp`/`console`/`JSON`/`Math`/…) é metadado `MethodSpec` (nome, aridade/
overloads, coerções de argumento, símbolo, intrínseco opcional) resolvido por
**UM** caminho genérico:

```rust
pub enum Target {
    Intrinsic(&'static str),  // inline como IR Cranelift nativa (spec marca intrínseco)
    Extern(&'static str),     // emit de um `call` typed ao símbolo extern (caminho genérico)
    ShapeMethod,              // dispatch via shape/IC (método de objeto de usuário)
}

pub fn resolve_method(recv_kind: &str, method: &str, argc: usize) -> Option<Target> {
    // dirigido inteiramente por SPECS / GLOBAL_CLASS_SPECS — zero special-case por método
}
```

Dispatch sobre um `PolyValue`: lê a tag → kind do heap → tabela de métodos do
kind (primordial: direto; registrado: lookup no Registry) → emit. Intrínsecos
(`sqrt`/`abs`/`min`/`max`) ainda inlinам como IR Cranelift quando o spec marca.

Isto deleta o switchboard `calls/mod.rs` de 4622 LOC: o `JSON.stringify`
5×-duplicado, o `Math.max` 2×, as listas hardcoded de `console.*` — tudo vira uma
entrada de metadado.

### 10.2 ABI gerada — matando a classe link-OK/SIGILL

Os **1113** `add_fn!` manuais de `jit.rs` são **DERIVADOS** dos mesmos `SPECS`
que o codegen lê:

```rust
pub struct SymbolEntry { pub name: &'static str, pub ptr: *const u8 }

pub fn jit_symbols() -> Vec<SymbolEntry> {
    // itera SPECS, emite (símbolo, fn_ptr); ASSERT de cobertura em tempo de build:
    // todo símbolo referenciado pelo codegen existe com assinatura lowered casada.
}
```

A **assertion de cobertura em tempo de build** verifica que todo símbolo que o
codegen referencia existe **com assinatura lowered casada**. Um rename que antes
gerava *link OK + SIGILL em runtime* agora **falha o build** — a classe inteira
de bug morre.

### 10.3 A fronteira extern "C" típica sobrevive

O caminho monomórfico continua cruzando a fronteira com **primitivas extern "C"
typed** (`AbiType`, §3.1) — intacto. PolyValues cruzam a fronteira com **uma
convenção tagged-in/tagged-out** para as chamadas genéricas de runtime. Os dois
coexistem: monomórfico paga o caminho rápido typed; genérico paga o tagged. O
inline de intrínsecos coexiste com ambos.

---

## 11. Por que isto é mais simples que o V8 (e o custo honesto)

### 11.1 Complexidade do V8 que o RTS legitimamente PULA

- **Sem interpretador de bytecode / Ignition.** O RTS compila direto para
  nativo; não há tier interpretado.
- **Sem tier especulativo TurboFan.** Não há recompilação especulativa baseada
  em feedback de tipo coletado.
- **Sem deopt / OSR.** Como não há especulação, não há de-otimização nem
  on-stack replacement para sair de código especulado que falhou a assunção.
- **Sem patching de código de inline-cache.** As ICs do RTS são *dados* (§8.2),
  não código auto-modificável.
- **Sem deprecação de hidden-class / grafo de código-dependente.** O V8 mantém um
  grafo de quais funções compiladas dependem de quais hidden classes para
  invalidá-las quando uma classe é depreciada. O RTS não tem esse grafo: ICs de
  dado simplesmente re-resolvem na próxima execução do site.

### 11.2 O custo honesto (declarado sem rodeios)

- **Código polimórfico/megamórfico é mais lento que o V8.** Sem o tier
  especulativo, um site verdadeiramente megamórfico paga o resolver genérico
  toda vez (a IC vira `Mega`).
- **JS quente não-anotado paga um tag-check.** Onde o tipo estático não prova
  monomorfismo, o fast-path inline (§7.3) ainda custa um `icmp` de tag por
  operação. O V8 elidiria isso após warmup especulativo; o RTS não.
- **Primeira execução não tem warmup especulativo.** Não há coleta de feedback
  que melhore o código na 2ª passada além do preenchimento das ICs de dado.

**O trade é deliberado:** o RTS troca o pico especulativo do V8 por um motor
**ordens de magnitude menor e sólido por construção**, mantendo o caminho
numérico monomórfico ~5× Bun (que é o caso de uso-alvo do RTS: TS tipado
compilado a nativo). Para JS dinâmico pesado, o RTS será mais lento que o V8 — e
isso é aceitável e esperado.

---

## 12. Plano de migração strangler-fig

`rts-codegen-old` permanece plugado no `bin`/`cli`. `rts-codegen-new` é
construído **atrás** dele, fase a fase, maior-alavancagem-primeiro. **O piso de
honestidade/build nunca afrouxa; o número de paridade permanece real** (nenhum
fixture deletado/desabilitado/hardcoded para inflar a métrica; nada de
crash/hang commitado como "pass"; build sempre compila).

| Fase | Entrega | Critério de pronto | Guarda de regressão |
|------|---------|--------------------|--------------------|
| **P0** | `value.rs` (PolyValue) — **feito no Incremento 1** | modelo puro + roundtrip JIT Cranelift exaustivamente testados | testes unitários do `value.rs` verdes |
| **P1** | Deletar o modelo-mental MIR + baixar **uma** fn numérica pelo caminho novo (HIR→Cranelift direto) | uma fn numérica roda end-to-end via `lower/` + `pipeline.rs` produzindo o mesmo resultado que o antigo | fixture numérico A/B contra o motor antigo |
| **P2** | Containers de PolyValue substituindo `i64`+`FloatPrim` | `Map`/`Vec` armazenam `PolyValue`; `Entry::FloatPrim` removível | suíte de containers heterogêneos (float fracionário em Map/Vec) verde |
| **P3** | Shapes + ICs para objetos | objeto default usa shape + IC de dado; `HashMap` só patológico | suíte de objetos/property-access + dispatch verde, sem `gc.string_eq` por override |
| **P4** | Dispatch data-driven + `abi_gen` | `resolve_method` dirige tudo por SPECS; símbolos derivados com assert de cobertura | assert de cobertura passa; nenhum `add_fn!` manual remanescente |
| **P5** | Cutover | renomeia `rts-codegen-new` → `rts-codegen`; aposenta `rts-codegen-old` | suíte TS completa + paridade cross-runtime ≥ a do tag `v0.0-202606072107`, número real |

Cada fase roda a suíte incrementalmente (não só no fim). Regressão permitida só
se **explícita e justificada** no commit/PR; regressão silenciosa bloqueia merge.

---

## 13. O que é deletado do motor antigo (por nome)

Lista canônica do que **sai** quando o cutover (P5) acontece:

1. As **4 side-tables** em `ctx.rs`: `fresh_handle_set`, `optional_chain_values`,
   `var_member_call_values`, `var_vec_slot_values` (substituídas pela tag no
   PolyValue + lattice `Repr`).
2. `Entry::FloatPrim` em `rts-engine/src/heap/handles.rs` (floats cabem inline no
   PolyValue).
3. Os helpers de re-tag em runtime `__RTS_FN_RT_FLOAT_BOX` / `_UNBOX` /
   `_EQ_AMBIG` / `_NUM_ARITH` (box/unbox são IR pura).
4. O **tier MIR inteiro** — o uso do crate `rts-mir` pelo codegen (84-inst SSA +
   passes `fold`/`fma`/`cse`/`dce`/`narrow`/`inline`), que re-fazia a egraph do
   Cranelift.
5. O **codegen AST duplicado** — o segundo caminho completo que existia só como
   fallback do MIR.
6. `guards.rs::guard_for` **como código morto** — vira a autoridade real de
   coerção (pilar 3) ou é substituído; o que não sobrevive é o estado atual
   (definido, zero call sites de produção, `TPL_COERCE_AUTO` ad-hoc fazendo o
   trabalho).
7. Os **1113 `add_fn!`** manuais de `jit.rs` (derivados de SPECS por `abi_gen`).
8. Os **objetos com `HashMap<String,i64>` como default** (substituídos por shapes
   + slots inline; hashmap só patológico).
9. O **dispatch por comparação de string** (`gc.string_eq` O(N) por override,
   substituído por shape-id + IC).
10. As **duplicações no switchboard** `calls/mod.rs`: `JSON.stringify`
    5×-duplicado, `Math.max` 2×, listas hardcoded de `console.*` (viram metadado
    `MethodSpec`).
11. As **heurísticas de forma-da-AST** (`is_map_get_call` e similares) — a
    decisão de coerção passa a ser por `Repr` provado / check de tag, nunca por
    inspeção da forma da árvore.

---

## 14. Apêndice — mapa de módulos do crate

`crates/rts-codegen-new/src/`:

| Arquivo | Papel | Pilar | Estado (na escrita) |
|---------|-------|-------|---------------------|
| `lib.rs` | manifesto + reexports dos módulos | — | pronto |
| `value.rs` | `PolyValue` NaN-boxed de 64 bits | 1 | **Incremento 1** (declarado em `lib.rs`) |
| `repr.rs` | lattice `Repr` + `RefKind` + `join` | 2 | esqueleto pronto |
| `shape.rs` | `Shape` / `ShapeTable` / árvore de transição | 4 | esqueleto (`todo!`) |
| `ic.rs` | `IcState` / `PropIcCell` (IC de dado) | 4 | esqueleto pronto |
| `dispatch.rs` | `Target` / `resolve_method` data-driven | 6 | esqueleto (`todo!`) |
| `abi_gen.rs` | `SymbolEntry` / `jit_symbols` derivados de SPECS | 6 | esqueleto (`todo!`) |
| `lower/mod.rs` | `lower_function` — HIR → Cranelift, caminho único | 5 | esqueleto (`todo!`) |
| `pipeline.rs` | `run_jit` / `compile_aot` — JIT+AOT compartilhados | 5 | esqueleto (`todo!`) |

Cross-references externos relevantes:

- `crates/rts-engine/src/heap/handles.rs` — layout de handle `[gen|slot|shard]`
  que o payload de 48 bits do PolyValue reusa (§5.4/§5.5); ponto de mudança do GC.
- `crates/rts-engine/src/abi/guards.rs` — `guard_for` (pilar 3), hoje morto.
- `crates/rts-codegen-old/` — motor congelado; fonte das citações de LOC e dos
  itens deletados (§13).
- `CLAUDE.md` + `.claude/rules/` — doutrina PRIMORDIAL-vs-Registry (§3.2) e o
  piso de honestidade/build (§12), que este redesenho respeita integralmente.
