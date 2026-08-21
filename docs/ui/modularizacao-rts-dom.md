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
