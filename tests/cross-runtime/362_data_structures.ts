// Cross-runtime: common data structures implemented in JS.

// Stack
class Stack<T> {
  private data: T[] = [];
  push(v: T) { this.data.push(v); }
  pop(): T | undefined { return this.data.pop(); }
  peek(): T | undefined { return this.data[this.data.length - 1]; }
  get size() { return this.data.length; }
  isEmpty() { return this.data.length === 0; }
}
const s = new Stack<number>();
s.push(1); s.push(2); s.push(3);
console.log(s.peek());  // 3
console.log(s.pop());   // 3
console.log(s.size);    // 2

// Queue (via two-stack O(1) amortized)
class Queue<T> {
  private inbox: T[] = [];
  private outbox: T[] = [];
  enqueue(v: T) { this.inbox.push(v); }
  dequeue(): T | undefined {
    if (!this.outbox.length) while (this.inbox.length) this.outbox.push(this.inbox.pop()!);
    return this.outbox.pop();
  }
  get size() { return this.inbox.length + this.outbox.length; }
}
const q = new Queue<string>();
q.enqueue("a"); q.enqueue("b"); q.enqueue("c");
console.log(q.dequeue());  // a
q.enqueue("d");
console.log(q.dequeue());  // b
console.log(q.size);       // 2

// Linked list
class LLNode<T> { constructor(public val: T, public next: LLNode<T> | null = null) {} }
class LinkedList<T> {
  head: LLNode<T> | null = null;
  push(v: T) {
    const node = new LLNode(v);
    if (!this.head) { this.head = node; return; }
    let cur = this.head;
    while (cur.next) cur = cur.next;
    cur.next = node;
  }
  toArray(): T[] {
    const out: T[] = [];
    let cur = this.head;
    while (cur) { out.push(cur.val); cur = cur.next; }
    return out;
  }
  reverse() {
    let prev: LLNode<T> | null = null, cur = this.head;
    while (cur) { const next = cur.next; cur.next = prev; prev = cur; cur = next; }
    this.head = prev;
  }
}
const ll = new LinkedList<number>();
[1,2,3,4,5].forEach(v => ll.push(v));
console.log(ll.toArray().join(","));
ll.reverse();
console.log(ll.toArray().join(","));

// MinHeap
class MinHeap {
  private h: number[] = [];
  push(v: number) {
    this.h.push(v);
    let i = this.h.length - 1;
    while (i > 0) {
      const p = (i - 1) >> 1;
      if (this.h[p] <= this.h[i]) break;
      [this.h[p], this.h[i]] = [this.h[i], this.h[p]];
      i = p;
    }
  }
  pop(): number | undefined {
    if (!this.h.length) return undefined;
    const top = this.h[0];
    const last = this.h.pop()!;
    if (this.h.length) {
      this.h[0] = last;
      let i = 0;
      while (true) {
        let s = i, l = 2*i+1, r = 2*i+2;
        if (l < this.h.length && this.h[l] < this.h[s]) s = l;
        if (r < this.h.length && this.h[r] < this.h[s]) s = r;
        if (s === i) break;
        [this.h[s], this.h[i]] = [this.h[i], this.h[s]];
        i = s;
      }
    }
    return top;
  }
}
const heap = new MinHeap();
[5,3,8,1,9,2,7].forEach(v => heap.push(v));
const sorted: number[] = [];
for (let i = 0; i < 7; i++) sorted.push(heap.pop()!);
console.log(sorted.join(","));  // 1,2,3,5,7,8,9
