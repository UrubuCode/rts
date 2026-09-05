# Reftests do Web Platform Tests contra o nosso motor

A quarta régua do CSS, e a que os browsers usam entre si: um **reftest** do
[WPT](https://github.com/web-platform-tests/wpt) é um `test.html` com um
`<link rel="match" href="ref.html">`, e o contrato é "os dois renderizam
igual". Não precisa de Chrome nem de Edge: rasterizamos os DOIS com o
`claude-raster` (o rasterizador headless da régua de pintura) e comparamos
pixel a pixel. É auto-consistência — a mesma pergunta que o `wptrunner` faz.

## Correr

```bash
# um checkout esparso: `css` INTEIRO, mais `resources` e `fonts`
git clone --filter=blob:none --depth 1 --sparse https://github.com/web-platform-tests/wpt.git $TEMP/wpt
cd $TEMP/wpt && git sparse-checkout set css resources fonts

```

> **O checkout e PARTILHADO, e encolhe-lo em silencio ja aconteceu.** Este
> `git sparse-checkout set` e um passo de INSTALACAO, nao um comando para
> repetir: correr uma lista mais curta com a arvore ja alargada apaga as
> pastas que ficam de fora, e uma medicao a decorrer passa a contar menos
> testes **sem falhar** — em 2026-09-05 `css/CSS2` caiu de 6241 reftests para
> 102 a meio de uma varredura, por isto. A lista antiga era
> `css/css-flexbox css/CSS2/floats css/css-position css/css-grid/alignment
> css/css-text/white-space resources`, e e por causa dela que `css-flexbox`
> media 489 reftests em vez de 870: os testes cujo `rel=match` aponta para
> fora da pasta (`../support/`, `/css/support/`) eram descartados por o alvo
> nao existir em disco. Use `--esperado N` para que o corredor RECUSE medir um
> corpus que mudou de tamanho.

```bash
cargo build --release -p rts-dom --example claude-raster
bun scripts/wpt_reftests.mjs $TEMP/wpt/css/css-flexbox            # todos
bun scripts/wpt_reftests.mjs $TEMP/wpt/css/css-flexbox --max 300  # os primeiros N, por ordem de nome
bun scripts/wpt_reftests.mjs $TEMP/wpt/css/css-flexbox --filtro 'writing-mode'  # só os que casam, para ITERAR
```

Saída: `passam/total`, os 15 piores por percentagem de pixels diferentes, e
`relatorio.json` na pasta de saída (`--out`, por omissão `$TEMP/wpt-reftests`)
com os PNG de teste e referência lado a lado para olhar.

## O número, hoje

**2026-09-05, `css/css-flexbox`, os 870 reftests: 475 passam (54,6 %)**, 394
falham, 1 nao rasterizou. Medido no `main` `ce1d8f069` com o corredor recursivo
e o checkout completo; o relatorio fica em `$TEMP/wpt-baseline-main-870.json`.

**Este numero substitui um de 393/489 (80,4 %), e a diferenca nao e o motor: e
o denominador.** As duas causas estao ambas no instrumento e ambas corrigidas —
o corredor nao descia a subpastas, e o checkout esparso nao trazia as pastas de
referencia, pelo que um teste cujo `rel=match` apontava para fora da pasta era
descartado sem dizer nada. 381 reftests de `css-flexbox` nunca tinham sido
medidos. Os dois numeros **nao sao comparaveis por percentagem**, so por nome.

Historico do numero antigo, para nao se perder o que ele mediu: 193/489 a
2026-09-04 de manha, 393/489 ao fim do dia, ao longo dos lotes de flex.

## As duas suites, e as tres flags que as ligam

| suite | pares teste/referencia | flag |
|---|---|---|
| WPT (`wpt/css/*`) | `<link rel="match" href>` no teste | `--pares match` (omissao) |
| Blink (`chromium/third_party/blink/web_tests`) | `X-expected.html` ao lado de `X.html` | `--pares sufixo` |

Os web_tests do Blink descarregam-se do mesmo modo:

```bash
git clone --filter=blob:none --depth 1 --sparse https://github.com/chromium/chromium.git $TEMP/blink-wt
cd $TEMP/blink-wt && git sparse-checkout set third_party/blink/web_tests
```

Sao **4 046** reftests (`-expected.html`), mais 28 576 `-expected.png` (baselines
de pintura) e 20 114 `-expected.txt` (na maioria saida de `testharness.js`, que
e a outra regua — a que precisa do nosso motor de JS e ainda nao existe).

`--esperado N` recusa correr quando o corpus nao tem o tamanho declarado. Nao e
paranoia: um corpus silenciosamente menor e um numero mais pequeno com ar de
numero, e o honesty floor do `CLAUDE.md` chama-lhe "verify the input, not just
the output".

## Varrer tudo, e ler a arvore

```bash
bun scripts/wpt_css_todas.mjs "$TEMP/wpt/css" --out "$TEMP/wpt-css-todas"
bun scripts/wpt_css_todas.mjs "$TEMP/blink-wt/third_party/blink/web_tests" --pares sufixo --out "$TEMP/blink-todas"
bun scripts/reftests_arvore.mjs "$TEMP/wpt-css-todas" --prof 2
```

`wpt_css_todas.mjs` corre pasta a pasta (cada uma mantem o seu `relatorio.json`,
que e a unidade que se compara por nome entre duas medicoes) e imprime a tabela
por cima deles. `reftests_arvore.mjs` le esses relatorios e desenha a
hierarquia com uma barra por ramo — porque um total sozinho nao diz se o motor
faz uma area bem e outra nada, e uma pasta que este motor nem tenta baixa-o sem
dizer nada sobre o motor.

## O que este número NÃO é

- **Não é a régua do Blink.** Um reftest que passa diz "o motor é coerente
  consigo próprio nestes dois documentos"; um teste que falha diz onde a
  coerência parte — e é aí que se olha. O corpus de `tests/css/` e a régua de
  página real (`scripts/parity/`) continuam a ser as que medem contra o Chrome.
- **O texto é o `ApproxMeasurer`**, não a Ahem que o WPT assume: um teste cuja
  referência troca texto por caixas pode falhar por fonte e não por layout.
- **Sem JS**: o rasterizador não tem motor; um teste com `<script>` corre sem
  ele, e a saída diz quantos são. A família `testharness.js` (parsing, CSSOM,
  `getComputedStyle`) é outra régua, a montar sobre o `rts test` e a fachada
  DOM — ainda por fazer.
- `rel="mismatch"` fica de fora; tolerância 8/255 por canal (`--tol`).
