# Critica adversarial do plano

Vou ser direto e duro, como pedido. O plano é tecnicamente competente e a pesquisa de base é boa, mas ele comete o pecado clássico de quem nunca enviou um motor de layout: subestima as três coisas que efetivamente consomem o tempo (texto, cascade real, incrementalidade) e empacota um escopo "HTML avançado + CSS5" que é fantasia. Crítica concreta abaixo.

---

## 1) ESCOPO — "HTML avançado + CSS5" é fantasia; o subset real está escondido na pesquisa, não no plano

O plano de arquitetura é honesto em alguns pontos (P1 = simple selectors, block-only), mas o *título* da tarefa ("HTML avançado + CSS5") e o corpo não batem. A pesquisa css-subset entrega um subset sóbrio e correto; **o plano de arquitetura não cita esse subset com a mesma franqueza** — ele descreve a tubulação de 5 árvores como se ela já fosse o motor. Tubulação não é o trabalho. O trabalho é o conteúdo de cada caixa.

O que custa 10x mais do que o plano sugere, em ordem de dor:

1. **Inline/text layout (Fase 7, jogada pro fim, 10% do progresso bar)** — isto é 40-60% do esforço real de um motor, e o plano o trata como o último 10%. Line breaking, medição por run, baseline, mistura de fontes inline, whitespace collapsing (o algoritmo de colapso de espaço do CSS é uma fonte infinita de bugs sutis), `white-space`, justificação. Voltarei nisso na seção 2.
2. **Cascade "correto"** — o plano diz "ordena por especificidade, aplica". Isso é a parte fácil. O difícil é shorthand expansion (`margin: 1px 2px 3px 4px`, `border: 1px solid red`, `font: ...`), `initial`/`inherit`/`unset`, valores percentuais que resolvem em momentos diferentes (`%` de width resolve no layout, não na cascade), e `em`/`rem`/`%`/`auto` com regras de resolução distintas. O plano resolve `Em`/`Percent` "contra o pai" na Fase 3 — **errado para width/height/margin/padding em %**, que dependem do *containing block* (Fase 4), não do computed do pai. Isso é um bug de design já presente no `enum Dimension { Auto, Px(f32) }`: ele descarta `%` cedo demais.
3. **Fontes** — o plano assume que egui resolve fontes. egui resolve *uma* família embarcada. `font-family`, fallback, font matching, web fonts, bold sintético vs. peso real, itálico sintético — nada disso vem de graça. O plano menciona `weight: u16` e `italic: bool` como se fossem aplicáveis ao galley; egui não tem síntese de peso arbitrário sem você fornecer os arquivos de fonte.

**Veredito:** o subset *atingível* é o da Fase 1/2 da pesquisa css-subset (block + inline básico, ~12 propriedades, simple selectors + descendant). O plano deveria declarar isso no título e abandonar "avançado/CSS5".

---

## 2) TEXT LAYOUT — o monstro está reconhecido mas terceirizado com otimismo perigoso

O plano sabe que texto é difícil (cria o trait `TextMeasurer`), mas a forma como ele divide trabalho entre "egui mede" e "eu quebro linha" é ingênua em dois pontos concretos:

- **O trait `TextMeasurer` está mal desenhado.** A assinatura `measure(text, font_size, weight, italic) -> (w,h)` trata uma run como atômica e uniforme. Texto real numa linha é multi-run, multi-fonte, multi-cor (`<b>`, `<span>`, `<a>`), e a quebra de linha tem que acontecer **através** dos limites de run, não run a run. Se você quebra run-a-run, "fica **bold** aqui" quebra errado entre o normal e o bold. A pesquisa egui-as-paint já aponta a saída correta — `LayoutJob` com múltiplas `LayoutSection` — mas aí **quem quebra a linha é o egui (via `wrap.max_width`), não você**. O trait do plano assume o contrário (você quebra, egui mede granular via `glyph_width`). Você não pode ter os dois: ou delega a quebra de uma linha inteira (multi-run) ao `layout_job` do egui, ou reimplementa shaping multi-run você mesmo. O plano fala dos dois caminhos sem escolher, e o caminho "eu quebro com `glyph_width`" **não compõe** com spans inline mistos.
- **Recomendação dura:** delegue **o máximo possível** ao galley. Construa um `LayoutJob` por *bloco* de contexto inline (não por run), deixe `wrap.max_width` = largura do content box, e leia de volta `galley.rows` para descobrir onde o egui quebrou e posicionar. Você perde controle fino (hifenização, `text-indent` em linhas específicas), mas ganha bidi, kerning, shaping e quebra multi-run **de graça e corretos**. Reimplementar isso em Rust puro é um projeto de meses por si só. O plano mantém a porta aberta para "fazer você mesmo com `glyph_width`/`row_height`" — **feche essa porta**, é uma armadilha de tempo.
- **O que o plano nem menciona:** bidi (texto RTL/árabe/hebraico), grapheme clusters (emoji, combining marks), whitespace collapsing, `word-break`/`overflow-wrap`. Se a meta é só LTR latino, **diga isso explicitamente** e corte bidi do escopo. Hoje o plano finge que `String` + `glyph_width` cobre texto, e não cobre.

