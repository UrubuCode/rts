// Pins defineProperty on an EXISTING property: a partial descriptor keeps the
// unmentioned attributes, but crossing between data and accessor form resets
// the fields of the abandoned form to their defaults. 154_object_defineproperty
// only defines fresh properties.

const o: any = {};
Object.defineProperty(o, "p", { value: 1, writable: true, enumerable: true, configurable: true });

function shape(target: any, key: string): string {
  const d = Object.getOwnPropertyDescriptor(target, key) as any;
  if (d === undefined) return "undefined";
  const kind = "get" in d ? "accessor" : "data";
  const body = kind === "data"
    ? "value=" + String(d.value) + ",w=" + d.writable
    : "get=" + (typeof d.get) + ",set=" + (typeof d.set);
  return kind + "{" + body + ",e=" + d.enumerable + ",c=" + d.configurable + "}";
}

// a write is reported through Reflect.set, which answers a boolean in both
// strict and sloppy code: whether a refused assignment THROWS depends on the
// caller's mode, and this file must not measure the host's choice of mode.
function write(target: any, key: string, value: unknown): string {
  return String(Reflect.set(target, key, value));
}

console.log("start=" + shape(o, "p"));

// partial redefine: only enumerable changes, writable/configurable survive
Object.defineProperty(o, "p", { enumerable: false });
console.log("part=" + shape(o, "p"));

// data -> accessor: writable is GONE, enumerable/configurable are preserved
let backing = "acc";
Object.defineProperty(o, "p", { get() { return backing; } });
console.log("toacc=" + shape(o, "p"));
console.log("read=" + o.p);
console.log("write_nosetter=" + write(o, "p", "ignored-no-setter"));
console.log("after_write=" + o.p);

// adding only a setter keeps the previously installed getter
Object.defineProperty(o, "p", { set(v: string) { backing = "<" + v + ">"; } });
console.log("withset=" + shape(o, "p"));
console.log("write_setter=" + write(o, "p", "hello"));
console.log("read2=" + o.p);

// accessor -> data: get/set are GONE, writable defaults to FALSE
Object.defineProperty(o, "p", { value: 42 });
console.log("todata=" + shape(o, "p"));
console.log("write_readonly=" + write(o, "p", 99));
console.log("after_data_write=" + o.p);

// and back once more, then to data with an explicit writable
Object.defineProperty(o, "p", { get() { return "g2"; }, configurable: true });
console.log("acc2=" + shape(o, "p"));
Object.defineProperty(o, "p", { value: 7, writable: true });
console.log("data2=" + shape(o, "p"));

// a fresh property defaults every omitted attribute to false/undefined
const fresh: any = {};
Object.defineProperty(fresh, "q", { value: 1 });
console.log("fresh=" + shape(fresh, "q"));
console.log("fresh_keys=" + Object.keys(fresh).length);

// an accessor defined with no get answers undefined and drops writes silently
const noget: any = {};
let sink = "none";
Object.defineProperty(noget, "r", { set(v: string) { sink = v; }, configurable: true });
console.log("noget_read=" + noget.r);
console.log("write_setonly=" + write(noget, "r", "written"));
console.log("noget_sink=" + sink);
console.log("noget_shape=" + shape(noget, "r"));

// an assignment can never convert a data property into an accessor
const plain: any = { d: 1 };
console.log("write_plain=" + write(plain, "d", 2));
console.log("plain=" + shape(plain, "d"));

// defineProperty with an empty descriptor on an existing property is a no-op
Object.defineProperty(plain, "d", {});
console.log("empty=" + shape(plain, "d"));

// but on a MISSING property an empty descriptor creates the all-false data slot
Object.defineProperty(plain, "e", {});
console.log("empty_new=" + shape(plain, "e"));
console.log("empty_new_in=" + ("e" in plain));
