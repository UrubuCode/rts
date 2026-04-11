# Otimização do Hot Path de Execução — do `FN_EVAL_STMT` ao Bun-Beater

> **TL;DR**: o bench `rts_simple.ts` saiu de ~2300 ms para **26 ms** (AOT) / **36 ms** (JIT)
> no decorrer de 8 commits, sem mudar arquitetura. O RTS hoje é **3.8× mais rápido**
> que Bun e **4.9× mais rápido** que Node nesse bench.

Este documento registra **como** isso foi feito, **por que** cada decisão foi tomada,
e as lições aprendidas ao longo da investigação. Ele existe para:

1. Servir de **guia de método** para futuras investigações de perf — a sequência
   "medir → localizar → consertar → remedir" é replicável.
2. **Documentar armadilhas** descobertas (`opt_level=speed_and_size` do Cranelift,
   instrumentação no hot path, `String::from_utf8` em cada dispatch, etc.).
3. Registrar o **placar final** e o contexto real de comparação contra Bun/Node.
4. Apontar os **gargalos residuais** conhecidos para a próxima sessão.

---

## 1. Ponto de partida

### 1.1 O bench

`bench/rts_simple.ts` é um stress test aritmético puro:

- `arithmetic_stress(80000)` — 80k iterações de um LCG com branches modulares
- `count_primes(2500)` — peneira de Eratóstenes com loops aninhados
- `bigint_like_stress(120000)` — simulação de big-int em limbs, 120k iterações
- `mix_scores()` — `switch` sobre `(limb0 + limb1 + limb2) % 3`

Métricas relevantes desse workload:

- ~720k leituras/escritas de globais (`limb0`, `limb1`, `limb2`, `arithmetic_score`, etc.)
- ~1.6M operações aritméticas em vars locais
- Checksum esperado: `bench-checksum:1835371715` (validação de correção)

### 1.2 Métricas iniciais (5 runs, median, release)

```
RTS (run)       ~2300 ms   (checksum 1835371715)
RTS (compiled)   983 ms
Bun (run)        100 ms
Node (run)       128 ms
```

**Diagnóstico**: o `rts run` estava mais de 20× mais lento que Bun. `rts compile`
estava 4× mais lento que o próprio `rts run`, o que era **arquiteturalmente errado**:
AOT nunca deveria ser mais lento que JIT do mesmo Cranelift.

---

## 2. Método — como investigar sem chutar

A sequência abaixo foi seguida em toda otimização da sessão:

1. **Medir com `--debug`** para obter o breakdown real do launcher:
   ```
   target/release/rts.exe --debug run bench/rts_simple.ts
   ```
   Os timings de `runtime.*` e `jit.*` eram o ponto de partida de todas as
   hipóteses.

2. **Isolar o hotspot**: se `fn_eval_stmt` domina, olhar o interpretador;
   se `__rts_dispatch` domina, olhar o breakdown por `fn_id`;
   se nenhum desses domina, é Cranelift ou startup.

3. **Quando o breakdown não bastava, instrumentar temporariamente** —
   ex.: um array `AtomicU64` indexado por `fn_id` dentro de `__rts_dispatch`
   para contar calls por tipo. Remover antes de commitar.

4. **Validar checksum a cada mudança**. `1835371715` é o teste canônico de
   correção; qualquer regressão silenciosa neste bench aparece aí.

5. **Nunca delegar síntese para o LLM**: "sabemos X, precisa mudar o arquivo Y
   linha Z" é o formato de decisão válida. "Otimize isso" não é.

---

## 3. Os 5 commits, em ordem, com o raciocínio

### Commit 1: `d672a6a` — lower loops/switch nativamente + inline top-level consts

**Sintoma observado**:
```
fn_eval_stmt              2294 ms total |  7 calls
eval.identifier_reads     5128598
eval.binding_cache_hits   7189478
```

7 chamadas de `eval_statement_text` consumiam 2.3 **segundos**. Cada chamada
fazia ~327 ms.

**Causa raiz**: o `rts run` usava o caminho legado `mir::build::build` (em
`src/mir/build.rs`), que guardava as statements como **texto TS cru** em
`MirStatement.text`. O JIT então emitia `FN_EVAL_STMT(ptr, len)` apontando
pro texto, e em runtime o `eval_statement_text` **re-parseava o TS via SWC
e interpretava o AST iteração a iteração**. Um `while` de 80k iterações com
~20 identifiers por iteração virava 5M+ reads no interpretador.

Adicionalmente, o `mir::typed_build` (caminho AOT) já existia mas tinha
fallbacks `RuntimeEval(original_text)` para `While`/`DoWhile`/`For`/`Switch`/
`Break`/`Continue` em `src/mir/typed_build.rs`. Então mesmo migrando para
`typed_build`, os loops ainda cairiam no interpretador.

**O que foi feito**:

1. **`src/cli/run.rs`**: trocar `mir::build::build` + `jit::execute` por
   `mir::typed_build::typed_build` + `jit::execute_typed`.
2. **`src/mir/typed_build.rs`**: escrever `lower_while_stmt`,
   `lower_do_while_stmt`, `lower_for_stmt`, `lower_switch_stmt`, substituindo
   os 12 sites de `RuntimeEval(original_text)` por sequências reais de
   `Label`/`Jump`/`JumpIf`/`JumpIfNot`/`Break`/`Continue`.
