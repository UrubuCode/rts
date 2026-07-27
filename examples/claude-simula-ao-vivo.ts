// SIMULA UM USUÁRIO — e mostra na tela enquanto acontece.
//
// O mesmo roteiro de `claude-simula-usuario.ts`, só que com JANELA: entre uma
// ação e a próxima o programa desenha alguns frames, então dá para acompanhar o
// contador subindo, o texto sendo digitado letra a letra e o recibo aparecendo.
//
// O cursor simulado é desenhado como um marcador na página (o SO não move o
// mouse de verdade — quem se move é o cursor virtual do DOM).
//
//   target/release/rts.exe run examples/claude-simula-ao-vivo.ts
import egui from "rts:egui";
import dom from "rts:dom";
import { io, time } from "rts";

const VW = 820;
const VH = 640;

// Quantos frames desenhar entre uma ação e a próxima. Mais frames = mais devagar
// e mais fácil de acompanhar.
const FRAMES_POR_PASSO = 26;

const LOJA = `<html><head><style>
  body { background: #0f172a; color: #e2e8f0; padding: 14px; }
  h1 { color: #38bdf8; font-size: 24px; margin: 4px; }
  .painel { background: #1e293b; padding: 10px; margin: 6px; border: 1px solid #334155; }
  .item { display: block; background: #334155; color: #e2e8f0; padding: 9px; margin: 5px;
          border: 1px solid #475569; }
  .btn { display: block; background: #2563eb; color: #ffffff; padding: 9px; margin: 5px;
         border: 1px solid #3b82f6; }
  .val { color: #fbbf24; font-size: 20px; margin: 5px; }
  .txt { color: #94a3b8; font-size: 14px; margin: 5px; }
  .ok { color: #4ade80; font-size: 15px; margin: 5px; }
  input { background: #0b1220; color: #e2e8f0; padding: 6px; margin: 5px; }
  .narra { color: #f472b6; font-size: 15px; margin: 8px; }
</style></head><body>

<h1>Loja RTS — usuario simulado</h1>
<p id="narra" class="narra">(iniciando…)</p>

<div class="painel">
  <div class="item" id="p1">Teclado — R$ 200</div>
  <div class="item" id="p2">Monitor — R$ 900</div>
  <p id="carrinho" class="txt">carrinho vazio</p>
</div>

<div class="painel">
  <p id="qtd" class="val">0</p>
  <div class="btn" id="mais">+ uma unidade</div>
  <div class="btn" id="menos">- uma unidade</div>
</div>

<div class="painel">
  <input id="cupom" type="text" value="" />
  <div class="btn" id="aplicar">aplicar cupom</div>
  <p id="status" class="txt">-</p>
</div>

<div class="painel">
  <div class="btn" id="finalizar">finalizar compra</div>
  <p id="recibo" class="ok">-</p>
</div>

<script>
  let escolhido = '';
  let preco = 0;
  let qtd = 0;
  let desconto = 0;

  function mostraQtd() {
    const q = document.getElementById('qtd');
    if (q !== null) { q.setInnerHTML('' + qtd); }
  }

  function escolhe(id, nome, p) {
    const el = document.getElementById(id);
    if (el === null) { return; }
    el.addEventListener('click', function (e) {
      escolhido = nome; preco = p; qtd = 1;
      mostraQtd();
      const c = document.getElementById('carrinho');
      if (c !== null) { c.setInnerHTML('no carrinho: ' + nome); }
    });
  }

  escolhe('p1', 'Teclado', 200);
  escolhe('p2', 'Monitor', 900);

  const mais = document.getElementById('mais');
  if (mais !== null) {
    mais.addEventListener('click', function (e) {
      if (escolhido.length > 0) { qtd = qtd + 1; mostraQtd(); }
    });
  }

  const menos = document.getElementById('menos');
  if (menos !== null) {
    menos.addEventListener('click', function (e) {
      if (qtd > 0) { qtd = qtd - 1; mostraQtd(); }
    });
  }

  const aplicar = document.getElementById('aplicar');
  if (aplicar !== null) {
    aplicar.addEventListener('click', function (e) {
      const c = document.getElementById('cupom');
      const s = document.getElementById('status');
      if (c === null || s === null) { return; }
      const v = c.value;
      if (v === 'RTS20') { desconto = 20; s.setInnerHTML('cupom aplicado: 20% off'); }
      else if (v.length === 0) { s.setInnerHTML('digite um cupom'); }
      else { desconto = 0; s.setInnerHTML('cupom invalido: ' + v); }
    });
  }

  const fin = document.getElementById('finalizar');
  if (fin !== null) {
    fin.addEventListener('click', function (e) {
      const r = document.getElementById('recibo');
      if (r === null) { return; }
      if (escolhido.length === 0) { r.setInnerHTML('carrinho vazio'); return; }
      const bruto = preco * qtd;
      const liq = bruto - (bruto * desconto) / 100;
      r.setInnerHTML(qtd + 'x ' + escolhido + ' = R$ ' + liq);
    });
  }
</script>
</body></html>`;

const d = dom.parseHtml(LOJA);
const doc = new Document(d);
runScripts(doc);

