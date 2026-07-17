// Auto-Pong renderizado e jogado pelo motor do RTS.
//
// Carrega `site/game.html` (HTML+CSS do jogo), mostra na janela via egui, e roda
// a FISICA do proprio <script> da pagina: o step() definido no <script> e
// compilado UMA vez (new Function) e chamado a cada frame do loop. Todo o estado
// vive no DOM (data-x/data-y/left/top) — o motor le+reescreve por frame, o que
// exercita cascade + position:absolute + mutacao + o cache de layout por revisao.
//
//   target/release/rts.exe run examples/claude-game.ts
import egui from "rts:egui";
import dom from "rts:dom";
import { fs } from "rts";

const VW = 700;

// carrega o HTML do jogo (o <style> + <div>s + <script> de fisica).
const html = fs.read_text("site/game.html");
const d: i64 = dom.parseHtml("<html><body>" + html + "</body></html>");
const doc = new Document(d);

// roda o <script> da pagina: ele semeia data-vx/data-vy e DEFINE `step()` +
// helpers como fns globais. Como cada <script> roda num escopo isolado, o step
// dessa passada nao sobrevive — mas ele TAMBEM ja aplicou o setup inicial. Para
// o tick por frame, compilamos o corpo de fisica UMA vez aqui (mesmo texto do
// <script>, exposto como um step chamavel que persiste entre frames).
runScripts(doc);

// Corpo de step: identico a logica do <script> da pagina, compilado uma vez para
// persistir entre frames (o padrao validado: new Function chamada por frame muta
// o DOM e o estado vive no proprio DOM). `__h` = handle do DOM.
const STEP_SRC =
  "const document = new Document(__h);"
  + "const ARENA_W = 640; const ARENA_H = 420; const BALL = 28; const PADDLE_H = 90;"
  + "function numAttr(el, name) { const s = el.getAttribute(name); if (s.length === 0) { return 0; } return parseInt(s, 10); }"
  + "function numPx(el, prop, fb) { const v = el.getStyleProp(prop); if (v.length === 0) { return fb; } return parseInt(v, 10); }"
  + "function stepBall(id) {"
  + "  const b = document.getElementById(id); if (b === null) { return; }"
  + "  let x = numAttr(b, 'data-x'); let y = numAttr(b, 'data-y');"
  + "  if (b.getAttribute('data-init').length === 0) { x = numPx(b, 'left', 300); y = numPx(b, 'top', 60); b.setAttribute('data-init', '1'); }"
  + "  let vx = numAttr(b, 'data-vx'); let vy = numAttr(b, 'data-vy');"
  + "  x = x + vx; y = y + vy;"
  + "  if (x <= 0) { x = 0; vx = 0 - vx; }"
  + "  if (x >= ARENA_W - BALL) { x = ARENA_W - BALL; vx = 0 - vx; }"
  + "  if (y <= 0) { y = 0; vy = 0 - vy; }"
  + "  if (y >= ARENA_H - BALL) { y = ARENA_H - BALL; vy = 0 - vy; }"
  + "  b.setAttribute('data-x', '' + x); b.setAttribute('data-y', '' + y);"
  + "  b.setAttribute('data-vx', '' + vx); b.setAttribute('data-vy', '' + vy);"
  + "  b.setStyleProp('left', x + 'px'); b.setStyleProp('top', y + 'px');"
  + "}"
  + "function stepPaddle() {"
  + "  const p = document.getElementById('paddle'); if (p === null) { return; }"
  + "  let bestY = 210; let bestX = 9999; const ids = ['b0','b1','b2']; let i = 0;"
  + "  while (i < 3) { const b = document.getElementById(ids[i]); if (b !== null) { const bx = numAttr(b,'data-x'); const by = numAttr(b,'data-y'); if (bx < bestX) { bestX = bx; bestY = by; } } i = i + 1; }"
  + "  let py = numPx(p, 'top', 165); const alvo = bestY - (PADDLE_H / 2) + (BALL / 2);"
  + "  if (py < alvo) { py = py + 6; } if (py > alvo) { py = py - 6; }"
  + "  if (py < 0) { py = 0; } if (py > ARENA_H - PADDLE_H) { py = ARENA_H - PADDLE_H; }"
  + "  p.setStyleProp('top', py + 'px');"
  + "  if (bestX <= 34 && bestY + BALL >= py && bestY <= py + PADDLE_H) {"
  + "    const hitEl = document.getElementById('placar');"
  + "    if (hitEl !== null) { let hv = parseInt(hitEl.textContent, 10); hitEl.setInnerHTML('' + (hv + 1)); }"
  + "  }"
  + "}"
  + "const fEl = document.getElementById('frames'); let fv = 0; if (fEl !== null) { fv = parseInt(fEl.textContent, 10); }"
  + "stepBall('b0'); stepBall('b1'); stepBall('b2'); stepPaddle();"
  + "if (fEl !== null) { fEl.setInnerHTML('' + (fv + 1)); }"
  + "return fv + 1;";

const step: any = new Function("__h", STEP_SRC);

const win = egui.openWindow("RTS Auto-Pong", VW, 560, 0);
while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break;
  step(d); // avanca a fisica um frame (muta o DOM da pagina)
  egui.beginFrame(win);
  egui.render(win, d);
  egui.endFrame(win);
}
egui.close(win);
dom.free(d);