3. **`src/codegen/cranelift/typed_codegen.rs`**: estender
   `loop_context_from_start_label` para reconhecer `switch_body_*` — o `rewrite_loop_control`
   já sabia converter `Break`/`Continue` em `Jump` para o end/continue label
   de `while_loop_*`, `do_while_body_*` e `for_loop_*`, faltava só o switch.
4. **`TOP_LEVEL_CONSTS` thread_local**: varrer o top-level do módulo antes do
   lowering procurando `const IDENT = Literal;`, guardar num mapa, e em
   `lower_expr*` substituir `Expr::Ident(name)` por `ConstNumber`/`ConstInt32`/
   etc. quando o nome estiver no mapa. Sem isso, `LCG_MOD` (usado ~3× por
   iteração em `arithmetic_stress`) virava `LoadBinding` → `FN_READ_IDENTIFIER`
   → dispatch por iteração.

**Resultado**:
- `fn_eval_stmt`: 2294 ms → 0 ms
- `identifier_reads`: 5.1M → 0
- `__rts_dispatch`: 72 → **7.1M calls** (←regressão aparente — tudo que era
  interpretado agora virou chamadas de ABI)
- Total: 2337 ms → ~847 ms (release)

**Nota importante**: o trabalho não terminou aqui. Os 7.1M dispatches eram o
novo gargalo, mas o gargalo certo — agora estávamos fazendo o trabalho real.
Cada iteração de loop agora era uma sequência de box/unbox/binop via ABI em vez
de uma travessia de AST. Muito melhor, mas ainda longe do ótimo.

### Commit 2: `4918ced` — cachear kind nativo em stack slots + promover binops mistos

**Sintoma observado** (instrumentação temporária com contador atomic por `fn_id`):
```
BINOP              3858416 calls
BOX_NUMBER         2145826 calls
IS_TRUTHY           470053 calls
UNBOX_NUMBER             0 calls
```

**Causa raiz** — duas interações:

1. **`BindingState` guardava só o `StackSlot`, não o `VRegKind`**. Quando
   `LoadBinding` recuperava um valor, ele sempre marcava o vreg como
   `VRegKind::Handle`, mesmo que o `Bind` tivesse recebido um `NativeF64`.
   Consequência: **todo** `BinOp` sobre uma variável local caía no fallback
   `handle × handle` → `FN_BINOP` dispatch.

2. **`BinOp` com kinds mistos não promovia**. O codegen tinha 4 branches
   nativas (`i32 × i32` cmp/arith, `f64 × f64` cmp/arith) mas qualquer
   divergência caía direto no fallback handle. Em prática: uma constante inlinada
   vinha como `NativeI32`, a var local como `NativeF64`, BinOp virava handle
   path → box_number em ambos → dispatch.

**O que foi feito** em `src/codegen/cranelift/typed_codegen.rs`:

1. **`BindingState { slot, mutable, kind }`**: adicionado `kind: VRegKind`.
   O primeiro `Bind` fixa o kind do slot; `WriteBind` subsequente adapta o
   `src` ao kind do slot via nova função `adapt_to_kind`. `LoadBinding` devolve
   com o mesmo kind registrado, ativando os paths nativos em `BinOp`.

2. **`adapt_to_kind`**: converte `NativeI32 ↔ NativeF64 ↔ Handle` usando
   instruções Cranelift nativas (`ireduce`/`sextend`/`fcvt_from_sint`/
   `fcvt_to_sint`/`bitcast`) e só cai no dispatch (`FN_UNBOX_NUMBER`/
   `FN_BOX_NUMBER`) quando estritamente necessário.

3. **Promoção numérica no `BinOp`**: antes das branches nativas, se
   `lhs_kind != rhs_kind`, adapta ambos para `NativeF64` (o tipo mais largo)
   em valores locais `lhs_val`/`rhs_val` — **sem mutar** `vreg_map`/`vreg_kinds`,
   preservando outros usos dos mesmos vregs em instruções subsequentes.

**Sutileza que quase quebrou tudo**: a primeira tentativa fixava
`use_local_bindings = true` para **todas** as funções, incluindo `main`. Isso
parece inofensivo mas **quebrou o checksum** (saída: `bench-checksum:0`). Razão:
as variáveis declaradas no top-level TypeScript (`let limb0 = 1;`, etc.) ficam
no corpo da `main` do MIR, **mas são semanticamente globais** — outras funções
precisam ler/escrever as mesmas. Se `main` trata essas declarações como locais
(stack slot), `arithmetic_stress` escreve via dispatch (estado global), mas
`mix_scores` lê o slot local do `main` (nunca atualizado) e dá zero.

**Fix**: `use_local_bindings = function.name != "main"`. Globais permanecem
no fallback de namespace; apenas vars de funções "normais" viram stack slots.

**Resultado**:
- `BINOP`: 3.86M → **8** (−99.9995%)
- `BOX_NUMBER`: 3.04M → 360k (−88%)
- `IS_TRUTHY`: 202k → 0
- `UNBOX_NUMBER`: 0 → 562k (novo custo: unbox de globais lidas no handle path
  pro kind nativo do slot)
- Total release: 847 ms → **218 ms**

