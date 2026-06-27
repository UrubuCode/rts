// Dumpa a DisplayList do RTS em JSON — o lado RTS da comparação com o navegador.
// Roda headless (sem janela): lê pagina.html, computa o layout numa largura dada,
// imprime cada caixa/texto com x/y/w/h. Compare com o JSON do extrair-render.js.
//   target/release/rts.exe run examples/claude-dump-layout.ts
import dom from "rts:dom";
import { fs } from "rts";

// Defaults HTML (div/p/section/h1…) JÁ são block embutidos no motor (UA-stylesheet),
// e <row>/<tags> têm display:flex no CSS — então NÃO precisa de NENHUM defineBlock.
// O HTML+CSS é autônomo (como no navegador).

const html = fs.read_text("examples/pagina.html");
const d = dom.parseHtml(html);
// largura de viewport igual à da janela do navegador para a comparação bater.
// (passe o mesmo "viewport" que o extrair-render.js reportou.)
dom.dumpLayout(d, 1889);
dom.free(d);
