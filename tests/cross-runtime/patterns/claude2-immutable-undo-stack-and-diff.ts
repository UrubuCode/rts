// ONE thing: a whole program — an undo/redo stack over immutable snapshots,
// with a structural diff between versions. It leans on Object.freeze, spread,
// Map ordering and closures at the same time, which is where an engine answers
// wrongly rather than not at all.
type State = { items: string[]; sel: number; flags: { [k: string]: boolean } };

function freezeDeep<T>(v: T): T {
  if (v && typeof v === "object") {
    Object.getOwnPropertyNames(v).forEach((k) => freezeDeep((v as any)[k]));
    Object.freeze(v);
  }
  return v;
}

function makeStore(initial: State) {
  const past: State[] = [];
  const future: State[] = [];
  let present = freezeDeep(initial);
  return {
    get: () => present,
    apply(label: string, f: (s: State) => State) {
      past.push(present);
      future.length = 0;
      present = freezeDeep(f(present));
      return label + " -> " + JSON.stringify(present);
    },
    undo() {
      if (past.length === 0) return "undo:empty";
      future.push(present);
      present = past.pop() as State;
      return "undo -> " + JSON.stringify(present);
    },
    redo() {
      if (future.length === 0) return "redo:empty";
      past.push(present);
      present = future.pop() as State;
      return "redo -> " + JSON.stringify(present);
    },
    depth: () => past.length + "/" + future.length,
  };
}

const store = makeStore({ items: ["a"], sel: 0, flags: { dirty: false } });
console.log(store.apply("add-b", (s) => ({ ...s, items: [...s.items, "b"] })));
console.log(store.apply("select-1", (s) => ({ ...s, sel: 1 })));
console.log(store.apply("mark", (s) => ({ ...s, flags: { ...s.flags, dirty: true } })));
console.log("depth=" + store.depth());
console.log(store.undo());
console.log(store.undo());
console.log("depth=" + store.depth());
console.log(store.redo());
console.log("depth=" + store.depth());
console.log(store.apply("add-c", (s) => ({ ...s, items: [...s.items, "c"] })));
console.log("futureCleared=" + store.depth());
console.log(store.redo());

// The snapshots really are frozen, probed mode-independently.
const cur: any = store.get();
console.log("frozen=" + Object.isFrozen(cur) + " nestedFrozen=" + Object.isFrozen(cur.flags) + " arrFrozen=" + Object.isFrozen(cur.items));
console.log("writeRefused=" + Reflect.set(cur, "sel", 99) + " still=" + cur.sel);
console.log("pushRefused=" + (() => { try { cur.items.push("z"); return "no-throw"; } catch (e: any) { return e.constructor.name; } })());
console.log("deleteRefused=" + Reflect.deleteProperty(cur, "sel") + " still=" + cur.sel);

// --- structural diff between two snapshots ---
function diff(a: any, b: any, path: string, out: string[]): string[] {
  if (a === b) return out;
  const aObj = a && typeof a === "object";
  const bObj = b && typeof b === "object";
  if (!aObj || !bObj) { out.push(path + ": " + JSON.stringify(a) + " -> " + JSON.stringify(b)); return out; }
  const keys = new Set<string>();
  Object.keys(a).forEach((k) => keys.add(k));
  Object.keys(b).forEach((k) => keys.add(k));
  for (const k of Array.from(keys).sort()) {
    const inA = Object.prototype.hasOwnProperty.call(a, k);
    const inB = Object.prototype.hasOwnProperty.call(b, k);
    if (!inA) out.push(path + "/" + k + ": <added> " + JSON.stringify(b[k]));
    else if (!inB) out.push(path + "/" + k + ": <removed> " + JSON.stringify(a[k]));
    else diff(a[k], b[k], path + "/" + k, out);
  }
  return out;
}

const v1 = { items: ["a", "b"], sel: 0, flags: { dirty: false, pinned: true } };
const v2 = { items: ["a", "B", "c"], sel: 0, flags: { dirty: true } };
console.log("diff:\n  " + diff(v1, v2, "", []).join("\n  "));
console.log("diffSelf=" + JSON.stringify(diff(v1, v1, "", [])));
console.log("diffPrim=" + diff(1, "1", "", []).join("|"));
console.log("diffNull=" + diff({ a: null }, { a: {} }, "", []).join("|"));
