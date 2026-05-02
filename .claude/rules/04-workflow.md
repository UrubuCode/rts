# Workflow — convencoes, testes, debug, benchmarks

## Convencoes

- Linguagem do codigo: Rust (ingles nos identificadores)
- Linguagem de comunicacao: portugues
- Commits seguem conventional commits: `feat:`, `fix:`, `perf:`,
  `refactor:`, `docs:`, `chore:`
- Novo namespace precisa ser registrado em: `abi::SPECS` (e o
  `rts.d.ts` gerado a partir dai)
- O `rts.d.ts` so contem `declare module "rts"` — gerado a partir
  de `abi::SPECS`, CI lintao committed file contra o gerador
- Build eh via `cargo` direto — `xtask` foi removido

## Regras gerais de design

- Nao implementar APIs de alto nivel em Rust — Rust so expoe
  primitivas raw via `"rts"`
- Packages TS em `builtin/*` constroem APIs ergonomicas sobre o
  `"rts"` (nesta branch: `console/`, `globals/`, `rts-types/`)
- `rts.d.ts` so contem `declare module "rts"` — nao adicionar
  outros modulos
- Handles numericos (u64) para recursos runtime (buffers, sockets,
  strings dinamicas, etc)
- Distribuicao standalone: runtime support resolvido por objetos
  `.o/.obj` precompilados (via `RTS_RUNTIME_OBJECTS_DIR` ou pasta
  `runtime-objects` ao lado do `rts`); nao dependemos de download
  externo em tempo de build

## Progress bar em tarefas longas

Quando o usuario pede um trabalho com varias etapas (ex: novo
namespace, feature feat:js/feat:ts, fix multi-arquivo) mostra uma
barra de progresso ASCII a cada modificacao significativa,
ancorando a percepcao do usuario do quanto falta.

Formato:

```
[▰▰▰▱▱▱▱▱▱▱] 30% — descricao curta da etapa atual
```

Regras:
- 10 segmentos: `▰` preenchido, `▱` vazio. Percentual eh o valor
  real, nao o numero de segmentos (ex: 25% = 2 segmentos cheios +
  50% do 3o arredondado pra cheio).
- Atualizar a cada modificacao concreta: arquivo criado, build
  passou, test rodou, commit feito.
- Em caso de erro: prefixar `❌ erro:` e voltar a percentagem para
  o ponto onde a confianca caiu. Continuar a partir dali.
- Marco final: `[▰▰▰▰▰▰▰▰▰▰] 100% ✅ — resumo (PR #N, X/Y testes)`.

Exemplos de etapas tipicas (namespace novo):
- 10% mod.rs criado
- 25% abi.rs definido
- 45% ops.rs implementado
- 55% rt.rs criado
- 70% registrado em SPECS + mod.rs + rt_all
- 80% JIT registrado + build.rs atualizado
- 90% build passou + fixture basico ok
- 100% PR aberto/merged

## Assumindo issues do GitHub

