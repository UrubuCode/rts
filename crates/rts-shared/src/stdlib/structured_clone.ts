// structuredClone — the rts-shared stdlib global (NOT a primordial; no native
// syntax). Pure TS over primordials only (typeof / Array.isArray / Object.keys /
// recursion) — the engine names NOTHING here.
//
// A deep, structural clone of plain objects + arrays. A MEMO (two parallel
// arrays: seen originals ↔ their clones, identity `===` lookup) preserves
// CYCLES and shared references: a self-referencing object clones to a clone
// that references ITSELF (`copy.self === copy`), matching the real algorithm.
// The memo also bounds the recursion for cyclic inputs (a revisited object
// returns its in-progress clone instead of recursing forever). NOTE: this
// models the JSON-shaped surface (objects, arrays, primitives); Date/Map/Set
// value cloning needs method dispatch on an `any`-typed receiver (a separate
// increment) and is NOT modeled — such a value is cloned structurally, not
// type-preservingly.
//
// IMPORTANT — naming: a prelude function is lowered for EVERY program, and its
// LOCAL names must not collide with a user's module-level captured `let` (which
// the engine promotes to a by-name runtime cell). So every local here carries the
// `__sc` prefix — a bare `out`/`arr`/`keys` would alias a user global of that name.
//
// Bodies use ONLY operations the engine lowers on an `any` value (typeof /
// Array.isArray / Object.keys / dynamic index+get / array push|length|index): a
// single unsupported method here would bail the whole compile. Keep it
// primordial-only.

function __structuredCloneInner(__scValue: any, __scSeen: any[], __scClones: any[]): any {
  if (__scValue === null || typeof __scValue !== "object") {
    return __scValue;
  }
  // Memo hit: a cycle / shared reference — return the (possibly in-progress)
  // clone registered for this original.
  for (let __scM = 0; __scM < __scSeen.length; __scM++) {
    if (__scSeen[__scM] === __scValue) {
      return __scClones[__scM];
    }
  }
  if (Array.isArray(__scValue)) {
    const __scArr: any[] = [];
    __scSeen.push(__scValue);
    __scClones.push(__scArr);
    for (let __scI = 0; __scI < __scValue.length; __scI++) {
      __scArr.push(__structuredCloneInner(__scValue[__scI], __scSeen, __scClones));
    }
    return __scArr;
  }
  const __scOut: any = {};
  __scSeen.push(__scValue);
  __scClones.push(__scOut);
  const __scKeys = Object.keys(__scValue);
  for (let __scI = 0; __scI < __scKeys.length; __scI++) {
    const __scK = __scKeys[__scI];
    __scOut[__scK] = __structuredCloneInner(__scValue[__scK], __scSeen, __scClones);
  }
  return __scOut;
}

function structuredClone(value: any): any {
  return __structuredCloneInner(value, [], []);
}
