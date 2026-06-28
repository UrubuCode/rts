// getBoundingClientRect HEADLESS — o DOM nativo do RTS lê o LAYOUT que ele mesmo
// calcula e devolve x/y/width/height de cada elemento, SEM janela. É o que Node/Bun
// não fazem sem jsdom (e mesmo com jsdom não calculam layout real).
//   target/release/rts.exe run examples/claude-dom-bounding-rect.ts
import { fs, io } from "rts";

// parseDocument/Element/getBoundingClientRect vêm da fachada DOM (prelude global).
const d = parseDocument(fs.read_text("examples/pagina.html"));

io.print("=== getBoundingClientRect (viewport 984, headless) ===");

// h1 — querySelector por tag (var direta é despachável no motor).
const h1 = d.querySelector("h1");
if (h1 !== null) {
  const r = h1.getBoundingClientRect(984);
  io.print("h1     x=" + r.x + " y=" + r.y + " w=" + r.width + " h=" + r.height);
}

// #hero — a caixa larga (width:100% border-box). x/w batem com o Chrome (20/944).
const hero = d.querySelector("#hero");
if (hero !== null) {
  const r = hero.getBoundingClientRect(984);
  io.print("#hero  x=" + r.x + " y=" + r.y + " w=" + r.width + " h=" + r.height);
  io.print("       top=" + r.top + " left=" + r.left + " right=" + r.right + " bottom=" + r.bottom);
}

io.print("");
io.print("Comparado com o Chrome (mesma pagina.html, getBoundingClientRect):");
io.print("  h1    Chrome x=20 y=40.1 w=944  | #hero Chrome x=20 w=944");
io.print("  x/width batem ao pixel; y difere ~10px (margin-collapse pai/filho, pendente).");
