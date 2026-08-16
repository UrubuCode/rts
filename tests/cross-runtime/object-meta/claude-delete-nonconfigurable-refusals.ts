// Pins WHEN a deletion is refused: a non-configurable property, a sealed
// object, an array's length, a function's prototype and a boxed string's index
// all say no. The refusal is read through Reflect.deleteProperty, which answers
// a boolean in every mode — the `delete` operator turns the same refusal into a
// throw only in strict code, which is the host's choice, not the language's.
// claude-delete-return-value only deletes deletable things.

const o: any = { loose: 1 };
Object.defineProperty(o, "fixed", { value: 2, configurable: false, enumerable: true, writable: true });
Object.defineProperty(o, "fixedAcc", { get() { return 3; }, configurable: false, enumerable: true });

console.log("loose=" + Reflect.deleteProperty(o, "loose") + ",left=" + Object.keys(o).join("|"));
console.log("fixed=" + Reflect.deleteProperty(o, "fixed") + ",read=" + o.fixed);
console.log("fixedAcc=" + Reflect.deleteProperty(o, "fixedAcc") + ",read=" + o.fixedAcc);
console.log("missing=" + Reflect.deleteProperty(o, "nothere"));
console.log("inherited=" + Reflect.deleteProperty(o, "toString") + ",still=" + (typeof o.toString));

// the operator agrees on every case that SUCCEEDS, in any mode
const ok: any = { a: 1, b: 2 };
console.log("op_success=" + (delete ok.a) + ",keys=" + Object.keys(ok).join("|"));
console.log("op_missing=" + (delete ok.zzz));
console.log("op_inherited=" + (delete ok.toString) + ",still=" + typeof ok.toString);
console.log("op_computed=" + (delete ok["b"]) + ",empty=" + (Object.keys(ok).length === 0));

// sealed, frozen and merely non-extensible objects
const sealed: any = Object.seal({ a: 1, b: 2 });
console.log("sealed=" + Reflect.deleteProperty(sealed, "a") + ",keys=" + Object.keys(sealed).join("|"));
const frozen: any = Object.freeze({ a: 1 });
console.log("frozen=" + Reflect.deleteProperty(frozen, "a"));
const nonExt: any = Object.preventExtensions({ a: 1 });
console.log("nonext=" + Reflect.deleteProperty(nonExt, "a") + ",keys=" + Object.keys(nonExt).join("|"));
console.log("nonext_readd=" + Reflect.set(nonExt, "a", 1) + ",has=" + Object.hasOwn(nonExt, "a"));

// array indices are configurable, length is not
const arr: any = [1, 2, 3];
console.log("arr_index=" + Reflect.deleteProperty(arr, "1") + ",len=" + arr.length + ",json=" + JSON.stringify(arr));
console.log("arr_hole=" + Object.hasOwn(arr, "1") + ",read=" + arr[1]);
console.log("arr_length=" + Reflect.deleteProperty(arr, "length") + ",len=" + arr.length);
console.log("arr_oob=" + Reflect.deleteProperty(arr, "99"));
const frozenArr: any = Object.freeze([1, 2]);
console.log("frozen_arr=" + Reflect.deleteProperty(frozenArr, "0") + ",v=" + frozenArr[0]);

// a boxed string's index properties are non-configurable
const boxed: any = Object(" ab");
console.log("str_index=" + Reflect.deleteProperty(boxed, "1") + ",read=" + boxed[1]);
console.log("str_length=" + Reflect.deleteProperty(boxed, "length"));
console.log("str_added=" + Reflect.set(boxed, "extra", 1) + ",del=" + Reflect.deleteProperty(boxed, "extra"));

// function meta: name/length are configurable, prototype is not
function fn(_a: number): void { /* noop */ }
console.log("fn_name=" + Reflect.deleteProperty(fn, "name") + ",name=" + JSON.stringify(fn.name));
console.log("fn_length=" + Reflect.deleteProperty(fn, "length") + ",length=" + fn.length);
console.log("fn_prototype=" + Reflect.deleteProperty(fn, "prototype") + ",proto=" + typeof fn.prototype);

// delete of a non-reference operand evaluates it and answers true in any mode
console.log("expr=" + (delete (1 + 1)));
console.log("literal=" + (delete ("abc")));
let sideEffects = 0;
function bump(): any { sideEffects++; return { a: 1 }; }
console.log("call=" + (delete bump().a) + ",calls=" + sideEffects);

// deleting through a proxy consults the trap and the target's real answer
const seen: string[] = [];
const t: any = { p: 1 };
Object.defineProperty(t, "nc", { value: 2, configurable: false });
const px: any = new Proxy(t, {
  deleteProperty(target, key) { seen.push(String(key)); return Reflect.deleteProperty(target, key); },
});
console.log("proxy_p=" + Reflect.deleteProperty(px, "p"));
console.log("proxy_nc=" + Reflect.deleteProperty(px, "nc"));
console.log("proxy_seen=" + seen.join("|"));

// a trap that answers true for a non-configurable key is caught by the invariant
const liar: any = new Proxy(t, { deleteProperty() { return true; } });
try {
  console.log("proxy_liar=" + Reflect.deleteProperty(liar, "nc"));
} catch (e: any) {
  console.log("proxy_liar=throw:" + e.constructor.name);
}
console.log("nc_still=" + Object.hasOwn(t, "nc"));

// delete never touches the prototype chain
const proto: any = { shared: "P" };
const child: any = Object.create(proto);
child.shared = "C";
console.log("shadow_before=" + child.shared);
console.log("delete_shadow=" + Reflect.deleteProperty(child, "shared") + ",after=" + child.shared);
console.log("delete_again=" + Reflect.deleteProperty(child, "shared") + ",after=" + child.shared);
console.log("proto_intact=" + proto.shared);
