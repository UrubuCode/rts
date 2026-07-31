// `Object` — the LAST `.ts` remnant of this class, kept for ONE reason.
//
// Everything else moved to pure Rust in the `rtse` format:
//
//   - the INSTANCE surface (`toString`/`toLocaleString`/`valueOf` +
//     `hasOwnProperty`/`propertyIsEnumerable`/`isPrototypeOf`) →
//     `rts-primitives/src/object/value_class.rs`, a real
//     `#[rtse::class("Object", value)]`;
//   - `Object(value)` / `new Object(value)` → the `__rtsadp_obj_factory`
//     trampoline;
//   - `Object.groupBy` → `__rtsadp_obj_group_by`, where the per-element bucket
//     lookup is the engine's own shape-indexed property read.
//
// What survives is the STATIC surface read as a VALUE — see the block below for
// why it cannot follow them yet.
//
// COEXISTENCE: this ambient class and the Rust value-class both describe
// `Object`. That is intentional and temporary. `try_primitive_class_method`
// consults the ambient descriptor FIRST and falls through to the Registry class
// when it does not carry the member, so instance calls reach the Rust class
// while the statics keep resolving here. Deleting this file becomes correct the
// moment a Registry class static can be read as a value.

class Object {
  // ── ESTÁTICOS lidos como VALOR ───────────────────────────────────────────
  //
  // A forma CHAMADA (`Object.keys(o)`) nunca passa por aqui: o
  // `front/run/objstatic.rs` a intercepta antes e lowerizada nativa, por shape
  // — caminho rápido, intacto (ele tem um carve-out explícito para não deixar
  // esta classe ambiente sombreá-lo).
  //
  // Estes existem para a forma LIDA (`const f = Object.keys`), que antes
  // bailava com "no such static field on class `Object`": o leitor genérico de
  // estático de classe procura em `desc.statics`, e este bloco é o que o
  // popula. É o mesmo mecanismo que já faz `JSON.stringify` ser reificável.
  //
  // Cada um delega ao próprio caminho nativo — sem reimplementar nada.
  static keys(o: any): any { return Object.keys(o); }
  static values(o: any): any { return Object.values(o); }
  static entries(o: any): any { return Object.entries(o); }
  static assign(alvo: any, fonte: any): any { return Object.assign(alvo, fonte); }
  static freeze(o: any): any { return Object.freeze(o); }
  static isFrozen(o: any): boolean { return Object.isFrozen(o); }
  static seal(o: any): any { return Object.seal(o); }
  static isSealed(o: any): boolean { return Object.isSealed(o); }
  static getPrototypeOf(o: any): any { return Object.getPrototypeOf(o); }
  static getOwnPropertyNames(o: any): any { return Object.getOwnPropertyNames(o); }
  // O resto da superfície estática que o caminho nativo JÁ lowera na forma
  // CHAMADA. Sem estes shims a forma LIDA continuava bailando — e é a forma que
  // bundle minificado usa o tempo todo (`var d = Object.defineProperty`).
  static defineProperty(o: any, k: any, d: any): any { return Object.defineProperty(o, k, d); }
  static defineProperties(o: any, d: any): any { return Object.defineProperties(o, d); }
  static fromEntries(it: any[]): any { return Object.fromEntries(it); }
  static create(proto: any): any { return Object.create(proto); }
  static setPrototypeOf(o: any, proto: any): any { return Object.setPrototypeOf(o, proto); }
  static getOwnPropertyDescriptor(o: any, k: any): any { return Object.getOwnPropertyDescriptor(o, k); }
  static getOwnPropertySymbols(o: any): any { return Object.getOwnPropertySymbols(o); }
  static preventExtensions(o: any): any { return Object.preventExtensions(o); }
  static isExtensible(o: any): boolean { return Object.isExtensible(o); }
}
