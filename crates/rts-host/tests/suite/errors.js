// `Error` and the family that inherits from it.
let failed = "";
function check(name, held) { if (!held) { failed = failed + name + ","; } }

check("message", new Error("boom").message === "boom");
check("name", new Error("boom").name === "Error");
check("to-string", new Error("boom").toString() === "Error: boom");
check("no-message", new Error().toString() === "Error");
check("without-new", Error("boom").message === "boom");
check("typeof", typeof new Error("x") === "object");

check("type-error-name", new TypeError("x").name === "TypeError");
check("range-error-name", new RangeError("x").name === "RangeError");
check("syntax-error-name", new SyntaxError("x").name === "SyntaxError");
check("reference-error-name", new ReferenceError("x").name === "ReferenceError");
check("eval-error-name", new EvalError("x").name === "EvalError");
check("uri-error-name", new URIError("x").name === "URIError");

check("subclass-to-string", new TypeError("nope").toString() === "TypeError: nope");
check("instance-of-parent", new RangeError("x") instanceof Error);
check("instance-of-self", new RangeError("x") instanceof RangeError);
check("not-a-sibling", (new RangeError("x") instanceof TypeError) === false);

// `toString` reads `name` through the ordinary property path, so replacing it
// is answered — which reading the registered class name would have got wrong.
let renamed = new Error("b");
renamed.name = "Mine";
check("renamed", renamed.toString() === "Mine: b");

// A user class extending a built-in reaches its own prototype, which is what
// the native constructor asking `new.target` buys.
class Mine extends Error { own() { return this.message; } }
check("user-subclass-method", new Mine("m").own() === "m");
check("user-subclass-instance", new Mine("m") instanceof Error);
check("user-subclass-message", new Mine("m").message === "m");

return failed;
