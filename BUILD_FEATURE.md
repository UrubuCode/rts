# BUILD_FEATURE: Rust Namespace Primitives

## Ideia Central

RTS funciona como uma **camada de instrução de máquina pura** — Rust exporta apenas primitivas brutas (alocação, aritmética, I/O, GC). Não há abstração de tipos JavaScript no lado Rust.

`String`, `Number`, `Array` e toda a semântica JS/TS são implementados em TypeScript, dentro do sistema de módulos do próprio projeto. O `rts` é a "linguagem de máquina"; os módulos TS são a stdlib construída sobre ela.

Isso elimina `JSValue` e `lang` do Rust — Rust não sabe o que é um valor JavaScript, só opera sobre bytes, inteiros, floats e ponteiros.

**Por que?** Rust não deve conhecer semântica de linguagem de alto nível. Separar responsabilidades permite que a stdlib TS evolua independentemente, sem recompilar o runtime. Além disso, operações mistas resolvidas no Rust criariam FFI overhead desnecessário para operações que podem ser executadas em 2-3 instruções assembly.

---

## Analogia

```
Kernel  → syscalls brutas       →  libc   → stdlib   → aplicação
Rust    → rts primitivas brutas →  packages/std.ts   → user.ts
```

---

## O que some do Rust

- `JSValue` — removido. Rust não abstrai valores JS.
- `lang/` — removido. Semântica de linguagem não é responsabilidade do Rust.
- Coerção de tipos inline no codegen — resolvida pelo HIR antes de chegar no Rust.

**Por que?** Manter `JSValue` e `lang/` no Rust criaria um acoplamento forte entre runtime e semântica da linguagem. Cada mudança nas regras de coerção do TypeScript exigiria recompilar o Rust. Isso é insustentável para evolução rápida da linguagem.

---

## O que fica no Rust

Apenas o que só o Rust pode fazer:

```
rts_alloc(size: u64) → ptr
rts_str_new(ptr: u64, len: u64) → handle
rts_i64_add(a: i64, b: i64) → i64
rts_f64_mul(a: f64, b: f64) → f64
rts_mem_copy(dst: u64, src: u64, len: u64)
rts_gc_collect()
... I/O, sockets, fs, crypto (namespaces existentes)
```

Tipos Rust diretos: `i64`, `f64`, `u64` (handles/ponteiros), `bool`. Sem coerção, sem semântica JS.

**Por que?** Operações tipo-safety (i64 + i64) são baratas e previsíveis. Rust lida bem com isso. O problema é quando há ambiguidade de tipos — isso deve ser resolvido em camada superior.

---

## Coerção e Operações Mistas — Responsabilidade do HIR via `rust.natives`

Operações que envolvem tipos mistos (ex: `1 + "1"`) são resolvidas **estaticamente no HIR**. O HIR reconhece o padrão e injeta uma chamada a `rust.natives.*`:

```ts
// source
const valor = 1 + "1";

// após HIR
import { natives, type u64 } from "rts";
const valor: u64 = natives._rts_merge(1, "1");
```

O namespace é `rust.natives.*`, definido em `src/namespaces/rust/natives.rs`.

**`natives` não é Rust** — são **extensões C nativas**. O MIR converte chamadas `rust.natives.*` diretamente em machine code C-level, sem passar pelo Rust. O objetivo é resolver lógicas que teriam perda de desempenho indo pelo Rust (coerção, merge de tipos, operações mistas).

**Por que C nativas em vez de Rust?** 
- Chamada Rust tem overhead de FFI boundary (stack frame, salvamento de registradores)
- C nativo via MIR pode ser emitido como 3-5 instruções assembly inline
- Para operações como `1 + "1"`, pagar 50ns de chamada Rust vs 2ns de instrução direta é inviável em hot paths

### Fluxo por camada

```
HIR (reconhece coerção) → natives C (executa) → machine code
MIR   — converte rust.natives.* → extends C nativas (machine code direto)
codegen — emite a call C nativa, sem overhead de Rust
```

**Por que o HIR e não o codegen?** Codegen já é complexo (Cranelift IR). Adicionar lógica de coerção JS multiplicaria casos de teste e tornaria a manutenção proibitiva. O HIR tem visibilidade semântica completa para tomar decisões de coerção.

---

## Otimizações de Performance — Além do C Native

Para máxima performance, operações críticas recebem tratamento especial:

### 1. Inline Expansion para Operadores Frequentes

Operações como `+`, `-`, `*`, `/`, `==` são expandidas inline em vez de chamar C:

