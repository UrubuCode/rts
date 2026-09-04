# Corpus de fixtures CSS — a rede de segurança por comportamento

Páginas pequenas, uma por mecanismo, com o resultado esperado **medido no
Chrome**. Existe para preencher o meio que faltava entre os 232 testes
unitários do `rts-dom` e as páginas reais de 2 MB: nos primeiros vê-se que uma
função devolve o que devolve, nas segundas vê-se que "algo está torto"; aqui
vê-se **o quê**, porque cada ficheiro tem uma coisa só em jogo.

Quando alguém implementar `background` e partir `border`, é aqui que aparece.

---

## O Chrome é a RÉGUA

Nunca uma capacidade nossa. O ficheiro `claude-<nome>.esperado.json` ao lado de
cada fixture foi extraído de um Chrome real, a 1280x800, com
`getBoundingClientRect()` e `getComputedStyle()` — e **não** foi escrito à mão a
partir da especificação nem do que o nosso motor respondeu.

A diferença é o valor todo deste corpus. Um esperado escrito à mão fixa o que
quem o escreveu *acha* que o CSS faz, e a especificação erra na borda com mais
frequência do que se admite; um esperado medido fixa o que o browser que os
utilizadores têm realmente faz. `scripts/css_fixtures_medir.md` é o
procedimento.

---

## Correr

```bash
bash scripts/css_fixtures.sh                  # tolerância 1px
CSS_TOL=2   bash scripts/css_fixtures.sh      # tolerância 2px
CSS_FILTRO=flex bash scripts/css_fixtures.sh  # só o que tem "flex" no nome
CSS_VERBOSE=1   bash scripts/css_fixtures.sh  # lista também as que passam
```

Não constrói nada — usa o `target/release/examples/run_fixture.exe` que já
existe, porque um `cargo build --release` são minutos e o `CLAUDE.md` proíbe-o
no laço de iteração.

---

## O número, hoje

**2026-09-04: 41 das 49 fixtures passam**, a 1px de tolerância. São 23 desvios
em 8 ficheiros — o denominador cresceu de 42 para 49 desde a última vez que
este número foi gravado aqui (2026-08-18: 7/42); a tabela abaixo não compara
as duas medições ponto a ponto, só regista a mais recente.

**Contra que binário:** `target/release/rts.exe`, construído em 2026-09-03
sobre o commit `fc84d04f`, via `examples/claude-css-runner.ts`.

**As 8 que falham FICAM no corpus a falhar** — a regra que não levanta,
`CLAUDE.md`: não se apaga uma fixture nem se ajusta um esperado para o número
subir, porque um corpus verde por construção não é uma rede de segurança, é um
enfeite.

```
claude-clear                claude-position-absolute    claude-vertical-align
claude-float-clear          claude-position-relative    claude-white-space
claude-grid-areas           claude-text-align
```

### O que está por trás delas

Só os mecanismos das 8 fixtures que ainda falham hoje (2026-09-04). As linhas
sobre `background-shorthand`/`cor-e-fundo`/`especificidade`/`heranca`/
`important`/`seletor-*`/`where-vs-is` (altura de linha por omissão),
`border-lados` (border-<lado> como longhand), `list-style-type` (folha UA de
listas), `where-vs-is` (especificidade de `:is`/`:where`),
`computed-valor-inicial`/`overflow`/`opacity-visibility`/`flex-*`/`grid-colunas`/
`grid-linhas-gap` (`computedProperty` do valor inicial), `box-model`/
`margin-collapse`/`padding-border` (colapso de margens), `display-basico`
(inline/inline-block/none) e `font-size-unidades` (em/rem/% em font-size)
saíram porque nenhuma fixture que citavam continua na lista de falhas —
mecanismo resolvido, não fixture apagada (a fixture continua no corpus e
corre; só deixou de precisar de linha aqui).

