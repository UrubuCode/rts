// CONTROLAR a página inteiramente por TypeScript — sem janela, sem mouse real,
// sem browser externo. É o Puppeteer nativo: o script abre a página, acha os
// elementos, clica, digita e confere o resultado, tudo no processo.
//
//   target/release/rts.exe run examples/claude-dom-automacao.ts
import dom from "rts:dom";
import { io } from "rts";

const PAGINA = `<html><head><style>
  .btn { display: block; padding: 10px; margin: 6px; }
</style></head><body>
<h1>Loja</h1>

<p id="placar">0</p>
<div id="mais" class="btn">adicionar ao carrinho</div>
<div id="limpar" class="btn">esvaziar</div>

<input id="cupom" type="text" value="" />
<div id="aplicar" class="btn">aplicar cupom</div>
<p id="status">sem cupom</p>

<p>Veja os <a id="termos" href="/termos">termos de uso</a> antes de comprar.</p>
<p>Ou <a id="ajuda" href="/ajuda">peça ajuda</a> (este cancela a navegacao).</p>

<script>
  let itens = 0;

  const mais = document.getElementById('mais');
  if (mais !== null) {
    mais.addEventListener('click', function (e) {
      itens = itens + 1;
      const p = document.getElementById('placar');
      if (p !== null) { p.setInnerHTML('' + itens); }
    });
  }

  const limpar = document.getElementById('limpar');
  if (limpar !== null) {
    limpar.addEventListener('click', function (e) {
      itens = 0;
      const p = document.getElementById('placar');
      if (p !== null) { p.setInnerHTML('0'); }
    });
  }

  const aplicar = document.getElementById('aplicar');
  if (aplicar !== null) {
    aplicar.addEventListener('click', function (e) {
      const c = document.getElementById('cupom');
      const s = document.getElementById('status');
      if (c !== null && s !== null) {
        const v = c.value;
        if (v === 'RTS10') { s.setInnerHTML('cupom valido: 10% off'); }
        else if (v.length === 0) { s.setInnerHTML('digite um cupom'); }
        else { s.setInnerHTML('cupom invalido: ' + v); }
      }
    });
  }

  const ajuda = document.getElementById('ajuda');
  if (ajuda !== null) {
    ajuda.addEventListener('click', function (e) {
      e.preventDefault();
      const s = document.getElementById('status');
      if (s !== null) { s.setInnerHTML('ajuda abriria um chat (navegacao cancelada)'); }
    });
  }
</script>
</body></html>`;

const d = dom.parseHtml(PAGINA);
const doc = new Document(d);
runScripts(doc);

io.print("=== automacao: dirigindo a pagina por TS ===");
io.print("");

// ── 1. Clicar num botao POR SELETOR, 3 vezes ─────────────────────────────────
io.print("placar inicial       : " + doc.textOf("#placar"));
doc.click("#mais");
doc.click("#mais");
doc.click("#mais");
io.print("apos 3 cliques       : " + doc.textOf("#placar"));

doc.click("#limpar");
io.print("apos esvaziar        : " + doc.textOf("#placar"));
io.print("");

// ── 2. Preencher um campo e submeter ─────────────────────────────────────────
doc.type("#cupom", "RTS10");
io.print("campo cupom          : " + doc.valueOfField("#cupom"));
doc.click("#aplicar");
io.print("status apos aplicar  : " + doc.textOf("#status"));

// cupom errado
const campo = doc.querySelector("#cupom");
if (campo !== null) campo.clear();
doc.type("#cupom", "XXX");
doc.click("#aplicar");
io.print("status cupom errado  : " + doc.textOf("#status"));
io.print("");

// ── 3. Link com acao default vs cancelado ────────────────────────────────────
const href1 = doc.click("#termos");
io.print("clicar em #termos    : " + (href1.length > 0 ? "navegaria para " + href1 : "(sem navegacao)"));

const href2 = doc.click("#ajuda");
io.print("clicar em #ajuda     : " + (href2.length > 0 ? "navegaria para " + href2 : "(cancelado por preventDefault)"));
io.print("status apos ajuda    : " + doc.textOf("#status"));
io.print("");

// ── 4. Simulando o USUARIO: mouse e teclado por gesto ────────────────────────
io.print("=== agora simulando o usuario (gesto real) ===");
const m = doc.mouse;
m.viewport(900);

// posiciona o mouse sobre o botao pela geometria que o layout reporta
const btn = doc.querySelector("#mais");
if (btn !== null) {
  const r = btn.getBoundingClientRect(900);
  const cx = r.x + r.width / 2;
  const cy = r.y + r.height / 2;
  io.print("botao #mais em       : x=" + r.x + " y=" + r.y + " w=" + r.width + " h=" + r.height);
  m.move(cx, cy);
  const sob = m.elementUnder();
  io.print("elemento sob o mouse : " + (sob === null ? "nenhum" : sob.getAttribute("id")));
  // clique COMPLETO: move -> mousedown -> mouseup -> click
  m.click(cx, cy);
  m.click(cx, cy);
  io.print("placar apos 2 gestos : " + doc.textOf("#placar"));
}

// teclado: foca o campo e digita tecla a tecla
const c2 = doc.querySelector("#cupom");
if (c2 !== null) {
  c2.clear();
  c2.focus();
  const kb = doc.keyboard;
  kb.typeText("RTS10");
  io.print("digitado tecla a tecla: " + doc.valueOfField("#cupom"));
  kb.press("Backspace");
  io.print("apos um Backspace     : " + doc.valueOfField("#cupom"));
}

io.print("");
io.print("=== fim — nenhuma janela foi aberta ===");
dom.free(d);
