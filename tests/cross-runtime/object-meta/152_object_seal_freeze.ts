// Cross-runtime: Object.seal and freeze
//
// Declara-se MODULO de proposito, e as escritas recusadas estao APANHADAS. Ver
// 154_object_defineproperty.ts, que diz porque: sem `export {}` os dois motores
// respondiam ao modo em vez de a semantica, e depois de fixado o modo ficavam a
// discordar so no texto da mensagem.
export {};

const obj1: any = { a: 1, b: 2 };
Object.seal(obj1);
console.log("sealed=" + Object.isSealed(obj1));
console.log("frozen=" + Object.isFrozen(obj1));
// Selado, nao congelado: a escrita numa propriedade que ja existe PASSA.
obj1.a = 10;
console.log("modified=" + obj1.a);

const obj2: any = { x: 1, y: 2 };
Object.freeze(obj2);
console.log("frozen2=" + Object.isFrozen(obj2));
console.log("sealed2=" + Object.isSealed(obj2));
try {
  obj2.x = 10;
  console.log("write=silent");
} catch (e: any) {
  console.log("write=" + e.constructor.name);
}
console.log("not_modified=" + obj2.x);

const obj3 = { p: 1 };
console.log("normal_sealed=" + Object.isSealed(obj3));
console.log("normal_frozen=" + Object.isFrozen(obj3));
