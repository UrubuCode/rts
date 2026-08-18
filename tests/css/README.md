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

**2026-08-18: 7 das 42 fixtures passam**, a 1px de tolerância. São 256 desvios
em 35 ficheiros. A 2px passam 8; a 5px, 9 — a tolerância comprou quase nada, o
que diz que o que falha, falha por mecanismo e não por arredondamento.

**A entrada foi verificada**: as 42 fixtures que existem foram as 42 medidas no
Chrome, todas a 1280x800 e nenhuma a transbordar a altura do viewport (uma
barra de scroll estreitaria o layout em ~15px e falsificaria todas as larguras
percentuais dessa página). O corredor conta os `*.html` que existem, não os que
correram.

**Contra que binário:** o `target/release/examples/run_fixture.exe` de
2026-08-18 17:09. O `HEAD` desta branch é `0bcbb0ef` (17:24), **15 minutos mais
recente** — e esse commit toca precisamente no grid por áreas nomeadas, uma das
fixtures na tabela abaixo. Este número não foi re-medido contra ele, e diz-se
em vez de se deixar passar por um número de `HEAD`: um número que não foi
re-medido é uma alegação. Reconstrua e volte a correr para o atualizar.

Passam: `box-sizing`, `dimensoes-percentuais`, `flex-alinhamento`,
`flex-grow-shrink`, `min-max`, `position-fixed`, `z-index`.

**As 35 que falham FICAM no corpus a falhar.** Não se apaga uma fixture nem se
ajusta um esperado para o número subir — está no `CLAUDE.md` como a regra que
não levanta, e um corpus verde por construção não é uma rede de segurança, é
um enfeite.

### O que está por trás delas

| mecanismo | fixtures | o desvio |
|---|---|---|
| altura de linha por omissão | `background-shorthand`, `cor-e-fundo`, `especificidade`, `var-fallback`, `heranca`, `important`, `seletor-irmaos`, `seletor-atributo`, `where-vs-is` | uma caixa de uma linha de texto mede 20.8 e o Chrome diz 18 — a nossa `line-height` inicial é um fator fixo onde o Chrome usa a métrica da fonte. **É a causa isolada mais cara do corpus**: as três fixtures de seletores acertam TODAS as cores e falham só por isto |
| `border-<lado>` como longhand | `border-lados` | `border-top: 10px solid` não acrescenta nada à altura: a caixa mede 20 onde o Chrome diz 30, e os quatro lados diferentes dão 200x20 onde o Chrome diz 206x24 |
| `clear` por lado | `clear`, `float-clear` | `clear: right` desce abaixo do float ESQUERDO (y=95 onde o Chrome diz 40) — os três valores comportam-se como um só, e `clear: none` também desce |
| `vertical-align` | `vertical-align` | ausente: os sete `inline-block` de alturas diferentes ficam todos em y=0, onde o Chrome os espalha entre 7.25 e 19.91 |
| folha de estilo do agente para listas | `list-style-type` | um `<li>` começa em x=0 e mede 1280 de largura; o Chrome dá x=40 e 1240, porque o `<ul>` traz `padding-inline-start: 40px` da folha de UA |
| especificidade de `:is` / `:where` | `where-vs-is` | `:is(.marca)` perde para uma `div` escrita depois (devia valer 0,1,0) e `:is(p, #nao-existe)` não conta pelo `#id`; a metade do `:where` já está certa |
| `computedProperty` não resolve o valor INICIAL | `computed-valor-inicial`, `overflow`, `opacity-visibility`, `flex-row`, `flex-column`, `flex-wrap`, `grid-colunas`, `grid-linhas-gap`, `float-clear` | uma propriedade que ninguém declarou responde `""` onde o browser responde `block` / `visible` / `static` |
| colapso de margens | `box-model`, `margin-collapse`, `padding-border`, `position-absolute` | margens verticais adjacentes somam-se em vez de colapsarem, e a do primeiro filho não atravessa um pai transparente |
| `float` | `float-clear`, `clear` | o float não sai do fluxo (o pai só de floats mede 60 e devia medir 0) e o `clear` não empurra |
| `inline` vs `inline-block` vs `none` | `display-basico` | `inline` aceita `width`/`height` (300x300 onde o Chrome dá 26x19), `inline-block` computa como `inline`, e `display: none` continua a ocupar 255px de fluxo |
| deslocamento de `position` | `position-relative`, `position-absolute` | `top`/`left` num relativo não deslocam; `top:0;left:0;right:0;bottom:0` num absoluto dá 0x0 em vez de esticar |
| `em` / `rem` / `%` em `font-size` | `font-size-unidades` | `150%` computa 20px em vez de 30px — a percentagem não usa o pai como base |
| `white-space` | `white-space` | `nowrap` e `pre` não mudam a quebra: as quatro caixas medem 20 de altura onde o Chrome dá 20, 20, 40 e 40 |
| medida de texto | `text-align`, `largura-auto`, `letter-spacing` | a nossa largura de 5 caracteres monospace é 28.8 onde o Chrome dá 26.39, e o `letter-spacing` não entra na medição de todo |
| `grid-template-areas` | `grid-areas` | os itens da linha do meio ficam com altura 0 e o rodapé sobe para y=60 em vez de y=360 |
| `line-height` computado | `line-height` | responde o fator cru (`2`) onde o browser responde o valor resolvido (`32px`) |

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
