// Class constructs beyond the base lowering: private members, a computed
// member name, and a static block.
let failed = "";
function check(name, held) { if (!held) { failed = failed + name + ","; } }

// A private field is read and written from inside the class, through the
// ordinary property path under a key no string literal a program writes can
// spell — see `crates/rts-codegen/src/emit/class.rs`'s module doc for why that
// is privacy here rather than an access check.
class Box {
    #value;
    constructor(v) { this.#value = v; }
    get() { return this.#value; }
    set(v) { this.#value = v; }
    #double() { return this.#value * 2; }
    doubled() { return this.#double(); }
}

let b = new Box(21);
check("private-field-read", b.get() === 21);
b.set(30);
check("private-field-write", b.get() === 30);
check("private-method", b.doubled() === 60);
// The reserved key space a private member's key lives in is the same one a
// well-known symbol's key does, and `Object.keys` already filters it out.
check("private-not-enumerable", Object.keys(b).length === 0);

// A computed member name is evaluated once, at class-definition time, against
// the scope the class was written in.
let name = "computedName";
class Computed {
    [name]() { return "hit"; }
}
check("computed-member-name", new Computed().computedName() === "hit");

// `static { … }` runs once, at class-definition time, with `this` bound to
// the constructor.
let ran = false;
let sawConstructor = false;
class WithStatic {
    static {
        ran = true;
        // `this` inside a static block is the constructor, not an instance —
        // so it has a `prototype`, which nothing else `this` could be here has.
        sawConstructor = typeof this.prototype === "object";
    }
}
check("static-block-ran", ran === true);
check("static-block-this-is-the-class", sawConstructor === true);

return failed;

// The correction the reserved space exists for: a private key lives under
// `@@#`, so a property a program genuinely wrote whose name begins with `#` is
// an ordinary one. Filtering on `#` alone made this disappear.
let selectors = {};
selectors["#main"] = 1;
check("a-hash-property-is-ordinary", Object.keys(selectors).length === 1);
check("a-hash-property-reads-back", selectors["#main"] === 1);
check("a-hash-property-is-in", "#main" in selectors);