Quando comecar a trabalhar em uma issue (ex: usuario diz "vamos
fazer a #97"), antes de codar marca a issue como assumida via
`gh issue edit` ou comentando — para que outros contribuintes saibam
que ja tem alguem trabalhando.

Forma minima: comentar na issue indicando inicio de trabalho.

```bash
gh issue comment <num> --body "Assumindo essa issue. Trabalho em andamento."
```

Quando possivel, atribuir a si mesmo via
`gh issue edit <num> --add-assignee @me` (funciona se a conta
autenticada eh collaborator do repo).

Ao terminar (PR mergeado), comentar de novo com link do PR e fechar
quando apropriado.

## Criatividade ao testar

Ao adicionar/modificar features, nao basta um teste happy-path.
Seja criativo e cubra varias variacoes de codigo na pasta `tests/`:

- Caminho normal **e** caminhos atipicos (vazio, condicional,
  aninhado, dentro de loop, dentro de try/catch, em member call,
  etc).
- Combinar a feature com features adjacentes (ex: arrow + classe,
  arrow + generics, arrow + spread).
- Casos de borda do TS/JS — undefined, null, recursao, tail call,
  identificadores comuns (`__rts*`, `this`, palavras reservadas).
- Quando uma variacao falhar e estiver fora do escopo da PR atual,
  abrir issue com o snippet minimo que reproduz e remover do teste
  ate o follow-up.

Os testes vivem em `tests/*.test.ts` (formato `rts:test`).
Reaproveite o template padrao: `__rtsCapturedOutput`, `print()`
shim, `describe()` com 1 ou mais `test()`/`expect().toBe()`.
Multiplos `test()` por arquivo sao bem-vindos pra cobrir variacoes
sem inflar o numero de arquivos.

## Como testar

```bash
cargo test                                        # testes unitarios + fixtures
cargo build --release                             # build release
target/release/rts.exe run file.ts                # executar via JIT in-memory
target/release/rts.exe compile -p file.ts output  # compilar nativo (AOT)
target/release/rts.exe apis                       # listar APIs disponiveis
```

Fixtures de codegen vivem em `tests/fixtures/*.{ts,out}`. O teste
`codegen_fixtures` compila o `.ts` e compara stdout com o `.out`
byte-a-byte. Para adicionar nova fixture:

1. `tests/fixtures/<name>.ts` — programa
2. `tests/fixtures/<name>.out` — saida esperada (LF, sem CRLF)
3. `#[test] fn fixture_<name>() { run_fixture("<name>") }` em
   `tests/codegen_fixtures.rs`

## Debug do codegen — `rts ir`

Para inspecionar o IR Cranelift gerado de qualquer programa antes
do define+compile, use o comando `rts ir`:

```bash
target/release/rts.exe ir file.ts 2>&1 | head -100
```

Imprime o IR completo de cada `user fn` mais o `__RTS_MAIN`
(top-level). Saida vai para stderr. Nao executa o programa.

**Use `-e`/`eval` para snippets** — evita criar arquivos
temporarios soltos no projeto. Imports relativos (`./mod`) nao
funcionam em eval (so' builtins `import { x } from "rts"`).

**Quando o Claude deve usar isso:** sempre que estiver debugando
desempenho ou suspeitando de codegen ineficiente. Ler o IR mostra
imediatamente:

- loops com `load`/`store` redundantes (vars nao promovidas a
  Cranelift Variable, sites sem cache de `gv`);
- subexpressoes lower duplicadas (try_operator_overload /
  try_bin_imm chamando lower_expr antes de checar se vao usar);
- `uextend` desnecessarios em comparacoes que vao direto pro `brif`;
- conversoes f64↔i32 em loop hot (literals como `1.0`
  mal-classificados);
- `global_value` repetidos para o mesmo simbolo;
- chamadas extern (calls externas) que poderiam ser intrinsics
  inline.

**Padrao de uso:**

1. Rodar bench (RTS lento? conferir gap com Bun/Node).
2. `rts ir file.ts 2>&1 | sed -n '/<fn-de-interesse>/,/^---/p'` —
   isolar a fn problematica.
3. Olhar `block` que eh header/body do hot loop. Procurar:
   - quantos `load`/`store` por iteracao (idealmente 0 para vars
     locais);
   - quantos `call` (cada call extern eh caro);
   - duplicacao de subexpressoes (mesma `fmul`/`fadd` repetida).
4. Identificar a causa no codegen (`src/codegen/lower/`) e
   corrigir.
5. Re-dump pra confirmar; rodar `cargo test --release --lib` +
   `target/release/rts.exe test` pra garantir 0 regressao.

**Exemplo real (commit 4a418d1):** `x*x + y*y <= 1.0` em loop
tinha 6× `fmul x x` + 3× `fmul y y` + 3× `fadd` no IR —
`try_operator_overload` e `try_bin_imm` faziam lower duplicado de
subexprs antes de saber se iam usar. Fix reduziu pra 1× cada (~6%
mais rapido em Monte Carlo).

## Benchmarks

Benches canonicos em `bench/`:

- `monte_carlo_pi.ts` — estimacao de pi por Monte Carlo 10M
  (xorshift64 inline)
- `pi_bigfloat.ts` — pi via Machin 30 digitos usando `bigfloat`
- `pi_machin.ts` — pi via Machin em f64 (16 digitos)

Placar atual (medianas, atualizado 2026-05-01):

| Bench                       | RTS JIT | RTS AOT | Bun    | Node    |
|-----------------------------|---------|---------|--------|---------|
| Monte Carlo 10M             | 26.8 ms | 16.9 ms | 91.8 ms| 113.9 ms|
| Monte Carlo 10M (8 workers) | 30.3 ms | —       | 147.6 ms (Workers) | — |

RTS AOT vs Bun: **5.14× mais rapido**. RTS multi-thread vs Bun
Workers: **4.66× mais rapido**.

HTTP server (issue #399 + actix-web): pico **29k req/s** (78% do
actix puro Rust em mesmo workload, 2× mais que `Bun.serve`).

Suite completa:

```bash
powershell.exe -ExecutionPolicy Bypass -File bench/benchmark.ps1
```
