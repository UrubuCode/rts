# Inventário de lacunas CSS/DOM — RTS versus Blink

**Estado de referência:** `origin/main` após o merge do PR #2582, commit `f2cea6976003945c34c03d00514daca1fa5dc7ac`.

**Medição (2026-08-27):** corpus local de 49 fixtures, Chrome/Blink a 1280×800, tolerância de 1 px, através de `examples/claude-css-runner.ts`: **41/49 fixtures aprovadas**, com **23 desvios em 8 fixtures**; `rts-dom` com 713 testes aprovados (`docs/ui/blink-parity-2026-08-27.md`).

> **Actualização 2026-09-04: os 23 desvios da secção 3 estão FECHADOS — 49/49.** A vaga 1 de `crates/rts-dom/PLAN.md` (lotes C, D, E, F) fechou as quatro prioridades A–D deste inventário, cada uma com um teste Rust que afirma os rects do Chrome. A secção 3 fica como registo do que foi medido e de como se lia; as secções 4, 5 e 6 continuam a ser o inventário do que falta, e a ordem do §7 continua a partir do item E (arquitectura da cascade), agora no PLAN §4.J e §5. A auditoria estrutural de 2026-09-04 (`docs/ui/html-engine/analises/2026-09-04-auditoria-estrutural/`) é a leitura actual do motor.

## 1. Como ler este inventário

Este documento separa quatro estados. A presença de um nome em `style/props/tabela.rs` só prova que existe um slot no `ComputedStyle`; não prova que o valor tenha efeito no layout, na pintura ou no hit-test. A distinção é necessária porque o CSS define uma cadeia de valores **declared → cascaded → specified → computed → used → actual**, e `getComputedStyle()` pode expor valores computados ou valores dependentes do layout [1].

| Estado | Critério aplicado no RTS | Interpretação para compatibilidade |
|---|---|---|
| **Implementado e consumido** | O parser aceita, a cascade resolve e existe consumidor verificável no DOM, layout, pintura ou animação. | Pode ser considerado coberto no escopo medido, sem generalizar para toda a especificação. |
| **Parcial/limitações** | Há parser, slot e pelo menos um consumidor, mas apenas um subconjunto da semântica Blink é aplicado. | É suporte funcional, mas ainda requer fixtures específicas e não deve ser chamado de completo. |
| **Parseado/guardado mas sem efeito** | O valor é tipado, serializado ou preservado, mas nenhum caminho de layout/pintura/hit-test o consulta. | `getComputedStyle()` pode parecer correcto enquanto a página visual continua igual. |
| **Ausente** | A sintaxe ou a semântica ainda não entra no caminho aplicável, ou a regra é descartada. | Deve ser implementado antes de prometer compatibilidade para esse recurso. |

A arquitectura de referência de Blink também mantém separadas a indexação de regras, o matching, a resolução da cascade e a construção dos valores usados [2]. Portanto, um desvio de rectângulo não deve ser corrigido apenas alterando a serialização CSS.

## 2. O que já está implementado e consumido

A base estrutural já é utilizável. O tokenizer lossless com spans, o AST recursivo (`StylesheetAst`/`BlockAst`/`DeclarationAst`), o lowering para `Rule`/`DeclBlock`, a indexação de candidatos e a cascade normal/`!important` estão activos em `style/syntax.rs`, `style/stylesheet/rules.rs`, `style/stylesheet/sheet.rs` e `style/ruleindex.rs`. `var()` e custom properties são resolvidos por elemento antes da aplicação das declarações, e `initial`/`unset` possuem caminho semântico para valores conhecidos.

