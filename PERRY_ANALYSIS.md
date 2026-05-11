# Análise: PerryTS vs RTS — estratégias de codegen TS/JS

Documento técnico comparando como o **PerryTS** (https://github.com/PerryTS/perry)
trata TypeScript/JavaScript no caminho até código nativo, contra o que o
**RTS** faz hoje. Foco em decisões que poderíamos portar (ou conscientemente
recusar).

---

## TL;DR

| Aspecto | PerryTS | RTS |
|---|---|---|
| Backend codegen | **LLVM** (via crate próprio) | **Cranelift** (JIT + ObjectModule) |
| Parser | SWC | SWC |
| Representação de valor | **NaN-boxing** (f64 com tags nos high bits) | **i64 nu** + handles GC para refs |
| IR intermediário | **HIR tipado** (`perry-hir`) com lowering próprio | **HIR tipado (`rts-hir`) + MIR SSA (`rts-mir`)** ativos por default; routing híbrido com fallback AST |
| Type refinement | Agressivo em `type_analysis.rs` | Parcial via `type_system/` |
| Multi-target | iOS, Android, macOS, Win, Linux, Vision, watchOS, tvOS, WASM | Win/Linux/macOS via cargo-target |
| Runtime ABI | `js_*` calls com NaN-boxed f64 atravessando boundary | `extern "C"` tipado por `AbiType` (I64/F64/Bool/StrPtr/Handle) |

**Conclusão curta:** Perry escolheu *uniformidade* (NaN-box em tudo, paga overhead
de unbox em hot paths mas simplifica ABI). RTS escolheu *especialização*
(tipos nativos no boundary, paga complexidade no codegen mas roda mais rápido
em código tipado). Os dois estão certos pra problemas diferentes.

---

## 1. Estrutura do repo PerryTS

Workspace cargo com **30+ crates**:

```
crates/
  perry-parser/         # SWC wrapper
  perry-hir/            # HIR tipado (analysis, lower, monomorph, walker)
  perry-types/          # Type system
  perry-transform/      # Passes pré-codegen
  perry-codegen/        # Backend LLVM principal (~1.7M linhas em src/)
  perry-codegen-js/     # Transpile pra JS (web target)
  perry-codegen-wasm/   # WASM
  perry-codegen-swiftui/, -arkts/, -glance/, -wear-tiles/  # UI nativa por plataforma
  perry-runtime/        # Runtime support (NaN-box ABI)
  perry-jsruntime/      # JS interp fallback (?)
  perry-stdlib/         # std nativa
  perry-dispatch/       # Dispatcher de calls runtime
  perry-diagnostics/    # Erros
  perry-ui/, -ui-ios/, -ui-macos/, -ui-android/, -ui-windows/, -ui-gtk4/, ...
  perry-updater/        # Auto-update do toolchain
  perry/                # Crate binário (CLI)
```

**Comparação:** RTS tem 1 crate só, com módulos `abi/`, `codegen/`, `namespaces/`,
`runtime/`. Equivalente em escopo mas menos granular. Perry separou para permitir
target swap (codegen-wasm não puxa LLVM).

---

## 2. NaN-boxing (a decisão central do Perry)

`crates/perry-codegen/src/nanbox.rs` define o ABI de valor:

```rust
// Tags nos high 16 bits do payload NaN de um f64:
TAG_UNDEFINED:   0x7FFC_0000_0000_0001
TAG_NULL:        0x7FFC_0000_0000_0002
TAG_FALSE:       0x7FFC_0000_0000_0003
TAG_TRUE:        0x7FFC_0000_0000_0004
TAG_HOLE:        0x7FFC_0000_0000_0010   // sentinela array sparse
POINTER_TAG:     0x7FFD_0000_0000_0000   // payload = 48-bit ptr
INT32_TAG:       0x7FFE_0000_0000_0000   // payload = i32
STRING_TAG:      0x7FFF_0000_0000_0000   // payload = handle string
BIGINT_TAG:      0x7FFA_0000_0000_0000
```

**Idéia:** todo valor JS cabe num `f64` (8 bytes). Floats ficam como floats reais
(qualquer NaN de operação numérica genuína vive fora da banda de tags). Os outros
tipos viajam como NaN com payload tagado.

**Detalhe interessante:** as constantes são também armazenadas como **strings i64**
(`TAG_UNDEFINED_I64: &str = "9222246136947933185"`) pra colar direto em LLVM IR
textual — se passassem pelo parser de double do LLVM, perderiam bits do payload
em alguns alvos. Cuidado de baixo nível que mostra o quão load-bearing essa
representação é.

### Comparação com RTS

RTS hoje (ver `src/abi/types.rs`):
- `AbiType::I64 | F64 | Bool | StrPtr | Handle | Void | I32 | U64`
- Cada função extern `"C"` declara tipos exatos
- `StrPtr` expande pra dois slots (`ptr + len`) via Cranelift
- Handles GC são `u64` opacos (gen:16 + slot:48)

**Trade-offs:**

| | NaN-box (Perry) | Tipado (RTS) |
|---|---|---|
| Tamanho do valor | 8 bytes uniforme | varia (i64, f64, par ptr+len) |
| ABI extern | `js_add(f64, f64) -> f64` único | `__RTS_FN_NS_X_Y(...)` com assinatura específica |
| Hot path numérico | precisa unbox/check tag toda vez | direto em registrador |
| Polimorfismo dinâmico (`any`) | trivial — bits já carregam tipo | precisa coerce/guard em call site |
| Tamanho do codegen | 1 path por op (genérico) | N paths (especializado por tipo) |
| Compat com Cranelift JIT | viável mas custoso | natural |

**Pra RTS:** migrar pra NaN-box seria refator gigante (toda ABI muda). Mas o problema
documentado em `project_architecture_issues.md` (memória) — "pipeline tudo i64
perdendo tipo, delega pro runtime decidir" — é exatamente o que NaN-box resolve.
**Não recomendo migrar.** Recomendo melhorar refine de tipo (próxima seção)
pra ter o melhor dos dois mundos: ABI tipada onde possível, fallback genérico
onde tipo é desconhecido.

---

## 3. HIR tipado (`perry-hir`)

Crate dedicado ao IR intermediário antes do codegen. Arquivos:

```
perry-hir/src/
  ir.rs              # 77k — definições de Expr, Stmt, Type, FuncId, GlobalId, LocalId
  analysis.rs        # 67k — passes de análise
  lower.rs           # 416k — AST(SWC) → HIR
  lower_decl.rs      # 190k — declarações
  lower_patterns.rs  # 15k  — destructuring patterns
  lower_types.rs     # 47k  — tipos
  destructuring.rs   # 99k  — expansão de patterns
  monomorph.rs       # 140k — monomorfização de generics
  walker.rs          # 63k  — visitor
  js_transform.rs    # 151k — semântica JS (coerções, ToNumber, ToString...)
  jsx.rs, enums.rs
```

**Tipos** (`perry-types::Type`):
- `Number, String, Boolean, Any`
- `Array<T>` (paramétrico)
- `Generic { base, type_args }` (Set, Map, Promise<T>...)
- `Named(...)`, `Function(...)`, etc.

### O que isso habilita

`crates/perry-codegen/src/type_analysis.rs` tem `refine_type_from_init` —
recebe um `Expr` inicializador e devolve o tipo refinado:

```rust
match init {
    Expr::Number(_) | Expr::Integer(_) => Some(Type::Number),
    Expr::Binary { op, left, right } => {
        if is_numeric_expr(ctx, left) && is_numeric_expr(ctx, right) {
            Some(Type::Number)
        } else { None }
    }
    Expr::Array(_) | Expr::ArraySpread(_) => Some(Type::Array(Box::new(Type::Any))),
    Expr::New { class_name, .. } if class_name == "Array" =>
        Some(Type::Array(Box::new(Type::Any))),
    Expr::TextEncoderEncode(_) => Some(Type::Array(Box::new(Type::Number))),
    Expr::StringSplit { .. } => Some(Type::Array(Box::new(Type::String))),
    Expr::SetNewFromArray(_) | Expr::SetNew => Some(Type::Generic {
        base: "Set".into(), type_args: vec![]
    }),
    // ...
}
```

**Por quê isso importa:** sem refine, `let i = 0; for (...) i = i + 1` mantém `i`
como `Any` e cada `i + 1` vira `js_number_coerce(i) + 1` no IR — overhead enorme
em hot loops. Com refine, `i` vira `Number` e o codegen emite `add` direto.

Os comentários no código identificam casos reais corrigidos: object_create,
binary_trees, fibonacci com `let i = 0` sem anotação explícita perdiam fast path.

### Comparação com RTS

RTS tem `type_system/` mas não faz refine tão agressivo. Hot loops que dependem
de variáveis sem anotação explícita podem cair em paths genéricos.
Onde o RTS já vence: quando o user **anota** (`let i: number = 0`), o codegen
já lower otimizado.

**Oportunidade real pra RTS (low-hanging fruit):** portar o `refine_type_from_init`.
Requer:
1. Identificar onde no `type_system/` ou `codegen/lower/statements/decls.rs`
   o tipo é decidido pra `let` sem anotação
2. Adicionar pattern matching nos initializers comuns (literal numérico, binop
   numérico, array literal, `new Array(n)`, `new Set/Map`)
3. Garantir que o tipo refinado se propaga pro escopo do bloco

Não é refator de ABI — é melhoria local no codegen. Provavelmente dá ganho
mensurável em workloads não-anotados (TS code típico de npm).

---

## 4. Tamanho do codegen JS é grande (mesmo em quem fez certo)

`crates/perry-codegen/src/`:

```
expr.rs            546k
collectors.rs      221k
codegen.rs         201k
lower_call.rs      306k
runtime_decls.rs   123k
stmt.rs            93k
type_analysis.rs   59k
lower_string_method.rs  33k
linker.rs          27k
block.rs           25k
boxed_vars.rs      45k
lower_array_method.rs  21k
loop_purity.rs     12k
```

Plus o `lower.rs` do HIR (416k). É **muito código**.

**Lição:** mesmo Perry, com HIR tipado e separação de concerns, gasta 500k+ em
um arquivo de expressões. JS/TS tem tantos casos especiais (coerção, this binding,
prototype, generators, destructuring, optional chaining, regex, template literals,
JSX, async/await, ...) que codegen completo é inerentemente grande.

**Pra RTS:** o tamanho dos arquivos em `src/codegen/lower/` não é sinal de mau
design — é o custo de cobrir a linguagem. Refator vale quando há **duplicação**
ou quando um caminho não-trivial está misturado com o trivial. Nesses 546k do
`expr.rs` Perry tem `lower_call/` separado em pasta (não arquivo único), e o
RTS já faz o mesmo (`lower/expressions/`, `lower/statements/`).

---

## 5. Multi-target: o que Perry tem que RTS não tem

Crates `perry-codegen-{wasm,arkts,glance,wear-tiles,swiftui,js}` + `perry-ui-{ios,
macos,android,windows,gtk4,visionos,watchos,tvos}` mostram que Perry foi
arquitetado pra ser uma toolchain *cross-platform end-to-end*.

- **arkts** = HarmonyOS (Huawei)
- **glance** = Android home-screen widgets
- **wear-tiles** = Wear OS tiles
- **swiftui** = nativo Apple

RTS hoje é Win+Linux+macOS via Cranelift padrão. UI é FLTK (`namespaces/ui/`).

Não é gap funcional óbvio (RTS pode chamar libs nativas via FFI), mas é um modelo
de produto diferente: Perry quer ser "um TS que vira app nativo iOS/Android",
RTS é "um runtime TS pra servidor + CLI rápida". Direções não conflitam, é
escolha de produto.

---

## 6. Runtime support

**Perry:**
- `perry-runtime/` — implementação Rust dos `js_*` builtins (NaN-box ABI)
- `perry-jsruntime/` — possivelmente fallback interpretado pra dynamic features
- `perry-stdlib/` — std nativa (mysql2, pg, ws, axios, ...) com features cargo
- `perry-dispatch/` — dispatcher centralizado de calls runtime
- `NATIVE_MODULES` em `perry-hir/src/ir.rs`: lista de packages npm com
  implementação nativa Rust (mysql2, pg, ioredis, axios, node-fetch, ws,
  zlib, ethers, mongodb, jsonwebtoken, nanoid, dotenv, validator, ...)

**Insight:** Perry assume que o codegen vai detectar `import { Redis } from "ioredis"`
e roteá-lo pra `js_ioredis_*` na stdlib nativa, **bypassando npm**. Comentários
no código mencionam que ioredis precisou entrar em `NATIVE_MODULES` pra que
`requires_stdlib` retornasse true e o linker puxasse o archive da feature
`database-redis`.

**RTS:**
- `src/namespaces/` cobre similar (40+ namespaces): net, tls, crypto, fs, io,
  http_server (actix), regex, ui, ...
- Sem mapeamento automático de pkgs npm pra implementação nativa hoje. RTS
  espera que o user importe `"rts"` direto.

**Oportunidade:** o RTS poderia mapear `import "axios"` pra usar `tls + http_server`
internos automaticamente. É feature de DX, não de codegen. Não urgente.

---

## 7. Recomendações pro RTS (priorizadas)

### Alta — fazer agora se houver bandwidth

1. **Type refinement em `let` sem anotação.** Portar `refine_type_from_init`
   do Perry. Reduz overhead em hot loops com counters não-anotados. Esforço
   baixo (~2 dias), ganho mensurável em benchmarks com código TS "natural".

### Média — vale considerar

2. **Mapeamento npm → nativo.** Listar pkgs comuns (`fs`, `path`, `crypto`,
   `axios`, `ws`) que viram `"rts"` automaticamente no resolver de módulos.
   Já existe parcialmente em `src/nodespace/`. Expandir.

3. **Separar codegen por target em sub-crates.** Se RTS for adicionar WASM
   ou cross-compile sério, vale extrair `codegen/` num crate separado e
   ter `codegen-cranelift`, `codegen-wasm`, etc. Hoje é prematuro.

### Baixa — provavelmente não fazer

4. **Migrar pra NaN-boxing.** Refator gigante, risco alto, ganho concentrado
   em código `any`-pesado. RTS é otimizado pra TS bem-tipado e ganharia
   pouco. **Não recomendo.**

5. **Trocar Cranelift por LLVM.** LLVM tem autovec real e otimizações que
   Cranelift não tem (ver issue #92 fechada no RTS). Mas LLVM é build pesado,
   licenciamento complicado pra distribuição standalone, e Cranelift roda
   embutido. **Não recomendo trocar**, recomendo continuar no path "Cranelift +
   passes silent parallelism" pra ganhar onde Cranelift perde.

---

## 8. O que o RTS já faz melhor

Pra ser justo na comparação:

- **Silent parallelism** (`array_methods_pass`, `reduce_pass`, `purity_pass`):
  RTS reescreve `arr.map(fn)` pra `parallel.map` automaticamente quando `fn` é
  pura. Perry não tem nada equivalente visível no repo público.
- **HandleTable shard-aware**: 32 shards lock-free pra reduzir contenção.
  Perry usa NaN-box ptr direto pra heap GC; modelo diferente, vantagens
  diferentes.
- **GC scanner Win32 com `GetCurrentThreadStackLimits`**: documentado em
  `project_gc_not_integrated` e CLAUDE.md. Resolve bug específico que Perry
  pode ou não ter.
- **HTTP server 29k req/s** (78% do actix puro Rust): integração tokio compartilhado
  bem feita.
- **CLI `rts ir`** pra inspecionar Cranelift IR — debug ergonômico que poucos
  toolchains expõem.

---

## 9. Referências (URLs do repo Perry)

- Repo: https://github.com/PerryTS/perry
- NaN-box: `crates/perry-codegen/src/nanbox.rs`
- HIR: `crates/perry-hir/src/ir.rs`, `lower.rs`, `monomorph.rs`
- Type refine: `crates/perry-codegen/src/type_analysis.rs`
- Native modules: `crates/perry-hir/src/ir.rs` (constante `NATIVE_MODULES`)
- Runtime ABI: `crates/perry-runtime/src/value.rs` (não inspecionado, citado
  por comentário em nanbox.rs)

---

## Apêndice: snippet NaN-box pra colar em discussão

```rust
// PerryTS — perry-codegen/src/nanbox.rs
pub const TAG_UNDEFINED: u64 = 0x7FFC_0000_0000_0001;
pub const TAG_NULL:      u64 = 0x7FFC_0000_0000_0002;
pub const TAG_FALSE:     u64 = 0x7FFC_0000_0000_0003;
pub const TAG_TRUE:      u64 = 0x7FFC_0000_0000_0004;
pub const POINTER_TAG:   u64 = 0x7FFD_0000_0000_0000;
pub const INT32_TAG:     u64 = 0x7FFE_0000_0000_0000;
pub const STRING_TAG:    u64 = 0x7FFF_0000_0000_0000;
pub const BIGINT_TAG:    u64 = 0x7FFA_0000_0000_0000;
pub const TAG_MASK:      u64 = 0xFFFF_0000_0000_0000;
```

```rust
// RTS hoje — src/abi/types.rs (resumido)
pub enum AbiType {
    Void, Bool, I32, I64, U64, F64, StrPtr, Handle,
}
// Cada função declara assinatura específica.
// Sem boxing no boundary extern "C".
```

Os dois modelos são internamente coerentes. Perry escolheu uniformidade
(simplicidade no boundary, custo no hot path). RTS escolheu especialização
(performance no hot path, complexidade no codegen pra cobrir todos os tipos).