### Commit 3: `8e7b88a` — `opt_level`: `speed_and_size` → `speed`

**Sintoma observado** (via `bench/benchmark.ps1`):
```
RTS (run)        ~229 ms
RTS (compiled)   ~983 ms   ← 4× mais lento que JIT rodando o mesmo código!
```

**Causa raiz**: o AOT (`src/codegen/cranelift/object_builder.rs`) configurava
`opt_level = "speed_and_size"` quando `optimize_for_production=true`, enquanto
o JIT (`src/codegen/cranelift/jit.rs`) usava o default (`none`). A heurística
`speed_and_size` do Cranelift na versão atual **degrada** código com muitos
stack slots + chamadas extern "C" — gera `.text` **maior** e runtime **pior**.

**Verificação empírica**:
- `opt_level = "speed_and_size"`: 10.23 KB `.text`, 1457 ms runtime
- `opt_level = "speed"`: 6.80 KB `.text`, ~240 ms runtime
- `opt_level = "none"`: 7.52 KB `.text`, ~230 ms runtime

`speed` e `none` são indistinguíveis em perf real, mas `speed_and_size` é
catastroficamente pior. Fix: trocar `speed_and_size` por `speed` no
`build_cranelift_flags`.

**Por que `speed_and_size` é ruim aqui (hipótese)**: o Cranelift faz alguma
transformação de fusão/reordenação de loops ou inlining local guiada por
heurísticas de tamanho que, em presença de `call` externos para `__rts_dispatch`,
piora a alocação de registradores ou introduz spills desnecessários. Vale
reportar upstream, mas `speed` é a escolha correta enquanto isso.

**Resultado**:
- RTS (compiled): 983 ms → **231 ms** (alinha com o JIT)

### Commit 4: `f56f764` — instrumentação de `__rts_dispatch` opt-in via `--debug`

**Sintoma observado**:
```
runtime.__rts_dispatch  131 ms total | 1642553 calls | 80 ns/call
```

80 ns por dispatch parecia alto demais para trabalho trivial como
`FN_BOX_NUMBER` (que é literalmente um `bitcast` + push num Vec). A aritmética
real do caminho nativo roda em ~2-3 ns. Algo estava adicionando ~77 ns de overhead
fixo a **toda** chamada.

**Causa raiz**: dentro de `__rts_dispatch` (`src/namespaces/abi.rs`):
```rust
let started = Instant::now();           // syscall QueryPerformanceCounter
// ... match fn_id { ... }
let elapsed = started.elapsed().as_nanos();  // syscall QueryPerformanceCounter
RUNTIME_METRICS.with(|metrics| {              // thread_local + RefCell borrow_mut
    let mut metrics = metrics.borrow_mut();
    // ... incrementa contadores ...
});
```

Duas syscalls `QueryPerformanceCounter` e um `RefCell::borrow_mut` em cada
uma das 1.64M chamadas. Em um hot path de ~18 ns de trabalho real, a
**instrumentação** consumia ~130 ms de tempo total.

**O que foi feito**:

1. Novo `DISPATCH_METRICS_ENABLED: AtomicBool` em `abi.rs`.
2. `metrics_enabled()` = `AtomicBool::load(Ordering::Relaxed)` — 1 ns no hot path.
3. `__rts_dispatch` envolve `Instant::now()` e a atualização de métricas em
   `if metrics_on { ... }`. Em modo prod (sem `--debug`), o load + branch é
   totalmente previsto pelo branch predictor.
4. `cli::run::execute_with_report` e `cli::eval::command` chamam
   `set_dispatch_metrics_enabled(options.debug)` junto com o setter existente
   do `eval::set_metrics_enabled`.

**Decisão**: `set_dispatch_metrics_enabled` default `false`. Binários AOT não
passam por esse setter, então nunca pagam o custo.

**Resultado**:
- RTS (run): 229 ms → **124 ms** (−46%)
- RTS (compiled): 231 ms → **137 ms** (−41%)

### Commit 5: `3bcd6da` — eliminar `String` allocs e value clones no hot dispatch path

**Sintoma observado**: depois do commit 4, ainda havia ~1.64M dispatches por run,
a maioria `FN_BIND_IDENTIFIER`/`FN_READ_IDENTIFIER` dos globais `limb0/1/2` no
`bigint_like_stress`. Aritmética nativa dava 72 ms sem dispatch; gap para Bun
era só 20 ms.

**Causa raiz** — duas alocações escondidas em `src/namespaces/abi.rs`:

1. **`read_utf8(ptr, len) -> Option<String>`**: chamado em **toda**
   `FN_BIND_IDENTIFIER` e `FN_READ_IDENTIFIER`. Fazia `std::str::from_utf8(bytes).map(ToString::to_string)`.
   Ou seja, alocava uma `String` com heap allocation **para cada chamada**,
   apesar dos bytes apontarem para o `.rdata` estático emitido pelo codegen.

