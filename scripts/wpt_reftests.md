# Reftests do Web Platform Tests contra o nosso motor

A quarta régua do CSS, e a que os browsers usam entre si: um **reftest** do
[WPT](https://github.com/web-platform-tests/wpt) é um `test.html` com um
`<link rel="match" href="ref.html">`, e o contrato é "os dois renderizam
igual". Não precisa de Chrome nem de Edge: rasterizamos os DOIS com o
`claude-raster` (o rasterizador headless da régua de pintura) e comparamos
pixel a pixel. É auto-consistência — a mesma pergunta que o `wptrunner` faz.

## Correr

```bash
# um checkout esparso: só as pastas que interessam (o repo inteiro é enorme)
git clone --filter=blob:none --depth 1 --sparse https://github.com/web-platform-tests/wpt.git $TEMP/wpt
cd $TEMP/wpt && git sparse-checkout set css/css-flexbox css/CSS2/floats css/css-position css/css-grid/alignment css/css-text/white-space resources

cargo build --release -p rts-dom --example claude-raster
bun scripts/wpt_reftests.mjs $TEMP/wpt/css/css-flexbox            # todos
bun scripts/wpt_reftests.mjs $TEMP/wpt/css/css-flexbox --max 300  # os primeiros N, por ordem de nome
```

Saída: `passam/total`, os 15 piores por percentagem de pixels diferentes, e
`relatorio.json` na pasta de saída (`--out`, por omissão `$TEMP/wpt-reftests`)
com os PNG de teste e referência lado a lado para olhar.

## O número, hoje

**2026-09-04, `css/css-flexbox`, os 489 reftests: 193 passam (39,5 %)** — eram
186 antes dos lotes ib-nowrap, justify-fisico e clearfix;, 0
falharam a rasterizar; 97 das 303 falhas têm menos de 0,5 % de pixels
diferentes (anti-aliasing e fonte), 30 têm mais de 5 %. Por tema no nome:
shrink 27, writing-mode 21, wrap 19, justify 16, baseline 12. **Corre no CI**
(job `dom-rulers`, WPT fixado em `972e0e10`) e o número vai para o bloco
"CSS and DOM parity" do README. Medido no motor da vaga 7 (main
`4741b63d9` + o lote inline-block). Os piores são alinhamento por baseline em
várias linhas (`flexbox-baseline-multi-line-horiz-003/004`, 36 %), tamanhos
definidos (`flexbox-definite-sizes-005`, 30 %), `writing-mode` vertical
(que este motor não tem) e os testes com `<script>` (corridos sem JS).

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