---

## 3) egui-como-paint — hit-testing está SUBdimensionado (é o calcanhar real)

A pesquisa egui-as-paint é excelente e cobre as armadilhas de coordenada/scroll/repaint corretamente (`allocate_painter`, `show_viewport`, tradução content→screen, recriar galley em mudança de DPI). **Mas o plano de arquitetura quase não fala de hit-testing**, e é aí que o modo imediato te morde:

- `allocate_painter(size, Sense::hover())` te dá **um** `Response` para a superfície inteira. Para saber *qual box* foi clicado (link, botão, qual `<a href>`), você precisa fazer hit-testing **você mesmo**: manter uma lista de retângulos clicáveis + node-id, e no frame seguinte testar `response.interact_pointer_pos()` contra eles. O plano menciona isso de passagem ("registra seu Rect... casado por node-id") mas não dimensiona o trabalho: você está reconstruindo o sistema de eventos do DOM (capture/bubble não, mas pelo menos "qual é o alvo do clique", z-order resolvendo sobreposição, hover state para `:hover`, cursor `pointer` sobre links).
- **Latência de 1 frame** já existe no código atual (`button_results`/`button_cursor`) e o plano herda isso conscientemente. OK para botão. **Não OK para `:hover`** — hover state com 1 frame de atraso pisca. E `:hover` está no escopo "CSS" implícito. Ou você corta `:hover` (recomendado para o MVP) ou aceita que ele exige re-layout reativo no mesmo frame, o que o pipeline de 5 árvores recomputado-do-zero não suporta barato.
- **Repaint:** modo imediato re-pinta tudo todo frame. O plano cobre culling por viewport (bom), mas **não cobre rebuild da árvore**. Hoje `egui.html(string)` re-parseia? Se sim, você reconstrói DOM→Style→Layout a cada frame que a string muda — e como o RTS é imediato, o TS provavelmente chama `egui.html(...)` todo frame. Isso é um reflow completo por frame. Para uma página estática pequena, tudo bem. Para qualquer coisa com texto real, medir o galley de tudo todo frame vai dominar o tempo. **O plano não tem estratégia de cache de layout entre frames**, e o modelo de árvore efêmera (`StyledNode<'a>` com empréstimos da árvore-pai) torna o cache *mais* difícil, não mais fácil — lifetimes amarrados ao frame.

---

## 4) INCREMENTALIDADE — "construa 5 camadas antes de ver um pixel" (o defeito mais grave do plano)

Esta é a crítica mais séria. **O primeiro pixel renderizado pelo motor novo só aparece no passo 6 de 7** (90% da barra de progresso). Passos 1-5 (DOM, CSS, Style, Layout, Display list) produzem **structs Rust que ninguém vê**. Você vai escrever ~60% do código contra testes unitários antes de qualquer coisa aparecer na tela. Isso é exatamente o anti-padrão que a própria CLAUDE.md condena ("entrega valor em cada fase").

Pior: o passo 4 (layout) **depende** do `TextMeasurer`, que só existe de verdade no passo 6. O plano "resolve" isso com um measurer mockado — ou seja, você valida o layout block contra larguras de texto *falsas*, e quando o measurer real entra, todo o layout inline muda e você re-debuga. O mock te dá uma sensação falsa de progresso.

**Reordene para ver pixel cedo:**
1. Comece pelo **caminho vertical mais fino possível**: parse trivial (`<p>texto</p>` + `<h1>`), sem CSS, sem cascade, block-only, e **pinte imediatamente** via `Painter` com galley. Isso é DOM mínimo + layout block mínimo + paint, ligados ponta-a-ponta, na primeira semana. Um pixel real na tela.
2. *Depois* engrosse cada camada (CSS, cascade, inline, scroll). Cada incremento renderiza algo novo e visível.

O risco do plano atual é o clássico "5 árvores prontas, 0 pixels, e quando liga tudo nada alinha". Você não tem feedback visual para pegar os erros de coordenada/baseline/box model até o fim — e esses erros *só* aparecem visualmente.

---

## 5) MIGRAÇÃO — coexistência está OK no papel, mas o ponto de fricção real é o re-parse e o estado de eventos

Esta parte o plano acertou na decisão (fila plana sobrevive como "modo simples", HTML é caminho novo e separado, sem conversão HTML→`WidgetCmd`). A calculadora e os widgets atuais **não quebram** porque o caminho novo é paralelo. Bom.

