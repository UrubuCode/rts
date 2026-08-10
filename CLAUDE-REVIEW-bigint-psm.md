# Revisão: BigInt (`<<`, `>>`, `**`, `~`, `Number(1n)`) e a saída do `psm`

Handoff de uma sessão de 2026-08-10. Escrito para quem vai revisar o diff sem ter
acompanhado o caminho até ele.

**Este arquivo não é o `CLAUDE.md`.** O da raiz é o contrato do repositório e
continua sendo a única entrada; isto é o relatório de uma mudança e sai daqui
quando a branch entrar.

---

## O que foi pedido

Dois problemas, relatados juntos:

1. um erro que aparecia como "de linker" depois que o sistema passou a usar o
   codegen novo, batendo em `rts-host-rwk` e `rts-cli` e poupando
   `rts-core-rwk`, `rts-std-rwk`, `rts-node-rwk` e `rts-runtime-rwk`;
2. BigInt quebrado — `<<` dando `0`, `>>` dando `0`, `**` dando `NaN` e
   `Number(5n)` dando `NaN`.

Depois, no meio do trabalho: fazer o caso do número grande demais responder o
que o Node responde.

---

## Parte 1 — o "erro de linker" não era de linker

### O que está provado

`cargo check` de `rts-host-rwk` e de `rts-cli` termina **limpo no host Windows**.
Não há símbolo faltando, nem duplicado, nem staticlib estagnada — o build de duas
etapas do `rts-runtime` não tem relação com isto. O erro só existe ao **cruzar**
para Linux, e vem de `build.rs` de terceiros que compilam C/assembly **para o
target**, o que exige uma toolchain C cruzada que a máquina não tem.

### Por que só aqueles dois crates

Rota única: `swc_ecma_parser` → `stacker` → `psm`, e `stacker` é feature
**default** do swc. Os crates que passavam limpo estão ABAIXO do `rts-codegen` no
grafo; `rts-host-rwk` está acima dele (`crates/rts-codegen/Cargo.toml` declara
`swc_ecma_parser` direto). Para o `rts-cli` o `psm` já estava na árvore antes do
cutover, via `rts-parser` — **ali não é regressão do motor novo.**

### O que esta branch faz

Desliga a feature nos dois manifestos. Precisa ser nos **dois**, senão o cargo
reunifica a feature e o `psm` volta:

```toml
swc_ecma_parser = { version = "39.0.0", default-features = false, features = ["typescript"] }
```

O swc tem fallback limpo sem ela — sob `not(feature = "stacker")` o `maybe_grow`
vira `callback()` direto. Verificado: `cargo tree -p rts-cli -i psm` responde que
o pacote não existe mais no workspace, e o check segue verde.

**O que se perde:** o crescimento automático de pilha do parser em expressão
profunda. O repo já cobre isso pelo outro lado — `rts-cli/src/cli/new_engine.rs`
e `rts-host-rwk/examples/suite_run.rs` rodam o compile numa thread de 64 MB. Se um
fixture profundo estourar, a correção é subir o `STACK`, **não** trazer o `psm` de
volta. A suíte inteira rodou depois da mudança e nada estourou.

### O que NÃO está resolvido, e é o mais importante desta metade

**Tirar o `psm` não te dá o build Linux.** Com o target instalado
(`rustup target add x86_64-unknown-linux-gnu`), o check cruzado ainda falha — em
**`ring`**:

```
error: failed to run custom build command for `ring v0.17.14`
  error occurred in cc-rs: failed to find tool "x86_64-linux-gnu-gcc"
```

`ring` vem de `rustls` ← `rts-natives` ← `rts-engine` ← (`rts-cli`,
`rts-codegen-new`, `rts-dom`, …). É C e assembly de verdade: é o TLS que `fetch`,
WebSocket e HTTPS usam, e não é uma feature que dê para desligar.

**Conclusão honesta: cruzar Windows→Linux exige uma toolchain C de qualquer
jeito.** A saída do `psm` continua certa — uma dependência C a menos, e uma da
qual não dependemos de fato — mas quem quiser o binário Linux vai por
`cargo-zigbuild` (o `zig cc` faz esse cross sem instalar toolchain) ou por
container/CI Linux. Isso é decisão de projeto e **não foi tomada aqui**.

---

## Parte 2 — BigInt

### O diagnóstico, que é mais curto do que parece

Cinco divergências, **três causas**.

`bigint_class::binary` é quem decide bigint-vs-Number, e **não existe tabela de
despacho**: cada entry point faz a pergunta por conta própria, com um
`if let Some(…) = binary(…) { return … }` copiado. No HEAD:

