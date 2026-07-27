// PLAYGROUND interativo do rts-dom — uma página com botões, links, campos e
// contadores, para clicar e ver o motor reagindo AO VIVO.
//
// Tudo o que acontece ao clicar é decidido por JS DA PRÓPRIA PÁGINA (os
// `<script>` abaixo registram `addEventListener`), exatamente como num browser:
// o egui só reporta "clicou no ponto (x,y)", o rts-dom faz o hit-test, despacha
// o evento com bubbling e executa a ação default se ninguém cancelar.
//
//   target/release/rts.exe run examples/claude-dom-playground.ts
import egui from "rts:egui";
import dom from "rts:dom";
import input from "rts:input";
import { io } from "rts";

const VW = 900;
const VH = 700;

const PAGINA = `<html><head><style>
  body { background: #0f172a; color: #e2e8f0; padding: 16px; }
  h1 { color: #38bdf8; font-size: 26px; margin: 4px; }
  h2 { color: #a78bfa; font-size: 17px; margin: 10px; }
  .card { background: #1e293b; padding: 12px; margin: 8px; border: 1px solid #334155; }
  .btn { display: block; background: #2563eb; color: #ffffff; padding: 10px;
         margin: 6px; border: 1px solid #3b82f6; }
  .btn-verde { display: block; background: #16a34a; color: #ffffff; padding: 10px;
               margin: 6px; border: 1px solid #22c55e; }
  .btn-vermelho { display: block; background: #dc2626; color: #ffffff; padding: 10px;
                  margin: 6px; border: 1px solid #ef4444; }
  .placar { color: #fbbf24; font-size: 22px; margin: 8px; }
  .saida { color: #94a3b8; font-size: 14px; margin: 6px; }
  .link { color: #38bdf8; }
  input { background: #0b1220; color: #e2e8f0; padding: 6px; margin: 6px; }
  .rodape { color: #64748b; font-size: 12px; margin: 10px; }
</style></head><body>

<h1>rts-dom — playground interativo</h1>
<p class="saida">Clique nos botoes. Tudo abaixo e decidido por JS da propria pagina.</p>

<div class="card">
  <h2>1. Contador (listener de click)</h2>
  <p id="placar" class="placar">0</p>
  <div id="mais" class="btn-verde">+1  (clique aqui)</div>
  <div id="menos" class="btn-vermelho">-1  (clique aqui)</div>
  <div id="zerar" class="btn">zerar</div>
</div>

<div class="card">
  <h2>2. Campo de texto (digite depois de clicar nele)</h2>
  <input id="nome" type="text" value="" />
  <div id="saudar" class="btn">saudar</div>
  <p id="saudacao" class="saida">(clique no campo, digite, e aperte saudar)</p>
</div>

<div class="card">
  <h2>3. Bubbling — o clique sobe ate o pai</h2>
  <div id="pai" class="btn">
    <span id="filho">clique NESTE texto (o pai recebe o evento)</span>
  </div>
  <p id="log-bubble" class="saida">-</p>
</div>

<div class="card">
  <h2>4. Link com acao default vs preventDefault</h2>
  <p class="saida">Texto normal com <a id="lnk" href="/destino" class="link">um link de verdade</a> no meio da linha.</p>
  <p class="saida">E este <a id="lnk-cancel" href="/nao-vai" class="link">link cancelado por preventDefault</a>.</p>
  <p id="log-link" class="saida">-</p>
</div>

<p class="rodape">Feche a janela para sair. O terminal mostra cada navegacao.</p>

<script>
  let n = 0;

  function setPlacar() {
    const p = document.getElementById('placar');
    if (p !== null) { p.setInnerHTML('' + n); }
  }

  const mais = document.getElementById('mais');
  if (mais !== null) {
    mais.addEventListener('click', function (e) { n = n + 1; setPlacar(); });
  }

  const menos = document.getElementById('menos');
  if (menos !== null) {
    menos.addEventListener('click', function (e) { n = n - 1; setPlacar(); });
  }

  const zerar = document.getElementById('zerar');
  if (zerar !== null) {
    zerar.addEventListener('click', function (e) { n = 0; setPlacar(); });
  }

  const saudar = document.getElementById('saudar');
  if (saudar !== null) {
    saudar.addEventListener('click', function (e) {
      const campo = document.getElementById('nome');
      const alvo = document.getElementById('saudacao');
      if (campo !== null && alvo !== null) {
        const v = campo.value;
        alvo.setInnerHTML(v.length === 0 ? '(campo vazio)' : 'Ola, ' + v + '!');
      }
    });
  }

  // Bubbling: o listener esta no PAI, mas o clique acontece no FILHO.
  const pai = document.getElementById('pai');
  if (pai !== null) {
    pai.addEventListener('click', function (e) {
      const l = document.getElementById('log-bubble');
      if (l !== null) { l.setInnerHTML('o clique subiu do filho ate o pai'); }
    });
  }

  // preventDefault: o listener roda, mas a navegacao NAO acontece.
  const cancel = document.getElementById('lnk-cancel');
  if (cancel !== null) {
    cancel.addEventListener('click', function (e) {
      e.preventDefault();
      const l = document.getElementById('log-link');
      if (l !== null) { l.setInnerHTML('preventDefault() chamado — nao navegou'); }
    });
  }
</script>
</body></html>`;

const d = dom.parseHtml(PAGINA);
const docF = new Document(d);
runScripts(docF);

const win = egui.openWindow("rts-dom playground", VW, VH, 0);
io.print("[playground] janela aberta — clique nos botoes da pagina");

while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break;

  const mx = input.mouseX(win);
  const my = input.mouseY(win);
  const clicou = input.mouseClicked(win, 0);

  // Href da AÇÃO DEFAULT: resolvido no clique, aplicado depois do pump (a ordem
  // do browser — o listener roda primeiro e pode cancelar).
  let href = "";
  if (clicou !== 0) {
    const alvo = dom.nodeAt(d, VW, mx, my);
    if (alvo !== -1) {
      // foca o campo se o clique caiu num <input>
      const campo = dom.inputAt(d, VW, mx, my);
      dom.focusInput(d, campo);
      const ancora = dom.closest(d, alvo, "a");
      if (ancora !== -1) {
        href = dom.getAttribute(d, ancora, "href");
      }
    }
  }

  // Digitação no campo focado.
  const digitado = input.textInput(win);
  if (digitado.length > 0) dom.inputFeedText(d, digitado);
  if (input.key(win, 4, 1) !== 0) dom.inputBackspace(d);

  egui.beginFrame(win);
  egui.render(win, d);
  egui.endFrame(win);

  // Despacha os cliques do frame aos listeners da página; devolve 1 se algum
  // chamou preventDefault().
  const cancelou = pumpEventCallbacksCancelable(docF);

  if (href.length > 0) {
    if (cancelou !== 0) {
      io.print("[link] " + href + " — CANCELADO por preventDefault()");
    } else {
      io.print("[link] navegaria para: " + href);
      const alvo = docF.querySelector("#log-link");
      if (alvo !== null) alvo.setInnerHTML("acao default: navegaria para " + href);
    }
  }
}

io.print("[playground] janela fechada");
dom.free(d);
egui.close(win);
