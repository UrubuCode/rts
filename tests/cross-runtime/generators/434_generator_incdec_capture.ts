// Cross-runtime: `++`/`--` sobre uma variável CAPTURADA por um
// generator-expressão.
//
// O #2091 passou a levar as capturas ESCRITAS por referência (par
// getter/setter), mas deixava `x++` recusado — e era exatamente ele que os três
// generators restantes da carga do WhatsApp Web usavam (o diagnóstico
// `RTS_DIAG_GEN=1` nomeou a forma: "recusou: `l++`").
//
// O valor da EXPRESSÃO difere entre as duas formas, e é isso que decide a
// tradução: `++s` vale o NOVO, `s++` vale o ANTIGO. O pós-fixo é reconstruído a
// partir do novo (`novo - 1`), o que é exato porque `++` já força ToNumber.

function drive(it: any, envia: string): string {
  let r = it.next();
  let o = "";
  while (!r.done) {
    o = o + "[y " + r.value + "]";
    r = it.next(envia);
  }
  return o + "[ret " + r.value + "]";
}

function fabrica() {
  let n = 0;
  let m = 10;
  const g = function* () {
    const a = yield "pos:" + n++;
    const b = yield "pre:" + ++n;
    const c = yield "dec_pos:" + m--;
    const d = yield "dec_pre:" + --m;
    return a + "/" + b + "/" + c + "/" + d;
  };
  return {
    g: g,
    n: function (): number { return n; },
    m: function (): number { return m; },
  };
}

const o = fabrica();
console.log("passos=" + drive(o.g(), "S"));
console.log("n_escapou=" + o.n());
console.log("m_escapou=" + o.m());

// duas instâncias do MESMO generator compartilham a variável capturada
const p = fabrica();
drive(p.g(), "A");
drive(p.g(), "B");
console.log("acumulou=" + p.n());

// `++` sobre capturada dentro de try (a forma do bundle real)
function comTry() {
  let tentativas = 0;
  const g = function* (x: string) {
    try {
      tentativas++;
      const r = yield "t:" + x;
      return r;
    } catch (e) {
      return "erro";
    }
  };
  return { g: g, tentativas: function (): number { return tentativas; } };
}
const t = comTry();
drive(t.g("a"), "X");
drive(t.g("b"), "Y");
console.log("tentativas=" + t.tentativas());
