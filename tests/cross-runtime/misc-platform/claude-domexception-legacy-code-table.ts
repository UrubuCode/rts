// Cross-runtime: DOMException carries a LEGACY numeric code that is derived
// from its name through a fixed 25-entry table — names added after the table
// was frozen answer 0 — and the same numbers are exposed as constants on both
// the constructor and its prototype.

const d = new DOMException("nope", "AbortError");
console.log("fields=" + d.name + "|" + d.message + "|" + d.code);
console.log("defaults=" + new DOMException().name + "|" + JSON.stringify(new DOMException().message) + "|" + new DOMException().code);
console.log("message_only=" + new DOMException("m").name + "|" + new DOMException("m").code);
console.log("unknown_name=" + new DOMException("m", "NotARealName").name + " code=" + new DOMException("m", "NotARealName").code);
console.log("coerced_args=" + new DOMException(1 as any, 2 as any).message + "|" + new DOMException(1 as any, 2 as any).name);

// It is an Error subclass with its own tag, and nothing of it is an own
// enumerable property.
console.log("is_error=" + (d instanceof Error) + "," + (d instanceof DOMException));
console.log("proto_chain=" + (Object.getPrototypeOf(DOMException.prototype) === Error.prototype));
console.log("tag=" + Object.prototype.toString.call(d));
console.log("toString=" + d.toString());
console.log("own_enumerable=" + JSON.stringify(Object.keys(d)));
console.log("name_is_accessor=" + typeof (Object.getOwnPropertyDescriptor(DOMException.prototype, "name") as any).get);
console.log("message_is_accessor=" + typeof (Object.getOwnPropertyDescriptor(DOMException.prototype, "message") as any).get);
console.log("ctor_shape=" + DOMException.name + " len=" + DOMException.length);
try {
  (DOMException as any)("x");
  console.log("call_without_new=accepted");
} catch (e: any) {
  console.log("call_without_new=" + e.constructor.name);
}

// The 25 names that map to a non-zero code, in table order.
const named: string[] = [
  "IndexSizeError",
  "HierarchyRequestError",
  "WrongDocumentError",
  "InvalidCharacterError",
  "NoModificationAllowedError",
  "NotFoundError",
  "NotSupportedError",
  "InUseAttributeError",
  "InvalidStateError",
  "SyntaxError",
  "InvalidModificationError",
  "NamespaceError",
  "InvalidAccessError",
  "TypeMismatchError",
  "SecurityError",
  "NetworkError",
  "AbortError",
  "URLMismatchError",
  "QuotaExceededError",
  "TimeoutError",
  "InvalidNodeTypeError",
  "DataCloneError",
];
for (const n of named) {
  console.log("code[" + n + "]=" + new DOMException("x", n).code);
}

// Names introduced after the table was frozen carry no code at all.
const codeless: string[] = ["EncodingError", "NotReadableError", "UnknownError", "ConstraintError", "DataError", "TransactionInactiveError", "ReadOnlyError", "VersionError", "OperationError", "NotAllowedError"];
console.log("codeless=" + codeless.map(function (n) {
  return new DOMException("x", n).code;
}).join(","));

// The constants, on the constructor and mirrored on the prototype.
const consts: string[] = ["INDEX_SIZE_ERR", "DOMSTRING_SIZE_ERR", "HIERARCHY_REQUEST_ERR", "WRONG_DOCUMENT_ERR", "INVALID_CHARACTER_ERR", "NO_DATA_ALLOWED_ERR", "NO_MODIFICATION_ALLOWED_ERR", "NOT_FOUND_ERR", "NOT_SUPPORTED_ERR", "INUSE_ATTRIBUTE_ERR", "INVALID_STATE_ERR", "SYNTAX_ERR", "INVALID_MODIFICATION_ERR", "NAMESPACE_ERR", "INVALID_ACCESS_ERR", "VALIDATION_ERR", "TYPE_MISMATCH_ERR", "SECURITY_ERR", "NETWORK_ERR", "ABORT_ERR", "URL_MISMATCH_ERR", "QUOTA_EXCEEDED_ERR", "TIMEOUT_ERR", "INVALID_NODE_TYPE_ERR", "DATA_CLONE_ERR"];
for (const c of consts) {
  console.log("const[" + c + "]=" + (DOMException as any)[c] + " onProto=" + (DOMException.prototype as any)[c]);
}
const constDesc = Object.getOwnPropertyDescriptor(DOMException, "ABORT_ERR") as any;
console.log("const_descriptor=w:" + constDesc.writable + " e:" + constDesc.enumerable + " c:" + constDesc.configurable);
console.log("two_gaps=" + [(DOMException as any).DOMSTRING_SIZE_ERR, (DOMException as any).NO_DATA_ALLOWED_ERR, (DOMException as any).VALIDATION_ERR].join(","));
console.log("constants_match_table=" + (new DOMException("x", "AbortError").code === (DOMException as any).ABORT_ERR));

// A DOMException survives structuredClone with its name, message and code.
const cloned: any = structuredClone(d);
console.log("cloned=" + cloned.constructor.name + "/" + cloned.name + "/" + cloned.message + "/" + cloned.code);
console.log("cloned_is_copy=" + (cloned !== d) + " still_error=" + (cloned instanceof Error));

// The runtime raises real DOMExceptions from the platform surface.
const raised: Array<[string, () => void]> = [
  ["btoa_over_latin1", function () { btoa("Ā"); }],
  ["atob_bad_alphabet", function () { atob("@"); }],
  ["clone_function", function () { structuredClone(function () { return 1; }); }],
];
for (const r of raised) {
  try {
    r[1]();
    console.log("raised_" + r[0] + "=no-throw");
  } catch (e: any) {
    console.log("raised_" + r[0] + "=" + e.constructor.name + "/" + e.name + "/" + e.code + " isDOMException=" + (e instanceof DOMException));
  }
}
