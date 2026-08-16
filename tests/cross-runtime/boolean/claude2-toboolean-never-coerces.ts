// ONE thing: ToBoolean does not CALL anything. Every object is true by the
// closed rule "type is Object", so valueOf, toString and @@toPrimitive are
// never consulted, and a Proxy in a condition fires none of its traps — which
// a logging handler proves rather than asserts.

const calls: string[] = [];

const lying: any = {
  valueOf() {
    calls.push("valueOf");
    return false;
  },
  toString() {
    calls.push("toString");
    return "";
  },
};

const primitiveHook: any = {
  [Symbol.toPrimitive](hint: string) {
    calls.push("toPrimitive:" + hint);
    return false;
  },
};

const handler: ProxyHandler<any> = {
  get(t, k) {
    calls.push("get:" + String(k));
    return (t as any)[k];
  },
  has(t, k) {
    calls.push("has:" + String(k));
    return k in (t as any);
  },
  ownKeys(t) {
    calls.push("ownKeys");
    return Reflect.ownKeys(t as any);
  },
  getOwnPropertyDescriptor(t, k) {
    calls.push("gopd:" + String(k));
    return Reflect.getOwnPropertyDescriptor(t as any, k);
  },
  getPrototypeOf(t) {
    calls.push("getPrototypeOf");
    return Reflect.getPrototypeOf(t as any);
  },
};
const spy: any = new Proxy({ valueOf: () => false }, handler);

function truthiness(label: string, v: any): void {
  calls.length = 0;
  const viaIf = v ? "truthy" : "falsy";
  const viaNot = !v;
  const viaBoolean = Boolean(v);
  const viaDouble = !!v;
  const viaTernaryChain = v && true;
  console.log(
    label +
      " | if:" + viaIf +
      " | !:" + viaNot +
      " | Boolean():" + viaBoolean +
      " | !!:" + viaDouble +
      " | &&:" + String(viaTernaryChain) +
      " | calls:[" + calls.join(",") + "]"
  );
}

// --- objects that try very hard to be false, and fail ---
truthiness("valueOf_false", lying);
truthiness("toPrimitive_false", primitiveHook);
truthiness("proxy_spy", spy);
truthiness("boolean_wrapper_false", new Boolean(false));
truthiness("number_wrapper_zero", new Number(0));
truthiness("number_wrapper_nan", new Number(NaN));
truthiness("string_wrapper_empty", new String(""));
truthiness("bigint_wrapper_zero", Object(0n));
truthiness("empty_array", []);
truthiness("empty_object", {});
truthiness("null_prototype", Object.create(null));
truthiness("function", function () {});
truthiness("arrow", () => 0);
truthiness("class", class {});
truthiness("empty_map", new Map());
truthiness("empty_set", new Set());
truthiness("regexp", /(?:)/);
truthiness("error", new Error("x"));
truthiness("symbol", Symbol("s"));
truthiness("empty_typed_array", new Uint8Array(0));
truthiness("array_buffer", new ArrayBuffer(0));
truthiness("promise", Promise.resolve(false));

// --- the seven falsy primitives, and nothing else is falsy ---
truthiness("false", false);
truthiness("zero", 0);
truthiness("neg_zero", -0);
truthiness("zero_bigint", 0n);
truthiness("empty_string", "");
truthiness("nan", NaN);
truthiness("null", null);
truthiness("undefined", undefined);
truthiness("space_string", " ");
truthiness("zero_string", "0");
truthiness("false_string", "false");
truthiness("neg_one", -1);
truthiness("tiny", Number.MIN_VALUE);
truthiness("neg_bigint", -1n);

// --- a revoked Proxy is still an object, so it is still truthy and the
//     revocation never surfaces ---
const rev = Proxy.revocable({}, {});
rev.revoke();
try {
  console.log("revoked_proxy_truthy=" + (rev.proxy ? "truthy" : "falsy") + " boolean=" + Boolean(rev.proxy));
} catch (e) {
  console.log("revoked_proxy!" + (e as any).constructor.name);
}

// --- the same object in a context that DOES coerce, for contrast ---
calls.length = 0;
console.log("plus_operator=" + String(lying + 1) + " calls=[" + calls.join(",") + "]");
calls.length = 0;
console.log("loose_equality=" + (lying == false) + " calls=[" + calls.join(",") + "]");
calls.length = 0;
console.log("relational=" + (lying < 1) + " calls=[" + calls.join(",") + "]");
calls.length = 0;
console.log("template=" + `${lying}` + " calls=[" + calls.join(",") + "]");
calls.length = 0;
console.log("toPrimitive_plus=" + String(primitiveHook + 1) + " calls=[" + calls.join(",") + "]");
calls.length = 0;
console.log("proxy_property_read=" + String(spy.valueOf()) + " calls=[" + calls.join(",") + "]");

// --- and where the condition is a comparison, coercion is back ---
calls.length = 0;
if (lying == false) {
  console.log("equality_condition_taken calls=[" + calls.join(",") + "]");
}
calls.length = 0;
const filtered = [lying, primitiveHook, 0, "", false, {}].filter(Boolean);
console.log("filter_Boolean_length=" + filtered.length + " calls=[" + calls.join(",") + "]");
console.log("every_truthy=" + [1, "a", {}, []].every(Boolean));
console.log("some_falsy=" + [1, "a", 0].some((v) => !v));
