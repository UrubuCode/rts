// Query (getElementsBy*/querySelector por subárvore) + mutação (cloneNode) do DOM
// nativo — o diferencial "DOM em Rust sem jsdom".
//   target/release/rts.exe run examples/claude-dom-query-mutation.ts
import { io } from "rts";

const d = parseDocument(
  "<ul id='list'><li class='item'>A</li><li class='item'>B</li></ul><ul id='other'><li class='item'>X</li></ul>");

// getElementsByClassName / TagName (coleções)
io.print("itens (classe): " + d.getElementsByClassName("item").length);  // 3
io.print("<li> (tag): " + d.getElementsByTagName("li").length);          // 3

// querySelector POR SUBÁRVORE — restrito ao #list (não vê o #other)
const list = d.querySelector("#list");
if (list !== null) {
  io.print("itens dentro de #list: " + list.querySelectorAll(".item").length);  // 2

  // cloneNode(deep) — duplica um <li> com seu conteúdo, e anexa
  const first = list.querySelector(".item");
  if (first !== null) {
    const copy = first.cloneNode(true);
    io.print("clone textContent: '" + copy.textContent + "'");
  }
}
