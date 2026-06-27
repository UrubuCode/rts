import egui from "rts:egui";
import dom from "rts:dom";

// PÁGINA HTML+CSS PURO via a tag <style> (cascade fiel à MDN: tag < .class < #id,
// + !important). O ESTILO todo vem do <style> — nada de dom.defineStyle imperativo
// para cor/caixa. Só o LAYOUT (display: vertical/horizontal) ainda é defineBlock,
// porque `display` não é uma propriedade que o parser de estilo cobre ainda.
//   target/release/rts.exe run examples/claude-egui-style-pagina.ts

// display: 0=vertical 1=wrap(parágrafo) 2=horizontal. (layout, não estilo)
dom.defineBlock("body", 0, 0, 0, 0);
dom.defineBlock("h1", 0, 0, 0, 8);
dom.defineBlock("h2", 0, 0, 0, 6);
dom.defineBlock("p", 1, 0, 0, 0);
dom.defineBlock("div", 0, 0, 0, 6);
dom.defineBlock("section", 0, 0, 0, 10);
dom.defineBlock("header", 0, 0, 0, 8);
dom.defineBlock("footer", 0, 0, 0, 8);
dom.defineBlock("row", 2, 0, 0, 8);   // <row> = faixa horizontal (flex-row: lado a lado, encolhe)
dom.defineBlock("tags", 1, 0, 0, 8);  // <tags> = wrap (inline-block: flui lado a lado E QUEBRA linha)

// ───────────────────────────────────────────────────────────────────────────────
// A página: TUDO estilizado por CSS de autor no <style>. Mostra:
//  • seletor de TAG  (body, h1, h2, p)
//  • seletor de CLASSE (.card, .badge, .muted, .stat, .ok, .warn)
//  • seletor de ID   (#hero vence a classe)
//  • !important       (.locked força a cor mesmo com style="" inline tentando mudar)
//  • várias coisas por linha (<row> horizontal com 3 .card e badges)
const html =
  "<style>" +
  "  body { background:#0f1420; color:#d8dee9; padding:18 }" +
  "  h1 { color:#88ccff; font-size:30 }" +
  "  h2 { color:#7aa2d0; font-size:20 }" +
  "  p  { color:#b8c2d0; font-size:15 }" +
  "  .muted { color:#6b7686; font-size:13 }" +
  // cards: caixa com fundo, borda, raio, padding, largura — tudo CSS de autor.
  "  .card { background:#1a2030; border-width:2; border-color:#2e3a52; border-radius:10; padding:14; width:30% }" +
  "  .stat { color:#9fb3c8; font-size:14 }" +
  "  .num  { color:#ffffff; font-size:26 }" +
  "  .ok   { color:#7ee787 }" +              // classe verde
  "  .warn { color:#ffb454 }" +              // classe laranja
  "  .badge { background:#223052; color:#9ecbff; padding:6; border-radius:6; font-size:13 }" +
  // ID vence a CLASSE: #hero é um .card, mas o id sobrescreve fundo+borda.
  "  #hero { background:#142a44; border-color:#3a6ea5; width:100% }" +
  // !important: vence o style="" inline normal do nó .locked logo abaixo.
  "  .locked { color:#ff6b6b !important }" +
  "</style>" +

  "<header>" +
  "  <h1>Painel do Usuario</h1>" +
  "  <p class='muted'>HTML + CSS puro renderizado pelo motor do RTS (DOM headless + egui so pinta)</p>" +
  "</header>" +

  // #hero é .card MAS o #id sobrescreve fundo/borda/largura (especificidade 100>10).
  "<section id='hero' class='card'>" +
  "  <h2>Marcos</h2>" +
  "  <p>Engenheiro de compiladores. Construindo um motor de DOM nativo onde Node e Bun nao tem.</p>" +
  // !important: o inline tenta cinza, mas .locked{color !important} forca vermelho.
  "  <p class='locked' style='color:#888888'>Status: bloqueado (cor forcada por !important, o inline cinza perde)</p>" +
  "</section>" +

  // <row> horizontal: 3 cards lado a lado (varias coisas por linha), cada um .card.
  "<row>" +
  "  <div class='card'><p class='num'>128</p><p class='stat ok'>projetos ativos</p></div>" +
  "  <div class='card'><p class='num'>4.2k</p><p class='stat'>commits no mes</p></div>" +
  "  <div class='card'><p class='num'>7</p><p class='stat warn'>PRs aguardando</p></div>" +
  "</row>" +

  "<section>" +
  "  <h2>Tags</h2>" +
  "  <tags>" +
  "    <p class='badge'>rust</p><p class='badge'>cranelift</p><p class='badge'>typescript</p>" +
  "    <p class='badge'>egui</p><p class='badge'>cranelift</p><p class='badge'>wgpu</p>" +
  "    <p class='badge'>winit</p><p class='badge'>tokio</p><p class='badge'>serde</p>" +
  "  </tags>" +
  "</section>" +

  "<footer>" +
  "  <p class='muted'>cascade: tag &lt; .classe &lt; #id &lt; inline &lt; !important — fiel a MDN</p>" +
  "</footer>";

const d = dom.parseHtml(html);

const win = egui.openWindow("RTS — pagina HTML+CSS via <style>", 760, 720, 0);

while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break;
  egui.beginFrame(win);
  egui.render(win, d);
  egui.endFrame(win);
}

egui.close(win);
dom.free(d);
