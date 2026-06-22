// structuredClone — the rts-shared stdlib global (NOT a primordial; no native
// syntax). Pure TS over primordials only (typeof / Array.isArray / Object.keys /
// recursion) — the engine names NOTHING here.
//
// A deep, structural clone of plain objects + arrays. A DEPTH GUARD bounds the
// recursion so a cyclic / pathologically-deep input cannot stack-overflow (it
// stops cloning past the bound and returns the value as-is — a defined result,
// never a crash). NOTE: this models the JSON-shaped surface (objects, arrays,
// primitives); Date/Map/Set value cloning needs method dispatch on an `any`-typed
// receiver (a separate increment) and is NOT modeled — such a value is cloned
// structurally, not type-preservingly.
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

function __structuredCloneInner(__scValue: any, __scDepth: number): any {
  if (__scValue === null || typeof __scValue !== "object") {
    return __scValue;
  }
  // Cycle / too-deep guard: stop past the bound (a defined value, never a crash).
  if (__scDepth > 1000) {
    return __scValue;
  }
  if (Array.isArray(__scValue)) {
    const __scArr: any[] = [];
    for (let __scI = 0; __scI < __scValue.length; __scI++) {
      __scArr.push(__structuredCloneInner(__scValue[__scI], __scDepth + 1));
    }
    return __scArr;
  }
  const __scOut: any = {};
  const __scKeys = Object.keys(__scValue);
  for (let __scI = 0; __scI < __scKeys.length; __scI++) {
    const __scK = __scKeys[__scI];
    __scOut[__scK] = __structuredCloneInner(__scValue[__scK], __scDepth + 1);
  }
  return __scOut;
}

function structuredClone(value: any): any {
  return __structuredCloneInner(value, 0);
}