Os riscos reais que o plano subdimensiona:
- **`egui.html(str)` muda o corpo mas o modelo de eventos é incompatível.** Hoje botões casam por **índice posicional** (`button_cursor`). O modo HTML quer casar por **node-id**. Mas o node-id só é estável se o DOM for estável entre frames — e se o TS re-chama `egui.html(stringDiferente)` todo frame, os node-ids dançam. O plano afirma "node-id é mais estável que índice" sem mostrar de onde vem o id: se é gerado por ordem de parse, é **exatamente tão frágil quanto o índice**. Id estável exige `id=`/`key=` explícito no HTML ou um esquema de reconciliação — que o plano não tem.
- **Dois buffers no `UiCtx` (`FrameContent::Simple | Html`)** — e se o usuário misturar `egui.label()` com `egui.html()` no mesmo frame? O plano assume exclusividade ("`endFrame` escolhe o walker pelo conteúdo presente"). Misturar é um caso real (HTML + um slider nativo embaixo) e o plano não diz como compor os dois walkers na mesma janela com ordem correta.

---

## 6) CSS5/moderno — corte explícito (a pesquisa já lista, o plano não assume)

O plano de arquitetura **não declara o que está fora**. A pesquisa css-subset declara, e bem. Esta lista tem que estar no plano, não enterrada na pesquisa. **Corte explicitamente e sem volta para o MVP:**

- **Flexbox** — cada formatting context é "um mini-projeto" (a própria pesquisa diz). Flex é resolução iterativa de `grow/shrink/basis`. Fora do MVP. (E é o que as pessoas mais vão querer — seja honesto que não tem.)
- **Grid** — fantasia completa para este escopo. Resolução de trilhas `fr`/`minmax`/auto-placement é um subsistema maior que todo o resto do motor. Cortar e nunca prometer.
- **`position: absolute/fixed/sticky`, `float`, `z-index` real (stacking contexts)** — fora. O plano fala de "z-order = ordem da display list", o que é verdade *até* você ter `z-index`/`position`, aí quebra.
- **Container queries, `:has()`, `@scope`, cascade layers `@layer`, nesting** — fantasia. A pesquisa já marca `:has()` e container queries como genuinamente caros (invalidação / dependência circular layout↔estilo). Nunca prometer.
- **`transform`/`transition`/`animation`/`filter`/`clip-path`** — fora. Animação exige um loop temporal + invalidação que o pipeline efêmero não suporta.
- **`var()`/custom properties** — fora do MVP (passo de resolução em cascade com fallback).

Manter só: block + inline normal flow, `display: block|inline|none`, box model, ~12 propriedades de paint/box, simple + descendant selectors, especificidade + herança. **Isto já é 3-6 meses de trabalho honesto.** "CSS5" some.

---

## OS 5 RISCOS REAIS MAIS SÉRIOS

1. **Text/inline layout subdimensionado e mal arquitetado.** É 40-60% do esforço, está como "último 10%", e o trait `TextMeasurer` não compõe com spans inline mistos. Sem reescrever a fronteira de medição em torno do `LayoutJob`/`galley.rows` do egui, isto trava o projeto. **Maior risco isolado.**

2. **Zero pixels até 90% do plano.** Construir 5 camadas com measurer mockado antes de ver a tela é receita para "tudo pronto, nada alinha". O feedback visual que pega erros de baseline/coordenada/box model só chega no fim.

3. **Reflow completo por frame + nenhum cache de layout.** Modo imediato + `StyledNode<'a>` efêmero amarrado ao frame = re-parse + re-style + re-measure de tudo todo frame. Texto real domina o tempo. Não há plano de cache, e os lifetimes escolhidos *atrapalham* o cache.

4. **Hit-testing e identidade de evento.** "Qual box foi clicado" e node-id estável entre frames não estão resolvidos. Node-id gerado por ordem de parse é tão frágil quanto índice. `:hover` com latência de 1 frame pisca. Você está reconstruindo metade do sistema de eventos do DOM sem dizer.

5. **Resolução de unidades no momento errado.** `enum Dimension { Auto, Px(f32) }` descarta `%` na Fase 3, mas `%` de width/margin resolve contra o *containing block* na Fase 4. Bug de design já no struct. Computed values têm momentos de resolução distintos (`em` cedo, `%` tarde) — o plano colapsa os dois.

## O QUE CORTAR (sem dó)
Flex, grid, position/float/z-index, container queries, `:has()`, `@layer`, nesting, var(), transform/transition/animation, bidi/RTL, web fonts, font fallback. Tudo "CSS5". Reduzir seletores a simple+descendant e propriedades a ~12.

## ONDE O PLANO PRECISA SER MAIS HUMILDE
- Trocar o título "HTML avançado + CSS5" por "subset block+inline de HTML/CSS estático, LTR, fonte única".
- Admitir que **o egui faz o texto** (via `LayoutJob`/`galley`), não que você faz com `glyph_width`. Fechar a porta do "faço eu mesmo".
- Inverter a ordem: **pixel na primeira semana** (caminho vertical fino), camadas engrossadas depois — não 5 árvores antes do primeiro pixel.
- Resolver `%` no layout, não na cascade. Corrigir `Dimension` para carregar `Percent`.
- Ter uma resposta para **re-parse/cache entre frames** e para **identidade estável de nó** (id/key explícito), antes de escrever a Fase 4.

O esqueleto de 5 árvores está certo (é o pipeline canônico). O erro do plano não é a arquitetura — é confundir ter a tubulação pronta com ter um motor, subestimar texto, e não entregar pixel até o fim.