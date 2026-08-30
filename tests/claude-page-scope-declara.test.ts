// O que um <script> de pagina significa: as declaracoes de TOPO de um script
// ficam no objeto global do documento, e o script SEGUINTE ve-as.
//
// Nao e uma afirmacao sobre este motor — e o que a linguagem diz de script
// code (ECMA-262 §16.1.7): `var` e `function` de topo criam propriedades do
// global object, ao contrario de `let`/`const`, que ficam no Script Record.
// E o mecanismo inteiro de que um bundle depende: o script 1 publica `__d` e
// o script 30 chama-o.
//
// Escrito ANTES da mudanca que o faz passar, e a asserir VALORES — nunca que
// alguma substituicao aconteceu. E o gate 1 da regra 11 do README do
// rts-codegen.
import * as vm from "node:vm";

const escopo: any = {};
vm.createContext(escopo);

// ── `var` de topo atravessa ──────────────────────────────────────────────────
vm.runInContext("var b = 5;", escopo);
console.log("var visto pelo fragmento seguinte:", vm.runInContext("typeof b === 'number' ? b : 'ausente'", escopo));
console.log("var como propriedade do escopo:", escopo.b);

// ── `function` de topo atravessa — o caso do `__d` ───────────────────────────
vm.runInContext("function d(n) { return n * 2; }", escopo);
console.log("function chamada pelo fragmento seguinte:", vm.runInContext("typeof d === 'function' ? d(21) : 'ausente'", escopo));

// ── atribuicao livre em sloppy vai para ESTE documento, nao para o processo ──
// Sem isto dois documentos partilham a variavel, que e isolamento partido.
vm.runInContext("c = 7;", escopo);
console.log("atribuicao livre no escopo do documento:", escopo.c);

// ── `let`/`const` NAO atravessam: ficam no Script Record, por especificacao ──
vm.runInContext("let naoSai = 1; const tambemNao = 2;", escopo);
console.log("let nao vaza para o escopo:", escopo.naoSai);
console.log("const nao vaza para o escopo:", escopo.tambemNao);

// ── e um documento SEGUNDO nao ve nada do primeiro ───────────────────────────
const outro: any = {};
vm.createContext(outro);
console.log("outro documento nao ve o var:", vm.runInContext("typeof b", outro));
console.log("outro documento nao ve a atribuicao livre:", vm.runInContext("typeof c", outro));
