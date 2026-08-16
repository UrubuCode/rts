// Cross-runtime: a binary min-heap driving Dijkstra over a fixed graph, with a
// trace of every sift, pop and relaxation. Ties are broken by node name so the
// order is total and the output cannot depend on a sort's stability.

class MinHeap {
  private items: Array<{ key: number; name: string }> = [];
  readonly trace: string[] = [];

  get size(): number { return this.items.length; }

  private less(a: { key: number; name: string }, b: { key: number; name: string }): boolean {
    return a.key !== b.key ? a.key < b.key : a.name < b.name;
  }

  push(key: number, name: string): void {
    this.items.push({ key: key, name: name });
    let i = this.items.length - 1;
    let swaps = 0;
    while (i > 0) {
      const parent = (i - 1) >> 1;
      if (!this.less(this.items[i], this.items[parent])) break;
      const t = this.items[i];
      this.items[i] = this.items[parent];
      this.items[parent] = t;
      i = parent;
      swaps += 1;
    }
    this.trace.push("push " + name + "@" + key + " up=" + swaps + " " + this.dump());
  }

  pop(): { key: number; name: string } | null {
    if (this.items.length === 0) return null;
    const top = this.items[0];
    const last = this.items.pop() as { key: number; name: string };
    let swaps = 0;
    if (this.items.length > 0) {
      this.items[0] = last;
      let i = 0;
      while (true) {
        const l = 2 * i + 1;
        const r = l + 1;
        let smallest = i;
        if (l < this.items.length && this.less(this.items[l], this.items[smallest])) smallest = l;
        if (r < this.items.length && this.less(this.items[r], this.items[smallest])) smallest = r;
        if (smallest === i) break;
        const t = this.items[i];
        this.items[i] = this.items[smallest];
        this.items[smallest] = t;
        i = smallest;
        swaps += 1;
      }
    }
    this.trace.push("pop  " + top.name + "@" + top.key + " down=" + swaps + " " + this.dump());
    return top;
  }

  dump(): string {
    return "[" + this.items.map((it) => it.name + ":" + it.key).join(" ") + "]";
  }

  // A heap is valid when no child is less than its parent.
  isValid(): boolean {
    for (let i = 1; i < this.items.length; i++) {
      if (this.less(this.items[i], this.items[(i - 1) >> 1])) return false;
    }
    return true;
  }
}

// Heap sanity: push a shuffled sequence and pop it back sorted.
const h = new MinHeap();
const seq = [5, 3, 8, 1, 9, 2, 7, 4, 6, 0];
for (let i = 0; i < seq.length; i++) h.push(seq[i], "n" + seq[i]);
console.log("--- heap build");
for (const line of h.trace) console.log(line);
console.log("valid_after_build=" + h.isValid());

const popped: number[] = [];
while (h.size > 0) popped.push((h.pop() as any).key);
console.log("sorted=" + popped.join(","));
console.log("--- heap drain");
for (const line of h.trace.slice(seq.length)) console.log(line);

// Ties break by name, so equal keys come out in a fixed order.
const tied = new MinHeap();
for (const name of ["d", "a", "c", "b"]) tied.push(1, name);
const tieOrder: string[] = [];
while (tied.size > 0) tieOrder.push((tied.pop() as any).name);
console.log("tie_order=" + tieOrder.join(","));

// The graph: an undirected weighted graph with a cheaper indirect route.
const edges: Array<[string, string, number]> = [
  ["A", "B", 4], ["A", "C", 2],
  ["B", "C", 5], ["B", "D", 10],
  ["C", "E", 3],
  ["D", "F", 11], ["E", "D", 4],
  ["B", "E", 1], ["F", "G", 2], ["D", "G", 5],
  ["H", "G", 1],
];
const adj: Record<string, Array<[string, number]>> = {};
for (const [u, v, w] of edges) {
  if (adj[u] === undefined) adj[u] = [];
  if (adj[v] === undefined) adj[v] = [];
  adj[u].push([v, w]);
  adj[v].push([u, w]);
}
for (const k of Object.keys(adj)) adj[k].sort((x, y) => (x[0] < y[0] ? -1 : x[0] > y[0] ? 1 : 0));
console.log("nodes=" + Object.keys(adj).sort().join(","));
console.log("degrees=" + Object.keys(adj).sort().map((k) => k + ":" + adj[k].length).join(","));

function dijkstra(start: string): string[] {
  const dist: Record<string, number> = {};
  const prev: Record<string, string> = {};
  const done: Record<string, boolean> = {};
  const log: string[] = [];
  for (const k of Object.keys(adj)) dist[k] = Infinity;
  dist[start] = 0;

  const pq = new MinHeap();
  pq.push(0, start);

  while (pq.size > 0) {
    const top = pq.pop() as { key: number; name: string };
    if (done[top.name]) { log.push("skip " + top.name + "@" + top.key); continue; }
    done[top.name] = true;
    log.push("settle " + top.name + "=" + top.key);
    for (const [next, w] of adj[top.name]) {
      if (done[next]) continue;
      const candidate = dist[top.name] + w;
      if (candidate < dist[next]) {
        log.push("  relax " + top.name + "->" + next + " " + (dist[next] === Infinity ? "inf" : String(dist[next])) + "=>" + candidate);
        dist[next] = candidate;
        prev[next] = top.name;
        pq.push(candidate, next);
      } else {
        log.push("  keep  " + top.name + "->" + next + " " + candidate + ">=" + dist[next]);
      }
    }
  }

  log.push("dist " + Object.keys(dist).sort().map((k) => k + "=" + (dist[k] === Infinity ? "inf" : String(dist[k]))).join(","));
  const path = (to: string): string => {
    const chain: string[] = [];
    let cur: string | undefined = to;
    while (cur !== undefined) { chain.push(cur); cur = prev[cur]; }
    return chain.reverse().join(">");
  };
  for (const k of Object.keys(dist).sort()) log.push("path " + k + " = " + path(k));
  return log;
}

console.log("--- dijkstra from A");
for (const line of dijkstra("A")) console.log(line);

console.log("--- dijkstra from G");
for (const line of dijkstra("G")) console.log(line);

// An isolated node is unreachable and keeps distance Infinity.
adj["Z"] = [];
console.log("--- with isolated node");
const withZ = dijkstra("A");
console.log(withZ[withZ.length - Object.keys(adj).length - 1]);
console.log("isolated_path=" + withZ[withZ.length - 1]);
