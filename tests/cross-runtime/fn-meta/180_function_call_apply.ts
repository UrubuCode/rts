// Cross-runtime: Function.call and apply
//
// Declara-se MODULO de proposito, e o `this` nulo esta APANHADO. Ver
// 154_object_defineproperty.ts para o porque do primeiro: sem `export {}` o
// node corria isto nao-estrito, onde um `this` nulo vira o objecto global e
// `this.suffix` responde `undefined`, enquanto o bun corria estrito, onde fica
// nulo e a leitura lanca. Os dois estavam certos para o seu modo, o que e
// exactamente o que uma fixture cruzada nao pode deixar em aberto.
export {};

function greet(this: any, greeting: string, name: string) {
  return greeting + " " + name + (this.suffix || "");
}

const obj = { suffix: "!" };

console.log("call=" + greet.call(obj, "Hello", "World"));
console.log("apply=" + greet.apply(obj, ["Hi", "There"]));

// Sem contexto: em modo estrito o `this` fica NULO em vez de virar o global, e
// portanto ler uma propriedade dele lanca. O que se compara e isso.
try {
  console.log("call_null=" + greet.call(null, "Hey", "You"));
} catch (e: any) {
  console.log("call_null=" + e.constructor.name);
}

// Math.max with apply
const nums = [1, 5, 3, 9, 2];
console.log("max=" + Math.max.apply(null, nums));