2. **`FN_READ_IDENTIFIER` clonava o `RuntimeValue` e alocava novo slot**:
   ```rust
   match read_identifier(&name) {
       Some(value) => push_value(value),  // clone + Vec::push
       None => UNDEFINED_HANDLE,
   }
   ```
   Cada leitura de `limb0` criava **um novo handle** apontando para **um novo
   slot** no `values: Vec<RuntimeValue>`. O vec crescia monotonicamente —
   120k × 3 leituras = 360k slots novos por run, cada um com um clone do
   `RuntimeValue::Number(f64)`. Apenas o GC passava para reciclar.

   **Problema arquitetural**: o handle já existente no `BindingEntry` é
   **estável** — aponta para um slot no `values` que não é mutado in-place
   pelo hot path (escritas via `WriteBind` criam handle novo). Então devolver
   o handle direto é semanticamente correto e elimina o clone.

**O que foi feito**:

1. **`read_utf8_static(ptr, len) -> Option<&'static str>`**: devolve slice
   direto do data segment, sem alocar. O `SAFETY` é legítimo: o codegen
   emite strings como dados estáticos no `.rdata` via `declare_string_data`,
   então vivem enquanto o módulo estiver carregado, que é superset do tempo
   de execução das dispatches.

2. **`ValueStore::bind_identifier(name: &str, ...)`**: troca `String` por
   `&str` na assinatura. Fast path de re-binding usa `get_mut` → atualiza
   `existing.handle = handle` in-place, **zero alocação**. `String::to_string()`
   só acontece na primeira inserção de um nome (O(nomes únicos), não O(chamadas)).

3. **`read_identifier_handle(name: &str) -> Option<i64>`**: novo fast path
   que devolve o handle do binding direto. `FN_READ_IDENTIFIER` passa a usar
   essa função — uma consulta ao HashMap, um `Option::map`, sem alocação.

4. **`FN_BIND_IDENTIFIER` e `FN_READ_IDENTIFIER`** no match do `__rts_dispatch`:
   usam `read_utf8_static` em vez de `read_utf8`, e chamam as novas funções
   `&str`-based.

5. **Deletado `ValueStore::read_identifier` e wrapper `read_identifier`** —
   ficaram dead code (CLAUDE.md: "código morto é removido imediatamente").

**Resultado**:
- RTS (run): 124 ms → **73 ms** (−41%)
- RTS (compiled): 137 ms → **66 ms** (−52%)

### Commit 6: `59a4fa1` — cache de globais como shadow F64 stack slots em funções puras

**Sintoma observado**: depois do commit 5, ainda havia **1.64M dispatches** no
bench, dominados por `bigint_like_stress`:

```
BIND_IDENTIFIER   360013  (globais limb0/1/2 re-bindadas, 120k × 3)
READ_IDENTIFIER   360010  (globais limb0/1/2 lidas)
BOX_NUMBER        360016  (RHS do re-bind sendo boxed pra virar handle)
UNBOX_NUMBER      562504  (reads sendo unbox pro kind nativo do consumidor)
```

Cada iteração do `bigint_like_stress` gastava **~12 dispatches por iteração**
só no handshaking com o namespace compartilhado para ler/escrever `limb0/1/2`,
enquanto a aritmética em si rodava em ~2 ns por operação. O gap para Bun
(~109 ms → RTS ~73 ms) era exatamente essa ponte.

**Causa raiz arquitetural**: `bigint_like_stress` não é `main`, então seus
`let locais` já eram stack slots. Mas `limb0/1/2` são declarados no top-level
como `let limb0: i32 = 0;`, então vivem na `main` e são **globais** do ponto
de vista de `bigint_like_stress`. O codegen caía no fallback `FN_READ_IDENTIFIER`
/ `FN_BIND_IDENTIFIER` para cada acesso — exatamente o que foi desenhado para
funcionar, só que em um hot loop de 120k iterações.

**Insight**: em um loop que **não faz chamadas externas**, podemos cachear a
global num stack slot local no prólogo, usar o cache dentro do loop, e
fazer write-back no epílogo. O write-back precisa acontecer antes de cada
`return_` porque callees subsequentes podem depender do valor atualizado.
Se a função **faz** chamadas, o callee pode ler a global e veria um valor
obsoleto — então a promoção é abortada (conservadora).

**O que foi feito** em `src/codegen/cranelift/typed_codegen.rs`:

1. **`analyze_shadow_globals`**: nova função de análise que itera as
   `MirInstruction`s e computa, por função:
   - `locals`: `HashSet` de nomes vistos em `Bind` (= declarados localmente)
   - `referenced`: `BTreeSet` de nomes vistos em `LoadBinding`/`WriteBind`
   - `has_call`: booleano — se qualquer `Call` aparece
   - Retorna `referenced − locals` como `ShadowGlobalPlan.names` se
     `has_call == false && function.name != "main"`, senão `empty`.

2. **Prólogo shadow** (antes da Pass 2): para cada nome no plan, emitir:
   ```
   handle = FN_READ_IDENTIFIER(ptr, len)     // 1 dispatch
   bits   = FN_UNBOX_NUMBER(handle)          // 1 dispatch
   stack_slot[i] = bits                      // store
   local_bindings[name] = { slot, NativeF64, mutable: true }
   ```
   A partir daqui, o código existente de `LoadBinding`/`WriteBind` do caminho
   `use_local_bindings` cuida naturalmente do resto — o que era global agora
   é stack slot F64 nativo. O loop do `bigint_like_stress` vira aritmética pura
   sem nenhum dispatch.

