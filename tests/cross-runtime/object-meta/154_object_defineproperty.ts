// Cross-runtime: Object.defineProperty
//
// Declara-se MODULO de proposito. Sem `export {}` o node trata um .ts sem
// import/export como CommonJS (nao-estrito) e o bun como modulo ES (estrito),
// e os dois respondiam ao MODO em vez de a semantica: a escrita numa
// propriedade nao-gravavel e silenciosa num e lanca no outro.
//
// E a escrita esta APANHADA em vez de deixada a matar o processo, porque o que
// ha para comparar entre motores e "lancou um TypeError e nao mudou o valor" —
// o texto da mensagem e o formato do stack trace nunca foram comparaveis, e era
// so neles que o bun e o node ficavam a discordar depois do modo estar fixado.
export {};

const obj: any = {};
Object.defineProperty(obj, "a", {
  value: 42,
  writable: false,
  enumerable: true,
  configurable: false
});

console.log("value=" + obj.a);
try {
  obj.a = 100;
  console.log("write=silent");
} catch (e: any) {
  console.log("write=" + e.constructor.name);
}
console.log("still=" + obj.a);

Object.defineProperty(obj, "b", {
  value: 10,
  enumerable: false
});

console.log("keys=" + Object.keys(obj).join(","));
console.log("b=" + obj.b);

// Getter/setter
Object.defineProperty(obj, "c", {
  get() { return this._c || 0; },
  set(val) { this._c = val * 2; },
  enumerable: true
});

obj.c = 5;
console.log("getter=" + obj.c);