| Área | Suporte consumido actualmente | Evidência principal |
|---|---|---|
| Selectors básicos | Tags, classes, IDs, universal, atributos com presença/igualdade/prefixo/sufixo/substring/palavra/dash-prefix, e combinadores descendente, filho, irmão adjacente e irmão posterior. | `style/selector/tipos.rs`, `style/selector/sintaxe.rs`, `style/selector/casamento.rs`. |
| Pseudo-classes estruturais | `:first-child`, `:last-child`, `:only-child`, `:empty`, `:root`, `:nth-child()`, e equivalentes `*-of-type`. | `style/selector/tipos.rs` e matcher fornecido pelo DOM. |
| Pseudo-classes funcionais | `:not()`, `:is()`/`:matches()`, `:where()` e `:lang()`, com especificidade própria para cada forma. | `style/selector/sintaxe.rs`, incluindo parsing recursivo e `specificity()`. |
| Estados disponíveis no DOM | `:checked`, `:disabled`, `:enabled`, `:required`, `:hover`, `:focus`, `:focus-within`, `:focus-visible`, `:link`, `:read-only` e `:read-write`. | `style/selector/tipos.rs` e matcher de estado do DOM. |
| Fluxo de caixas | Block flow, inline, inline-block, `display:none`, `flow-root`, box model básico, margens, padding, bordas, `box-sizing`, limites de tamanho, overflow básico e replaced elements. | `layout/caixa.rs`, `layout/bloco.rs`, `layout/vertical.rs`, `layout/linha.rs`. |
| Flex e grid básicos | Flex row com parte de alinhamento, gaps e wrapping; grid com tracks básicas (`px`, `%`, `fr`, `auto`, parte de `minmax()`), auto-placement, áreas nomeadas e alinhamento elementar. | `layout/grid.rs`, `layout/flex.rs` e testes em `layout/tests/grid.rs`/`flex.rs`. |
| Texto elementar | Cor, tamanho/família/peso/estilo de fonte no modelo disponível, `line-height`, `text-transform`, `letter-spacing`, `white-space` básico, quebra por palavras, `text-align` básico e decoração simples. | `style/props/tabela.rs`, `style/values/texto.rs`, `layout/linha.rs`, `layout/quebra.rs`. |
| Pintura e efeitos suportados | Cor/fundo sólido, gradiente linear, sombra de caixa, opacidade, visibilidade, transformações 2D básicas, bordas por lado e radius armazenado por canto. | `layout/pintura.rs`, `style/background.rs`, `style/effects.rs`, `style/radius.rs`. |
| Pseudo-elementos gerados | `::before` e `::after` com `content`, matching separado da caixa originante e integração no fluxo de pintura. | `style/selector/tipos.rs`, `style/stylesheet/sheet.rs`, `pseudo` e layout de conteúdo gerado. |
| Condições e animações | `@media` por `min-width`/`max-width`, `@supports` tri-state para condições de declaração, `@layer` no caminho normal/important, `@keyframes`, transições/animações no subconjunto modelado. | `style/stylesheet/mod.rs`, `stylesheet/rules.rs`, `stylesheet/supports.rs`, `style/timing.rs`, `dom/animacao.rs`. |
| CSSOM inicial | `insert_rule()`/`delete_rule()` em blocos sintácticos, reconstrução transaccional de rules, keyframes, layers e índices; mutação básica do atributo `style`. | `style/stylesheet/sheet.rs` e `dom/estilo.rs`. |

O resultado não significa que cada propriedade dessas áreas seja completa. Por exemplo, o radius elíptico já é preservado e serializado como `10px 20px`, mas a pintura legada ainda dispõe de um único raio horizontal por rectângulo; por isso `border-radius` aparece novamente na categoria parcial.

## 3. Lacunas parciais com impacto directo no layout

Estas são as lacunas prioritárias porque já possuem uma reprodução mensurável contra Blink. A especificação distingue `width:auto`/`height:auto` computados dos comprimentos usados depois de o layout conhecer o containing block [1]; as falhas abaixo estão nessa fronteira de **used values**, não no tokenizer.

### 3.1 Prioridade A — floats, `clear` e BFC

**Seis desvios continuam em `claude-clear.html` e `claude-float-clear.html`.** O suporte existente já cria exclusões laterais e permite que texto contorne um float, mas ainda não tem o comportamento completo de um bloco formatting context.

