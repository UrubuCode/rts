import { describe, test, expect } from "rts:test";

// Um método que NÃO existe em `Array.prototype`, chamado sobre um receptor que
// o front apenas INFERIU ser array, resolve dinamicamente em vez de matar a
// compilação.
//
// O problema é a natureza da inferência: `is_array_receiver` decide encadeamento
// pelo NOME do método anterior (`x.filter(..).reverse()` "é array" porque
// `reverse` é um método de Array que devolve array), não por prova de tipo. Um
// bundle real colide com esses nomes o tempo todo — o `Collection` do Dexie,
// dentro do WhatsApp Web, define os SEUS `filter`/`reverse`/`until`, então
// `logs.orderBy(..).filter(..).reverse().until(..)` chegava ao caminho de Array
// e batia em `no Registry entry for Array.until(2 args)`. Um bail em tempo de
// compilação: o programa inteiro morria por causa do palpite.
//
// O despacho dinâmico é ao mesmo tempo mais correto e mais seguro: resolve o
// método no receptor de RUNTIME, então um Collection de verdade acha o seu
// `until`, e um receptor que realmente É um array recebe a resposta do JS (um
// TypeError ao chamar `undefined`) em vez de uma recusa de compilação. Vale só
// para nomes SEM nenhuma linha de Array em nenhuma aridade — todo método de
// Array implementado mantém o caminho tipado.
//
// Valores conferidos contra o Node.

function Coll(this: any, n: number) {
  this.n = n;
}
(Coll as any).prototype.reverse = function (this: any) {
  this.n = -this.n;
  return this;
};
(Coll as any).prototype.filter = function (this: any, f: any) {
  return this;
};
(Coll as any).prototype.until = function (this: any, f: any, inclusive: any) {
  return f(this.n) ? "hit:" + this.n : "miss";
};

const direto = new (Coll as any)(7)
  .reverse()
  .until(function (x: any) {
    return x === -7;
  }, true);

const encadeado = new (Coll as any)(3)
  .filter(function () {
    return true;
  })
  .reverse()
  .until(function (x: any) {
    return x > 0;
  }, false);

describe("método fora de Array.prototype sobre receptor inferido como array", () => {
  test("`.reverse().until(cb, flag)` acha o método do protótipo do usuário", () => {
    expect(direto).toBe("hit:-7");
  });

  test("a cadeia inteira `.filter().reverse().until()` resolve", () => {
    expect(encadeado).toBe("miss");
  });
});
