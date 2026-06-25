import egui from "rts:egui";
import dom from "rts:dom";

// PoC do LAYOUT EM TS: o motor de layout é ESTE código TS. Ele lê a árvore do
// rts:dom (childAt/tagName/getText/nodeStyleSlot/displayOf), calcula posições e
// tamanhos (box model + texto via measureText), e emite primitivos de canvas
// (drawRect/drawText) — o egui só pinta. É o núcleo da arquitetura "DOM/layout em
// TS, egui burro". Layout de N nós é código TS → alvo do paralelizador do RTS.
// Ver docs/specs/dom-in-ts-architecture.md.
//   target/release/rts.exe run examples/claude-layout-ts.ts

const NONE = -1;
// slots de estilo (contrato com rts:dom)
const S_COLOR = 0, S_BG = 1, S_FONT = 2, S_PAD = 3, S_MARGIN = 4, S_BW = 5, S_BC = 6, S_RADIUS = 7;

// ── monta o estado de estilo por tag (via rts:dom; o egui não decide nada) ──────
dom.defineBlock("page", 0, 0, 0, 0);
dom.defineBlock("card", 0, 0, 0, 0);
dom.defineBlock("h1", 0, 24, 0, 4);
dom.defineBlock("p", 0, 0, 0, 0);

dom.defineStyle("page", S_BG, 0x12161CFF);
dom.defineStyle("page", S_PAD, 16);
dom.defineStyle("card", S_BG, 0x1E2A3AFF);
dom.defineStyle("card", S_PAD, 14);
dom.defineStyle("card", S_MARGIN, 8);
dom.defineStyle("card", S_BW, 2);
dom.defineStyle("card", S_BC, 0x3399FFFF);
dom.defineStyle("card", S_RADIUS, 10);
dom.defineStyle("h1", S_COLOR, 0xFFFFFFFF);
dom.defineStyle("h1", S_FONT, 22);
dom.defineStyle("p", S_COLOR, 0xC0C8D0FF);
dom.defineStyle("p", S_FONT, 16);

// ── parseia o DOM (uma vez) ─────────────────────────────────────────────────────
const d = dom.parseHtml(
  "<page>" +
    "<h1>Layout calculado em TS</h1>" +
    "<card><h1>Card A</h1><p>Primeiro paragrafo do card A.</p></card>" +
    "<card><h1>Card B</h1><p>Segundo card, empilhado abaixo.</p></card>" +
  "</page>"
);

const win = egui.openWindow("Layout em TS (egui so pinta)", 520, 420, 0);
egui.moveWindow(win, 2200, 400); // UI na tela 2

// ── LAYOUT ENGINE em TS — 2 fases por nó (mede, depois pinta), pra a CAIXA do pai
// ficar ATRAS do texto dos filhos: a caixa é pintada ANTES de descer (usando a
// altura medida), então os filhos pintam por cima. ───────────────────────────────

// FASE 1: mede a altura total que o nó ocupa (sem pintar). Recursiva.
function measureNode(node: number, w: number): number {
  const text = dom.getText(d, node);
  const font = dom.nodeStyleSlot(d, node, 2);
  const pad = dom.nodeStyleSlot(d, node, 3);
  const margin = dom.nodeStyleSlot(d, node, 4);
  const m = margin === -1 ? 0 : margin;
  const p = pad === -1 ? 0 : pad;
  const childCount = dom.childCount(d, node);

  let inner = 0;
  if (childCount === 0) {
    if (text.length > 0) {
      const fsize = font === -1 ? 16 : font;
      inner = fsize + 6;
    }
  } else {
    let i = 0;
    while (i < childCount) {
      inner = inner + measureNode(dom.childAt(d, node, i), w - (m + p) * 2);
      i = i + 1;
    }
  }
  return inner + p * 2 + m * 2;
}

// FASE 2: pinta a caixa do nó e depois desce nos filhos (texto fica por cima).
// Retorna a altura ocupada (= measureNode).
function paintNode(node: number, x: number, y: number, w: number): number {
  const text = dom.getText(d, node);
  const color = dom.nodeStyleSlot(d, node, 0);
  const bg = dom.nodeStyleSlot(d, node, 1);
  const font = dom.nodeStyleSlot(d, node, 2);
  const pad = dom.nodeStyleSlot(d, node, 3);
  const margin = dom.nodeStyleSlot(d, node, 4);
  const bw = dom.nodeStyleSlot(d, node, 5);
  const bc = dom.nodeStyleSlot(d, node, 6);
  const radius = dom.nodeStyleSlot(d, node, 7);

  const m = margin === -1 ? 0 : margin;
  const p = pad === -1 ? 0 : pad;
  const boxX = x + m;
  const boxY = y + m;
  const boxW = w - m * 2;
  const boxH = measureNode(node, w) - m * 2;

  // pinta a CAIXA primeiro (fica atras do conteudo).
  if (bg !== -1 || bw !== -1) {
    const fill = bg === -1 ? 0x00000000 : bg;
    const sw = bw === -1 ? 0 : bw;
    const sc = bc === -1 ? 0 : bc;
    const rad = radius === -1 ? 0 : radius;
    egui.drawRect(win, boxX, boxY, boxW, boxH, fill, sw, sc, rad);
  }

  const contentX = boxX + p;
  const contentW = boxW - p * 2;
  let cy = boxY + p;

  const childCount = dom.childCount(d, node);
  if (childCount === 0) {
    if (text.length > 0) {
      const fsize = font === -1 ? 16 : font;
      const tcolor = color === -1 ? 0xFFFFFFFF : color;
      egui.drawText(win, contentX, cy, text, tcolor, fsize, 0);
    }
  } else {
    let i = 0;
    while (i < childCount) {
      const child = dom.childAt(d, node, i);
      cy = cy + paintNode(child, contentX, cy, contentW);
      i = i + 1;
    }
  }
  return boxH + m * 2;
}

const root = dom.rootId(d);

while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break;
  egui.beginFrame(win);
  // o documento tem 1 filho de topo (<page>)
  const pageCount = dom.childCount(d, root);
  let yy = 0;
  let i = 0;
  while (i < pageCount) {
    const top = dom.childAt(d, root, i);
    yy = yy + paintNode(top, 0, yy, 520);
    i = i + 1;
  }
  egui.endFrame(win);
}

dom.free(d);
egui.close(win);
