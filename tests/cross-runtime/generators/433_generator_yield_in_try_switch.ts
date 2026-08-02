// Cross-runtime: `yield` dentro de `try`, `switch` e bloco ROTULADO num
// generator-expressão que CAPTURA (o caminho eager-buffer).
//
// O desugar eager descia em `if`/`while`/`for`, mas passava `try`/`switch`/`L:`
// INTACTOS — então um `yield` lá dentro sobrevivia e chegava ao lowering como
// `Yield` cru ("expression raw/unrecognized"), derrubando o ARQUIVO INTEIRO.
//
// É a forma mais comum do `asyncToGenerator` do Babel — `try { yield f() }
// catch (e) { … }` —, então o buraco valia vários bundles de uma página real.
//
// Usa `for-of` (e não `.next()`) porque o buffer eager perde o protocolo de
// iterador ao atravessar propriedade/retorno — gap SEPARADO, registrado na
// issue #2092, que não deve mascarar o que esta fixture testa.

function coleta(it: Iterable<unknown>): string {
  let o = "";
  for (const v of it) o = o + "[" + v + "]";
  return o;
}

function fabrica() {
  let falhou = false;
  const g = function* (x: string) {
    if (x !== "pula") {
      try {
        yield "t:" + x;
      } catch (e) {
        falhou = true;
      }
    }
    yield "fim";
  };
  return {
    g: g,
    ok: function (): boolean {
      return !falhou;
    },
  };
}

const m = fabrica();
console.log("try_com=" + coleta(m.g("a")));
console.log("try_sem=" + coleta(m.g("pula")));
console.log("captura_intacta=" + m.ok());

// switch + bloco rotulado
function outra() {
  let marca = "";
  const h = function* (n: number) {
    switch (n) {
      case 1:
        yield "um";
        break;
      default:
        yield "outro";
    }
    L: {
      marca = "visitou";
      yield "rot";
    }
  };
  return { h: h, marca: function (): string { return marca; } };
}
const o = outra();
console.log("switch_1=" + coleta(o.h(1)));
console.log("switch_d=" + coleta(o.h(9)));
console.log("rotulo_efeito=" + o.marca());

// try/finally também
function comFinally() {
  let passos = "";
  const g = function* () {
    try {
      yield "corpo";
    } finally {
      passos = passos + "F";
    }
    yield "depois";
  };
  return { g: g, passos: function (): string { return passos; } };
}
const f = comFinally();
console.log("finally=" + coleta(f.g()));
console.log("finally_rodou=" + f.passos());