const m = doc.mouse;
m.viewport(VW);
const kb = doc.keyboard;

const win = egui.openWindow("usuario simulado — rts-dom", VW, VH, 0);

let vivo = 1;

// Desenha N frames. É o que dá tempo de VER o efeito de cada ação.
// Devolve 0 se a janela foi fechada (o roteiro para).
function desenhar(n: number): number {
  let i = 0;
  while (i < n) {
    if (egui.isOpen(win) === 0) return 0;
    if (egui.pump(win) !== 0) return 0;
    egui.beginFrame(win);
    egui.render(win, d);
    egui.endFrame(win);
    i = i + 1;
  }
  return 1;
}

// Narra o passo NA PRÓPRIA PÁGINA e no terminal.
function narrar(texto: string): void {
  const n = doc.querySelector("#narra");
  if (n !== null) n.setInnerHTML(texto);
  io.print("» " + texto);
  if (desenhar(FRAMES_POR_PASSO) === 0) vivo = 0;
}

// Move o cursor virtual até o elemento e clica — a sequência de um usuário.
function clicarEm(sel: string): void {
  if (vivo === 0) return;
  const el = doc.querySelector(sel);
  if (el === null) {
    io.print("  [!] nao encontrei " + sel);
    return;
  }
  const r = el.getBoundingClientRect(VW);
  const cx = r.x + r.width / 2;
  const cy = r.y + r.height / 2;
  m.move(cx, cy);                        // cursor chega (dispara hover)
  if (desenhar(8) === 0) { vivo = 0; return; }
  m.click(cx, cy);                       // mousedown -> mouseup -> click
  if (desenhar(FRAMES_POR_PASSO) === 0) vivo = 0;
}

function conferir(oque: string, sel: string, esperado: string): number {
  const lido = doc.textOf(sel);
  if (lido === esperado) {
    io.print("  ok  " + oque + " -> " + lido);
    return 1;
  }
  io.print("  FALHOU  " + oque + " -> esperava '" + esperado + "', li '" + lido + "'");
  return 0;
}

let ok = 0;
let total = 0;

// ── roteiro ──────────────────────────────────────────────────────────────────
narrar("o usuario olha a vitrine…");

if (vivo !== 0) {
  narrar("escolhe o Monitor");
  clicarEm("#p2");
  total = total + 2;
  ok = ok + conferir("carrinho", "#carrinho", "no carrinho: Monitor");
  ok = ok + conferir("quantidade", "#qtd", "1");
}

if (vivo !== 0) {
  narrar("quer 3 unidades — clica duas vezes no +");
  clicarEm("#mais");
  clicarEm("#mais");
  total = total + 1;
  ok = ok + conferir("quantidade", "#qtd", "3");
}

if (vivo !== 0) {
  narrar("se arrepende de uma e volta pra 2");
  clicarEm("#menos");
  total = total + 1;
  ok = ok + conferir("quantidade", "#qtd", "2");
}

if (vivo !== 0) {
  narrar("digita um cupom que nao existe: PROMO");
  const campo = doc.querySelector("#cupom");
  if (campo !== null) {
    campo.focus();
    // letra a letra, desenhando entre elas — dá pra ver o texto aparecendo
    let i = 0;
    const palavra = "PROMO";
    while (i < palavra.length && vivo !== 0) {
      kb.press(palavra.charAt(i));
      if (desenhar(5) === 0) vivo = 0;
      i = i + 1;
    }
  }
  clicarEm("#aplicar");
  total = total + 1;
  ok = ok + conferir("status", "#status", "cupom invalido: PROMO");
}

if (vivo !== 0) {
  narrar("apaga tudo com Backspace e digita RTS20");
  const campo = doc.querySelector("#cupom");
  if (campo !== null) {
    campo.focus();
    let i = 0;
    while (i < 5 && vivo !== 0) {
      kb.press("Backspace");
      if (desenhar(5) === 0) vivo = 0;
      i = i + 1;
    }
    const certo = "RTS20";
    i = 0;
    while (i < certo.length && vivo !== 0) {
      kb.press(certo.charAt(i));
      if (desenhar(5) === 0) vivo = 0;
      i = i + 1;
    }
  }
  clicarEm("#aplicar");
  total = total + 1;
  ok = ok + conferir("status", "#status", "cupom aplicado: 20% off");
}

if (vivo !== 0) {
  narrar("finaliza a compra");
  clicarEm("#finalizar");
  total = total + 1;
  // 2 x 900 = 1800, menos 20% = 1440
  ok = ok + conferir("recibo", "#recibo", "2x Monitor = R$ 1440");
}

if (vivo !== 0) {
  narrar("pronto — " + ok + "/" + total + " verificacoes passaram");
  io.print("");
  io.print(ok === total
    ? "o usuario simulado completou a compra com sucesso."
    : "a pagina divergiu do esperado.");
  // deixa a janela aberta para inspecionar o resultado final
  io.print("(feche a janela para sair)");
  while (egui.isOpen(win) !== 0) {
    if (egui.pump(win) !== 0) break;
    egui.beginFrame(win);
    egui.render(win, d);
    egui.endFrame(win);
  }
}

dom.free(d);
egui.close(win);