| mecanismo | fixtures | o desvio |
|---|---|---|
| `clear` por lado | `clear`, `float-clear` | `clear: right` desce abaixo do float ESQUERDO (y=95 onde o Chrome diz 40) — os três valores comportam-se como um só, e `clear: none` também desce |
| `vertical-align` | `vertical-align` | ausente: os sete `inline-block` de alturas diferentes ficam todos em y=0, onde o Chrome os espalha entre 7.25 e 19.91 |
| `float` | `float-clear`, `clear` | o float não sai do fluxo (o pai só de floats mede 60 e devia medir 0) e o `clear` não empurra |
| deslocamento de `position` | `position-relative`, `position-absolute` | `top`/`left` num relativo não deslocam; `top:0;left:0;right:0;bottom:0` num absoluto dá 0x0 em vez de esticar — confirmado como dívida aberta pela auditoria estrutural de 2026-09-04 ("`position:relative` nunca desloca a pintura", frente B) |
| `white-space` | `white-space` | `nowrap` e `pre` não mudam a quebra: as quatro caixas medem 20 de altura onde o Chrome dá 20, 20, 40 e 40 |
| medida de texto | `text-align` | a nossa largura de 5 caracteres monospace divergia do Chrome; `largura-auto` e `letter-spacing`, medidos pelo mesmo mecanismo, já passam — `text-align` é o que resta |
| `grid-template-areas` | `grid-areas` | os itens da linha do meio ficam com altura 0 e o rodapé sobe para y=60 em vez de y=360 |

---

## Acrescentar uma fixture

1. **Um `.html` autocontido**, pequeno, com o CSS embutido, isolando **UM**
   mecanismo. Prefixo `claude-`, nome em português a dizer o que fixa
   (`claude-flex-wrap.html`). O comentário no topo diz o que a fixture fixa e
   porquê — não o que o CSS já diz.
2. **`id` em tudo o que interessa.** É o identificador estável entre o Chrome e
   o nosso motor; um elemento sem `id` não é medido.
3. **Altura explícita e sem texto** quando a fixture é sobre geometria. Misturar
   a métrica da fonte faz um desvio de `padding` parecer um desvio de fonte, e
   uma fixture que não separa as duas coisas não vale a pena.
4. **Se a fixture é sobre um valor de estilo e não sobre geometria**, declare-o:

   ```html
   <meta name="fixar-estilo-em" content="nome,hex3,hex6">
   <meta name="fixar-estilo" content="color,background-color">
   ```

   `fixar-estilo` diz **quais propriedades** comparar, `fixar-estilo-em` **em
   que elementos** (omitido, são todos). Sem `fixar-estilo`, compara-se só a
   geometria.

   Isto é um estreitamento deliberado do que se COMPARA, e não um ajuste do
   esperado. Existe porque o nosso `computedProperty` devolve `""` para uma
   propriedade não declarada, então comparar as 23 propriedades em todos os
   elementos enterrava a geometria debaixo de trezentos
   `esperado block → obtido ""` de caminho para outra coisa. Esse `""` é um
   desvio real e está pinado, uma vez, em
   `claude-computed-valor-inicial.html` — que é a fixture que existe para isso.
5. **Meça no Chrome** (`scripts/css_fixtures_medir.md`) e grave o
   `.esperado.json`. Se a fixture falhar, **fica a falhar** e o número acima é
   corrigido para a incluir.

---

## O denominador

O corredor conta as fixtures que **existem** (`*.html` na pasta), não as que
correram. Uma fixture sem `.esperado.json` aparece numa linha própria —
`SEM ESPERADO (contam no denominador e não passam)` — em vez de sair
discretamente da conta. É a regra do `CLAUDE.md`, "verifique a entrada, não só
a saída", aplicada a este corpus: um número medido contra um conjunto
silenciosamente mais pequeno do que o anunciado é uma alegação vestida de
medição.

---

## Os ficheiros

```
tests/css/claude-*.html            a fixture
tests/css/claude-*.esperado.json   o que o Chrome mediu (nunca escrito à mão)
examples/claude-css-runner.ts      o corredor, sobre rts:dom
examples/claude-css-probe.ts       uma fixture só, para diagnosticar
scripts/css_fixtures.sh            o comando
scripts/css_fixtures_serve.ts      serve tests/css/ para o Chrome medir
scripts/css_fixtures_medir.md      como medir
```