| Fixture e alvo | Esperado Blink | RTS | Sintoma técnico |
|---|---:|---:|---|
| `claude-clear.html` — `#limpa-ambos.y` | 95 | 110 | Clearance é calculado com a combinação actual de strut/margem, não com a geometria exacta do float. |
| `claude-clear.html` — `#limpa-direita.y` | 40 | 80 | `clear:right` não selecciona apenas floats da direita. |
| `claude-clear.html` — `#limpa-esquerda.y` | 80 | 95 | `clear:left` não selecciona apenas floats da esquerda. |
| `claude-float-clear.html` — `#ao-lado.y` | 0 | 60 | O fluxo normal não conserva a posição esperada ao lado da exclusão. |
| `claude-float-clear.html` — `#limpo.y` | 60 | 80 | Clearance e margem do bloco seguinte são combinados no ponto errado. |
| `claude-float-clear.html` — `#pai-so-floats.h` | 0 | 60 | O pai cresce para conter floats mesmo quando não estabelece BFC. |

A implementação em `layout/float.rs` tem uma lista de exclusões, mas `style/text.rs` documenta que os três valores `left`/`right`/`both` ainda actuam como `both`. `layout/vertical.rs` também regista a divergência deliberada: o pai cresce para conter os seus floats fora de BFC. A próxima implementação deve manter exclusões por lado, separar o strut de clearance da margem normal e aplicar o crescimento do pai apenas quando houver BFC, `flow-root`, flex/grid, tabela, float, posicionamento ou overflow que o estabeleça. As regras gerais de BFC e colapso de margens estão alinhadas com a descrição do modelo visual CSS [3] e com a orientação de BFC da MDN [7].

**Critério de aceitação:** fixtures mínimas para `clear:left`, `clear:right`, `clear:both`, float herdado de um ancestral, pai apenas com floats e pai com `flow-root`/`overflow:hidden`; rects e altura do pai devem ser comparados ao Chrome antes e depois de cada corte.

### 3.2 Prioridade B — `position:relative` e posicionamento absoluto

**Seis desvios continuam em `claude-position-absolute.html` e `claude-position-relative.html`.** `absolute`/`fixed` já saem do fluxo e `absolute` procura um ancestral positioned, mas o cálculo ainda é shrink-to-fit simplificado. `relative` é parseado e permanece no fluxo, porém os offsets ainda não deslocam a caixa pintada.

| Fixture e alvo | Esperado Blink | RTS | Lacuna |
|---|---:|---:|---|
| `claude-position-absolute.html` — `#esticado.w` | 200 | 0 | `left:0; right:0` com largura automática não calcula o stretch do containing block. |
| `claude-position-absolute.html` — `#esticado.h` | 100 | 0 | `top:0; bottom:0` com altura automática não calcula o stretch vertical. |
| `claude-position-absolute.html` — `#irmao-normal.y` | 270 | 290 | A geometria usada do irmão ainda recebe o efeito do posicionamento de forma diferente do Blink. |
| `claude-position-absolute.html` — `#meio.y` | 50 | 70 | A composição entre margem, caixa normal e descendente absoluto ainda difere. |
| `claude-position-relative.html` — `#relativo.x` | 30 | 0 | `left` relativo não altera a posição visual. |
| `claude-position-relative.html` — `#relativo.y` | 55 | 40 | `top` relativo não altera a posição visual, embora o irmão deva conservar o seu lugar no fluxo. |

`layout/posicionado.rs` confirma que a resolução actual escolhe `left` antes de `right`, mede a caixa naturalmente e só depois aplica offsets. O trabalho deve introduzir a regra de stretch quando ambos os offsets de um eixo estão definidos e a dimensão é `auto`, além de uma fase separada para deslocar a pintura de `relative` sem modificar o espaço reservado no fluxo. O comportamento deve seguir o modelo de posicionamento CSS [6]. `sticky` está no enum, mas ainda se comporta como fluxo normal: não é um posicionamento sticky funcional.

**Critério de aceitação:** reproduções para `relative` com `top/left`, quatro combinações de offsets absolutos, margens e `width/height:auto`, nested absolute e `fixed` contra viewport; verificar rects do alvo e dos irmãos.

### 3.3 Prioridade C — grid areas e rows

