// Cross-runtime: a persistent (immutable) cons list and a persistent map built
// on it. Every update returns a new value, the old one is untouched, and the
// tail is SHARED rather than copied — asserted by object identity.

interface Cell { head: number; tail: Cell | null; }

const NIL: Cell | null = null;

function cons(head: number, tail: Cell | null): Cell {
  return Object.freeze({ head: head, tail: tail }) as Cell;
}
function fromArray(xs: number[]): Cell | null {
  let out: Cell | null = NIL;
  for (let i = xs.length - 1; i >= 0; i--) out = cons(xs[i], out);
  return out;
}
function toArray(list: Cell | null): number[] {
  const out: number[] = [];
  let cur = list;
  while (cur !== null) { out.push(cur.head); cur = cur.tail; }
  return out;
}
function len(list: Cell | null): number {
  let n = 0;
  let cur = list;
  while (cur !== null) { n += 1; cur = cur.tail; }
  return n;
}
function nth(list: Cell | null, i: number): Cell | null {
  let cur = list;
  while (cur !== null && i > 0) { cur = cur.tail; i -= 1; }
  return cur;
}

const base = fromArray([1, 2, 3, 4, 5]) as Cell;
console.log("base=" + toArray(base).join(","));
console.log("base_len=" + len(base));

// Prepending shares the WHOLE original list.
const prepended = cons(0, base);
console.log("prepended=" + toArray(prepended).join(","));
console.log("base_unchanged=" + toArray(base).join(","));
console.log("tail_is_shared=" + (prepended.tail === base));
console.log("shared_cell_count=" + len(base));

// Replacing element 2 copies only the cells before it.
function setAt(list: Cell | null, index: number, value: number): Cell | null {
  if (list === null) return null;
  if (index === 0) return cons(value, list.tail);
  return cons(list.head, setAt(list.tail, index - 1, value));
}
const replaced = setAt(base, 2, 99) as Cell;
console.log("replaced=" + toArray(replaced).join(","));
console.log("base_still=" + toArray(base).join(","));
console.log("copied_0=" + (nth(replaced, 0) !== nth(base, 0)));
console.log("copied_1=" + (nth(replaced, 1) !== nth(base, 1)));
console.log("copied_2=" + (nth(replaced, 2) !== nth(base, 2)));
console.log("shared_3=" + (nth(replaced, 3) === nth(base, 3)));
console.log("shared_4=" + (nth(replaced, 4) === nth(base, 4)));

// Count how many cells are shared between two versions.
function sharedSuffix(a: Cell | null, b: Cell | null): number {
  const seen = new Set<Cell>();
  let cur = a;
  while (cur !== null) { seen.add(cur); cur = cur.tail; }
  let count = 0;
  cur = b;
  while (cur !== null) { if (seen.has(cur)) count += 1; cur = cur.tail; }
  return count;
}
console.log("shared_after_set2=" + sharedSuffix(base, replaced));
console.log("shared_after_set0=" + sharedSuffix(base, setAt(base, 0, 42)));
console.log("shared_after_set4=" + sharedSuffix(base, setAt(base, 4, 42)));

// Frozen cells refuse mutation, so the sharing is safe. `Reflect.set` reports
// the refusal as a boolean in both strict and sloppy code.
console.log("is_frozen=" + Object.isFrozen(base));
const beforeHead = base.head;
console.log("set_head_refused=" + Reflect.set(base, "head", 1000));
console.log("set_tail_refused=" + Reflect.set(base, "tail", null));
console.log("delete_head_refused=" + Reflect.deleteProperty(base, "head"));
console.log("head_unchanged=" + (base.head === beforeHead));
console.log("tail_unchanged=" + (base.tail === nth(base, 1)));

// A version history: each edit keeps every earlier version valid.
const history: Array<Cell | null> = [base];
for (let i = 0; i < 5; i++) history.push(setAt(history[history.length - 1], i, (i + 1) * 10));
for (let i = 0; i < history.length; i++) {
  console.log("v" + i + "=" + toArray(history[i]).join(",") + " shared_with_v0=" + sharedSuffix(base, history[i]));
}

// A persistent map: an association list where an update shadows the old entry.
interface Entry { key: string; value: number; next: Entry | null; }
function put(map: Entry | null, key: string, value: number): Entry {
  return Object.freeze({ key: key, value: value, next: map }) as Entry;
}
function get(map: Entry | null, key: string): number | undefined {
  let cur = map;
  while (cur !== null) { if (cur.key === key) return cur.value; cur = cur.next; }
  return undefined;
}
function keys(map: Entry | null): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  let cur = map;
  while (cur !== null) { if (!seen.has(cur.key)) { seen.add(cur.key); out.push(cur.key); } cur = cur.next; }
  return out.sort();
}

const m0 = put(put(put(null, "a", 1), "b", 2), "c", 3);
const m1 = put(m0, "b", 20);
const m2 = put(m1, "d", 4);
console.log("m0=" + keys(m0).map((k) => k + "=" + get(m0, k)).join(","));
console.log("m1=" + keys(m1).map((k) => k + "=" + get(m1, k)).join(","));
console.log("m2=" + keys(m2).map((k) => k + "=" + get(m2, k)).join(","));
console.log("m0_b_still=" + get(m0, "b"));
console.log("m1_shares_m0=" + (m1.next === m0));
console.log("m2_shares_m1=" + (m2.next === m1));
console.log("missing=" + String(get(m2, "zz")));

// Structural equality by walking, not by identity.
function equal(a: Cell | null, b: Cell | null): boolean {
  while (a !== null && b !== null) {
    if (a === b) return true;
    if (a.head !== b.head) return false;
    a = a.tail;
    b = b.tail;
  }
  return a === b;
}
console.log("equal_same=" + equal(base, base));
console.log("equal_copy=" + equal(base, fromArray([1, 2, 3, 4, 5])));
console.log("equal_diff=" + equal(base, fromArray([1, 2, 3, 4, 6])));
console.log("equal_shorter=" + equal(base, fromArray([1, 2, 3])));

// Reversing builds all-new cells, so nothing is shared.
function reverse(list: Cell | null): Cell | null {
  let out: Cell | null = NIL;
  let cur = list;
  while (cur !== null) { out = cons(cur.head, out); cur = cur.tail; }
  return out;
}
const reversed = reverse(base);
console.log("reversed=" + toArray(reversed).join(","));
console.log("reversed_shares=" + sharedSuffix(base, reversed));

// Appending shares only the SECOND list.
function append(a: Cell | null, b: Cell | null): Cell | null {
  if (a === null) return b;
  return cons(a.head, append(a.tail, b));
}
const second = fromArray([7, 8]) as Cell;
const joined = append(base, second) as Cell;
console.log("joined=" + toArray(joined).join(","));
console.log("joined_shares_second=" + (nth(joined, 5) === second));
console.log("joined_shares_first=" + sharedSuffix(base, joined));

// A deep chain still holds every version, bounded well below any stack limit.
let deep: Cell | null = NIL;
const versions: Array<Cell | null> = [];
for (let i = 0; i < 200; i++) { deep = cons(i, deep); versions.push(deep); }
console.log("deep_len=" + len(deep));
console.log("v10_len=" + len(versions[10]));
console.log("v199_head=" + (versions[199] as Cell).head);
console.log("v0_still_len1=" + len(versions[0]));
console.log("all_versions_share=" + (versions[100] === nth(deep, 99)));
