// Cross-runtime: `name` lives on Error.prototype and `message` only becomes an
// own property when an argument was passed; Error.prototype.toString joins the
// two and drops either half when it is empty.
const e = new Error("boom");
console.log("has-own-name=" + Object.prototype.hasOwnProperty.call(e, "name"));
console.log("has-own-message=" + Object.prototype.hasOwnProperty.call(e, "message"));
console.log("proto-name=" + Object.prototype.hasOwnProperty.call(Error.prototype, "name"));
console.log("proto-message=" + Object.prototype.hasOwnProperty.call(Error.prototype, "message"));
console.log("proto-message-value=" + JSON.stringify(Error.prototype.message));
console.log("proto-name-value=" + Error.prototype.name);

const nd: any = Object.getOwnPropertyDescriptor(Error.prototype, "name");
console.log("proto-name-desc=w" + nd.writable + ",e" + nd.enumerable + ",c" + nd.configurable);
const md: any = Object.getOwnPropertyDescriptor(e, "message");
console.log("own-message-desc=w" + md.writable + ",e" + md.enumerable + ",c" + md.configurable);

const bare = new Error();
console.log("bare-has-own-message=" + Object.prototype.hasOwnProperty.call(bare, "message"));
console.log("bare-message=" + JSON.stringify(bare.message));
console.log("bare-tostring=" + bare.toString());
console.log("bare-keys=" + Object.keys(bare).join(","));

// message is never enumerable, so JSON.stringify sees nothing of it.
console.log("json=" + JSON.stringify(e));
console.log("json-array=" + JSON.stringify([e]));

// Every subclass carries its own prototype `name`.
console.log("type-name=" + TypeError.prototype.name);
console.log("range-name=" + RangeError.prototype.name);
console.log("syntax-name=" + SyntaxError.prototype.name);
console.log("ref-name=" + ReferenceError.prototype.name);
console.log("uri-name=" + URIError.prototype.name);
console.log("eval-name=" + EvalError.prototype.name);
console.log("agg-name=" + AggregateError.prototype.name);
console.log("type-own-name=" + Object.prototype.hasOwnProperty.call(new TypeError("x"), "name"));

// Object.prototype.toString.call sees the Error tag for every one of them.
console.log("tag-error=" + Object.prototype.toString.call(e));
console.log("tag-type=" + Object.prototype.toString.call(new TypeError("x")));
console.log("tag-agg=" + Object.prototype.toString.call(new AggregateError([])));
console.log("tag-proto=" + Object.prototype.toString.call(Error.prototype));

// toString: name only, message only, both, and neither.
const t: any = new Error("msg");
t.name = "";
console.log("empty-name=" + JSON.stringify(t.toString()));
t.name = "Custom";
console.log("custom=" + t.toString());
t.message = "";
console.log("empty-message=" + JSON.stringify(t.toString()));
t.name = "";
console.log("both-empty=" + JSON.stringify(t.toString()));

// undefined name/message fall back to "Error" and "".
const u: any = new Error("m");
u.name = undefined;
console.log("undef-name=" + u.toString());
const u2: any = new Error("m");
u2.message = undefined;
console.log("undef-message=" + u2.toString());

// Non-string name/message are coerced by ToString.
const c: any = new Error();
c.name = 7;
c.message = { toString: () => "objmsg" };
console.log("coerced=" + c.toString());

// toString on a non-Error object works as long as it has the two keys.
console.log("borrowed=" + Error.prototype.toString.call({ name: "N", message: "M" }));
console.log("borrowed-empty=" + JSON.stringify(Error.prototype.toString.call({})));
try {
  Error.prototype.toString.call("string" as any);
  console.log("borrowed-prim=no-throw");
} catch (err: any) {
  console.log("borrowed-prim=" + err.constructor.name);
}

// The prototype chain of the subclasses.
console.log("chain-type=" + (Object.getPrototypeOf(TypeError.prototype) === Error.prototype));
console.log("chain-ctor=" + (Object.getPrototypeOf(TypeError) === Error));
console.log("chain-agg=" + (Object.getPrototypeOf(AggregateError.prototype) === Error.prototype));
console.log("proto-ctor=" + (Error.prototype.constructor === Error));
console.log("error-length=" + Error.length);
console.log("agg-length=" + AggregateError.length);
console.log("type-length=" + TypeError.length);