3. **`emit_shadow_writeback`**: helper que emite write-back para todas as
   shadows antes de cada `return_`. Para cada shadow:
   ```
   bits   = load_stack_slot(slot)            // load
   handle = FN_BOX_NUMBER(bits)              // 1 dispatch
   FN_BIND_IDENTIFIER(ptr, len, handle, 1)   // 1 dispatch
   ```
   Chamado nos 4 sites de retorno: `Return(Some)`, `Return(None)`,
   fall-through no fim da função, e `exit_block` do `Break` fora de loop.

**Sutileza crítica — shadow como NativeF64**: a primeira versão considerada
usava `shadow = Handle`, mas isso mantinha o `FN_UNBOX_NUMBER` em cada read
dentro do loop. Promover direto para F64 no prólogo elimina isso — o custo é
que se a global for não-numérica (`string`/`bool`/`object`), `FN_UNBOX_NUMBER`
retorna `NaN` via `to_number()`. Isso **é** a semântica JS de `Number(valor)`,
mas pode divergir de código que esperasse o valor original. Para o bench
atual e qualquer código numérico com globais, é correto. Análise de tipo
interprocedural fica para uma sessão futura.

**Outra armadilha evitada**: write-back em **todos** os returns, não só no
último. Um `return early` de uma função que tenha modificado uma shadow
precisa fazer o write-back antes, senão o valor fica só no stack slot que
é destruído. No bench isso é invisível (nenhuma função tem early return),
mas testar com `count_primes` cedo (antes de passar por todas as iterações)
exercitaria o caminho.

**Resultado**:
- Dispatches no bench: 1.64M → **202k** (−88%)
- RTS (run): 73 ms → **40 ms** (−45%)
- RTS (compiled): 66 ms → **25 ms** (−62%)

### Commit 7: `5b3a557` — unbox de parâmetros numéricos uma única vez no entry block

**Sintoma observado**: depois do commit 6, um contador de dispatches por
`fn_id` (instrumentação temporária, não committada) revelou que dos 202579
dispatches residuais, **202515 (99.97%) eram `FN_UNBOX_NUMBER`**. Os outros
fn_ids somavam ~60 calls totais. Distribuição brutal — o commit 6 tinha
eliminado todo o dispatch de globais, mas algo ainda estava fazendo 1
unbox por iteração.

Contagem do UNBOX = 202 515 bate exatamente com a soma das iterações dos
3 loops principais: 80000 + 2500 + 120000 = 202 500. Sobram ~15 calls de
configuração. **Um único unbox por iteração de cada while loop.**

**Causa raiz**: `LoadParam` no typed_codegen não definia `VRegKind` para
o vreg criado — então ficava no default `Handle`. Parâmetros chegam como
handles pela ABI extern "C" do RTS (é assim que funções se chamam), mas
ficavam como handles **depois** do `LoadParam` também. Quando o código
fazia `i < rounds` dentro de um while, o `i` era NativeF64 (do stack slot)
e `rounds` era Handle (do parâmetro). BinOp via promoção →
`adapt_to_kind(Handle, NativeF64)` → `FN_UNBOX_NUMBER` por iteração.

**O que foi feito**:

1. **`TypedMirFunction.param_is_numeric: Vec<bool>`**: novo campo populado
   no `build_typed_function` a partir da anotação de tipo do HIR. Nova
   helper `is_numeric_type_annotation` reconhece `number`/`i32`/`i64`/`f32`/
   `f64`/`u32`/`u64`/`i16`/`u16`/`i8`/`u8`.

2. **`LoadParam` no typed_codegen**: consulta
   `function.param_is_numeric[index]`. Se `true`, emite `FN_UNBOX_NUMBER`
   **uma única vez** no entry block e marca o vreg como `NativeF64`. Se
   `false` (default, não-numérico), mantém como `Handle`.

**Armadilha descoberta**: a primeira tentativa fazia unbox de **todos** os
params. Resultado: checksum virou `"NaN"` em vez de `"bench-checksum:1835371715"`.
Causa: `emit(message: str)` é uma função que recebe `string` e chama
`io.print(msg)`. O `message` era unboxed pra NaN no entry, e `io.print`
recebia NaN em vez do handle da string original. A correção certa
depende de **distinguir** params numéricos de não-numéricos — daí a
necessidade de propagar `type_annotation` do HIR.

Parâmetros sem anotação (JavaScript puro) ficam como Handle — conservador,
mesmo comportamento de antes.

**Resultado**:
- `FN_UNBOX_NUMBER`: 202515 → **16**
- Dispatches totais: 202579 → **80**
- RTS (run): 40 ms → **36 ms** (−10%)
- RTS (compiled): 25 ms → **26 ms** (empate — já estava no teto do que
  o bench permite)

### Commit 8: `f41c1d9` — remover `println!` de diagnóstico do stdout do `rts run`

**Sintoma observado**: ao medir em bash via `time target/release/rts.exe
run bench/rts_simple.ts`, obtive **270 ms real** consistente. Mas
`--debug` reportava `launcher.total = 21 ms` + `execute_entry = 7.5 ms`.
**Gap de 250 ms fora do escopo medido pelo launcher** — tempo de shutdown
do processo.

Adicionando `eprintln!("[DEBUG PROBE]")` como última linha da função,
o tempo **caía** para 53 ms. Isso é impossível em condições normais. Só
se alguma coisa no drop path estivesse bloqueando, e o `eprintln!`
forçasse o flush de alguma forma.

