// Pins revocation DURING an operation: a trap that revokes its own proxy still
// returns normally, but any further internal method the same operation needs —
// the per-key descriptor after ownKeys, the get after the descriptor — throws.

function attempt(label: string, fn: () => string): void {
  try {
    console.log(label + "=" + fn());
  } catch (e: any) {
    console.log(label + "=throw:" + e.constructor.name);
  }
}

// a single-step operation completes: the trap has already produced the answer
const single = Proxy.revocable({ a: 1 }, {
  get(t, k, r) { single.revoke(); return "trapped:" + Reflect.get(t, k, r); },
});
attempt("get_revokes_self", () => String((single.proxy as any).a));
attempt("get_again", () => String((single.proxy as any).a));

// ownKeys revoking mid-flight: Object.keys still needs a descriptor per key
const duringKeys = Proxy.revocable({ a: 1, b: 2 }, {
  ownKeys(t) { duringKeys.revoke(); return Reflect.ownKeys(t); },
});
attempt("keys_revoked_in_ownKeys", () => Object.keys(duringKeys.proxy).join("|"));

// Reflect.ownKeys needs nothing after the trap, so the same handler succeeds
const duringOwnKeys = Proxy.revocable({ a: 1, b: 2 }, {
  ownKeys(t) { duringOwnKeys.revoke(); return Reflect.ownKeys(t); },
});
attempt("reflect_ownKeys_revoked", () => Reflect.ownKeys(duringOwnKeys.proxy).join("|"));

// revoking on the SECOND key: the first descriptor was already collected
const order: string[] = [];
const midway = Proxy.revocable({ a: 1, b: 2, c: 3 }, {
  ownKeys(t) { order.push("ownKeys"); return Reflect.ownKeys(t); },
  getOwnPropertyDescriptor(t, k) {
    order.push("gopd:" + String(k));
    if (k === "b") midway.revoke();
    return Reflect.getOwnPropertyDescriptor(t, k);
  },
});
attempt("keys_revoked_midway", () => Object.keys(midway.proxy).join("|"));
console.log("midway_order=" + order.join(","));

// the same shape for spread and JSON.stringify
const spreadRev = Proxy.revocable({ a: 1, b: 2 }, {
  getOwnPropertyDescriptor(t, k) { if (k === "b") spreadRev.revoke(); return Reflect.getOwnPropertyDescriptor(t, k); },
});
attempt("spread_revoked", () => Object.keys({ ...(spreadRev.proxy as any) }).join("|"));

const jsonRev = Proxy.revocable({ a: 1, b: 2 }, {
  get(t, k, r) { if (k === "b") jsonRev.revoke(); return Reflect.get(t, k, r); },
});
attempt("json_revoked", () => String(JSON.stringify(jsonRev.proxy)));

// a get trap that revokes and then reads its own proxy fails inside the trap
const selfRead = Proxy.revocable({ a: 1, other: 2 }, {
  get(t, k, r) {
    if (k === "a") {
      selfRead.revoke();
      return "self:" + (selfRead.proxy as any).other;
    }
    return Reflect.get(t, k, r);
  },
});
attempt("trap_reads_own_proxy", () => String((selfRead.proxy as any).a));

// apply: the call returns even though the proxy is dead by the time it does
const callRev = Proxy.revocable(function (x: number) { return x * 2; }, {
  apply(t, thisArg, args) { callRev.revoke(); return Reflect.apply(t as any, thisArg, args) + 1; },
});
attempt("apply_revokes_self", () => String((callRev.proxy as any)(10)));
attempt("apply_again", () => String((callRev.proxy as any)(10)));

// construct likewise
const newRev = Proxy.revocable(class { v = 7; }, {
  construct(t, args, nt) { newRev.revoke(); return Reflect.construct(t as any, args, nt); },
});
attempt("construct_revokes_self", () => String(new (newRev.proxy as any)().v));

// a has trap that revokes: `in` completes, the next one does not
const hasRev = Proxy.revocable({ a: 1 }, { has(t, k) { hasRev.revoke(); return Reflect.has(t, k); } });
attempt("has_revokes_self", () => String("a" in (hasRev.proxy as any)));
attempt("has_again", () => String("a" in (hasRev.proxy as any)));

// a getPrototypeOf trap that revokes, observed through instanceof
const protoTrapRev = Proxy.revocable({}, { getPrototypeOf(t) { protoTrapRev.revoke(); return Reflect.getPrototypeOf(t); } });
attempt("instanceof_revokes", () => String(protoTrapRev.proxy instanceof Object));
attempt("instanceof_again", () => String(protoTrapRev.proxy instanceof Object));

// revoking inside the DEFINE trap of a freeze: preventExtensions already ran,
// so the target is shut even though the operation dies
const freezeTarget: any = { a: 1, b: 2 };
const freezeRev = Proxy.revocable(freezeTarget, {
  defineProperty(t, k, d) { if (k === "b") freezeRev.revoke(); return Reflect.defineProperty(t, k, d); },
});
attempt("freeze_revoked", () => { Object.freeze(freezeRev.proxy); return "ok"; });
console.log("freeze_target_extensible=" + Object.isExtensible(freezeTarget));
console.log("freeze_target_a_writable=" + (Object.getOwnPropertyDescriptor(freezeTarget, "a") as any).writable);
console.log("freeze_target_b_writable=" + (Object.getOwnPropertyDescriptor(freezeTarget, "b") as any).writable);

// revoking a proxy used as a PROTOTYPE part-way through a chain walk
const protoRev = Proxy.revocable({ inherited: "P" }, {
  get(t, k, r) { protoRev.revoke(); return Reflect.get(t, k, r); },
});
const kid: any = Object.create(protoRev.proxy);
kid.own = "O";
console.log("kid_own=" + kid.own);
attempt("kid_inherited", () => String(kid.inherited));
attempt("kid_inherited_again", () => String(kid.inherited));
console.log("kid_own_still=" + kid.own);