| entry point | perguntava? |
|---|---|
| `bit_and`, `bit_or`, `bit_xor` | sim |
| `bit_not`, `shift_left`, `shift_right`, `exponent` | **não** |
| `shift_right_unsigned` | não — e está certo, `>>>` sobre bigint é `TypeError` na spec |

Sem a pergunta, o operando vai para `operators::as_number`, que devolve `None`
para um bigint, e `operands` substitui por `NaN`. Daí `to_int32(NaN)` é `0` — por
isso `<<` e `>>` davam `0` — e `powf(NaN, NaN)` é `NaN` — por isso `**` dava
`NaN`. **Mesma causa, sintomas diferentes só no último passo.**

`Number(5n)` é outra causa: `number/mod.rs` chamava a conversão compartilhada, e a
recusa dela está **certa** — é `ToNumber`, e `ToNumber(bigint)` é `TypeError` na
spec; é essa recusa que faz `1n + 1` não virar número em silêncio. O que faltava
era a carve-out: `Number(x)` usa `ToNumeric`, e `Number(1n)` é `1`.

`~1n` era a quinta divergência, não estava no relato original, e é o mesmo defeito
na forma unária.

**O front está inocente.** `emit/expr.rs` mapeia `Shl`/`Shr`/`Exponent` para o
`RuntimeOp` certo e `emit/proven.rs` recusa provar bitwise, então não há fast path
inline: todo shift chega ao entry point. O bug era inteiramente do runtime.

**A aritmética já existia e já era testada** — `BigInt::shl`/`shr` em
`bigint/bits.rs`, `pow` (square-and-multiply) em `arith.rs`. Nada de matemática
nova foi escrito; ela só não era alcançada. É por isso que o fix é pequeno.

### Representação (para quem for mexer)

Não é tag dedicada: é **client kind**, NaN-boxed carregando um `Slot(u32)` que é um
ÍNDICE num `Slab<BigInt>` no `Context`. `BigInt` é sign-magnitude
`{ negative: bool, digits: Vec<u32> }`, base 2^32, normalizado, **precisão
arbitrária sem teto**. Igualdade por VALOR e não por slot, senão `1n === 1n` seria
falso. Nunca coletado.

`9007199254740993n` e `18446744073709551617n` cabem folgado e **já respondiam
certo antes** — nenhuma das cinco divergências era de representação.

### Dois DoS que o fix teve de tratar antes de ser um fix

Isto é o que separa "passa no repro" de "pode entrar":

1. **O operando direito de `<<` e `**` é uma CONTAGEM.** `1n << 2n**40n` é uma
   linha de código e um terabyte de dígitos. Sem teto, o fix mata o processo na
   primeira contagem grande. Teto em 1 gigabit (`MAX_BITS`), que é onde o V8 para.
2. **`BigInt::shr` varria `(0..amount)`** — um passo por unidade da contagem, não
   por bit do operando. `-1n >> 2n**40n` não falharia: **nunca retornaria**. Estava
   invisível porque nada chamava `shr`; corrigir o item anterior o EXPÕE. A
   varredura agora para na largura, já que todo bit acima dela é zero.

### O caso do número grande demais: `RangeError`

Pedido depois, e implementado. `1n << 2n**40n` e `2n ** 2n**40n` lançam
`RangeError: Maximum BigInt size exceeded` (a mensagem do V8, para quem procurar o
texto achar a mesma página); `2n ** -1n` lança `Exponent must be non-negative`.
Capturável, `instanceof RangeError` verdadeiro, programa continua.

`-1n >> 2n**40n` **não** lança: shift à direita só perde bits, então contagem além
da largura satura em `-1n` — que é o que o Node responde.

#### A armadilha, e é a parte deste diff que merece revisão de verdade

A primeira tentativa **abortava o processo**. `throw::range_error` constrói o
objeto de erro através do mesmo `RefCell` do `Context`, e chamá-lo de dentro do
`with_current` dá `RefCell already borrowed` dentro de `_rts_shift_left` — num
frame que **não pode desenrolar**. Não é panic que falha um teste; é abort.

A forma final: `binary` devolve `Option<Result<u64, Refused>>` e carrega a recusa
para FORA do empréstimo; só `settled` lança. `settled` de propósito **não recebe
`Context`**, para que a regra seja difícil de quebrar sem querer.

**O ponto que o revisor precisa olhar:** `operators.rs` e `primitives.rs` chamam
`settled` DENTRO do empréstimo. Isso é são hoje e só hoje, porque `+ - * / %` e as
comparações não recusam. **Divisão por zero é o que quebra isso**: ela responde
`undefined` hoje onde a linguagem lança, e no dia em que passar a lançar — que é o
certo — esses cinco call sites têm de sair do empréstimo **na mesma mudança**.
Está escrito em `settled` e no site do `Op::Sub`.