```
// Em vez de: call _rts_add_mixed
// MIR gera:
if is_string(a) && is_number(b) {
    // inline: converte b para string, concatena
    // 10-15 instruções assembly, sem call overhead
} else if is_number(a) && is_string(b) {
    // inline similar
} else {
    call _rts_add_mixed_fallback
}
```

**Por que?** Para operações frequentes, o overhead de chamada de função domina o tempo de execução. Inline expansion reduz de ~50ns para ~15ns por operação.

### 2. Precomputed Tables para Conversões

Para `toString()` em inteiros pequenos:

```rust
static TO_STRING_TABLE: [&str; 256] = [
    "0", "1", "2", ... "255"
];

fn i64_to_string_fast(x: i64) -> String {
    if x >= 0 && x < 256 {
        TO_STRING_TABLE[x as usize].into()  // sem alocação, sem branch complexo
    } else {
        _rts_i64_to_string_slow(x)
    }
}
```

**Por que?** Conversão número→string é operação comum em coerção. Tabelas pré-computadas eliminam branches e alocações para 99% dos casos (números pequenos).

### 3. JIT Specialization (Futuro)

Runtime mantém cache de tipo por PC. Após N execuções, recompila função com tipos concretos:

```
Primeira execução: _rts_add_mixed (genérico)
Após 100 execuções: _rts_add_string_number (especializado)
Hot path: 3-5 instruções assembly
```

**Por que?** Estatisticamente, operandos tendem a ter tipos consistentes. Especializar dinamicamente paga o overhead inicial uma vez e acelera todas as execuções seguintes.

### 4. PGO (Profile Guided Optimization) no AOT

```bash
rts compile app.ts -pgo-generate
./app --train  # executa com workloads típicas
rts compile app.ts -pgo-use -o app.optimized
```

**Por que?** Coletando tipos reais em execução, o compilador pode:
- Reordenar branches: path mais comum primeiro
- Especializar funções baseado no profile
- Inline condicional baseado em frequência

### Comparativo de Performance

| Abordagem | Tempo por operação | Overhead | Quando usar |
|-----------|-------------------|----------|-------------|
| C native (base) | ~50ns | Baixo | Operações complexas |
| Inline expansion | ~15ns | Zero | Operadores frequentes (+, ==) |
| Precomputed tables | ~5ns | Zero | toString(), toNumber() |
| JIT specialization | ~20ns (após warmup) | Baixo | Funções muito quentes |
| **Combinado (recomendado)** | **~10-15ns** | **Baixo** | **Todas operações mistas** |

---

## Debug Info e Source Maps — Localização de Erros

Para desenvolvimento, é fundamental mostrar erros com localização precisa no arquivo fonte. O RTS implementa um sistema de debug info em múltiplas camadas.

### Modos de Operação via .env

O RTS lê automaticamente o arquivo `.env` (se existente) e determina o modo baseado nas variáveis:

```bash
# Prioridade (primeiro encontrado vence):
RTS_MODE=development    # Força modo dev (debug info completo)
RTS_MODE=production     # Força modo prod (sem debug info)
NODE_ENV=development    # Compatível com ecossistema Node
NODE_ENV=production
APP_ENV=development     # Compatível com frameworks
APP_ENV=production
RTS_PRODUCTION=1        # Legacy: modo produção
RTS_DEBUG=1             # Força debug info mesmo em produção
```

**Comportamento padrão:**
- Se nenhuma variável definida: assume `development` no `rts run` e `production` no `rts compile`
- Se `.env` não existe: assume padrão baseado no comando

### Estrutura dos Arquivos Gerados

```
compile output/
  module.o              # Código real (DWARF com debug info se -g)
  module.ometa          # Metadata JSON (sourcemap + location table)
  module.debug          # DWARF puro (opcional, separado)
```

#### `.ometa` (Object Metadata) - Formato

```json
{
  "version": 1,
  "mode": "development",
  "sourceRoot": "/project/src",
  "sources": ["index.ts", "utils.ts"],
  "sourceMap": "base64...",  // sourcemap original do TS
  "locations": {
    "0x12a3f": {
      "source": "index.ts",
      "line": 42,
      "column": 10,
      "range": [42, 10, 42, 25]
    },
    "0x12b00": {
      "source": "index.ts", 
      "line": 45,
      "column": 5
    }
  },
  "functions": {
    "_rts_foo": {
      "offset": 0x12a00,
      "size": 256,
      "source": "index.ts",
      "line": 40
    }
  }
}
```

### Responsabilidades por Camada

#### 1. **HIR** - Tracking de localização