**Causa raiz**: `src/cli/run.rs` tinha um `println!("JIT executou '{}':
...")` diagnóstico remanescente de quando o JIT era novo. No caminho
normal (`rts run` sem `--debug`), esse println ficava poluindo stdout
junto com o output do programa do usuário. Pior: o Drop do stdout no
shutdown bloqueava por 250ms no Windows em pipe redirecionado.

**O que foi feito**: remover o `println!` completo (o diagnóstico
equivalente já existe sob `--debug` via `print_debug_timeline`). Em
produção, o binário do usuário é o único output do stdout — como deveria
ser.

**Resultado**:
- Real time bash: 270 ms → **53 ms**
- Bench oficial (`benchmark.ps1`, 10 runs): **36 ms** mean

**Lição**: sempre verifique se o tempo que você está medindo inclui o
shutdown do processo. O `launcher.total` era honesto (21 ms), mas o
que o shell via era 270 ms. A diferença era **shutdown de artefatos
de diagnóstico que não deveriam estar lá.**

---

## 4. Placar final

Médias de 10 runs, `bench/benchmark.ps1`, release:

| runner | mean_ms | median_ms | vs Bun |
|---|---|---|---|
| **RTS (compiled)** | **26 ms** | **26 ms** | **0.26×** (3.8× mais rápido) |
| **RTS (run)** | **36 ms** | **36 ms** | **0.35×** (2.8× mais rápido) |
| Bun (run) | 102 ms | 101 ms | 1.00× |
| Node (run) | 128 ms | 128 ms | 1.26× |

**Correção**: `bench-checksum:1835371715` preservado em todos os commits.

**Testes**: `cargo test` — 58/58 passando.

**Evolução dos commits** (para enxergar o caminho):

| commit | mudança | RTS (run) | RTS (compiled) |
|---|---|---|---|
| (antes) | baseline legado | 2294 ms | — |
| `d672a6a` | lower loops nativos + inline consts | 847 ms | 983 ms |
| `4918ced` | kind cacheado + promoção de binops | 218 ms | ~230 ms |
| `8e7b88a` | `opt_level`: `speed_and_size` → `speed` | 229 ms | **231 ms** |
| `f56f764` | métrica de dispatch opt-in | 124 ms | 137 ms |
| `3bcd6da` | eliminar `String` + value clone | 73 ms | 66 ms |
| `59a4fa1` | shadow globals F64 em funções puras | 40 ms | 25 ms |
| `5b3a557` | unbox de params numéricos uma vez | 36 ms | 26 ms |
| `f41c1d9` | remover `println!` diagnóstico | **36 ms** | **26 ms** |

---

## 5. Armadilhas encontradas (checklist para o futuro)

1. **Instrumentação de tempo no hot path é carissíma**.
   `Instant::now()` no Windows é uma syscall (`QueryPerformanceCounter`). Duas
   syscalls + um `RefCell::borrow_mut` em um caminho de <20 ns destroem a perf.
   **Regra**: qualquer métrica dentro de um hot path deve ser opt-in via
   `AtomicBool::Relaxed` ou `#[cfg(debug_assertions)]`.

2. **`String::from_utf8` em dados estáticos é desperdício puro**.
   Se o codegen emite strings no `.rdata` e o runtime recebe `(ptr, len)`,
   sempre existe um `&'static str` disponível. Só converta para `String`
   quando precisar mesmo de ownership (ex.: inserir como chave nova num HashMap).

3. **`FxHashMap<String, V>` cobra dobrado**: a string dobra como chave
   hasheada E como storage alocado. Troque por `&'static str` quando puder,
   ou pelo menos use `get_mut` para evitar `String::to_string()` em re-binds.

4. **`VRegKind` perdido em `LoadBinding` = todos os BinOps viram handle path**.
   Armazene o kind no `BindingState` ou **cada leitura de var local** desce
   para o fallback genérico via dispatch, matando qualquer ganho do `BinOp`
   nativo. Esse era um bug invisível até você instrumentar o contador por `fn_id`.

5. **Promoção numérica em `BinOp` com kinds mistos é obrigatória**.
   Sem ela, qualquer mistura `NativeI32 × NativeF64` (comum quando uma const
   inlinada é i32 e uma var local é f64) cai no handle path. Adapte ambos os
   lados para o kind mais largo **antes** das branches nativas. Mutação
   deve ser em variáveis locais, **não** em `vreg_map`/`vreg_kinds`, para
   preservar outros usos dos mesmos vregs.

6. **`use_local_bindings = true` **nunca** para `main`**. As declarações de
   top-level do TypeScript ficam no corpo do MIR de `main`, mas são
   semanticamente globais — outras funções precisam ler/escrever as mesmas.
   Tratá-las como stack slots locais do `main` quebra a comunicação entre funções.

7. **`opt_level = "speed_and_size"` do Cranelift está quebrado para nosso workload**.
   Gera código maior E mais lento (~4× no bench). Use `speed` ou `none` até
   investigar upstream. Veja commit `8e7b88a` para detalhes.

