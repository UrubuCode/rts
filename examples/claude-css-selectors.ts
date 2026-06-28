// Seletores CSS compostos + combinadores + atributo + pseudo (#1752), TUDO à mão
// (0 deps — avaliamos lightningcss/swc_css e decidimos não usar). Paridade Chrome.
//   target/release/rts.exe run examples/claude-css-selectors.ts
import dom from "rts:dom";
import { io } from "rts";

const d = dom.parseHtml(
  "<nav id='menu'>" +
  "  <a href='/home' class='link active'>Home</a>" +
  "  <a href='https://ext.com' class='link'>Externo</a>" +
  "  <span>sep</span>" +
  "  <a href='/sobre' class='link'>Sobre</a>" +
  "</nav>");

const root = dom.rootId(d);
io.print("a.link.active (composto): " + dom.queryAllWithinCount(d, root, "a.link.active"));   // 1
io.print("#menu > a (filho direto): " + dom.queryAllWithinCount(d, root, "#menu > a"));        // 3
io.print("[href^='https'] (externos): " + dom.queryAllWithinCount(d, root, "[href^='https']")); // 1
io.print("span + a (link após sep): " + dom.queryAllWithinCount(d, root, "span + a"));          // 1
io.print("a:first-child (1º link): " + dom.queryAllWithinCount(d, root, "a:first-child"));      // 1
io.print("a:nth-child(odd): " + dom.queryAllWithinCount(d, root, "a:nth-child(odd)"));          // 2