`grid-template-areas` não está ausente: o parser cria a matriz, `grid-area:<nome>` encontra o rect nomeado e o item pode atravessar várias células. Contudo, o fixture real ainda apresenta **três desvios**.

| Fixture e alvo | Esperado Blink | RTS | Lacuna observada |
|---|---:|---:|---|
| `claude-grid-areas.html` — `#corpo.h` | 300 | 0 | A área `corpo` não recebe a altura da row central. |
| `claude-grid-areas.html` — `#lateral.h` | 300 | 0 | A área `lado` não recebe a altura da row central. |
| `claude-grid-areas.html` — `#rodape.y` | 360 | 60 | A row de rodapé é posicionada como se a matriz não tivesse dimensionado as rows anteriores. |

O código em `layout/grid.rs` já dimensiona `grid-template-rows` e conserva as rows declaradas pela matriz, mas a colocação e o dimensionamento de áreas continuam num subconjunto. Ainda faltam, como partes próximas do mesmo contrato, a colocação completa por `grid-column-start/end` e `grid-row-start/end`, `grid-auto-flow`/`dense`, colunas implícitas via `grid-auto-columns`, `auto-fill`/`auto-fit` e sizing intrínseco conforme a especificação de Grid [4].

**Critério de aceitação:** manter os testes já existentes de áreas lado a lado e spans, acrescentar o caso exacto da fixture divergente e medir separadamente rows explícitas, rows implícitas e colunas implícitas.

### 3.4 Prioridade D — inline formatting e tipografia

**Oito desvios continuam em `claude-text-align.html`, `claude-vertical-align.html` e `claude-white-space.html`.** O modelo de linha já existe, mas as métricas de fonte são aproximações calibradas por avanço médio, e a linha inline-block só implementa parte do alinhamento.

| Fixture e alvo | Esperado Blink | RTS | Lacuna |
|---|---:|---:|---|
| `claude-text-align.html` — `#herdado-pai.h` | 40 | 38 | A altura do pai com conteúdo inline herdado difere na formação do line box. |
| `claude-vertical-align.html` — `#base.y` | 14,91 | 0 | `baseline` não usa a baseline real do conteúdo. |
| `claude-vertical-align.html` — `#meio.y` | 10 | 5 | O cálculo de `middle` usa apenas a altura da linha/caixa. |
| `claude-vertical-align.html` — `#sub.y` | 19,91 | 0 | `sub` cai no mesmo caminho aproximado de baseline/topo. |
| `claude-vertical-align.html` — `#super.y` | 7,25 | 0 | `super` não desloca pela métrica tipográfica. |
| `claude-vertical-align.html` — `#texto-topo.y` | 16,91 | 0 | `text-top` não usa o topo da caixa de texto. |
| `claude-white-space.html` — `#pre.h` | 40 | 20 | A preservação de quebra literal/linhas não produz a mesma altura. |
| `claude-white-space.html` — `#pre-wrap.y` | 115 | 95 | A quebra preservando espaços e newline difere no cursor vertical. |

`layout/linha_ib.rs` aplica deslocamento apenas para `middle` e `bottom`; baseline, top, text-top, text-bottom, sub e super convergem para `dy = 0`. `layout/linha.rs` já consulta `white-space`, `line-height`, `word-break` e `overflow-wrap`, mas a previsão da banda e a métrica são simplificadas. A regra de alinhamento de inline-level boxes deve ser comparada com CSS Inline Layout [5], e não corrigida ajustando apenas a altura serializada.

**Critério de aceitação:** armazenar baseline/ascent/descent por atom ou run, testar os sete valores da fixture, preservar newline e tabs conforme `white-space`, e separar altura de line box de altura da caixa do elemento.

## 4. Cascade, selectors, at-rules e CSSOM ainda incompletos

Estas lacunas não geram necessariamente um dos 23 desvios actuais, mas impedem afirmar que a implementação acompanha a arquitectura de um browser.

### 4.1 Cascade e defaulting

`style/stylesheet/sheet.rs` ordena regras por layer, especificidade e ordem, com inversão para `!important`, e `style/parse/mod.rs` implementa `initial`, `inherit` e `unset` para propriedades conhecidas. O limite estrutural é que a unidade aplicada continua a ser um `DeclBlock` já reduzido por propriedade. Os registros não conservam, em cada declaração vencedora, origem, layer, importância, selector e valor declarado completo.

