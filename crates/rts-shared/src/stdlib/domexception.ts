// DOMException — the web platform's exception class (NOT a primordial; no
// native syntax). Pure TS over primordials: name/message/code with the spec's
// legacy code table. Registered as an engine include (ambient class).

class DOMException {
  readonly message: string;
  readonly name: string;

  constructor(message: string = "", name: string = "Error") {
    this.message = message;
    this.name = name;
  }

  // The LEGACY numeric code for the spec-listed names; 0 for everything else
  // (AbortError included — it has no legacy code).
  get code(): number {
    const n = this.name;
    if (n === "IndexSizeError") return 1;
    if (n === "HierarchyRequestError") return 3;
    if (n === "WrongDocumentError") return 4;
    if (n === "InvalidCharacterError") return 5;
    if (n === "NoModificationAllowedError") return 7;
    if (n === "NotFoundError") return 8;
    if (n === "NotSupportedError") return 9;
    if (n === "InUseAttributeError") return 10;
    if (n === "InvalidStateError") return 11;
    if (n === "SyntaxError") return 12;
    if (n === "InvalidModificationError") return 13;
    if (n === "NamespaceError") return 14;
    if (n === "InvalidAccessError") return 15;
    if (n === "SecurityError") return 18;
    if (n === "NetworkError") return 19;
    if (n === "AbortError") return 20;
    if (n === "URLMismatchError") return 21;
    if (n === "QuotaExceededError") return 22;
    if (n === "TimeoutError") return 23;
    if (n === "InvalidNodeTypeError") return 24;
    if (n === "DataCloneError") return 25;
    return 0;
  }

  toString(): string {
    return this.name + ": " + this.message;
  }
}
