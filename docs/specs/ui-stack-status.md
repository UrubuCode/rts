# Stack de UI do RTS — ESTADO ATUAL (plano consolidado)

> Mapa do que existe hoje no experimento da stack de UI, o que falta, e a relação
> com o roadmap egui (F0-F5). Datado 2026-06-25. Branch:
> `feat/dom-owns-state-and-facade`. Leia junto com:
> - `dom-in-ts-architecture.md` (DOM/layout em TS, egui canvas burro)
> - `dom-render-input-interfaces.md` (interfaces render/input abstratas)
> - `engine-limits-found-building-ui.md` (limites do motor encontrados na prática)

## A arquitetura em uma figura

```
rts-dom (Rust)        árvore + parser + estado(estilo/layout-intent) + store. ABI de leitura.
   ↓ (TS lê via ABI)
fachada DOM (TS)      Document/Element (spec MDN) + LAYOUT em TS (paralelizável)
   ↓
rts-render (Rust)     interface ABSTRATA render.* / input.* + trait Renderer/InputSource
   ↓ (backend ativo)
rts-egui              UM backend que implementa os traits (pinta + capta input). Trocável.

   E, em paralelo ao DOM, sobre a MESMA base:
rts:canvas (TS)       UI imediata (Canvas/App) + componentes + loop base. SEM DOM.
```

A chave: o TS fala `dom.*` / `render.*` / `input.*` — **nunca `egui.*`** (exceto
janela/loop). O egui é um backend plugável; trocá-lo não muda nada acima.

## ✅ O que JÁ EXISTE (entregue e validado em tela)

