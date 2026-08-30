import { test, expect } from "rts:test";

// Um `var` declarado dentro de um `for` que esta dentro de um `try` sobrevive
// ao fim do laco: e uma ligacao da FUNCAO, e o `break` nao a pode perder.
//
// FIXTURE VERMELHA: pina um defeito ABERTO.
//
// Isolado a quatro variantes, e sao precisas DUAS condicoes — nenhuma sozinha
// falha:
//
//     try + for            -> PERDE      (este teste)
//     try, sem for         -> ok
//     for, sem try         -> ok
//     try + rotulo + for   -> PERDE      (o rotulo e irrelevante)
//
// Veio do React 18, que anda a arvore de fibers exatamente assim:
//
//     try {
//       a: { for (var c = no.return; c !== null;) {
//              if (serve(c)) { var d = c; break a }
//              c = c.return }
//            throw Error(160); }
//       switch (d.tag) { ... }
//     } catch (k) { ... }
//
// O `d` chegava `undefined` ao `switch`, e o erro que o utilizador via era
// `Cannot read properties of undefined (reading 'tag')` — dentro do commit da
// arvore, sem nada que apontasse para a causa.
//
// O que ja se sabe da causa: sem o `try`, o `var` e um registo local e o
// `break` preserva-o; com o `try`, o nome precisa de viver no ambiente para
// sobreviver ao salto — e `assigned_under_protection` so conta ATRIBUICOES,
// nao declaracoes com inicializador, por isso nunca o torna residente.
// Contar tambem as declaracoes torna-o residente e NAO chega: medido, o valor
// continua a perder-se, o que aponta para o ambiente por passagem que o `for`
// abre. Fica escrito para quem retomar nao repetir a tentativa.

const html =
  "<div id='saida'>-</div><script>" +
  "function acha(v) {" +
  "  try {" +
  "    for (var c = v; c !== null;) {" +
  "      if (c.serve) { var d = c; break; }" +
  "      c = c.pai;" +
  "    }" +
  "    return d === undefined ? 'PERDEU' : d.nome;" +
  "  } catch (k) { return 'lancou'; }" +
  "}" +
  "var raiz = { nome: 'raiz', serve: true, pai: null };" +
  "var el = document.getElementById('saida');" +
  "if (el !== null) { el.setInnerHTML('' + acha(raiz)); }" +
  "</script>";

const doc = parseDocument(html);
runScripts(doc);
const saida = doc.getElementById("saida");
const texto = saida === null ? "(sem no)" : saida.textContent;

test("um var declarado num for dentro de um try sobrevive ao break", function () {
  expect(texto).toBe("raiz");
});
