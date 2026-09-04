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

**2026-09-04 (vaga 2): 51 das 54 fixtures passam**, a 1px de tolerância —
15 desvios em 3 ficheiros. Na mesma manhã eram 49 de 49; a vaga 2 do
`crates/rts-dom/PLAN.md` acrescentou cinco fixtures: as duas de `revert`
(lote J) passam à primeira, e as três da folha de UA (lote I) ficam a FALHAR
de propósito — `claude-ua-form-disabled` (fonte e largura de texto dos
controlos), `claude-ua-headings` (um desvio de 2px no `h6`), `claude-ua-th`
(largura de texto a negrito, `tr` com `border-spacing`). São lacunas do motor
que a fixture existe para nomear, e os esperados não se ajustam.

**Contra que binário:** `target/release/rts.exe` construído sobre
`feat/dom-vaga-2`, via `examples/claude-css-runner.ts`.

**A régua destas cinco é o Edge headless** (`scripts/css_fixtures_medir_edge.mjs`,
Blink 152), porque esta máquina não tem o Chrome — e o instrumento foi
validado antes de contar: as 49 fixtures com esperado medido no Chrome,
re-medidas no Edge, deram 1 164 números com desvio 0.

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
