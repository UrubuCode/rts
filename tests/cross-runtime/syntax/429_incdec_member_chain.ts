// Cross-runtime: `++`/`--` sobre uma CADEIA de propriedades.
//
// Mesma raiz do 428: o desugar precisa avaliar o objeto duas vezes, e o motor
// só aceitava um identificador ÚNICO como base — `this.estado.n++` recusava o
// arquivo inteiro com "assignment target must be a simple identifier".
const o: any = { a: { n: 1, arr: [5] } };

o.a.n++;
console.log("pos_inc=" + o.a.n);
++o.a.n;
console.log("pre_inc=" + o.a.n);
o.a.arr[0]--;
console.log("dec_indice=" + o.a.arr[0]);

// pós-fixo devolve o valor ANTIGO, pré-fixo o NOVO
console.log("valor_posfixo=" + (o.a.n++));
console.log("depois=" + o.a.n);
console.log("valor_prefixo=" + (++o.a.n));

// dentro de método, sobre `this`
class Contador {
  estado: any = { n: 0 };
  go(): number {
    this.estado.n++;
    return this.estado.n;
  }
}
const c = new Contador();
c.go();
console.log("this_cadeia=" + c.go());
