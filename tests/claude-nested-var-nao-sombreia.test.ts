import { test, expect } from "rts:test";

// Um `var` DENTRO de uma funcao aninhada e dessa funcao, e de mais ninguem.
// Nao pode mudar o que o mesmo nome significa na funcao de fora — e muito menos
// numa linha que corre ANTES dela.
//
// A analise de captura junta duas sobre-aproximacoes: conta como "mencionado
// numa funcao aninhada" tudo o que essa funcao escreve (incluindo o que ela
// propria declara), e conta o parametro de um `catch` como declarado pela
// funcao de fora. Cada uma sozinha e inofensiva. Juntas dao a essa funcao uma
// ligacao local vazia com o nome de uma que existe la fora, e a chamada morre.
//
// Medido num bundle real (WhatsApp Web) antes de existir este teste: um script
// morria em `TypeError: e is not a function` por causa exatamente disto.
//
// Escrito ANTES da correcao e a asserir VALORES.

function alvo(x: number): number { return x + 1; }

test("um var numa funcao aninhada nao sombreia a funcao de fora", function () {
  function usa(v: number): number {
    try { } catch (alvo) { }
    const r = alvo(41);
    const aninhada = function () { var alvo; return alvo; };
    return r + (v - v);
  }
  expect(usa(0)).toBe(42);
});

test("nem quando a chamada vem depois da funcao aninhada", function () {
  function usa(): number {
    try { } catch (alvo) { }
    const aninhada = function () { var alvo; return alvo; };
    return alvo(41);
  }
  expect(usa()).toBe(42);
});

// Os controlos: cada ingrediente SOZINHO ja funcionava, e tem de continuar.
test("sem o catch, continua a resolver a funcao de fora", function () {
  function usa(): number {
    const aninhada = function () { var alvo; return alvo; };
    return alvo(41);
  }
  expect(usa()).toBe(42);
});

test("o catch continua a ligar o seu proprio parametro dentro do bloco", function () {
  let visto = "";
  try { throw "erro"; } catch (alvo) { visto = "" + alvo; }
  expect(visto).toBe("erro");
});

// E o que a sobre-aproximacao protege TEM de continuar a valer: um nome que a
// funcao de fora declara e que uma aninhada REALMENTE le vive no ambiente.
test("uma aninhada que le um nome de fora continua a ve-lo", function () {
  function usa(): number {
    var n = 40;
    const aninhada = function () { return n + 2; };
    return aninhada();
  }
  expect(usa()).toBe(42);
});

test("e continua a ve-lo depois de um catch com o mesmo nome", function () {
  function usa(): number {
    var n = 40;
    try { throw 1; } catch (n) { }
    const aninhada = function () { return n + 2; };
    return aninhada();
  }
  expect(usa()).toBe(42);
});