Consequentemente, **`revert` e `revert-layer` continuam ausentes**. Não são sinónimos de limpar um campo final: `revert` recua para uma origem anterior e `revert-layer` recua para uma layer anterior [1]. Também permanecem parciais a semântica completa de nomes/nesting de layers, anonymous layers, a precedência de `@keyframes` dentro de layers, origem user/UA separada e o tratamento de todos os shorthands/reset-only longhands. O `@supports` actual responde sobretudo “o parser aceita?”; o próprio ficheiro `stylesheet/supports.rs` admite que uma propriedade guardada sem consumidor pode entrar como suportada.

**Próximo desenho recomendado:** introduzir `DeclarationRecord` por propriedade e por elemento, executar a ordenação de origem/layer/importance antes de reduzir para `ComputedStyle`, e só então aplicar defaulting e resolução de `var()`. Isso permite implementar `revert`/`revert-layer` sem criar uma segunda lista paralela de regras.

**2026-09-04 (lote J):** `revert` e `revert-layer` estão implementados, mas não pela via de um `DeclarationRecord` que guarda proveniência para TODA propriedade — o custo disso seria pago por toda página, com ou sem `revert`. Em vez disso, `declarations_from` continua a reduzir a cascade como antes (a unidade continua a ser um `DeclBlock`), e cada declaração `revert`/`revert-layer` marca só o NOME da propriedade em `ComputedStyle::revert_props`/`revert_layer_props` (`style/props/mod.rs`, o mesmo padrão de `inherit_props`). `style/stylesheet/revert.rs` (novo módulo) lê esses marcadores DEPOIS da cascade normal — e só quando algum existe — e RE-CORRE a `MatchedRules` (já pequena: as candidatas de um elemento, não a folha inteira) para resolver, por nome, a origem/layer anterior. A proveniência nunca é materializada para o resto das propriedades: uma folha sem `revert` paga duas comparações de `Option` a `None` por chamada de `declarations_from` e nada mais.

### 4.2 Selectors e pseudo-elementos

A base de selectors é funcional, mas o vocabulário actual é fechado. `:active` e `:visited` estão modelados, porém nunca casam porque o DOM não conserva esses estados. `:focus-visible` aproxima-se de `:focus` porque não há distinção entre foco de rato e teclado.

Continuam ausentes `:has()` e `:target`, explicitamente recusados em `style/selector/sintaxe.rs`; também não existem as pseudo-classes modernas de validação/formulário, `:scope`, `:default`, `:placeholder-shown`, `:autofill`, `:modal`, `:fullscreen`, `:picture-in-picture` e os estados de diálogo/popover. `::marker`, `::selection`, `::placeholder`, `::first-line`, `::first-letter` e demais pseudo-elementos não geram caixas; apenas `::before` e `::after` são consumidos. A implementação de `:has()` exigirá matching relacional e invalidação por descendentes, não apenas adicionar uma variante ao enum.

### 4.3 At-rules e condições

O lowering semântico actual trata `@media`, `@supports`, `@layer` e `@keyframes`. `@media` está limitado a `min-width`/`max-width` e keywords neutros; queries de orientação, preferência, impressão, resolução e listas completas não são avaliadas. `@supports selector(...)`, `font-tech(...)` e `font-format(...)` respondem false/indeterminado por desenho conservador.

Não há caminho aplicável para `@import`, `@font-face`, `@container`, `@scope`, `@property`, `@page`, `@counter-style` e regras de animação/transição mais amplas. At-rules desconhecidos podem permanecer no AST para tooling, mas não entram na cascade. A especificação de Cascade inclui `@import` e APIs específicas de layers, portanto a preservação no AST não equivale a suporte [1].

### 4.4 CSSOM

