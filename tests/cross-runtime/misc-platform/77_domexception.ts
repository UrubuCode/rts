// Cross-runtime compatibility: DOMException fields.
const err = new DOMException("nope", "AbortError");
console.log("name=" + err.name);
console.log("msg=" + err.message);
console.log("code=" + err.code);