```typescript
// HIR AST com location tracking
interface HIRNode {
  kind: string;
  loc: SourceLocation;  // ← HIR anexa em cada node
  // ...
}

interface SourceLocation {
  file: string;
  line: number;
  column: number;
  endLine: number;
  endColumn: number;
  sourceMapIndex: number;  // referência ao sourcemap original
}
```

**Por que HIR?** HIR tem visibilidade sintática completa e acesso ao sourcemap do TypeScript.

#### 2. **MIR** - Preserva e compacta localizações

```rust
struct MIRLocation {
    file_id: u32,        // ID no sourcemap
    line: u32,
    column: u32,
    byte_offset: u64,    // offset no código gerado (preenchido pelo Cranelift)
}

struct MIRInstruction {
    op: MIROp,
    loc: Option<MIRLocation>,  // ← MIR preserva
    // ...
}
```

**Por que MIR?** MIR é onde instruções são sequenciais e pode compactar localizações (ex: mesma linha para 10 instruções seguidas).

#### 3. **Cranelift** - Emite DWARF e preenche offsets

```rust
fn emit_instruction(&mut self, inst: &MIRInstruction, loc: &MIRLocation) {
    let start_offset = self.current_offset();
    
    // Emite código...
    self.emit_opcode(&inst.op);
    
    let end_offset = self.current_offset();
    
    // Emite debug info baseado no modo
    if self.mode.is_development() {
        self.debug_info.add_location_range(
            start_offset, end_offset,
            loc.file_id, loc.line, loc.column,
        );
        self.metadata.add_location_mapping(
            start_offset, end_offset, loc.clone()
        );
    }
}
```

**Por que Cranelift?** Conhece os offsets finais e já tem suporte a DWARF.

#### 4. **Runtime (C)** - Lê .ometa e formata erro

```c
// runtime/error.c
typedef struct {
    uint64_t offset;
    char* source_file;
    uint32_t line;
    uint32_t column;
} DebugLocation;

void rts_panic_with_location(const char* message, uint64_t pc_offset) {
    // Verifica modo via variável de ambiente (cache)
    static int mode = -1;
    if (mode == -1) {
        char* mode_env = getenv("RTS_MODE");
        if (!mode_env) mode_env = getenv("NODE_ENV");
        if (!mode_env) mode_env = getenv("APP_ENV");
        mode = (mode_env && strcmp(mode_env, "production") == 0) ? 0 : 1;
    }
    
    if (mode == 0) {
        // Produção: mostra apenas offset ou mensagem genérica
        fprintf(stderr, "Error: %s (at pc=0x%lx)\n", message, pc_offset);
        return;
    }
    
    // Desenvolvimento: carrega metadata
    Metadata* meta = load_metadata_for_pc(pc_offset);
    
    if (meta && meta->locations[pc_offset]) {
        DebugLocation loc = meta->locations[pc_offset];
        
        fprintf(stderr, "\n\x1b[31m%s\x1b[0m: %s\n", loc.source_file, message);
        fprintf(stderr, "    at \x1b[36m%s\x1b[0m:\x1b[33m%d\x1b[0m:\x1b[33m%d\x1b[0m\n", 
                loc.source_file, loc.line, loc.column);
        
        // Mostra linha do código com highlight
        char* source_line = get_source_line(meta, loc.line);
        fprintf(stderr, "    %s\n", source_line);
        fprintf(stderr, "    \x1b[32m%*s\x1b[31m^%s\x1b[0m\n", loc.column, "", source_line + loc.column);
    } else {
        fprintf(stderr, "Error: %s (no debug info)\n", message);
    }
    
    exit(1);
}
```

### Fluxo Completo de Erro

#### Em desenvolvimento (`RTS_MODE=development` ou `rts run --dev`)

```bash
$ rts run index.ts

Error: Cannot read property 'length' of undefined
    at /project/src/utils.ts:42:10
    at getUser (/project/src/user.ts:15:5)
    at main (/project/src/index.ts:8:3)

  40 | export function formatName(user) {
  41 |   // user pode ser undefined
> 42 |   return user.name.length;
     |          ^
  43 | }
```

#### Em produção (`RTS_MODE=production` ou `rts compile` sem `-g`)

```bash
$ ./app

Error: Cannot read property 'length' of undefined
    at pc=0x12a3f
```

### Otimizações

#### 1. Lazy loading do .ometa

```c
__attribute__((cold)) 
void rts_on_error(uint64_t pc) {
    static Metadata* cached = NULL;
    if (!cached) cached = load_metadata();
    // ...
}
```

#### 2. Compactação de localizações