`insert_rule()` e `delete_rule()` operam sobre os blocos sintácticos anexados e reconstruem a folha inteira. Isso é uma API inicial, não o CSSOM de browser. Ainda faltam a lista de `CSSRule` individuais com tipos e índices estáveis, `CSSStyleRule`, `CSSMediaRule`, `CSSLayerBlockRule`, `CSSImportRule`, `CSSKeyframesRule`, `CSSStyleDeclaration` individual, `cssRules`, `selectorText`, `style.cssText` com serialização canónica e a distinção completa entre declaração especificada e estilo computado. A fachada `dom/estilo.rs` oferece propriedades e mutações básicas, mas não essa hierarquia de objectos.

## 5. Parseado/guardado mas sem efeito

A lista seguinte é agrupada por consumidor ausente. Cada item tem slot/parser ou é preservado para compatibilidade, mas não deve ser contado como layout implementado apenas por aparecer no `ComputedStyle`.

| Grupo | Propriedades/valores | Limite actual |
|---|---|---|
| Grid avançado | `align-content`, `justify-self`, `grid-auto-flow`, `grid-auto-columns`, `grid-column-start/end`, `grid-row-start/end` e colocação por linhas completa. | Alguns valores são serializados; o layout usa sobretudo áreas/número de tracks e auto-placement simples. |
| Alinhamento/flex avançado | `align-self`, parte de `order`, `flex-direction` reverse/column, `flex-grow`, `flex-shrink` e `flex-basis` fora do subconjunto exercitado. | O flex funcional actual é principalmente row e distribuição básica; as combinações completas ainda não têm algoritmo equivalente ao Flexbox. |
| Texto guardado | `font-stretch`, `word-spacing`, `text-overflow`, `text-wrap:balance/pretty`, `tab-size`, `line-clamp`, `line-break`, `direction:rtl`, `unicode-bidi`, `hyphens`, `list-style-type` e `list-style-image`. | `direction` não inverte o fluxo, bidi não existe, hifenização e marcadores não são desenhados; algumas opções de quebra e overflow não chegam ao medidor. |
| Decoração e pintura fina | `background-clip`, `background-origin`, `background-attachment`, `mix-blend-mode`, `background-blend-mode`, `text-shadow`, `text-fill-color`, `text-decoration-style`, `text-decoration-color`, `text-underline-offset`, `text-decoration-skip-ink`, `text-decoration-thickness`, `caret-color`. | A lista de display não tem composição, sombra de texto, geometria de decoração por glifo ou recorte por camada. |
| Imagens e recortes | `background-image`, `background-repeat`, `background-position`, `background-size`, `object-fit`, `object-position`, `clip`, `clip-path`, `filter`, `mask-size`, `mask-position`, `mask-repeat`. | O motor não carrega backgrounds, o `<img>` não aplica object-fit/object-position neste caminho e não existe consumidor completo de clip/filter/mask; `mask-image` serve hoje como supressão temporária de fundo. |
| Transformações | Toda a família 3D (`perspective`, `transform-style`, `transform-box`, `translate3d`). | `transform-origin` e a matriz 2D completa (`matrix()`, `skewX`/`skewY`, composição de várias funções) estão implementadas (`layout/transformacao.rs`, lote S-transform) e `getBoundingClientRect` reflete a matriz; não existe matriz 3D nem profundidade de pintura, e a pintura em si (`rts-egui`) ainda só translada/escala — rotação/skew no backend é aproximada (ver o módulo). |
| Host/scroll | `pointer-events`, `cursor`, `scrollbar-color`, `scrollbar-width`, `zoom`, `caption-side`. | A tabela guarda os valores, mas hit-test, ponteiro, largura de barra, zoom de subárvore e layout da legenda ainda pertencem a consumidores não ligados ou ao backend. |

As propriedades intencionalmente recusadas por `style/inert.rs` ficam separadas desta lista: paginação (`page-break-*`, `break-*`, `orphans`, `widows`), composição/desempenho (`backdrop-filter`, `contain`, `content-visibility`, `isolation`, `will-change`), scroll snap/suave, decisões de host, features OpenType, queries de container/anchor, SVG e transformações 3D. Elas são conhecidas como **inertes por decisão de escopo**, não como funcionalidades parcialmente implementadas.

