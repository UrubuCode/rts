// node:util — legacy corner + type predicates + parseEnv.
import { describe, test, expect } from "rts:test";
import { isArray, inherits, _extend, parseEnv, types } from "node:util";

// PROVA (Node real v20.19.5): node -e "const u=require('node:util');
// console.log(typeof u.formatHex, typeof u.formatBin, typeof u.formatOct,
// typeof u.parseInt)" -> undefined undefined undefined undefined
// Nenhuma das quatro alguma vez existiu em node:util — vieram de um
// "node:util fase 1" descartado (commit deec4573, issue #288, fechada com
// label candidate-discard: "gh issue view 288" mostra o discard e
// "git log --oneline -1 deec4573" mostra o commit que a introduziu).
// Esta fixture foi reescrita para exercer a superficie REAL que este motor
// implementa (crates/rts-node/src/util/mod.rs), evitando o que ja e coberto
// por node_util_full/node_util_inspect/node_util_parseargs.

// --- isArray (DEP0044, top-level legacy) ------------------------------------
// node -e "const u=require('node:util'); console.log(u.isArray([1,2,3]), u.isArray({}))"
//   -> true false
const arrTrue = isArray([1, 2, 3]);
const arrFalse = isArray({});

// --- inherits ----------------------------------------------------------------
// node -e "function Base(){} Base.prototype.greet=function(){return 'hi'};
// function Sub(){}; require('node:util').inherits(Sub, Base);
// console.log(new Sub().greet(), Sub.super_ === Base)" -> hi true
function Base() {}
Base.prototype.greet = function () {
    return "hi";
};
function Sub() {}
inherits(Sub, Base);
const inheritedGreet = new (Sub as any)().greet();
const superLinked = (Sub as any).super_ === Base;

// --- _extend (DEP0060) --------------------------------------------------------
// node -e "console.log(JSON.stringify(require('node:util')._extend({a:1},{b:2})))"
//   -> {"a":1,"b":2}
const extended = _extend({ a: 1 }, { b: 2 });
const extendedOk = (extended as any).a === 1 && (extended as any).b === 2;

// --- parseEnv ------------------------------------------------------------------
// node -e "console.log(JSON.stringify(require('node:util').parseEnv('A=1\nB=two\n')))"
//   -> {"A":"1","B":"two"}
const env = parseEnv("A=1\nB=two\n");
const envOk = (env as any).A === "1" && (env as any).B === "two";

// --- util.types (brand predicates) --------------------------------------------
// node -e "const u=require('node:util');
// console.log(u.types.isArrayBufferView(new Uint8Array(1)), u.types.isArrayBufferView([1]),
// u.types.isModuleNamespaceObject({}))" -> true false false
const viewTrue = types.isArrayBufferView(new Uint8Array(1));
const viewFalse = types.isArrayBufferView([1]);
const notANamespace = types.isModuleNamespaceObject({});

describe("fixture:node_util_basic", () => {
    test("isArray", () => {
        expect(arrTrue).toBe(true);
        expect(arrFalse).toBe(false);
    });
    test("inherits links prototype and super_", () => {
        expect(inheritedGreet).toBe("hi");
        expect(superLinked).toBe(true);
    });
    test("_extend assigns from source", () => expect(extendedOk).toBe(true));
    test("parseEnv reads KEY=value pairs", () => expect(envOk).toBe(true));
    test("types.isArrayBufferView", () => {
        expect(viewTrue).toBe(true);
        expect(viewFalse).toBe(false);
    });
    test("types.isModuleNamespaceObject false for a plain object", () =>
        expect(notANamespace).toBe(false));
});