```rust
// MIR compacta: mesma linha para range de instruções
struct LocationRange {
    start_offset: u64,
    end_offset: u64,
    file_id: u32,
    line: u32,
    column: u32,
}
// Em vez de 1 location por instrução, 1 location por basic block
```

#### 3. Modos de compilação

```bash
# Desenvolvimento (debug info completo)
rts run index.ts                    # .ometa gerado automaticamente
rts run --dev index.ts              # força modo dev
RTS_MODE=development rts run index.ts

# Produção com debug (para debugging em produção)
rts compile -g index.ts             # .ometa + DWARF embutido

# Produção sem debug (máxima performance)
rts compile -O3 index.ts            # sem .ometa, erros só mostram offset
RTS_MODE=production rts compile index.ts

# Produção com sourcemaps externos
rts compile --source-map index.ts   # .ometa separado, binário sem debug
```

### Comparativo com outras engines

| Engine | Debug Info | Runtime erro | Modos |
|--------|------------|--------------|-------|
| Node.js | Sourcemap + V8 | Mostra linha + código | NODE_ENV |
| Bun | Sourcemap + Zig | Similar a Node | --dev flag |
| Deno | Sourcemap + Rust | Mostra com cores | --inspect |
| **RTS (proposto)** | `.ometa` + DWARF | Compatível + cores | RTS_MODE/.env |

---

## Estrutura do Namespace Rust

```
src/namespaces/
  rust/
    functions.rs   — rts_declare_fn, rts_call_fn, rts_return
    scope.rs       — rts_scope_push, rts_scope_pop, rts_set_var, rts_get_var
    constants.rs   — rts_declare_const
    memory.rs      — rts_alloc, rts_free, rts_mem_copy
    natives.rs     — extensões C nativas (coerção, merge, operações mistas)
    hotops.rs      — inline expansion para operadores frequentes (gerado via Cranelift)
    debug.rs       — rts_load_metadata, rts_resolve_location, formatação de erro
    mod.rs         — gera os .o igual os outros namespaces
```

Os `.o` ficam em `target/objs/runtime/rust/` e são linkados junto com os outros namespaces. Sem `librts_runtime.a`, sem cargo no usuário — mesmo sistema já existente.

**Por que adicionar `hotops.rs` e `debug.rs`?** 
- `hotops.rs`: separa operações quentes (inline expansion) do resto
- `debug.rs`: isolamento lógica de debug info (pode ser removido em produção)

---

## AOT e Runtime — Mesmo Pipeline

AOT (`rts compile`) e runtime (`rts run`) usam o **mesmo pipeline de codegen**. Não há divergência de implementação entre os dois modos.

A diferença é de escopo e modo:

| | `rts run` | `rts compile` |
|---|---|---|
| Módulos incluídos | Todos os namespaces | Apenas os usados (slicing) |
| Output | Executa direto | Gera binário final |
| Otimizações (`-p`) | Disponível | Disponível |
| PGO | Não | Sim (coleta + uso) |
| Debug info | Padrão: sim (modo dev) | Padrão: não (modo prod) |
| .ometa | Gerado automaticamente | Apenas com `-g` ou `--source-map` |

Otimizações e implementações do namespace rust se aplicam igualmente nos dois modos. A flag `-p` habilita otimizações no pipeline em qualquer um deles.

**Por que PGO só no AOT?** Coleta de profile requer execução representativa. No `rts run` o usuário quer execução imediata, não treinamento.

---

## O que muda no Codegen

O `typed_codegen` deixa de resolver tipos JS. Os tipos chegam já resolvidos pelo HIR/MIR — sem ambiguidade de "isso é JSNumber ou i64?". O codegen emite apenas chamadas tipadas:

```
call rts_scope_push()
call rts_declare_fn(name_ptr, arity, body_ptr)
call rts_i64_add(a, b)
call <inline expansion ou C native>  ← decidido pelo MIR baseado em hotness
```

**Por que?** Codegen mais simples = menos bugs. O MIR decide a estratégia (inline vs call) baseado em heurísticas (operador frequente? tipos conhecidos? loop?).

---

## Hot Paths / Funções Pequenas

Para operações onde overhead de call importa, as funções são geradas **diretamente via Cranelift como `.o`**, programaticamente durante o build do `rts`:

```
Cranelift → emit rts_i64_add, rts_f64_mul, rts_ptr_eq → .o
```

Mesmo sistema de objetos. A origem muda, o mecanismo não:
- Primitivas complexas: `rustc` compila `src/namespaces/rust/*.rs` → `.o`
- Hot ops tiny: Cranelift emite diretamente → `.o`
- Natives C: `natives.rs` compila extensões C → `.o`
- Inline expansion: `hotops.rs` gera código inline → `.o`
- Debug info: `debug.rs` compila helpers de erro → `.o`