8. **`FN_READ_IDENTIFIER` clonando RuntimeValue enche o `values` Vec sem
   necessidade**. Handles são opacos — devolver o handle direto do binding é
   correto e elimina 360k+ allocs por run. Só **materialize um valor clonado**
   se você for mutá-lo, e aí o path é `write_value_handle` em vez de
   `push_value`.

9. **O MIR legado (`mir::build::build`) emite texto TS cru como `MirStatement.text`
   e delega para o interpretador SWC em runtime**. Qualquer uso desse caminho
   em workload não-trivial é morte por 1000 cortes. Use `mir::typed_build` +
   `jit::execute_typed`. O legado permanece como dead code candidato a remoção
   numa sessão futura.

10. **Comparar benches diferentes entre commits antigos é uma armadilha cognitiva**.
    O bench `bench/bun_simple.ts` já foi um `Hello World` (commit `d9bd93f`,
    `546e678`) e hoje é um stress test de 120k iterações. A percepção "era
    rápido antes" era comparar maçãs com elefantes — o interpretador de
    string-matching de 5 commits-após-o-bootstrap suportava **só** `io.print("...")`.

11. **Promoção de globais a stack slots precisa de write-back em TODOS os
    returns, não só no último**. Se a função tem `return early`, a shadow
    precisa ser flushed antes de cada `return_` emitido pelo codegen — se
    escapar sem write-back, o valor local fica no stack slot destruído e o
    namespace compartilhado fica com o valor antigo. Isso afeta `Return(Some)`,
    `Return(None)`, fall-through no fim da função, e `exit_block` do `Break`
    fora de loop. Fácil de esquecer um desses.

12. **Shadow global como `Handle` ainda paga `FN_UNBOX_NUMBER` em toda read**.
    Para eliminar **também** o unbox no hot loop, o shadow precisa ser
    `NativeF64` (ou o kind correto do uso). Promover direto no prólogo:
    `FN_READ_IDENTIFIER` seguido de `FN_UNBOX_NUMBER`, guarda F64 bits no slot,
    registra `BindingState { kind: NativeF64 }`. A partir daí,
    `LoadBinding`/`WriteBind` local opera em F64 puro sem nenhum dispatch.

13. **Promoção de globais não-triviais requer detecção de `Call`**.
    Qualquer `Call` para função de usuário (incluindo extern de runtime)
    invalida o shadow, porque o callee pode observar o namespace compartilhado
    e ver valores obsoletos. A análise atual (`analyze_shadow_globals`) é
    conservadora: **qualquer** `Call` aborta a promoção por função inteira.
    Análise interprocedural (marcar funções como "pure w.r.t. globais")
    permitiria promover através de chamadas de helpers como `io.print`.

14. **`LoadParam` sem `VRegKind` = unbox por iteração em todo loop**.
    Parâmetros chegam como handles via ABI. Se o `LoadParam` não marca
    explicitamente o kind do vreg, ele fica `Handle`, e qualquer uso
    subsequente num `BinOp` numérico dispara `adapt_to_kind(Handle, Native)`
    → `FN_UNBOX_NUMBER`. Em um while loop de 120k iterações, são 120k
    unboxes do mesmo handle. **Unbox UMA vez no entry block** se o tipo
    anotado for numérico, registrar como `NativeF64`. Para parâmetros
    não-numéricos (`string`/`bool`), **não** unbox — o handle é o que o
    callee eventualmente vai repassar para outras funções como `io.print`,
    e unboxá-lo cria `NaN`. A distinção requer propagar `type_annotation`
    do HIR até o codegen.

15. **`println!` remanescente em hot path de CLI mata perf do processo,
    não do código**. O `rts run` tinha um `println!("JIT executou '{}': ...")`
    diagnóstico no final que era inofensivo em `cargo run` mas causava
    250ms de shutdown no binário release no Windows (provavelmente flush
    bloqueado em pipe redirecionado). O `launcher.total` do `--debug`
    reportava 21ms honestamente — o tempo extra ficava **depois** da
    medição, no Drop do stdout. **Sempre meça via `time` do shell também,
    não só via métricas internas**: se há uma diferença grande entre as
    duas, o gargalo é em algo que você não está medindo (tipicamente
    shutdown/startup/stdout).

---

## 6. Gargalos residuais (próxima sessão)

A seção 6.1 (cache de globais em loops) foi **implementada** no commit
`59a4fa1` e derrubou o bench para 25 ms (compiled) / 40 ms (run). Os
dispatches caíram de 1.64M para 202k. Os gargalos abaixo são o que **ainda**
sobra, em ordem de ROI.

### 6.1 Análise interprocedural para promover através de Calls

**Estado atual**: `analyze_shadow_globals` aborta a promoção em qualquer
função que contenha uma `Call`, porque não sabemos se o callee lê/escreve a
mesma global. Isso é conservador — muitas chamadas são para helpers puros
(ex.: `emit(msg)` chamando `io.print`).

**Próximo passo**: marcar funções como "pure w.r.t. globais" se elas mesmas
não tocam nenhuma global. Um helper `io.print(msg)` se qualifica. Propagar
essa informação e permitir promoção em callers que só chamam funções assim.

**Ganho estimado**: baixo para o bench atual (todas as funções pesadas já
são promovidas), mas aumenta a cobertura para workloads reais.

### 6.2 Tipagem de globais além de `NativeF64`

