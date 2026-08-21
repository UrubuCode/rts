# Modularizar o `rts-dom` — o plano, e a régua que o valida

O `CLAUDE.md` fixa **500 linhas** por ficheiro fora do codegen. Estes passam:

| ficheiro | linhas | quantas vezes o teto |
|---|---|---|
| `layout.rs` | 9 961 | **20×** |
| `dom.rs` | 5 974 | 12× |
| `style/newprops_tests.rs` | 1 826 | 3,7× |
| `inline_box.rs` | 1 299 | 2,6× |
| `style/stylesheet.rs` | 1 101 | 2,2× |
| `style/values.rs` | 1 073 | 2,1× |

## Porquê agora, e a razão não é estética

**Um ficheiro grande serializa trabalho que podia ser paralelo.** Numa sessão com
vários agentes, cada lote que tocou o `layout.rs` obrigou a congelar toda a gente
— "pare de escrever nesse ficheiro até eu confirmar o commit" — porque um `git
add` a meio de uma edição alheia commita meio lote, e um build sobre a árvore
partilhada compila um estado que nunca existiu. Ambos aconteceram no mesmo dia.

E há um efeito medido na qualidade: **o mesmo defeito de forma apareceu em três
sítios do mesmo ficheiro** — `is_block_level`, `cria_caixa_apesar_de_inline` e
`is_inline_block`, todos a perguntar *"não é inline?"* onde a pergunta era *"é de
bloco?"*. Cada um foi encontrado por medição separada, em lotes separados. Com a
decisão num módulo só, teria sido um.

## A régua que valida um refactor de mover código

**Uma modularização pura tem de mover ZERO pixels.** O dump da página tem de
sair **idêntico byte a byte** ao de antes:

```bash
cp scripts/parity/out-img/rts.jsonl /tmp/antes.jsonl
cargo build --release -p rts-host --example run_fixture
target/release/examples/run_fixture.exe examples/claude-parity-rts.ts > scripts/parity/out-img/rts.jsonl
cmp /tmp/antes.jsonl scripts/parity/out-img/rts.jsonl   # TEM de ser silencioso
```

Se diferir num byte, não foi um `move`: foi uma alteração de comportamento
disfarçada de arrumação, e é a forma mais fácil de perder trabalho medido. A
verificação custa ~90 s de build e 10 s de medição, e é mais forte que a suite —
os testes provam que o que eles cobrem continua igual; o dump prova que **16 813
elementos** continuam iguais.

`cargo test -p rts-dom` verde é condição necessária, não suficiente.

## A decomposição proposta para `layout.rs`

77 funções de topo, testes a partir da linha 6953 (~3 000 linhas). Os nomes já
declaram as fronteiras:

| módulo | o que leva | maiores funções |
|---|---|---|
| `layout/mod.rs` | tipos públicos, `layout_document`, `layout_cached`, re-exports | `layout_document` 98 |
| `layout/display.rs` | `DisplayItem`, `DisplayList`, `Corners`, `Rect`, `ScrollRegion` | `collect_geometry` 314 |
| `layout/medida.rs` | `TextMeasurer`, `ApproxMeasurer`, `intrinsic_content_width` | `intrinsic_content_width` 205 |
| `layout/bloco.rs` | fluxo de blocos, empilhamento, reuso | `layout_children_vertical` 370, `layout_block_reusing` 214 |
| `layout/flex.rs` | eixo horizontal e coluna | `layout_children_horizontal` 296, `layout_children_column` 235 |
| `layout/grid.rs` | grid | `layout_children_grid` 259, `resolve_tracks` 91 |
| `layout/linha.rs` | fluxo inline, runs, quebra | `wrap_runs` 412, `layout_inline_flow` 343, `collect_runs` 280 |
| `layout/replaced.rs` | imagem, svg, input, widget | `layout_input` 144, `layout_image` 75 |
| `layout/consulta.rs` | geometria pedida de fora | `bounding_rect` **828** |

Nem todos ficam abaixo de 500 à primeira — `wrap_runs` sozinho tem 412 e
`bounding_rect` 828. Onde não couber, **o que não cabe é dito e datado**, não
arredondado: um ficheiro que fica em 700 com uma linha a explicar porquê é
honesto; um que fica em 700 em silêncio repõe o problema.

## A ordem, e porque é que ela importa

1. **Primeiro os testes.** Tirar `#[cfg(test)]` para `layout/tests/` corta ~3 000
   linhas sem tocar numa única linha de lógica — é o movimento com melhor razão
   risco/benefício de todos, e sozinho tira o ficheiro de 9 961 para ~6 950.
2. **Depois as folhas** — `display.rs`, `medida.rs`, `replaced.rs`, `consulta.rs`:
   pouco acopladas, cada uma verificável à parte.
3. **Por fim o núcleo** — `bloco.rs`, `linha.rs`, `flex.rs`, `grid.rs`, que
   partilham estado de passagem e são onde um `move` se pode tornar uma
   alteração sem ninguém dar por ela. **Cada um é um commit próprio com o `cmp`
   a passar.**

Um refactor grande num commit só é indistinguível de um refactor grande com um
bug lá dentro.

## O que NÃO se faz nesta arrumação