### Crates novos
- **`rts-dom`** — DOM retido headless: parser HTML, árvore em arena, `NodeId`
  versionado `{gen,idx}`, query (tag/#id/.classe) O(1), mutação, store público
  (`with_dom`), estado de estilo (`style.rs`) e layout-intent (`block.rs`)
  migrados pra cá. ABI `rts:dom` (parseHtml/querySelector/setText/getText/
  getAttribute/tagName/childCount/childAt/nodeStyleSlot/displayOf/defineStyle/...).
- **`rts-render`** — interface ABSTRATA: trait `Renderer` (rect/text/line/image/
  measureText/begin/end) + `InputSource` (mouse/key/text por polling) + registro
  do backend ativo. Namespaces ABI `render` e `input`. Fachada `.ts` `rts:canvas`
  (Canvas/App/componentes) como prelude.

### Camada DOM (TS)
- **Fachada `document`/`Element`** fiel à MDN: querySelector/querySelectorAll/
  getElementById/createElement/documentElement; textContent (get/set), tagName,
  id, className, getAttribute/setAttribute/hasAttribute, children, appendChild,
  remove. Validado: `document.querySelector(".t").textContent = x` funciona.
- **Layout engine em TS** (PoC): percorre a árvore via ABI, calcula posições/box
  model, emite comandos de canvas. É código TS → alvo do paralelizador do RTS.

### Camada render/input (abstrata, backend plugável)
- `render.*`: rect/text/line/**image** (bitmap RGBA → vídeo/imagem/viewport)/
  measureText/beginFrame/endFrame. egui é o backend (impl do trait).
- `input.*` — **COMPLETO** (3 fases, ver `input-system-design.md`):
  - **Mouse:** mouseX/Y, down/clicked/**pressed/released/doubleClicked**,
    **deltaX/Y**, **dragging** (drag nativo), wheel/**wheelX**, **setCursor**.
  - **Teclado:** códigos completos (A-Z 100-125, 0-9 130-139, F1-F12 140-151,
    edição/navegação 1-15) × **keyPressed/keyDown/keyReleased**; **modificadores**
    modCtrl/Shift/Alt/Cmd (atalhos).
  - egui capta o cru (winit/SO); `input.*` é a fachada abstrata; trocável por
    outro backend. **O input NÃO depende de egui** — é um plugin.
  - **Camada ergonômica (TS, no App):** FOCO real (focusedId/setFocus/isFocused),
    `clickable(id)` (idle/hover/pressed/clicado, com release-dentro), `textField(id)`
    (campo com foco exclusivo — só o focado digita). Resolve formulários reais.

### Camada canvas (UI imediata, sem DOM)
- **`Canvas`/`App`** + `createApp`/`createAppAt`: loop base (o dev mantém o while;
  beginFrame/delta/endFrame tiram o boilerplate), delta time (via `rts:time`),
  frameCount, **controlador de FPS** (setFps/fps).
- **Componentes**: label, panel, button, slider, checkbox, progressBar, **tabs**,
  textInput, **layout automático** (column + auto*). hit-test/hover/clique embutidos.

### Janelas
- **multi-window** (N janelas num programa — já suportado, UiCtx por handle).
- **multi-monitor**: `moveWindow` + `setNextWindowPos`/`createAppAt` (nasce no
  monitor escolhido — confiável).

### Exemplos (todos rodam, validados em tela)
claude-dom-headless / dom-facade / dom-interactive / canvas-poc / render-abstract /
input-abstract / layout-ts / canvas-facade / app-loop / components / tabs /
multiwindow / image-video / **showcase** (4 abas) / **keyboard** (teclado+mods) /
**mouse** (drag/double/cursor) / **focus-form** (2 campos com foco real).

### Docs
3 specs de arquitetura + o mapa de limites do motor.

## ⬜ O que FALTA (próximos passos, priorizados)

### Curto prazo (refinamento do que existe)
1. **Medição de texto EXATA** — hoje `measureText` é aproximado (0.52·size·n); a
   exata via atlas de fontes do egui é um TODO isolado em `canvas.rs`/`measure_text`.
2. **Backspace/edição de cursor no textField** — append + foco JÁ funcionam;
   backspace/seleção/cursor-no-meio dependem do limite `.length`/string-ops sobre
   shape não-provada (ver limites #4).
3. **Modo vsync-off** (benchmark) — pra medir FPS acima do teto do monitor; hoje o
   Fifo limita (e tem kill-gate por causa do bug da janela-que-parava).
4. **2º backend (headless/PPM)** — provar "N renders genéricos" de fato (render.*
   escrevendo num buffer/PNG, sem janela). É o teste definitivo do isolamento.
5. **Input fase 4** (opcional) — drag-helper no App + cursor automático (mãozinha
   sobre clickable). Touch/gamepad/IME = futuro distante.

> **INPUT está COMPLETO** (fases 1-3: teclado+mods, mouse rico, foco+eventos) —
> saiu das pendências. Bug "dados de ponteiros" (retorno string com alias U64 em
> vez de AbiType::Handle literal) corrigido — ver limites #9.

### Médio prazo (completar camadas)
5. **Fachada DOM → spec MDN completa** — Node/Text/classList/innerHTML/insertBefore/
   removeChild (ver `dom-in-ts-architecture.md` tabela).
6. **Layout engine TS completo** — wrap de texto, width%, display horizontal/grid,
   margin-collapse; e ligá-lo ao paralelizador.
7. **Eventos DOM ricos** — addEventListener-like por polling; hover/focus.
8. **Decoder de imagem (PNG/JPG)** e depois vídeo (codec) — `render.image` já
   aceita qualquer bitmap RGBA; falta a fonte dos pixels.

### Dependências do MOTOR (destravar primeiro — ver limites doc)
Os limites #1/#2/#4 do `engine-limits-found-building-ui.md` (const-captura-em-fn,
dispatch-sobre-getter, .length/string-ops sobre shape não-provada) são os que mais
travam UI rica. Implementá-los no motor simplifica MUITO a fachada e os
componentes (hoje cheios de workarounds: number-em-vez-de-bool, literais,
métodos-diretos).

## Relação com o roadmap egui F0-F5 (`html-engine/rts-html-roadmap.md`)

O experimento **divergiu** do F0-F5 a partir do momento em que o layout migrou
pro TS (motivado pelo paralelismo do RTS — o roadmap assumia egui-layout). O que
do roadmap permanece: o `rts-dom` (árvore/estado) e a doutrina. O que mudou: quem
faz o layout (TS, não egui) e a interface abstrata render/input (nova). O roadmap
F0-F5 descreve o caminho "egui renderiza HTML direto"; este experimento descreve
"DOM/layout em TS sobre render abstrato". Decidir com o time qual é o oficial —
ou se coexistem (o roadmap como motor-HTML-puro, este como stack-de-UI-geral).

## Resumo de uma linha

Existe hoje uma fundação de UI completa e funcional sobre o RTS — DOM real,
layout em TS, render/input abstratos com backend plugável, canvas ergonômico,
biblioteca de componentes, multi-window/monitor, render.image — tudo validado em
tela, com um mapa claro dos limites do motor a destravar. Pronta pra consolidar e
crescer.
