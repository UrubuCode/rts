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
// O QUE JA ESTA ESTABELECIDO, medido, para quem retomar nao repetir:
//
// As tres variantes que discriminam:
//
//     try + DECLARACAO + for      -> PERDE   (este teste)
//     try + DECLARACAO, sem for   -> ok
//     try + ATRIBUICAO  + for     -> ok      (`var d;` fora, `d = c` dentro)
//     for + declaracao, sem try   -> ok
//
// Logo sao precisas TRES coisas ao mesmo tempo: o `try`, o laco, e a
// DECLARACAO (nao a atribuicao) la dentro.
//
// O sitio: `emit::protect` faz `scope.restore(&before)` ao chegar ao join do
// `try` (protect.rs, a seguir a `switch_to(join)`), e explica porque isso e
// sound: *"everything the body assigns lives in memory by now —
// `capture::assigned_under_protection` put it there"*. Tudo o que ficou num
// REGISTO e descartado nesse restauro.
//
// DUAS HIPOTESES JA REFUTADAS, com medicao:
//
//  1. "`assigned_under_protection` nao conta declaracoes, so atribuicoes."
//     Verdade — mas acrescentar `vars_at_any_depth` ao ramo do `Try` faz o nome
//     ficar residente (medido: `protected` deixa de ser vazio) e o valor
//     CONTINUA a perder-se. Revertido.
//
//  2. "`assigned_in_stmt` (o plano do laco) nao ve declaracoes."
//     Falso: ve, e o comentario dele ate diz porque — *"the names it introduces
//     count as written... a `var` inside a protected region is visible after
//     it"*.
//
// O que falta e perceber PORQUE a residencia nao chega: ou a escrita vai para
// um ambiente e a leitura le de outro, ou o valor nao chega ao bloco de saida
// do laco. Uma sonda em `binding::read` para o nome nao disparou, o que quer
// dizer que a leitura nao passa por onde se esperava — e e por ai que se
// comeca.

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