### Doc do módulo corrigido

O doc de `bigint_class.rs` dizia que operação mista responde `NaN` "porque não dá
para lançar", já que `entry/throw.rs` encerraria o programa. **Este fix torna isso
falso** — o `RangeError` lança e o programa segue. Reescrito: agora é uma escolha,
não um limite. E a escolha é que o `TypeError` do misto tem de entrar em todos os
operadores de uma vez; pôr só nos três tocados deixaria uma regra da linguagem com
duas respostas dependendo do operador.

(O `CLAUDE.md` da raiz proíbe deixar regra que o código contradiz — por isso o doc
mudou junto e não depois.)

---

## Medição

Régua: `rts-host-rwk/examples/suite_run`, **um processo por arquivo**, release.
Baseline em `git worktree` no `HEAD` com `CARGO_TARGET_DIR` próprio — **sem
`git stash`**, que aplicaria um stash alheio se o tree estivesse limpo.

| | passa |
|---|---|
| baseline (HEAD) | 740 / 796 |
| esta branch | **741 / 797** |

**Perdidos: nenhum. Ganho: a fixture nova.** A lista de perdidos vazia é o que
autoriza dizer "sem regressão"; o número líquido nunca autorizou.

Rodada duas vezes: uma depois do fix de aritmética, outra depois do `RangeError` —
porque a segunda mudança alterou o call site de **todo** operador aritmético, não
só dos três que ganharam o throw.

Também: `cargo test -p rts-core-rwk` 229/229; `scripts/read_before_commit.sh
--no-build` sem violação dura (as listas amarelas são de `rts-codegen-new` e não
cresceram — o trabalho todo foi em `rts-core-rwk`).

Que o baseline era genuíno foi conferido rodando o repro no binário dele:
`shl=0 shr=0 pow=NaN Number(5n)=NaN`.

### Cobertura: o achado que vale mais que o número

**Ganho zero de arquivos existentes.** O fix não fez nenhum teste da suíte passar,
porque **a suíte não cobria isso**: só existem `bigint_asintn` e
`bigint_literal_as_i64`, e nenhum toca shift, `**` ou `Number(1n)`. A suíte não
protegia o fix e não pegaria a regressão dele.

Daí `tests/claude-bigint-shift-pow.test.ts`, que **passa com o fix e falha sem** —
verificado nos dois binários. Cobre também o que o repro original não pegava:
`1n << 64n` (onde `1 << 64` daria `1`), contagem negativa invertendo a direção,
`-5n >> 1n` arredondando para -infinito, `~1n`, e os três `RangeError`.

---

## Arquivos

| arquivo | o quê |
|---|---|
| `crates/rts-codegen/Cargo.toml` | desliga `stacker` |
| `crates/rts-parser/Cargo.toml` | idem — precisa ser nos dois |
| `crates/rts-core-rwk/src/entry/bitwise.rs` | a pergunta bigint em `<<`, `>>`, `**`, `~`; cada uma no seu próprio empréstimo |
| `crates/rts-core-rwk/src/entry/bigint_class.rs` | `Op::{Shl,Shr,Pow}`, teto, `settled`, doc corrigido |
| `crates/rts-core-rwk/src/entry/number/mod.rs` | carve-out do `Number(1n)` |
| `crates/rts-core-rwk/src/entry/operators.rs` | `Result` propagado + o comentário sobre divisão por zero |
| `crates/rts-core-rwk/src/entry/primitives.rs` | `Result` propagado no `+` |
| `crates/rts-core-rwk/src/bigint/bits.rs` | varredura do `shr` limitada à largura + teste que a pina |
| `crates/rts-core-rwk/src/bigint/mod.rs` | `bit_len()`, para recusar ANTES de alocar |
| `tests/claude-bigint-shift-pow.test.ts` | novo |

---

## Aberto — nada disto foi decidido aqui

1. **`TypeError` do misto.** `1n + 1` responde `NaN` e devia lançar. Vale para
   todos os operadores de uma vez, e exige mover os call sites de `operators.rs`
   para fora do empréstimo.
2. **Divisão por zero de bigint** responde `undefined` onde o Node lança
   `RangeError`. Mesma dependência do item 1.
3. **Cross para Linux**: `cargo-zigbuild` ou container, por causa do `ring`.
4. **`e.constructor.name` é `undefined`** em erros — pré-existente e não desta
   mudança (acontece igual com `new RangeError("x")` escrito à mão). `e.name` e
   `String(e)` estão corretos.
5. **`Object(1n)`** (wrapper) continua ausente, por decisão já registrada no doc
   do módulo.
