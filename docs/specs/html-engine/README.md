# Motor de render HTML+CSS do RTS — estudo e plano

Pasta com **todo o estudo** do motor de render HTML+CSS próprio do RTS, sobre o
`rts-egui`. Objetivo de longo prazo: o "nosso DOOM" — um motor de render do zero,
dominando cada camada (rumo a um motor de browser).

> **Status:** ESTUDO + PLANO (pré-implementação) — MAS já existe na main um motor
> de render de HTML retido por um **caminho DIFERENTE** deste plano (DOM em árvore
> + alocador de blocos data-driven em TS + mutação via JS, tudo na `rts-egui`; a
> crate `rts-html` das 5 árvores NÃO foi criada). Ver a seção
> **"⚠️ STATUS DE IMPLEMENTAÇÃO"** no topo do [PLANO.md](PLANO.md) para o que foi
> feito e onde diverge. **A comparar com os devs:** este plano (motor de browser
> canônico de 5 árvores) vs. o caminho leve implementado — qual vira a direção
> oficial. Os dois coexistem até essa decisão.

## Como ler

1. **[PLANO.md](PLANO.md)** — o documento canônico. Spec de design completa: escopo
   honesto (o que é e o que explicitamente NÃO é), arquitetura de 5 árvores,
   onde cada camada vive, subset CSS por fase, e as **fases de implementação
   P0→P7** (pixel-primeiro, com gates de risco). **Comece por aqui.**
2. **[arquitetura.md](arquitetura.md)** — a síntese arquitetural detalhada: o
   pipeline DOM → Style → Layout → Display list → Paint, com os structs Rust de
   cada fase.
3. **[critica-adversarial.md](critica-adversarial.md)** — a revisão cética que
   cortou o escopo ao realista (flexbox/grid/position/CSS5-moderno fora) e
   apontou os riscos reais (text layout, hit-testing, "5 árvores sem pixel").
   As correções dela estão **incorporadas** no PLANO.
4. **[analises/](analises/)** — as 4 pesquisas-base que sustentam tudo:
   - `analise-browser-pipeline.md` — como motores reais (Servo/Blink/robinson)
     estruturam o pipeline; por que DOM é árvore, não lista.
   - `analise-css-subset.md` — subset pragmático de CSS por prioridade (tabela
     fase 1/2/3); o que nunca entra.
   - `analise-egui-as-paint.md` — egui como backend de PAINT absoluto (Painter,
     galley, medição de texto, ScrollArea), não como layout.
   - `analise-rts-constraints.md` — encaixe no RTS: doutrina (Rust=infra,
     TS=alto nível), limites do engine TS, decisão de crate nova `rts-html`.

## Decisões-chave (resumo)

- **A fila plana `WidgetCmd` atual NÃO escala** para CSS/box model (sem
  ancestralidade/herança/cascade). O modo HTML é um **caminho novo e separado**:
  produz uma **display list**, não `WidgetCmd`. Os dois coexistem (o modo widget
  imediato — calculadora, botões — continua na fila plana).
- **Motor em Rust, crate nova `rts-html`** (DOM/CSS/Style/Layout/Display list),
  zero dependência de egui. A `rts-egui` vira backend de janela + paint. O alto
  nível (montar HTML/CSS, eventos) é TS via `egui.html(string)`. O engine TS novo
  trava em parser char-by-char — por isso o motor é Rust, não TS.
- **egui vira o PAINTER de baixo nível** quando chegarmos ao box model: nós
  calculamos x,y; o egui pinta via `Painter` absoluto + mede texto via
  `LayoutJob`/`Galley` + faz scroll via `ScrollArea`. Paramos de usar o layout
  automático (`ui.label` empilhando).
- **Escopo honesto:** cascade real + box model block/inline + scroll + links
  clicáveis. ~3-6 meses. Um "rich text com caixas", **não** um browser. Flexbox,
  grid, position absoluta, animations, CSS5 moderno — **fora do MVP**.
- **Pixel na primeira semana (P1):** caminho vertical fino ponta-a-ponta antes
  de construir as 5 árvores completas.