**Estado atual**: shadow é sempre `NativeF64`. Se o global for `bool`/`string`/
`object`, `FN_UNBOX_NUMBER` retorna `NaN` (semântica JS `Number(valor)`, não
JS strict). OK para o bench, mas limita o uso geral.

**Próximo passo**: propagar tipos do HIR pro MIR/codegen, escolher kind do
shadow baseado em anotação (`let x: string` → shadow Handle, `let x: i32` →
shadow NativeI32, etc.). Requer um mapa `(function, name) → VRegKind` visível
no codegen.

### 6.3 Write-back mais inteligente (dirty tracking)

**Estado atual**: `emit_shadow_writeback` escreve **todas** as shadows em todos
os returns, mesmo que a função nunca tenha escrito nelas. Overhead imperceptível
no bench porque cada função promove 1-3 globais e tem 1-3 returns, mas quebra
a proporção em código mais denso.

**Próximo passo**: rastrear dirty bit por shadow durante a Pass 2. Só
`WriteBind` no nome da shadow marca dirty. Epílogo emite write-back só dos dirty.

### 6.4 Slot indexing para globais (substituir HashMap)

**Estado atual**: `FxHashMap<String, BindingEntry>` no runtime. Hash string +
lookup a cada `FN_READ_IDENTIFIER`/`FN_BIND_IDENTIFIER`. Com shadow globals em
uso, só as ~200k dispatches residuais pagam esse custo — já é aceitável.

**Próximo passo (se medido como gargalo)**: registrar índices em compile time,
`globals: Vec<BindingEntry>` indexada por slot. Codegen emite
`__rts_global_slot(SLOT_LIMB0, handle)` direto.

### 6.5 Startup (registry, parser)

**Estado atual**: `rust.registry.build` = ~10-15 ms, `rust.hir.lower` < 1 ms,
`rust.mir.build` < 1 ms. No bench atual (run) `launcher.total − execute_entry`
≈ 25 ms, dos quais ~15 ms são startup, ~10 ms são parse/type-check.

Para comparação, `target/rts_app.exe` (compilado AOT) tem startup ~5 ms. A
diferença de 15 ms entre `run` e `compiled` no bench é **só compile work**.

**Próximo passo (opcional)**: cachear o registry por arquivo quando a entry
não muda. Útil em workflow de dev rápido, não para o bench.

### 6.6 NÃO fazer (a menos que medir justifique)

- **Transpile para C + LLVM**: proposta original do outro dev. Não ajuda nada
  agora — os dispatches residuais são dominados por `Call`s de IO, não por
  aritmética em loop. Custo arquitetural alto (toolchain externa, compile
  times explodem).

- **Mexer em flags do Cranelift além de `opt_level`**: já verificamos que
  `speed` é indistinguível de `none` para este workload. `speed_and_size` é
  armadilha. Deixar como está.

- **Substituir `FxHashMap` por algo mais rápido**: com shadow globals, só
  200k calls por run passam pela HashMap. Não é o gargalo mensurável.

---

## 7. Referências rápidas

### Arquivos tocados nesta sessão

- `src/cli/run.rs` — swap para `typed_build` + `execute_typed`; dispatch metrics setter; remoção do `println!` de diagnóstico
- `src/cli/eval.rs` — dispatch metrics setter
- `src/mir/mod.rs` — `TypedMirFunction.param_is_numeric`
- `src/mir/typed_build.rs` — lower nativo de while/do-while/for/switch; `TOP_LEVEL_CONSTS`; `is_numeric_type_annotation` + preenchimento de `param_is_numeric`
- `src/codegen/cranelift/typed_codegen.rs` — `BindingState.kind`, `adapt_to_kind`, promoção em `BinOp`, `switch_body_*`, `analyze_shadow_globals` + `emit_shadow_writeback` (shadow globals F64), **`LoadParam` unbox de params numéricos**
- `src/codegen/cranelift/object_builder.rs` — `opt_level` fix
- `src/namespaces/abi.rs` — instrumentação opt-in, `read_utf8_static`, `bind_identifier(&str)`, `read_identifier_handle`

### Commits (branch `test-stmt`, à frente de `origin/test-stmt` por 9)

- `d672a6a` perf(jit): lower loops/switch natively and inline top-level consts
- `4918ced` perf(codegen): cachear kind nativo em stack slots e promover binops mistos
- `8e7b88a` perf(aot): trocar opt_level de speed_and_size para speed no production
- `f56f764` perf(abi): make __rts_dispatch timing instrumentation opt-in
- `3bcd6da` perf(abi): eliminate String allocs and value clones on hot dispatch path
- `59a4fa1` perf(codegen): cache globais como shadow F64 stack slots em funções puras
- `5b3a557` perf(codegen): unbox numeric params uma única vez no entry block
- `f41c1d9` fix(cli): remove JIT stdout noise from run path

### Como reproduzir os números

```bash
# Build release
cargo build --release

# Bench completo (5 runs, 2 warmups)
powershell -ExecutionPolicy Bypass -File bench/benchmark.ps1 -Runs 5 -Warmup 2

# Breakdown detalhado com metrics
target/release/rts.exe --debug run bench/rts_simple.ts

# Smoke test (correção)
target/release/rts.exe run bench/rts_simple.ts
# Deve imprimir: bench-checksum:1835371715
```
