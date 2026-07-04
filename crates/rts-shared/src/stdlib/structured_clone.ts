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
  // TYPE-PRESERVING Map/Set clone (the real algorithm): a fresh instance with
  // each key/value deep-cloned. (Their `#`-private backing became correctly
  // NON-enumerable, so the old key-walk sees zero keys and would return the
  // SAME instance — a mutation on the clone then leaked into the original.)
  if (__scValue instanceof Map) {
    const __scMap = new Map();
    __scSeen.push(__scValue);
    __scClones.push(__scMap);
    const __scEnt = __scValue.entries();
    for (let __scI = 0; __scI < __scEnt.length; __scI++) {
      const __scPair = __scEnt[__scI];
      __scMap.set(
        __structuredCloneInner(__scPair[0], __scSeen, __scClones),
        __structuredCloneInner(__scPair[1], __scSeen, __scClones),
      );
    }
    return __scMap;
  }
  if (__scValue instanceof Set) {
    const __scSet = new Set();
    __scSeen.push(__scValue);
    __scClones.push(__scSet);
    const __scVals = __scValue.values();
    for (let __scI = 0; __scI < __scVals.length; __scI++) {
      __scSet.add(__structuredCloneInner(__scVals[__scI], __scSeen, __scClones));
    }
    return __scSet;
  }
  // TYPE-PRESERVING Date clone: a fresh instance at the same epoch (the opaque
  // as-is return shared the instance — `copy.setFullYear` mutated the original).
  // Date stays the AS-IS return below (the backend-opaque path): reading its
  // epoch needs an instance-method call on an `any` receiver, and the dynamic
  // Registry-class dispatch is a separate increment — a `getTime()` here was a
  // runtime TypeError. Data is preserved; only clone-mutation isolation for
  // Date is deferred (the pre-existing behavior).
  const __scKeys = Object.keys(__scValue);
  // A BACKEND-OPAQUE instance (Date/RegExp/URL — a Rust-backed object with NO
  // enumerable own keys): cloning key-by-key would produce an empty `{}` and
  // lose the value. Return it as-is (data preserved; a type-preserving deep
  // copy needs method dispatch on an `any` receiver — a later increment). An
  // EMPTY plain `{}` also takes this path (indistinguishable without type
  // dispatch) — its clone would be an identical empty object anyway.
  if (__scKeys.length === 0) {
    return __scValue;
  }
  const __scOut: any = {};
  __scSeen.push(__scValue);
  __scClones.push(__scOut);
  for (let __scI = 0; __scI < __scKeys.length; __scI++) {
    const __scK = __scKeys[__scI];
    __scOut[__scK] = __structuredCloneInner(__scValue[__scK], __scSeen, __scClones);
  }
  return __scOut;
}

function structuredClone(value: any, __scOpts?: any): any {
  // An ArrayBuffer clones its BYTES into a fresh buffer; when listed in
  // `opts.transfer`, the SOURCE detaches (byteLength reads 0 afterwards) —
  // the `engine.*` bridges own the byte-level ops the prelude cannot express.
  if (engine.is_buffer(value)) {
    const __scCopy = engine.buffer_clone(value);
    if (__scOpts && __scOpts.transfer) {
      const __scT = __scOpts.transfer;
      for (let __scI = 0; __scI < __scT.length; __scI++) {
        if (__scT[__scI] === value) {
          engine.buffer_detach(value);
          break;
        }
      }
    }
    return __scCopy;
  }
  return __structuredCloneInner(value, [], []);
}
