// Cross-runtime: argumentos EXTRAS num `new` são ignorados.
//
// Chamar um construtor com mais argumentos do que ele declara é legal em JS —
// os que sobram simplesmente não são lidos. O RTS exigia aridade EXATA para as
// classes registradas e recusava o ARQUIVO INTEIRO por um `new FormData(a, b)`
// ou `new AbortSignal(x)` vindos de um bundle.
//
// Os extras continuam sendo AVALIADOS: a avaliação é observável, só o valor é
// descartado — por isso a fixture conta os efeitos colaterais.

let efeitos = 0;
function ef(v: string): string {
  efeitos = efeitos + 1;
  return v;
}

// classe de usuário
class Ponto {
  v: unknown;
  constructor(a: unknown) {
    this.v = a;
  }
}
const p = new (Ponto as any)(1, ef("x"), ef("y"));
console.log("usuario=" + p.v);
console.log("extras_avaliados=" + efeitos);

// classes registradas
const m = new (Map as any)([["k", 1]], ef("m"));
console.log("map=" + m.get("k"));

const s = new (Set as any)([1, 2], ef("s"));
console.log("set=" + s.size);

const u = new (URL as any)("https://exemplo.com/caminho", undefined, ef("u"));
console.log("url=" + u.pathname);

console.log("efeitos_total=" + efeitos);
