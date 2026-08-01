// Cross-runtime: compound-assign (`+=`, `*=`, `|=`, …) sobre uma CADEIA de
// propriedades, não só sobre `ident.prop`.
//
// `o.p += v` é desugarado para `o.p = o.p + v`, o que exige poder avaliar o
// OBJETO duas vezes. O motor só aceitava isso quando o objeto era um
// identificador único, então `this.$1.count += 1` e `a.b[k] |= m` — rotina em
// código minificado — recusavam o ARQUIVO INTEIRO.
//
// Uma chave COMPUTADA (`a[f()].n += 1`) continua recusada de propósito: ali a
// dupla avaliação rodaria `f()` duas vezes.

const o: any = { a: { b: 1, arr: [10, 20] }, k: "b" };

o.a.b += 5;
console.log("membro_aninhado=" + o.a.b);

o.a.arr[1] *= 3;
console.log("indice_em_cadeia=" + o.a.arr[1]);

o.a[o.k] |= 8;
console.log("indice_por_ident=" + o.a.b);

o.a.arr[0] -= 4;
console.log("subtracao=" + o.a.arr[0]);

o.a.b <<= 2;
console.log("shift=" + o.a.b);

// dentro de método, sobre `this`
class Contador {
  estado: any = { n: 1, tags: ["x"] };
  bump(): number {
    this.estado.n += 2;
    return this.estado.n;
  }
  concat(): string {
    this.estado.tags[0] += "y";
    return this.estado.tags[0];
  }
}
const c = new Contador();
console.log("this_cadeia=" + c.bump());
console.log("this_indice=" + c.concat());

// concatenação de string por cadeia
const s: any = { m: { txt: "a" } };
s.m.txt += "b";
console.log("string=" + s.m.txt);

// o valor da expressão é o NOVO valor
const r: any = { v: { n: 1 } };
console.log("valor_da_expr=" + (r.v.n += 9));
