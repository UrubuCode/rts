import egui from "rts:egui";
import dom from "rts:dom";
import render from "rts:render";
import input from "rts:input";

// I2 — DOM INTERATIVO: junta tudo. O DOM (rts:dom) é a árvore; o LAYOUT em TS
// calcula posições E guarda o retângulo de cada nó; o INPUT (input.*) dá o mouse;
// o programa faz HIT-TEST (qual nó está sob o cursor?) e dispara a ação no clique
// — o DOM é dono dos eventos, o backend (egui) só capta o cru e pinta (render.*).
// É o modelo do browser: compositor reporta coords, DOM dispara onclick.
//   target/release/rts.exe run examples/claude-dom-interactive.ts

// estilo dos itens (via rts:dom; o egui não decide nada)
dom.defineBlock("item", 0, 0, 0, 0);
dom.defineStyle("item", 1, 0x1E2A3AFF); // bg
dom.defineStyle("item", 5, 2);          // border width
dom.defineStyle("item", 6, 0x3399FFFF); // border color
dom.defineStyle("item", 7, 8);          // radius
dom.defineStyle("item", 0, 0xFFFFFFFF); // text color
dom.defineStyle("item", 2, 18);         // font

const d = dom.parseHtml(
  "<ul><item>Item A</item><item>Item B</item><item>Item C</item></ul>"
);

const win = egui.openWindow("DOM interativo (clique nos itens)", 460, 360, 0);
egui.moveWindow(win, 2200, 400); // tela 2

// retângulos por nó (arrays paralelos module-level — captura segura no motor)
const nodeIds: number[] = [];
const rectX: number[] = [];
const rectY: number[] = [];
const rectW: number[] = [];
const rectH: number[] = [];
const clickCount: number[] = [];

// coleta os <item> uma vez (NodeId estável)
const ul = dom.querySelector(d, "ul");
const n = dom.childCount(d, ul);
let k = 0;
while (k < n) {
  nodeIds.push(dom.childAt(d, ul, k));
  clickCount.push(0);
  rectX.push(0); rectY.push(0); rectW.push(0); rectH.push(0);
  k = k + 1;
}

const COUNT = nodeIds.length;

while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break;
  render.beginFrame(win);

  const mx = input.mouseX(win);
  const my = input.mouseY(win);
  const clicked = input.mouseClicked(win, 0);

  render.rect(win, 0, 0, 460, 360, 0x12161CFF, 0, 0, 0);
  render.text(win, 20, 16, "Clique nos itens do DOM:", 0xB0B8C0FF, 16, 0);

  // LAYOUT em TS: empilha os itens, guarda o retângulo de cada nó, pinta via render.*
  let i = 0;
  let y = 56;
  while (i < COUNT) {
    const node = nodeIds[i];
    const x = 20;
    const w = 420;
    const h = 56;
    // guarda o retângulo deste nó (pro hit-test)
    rectX[i] = x; rectY[i] = y; rectW[i] = w; rectH[i] = h;

    // HIT-TEST em TS: o mouse está sobre o retângulo DESTE nó?
    const over = mx >= x && mx <= x + w && my >= y && my <= y + h;
    if (over && clicked !== 0) {
      clickCount[i] = clickCount[i] + 1;
    }

    // cor da caixa varia com hover
    let bg = dom.nodeStyleSlot(d, node, 1);
    if (over) bg = 0x2A3A50FF;
    const bc = dom.nodeStyleSlot(d, node, 6);
    const rad = dom.nodeStyleSlot(d, node, 7);
    render.rect(win, x, y, w, h, bg, 2, bc, rad);

    // texto do nó (do DOM) + contagem de cliques
    const label = dom.getText(d, node);
    render.text(win, x + 16, y + 10, label, 0xFFFFFFFF, 18, 0);
    render.text(win, x + 16, y + 32, "cliques: " + clickCount[i], 0x99CCFFFF, 13, 0);

    y = y + h + 12;
    i = i + 1;
  }

  render.endFrame(win);
}

dom.free(d);
egui.close(win);
