import { test, expect } from "rts:test";

// As tres coisas que separam um DOM onde o React anda de um onde o Preact anda.
//
// Nenhuma delas e um capricho de conformidade: cada uma foi encontrada com o
// Preact 10.24.3 real do CDN a montar e a nao responder, e o codigo dele que a
// exige esta citado ao lado. As tres falhavam em SILENCIO — a aplicacao montava,
// pintava, e nada dizia porque nao reagia.

const doc = parseDocument("<div id='d'>antes</div>");
const el: any = doc.getElementById("d");

// 1. `'onclick' in el` decide o NOME do evento que o Preact regista:
//
//        l = l.toLowerCase() in n ? l.toLowerCase().slice(2) : l.slice(2)
//
// Com `false`, ele regista "Click" em vez de "click" — um tipo que nada
// despacha, e o registo em si corre bem.
test("um elemento tem os on<evento> como propriedade", function () {
  expect("onclick" in el).toBe(true);
  expect("oninput" in el).toBe(true);
});

// 2. `this` num ouvinte e o no. O Preact regista UM despachante por tipo:
//
//        function eventProxy(e) { return this.l[e.type + false](e) }
//
// e a tabela `l` mora no no. Com `this` a valer `undefined`, nenhum `onClick`
// de nenhum componente dispara.
test("um ouvinte corre com `this` ligado ao no", function () {
  let recebido: any = null;
  el.addEventListener("click", function (this: any) { recebido = this; });
  el.dispatchEvent("click");
  expect(recebido === el).toBe(true);
});

// 3. `no.data` e o texto de um Text, que e como o Preact o actualiza:
//
//        if (null === x) m === k || (c && n.data === k) || (n.data = k)
//
// O React escreve `nodeValue`. Sem `data` a atribuicao criava um campo no
// wrapper e o texto ficava parado: o que mudava de ESTRUTURA reconciliava e o
// que mudava so de TEXTO nao.
test("o texto de um no le-se e escreve-se por `data`", function () {
  const t: any = el.firstChild;
  expect(t.data).toBe("antes");
  t.data = "depois";
  expect(el.textContent).toBe("depois");
  expect(t.nodeValue).toBe("depois");
});
