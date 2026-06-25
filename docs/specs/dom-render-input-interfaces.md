# Interfaces de render e input — DOM isolado, backend plugável

> Spec das DUAS interfaces que isolam o DOM/layout de QUALQUER backend de janela.
> O DOM não conhece o egui; o egui é só UM backend que implementa estas
> interfaces. Decidido com o usuário (2026-06-25). Complementa
> `docs/specs/dom-in-ts-architecture.md`. Status: **spec** (implementação faseada).

## O princípio: dois fluxos, ambos abstratos

Uma UI tem saída (pintar) e entrada (mouse/teclado). Os dois cruzam a fronteira
DOM↔backend, e os dois são abstraídos pela mesma razão: trocar o backend (egui →
web → headless) não deve tocar o DOM/layout.

```
DOM/layout (TS)  ──comandos de render──►  backend   [SAÍDA]
DOM/layout (TS)  ◄──input cru (poll)────   backend   [ENTRADA]
```

O TS NUNCA nomeia `egui`. Ele fala com `render.*` e `input.*` genéricos; o backend
ativo (hoje egui) implementa esses primitivos. Outro backend é trocar a
implementação, não o DOM.

## Interface 1 — RENDER (saída). O DOM manda, o backend pinta.

O layout-TS calcula posições e emite primitivos ABSOLUTOS. O backend só executa.
Cores `0xRRGGBBAA` (number); coords/tamanhos em pontos (number).

| Primitivo | Assinatura | Semântica |
|---|---|---|
| `render.beginFrame(target)` | `(target) -> void` | abre um frame de pintura no alvo (janela). |
| `render.rect` | `(target, x, y, w, h, fill, strokeW, stroke, radius) -> void` | retângulo preenchido + borda + cantos. |
| `render.text` | `(target, x, y, text, color, size, flags) -> void` | texto em (x,y) topo-esquerda. flags 1=bold 2=italic 4=mono. |
| `render.line` | `(target, x1, y1, x2, y2, w, color) -> void` | linha. |
| `render.measureText` | `(target, text, size, bold) -> width` | **largura do texto na fonte real**. A ÚNICA op do render que o layout PRECISA consultar (medir exige a fonte; o TS não tem). Síncrona. |
| `render.endFrame(target)` | `(target) -> void` | fecha + apresenta o frame. |

> Hoje implementado como `egui.drawRect/drawText/drawLine/measureText/beginFrame/
> endFrame` (PoC F-canvas). O passo de isolamento é renomear/rotear para um
> namespace `render` genérico que o egui satisfaz — o TS deixa de importar
> `rts:egui` e passa a falar só `render`.

### Display list (opcional, evolução)
Em vez de chamar `render.rect(...)` N vezes, o layout pode produzir uma DISPLAY
LIST (array de comandos) e entregá-la ao backend de uma vez. Vantagem: o backend
nem precisa ser chamado pelo TS — lê o buffer (permite headless, serializar,
mandar pela rede, paralelizar a geração). Difere só na entrega; os primitivos são
os mesmos. Decidir quando virar gargalo.

## Interface 2 — INPUT (entrada). O backend capta cru, o DOM interpreta.

**Quem capta:** o backend (tem a janela; o SO entrega o input a ele). Ele só
reporta o CRU — posição, clique, tecla — SEM saber de nós DOM.
**Quem interpreta:** o DOM/layout. Ele fez o hit-test (tem as posições!), então
ELE sabe qual nó está sob o mouse e dispara os eventos DOM.

Modelo **POLLING** (não callback reativo — o motor não suporta closures
capturantes bem; roadmap F3): a cada frame o TS pergunta ao backend o estado do
input e faz o dispatch.

| Primitivo | Assinatura | Semântica |
|---|---|---|
| `input.mouseX/mouseY` | `(target) -> number` | posição do cursor (pontos), no espaço de coords do render. |
| `input.mouseDown` | `(target, button) -> bool` | botão pressionado AGORA (0=esq 1=dir 2=meio). |
| `input.mouseClicked` | `(target, button) -> bool` | houve um clique completo NESTE frame. |
| `input.wheel` | `(target) -> number` | delta de scroll do frame. |
| `input.keyDown` | `(target, keycode) -> bool` | tecla pressionada agora. |
| `input.keyPressed` | `(target, keycode) -> bool` | tecla disparou neste frame (com repeat). |
| `input.textInput` | `(target) -> string` | texto digitado neste frame (UTF-8). |

### Hit-test e eventos: responsabilidade do DOM (TS)
O DOM/layout, a cada frame:
1. lê `input.mouseX/Y` + `input.mouseClicked`.
2. **hit-test**: encontra o nó cujo retângulo de layout contém (x,y) — usando as
   posições que ELE calculou (guardadas no layout pass). O backend não participa.
3. dispara os eventos DOM do nó: `onclick`, `onmouseover`, `addEventListener`
   (todos resolvidos em TS por polling; sem armazenar closures capturantes —
   estado em variáveis module-level, padrão do roadmap F3).

Assim o backend permanece BURRO (não conhece nós, não faz hit-test) e o DOM é
dono da semântica de eventos — espelhando o browser (o compositor reporta
coordenadas; o DOM dispara eventos).

## Por que essa divisão (resumo)

- **Render:** o backend não decide layout (o TS decide) — só pinta primitivos +
  mede texto (precisa da fonte). Trocar de backend = reimplementar 6 primitivos.
- **Input:** o backend não interpreta (o DOM faz hit-test + eventos) — só reporta
  o cru. Trocar de backend = reimplementar ~7 leituras de estado.
- **DOM isolado:** fala `render.*`/`input.*`, nunca `egui.*`. É o "sistema isolado
  que qualquer render sabe renderizar" — e do qual qualquer backend sabe ler input.

## Plano de implementação (fases)

1. **I0 — namespace `render` genérico:** rotear os `egui.draw*`/`measureText`
   atuais para um namespace `render` (o egui é o impl). O layout-TS passa a falar
   `render.*`. (Isolamento da SAÍDA — o PoC já tem os primitivos.)
2. **I1 — namespace `input`:** egui expõe `input.mouseX/Y/clicked/...` por polling.
3. **I2 — hit-test no layout-TS:** o layout guarda os retângulos por nó; um
   `hitTest(x,y)->node` em TS; dispatch de `onclick` por polling.
4. **I3 — backend headless (prova do isolamento):** um segundo backend que
   implementa `render.*` escrevendo num PPM (screenshot) — prova que o DOM
   renderiza sem egui. (Opcional, mas é o teste definitivo do isolamento.)
