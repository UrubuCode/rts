# Motor de render HTML+CSS do RTS — estudo e plano

Pasta com **todo o estudo** do motor de render HTML+CSS próprio do RTS, sobre o
`rts-egui`. Objetivo de longo prazo: o "nosso DOOM" — um motor de render do zero,
dominando cada camada (rumo a um motor de browser).

> **Status (2026-06-23):** DECIDIDO. O motor leve de HTML retido já na main
> (DOM em árvore + alocador de blocos data-driven em TS + mutação por NodeId, na
> `rts-egui`) é a **direção oficial**, evoluído IN-PLACE por fases. A crate
> `rts-html` das 5 árvores **não será criada**. O plano operacional vivo é o
> **[rts-html-roadmap.md](rts-html-roadmap.md)** (F0-F5). O antigo plano de 5
> árvores foi **rebaixado a north-star congelado** ([rts-html-north-star.md](rts-html-north-star.md)),
> referência conceitual que não dita fases. Decisão tomada após análise
> multi-agente (4 abordagens × 3 lentes adversariais + crítica de completude).

## Como ler

1. **[rts-html-roadmap.md](rts-html-roadmap.md)** — **o documento operacional
   vivo. COMECE POR AQUI.** Estratégia, os 10 pontos de decisão resolvidos, os 6
   invariantes duros, o roadmap F0-F5 (pixel-cedo, kill-gates verificáveis), e a
   primeira fatia concreta de ≤1 dia.
2. **[rts-html-north-star.md](rts-html-north-star.md)** — o antigo `PLANO.md` de 5
   árvores (DOM→Style→Layout→DisplayList→Paint), CONGELADO como teto teórico.
   Referência conceitual; NÃO dita fases. Só "acorda" se o critério de teto de F4
   provar que o egui não basta além do parágrafo rico.
3. **[arquitetura.md](arquitetura.md)** — a síntese arquitetural detalhada do
   pipeline canônico (fundamento do north-star), com os structs Rust de cada fase.
4. **[critica-adversarial.md](critica-adversarial.md)** — a revisão cética que
   cortou o escopo ao realista (flexbox/grid/position/CSS5-moderno fora) e
   apontou os riscos reais (text layout, hit-testing, "5 árvores sem pixel").
   Suas correções alimentaram tanto o north-star quanto o roadmap.
5. **[analises/](analises/)** — as 4 pesquisas-base que sustentam tudo:
   - `analise-browser-pipeline.md` — como motores reais (Servo/Blink/robinson)
     estruturam o pipeline; por que DOM é árvore, não lista.
   - `analise-css-subset.md` — subset pragmático de CSS por prioridade (tabela
     fase 1/2/3); o que nunca entra.
   - `analise-egui-as-paint.md` — egui como backend de PAINT absoluto (Painter,
     galley, medição de texto, ScrollArea), não como layout.
   - `analise-rts-constraints.md` — encaixe no RTS: doutrina (Rust=infra,
     TS=alto nível), limites do engine TS, decisão de crate nova `rts-html`.

## Decisões-chave (resumo — estratégia DECIDIDA, ver roadmap)

> As decisões abaixo são as VIGENTES (roadmap). As decisões antigas (crate nova
> `rts-html`, paint absoluto universal) viraram o north-star congelado — o estudo
> em `analise-*` que as fundamentou continua válido como base conceitual.

- **Evoluir o motor leve IN-PLACE na `rts-egui`** — não criar `rts-html`. O DOM
  retido em árvore + atributos + índices O(1) + mutação por NodeId já existem e
  são reaproveitados; só as funções internas de `frame.rs::render_*` evoluem.
- **egui é o motor de layout POR PADRÃO** (os 4 displays); paint absoluto
  (`allocate_painter` + `LayoutJob`/`galley.rows`) entra como exceção cirúrgica
  num único `render_*` (F4), só onde o egui comprovadamente não compõe
  (parágrafo inline rico com link). A regra "paint absoluto universal" do
  north-star foi cortada.
- **CSS chega cedo via slot numérico opaco** (`defineStyle`/`setStyle`): o Rust
  nunca dá match em string CSS; o TS mapeia nome→índice. `BlockDef` (display)
  ganha um `ComputedStyle` por NodeId com cascade em Rust.
- **Eventos por polling** (`pollEvent(h) → NodeId`), sem listener reativo
  (bloqueado por #195/captura mutável). A mutação programática de DOM (que o
  north-star nem previa) permanece.
- **Escopo honesto:** "rich text + caixas estilizadas + clique" usável em ~6.5-9.5
  semanas (F0-F3); parágrafo inline rico + links em +2-3 (F4). Flexbox, grid,
  position absoluta, animations, CSS5 moderno, font-family — **fora** (cortes
  herdados do north-star).
- **Pixel na primeira semana (P1):** caminho vertical fino ponta-a-ponta antes
  de construir as 5 árvores completas.
