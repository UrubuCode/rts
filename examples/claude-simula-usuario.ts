// SIMULA UM USUÁRIO usando um site — por gesto, não por chamada de API.
//
// A diferença importa: `doc.click("#btn")` age direto no nó, como uma chamada de
// função. Um usuário move o cursor até o elemento, aperta, solta, e só então o
// clique existe — passando por hover, hit-test e a sequência real de eventos.
// Este script faz o segundo, que é o que exercita os mesmos caminhos que uma
// pessoa exercitaria.
//
// O roteiro é o de uma compra: olhar a página, escolher um produto, ajustar a
// quantidade, tentar um cupom errado, corrigir, e finalizar — CONFERINDO o que a
// página respondeu a cada passo, e parando se algo não bater.
//
//   target/release/rts.exe run examples/claude-simula-usuario.ts
import dom from "rts:dom";
import { io, time } from "rts";

const VW = 900;

// Pausa entre ações: um usuário não clica 5 vezes no mesmo milissegundo. Serve
// também para tornar o log legível quando se acompanha a execução.
const PAUSA = 120;

const LOJA = `<html><head><style>
  .btn { display: block; padding: 10px; margin: 6px; }
  .item { display: block; padding: 8px; margin: 4px; }
</style></head><body>
<h1>Loja RTS</h1>

<div class="item" id="p1">Teclado — R$ 200</div>
<div class="item" id="p2">Monitor — R$ 900</div>

<p id="carrinho">carrinho vazio</p>
<p id="qtd">0</p>
<div class="btn" id="mais">+</div>
<div class="btn" id="menos">-</div>

<input id="cupom" type="text" value="" />
<div class="btn" id="aplicar">aplicar cupom</div>
<p id="status">-</p>

<div class="btn" id="finalizar">finalizar compra</div>
<p id="recibo">-</p>

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
      escolhido = nome;
      preco = p;
      qtd = 1;
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

let passos = 0;
let falhas = 0;

// Move o cursor ATÉ o centro do elemento e clica — a sequência de um usuário.
// Devolve 0 se não achou o elemento (o "usuário" não encontrou o botão).
function clicarEm(sel: string): number {
  const el = doc.querySelector(sel);
  if (el === null) {
    io.print("  [!] nao encontrei " + sel + " na tela");
    return 0;
  }
  const r = el.getBoundingClientRect(VW);
  const cx = r.x + r.width / 2;
  const cy = r.y + r.height / 2;
  // primeiro o cursor chega (dispara hover), depois o clique.
  m.move(cx, cy);
  time.sleep_ms(PAUSA);
  m.click(cx, cy);
  time.sleep_ms(PAUSA);
  return 1;
}

// Confere o que a página respondeu. Um usuário olha a tela depois de agir.
function conferir(oque: string, sel: string, esperado: string): void {
  passos = passos + 1;
  const lido = doc.textOf(sel);
  if (lido === esperado) {
    io.print("  ok  " + oque + " -> " + lido);
  } else {
    falhas = falhas + 1;
    io.print("  FALHOU  " + oque + " -> esperava '" + esperado + "', li '" + lido + "'");
  }
}

io.print("=== usuario chega na loja ===");
io.print("carrinho: " + doc.textOf("#carrinho"));
io.print("");

io.print("=== escolhe o Monitor ===");
clicarEm("#p2");
conferir("carrinho", "#carrinho", "no carrinho: Monitor");
conferir("quantidade", "#qtd", "1");
io.print("");

io.print("=== aumenta para 3 unidades ===");
clicarEm("#mais");
clicarEm("#mais");
conferir("quantidade", "#qtd", "3");
io.print("");

io.print("=== se arrepende de uma ===");
clicarEm("#menos");
conferir("quantidade", "#qtd", "2");
io.print("");

io.print("=== tenta um cupom que nao existe ===");
const campo = doc.querySelector("#cupom");
if (campo !== null) {
  campo.focus();
  kb.typeText("PROMO");   // digita tecla a tecla, como uma pessoa
  time.sleep_ms(PAUSA);
}
io.print("  digitou: '" + doc.valueOfField("#cupom") + "'");
clicarEm("#aplicar");
conferir("status do cupom", "#status", "cupom invalido: PROMO");
io.print("");

io.print("=== apaga e digita o cupom certo ===");
if (campo !== null) {
  campo.focus();
  // apaga tecla a tecla, como uma pessoa faria
  let i = 0;
  while (i < 5) { kb.press("Backspace"); i = i + 1; }
  time.sleep_ms(PAUSA);
  io.print("  apos apagar: '" + doc.valueOfField("#cupom") + "'");
  kb.typeText("RTS20");
  time.sleep_ms(PAUSA);
}
io.print("  digitou: '" + doc.valueOfField("#cupom") + "'");
clicarEm("#aplicar");
conferir("status do cupom", "#status", "cupom aplicado: 20% off");
io.print("");

io.print("=== finaliza a compra ===");
clicarEm("#finalizar");
// 2 monitores a 900 = 1800, menos 20% = 1440
conferir("recibo", "#recibo", "2x Monitor = R$ 1440");
io.print("");

io.print("=== fim do roteiro ===");
io.print(passos + " verificacoes, " + falhas + " falha(s)");
if (falhas === 0) {
  io.print("o usuario simulado completou a compra com sucesso.");
} else {
  io.print("a pagina nao se comportou como esperado.");
}

dom.free(d);
