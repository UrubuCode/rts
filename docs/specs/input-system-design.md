# Sistema de input — spec do alvo completo

> O input atual (`rts:input`) é um polling cru MÍNIMO (provou a arquitetura, mas
> tem furos grandes pra UI séria/browser). Esta spec desenha o alvo COMPLETO antes
> de implementar — o que o trait `InputSource` precisa, o que vai em TS, e as
> fases. Datado 2026-06-25. Complementa `dom-render-input-interfaces.md`.

## O que existe HOJE (`InputSource` / `input.*`)

| Fn | Estado |
|---|---|
| `mouseX/mouseY` | ✅ posição |
| `mouseDown(button)` | ✅ segurando (0=esq 1=dir 2=meio) |
| `mouseClicked(button)` | ✅ clique completo no frame |
| `wheel` | ✅ scroll vertical |
| `keyPressed(key)` | ◐ só 8 teclas (Enter/Esc/Space/Backspace/4 setas) |
| `textInput` | ✅ texto digitado (UTF-8) |

**Furos:** teclado quase vazio; sem modificadores; sem press/release separados; sem
drag/double-click; sem foco; sem cursor. Polling manual em tudo (sem eventos).

## Princípio (mantido)

O backend CAPTA o cru (tem a janela); o DOM/app INTERPRETA (hit-test + foco +
eventos). Polling, não callback (limite do motor: closures capturantes quebram).
Tudo abstrato — o TS fala `input.*`, nunca `egui.*`. O egui mapeia das suas APIs
(`egui::Key`, `Modifiers`, `PointerState`).

## Camada 1 — INPUT CRU (trait `InputSource` + `input.*`)

### Mouse (completar)
| Fn nova | Semântica |
|---|---|
| `mousePressed(button)` | botão FOI pressionado NESTE frame (transição up→down) |
| `mouseReleased(button)` | botão FOI solto neste frame (down→up) |
| `mouseDoubleClicked(button)` | duplo-clique neste frame |
| `mouseDeltaX/Y` | movimento relativo do cursor no frame (câmera/scrub) |
| `wheelX` | scroll horizontal (já temos `wheel` = vertical) |
| `dragging(button)` | `true` enquanto arrasta (pressed + movendo) — conveniência |

### Teclado (o maior buraco — códigos NEUTROS completos)
Hoje 8 teclas. Alvo: tabela neutra cobrindo o que o egui expõe. Códigos `KEY_*`:
- **Edição/navegação:** Enter, Escape, Space, Backspace, Tab, Delete, Insert,
  Home, End, PageUp, PageDown, ArrowUp/Down/Left/Right.
- **Letras:** A..Z (códigos 100..125).
- **Dígitos:** 0..9 (códigos 130..139).
- **Função:** F1..F12 (códigos 140..151).
- (Pontuação/símbolos chegam via `textInput`, não como keycode — segue o egui.)

| Fn nova/ampliada | Semântica |
|---|---|
| `keyDown(key)` | tecla segurada AGORA (estado contínuo) |
| `keyPressed(key)` | disparou neste frame (com auto-repeat) |
| `keyReleased(key)` | solta neste frame |

### Modificadores (essencial — atalhos, shift+click)
| Fn | Semântica |
|---|---|
| `modCtrl()` | Ctrl segurado |
| `modShift()` | Shift |
| `modAlt()` | Alt |
| `modCmd()` | Cmd/Super (Win/⌘) — egui `command` (cross-platform) |

Com isso o TS faz `if (input.modCtrl() && input.keyPressed(KEY_C))` → copiar.

### Cursor (feedback visual)
| Fn | Semântica |
|---|---|
| `setCursor(target, kind)` | muda o cursor: 0=default 1=pointer(mão) 2=text(I) 3=resize... O egui mapeia pra `CursorIcon`. O app chama ao passar sobre link/campo. |

## Camada 2 — EVENTOS + FOCO (em TS, sobre o input cru)

O polling cru é a base; pra UI grande, uma camada de conveniência em TS (no
`rts:canvas`/fachada DOM) constrói o que falta — SEM callback (estado em
variáveis; o app chama por frame):

- **FOCO:** um `focusedId` (qual elemento tem o teclado). `clicked` num campo o
  foca; Tab move o foco; só o focado lê `textInput`/teclas. Resolve o textInput de
  verdade (hoje qualquer campo "focado" lê tudo).
- **Hit-test → eventos por nó:** o DOM/app já faz hit-test; formalizar em
  helpers: `onClick(rect)`, `onHover(rect)`, `onDrag(rect)` que retornam o estado
  daquele alvo (lendo o input cru + comparando com o focado/hover anterior).
- **Drag completo:** começo (pressed sobre o alvo) → durante (delta) → fim
  (released) — guardado em estado module-level; um helper `beginDrag/dragDelta/
  endDrag`.
- **Double-click, repeat de tecla:** já no cru; a camada só expõe ergonômico.

(Bubbling/propagação de eventos DOM fica pra quando a fachada DOM tiver a árvore
de listeners — fase posterior; o browser precisa, a UI imediata não.)

## Camada 3 — futuro (não agora)
Touch/multitouch, gamepad, IME (composição p/ CJK), clipboard real (Copy/Cut/Paste
do egui já dão eventos), pointer lock.

## Fases de implementação
1. **Teclado completo + modificadores** (cru) — o maior destravamento. trait +
   egui mapeia `egui::Key`/`Modifiers`. ← começar aqui.
2. **Mouse rico** (pressed/released/double/delta/drag/wheelX/setCursor) — cru.
3. **Foco + eventos por nó** (TS) — a camada usável.
4. **Drag helper + cursor automático** (TS).

## Relação com os limites do motor
A camada 2 (TS) esbarra nos mesmos limites de `engine-limits-found-building-ui.md`
(estado em variáveis module-level, sem closures; number em vez de bool de método).
A spec já assume isso — nada de callback, tudo polling + estado explícito.
