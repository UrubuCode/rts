// Reflect — rts-shared stdlib utility (NOT a primordial; no native syntax). Pure
// TS over primordials only: dynamic property access (`target[key]`) + Object.keys.
// The engine NAMES nothing Reflect-specific — `Reflect.get(...)` is an ordinary
// static call on this ambient class.
//
// Why pure TS works for the trap-bearing cases: `target[key]` /
// `target[key] = value` lower through the engine's DYNAMIC property trampolines
// (`__rtsadp_obj_get`/`_set`), which detect a Proxy receiver and fire its
// `get`/`set` trap — so `Reflect.get(proxy, k)` / `Reflect.set(proxy, k, v)`
// observe the trap automatically, no Reflect-side proxy logic.
//
// Phase 1 surface: get / set / has. The descriptor surface
// (defineProperty / getOwnPropertyDescriptor / ownKeys) and the
// prototype/apply/construct reflectors are a later increment.
class Reflect {
  // Reflect.get(target, key[, receiver]) — `receiver` (a getter's `this`) is not
  // modeled; the 2-arg form covers the observed usage.
  static get(target: any, key: any, receiver?: any): any {
    return target[key];
  }

  // Reflect.set(target, key, value[, receiver]) — returns whether the assignment
  // succeeded. The dynamic set never reports failure here, so it is always `true`
  // (a `set` trap that returns `false` is not yet propagated — later increment).
  static set(target: any, key: any, value: any, receiver?: any): any {
    target[key] = value;
    return true;
  }

  // Reflect.has(target, key) — `key in target`, via the engine's shape-aware
  // membership op (`__rtsadp_obj_has`), which also fires a Proxy `has` trap.
  // The prototype chain is a later increment.
  static has(target: any, key: any): any {
    return engine.obj_has(target, key);
  }

  // Reflect.deleteProperty(target, key) — `delete target[key]`; returns whether the
  // property is gone afterward (always true here — the model has no
  // non-configurable own properties).
  static deleteProperty(target: any, key: any): any {
    delete target[key];
    return true;
  }

  // Reflect.ownKeys(target) — the target's own (string) keys. Symbol keys are a
  // later increment (Symbol itself is not yet modeled).
  static ownKeys(target: any): any {
    return Object.keys(target);
  }

  // Reflect.apply(target, thisArg, argumentsList) — call `target` with the args
  // from the array. `thisArg` is not bound (the engine's function invoke has no
  // `this` slot — the common `Reflect.apply(fn, null, [...])` is covered).
  static apply(target: any, thisArg: any, argumentsList: any): any {
    return target.apply(thisArg, argumentsList);
  }

  // Reflect.construct(target, argsArray) — `new target(...args)`, via a ponte do
  // new-thunk (engine.construct); um Proxy target dispara a trap `construct`.
  static construct(target: any, argumentsList: any): any {
    return engine.construct(target, argumentsList);
  }

  // Reflect.getPrototypeOf(target) — the object's [[Prototype]] (or null). Reusa o
  // Object.getPrototypeOf (lê a proto side-table do Object.create).
  static getPrototypeOf(target: any): any {
    return Object.getPrototypeOf(target);
  }

  // Reflect.setPrototypeOf(target, proto) — grava o [[Prototype]] e retorna
  // SUCESSO (bool). Roteia por engine.set_proto_check, que dispara a trap
  // `setPrototypeOf` de um Proxy (trap false = rejeita). Um proto null/não-objeto
  // remove a cadeia.
  static setPrototypeOf(target: any, proto: any): any {
    return engine.set_proto_check(target, proto);
  }

  // Reflect.defineProperty(target, key, descriptor) — DATA descriptor: grava o
  // VALOR + os FLAGS REAIS (writable/enumerable/configurable) na descriptor table do
  // engine. Flags OMITIDOS no descriptor default-am para FALSE (semântica JS de
  // defineProperty, diferente de uma atribuição comum que é all-true). Retorna se a
  // definição teve sucesso (false ao adicionar key nova num objeto não-extensível).
  // Accessor descriptors (get/set) são incremento separado.
  static defineProperty(target: any, key: any, descriptor: any): any {
    const flags: number =
      (descriptor.writable ? 1 : 0) |
      (descriptor.enumerable ? 2 : 0) |
      (descriptor.configurable ? 4 : 0);
    return engine.define_prop(target, key, descriptor.value, flags);
  }

  // Reflect.isExtensible(target) — estado REAL de extensibilidade (a descriptor
  // table marca objetos passados a preventExtensions/freeze/seal).
  static isExtensible(target: any): any {
    return engine.is_extensible(target);
  }

  // Reflect.preventExtensions(target) — marca o objeto não-extensível (bloqueia
  // keys novas, de verdade — obj_set passa a rejeitá-las). Retorna true.
  static preventExtensions(target: any): any {
    return engine.prevent_ext(target);
  }

  // Reflect.getOwnPropertyDescriptor(target, key) — descriptor de uma OWN prop com
  // os FLAGS REAIS lidos da descriptor table (uma prop criada por atribuição comum
  // é all-true; uma criada por defineProperty traz seus flags). `undefined` se a key
  // não é own (não anda no proto).
  static getOwnPropertyDescriptor(target: any, key: any): any {
    // engine.get_own_desc sintetiza o descriptor com os FLAGS REAIS e roteia a
    // trap `getOwnPropertyDescriptor` de um Proxy (#218 fase 3).
    return engine.get_own_desc(target, key);
  }
}
