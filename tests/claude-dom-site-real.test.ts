import { describe, test, expect } from "rts:test";

// Uma página com os padrões que um SITE REAL usa. Se isto funciona ponta a
// ponta, o DOM está utilizável para sites — se não, o que falhar é o gap.

const SITE = `<html><head><style>
  .nav a { color: #06c; }
  .card { display: block; padding: 10px; }
  .btn { display: block; padding: 8px; }
  .oculto { display: none; }
</style></head><body>
<nav class="nav">
  <a href="/" id="home">Inicio</a>
  <a href="/produtos" id="prod">Produtos</a>
  <a href="#" id="menu">Menu</a>
</nav>

<div class="card" id="card1" data-id="42" data-nome="Camiseta">
  <h3>Camiseta</h3>
  <p class="preco">R$ 59,90</p>
  <button class="btn add" data-id="42">comprar</button>
</div>

<div class="card" id="card2" data-id="43" data-nome="Boné">
  <h3>Bone</h3>
  <p class="preco">R$ 29,90</p>
  <button class="btn add" data-id="43">comprar</button>
</div>

<div id="painel" class="oculto">painel do menu</div>
<p id="carrinho">0 itens</p>

<form id="busca" action="/buscar">
  <input id="q" type="text" value="" placeholder="buscar" />
  <select id="cat">
    <option value="todos" selected>Todos</option>
    <option value="roupas">Roupas</option>
  </select>
  <input id="promo" type="checkbox" name="promo" />
  <button type="submit" class="btn">buscar</button>
</form>
<p id="resultado">-</p>

<script>
  let itens = 0;

  // padrão 1: DELEGAÇÃO de evento — um listener no container, não um por botão.
  // É como toda lista dinâmica de site funciona.
  const cards = document.querySelectorAll('.add');
  let i = 0;
  while (i < cards.length) {
    const b = cards[i];
    b.addEventListener('click', function (e) {
      itens = itens + 1;
      const c = document.getElementById('carrinho');
      if (c !== null) { c.setInnerHTML(itens + ' itens'); }
    });
    i = i + 1;
  }

  // padrão 2: toggle de visibilidade com preventDefault num <a href="#">
  const menu = document.getElementById('menu');
  if (menu !== null) {
    menu.addEventListener('click', function (e) {
      e.preventDefault();
      const p = document.getElementById('painel');
      if (p !== null) {
        const cls = p.getAttribute('class');
        p.setAttribute('class', cls === 'oculto' ? '' : 'oculto');
      }
    });
  }

  // padrão 3: submit interceptado (SPA) lendo os campos
  const f = document.getElementById('busca');
  if (f !== null) {
    f.addEventListener('submit', function (e) {
      e.preventDefault();
      const q = document.getElementById('q');
      const cat = document.getElementById('cat');
      const promo = document.getElementById('promo');
      const r = document.getElementById('resultado');
      if (q !== null && cat !== null && promo !== null && r !== null) {
        r.setInnerHTML('busca: ' + q.value + ' / ' + cat.value
          + ' / promo=' + (promo.checked ? 'sim' : 'nao'));
      }
    });
  }
</script>
</body></html>`;

const doc = parseDocument(SITE);
runScripts(doc);

// Cada padrao abaixo e algo que TODO site usa. Se um quebrar, o DOM deixou de
// servir para sites — e o teste diz qual.

const navProd = doc.click("#prod");
const navMenu = doc.click("#menu");
const painel = doc.querySelector("#painel");
const painelClasse = painel === null ? "?" : painel.getAttribute("class");

const botoes = doc.querySelectorAll(".add");
const nBotoes = botoes.length;
doc.click(".add");
const carrinho1 = doc.textOf("#carrinho");
if (nBotoes > 1) botoes[1].click();
const carrinho2 = doc.textOf("#carrinho");

const c1 = doc.querySelector("#card1");
const dataId = c1 === null ? "?" : c1.getAttribute("data-id");
const dataNome = c1 === null ? "?" : c1.getAttribute("data-nome");

doc.type("#q", "camiseta");
const promo = doc.querySelector("#promo");
if (promo !== null) promo.click();
const promoMarcado = promo === null ? false : promo.checked;
const acaoSubmit = doc.click("form#busca button");
const resultado = doc.textOf("#resultado");

const temPreco = doc.querySelector(".preco") !== null;
const nCards = doc.querySelectorAll(".card").length;
const temNavA = doc.querySelector("nav a") !== null;
const temDataAttr = doc.querySelector("[data-id]") !== null;

describe("site real: navegacao", () => {
  test("link comum navega", () => expect(navProd).toBe("/produtos"));
  test("link href=# nao navega", () => expect(navMenu).toBe(""));
  test("preventDefault revelou o painel", () => expect(painelClasse).toBe(""));
});

describe("site real: lista dinamica", () => {
  test("achou os dois botoes", () => expect(nBotoes).toBe(2));
  test("primeiro comprar", () => expect(carrinho1).toBe("1 itens"));
  test("segundo comprar (outro no, mesmo handler)", () => expect(carrinho2).toBe("2 itens"));
});

describe("site real: data-attributes", () => {
  test("data-id", () => expect(dataId).toBe("42"));
  test("data-nome", () => expect(dataNome).toBe("Camiseta"));
});

describe("site real: formulario SPA", () => {
  test("checkbox marca ao clicar", () => expect(promoMarcado).toBe(true));
  test("submit cancelado pelo handler", () => expect(acaoSubmit).toBe(""));
  test("handler leu input, select e checkbox", () =>
    expect(resultado).toBe("busca: camiseta / todos / promo=sim"));
});

describe("site real: seletores CSS", () => {
  test("por classe", () => expect(temPreco).toBe(true));
  test("querySelectorAll conta certo", () => expect(nCards).toBe(2));
  test("descendente 'nav a'", () => expect(temNavA).toBe(true));
  test("atributo [data-id]", () => expect(temDataAttr).toBe(true));
});
