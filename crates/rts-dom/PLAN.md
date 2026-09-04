# O plano do motor HTML/CSS/DOM

O que este crate (e os que o rodeiam: `rts-dom-bridge`, `rts-egui`) ainda tem
de fazer, na ordem em que se faz, escrito para que um agente — ou uma pessoa —
possa pegar num lote sem ler mais nada primeiro, e para que quem pare a meio
saiba onde retomar.

Vive aqui e não em `docs/` pela regra 2 de `docs/README.md`: um plano vai
stale no momento em que o trabalho começa, e quem repara é quem edita o crate.
A imagem do que o motor É (o veredito, os findings, a evidência) está em
`docs/ui/html-engine/analises/2026-09-04-auditoria-estrutural/README.md`; este
ficheiro é o que se FAZ com ela. Não repete o que lá está.

---

## 0. Como retomar — LEIA ISTO PRIMEIRO

**A tabela abaixo é o estado.** Quem fecha um lote actualiza a linha dele
**no mesmo commit** que o fecha (hash, data, o que mudou nos números). Quem
começa um lote muda o estado para "em curso" com o nome da branch. Um lote sem
linha aqui não existe.

| lote | nome | vaga | estado | branch / commit | régua que o mede |
|---|---|---|---|---|---|
| A | contrato DOM→JS | 1 | ☑ integrado em `feat/dom-vaga-1` (2026-09-04) | `feat/dom-contrato-js` → `bda7e065`+`667f2609` | `cargo test -p rts-dom-bridge`; `tests/claude-dom-getboundingclientrect.test.ts` |
| B | um medidor activo | 1 | ☑ integrado em `feat/dom-vaga-1` (2026-09-04) | `feat/dom-medidor-ativo` → `4c3ee981` | testes em `rts-dom` (medidor falso) |
| C | `position` relative/absolute | 1 | ☑ integrado; corpus 6/6 desvios fechados | `feat/dom-position-relativo-absoluto` → `a2d6dde1` | corpus: `claude-position-*` (6 desvios) |
| D | grid: rows por áreas | 1 | ☑ integrado; corpus 3/3 fechados | `feat/dom-grid-areas-rows` → `161c532a` | corpus: `claude-grid-areas` (3) |
| E | BFC, floats, `clear` | 1 | ☑ integrado; corpus 6/6 fechados, suite sem perdidos | `feat/dom-bfc-floats-clear` → `cf592ba0` | corpus: `claude-clear`, `claude-float-clear` (6) |
| F | baseline, `vertical-align`, `white-space` | 1 | ☑ integrado; corpus 8/8 fechados (o 8.º por C/E) | `feat/dom-linha-baseline` → `ebe268ef` | corpus: `claude-vertical-align`, `claude-white-space`, `claude-text-align` (8) |
| G | scroll no documento | 2 | ☑ integrado; fixture verde; janela real POR MEDIR | `feat/dom-scroll-no-documento` → `951a72b2` | teste de `scrollTop`/`scrollTo` + exemplo em janela |
| H | o escopo de página não vê o Node | 2 | ☑ integrado; fixture verde; `eval` indirecto ainda vê `process` (§4.H) | `feat/dom-escopo-pagina-sem-node` → `8e30fcc1` | fixture `claude-dom-page-nao-ve-process` |
| I | folha de UA em CSS real | 2 | ☑ `style/ua.css` parseada pelo mesmo parser, origem UA na cascade (inversão no `!important`); `ua_display` fora do layout; sensibilidade a atributo por NOME; `em`/`rem` em margens no `getComputedStyle`. PARCIAL: `UA_TABLE` ainda alimenta o eixo display/iniciais; `scrollbar.rs` só desenhado. Fixtures `claude-ua-*` medidas no Blink e a FALHAR (15 desvios = lacunas do motor: largura de texto a negrito/controlos, fonte dos controlos, `tr` com `border-spacing`) | `feat/dom-lote-i-folha-ua` → `615983cf`+`1b9e817e` (2026-09-04) | `<th>` negrito/centro; `scrollbar.rs` apagado |
| J | `DeclarationRecord` e `revert` | 2 | ☑ `revert`/`revert-layer` por marcador, resolvidos sobre as regras casadas do elemento (sem lista por propriedade — custo zero sem `revert`, provado por teste); fixtures `claude-revert*` medidas no Blink e a PASSAR; suite 859/888 sem perdidos | `feat/dom-lote-j-declaration-record` → `7bd05644` (2026-09-04) | fixture `revert`/`revert-layer` medida no Chrome |
| K | invalidação escopada para `:nth-child` | 2 | ☑ subárvore do pai em vez de `touch()` global; testes de escopo; corpus 49/49 e suite 858/887 sem perdidos; TEMPO por medir (`dom_metrics`) | `feat/dom-lote-k-invalidacao-escopada` (2026-09-04) | `dom_metrics`: cascades por `appendChild` |
| L | cache de fragmentos em flex/grid/tabela | 2 | ☑ itens de flex-row/coluna/grid reusam (`FragmentKey` + tamanhos impostos); tabela e out-of-flow NÃO; corpus 49/49, suite 858/887 sem perdidos, paridade em 6 páginas: desvio máx. 6,1e-5 px (artefacto do reuso); `fragment_hits > 0` provado por teste; número do `dom_metrics` numa app real por medir | `feat/dom-lote-l-cache-flex-grid` → `3bccf4ea` (2026-09-04) | `dom_metrics`: subárvores reusadas numa app flex |
| M | ciclo de vida do nó | 2 | ☑ geração por NÓ, freelist, `releaseSubtree` decidido pela fachada; fixture `claude-dom-node-lifecycle` verde; wrappers fracos BLOQUEADOS por #2636 (`WeakRef` retém) | `feat/dom-lote-m-ciclo-de-vida` → `fe750c8b`+`b2e54326` (2026-09-04) | teste: arena não cresce ao remover/inserir N vezes |
| N | réguas no CI | 2 | ☑ `dom-rulers` e `dom-tests` VERDES no runner (run 33840034384, 2026-09-04); `dom-rulers` corre com `if: !cancelled()` porque a matriz `build` teve o macOS vermelho de 03/09 a 04/09 (#2632, corrigido em #2633); ficam por fazer a escrita automática do número no README e a régua de pintura | `main` (#2629, #2633) | resumo do job + check vermelho |
| O | selectores que faltam | 3 | ☑ `:target` (com `setLocationHash` no bridge e `fixar-hash` no corredor), `:scope`, `:default`, `:placeholder-shown`, `:active`/`:visited` (estado no `Dom`), `::marker` (cor), `:has()` (invalidação global quando presente, pinada). Não feitos: `::first-line`/`::first-letter`, `:autofill`, `:modal`, `:focus-visible` real | `feat/dom-lote-o-selectores` → `1baa405f`+`4e4b5540`+`3df13ad5` (2026-09-04) | `claude-sel-*` (3 de 4 passam; `sel-has` é do lote S-inline) |
| P | at-rules | 3 | ☑ `@media` completo (intervalos, `not`/`only`, listas OR, `orientation`, `resolution`, `prefers-*` com valor do host), `window.matchMedia` real (`mediaMatches` no bridge), `@import` resolvido na fachada, `@property` (registo; sem validação de `syntax`), at-rules ignorados contados | `feat/dom-lote-p-at-rules` → `b9c317b6`+`66fc9183`+`5b57c1e6` (2026-09-04) | `claude-media-completo`, `claude-property`, `claude-import` (3/3 passam) |
| Q | CSSOM | 3 | ☐ | — | §5 |
| R | grid e flex completos | 3–4 | ☑ colocação por linhas, colunas implícitas, `dense`, alinhamento, piso de min-content no `flex-shrink`, `*-reverse`, `align-content` multi-linha; e (vaga 4) `repeat(auto-fill\|auto-fit)`, `minmax` com lados intrínsecos, `fit-content()`. `auto-fit` não suprime o gap da track colapsada (dito) | `feat/dom-lote-r-grid-flex` → `bad665db`+`902aa6ea`; `feat/dom-lote-grid-intrinseco` → `7e13d037f` (2026-09-04) | `claude-grid-*`, `claude-flex-*` (12/12 passam) |
| S | propriedades sem efeito — grupo TEXTO | 3 | ☑ `word-spacing` (quebra, pintura e largura intrínseca), `tab-size`, `line-clamp`, `text-wrap` alias, `url()` serializado com aspas (e `background-image`/`mask-image` passam a responder), rect de bloco em linha = border box. `list-style-image` parcial (o marcador pinta a imagem, ninguém a carrega). Três remendos ao modelo de inline REVERTIDOS (partiam `display-basico`) → lote S-inline | `feat/dom-lote-s-texto` → `41031be5`, `c6243732`, `81845e2a`, `291ae43e` (2026-09-04) | `claude-text-overflow`, `word-spacing`, `tab-size`, `line-clamp` (4/4 passam), `list-style-image` |
| S-inline | caixa inline por FRAGMENTOS de linha | 4 | ◐ o corte mínimo: um inline SEM conteúdo nunca é promovido a caixa e é 0×0 na linha (`sel-has` passa; `display-basico` continua). A pintura de fundo/padding/borda por fragmento para inlines COM conteúdo continua a ser a promoção a caixa — declarado, não feito | `feat/dom-lote-s-inline` → `7869dc066` (2026-09-04) | `claude-sel-has`, `claude-display-basico` (2/2 passam) |
| S-transform | transformações 2D | 4 | ☑ `matrix()`, `skew*`, composição na ordem da spec, `transform-origin`, bounding box transformada (e dos descendentes), o fluxo não muda; pintura de rotação/skew no egui continua APROXIMADA (anchor exacto, w/h por norma das colunas) | `feat/dom-lote-transformacoes` → `508181548` (2026-09-04) | `claude-transform-*` (3/3 passam) |
| U | composição e pintura | 3 | ☐ | — | §5 |
| U-pintura-1 | rotação e recorte na pintura | 5 | ☑ a matriz viaja na `DisplayList` (`PushTransform`/`PopTransform`); o rasterizador e o egui pintam quadriláteros transformados; o `BeginClip` do `overflow` passa a conter os filhos (nunca continha); `visible` num eixo só computa `auto`; o rasterizador lê `fixar-hash`. Régua de pintura: as 4 fixtures acima de 2 % → 0 %, 0 %, 0,38 %, 0,05 %; corpus 84/86 ≤ 0,5 %, nenhuma > 2 %. Não feitos: cantos redondos sob matriz, blur da sombra, glifos rodados no egui | `feat/dom-lote-pintura-rotacao-clip` → `68011c503` (2026-09-04) | `scripts/css_pintura.md` |
| N-pintura | a régua de PINTURA (screenshot-diff contra o Blink) | 5 | ☑ `claude-raster` (rasterizador headless da DisplayList, PNG sem crate nova), `css_fixtures_screenshot_edge.mjs` (captura no Edge por CDP), `css_pintura_comparar.mjs` (diff por pixel, texto mascarado e reportado). Verificado: cor sólida 0%, gradiente 0,24%, box-model 0,01%. Não pinta texto nem imagens (dito). PNG não versionados | `feat/dom-regua-de-pintura` → `d415d9124` (2026-09-04) | `scripts/css_pintura.md` |
| réguas-6 | as réguas da vaga 6 ANTES do código | 6 | ☑ 4 fixtures medidas no Edge 152 (instrumento validado: 1 632 números dos 86 esperados, desvio 0): `claude-font-unidades-ch-ex` (T: `10ch`=87,97px, `10ex`=78,44px, `line-height: normal`=19px, fallback de família), `claude-inline-fragmentos` (S-inline-2: span que quebra = união 102,77×63 a y=-2, contentor 60 e não 22), `claude-hyphens-manual` (`&shy;` quebra: 40 vs 20), `claude-border-juncao` (pintura: junção diagonal das bordas, 0,13 %). Pintura lida: `triangulo-de-borda` 1,58 % É a junção por trapézio; `object-fit` 1,95 % é o rasterizador a não mascarar imagens (instrumento, não motor) | `docs/dom-reguas-vaga-6` (2026-09-04) | corpus 83/90, 7 esperadas |
| S-hifen | `hyphens: manual` — o hífen suave | 6 | ☑ `layout/hifen.rs`: o U+00AD é oportunidade de quebra (a linha fica com o maior prefixo que cabe com "-"), não pesa nem se pinta quando não quebra (também na largura natural de `medida.rs`), `none` apaga-o antes de medir; `auto` = `manual` (sem dicionário, dito). Um gancho só no `wrap_runs` (fecho do aglomerado que não cabe). Corpus 84/90 (`hyphens-manual` passa), pintura 0,09 %, suite 859/888 sem perdidos, 872 testes | `feat/dom-lote-s-hifen` → `39598b9ac` (2026-09-04) | `claude-hyphens-manual` |
| U-pintura-2 | junção diagonal das bordas; imagens mascaradas na régua | 6 | ☑ `DisplayItem::Quad` (quadrilátero convexo): com lados adjacentes de cores diferentes cada lado sai como o trapézio do canto exterior ao interior (`pintura::trapezios_dos_lados`); com cores iguais ficam as barras; `transparent` não emite. Rasterizador com `fill_quad` por varrimento; egui por mesh. `claude-raster` mascara `Image`/`Pixels` como mascara texto. Pintura: `triangulo-de-borda` 1,58 % → 0,04 %, `border-juncao` 0,13 % → 0,02 %; corpus de pintura 87/90 ≤ 0,5 %. ACHADO: `object-fit` fica em 1,95 % porque o motor não emite o FUNDO de um `<img>` sem imagem carregada (o rasterizador pintou 0 itens) — próximo alvo de pintura. Suite 859/888 sem perdidos; 875 testes | `feat/dom-lote-u-pintura-2` → `b4eebfac5` (2026-09-04) | `claude-border-juncao`, `claude-triangulo-de-borda` |
| S-inline-2 | inline com superfície por FRAGMENTOS de linha | 6 | ☑ `inline_por_fragmentos` (superfície sem width/height/margem) fica no fluxo: arestas como átomos (`ArestaInicio/Fim`), caixa = união dos fragmentos com padding/borda verticais, `inline_fragmentos::Superficies` pinta fundo e barras por linha (esquerda só no 1.º, direita só no último). A pergunta corrigida nos 5 sítios (`is_block_level`, `is_inline_block`, 3× `vertical.rs`). Cortes: cores cruas, sem radius nos fragmentos. Corpus 85/90, pintura 0,56 % → 0,06 %, suite 859/888 sem perdidos, 876 testes; paridade: 3 páginas 0 movidos, 3 movem os links da nav de 34 → 40,4px (= Blink: 14×1,6+18) | `feat/dom-lote-s-inline-2` → `add946b5e` (2026-09-04) | `claude-inline-fragmentos` |
| img-fundo | fundo do `<img>` sem pixels | 6 | ☑ `layout_image` emite o `background` na caixa reservada, com ou sem pixels (cor crua: sem `filter`/`opacity`, sem borda/padding — dito). Pintura `object-fit` 1,95 % → 1,22 %; o resto é a imagem `data:` que nenhum loader entrega ao `<img>` (o mesmo de `list-style-image`). Suite 859/888 sem perdidos; 876 testes | `feat/dom-lote-img-fundo` → `53b96700f` (2026-09-04) | `claude-object-fit` (pintura) |
| T | fontes: `ex`/`ch`, `line-height: normal`, `font-family` como lista | 6 | ☑ `Dimension::Ex`/`Ch` resolvem por `X_HEIGHT_RATIO`/`MONO_ADVANCE` (calibrados; `10ex` 78,6 vs 78,44 Blink, `10ch` 87,97 exacto; ABI `-1` como `calc`); `font-family` guarda a LISTA serializada como o Blink e `is_mono_family` percorre-a (a primeira família conhecida decide; desconhecida = indisponível). `line-height: normal` já batia (19px). NÃO feito, dito: fonte real (`@font-face`, `rustybuzz`, kerning, UAX#14) — a decisão de crate de §5.T fica aberta. Corpus 86/90 (só UA + cursor), pintura 2,45 % → 0,27 %, 878 testes, suite 859/888 sem perdidos | `feat/dom-lote-t-fontes` → `944161f7f` (2026-09-04) | `claude-font-unidades-ch-ex` |
| V-img | imagens `data:` no `<img>` | 7 | ☑ ponte `setImageDataUrl` (base64 + PNG 8 bits, 5 tipos de cor, sem entrelaçado — o resto responde 0) → `Dom::set_pixel_data` (o caminho do canvas) → `DisplayItem::Pixels`; `Dom::image_dims` (pergunta única nos 5 sítios); loader no passo 3 de `loadResources` e no corredor; `getComputedStyle().width/height` = valor USADO (CSSOM); rasterizador pinta `Pixels` e mascara `<img>` sem pixels. NÃO feito, dito: `http(s)`/ficheiro local no loader, JPEG/GIF/WebP, `object-fit` além de `fill`, `list-style-image`/`background-image` por URL. Corpus 87/91 (`img-natural` passa), medições 2 152/2 168, 879 testes, bridge 6, suite sem perdidos, paridade 0 movidos | `feat/dom-lote-v-img` → `930c78df5` (2026-09-04) | `claude-img-natural`, `claude-object-fit` (pintura) |
| V-img-2 | `<img>` de ficheiro local; o `<img>` é inline e senta na baseline | 7 | ☑ ponte `setImageFile` (PNG do disco → pixels no documento); `loadResources` resolve o `src` contra a base; o `<img>` sai da classificação de BLOCO em `vertical.rs`/`is_block_level` (partia `abc <img> def` em três linhas assim que a imagem chegava) e o átomo `Replaced` senta na baseline (y=15, não 4). NÃO feito, dito: `http(s)` (sem `fetchBytes` no motor novo), JPEG/GIF, `object-fit` além de `fill`. Corpus 88/92, medições 2 168/2 184, 879 testes, bridge 7, suite sem perdidos, paridade 0 movidos; a fixture pinta a 1 % só porque o rasterizador não lê ficheiros | `feat/dom-lote-v-img-2` → `17bec5631` (2026-09-04) | `claude-img-ficheiro` |
| flex-limites | `max-width`/`min-width` e margens `auto` no item flex; comprimentos computados em px | 7 | ☑ `FlexItem::max_main` (clamp da base e do grow — sem redistribuir o excedente, dito), `min-width` declarado substitui o piso de min-content, margens `auto` repartem o espaço livre antes do `justify-content`; o item com margem `auto` recebe o seu `main` como largura disponível (senão centrava duas vezes); `flex_limites.rs` extraído. `getComputedStyle` responde comprimentos relativos em px (`42em` → `672px`). **Régua de página real** (`scripts/parity/`, Edge): Bootstrap cover 36 → 39 elementos dentro de 1px | `feat/dom-lote-flex-max-width` → `9a62ffb57` (2026-09-04) | `claude-flex-item-max-width` |
| flex-item-bfc | um item de flex/grid contém os seus floats; o item de coluna que estica mede-se à largura do contentor | 7 | ☑ `establishes_block_formatting_context` pergunta pelo display do pai (Flexbox §4, Grid §6); o flex-column media a altura de um item que estica em shrink-to-fit (dois floats um debaixo do outro: 70 onde o Blink dá 40). Bootstrap cover 39 → **45** dentro de 1px; corpus 90/94, 881 testes, suite sem perdidos | `feat/dom-lote-flex-max-width` → `9d0044014` (2026-09-04) | `claude-flex-item-contem-floats` |
| load-resources | `loadResources` vivo no motor novo | 7 | ☑ `readTextFile` na ponte (`recursos.rs`): o `dom.ts` chamava `fs.read_text`/`fetch.fetchText`, globais do motor ANTIGO — nenhuma `<link rel=stylesheet>` local carregava fora dos testes e o `view.ts` do README morria em `rts:fs`. `http(s)` responde "" (dito: sem busca síncrona no motor novo). `view.ts` por `node:fs`/`process.argv`. Suite sem perdidos | `fix/dom-load-resources-motor-novo` → `d73e5987b` (2026-09-04) | — |
| intrinseco-whitespace | a largura intrínseca colapsa o whitespace do HTML | 7 | ☑ `intrinsic_content_width` e os nós de texto soltos medem o texto COLAPSADO (CSS Text §4.1; `pre`/`pre-wrap` preservam) — o botão fixo do Bootstrap cover media 103px a mais por catorze espaços de indentação. Bootstrap cover 45 → **45** de 57 a 1px; corpus 91/95, 882 testes, suite sem perdidos | `feat/dom-lote-intrinseco-whitespace` → `3996aa6ce` (2026-09-04) | `claude-intrinseco-whitespace` |
| svg-atributo | `width`/`height` do `<svg>` como comprimentos CSS; margens no placeholder | 7 | ☑ `1em`/`50%`/`24` resolvem (SVG 2 §7, presentation attributes) — o ícone `1em` do botão de tema caía no viewBox; o placeholder respeita `margin` como o `<img>`. Bootstrap cover 45 → **45** de 57 a 1px; corpus 92/96, 883 testes, suite sem perdidos | `feat/dom-lote-svg-atributo-em` → `828db35a8` (2026-09-04) | `claude-svg-atributo-em` |
| borda-por-lado-intrinseca | a borda por lado na largura intrínseca e na base flex | 7 | ☑ `child_outer_width`/`content_natural_width`/`flex_base_outer`/`limites_do_item` somam `used_widths` (esq+dir) em vez da borda escalar — o caret `::after` do Bootstrap (só bordas) media 0. Bootstrap cover 45 → **45** de 57; corpus 93/97, 884 testes, suite sem perdidos | `feat/dom-lote-borda-por-lado-intrinseca` → `9633f3500` (2026-09-04) | `claude-borda-por-lado-intrinseca` |
| inline-block-bfc | um inline-block flui os filhos como BLOCO; a corrida de inline-blocks alinha pela baseline própria | 7 | ☑ `to_display_code`: `Inline`/`InlineBlock` → eixo 0 (era "wrap": os filhos de qualquer inline-block iam pelo colocador horizontal do flex, alinhados pelo topo); `linha_ib` com `envelope_com_baseline`/`topo_do_item_com_baseline` (baseline própria: texto = borda+padding+meia+ascent, vazio = fundo, controlo = texto), strut pela caixa de linha (`line-height`), inline-block vazio na linha senta na baseline. O caret `::after` do Bootstrap a y=9. Corpus 93/97 (só UA + cursor), suite sem perdidos, paridade 0 movidos, 884 testes | `feat/dom-lote-borda-por-lado-intrinseca` → `4fd967d09` (2026-09-04) | `claude-borda-por-lado-intrinseca` |
| N-wpt | a régua dos REFTESTS do WPT | 7 | ☑ `scripts/wpt_reftests.mjs`: teste e referência rasterizados pelo `claude-raster` e comparados pixel a pixel (auto-consistência, sem browser). `css/css-flexbox`, primeiros 300 de 489: **114 passam (38 %)**; piores: baseline multi-linha, tamanhos definidos, `writing-mode` vertical, testes com `<script>` (sem JS). A família `testharness.js` (528 testes nas mesmas pastas) fica por montar | `feat/dom-lote-borda-por-lado-intrinseca` (2026-09-04) | `scripts/wpt_reftests.md` |
| ib-nowrap | a corrida de inline-blocks respeita `white-space: nowrap` | 7 | ☑ `linha_ib.rs` só quebra com `normal`/`pre-wrap`/`pre-line` — a referência de 27 reftests de flexbox do WPT (quatro inline-blocks de 6em num `nowrap` de 12em) quebrava em duas linhas aqui | `feat/dom-lote-ib-nowrap` → `ebdf25149` (2026-09-04) | `claude-inline-block-nowrap` |
| justify-fisico | `justify-content: left`/`right` são físicos | 7 | ☑ variantes `Left`/`Right` (Box Alignment §5.1): `left` encosta à esquerda mesmo em `row-reverse`, numa coluna valem `start`; resolvidos antes do espelho (`coluna::fisico_para_eixo`); serializados como `left`/`right`. 16 reftests "justify" do WPT | `feat/dom-lote-ib-nowrap` → `7e50df0f0` (2026-09-04) | `claude-justify-left-right` |
| clearfix | `::after{display:block;clear:both}` contém os floats | 7 | ☑ `clearfix.rs`: o fim do fluxo do contentor desce até ao fundo dos floats que o pseudo de bloco com `clear` nomeia (§9.5.2) — a referência de 20 reftests "wrap" do WPT. CORTE dito: `content`/`height`/fundo do pseudo de bloco não são desenhados. Corpus 96/100, 887 testes, suite sem perdidos; WPT css-flexbox 186 → **193/489** | `feat/dom-lote-ib-nowrap` → `7e50df0f0` (2026-09-04) | `claude-clear-em-pseudo` |
| V–Y | a superfície DOM que as bibliotecas pedem | 4 | ☐ | — | §6 |

**Estado após a vaga 1 (2026-09-04, 01:41)** — medido com o `rts.exe`
construído sobre `feat/dom-vaga-1` e comparado POR FICHEIRO contra o binário
de 2026-09-03 (`target/baseline.exe`, lista em `base-suite.txt`):

| régua | antes (baseline) | depois | perdidos |
|---|---|---|---|
| corpus CSS (`claude-css-runner`) | 41/49, 23 desvios | **49/49, 0 desvios** | nenhum |
| suite `*.test.ts` por `medir.sh` | 855/884 | **858/887** (+3 fixtures novas) | **nenhum** |
| `cargo test -p rts-dom --lib` | 718 | **756**, 0 falhas | — |
| `cargo test -p rts-dom-bridge` | nunca corria (doctest partido) | **4 + doctests**, 0 falhas | — |

O binário desta medição fica como o próximo `target/baseline.exe` e
`vaga1-suite.txt` como o próximo `base-suite.txt`. O que a vaga 1 NÃO mediu:
o scroll numa janela real (G) — a fixture headless passa, o exemplo em janela
fica para quem tiver ecrã.

**Estado após a vaga 2 (2026-09-04, ~10:00)** — lotes I, J, L, M (K e N já
estavam), medido com o `rts.exe` de `feat/dom-vaga-2` contra o binário do
lote K (`target/baseline.exe`, `base-suite.txt` = `lotek-suite.txt`):

| régua | antes (baseline) | depois | perdidos |
|---|---|---|---|
| corpus CSS | 49/49 | **51/54** — +5 fixtures: `revert`×2 passam, `ua`×3 falham de propósito (15 desvios = lacunas do motor) | nenhum |
| suite `*.test.ts` por `medir.sh` | 858/887 | **859/888** (+`claude-dom-node-lifecycle`) | **nenhum** |
| `cargo test -p rts-dom --lib --features metrics` | 758 (feature partida) | **788**, 0 falhas; a feature `metrics` voltou a compilar | — |
| paridade em 6 páginas locais (só o lote L) | — | desvio máx. 6,1e-5 px | — |

Régua nova: `scripts/css_fixtures_medir_edge.mjs` (Blink headless por CDP;
validado com desvio 0 contra os 49 esperados do Chrome). Encontrado de
caminho: `WeakRef`/`WeakMap` retêm fortemente (#2636), o que bloqueia os
wrappers fracos do lote M. O binário desta medição é o próximo
`target/baseline.exe`; `vaga2-suite.txt` o próximo `base-suite.txt`.

**Estado após a vaga 3 (2026-09-04, ~13:30)** — lotes O, P, R, S-texto + o
lote de réguas da vaga 4, medido com o `rts.exe` de `feat/dom-vaga-3` contra
o binário da vaga 2 (`target/baseline.exe`, `base-suite.txt` = `vaga2-suite.txt`):

| régua | antes (baseline) | depois | perdidos |
|---|---|---|---|
| corpus CSS | 51/54 | **72/86** — +32 fixtures: 21 passam, 11 esperadas a falhar (`tests/css/esperado-a-falhar.txt`: 3 UA, `grid-auto-fill`, `sel-has`, 9 réguas da vaga 4 medidas ANTES do código) | nenhum |
| suite `*.test.ts` por `medir.sh` | 859/888 | **859/888** | **nenhum** |
| `cargo test -p rts-dom --lib --features metrics` | 788 | **844**, 0 falhas | — |
| check local do `dom-rulers` | — | inesperadas: nenhuma; da lista a passar: nenhuma | — |

Retrabalho: 16 rondas em 4 lotes (4,0 por lote; o lote S sozinho 8) — 10
delas por fixtures medidas DEPOIS do código. É a razão da regra nova do §1
("a régua antes do código"), já aplicada às 12 réguas da vaga 4. O binário
desta medição é o próximo `target/baseline.exe`; `vaga3-suite.txt` o próximo
`base-suite.txt`. A paridade das 6 páginas locais foi REGENERADA como "antes"
(a margem de UA e o rect em linha moveram-na, e era isso que devia).

**Estado após a vaga 4 (2026-09-04, ~14:30)** — lotes S-inline, S-transform,
S-decor e o resto de R, todos com a RÉGUA MEDIDA ANTES do código; medido com
o `rts.exe` de `feat/dom-vaga-4` contra o binário da vaga 3:

| régua | antes (baseline) | depois | perdidos |
|---|---|---|---|
| corpus CSS | 72/86 | **82/86** — as 4 que falham são as 3 de UA e `cursor` com `url()` (lista `esperado-a-falhar.txt` encurtada de 14 para 4) | nenhum |
| suite `*.test.ts` por `medir.sh` | 859/888 | **859/888** | **nenhum** |
| `cargo test -p rts-dom --lib --features metrics` | 844 | **868**, 0 falhas (e 861 SEM a feature: o teste dos at-rules ignorados passou a ser condicionado — o `dom-tests` do CI corre sem ela) | — |
| check local do `dom-rulers` | — | inesperadas: nenhuma | — |

**Retrabalho: 0 rondas em 4 lotes** (a vaga 3 tinha custado 16 em 4). A
diferença foi uma só: as fixtures medidas e commitadas antes, e o agente a
correr o teste do corpus no worktree dele. O binário desta medição é o
próximo `target/baseline.exe`; `vaga4-suite.txt` o próximo `base-suite.txt`.

**Estado após a vaga 5 (2026-09-04, ~13:00)** — a régua de PINTURA e o
primeiro lote medido por ela (U-pintura-1), contra o binário da vaga 4:

| régua | antes (baseline) | depois | perdidos |
|---|---|---|---|
| corpus CSS (layout + computed) | 82/86 | **82/86** (as 4 esperadas) | nenhum |
| **pintura** (novo: pixels vs Edge, texto mascarado) | 79/86 ≤ 0,5 %, 4 > 2 % | **84/86 ≤ 0,5 %, 86/86 ≤ 2 %, 0 > 2 %** | nenhuma piorou |
| suite `*.test.ts` por `medir.sh` | 859/888 | **859/888** | **nenhum** |
| `cargo test -p rts-dom --lib --features metrics` | 868 | **870**, 0 falhas; `rts-egui` compila com `PushTransform` | — |

Retrabalho: 0 rondas em 2 lotes. O binário desta medição é o próximo
`target/baseline.exe`; `vaga5-suite.txt` o próximo `base-suite.txt`. As duas
fixtures entre 0,5 % e 2 % na pintura (`object-fit` 1,95 %, `triangulo-de-borda`
1,58 %) são o próximo alvo de pintura; os PNG continuam sem versão.

**Estado após as réguas da vaga 6 (2026-09-04, ~14:00)** — nenhum código do
motor mudou; o que mudou foi o denominador, de propósito:

| régua | antes | depois |
|---|---|---|
| corpus CSS (layout + computed) | 82/86, 4 esperadas | **83/90**, 7 esperadas (3 novas, "por implementar") |
| pintura (novas) | — | `border-juncao` 0,13 %, `hyphens-manual` 0,3 %, `inline-fragmentos` 0,56 %, `font-unidades` 2,45 % |

Os lotes de código que estas réguas pedem, por ordem de custo: **S-hifen**
(`&shy;` como oportunidade de quebra + hífen pintado; `hyphens: none` a
ignorá-lo), **U-pintura-2** (bordas como trapézios nas junções — fecha o
triângulo; e o `claude-raster` a mascarar `DisplayItem::Image` como mascara
texto, para o `object-fit` medir o que o motor faz e não o que o exemplo não
tem), **S-inline-2** (inline com conteúdo em fragmentos de linha: hoje o span
promovido a caixa nem sequer QUEBRA — `cx1` fica 22px alto em vez de 60 — e
engrossa a linha com a borda), **T** (`ch`/`ex`/`line-height: normal` pela
métrica da fonte do `TextMeasurer`; a lista de `font-family` serializada
inteira). Cada brief leva os números acima.

**Estado após a vaga 6 (2026-09-04, ~17:30)** — réguas antes do código, 5
lotes feitos à mão (os 15 agentes da sessão já tinham sido usados), ZERO
rondas de retrabalho:

| régua | antes (vaga 5) | depois |
|---|---|---|
| corpus CSS (layout + computed) | 82/86, 4 esperadas | **86/90**, 4 esperadas (3 UA, `cursor`) |
| pintura (pixels vs Edge) | 84/86 ≤ 0,5 % | **89/90 ≤ 0,5 %** (`object-fit` 1,22 % = a imagem `data:` sem loader) |
| suite `*.test.ts` por `medir.sh` | 859/888 | **859/888**, nenhum perdido em nenhum lote |
| `cargo test -p rts-dom --lib --features metrics` | 870 | **878** |

Lotes: réguas-6 (#2643), S-hifen (#2644), U-pintura-2 (#2645), img-fundo
(#2646), S-inline-2 (#2647), T. As três fixtures medidas antes do código
passaram todas. O que fica: o loader de imagem `data:` para
`<img>`/`list-style-image` (fecha `object-fit`), a fonte REAL de §5.T
(decisão de crate), e as 3 da folha de UA que só uma fonte real fecha.

**O lote V-img, desenhado às 18:30 e FEITO às 21:00 (a decisão mudou no
caminho: os pixels ficam no DOCUMENTO por `set_pixel_data`, não num handle de
Buffer — ver a linha V-img em cima; o que segue é o desenho original):** o `claude-object-fit` (1,22 % na pintura) e o `list-style-image`
param no mesmo sítio, e a investigação disse porquê: **`rts:imgdec` e
`dom.setImage` NÃO existem no motor novo** — `examples/claude-browser.ts` e
`claude-wa-app.ts` chamam-nos, mas são código do motor antigo (nenhum crate
regista o namespace; `setImage` não está em `dom_members()`). Só
`Dom::set_image`/`image_of` (rts-dom) e o `DisplayItem::Image` existem. O lote
tem três peças e uma decisão, tomada aqui para não ser retomada duas vezes:

1. **Descodificador**: crate `png` (e `jpeg-decoder`) em `rts-std`, sob um
   namespace `rts:imgdec` com `decode(ptr,len) -> handle`, `width`, `height`
   (o `#[rtse::class]` — RULE 0b `add-builtin-class`). O `rts-dom` continua
   sem dependências; o `claude-raster` continua a mascarar imagens — a régua
   de pintura das imagens passa a ser a do egui (screenshot da janela), não a
   do exemplo headless.
2. **Bridge**: `setImage(doc, node, handle, off, w, h)` em `dom_members()`
   (o teste de contrato obriga), a chamar `Dom::set_image`; e `imageOf` para
   o `naturalWidth`/`naturalHeight` de `HTMLImageElement`.
3. **Loader no documento** (`dom.ts`, não nos exemplos): ao montar/mutar um
   `<img src>` — `data:` descodificado por `atob` na hora; `http(s)` pelo
   `fetchBytes` assíncrono já existente; e o `list-style-image`/`background-image`
   do CSS pelo mesmo caminho, com a URL resolvida contra a base do documento
   (o que também fecha `claude-cursor-pointer-events`, que é só resolução de
   URL).

Régua ANTES do código: `claude-object-fit` (já medida: as quatro caixas 100×50
com a imagem `data:` de 1×1 por baixo), `claude-list-style-image` (já medida)
e uma fixture nova `claude-img-natural` com um PNG `data:` de 4×2 sem
`width`/`height` (a caixa tem de vir do tamanho natural: Blink 4×2) — a medir
no Edge antes de abrir o lote.

**Se retomou depois de uma paragem:** (1) `git branch -a | grep feat/dom-`
diz que lotes têm branch; (2) `git log main..origin/<branch>` diz se o commit
do lote existe; (3) a tabela diz se foi integrado. Um lote com branch e sem
"☑" é para verificar com o §2 e integrar, não para refazer.

---

## 1. As regras do trabalho (o que não se negocia)

- **RULE 0** do `CLAUDE.md`: antes de editar, ler `docs/ui/html-engine/README.md`
  e a secção 3 do roadmap (os invariantes — dois deles já expiraram e a
  auditoria diz quais), e o relatório da lente que cobre o lote.
- **Tecto de 500 linhas.** Estes já estão acima e **não crescem**: `dom.ts`
  (1 847), `style/syntax.rs` (1 122), `layout/bloco.rs` (1 061),
  `style/scenarios.rs` (829), `style/parse/mod.rs` (724),
  `style/stylesheet/sheet.rs` (580), `layout/vertical.rs` (556),
  `dom/mutacao.rs` (522), `layout/fragmento.rs` (509). Lógica nova = módulo
  novo pequeno; nos grandes entram chamadas.
- **Uma máquina.** Os agentes implementam e commitam na sua branch, num
  worktree isolado, e **não constroem** (um `cargo check -p rts-dom` no fim é o
  máximo — o crate não tem dependências). Quem orquestra constrói UMA vez com
  todos os lotes da vaga, corre as réguas do §2, e devolve o erro exacto ao
  agente do lote. Ver a memória `feedback_uma_maquina_build_centralizado`.
- **A régua é o Chrome.** Nunca se edita uma fixture nem um `.esperado.json`
  para um número subir; uma fixture que falha fica a falhar. Um lote de layout
  entra com um teste Rust que parseia o HTML EXACTO da fixture e afirma os
  rects do Chrome — assim a suite unitária pina o mesmo que o corpus.
- **A régua ANTES do código** (2026-09-04, depois de a vaga 3 custar 14 rondas
  de retrabalho, 10 delas por fixtures medidas depois de implementar): um lote
  começa por um "lote de réguas" — as fixtures são escritas, revistas (sem
  texto onde a pergunta é geometria; ids só no que se afirma; o selector ou a
  propriedade como única coisa em jogo), MEDIDAS no Blink
  (`scripts/css_fixtures_medir_edge.mjs`) e commitadas com `.esperado.json`
  antes de qualquer agente implementar. O brief do agente traz os números; o
  teste Rust em `layout/tests/*_corpus.rs` nasce com os rects do Blink e o
  agente corre-o sozinho (`cargo test -p rts-dom --lib <nome>` é permitido: o
  crate não tem dependências). O vermelho→verde acontece no worktree do
  agente, não na ronda de build do orquestrador.
- **Comparação por ficheiro.** Uma vaga só integra com a lista de PERDIDOS
  vazia nas três réguas (corpus, testes do crate, suite). Um número líquido
  não é um resultado.
- **Sem código morto; comentários dizem porquê; testes nomeiam o comportamento;
  fixtures novas têm prefixo `claude-`.** Convenções do `CLAUDE.md`.
- **Um lote, uma branch, um PR, `--squash`.** Lotes da mesma vaga são
  disjuntos em ficheiros por desenho; onde não puderem ser (o `bloco.rs`), o
  plano diz quem evita que região.

---

## 2. As réguas — os comandos exactos

```bash
# 1. testes do crate (sem dependências: segundos)
cargo test -p rts-dom --lib --no-fail-fast
cargo test -p rts-dom-bridge --no-fail-fast          # lote A em diante

# 2. o corpus CSS (precisa do binário; release, porque o runner é o rts:dom real)
cargo build --release                                 # o pacote RAIZ `rts` é o rts.exe; `-p rts-cli` só constrói a lib
target/release/rts.exe run examples/claude-css-runner.ts   # "N/49 passam" + desvios
CSS_FILTRO=position target/release/rts.exe run examples/claude-css-runner.ts

# 3. a suite, por ficheiro, contra o binário guardado
cp target/release/rts.exe target/baseline.exe         # SÓ no início de uma vaga
bash medir.sh target/baseline.exe base-suite.txt
bash medir.sh target/release/rts.exe now-suite.txt
comm -23 <(awk '$2==1{print $1}' base-suite.txt | sort) <(awk '$2==1{print $1}' now-suite.txt | sort)
#   ↑ a lista de PERDIDOS: tem de vir vazia

# 4. o dump de paridade (só para refactors que devem mover ZERO pixels)
#    docs/ui/modularizacao-rts-dom.md, secção "A régua que valida um refactor"
```

O corpus lê-se por fixture: `passa → passa`, `falha → passa` (ganho),
`passa → falha` (PERDIDO, bloqueia), `falha → falha` (compare os desvios: menos
é progresso, mais é regressão dentro de uma fixture que já falhava).

---

## 3. Vaga 1 — o que a auditoria disse que vinha primeiro (em curso)

Seis lotes disjuntos, lançados a 2026-09-04. Cada um tem o brief completo na
sessão que o lançou; o que fica aqui é o suficiente para verificar e para
retomar.

### A — contrato DOM→JS (`rts-dom-bridge`, `dom.ts`, 3 docs)

- **Fecha:** finding 1 da auditoria. `Element.getBoundingClientRect()` chama
  `dom.boundingComponent`, o bridge regista `boundingRect` → `TypeError` em
  qualquer página. `Element.setStyle(slot,val)` chama `dom.setStyle`, que não
  existe.
- **Entrega:** a fachada a chamar o que existe; `setStyle` apagado (sem
  chamadores) ou implementado sobre `dom/estilo.rs` (com); um `#[test]` no
  bridge que extrai todo `dom.<ident>(`/`engine.<ident>(` de `dom.ts`+`window.ts`
  e afirma que cada um é uma chave de `MEMBERS` — a vista gerada mínima, que
  teria apanhado isto; três funções mortas apagadas (`layout_inline_line`,
  `wrap_text`, `fragment_count`); as três docs que mentiam corrigidas
  (invariante 5, `tests/css/README.md`, `dom-metrics.md`).
- **Aceitação:** `cargo test -p rts-dom-bridge` verde e o teste de contrato
  a falhar se se reintroduzir `boundingComponent`; a fixture nova passa.
- **Depois (não neste lote):** a vista gerada a sério — `#[rtse::class]` fora
  do `rts-core` (branch `feat/class-abi-macro-fora-do-core` já compila a
  macro lá; falta a opção `extend` para namespaces em vários ficheiros) e um
  `.d.ts` emitido por `rts emit-types` que cubra `rts:dom`, `rts:egui`,
  `rts:input`.

### B — um medidor activo (`dom/geometria.rs`, módulo novo, `rts-egui`)

- **Fecha:** "duas verdades para a mesma geometria". `bounding_component` mede
  sempre com `ApproxMeasurer`; a janela pinta com `EguiMeasurer`.
- **Entrega:** `layout/medidor_ativo.rs` — thread-local com
  `Option<Rc<dyn TextMeasurer>>`, `registar`/`limpar`/`with_active`; a geometria
  usa o activo se existir; `rts-egui` regista o seu ao pintar; o comentário de
  `geometria.rs:16-18` passa a ser verdade.
- **Aceitação:** três testes (sem registo = aproximado; com medidor falso =
  reflecte-o; limpar = volta); a chave de cache continua a incluir
  `measurer.identity()`.

### C — `position` (`posicionado.rs`, `layout/relativo.rs` novo, `bloco.rs` mínimo)

- **Fecha:** 6 desvios. `relative` desloca a pintura (reusa o shift do
  `transform`, `bloco.rs:1005-1051`); `absolute` estica quando os dois offsets
  de um eixo estão definidos e a dimensão é `auto`; a margem de um filho
  colapsa através de um pai `relative` sem borda (`#meio.y 50`, hoje 70).
- **Aceitação:** `layout/tests/position_corpus.rs` com os rects do Chrome das
  duas fixtures; `claude-position-*` passam no corpus; `tests/colapso.rs`
  continua verde.
- **Depois:** `sticky` (está no enum, é fluxo normal); static position de um
  `absolute` sem offsets num pai inline; `z-index` além do v1.

### D — grid: rows por áreas (`grid.rs`, módulo novo se passar de 500)

- **Fecha:** 3 desvios. `grid-template-rows: 60px 1fr 40px` com altura 400
  não dá 300 à row do meio, ou não a atribui aos itens por área; o rodapé é
  colocado como se ela medisse 0.
- **Aceitação:** `layout/tests/grid_corpus.rs`; `claude-grid-areas` passa;
  os testes de áreas/spans existentes continuam verdes.
- **Depois (lote R do §5):** `grid-row/column-start/end` completos,
  `grid-auto-flow`/`dense`, tracks implícitas, `auto-fill`/`auto-fit`,
  sizing intrínseco, `align-content`/`justify-self`.

### E — BFC como entidade, floats, `clear` (`layout/bfc.rs` novo)

- **Fecha:** findings 1 e 2 da lente de layout, 6 desvios. A entidade
  `BlockFormattingContext` (exclusões por lado, "estabelece BFC?", clearance
  pendente) evolui o parâmetro `exclusoes` de `layout_block` em vez de
  acrescentar parâmetros; `Clear::clears` por lado; o pai só cresce para
  conter floats quando estabelece BFC (`overflow`≠`visible`, `flow-root`,
  flex/grid/tabela, float, absolute/fixed, inline-block, raiz).
- **Aceitação:** `layout/tests/float_corpus.rs` (as duas fixtures + um caso
  `flow-root` marcado como derivado da spec); o comentário "DIVERGÊNCIA
  CONHECIDA" de `vertical.rs:544` desaparece; nenhuma fixture de flex/grid/
  tabela/texto regride.
- **Risco declarado:** mexer na altura de todo o contentor com float move
  páginas reais; a suite por ficheiro e o dump de paridade são o que o apanha.

### F — baseline por átomo, `vertical-align`, `white-space` (`layout/alinhamento_vertical.rs` novo)

- **Fecha:** 8 desvios. Cada átomo/segmento da linha carrega ascent/descent;
  o line box tem strut; os oito valores de `vertical-align` são deslocamentos
  contra a baseline dominante; `pre` abre linha no `\n`; a `line-height` do
  shorthand `font` chega ao line box de um pai misto (bloco + inline).
- **Constantes:** ascent/descent/x-height do `ApproxMeasurer` derivadas do
  esperado do Chrome e documentadas como calibração (o método de
  `style/text_metrics.rs`); o `EguiMeasurer` sobrescreve-as com a métrica real
  do epaint.
- **Aceitação:** `layout/tests/inline_corpus.rs`; as três fixtures passam;
  nenhuma fixture de texto que hoje passa regride.

---

## 4. Vaga 2 — o resto do que a auditoria marcou como estrutural ou dívida

Começa quando a vaga 1 está integrada e medida. G e H podem correr em
paralelo com a vaga 1 se houver agentes livres: não tocam em layout.

### G — o scroll vive no documento (`Dom`, bridge, `dom.ts`/`window.ts`, `rts-egui`)

- **Fecha:** finding 3. O offset de scroll (página e cada `overflow:auto`)
  existe só em `egui::Context::memory()` (`frame/render/mod.rs:213`,
  `scroll.rs:27-30`); `scrollTop` não existe, `scrollTo`/`scrollBy` são vazios.
- **Desenho:** em `Dom`, `scroll: Cell<(f32,f32)>` para a página e
  `scroll_regioes: RefCell<HashMap<NodeIdx,(f32,f32)>>` — o padrão de
  `hovered`/`focused_input`. O bridge ganha `scrollTop`/`scrollLeft`/
  `scrollHeight`/`scrollWidth`/`scrollTo` (por nó e para `window`). O backend
  LÊ para desenhar e traduzir hit-test, e ESCREVE só em resposta a input
  (roda, arrastar barra, teclado) — nunca guarda para si. `scroll.rs:96-103`
  deixa de mutar a `DisplayList` a posteriori: o layout emite o `BeginClip`
  já deslocado porque lê o offset do `Dom`.
- **Aceitação:** teste Rust (mutar o offset muda o rect devolvido por
  `bounding_component` de um filho de `overflow:auto`); fixture
  `claude-dom-scroll.test.ts`; `examples/claude-tarefas.ts` com uma lista mais
  alta que a janela a rolar por `scrollTo` E pela roda.

### H — o escopo de página não vê o Node (`rts-core`, `rts-codegen`, `rts-host`)

- **Decisão TOMADA (2026-09-04, pelo orquestrador):** este motor NÃO é hoje
  uma fronteira de segurança — mesmo processo, mesmo heap, mesma `Context`, e
  `require`/`fetch` de recursos que a própria página nomeia não passam por
  nenhuma política. O código passa a DIZÊ-lo: o comentário de `NODE_ONLY`
  (`rts-codegen/src/emit/globals.rs`) já não promete o que não cumpria. As
  duas fugas fecham como CORRECÇÃO — é o bug que a lista existe para impedir
  desde sempre (uma página que vê `setImmediate` monta o React pelo ramo Node)
  — e não como implementação de uma fronteira de segurança que este motor não
  tem.
- **Feito, branch `feat/dom-escopo-pagina-sem-node`:**
  (1) o nome nunca entra na cadeia que `Scope::lookup` resolve a zero hops:
  `globals::without_node_only` filtra `NODE_ONLY` de `enclosing` ANTES de
  `emit_page_program`/`emit_eval_program` construírem o `chain`/`Scope` —
  opção (a) do enunciado, porque filtrar o resultado fecha a fuga
  independentemente do mecanismo exacto que a produzia, e o mecanismo em si
  não ficou provado por leitura estática sozinha (a auditoria mediu-o AO
  VIVO);
  (2) `Scoped::Eval`/`Scoped::Page` ganharam `hide_node_globals: bool`;
  `rts-host/src/live.rs` decide-o comparando o ambiente a
  `rts_core::entry::global_object` (só `vm.runInThisContext` partilha o
  global real) e, quando `true`, marca o ambiente com
  `rts_core::entry::mark_hides_node_globals` — uma propriedade `__rts_`
  no PRÓPRIO objecto, e não uma tabela lateral por CÉLULA, porque uma célula
  libertada e reutilizada tornaria uma entrada de tabela obsoleta uma marca
  no objecto ERRADO. Um `eval()` de dentro da página lê a marca de volta com
  `rts_core::entry::hides_node_globals`, que anda a MESMA cadeia
  `__rts_outer` que `environment_names` já anda.
- **Aceitação:** fixture `tests/claude-dom-page-nao-ve-process.test.ts`: num
  `<script>` de página, `typeof process`, `typeof Buffer`,
  `typeof setImmediate`, `typeof require` e `eval("typeof process")`
  respondem `"undefined"`; `typeof setTimeout`/`typeof fetch`/
  `typeof document` continuam o que um browser dá; e um controlo prova que
  FORA da página `typeof process` continua `"object"` — a diferença é o
  ESCOPO, não uma remoção do global.
- **Risco aceite e não fechado por este lote:** `vm.runInContext`/
  `runInNewContext` com um sandbox comum (não `runInThisContext`) passam a
  também esconder `NODE_ONLY` do sandbox — inclusive se o chamador tivesse
  posto lá `sandbox.process = algo` explicitamente, esse `process` deixa de
  resolver pela CADEIA (continua a resolver por leitura directa de
  propriedade, `sandbox.process`). Nenhum teste existente pina o caso
  contrário; `crates/rts-node/README.md`/`vm.rs` é onde revisitar se um
  programa real depender disso.
- **Gap conhecido, não fechado:** `eval` INDIRECTO — `(0, eval)("process")`,
  `globalThis.eval("process")`, um nome que aponta para `eval` chamado sem ser
  como identificador nu — continua a ver `process` de dentro de uma página. O
  mecanismo (`rts_core::entry::eval_source`) passa sempre `environment =
  undefined` para o compilador, que é como a especificação diz "corre no
  escopo global" — mas este motor não tem um global por página, então
  `hides_node_globals(undefined)` não tem nada para andar e responde `false`.
  Fechar isto exigiria saber de que ESCOPO A CHAMADA partiu sem um objecto de
  ambiente para perguntar, o que é um problema diferente do que este lote
  respondeu. A fixture só cobre a forma DIRECTA (`eval("...")`), que é a que a
  auditoria mediu e a que o enunciado pediu.
- **Não feito:** um `Context`/heap por documento em vez do singleton
  thread-local — a decisão tomada foi "não é fronteira de segurança", o que
  torna este item fora de escopo em vez de pendente.

### I — a folha de UA em CSS real (`style/`, `block/ua.rs` morre, `scrollbar.rs` morre)

- **Fecha:** dois findings da lente de estilo. `UA_TABLE` (13 slots) +
  `ua_display(tag)` (um `match` chamado pelo layout DEPOIS da cascade) não
  exprimem selectores nem propriedades fora do conjunto; `<th>` não é negrito.
  `scrollbar.rs` é um segundo parser de CSS textual com um bug de aninhamento
  em `@media`.
- **Desenho:** `style/ua.css` (texto, `include_str!`), parseado pelo MESMO
  `style::parse`, anexado ao `Stylesheet` com origem UA (abaixo de qualquer
  layer de autor, e abaixo do `!important` de autor pela ordem da cascade —
  o `!important` de UA fica acima, como a spec diz); `used_display` deixa de
  consultar `ua_display`; `::-webkit-scrollbar*` casa pelo matcher normal e
  `scrollbar.rs` só LÊ o computed.
- **Aceitação:** fixtures medidas no Chrome para `th`, `ul`/`ol` (a de
  `list-style-type` já existe), `input:disabled`, `h1`…`h6`, `table`;
  o dump de paridade (§2.4) — aqui vai mover pixels, e cada um tem de ser
  explicado por uma regra da folha nova.

### J — `DeclarationRecord` na cascade, e `revert`/`revert-layer` (`style/stylesheet/sheet.rs`)

- **Fecha:** a cascade colapsa cascaded+specified em `declarations_from`
  (`sheet.rs:519-577`); a proveniência morre aí.
- **Desenho:** por elemento, por propriedade, a lista ordenada de candidatas
  `{origem, layer, importância, especificidade, ordem, valor declarado}`;
  a redução para `ComputedStyle` é o último passo; `revert` recua uma origem,
  `revert-layer` uma layer. Não é uma segunda lista de regras: é o que
  `declarations_from` devolve antes de reduzir. Cuidado com o custo — a
  memoização por revisão (`computed_memo`) tem de continuar a valer; medir com
  `dom_metrics` antes e depois.
- **Aceitação:** fixtures `claude-revert.html`/`claude-revert-layer.html`
  medidas no Chrome; `getComputedStyle` inalterado nas 49 existentes.

### K — invalidação escopada para `:nth-child` (`dom/invalidacao.rs`, `sheet.rs`)

- **Fecha:** `position_sensitive()` é UM booleano por folha; um
  `tr:nth-child(odd)` em qualquer lado faz cada mutação estrutural cair no
  `touch()` global.
- **Desenho:** invalidar os irmãos (e o pai) do nó mutado quando há regras
  sensíveis a posição — o conjunto de invalidação por selector do Blink, na
  forma mínima.
- **Aceitação:** `dom_metrics`: cascades por `appendChild` numa página de
  3 000 nós com uma regra `:nth-child` deixam de ser O(n).

### L — a cache de fragmentos em flex/grid/tabela (`flex.rs`, `coluna.rs`, `grid.rs`, `table/`)

- **Fecha:** `layout_block_reusing` só é chamado do fluxo de bloco
  (`vertical.rs:464`); flex/grid/tabela/out-of-flow chamam `layout_block`.
- **Desenho:** a `FragmentKey` não depende do display; os três caminhos
  passam a chamar `layout_block_reusing`. O que pode partir: o "forced size"
  que flex/grid impõem tem de entrar na chave.
- **Aceitação:** `dom_metrics` numa app de cards em flex (o
  `claude-tarefas.ts`): subárvores reusadas > 0 por frame de mutação de texto.

### M — ciclo de vida do nó (`dom/arvore.rs`, `mutacao.rs`, `dom.ts`)

- **Fecha:** a arena nunca recicla, `__wrappers` só cresce, `dom.free` nunca
  é chamado pela fachada.
- **Desenho:** freelist de `idx` para nós desligados sem referência externa
  (o `NodeId` versionado já dá a geração); `__wrappers` sobre `WeakRef`/
  `FinalizationRegistry` quando o motor os tiver (`WeakMap` já existe — issue
  #217 diz o que falta); `document.close()`/descarte a chamar `dom.free`.
- **Aceitação:** teste: inserir e remover 100 000 nós não faz a arena crescer
  além do pico vivo.
- **Feito, branch `feat/dom-lote-m-ciclo-de-vida` (2026-09-04):** geração POR
  NÓ (`dom/freelist.rs`) — `Dom::node_generation: Vec<u32>` paralelo a
  `nodes`; `to_abi`/`from_abi` não mudaram (já empacotavam sem assumir
  geração uniforme). `alloc_slot` reusa um `idx` da freelist antes de
  crescer a arena; `recycle` incrementa só a geração DESSE `idx` e purga
  TODO mapa lateral indexado por `NodeIdx` (`style_overrides`, `listeners`,
  `listener_cbs`, `input_values`, `image_pixels`, `own_pixels`,
  `scroll_regioes`, `active_transitions`, `prev_computed`, `anim_override`,
  `anim_start`, `dirty_self`, `dirty_children`, `last_fragment`, `hovered`,
  `focused_input`) — sem isto o PRÓXIMO ocupante do `idx` herdaria estado do
  anterior. Quem decide reciclar: a fachada TS (decisão (a) do enunciado,
  rejeitada a (b) — contagem de referências exigiria uma chamada extra em
  CADA leitura que devolve um `NodeId`, não só na remoção). Novo membro do
  bridge: `dom.releaseSubtree(doc, node)`; a fachada chama-o de
  `removeChild`/`remove()` só quando NENHUM nó da subárvore removida tem
  wrapper em `__wrappers` — `Element.isConnected` novo (getter, sobe por
  `parentNode` até `rootId`), `Document.nodeCount`/`Document.close()`
  (chamam `dom.nodeCount`/`dom.free`+`__dropWindow`, que já existiam sem
  chamador). **`WeakRef`/`FinalizationRegistry` verificados e NÃO usados**:
  `WeakRef.deref()` neste motor nunca devolve `undefined` (o alvo é uma
  propriedade própria comum — `crates/rts-core/.../weakref.rs`) e
  `WeakMap`/`WeakSet` também retêm forte, então nenhuma coleção do lado TS
  hoje esquece uma entrada sozinha; um wrapper só é esquecido pela chamada
  explícita que este lote acrescentou. Testes: `dom/tests/freelist.rs`
  (Rust, 5 testes) + `tests/claude-dom-node-lifecycle.test.ts` (4 cenários).
  `cargo check -p rts-dom -p rts-dom-bridge --tests` limpo (só avisos
  pré-existentes noutros ficheiros). **Não medido ainda**: `cargo test`/a
  suite completa — por integrar pelo orquestrador.

### N — as réguas no CI (`.github/workflows/`, `tests/css/README.md`)

- **Fecha:** nenhum instrumento do `rts-dom` corre em CI.
- **Entrega:** um job (`continue-on-error: true`, como os outros três — está
  escrito no `CLAUDE.md` porquê) que corre `cargo test --profile fast -p
  rts-dom -p rts-dom-bridge` e o corpus, e reescreve o bloco de números do
  `tests/css/README.md` entre marcadores, como o `cross-runtime` faz.
- **Depois:** a primeira régua de PINTURA — screenshot-diff de um subconjunto
  pequeno (cor sólida, gradiente, borda, sombra) contra capturas do Chrome.

---

## 5. Vaga 3 — o CSS como área

O inventário `docs/ui/css-implementation-gaps.md` (secções 4, 5 e 6) é a
lista; aqui está a ordem e a forma. Cada lote começa por uma fixture medida
no Chrome (o procedimento é `scripts/css_fixtures_medir.md`) e só depois pelo
código. Um lote sem fixture não entra.

### O — selectores que faltam (`style/selector/`)

`:has()` (matching relacional + invalidação por descendente — depende de K),
`:target`, `:scope`, `:default`, `:placeholder-shown`, `:autofill`, `:modal`,
`:active`/`:visited` (o DOM tem de conservar o estado), `:focus-visible` real
(distinguir rato de teclado — depende do `rts-input`). Pseudo-elementos que
GERAM caixa: `::marker` (o `listitem/` já emite o marcador — é dar-lhe
estilo), `::first-line`, `::first-letter`, `::placeholder`, `::selection`.

### P — at-rules (`style/stylesheet/`)

`@import` (com o mecanismo de carregamento do lote G/`__readResource`, e a
política que o crítico diz que falta), `@font-face` (depende de T),
`@container` (queries de contentor precisam do tamanho usado → uma segunda
passada de estilo dependente de layout; desenhar antes de codar),
`@property`, `@scope`, `@page`/`@counter-style` (baixa). `@media` completa:
`orientation`, `prefers-*` (com um valor do host), `resolution`, listas e
`not`/`only`.

### Q — CSSOM (`dom/estilo.rs`, `style/stylesheet/sheet.rs`)

A hierarquia que as bibliotecas tocam: `document.styleSheets`, `cssRules`
com índices estáveis, `CSSStyleRule.selectorText`/`.style`, `CSSMediaRule`,
`CSSKeyframesRule`, `CSSStyleDeclaration` com `cssText` canónico,
`getPropertyValue`/`setProperty`/`removeProperty`/`priority`. O
`insert_rule`/`delete_rule` transaccional existente é a base. Depende de J
(especificado vs computado).

### R — grid e flex completos (`layout/grid*.rs`, `layout/flex.rs`, `coluna.rs`)

Grid: colocação por linhas (`grid-row/column-start/end`, spans negativos),
`grid-auto-flow` e `dense`, tracks implícitas (`grid-auto-rows/columns`),
`repeat(auto-fill|auto-fit)`, `minmax` completo, sizing intrínseco
(`min-content`/`max-content`/`fit-content`), `align-content`/`justify-content`
/`justify-self`/`align-self` completos, `subgrid` (baixa). Flex: o piso de
`min-content` no `flex-shrink` (o primitivo existe em `table/widths`),
`flex-direction: *-reverse`, `align-content` em multi-linha, `order` completo,
percentagens de `flex-basis`, `gap` em coluna. Cada um com a sua fixture.

### S — propriedades parseadas sem efeito (a lista da secção 5 do inventário)

Por grupo, e cada grupo é um lote com fixtures:
- **texto:** `text-overflow: ellipsis`, `word-spacing`, `tab-size`,
  `line-clamp`, `hyphens` (precisa de dicionário — decidir), `text-wrap:
  balance/pretty`, `list-style-image`.
- **decoração:** `text-shadow`, `text-decoration-style/color/thickness`,
  `text-underline-offset`, `background-clip/origin/attachment`,
  `mix-blend-mode` (pede composição — ver U).
- **imagens e recortes:** `background-image` (url — precisa do loader e do
  cache de imagens que `image_pixels` já tem), `background-repeat/position/
  size`, `object-fit/position`, `clip-path` (básico: `inset`, `circle`),
  `filter` (`opacity`, `blur` pede compositing).
- **transformações:** `transform-origin`, matriz 2D completa (`skew`,
  `matrix`), depois 3D com `perspective` (pede pintura por camadas — U).
- **host:** `cursor` e `pointer-events` (hit-test tem de os consultar),
  `scrollbar-width/color` (após G), `caption-side`, `zoom`.

### T — fontes reais (`layout/medida.rs`, `rts-egui`, e uma decisão de crate)

`@font-face` com carregamento de `ttf`/`otf`/`woff2` (o epaint carrega
fontes; `woff2` precisa de um decodificador), fallback por família e por
script, métricas reais em todo o lado (após B e F), kerning e ligaduras via
shaping (`rustybuzz` atrás do `TextMeasurer` — o trait já remede prefixos
inteiros, por isso encaixa), quebra de linha UAX#14 (`unicode-linebreak`) e
clusters de grafema (`unicode-segmentation`) em vez de `char_indices`. O
`rts-dom` NÃO ganha dependências por isto: o shaping vive atrás do trait, no
backend, e o headless calibrado continua a ser o modo sem janela. Bidi
(UAX#9) e `writing-mode` vertical só depois — é uma passada entre `wrap_runs`
e o posicionamento, e a lente 5 diz que não verificou se cabe sem mais.

### U — composição e pintura (`layout/pintura.rs`, `display.rs`, `rts-egui`)

Stacking contexts completos (`z-index` real, `opacity` < 1, `transform`,
`isolation`), pintura por camadas quando um segundo backend o pedir,
`box-shadow` com `spread`/`inset` completos, `border-image`, gradientes
radiais e cónicos, `outline`. E a régua de pintura de N, sem a qual nada
disto se mede.

---

## 6. Vaga 4 — a superfície DOM que as bibliotecas pedem

O React 18 e o Preact 10 montam e respondem (2026-08-30). O método que os
fez funcionar foi correr a biblioteca REAL e ler onde parava; é o método
desta vaga. A ordem é por quantas páginas reais cada uma destrava.

### V — o que o React ainda não exerce

`MutationObserver` (o que qualquer framework de "islands" usa),
`IntersectionObserver` e `ResizeObserver` (precisam do layout — após B/G),
`requestAnimationFrame` alinhado ao frame do backend, `getComputedStyle`
completo (valores iniciais resolvidos — a fixture `claude-computed-valor-inicial`
já pina o `""`), `dataset`, `classList` vivo (hoje método-valor),
`insertAdjacentHTML/Element`, `Range`/`Selection` (o caret e a selecção de
texto que o `input.rs` já tem por baixo), `<template>`/`content`,
`DocumentFragment`, `cloneNode(deep)`, `contains`, `closest`, `matches`
(verificar quais já existem em `dom.ts` — parte já existe).

### W — formulários completos

Constraint validation (`checkValidity`, `:valid`/`:invalid`, `required`,
`pattern`), `FormData`, `submit` com serialização, `<select multiple>`,
`<input type=range|date|color|file>` (o `replaced.rs` tem o básico),
`contenteditable` (grande — desenhar antes).

### X — bibliotecas reais como régua

Uma por lote, real do CDN, num exemplo `examples/claude-<lib>-janela.ts` e
numa fixture headless que afirma o DOM resultante: **Vue 3** (usa
`MutationObserver`? não — usa `Proxy` e `queueMicrotask`; deve montar quase
já), **jQuery 3** (o que mais exercita `getComputedStyle`, `offset()`,
`outerWidth` — precisa de B), **htmx** (`fetch` + `insertAdjacentHTML` +
`MutationObserver`), **Alpine.js**, **Lit** (web components: `customElements`,
shadow DOM — é a única desta lista que pede uma árvore nova; adiar).

### Y — Shadow DOM e custom elements

`attachShadow`, `<slot>`, `:host`, `::slotted`, `::part`, `customElements.define`
com os callbacks de ciclo de vida. Uma árvore de composição além da árvore de
nós — é o primeiro item que pede ao `Dom` de 51 campos um tipo separado (a
lente 1 diz onde isso dói). Só depois de I, J e V.

---

## 7. O que este plano não decide

- ~~Se o motor é uma fronteira de segurança (§4.H).~~ Decidido 2026-09-04: não
  é. §4.H tem o que isso mudou no código.
- Se e quando `dom.ts` (1 847 linhas) é partido em ficheiros — o bridge
  concatena preludes; partir é mecânico e vale um lote próprio com o dump de
  paridade a provar zero pixels movidos.
- Se o `#[rtse::class]` chega ao `rts-dom-bridge` antes ou depois da vaga 2 —
  depende da opção `extend` da macro, que é trabalho do `rts-macro`.