## 6. Ausências explícitas fora do caminho de propriedades

| Subsistema | Ausência verificável |
|---|---|
| Fontes reais | Não há `@font-face`, carregamento de fontes web, fallback CSS completo, métricas OpenType, kerning, eixos variáveis ou shaping Unicode completo; `TextMeasurer` usa aproximações calibradas. |
| Bidi e writing modes | Não há algoritmo bidi nem layout RTL; `direction` e `unicode-bidi` são armazenados/serializados. Também não existe writing-mode vertical. |
| Fragmentação | Não há paginação, colunas de texto completas, `@page`, `break-*` aplicável, `orphans`/`widows` efectivos ou valores por fragmento como `::first-line`. |
| SVG | Não há motor SVG; propriedades `fill`/`stroke` e afins são recusadas como inertes. |
| Composição | Não há stacking contexts completos, blend/backdrop/filter, clipping por `clip-path`/mask, perspective ou rasterização por camadas. `z-index` limita-se ao ordenamento v1 de alguns out-of-flow. |
| Shadow DOM/scoping | Não há host tree, slotted/part, `:host`, `::slotted`, `::part`, `@scope` ou invalidação de escopo. |
| DOM interactivo completo | `:active`, `:visited`, distinção de input modality para `:focus-visible`, formulário constraint validation, seleção de texto, caret e hit-test com `pointer-events` ainda não formam um modelo integrado. |

## 7. Ordem recomendada de implementação

A ordem abaixo maximiza o ganho comprovado no corpus e mantém a separação Blink entre estilo computado e geometria usada.

| Ordem | Frente | Ganho/evidência | Primeiro corte recomendado |
|---:|---|---|---|
| **A** | Float/clear/BFC | 6 desvios directos; corrige também regressões de fluxo em páginas com floats. | Exclusões por lado, clearance correcto, contenção do pai condicionada por BFC e fixtures mínimas. |
| **B** | Positioning | 6 desvios; `relative` e absolute stretch são contratos independentes. | Used width/height por offsets opostos, offsets relativos de pintura, static position e nested out-of-flow. |
| **C** | Grid areas/rows | 3 desvios; áreas já têm parser e testes, logo o corte pode ser pequeno e atribuível. | Corrigir row sizing/placement da fixture, depois grid lines e tracks implícitas. |
| **D** | Inline/text | 8 desvios; exige melhorar métricas e line boxes, não apenas serialização. | Baseline/ascent/descent por atom, `vertical-align` completo, newline/space preservation e altura do line box. |
| **E** | Cascade architecture | Não altera necessariamente os 23 rects de imediato, mas é pré-requisito para `revert`, `revert-layer`, origins e CSSOM honesto. | `DeclarationRecord` com origem/layer/importance/order, depois defaulting e redução para `ComputedStyle`. |

Cada corte futuro deve criar primeiro uma fixture reproduzível e um teste Rust focado, executar `cargo test -p rts-dom --lib --no-fail-fast`, medir o corpus, e só depois executar `cargo check --workspace`, `cargo build --workspace` e `git diff --check`. O trabalho deve partir de uma branch nova baseada em `main`, com commit incremental e PR próprio; esta documentação não altera `main`.

## Referências

[1]: https://www.w3.org/TR/css-cascade-5/ "W3C — CSS Cascading and Inheritance Level 5"
[2]: https://github.com/blueboxd/chromium-legacy/blob/master.lion/third_party/blink/renderer/core/css/style-calculation.md "Blink — Style Calculation"
[3]: https://www.w3.org/TR/CSS2/visuren.html "W3C — CSS 2.1 Visual Formatting Model"
[4]: https://www.w3.org/TR/css-grid-1/ "W3C — CSS Grid Layout Module Level 1"
[5]: https://www.w3.org/TR/css-inline-3/ "W3C — CSS Inline Layout Module Level 3"
[6]: https://www.w3.org/TR/css-position-3/ "W3C — CSS Positioned Layout Module Level 3"
[7]: https://developer.mozilla.org/en-US/docs/Web/CSS/Guides/Display/Block_formatting_context "MDN — Block formatting context"
