// Prova da tag <style>: CSS de autor com seletores tag/.class/#id, resolvido por
// especificidade na cascade (id > classe > tag), 100% HEADLESS (rts:dom, sem
// janela) — o diferencial do RTS sobre Node/Bun (que não têm DOM nativo).
import dom from "rts:dom";
import { print } from "rts:io";

// HTML+CSS puro: NENHUM defineStyle imperativo — o estilo vem do <style>.
const html =
  "<style>" +
  "  p { color:#ff0000; font-size:14 }" +      // tag
  "  .hl { color:#00ff00; padding:10 }" +       // classe (vence tag na cor)
  "  #destaque { color:#0000ff }" +             // id (vence tudo)
  "</style>" +
  "<p>normal</p>" +
  "<p class='hl'>classe</p>" +
  "<p id='destaque' class='hl'>id</p>";

const d = dom.parseHtml(html);

// slots: 0=color 2=font-size 3=padding (mesmos do style.rs).
function colorOf(sel: string): i64 {
  const node = dom.querySelector(d, sel);
  return dom.nodeStyleSlot(d, node, 0); // SLOT_COLOR
}

print("=== cascade da tag <style> (id > classe > tag) ===");
print("p normal       color=" + hex(colorOf("p")));            // #ff0000 (tag)
print("p.hl           color=" + hex(colorOf(".hl")));          // #00ff00 (classe)
print("p#destaque     color=" + hex(colorOf("#destaque")));    // #0000ff (id)

// font-size: só a tag define; a classe HERDA (cascade real).
const hl = dom.querySelector(d, ".hl");
print(".hl font-size  = " + dom.nodeStyleSlot(d, hl, 2));      // 14 (herda da tag p)
print(".hl padding    = " + dom.nodeStyleSlot(d, hl, 3));      // 10 (só a classe)

// !important (MDN estágio 1): a regra de TAG com !important vence o inline normal.
const imp = dom.parseHtml(
  "<style>.c { color:#ff0000 !important }</style>" +
  "<div class='c' style='color:#0000ff'>x</div>");
const dv = dom.querySelector(imp, ".c");
print("");
print("=== !important inverte a precedência de origem ===");
print(".c (<style>!important vence inline) color=" + hex(dom.nodeStyleSlot(imp, dv, 0)));

function hex(v: i64): string {
  if (v < 0) return "(nenhum)";
  return "0x" + v.toString(16).toUpperCase();
}
