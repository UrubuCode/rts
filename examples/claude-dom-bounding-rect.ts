// getBoundingClientRect HEADLESS — o DOM nativo do RTS lê o LAYOUT que ele mesmo
// calcula e devolve x/y/width/height de cada elemento, SEM janela. É o que Node/Bun
// não fazem sem jsdom (e mesmo com jsdom não calculam layout real).
//
// SEM argumento (fiel ao MDN — o browser não recebe viewport por chamada, e a ABI
// `boundingRect(doc, node, which)` também não: o layout usa o viewport ATUAL do
// `Dom`, default 1280x800 headless). Antes desta correção o método pedia
// `getBoundingClientRect(984)` e chamava uma função (`dom.boundingComponent`) que o
// bridge nunca registou — lançava `TypeError` sempre, incluindo aqui.
//   target/release/rts.exe run examples/claude-dom-bounding-rect.ts
import { fs, io } from "rts";

// parseDocument/Element/getBoundingClientRect vêm da fachada DOM (prelude global).
const d = parseDocument(fs.read_text("examples/pagina.html"));

io.print("=== getBoundingClientRect (viewport default 1280x800, headless) ===");

// h1 — querySelector por tag (var direta é despachável no motor).
const h1 = d.querySelector("h1");
if (h1 !== null) {
  const r = h1.getBoundingClientRect();
  io.print("h1     x=" + r.x + " y=" + r.y + " w=" + r.width + " h=" + r.height);
}

// #hero — a caixa larga (width:100% border-box).
const hero = d.querySelector("#hero");
if (hero !== null) {
  const r = hero.getBoundingClientRect();
  io.print("#hero  x=" + r.x + " y=" + r.y + " w=" + r.width + " h=" + r.height);
  io.print("       top=" + r.top + " left=" + r.left + " right=" + r.right + " bottom=" + r.bottom);
}

io.print("");
io.print("Nota: comparar com o Chrome exige o MESMO viewport dos dois lados. `rts:dom`");
io.print("não expõe setViewport à fachada — scripts/parity/ força o Chrome a 1280x800");
io.print("(o default deste Dom) por Emulation.setDeviceMetricsOverride, não o inverso.");