Não se corrige nada, não se renomeia nada, não se "melhora de passagem". Cada
uma dessas coisas quebra a única verificação forte que temos — o dump idêntico —
e passa a exigir medição por elemento, que é o custo que a arrumação existe para
evitar. Havendo algo a corrigir, anota-se e faz-se **depois**, num lote medido.

---

## O `dom.rs`, feito — e três correções que o próprio refactor trouxe

5 974 → **3 336 linhas** em dois passos, com **599 testes antes e depois de cada
um** e o `--numstat` no corpo de cada commit a provar que só se moveu.

| passo | resultado | linhas adicionadas ao `dom.rs` |
|---|---|---|
| 1 — testes para `dom/tests/` | 5 974 → 4 215 | **1** (`mod tests;`) |
| 2 — seis folhas | 4 215 → 3 336 | **13** (seis `mod`, dois `pub use`, três `use`) |

**1. Os testes eram 1 761 linhas, 29,5% — não "~3 000".** Este documento dizia
"~3 000" para o `layout.rs` e não media os do `dom.rs`. Um plano com números
redondos é um plano por medir, e a extração real também desmentiu a
decomposição: o módulo de testes da cascata saiu com 616 linhas e foi partido em
três em vez de entregue acima do teto.

**2. A visibilidade custou dez itens, não cinquenta e quatro — e a diferença era
culpa do plano.** Pôr o `struct Dom` num módulo próprio **cria** a necessidade de
abrir os 44 campos aos irmãos. Deixá-lo no `dom/mod.rs` custa zero: dez funções
livres passam a `pub(in crate::dom)`, que é exactamente o alcance que já tinham,
e **nenhum campo muda**.

Uma saiu da lista por medição — `closes_open_p` só é chamada dentro do próprio
ficheiro, e foi o compilador que o disse com um `unused import`. **Alargar "por
precaução" inventa uma fronteira que não existe.**

E os re-exports internos são `use` simples e não `pub(crate) use`: este último
republicava as dez em `crate::dom::*`, e o `layout.rs` passava a vê-las.

**3. Privado em Rust não é privado ao ficheiro.** É visível no módulo que define
**e em todos os descendentes**. Três blocos `impl Dom` em ficheiros próprios
leem campos privados e chamam métodos privados sem uma única anotação. Quem
partir o `layout.rs` precisa de saber isto antes de começar: metade do medo de
um refactor destes vem de assumir o contrário.

### O que não foi tocado, e porquê

Um `use super::*;` duplicado e um teste atrás de `#[cfg(feature = "metrics")]`
— este último explica 98 `#[test]` declarados contra 97 a correr, e vale saber
antes de contar testes num passo futuro. Ambos anotados, nenhum corrigido: uma
arrumação que corrige de passagem deixa de ser verificável, e a inocência de uma
mudança é precisamente o que ninguém consegue confirmar depois.

E a indentação de 4 espaços foi mantida nos ficheiros de teste, de propósito:
vários têm literais multi-linha em que o espaço à esquerda é **conteúdo**.

### O portão passou nos quatro passos: 16 813 elementos idênticos byte a byte

Quatro passos (`b91ae530`, `dc1ee62e`, `f6bbe566`, `13d12217`), **5 974 linhas
num ficheiro → 26 ficheiros, o maior com 488**, e o dump da página construído do
commit final é **byte a byte igual** ao de antes do refactor. Nenhum dos 16 813
elementos mudou um pixel.

O `dom.rs` deixou de existir. `lib.rs` está intacto — nenhum utilizador do crate
mudou uma linha. **33 itens de visibilidade em todo o refactor, todos
`pub(in crate::dom)`: zero `pub`, zero `pub(crate)`, zero campos.** O plano
original previa 54, e 44 desses eram uma necessidade que o próprio plano criava.

Isso é o que uma arrumação tem de conseguir provar, e a suite não o prova: os
604 testes dizem que o que eles cobrem continua igual; o dump diz que **a página
inteira** continua igual. `cargo test` verde continua a ser condição necessária,
não suficiente.

Uma nota sobre como o portão foi corrido, porque a árvore partilhada torna isto
não-óbvio: **os binários foram construídos em worktrees isolados nos commits**,
nunca da árvore de trabalho. Durante os três passos houve, em momentos
diferentes, um `style/parse.rs` com erro de sintaxe a meio da edição de outro
agente e dois testes vermelhos de um terceiro. Um binário feito da árvore nessas
alturas compila um estado que nunca existiu, e a medição que sair dele não é de
ninguém.

### O que este refactor rendeu além das linhas

**Uma fronteira que não existia.** O `matcher.rs` juntou as sete funções que
respondem *"este nó casa com este seletor?"*, e ao pô-las lado a lado apareceu a
regra que nenhuma delas escrevia: **três são aproximações, e podem responder
"sim" a mais, nunca "não" a mais.** Um falso positivo custa uma cascade extra;
um falso negativo perde um resultado do `querySelectorAll` ou deixa um nó com
estilo velho depois de um hover. Cada uma dizia isso à sua maneira; nenhuma dizia
que era a mesma regra nas três.

É a forma do defeito que o `layout.rs` tem **cinco vezes** — *"não é inline?"*
onde a pergunta é *"é de bloco?"* erra para o lado errado, e por isso cada cópia
falha sozinha e é corrigida sozinha, ao preço de um lote e uma medição cada.
