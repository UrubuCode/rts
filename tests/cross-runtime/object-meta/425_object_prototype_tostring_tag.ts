// Cross-runtime: `Object.prototype.toString.call(v)` — o `[object <Tag>]` da
// spec. É O idioma de checagem de tipo do ecossistema JS: toda biblioteca
// escreve `Object.prototype.toString.call(x) === "[object Array]"` porque é o
// único teste que funciona entre realms e não pode ser enganado por uma
// propriedade forjada.
//
// O RTS respondia a CONSTANTE "[object Object]" para qualquer receiver, então
// um ARRAY se reportava como `Object` — errado e em silêncio, que é justamente
// o modo de falha que esse idioma existe para evitar.
//
// ESCOPO desta fixture: os receivers OBJETO, que é onde o idioma é usado de
// verdade. Um receiver PRIMITIVO (`.call(1)`) ainda diverge no RTS por um
// problema separado no despacho de `.call` sobre receiver não-objeto, e está
// fora daqui de propósito — uma fixture cross-runtime que falha não documenta
// nada, ela só vira ruído no CI.

const tag = Object.prototype.toString;

console.log("objeto=" + tag.call({}));
console.log("objeto_com_campos=" + tag.call({ a: 1, b: 2 }));
console.log("array_vazio=" + tag.call([]));
console.log("array=" + tag.call([1, 2, 3]));
console.log("array_aninhado=" + tag.call([[1], [2]]));

// o idioma como ele aparece em bibliotecas
function ehArray(x: unknown): boolean {
  return Object.prototype.toString.call(x) === "[object Array]";
}
console.log("ehArray_arr=" + ehArray([1]));
console.log("ehArray_obj=" + ehArray({}));
console.log("ehArray_str=" + ehArray("abc"));

// instância de classe: sem Symbol.toStringTag o resultado é Object
class Caixa {
  v = 1;
}
console.log("instancia=" + tag.call(new Caixa()));

// objeto criado sem protótipo continua reportando Object
console.log("sem_proto=" + tag.call(Object.create(null)));

// o toString NORMAL (não via .call) não pode ter mudado
console.log("direto_obj=" + {}.toString());
console.log("direto_arr=" + [1, 2].toString());
console.log("concat=" + ("" + {}));
console.log("String_de_obj=" + String({}));