**Por que Cranelift para hot ops tiny?** Rustc adiciona overhead de segurança (bounds checks, panics) mesmo com `#[inline]`. Cranelift pode emitir exatamente as 3 instruções necessárias: `mov`, `add`, `ret`.

---

## Gerenciamento de Estado

O namespace rust segue os patterns já estabelecidos no projeto:

**Scope/variáveis** — `thread_local!`, execução é single-threaded por contexto:

```rust
thread_local! {
    static SCOPE_STACK: RefCell<Vec<HashMap<u64, u64>>> = RefCell::new(vec![]);
}
```

**Funções declaradas** — registry global com `OnceLock<Arc<Mutex<T>>>`.

**GC** — `safe_collect()` nos pontos de quiescência já definidos (retorno de função, fim de closure, pós-método).

**Debug Metadata** — cache global com lazy loading:

```rust
static METADATA_CACHE: OnceLock<Arc<Mutex<HashMap<u64, Metadata>>>> = OnceLock::new();
```

Sem sistema centralizado — cada arquivo gerencia seu próprio estado com patterns padrão de Rust, igual aos outros namespaces.

**Por que?** Consistência com o resto do códigobase. Novos desenvolvedores não precisam aprender patterns novos.

---

## Pipeline Final

```
rustc compila src/namespaces/rust/*.rs   →  .o  (scope, funções, memória, debug)
natives.rs compila extensões C           →  .o  (coerção base)
hotops.rs gera inline expansion          →  .o  (operadores frequentes)
Cranelift emite hot ops tiny             →  .o  (i64_add, f64_mul, ptr_eq)
codegen do usuário via Cranelift         →  .o  (calls tipadas com decisões MIR)

[Modo dev] debug.rs + metadata           →  .o + .ometa
[Modo prod] sem debug info               →  .o apenas

lld linka tudo                           →  user.exe
```

Nenhuma etapa requer cargo, rustc ou toolchain no ambiente do usuário.

**Por que essa ordem?** Dependências: rustc → runtime base, natives/hotops → operações mistas, Cranelift → primitivas finais. Linker resolve tudo.

---

## O que este approach resolve

- **JSValue removido**: Rust opera sobre tipos de máquina
- **lang removido**: semântica JS resolvida pelo HIR, não pelo Rust
- **Coerção estática**: HIR resolve no compile time via `rust.natives.*`, MIR decide estratégia (inline vs call)
- **AOT e runtime unificados**: mesmo pipeline, diferença apenas de slicing, escopo, PGO e debug info
- **Codegen mais simples**: tipos já chegam resolvidos, sem lógica JS inline
- **Stdlib em TS**: `String`, `Number`, `Array` evoluem independente do Rust
- **Sem nova infraestrutura**: mesmo sistema de `.o` existente, extendido
- **Hot paths eficientes**: múltiplas estratégias (Cranelift, inline expansion, precomputed tables) para diferentes perfis
- **Performance previsível**: ~10-15ns para operações mistas comuns, sem surpresas
- **Debug info completo**: sourcemaps + .ometa, erros com localização precisa em desenvolvimento
- **Modos via .env**: `RTS_MODE`, `NODE_ENV`, `APP_ENV` para controle de debug info
- **Compatibilidade ecossistema**: respeita variáveis de ambiente padrão do Node.js

---

## Decisões de Design — Porquês Resumidos

| Decisão | Alternativa rejeitada | Por que? |
|---------|----------------------|----------|
| Coerção no HIR | Coerção no Rust/Runtime | Isolamento semântico + performance |
| C natives via MIR | Rust functions | Overhead de FFI é inaceitável para operações pequenas |
| Inline expansion | Tudo como C native | 50ns → 15ns para operadores frequentes |
| Precomputed tables | Conversão genérica | 5ns vs 50ns para números pequenos (99% dos casos) |
| Cranelift para hot ops | rustc para tudo | rustc adiciona safety checks desnecessários |
| PGO opcional | Sempre ativo | Treinamento é caro, só compensa em AOT final |
| Estado thread_local | Estado compartilhado | Single-threaded por contexto, evita locks |
| Sem toolchain no usuário | Requer Rust toolchain | Experiência de usuário simples, build determinístico |
| Debug info no HIR → MIR → CL | Só no runtime | Cada camada tem info específica para seu nível |
| .ometa + DWARF | Apenas DWARF | .ometa é mais rápido para runtime ler que DWARF |
| .env para modo | Flag apenas | Integração com ecossistema, 12-factor apps |